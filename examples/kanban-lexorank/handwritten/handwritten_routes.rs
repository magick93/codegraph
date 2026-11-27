//! Hand-written board routes: LexoRank move + rebalance.
//!
//! This file is NOT generated. It survives regeneration (the sweep removes
//! only `*_handler.rs` siblings — see `clean_generated_output`) and is
//! auto-declared as a module by the post-generation filesystem scan
//! (`generate_mod_files`). It is mounted by the
//! `templates-override/api/router.tera` override as
//! `Router::new().merge(super::handwritten_routes::routes())`, so its paths
//! resolve under the domain root (`/api/v1/board/…`):
//!
//! - `POST /cards/{card_id}/move` — insert-before/after, cross-list aware;
//!   the server computes the midpoint key. Body:
//!   `{"list_id?": uuid, "before_id?": uuid, "after_id?": uuid}`.
//! - `POST /lists/{list_id}/rebalance` — re-space a whole list's keys back
//!   to short ones (`0|1, 0|2, …`).
//!
//! Rank keys use the `lexorank` crate (2.0): stored format `{bucket}|{rank}`
//! with the bucket fixed at 0 (see docs/lexorank-kanban.md). The column is
//! `COLLATE "C"` (migrations/custom.sql) so Postgres ORDER BY / comparison
//! agrees with the crate's internal string ordering.
//!
//! Context conventions mirror the generated planes: `Extension<ApiKeyInfo>`
//! auth (injected by the shared `/api` middleware), `PermissionGuard`
//! route guards, the RLS session-var bundle + `SET LOCAL ROLE app_user`
//! (mirrors `templates/ddd/command.tera`), and one tx per request. A
//! known limitation: these routes are not in the generated OpenAPI doc.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Extension, Json, Router,
};
use lexorank::LexoRank;
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, FromQueryResult, Statement,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::{permission::PermissionGuard as _, ApiKeyInfo};

/// The first key in any list (assigned by the insert trigger when empty).
const FIRST_KEY: &str = "0|1";
/// A far-right sentinel the rebalance spread walks towards.
const LAST_KEY: &str = "0|zzzzzzzz";

// ---------------------------------------------------------------------------
// Move
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MoveCardRequest {
    /// Target list — omit to keep the card's current list.
    pub list_id: Option<Uuid>,
    /// The card that will sit directly BEFORE the moved card (upper bound).
    pub before_id: Option<Uuid>,
    /// The card that will sit directly AFTER the moved card (lower bound).
    pub after_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct MoveResponse {
    pub id: Uuid,
    pub list_id: Uuid,
    pub rank: String,
}

pub async fn move_card(
    State(state): State<AppState>,
    Extension(ctx): Extension<ApiKeyInfo>,
    Path(card_id): Path<Uuid>,
    Json(body): Json<MoveCardRequest>,
) -> Result<Json<MoveResponse>, ApiError> {
    if body.before_id == Some(card_id) || body.after_id == Some(card_id) {
        return Err(ApiError::bad_request(
            "before_id/after_id must reference a different card",
        ));
    }

    let tx = state
        .db
        .begin()
        .await
        .map_err(internal_db("begin transaction"))?;
    set_rls_session_vars(&tx, &ctx).await?;

    // Lock the moving row first — concurrent moves of the same card serialize.
    let card = card_row(&tx, card_id).await?;
    let target_list = body.list_id.unwrap_or(card.list_id);

    // Direct bounds (cards may live in another list — their keys are still
    // valid bounds for the target position).
    let lower = match body.after_id {
        Some(id) => Some(bound_rank(&tx, id, "after").await?),
        None => None,
    };
    let upper = match body.before_id {
        Some(id) => Some(bound_rank(&tx, id, "before").await?),
        None => None,
    };

    let new_rank = match (lower, upper) {
        (Some(lo), Some(hi)) => midpoint(&lo, &hi).ok_or_else(|| {
            ApiError::conflict(
                "rank_exhausted: no room between the two neighbouring keys — \
                 rebalance the list (POST /api/v1/board/lists/<list_id>/rebalance)",
            )
        })?,
        // Append after `lo`: midpoint to the next surviving key in the list,
        // or lo's immediate successor key.
        (Some(lo), None) => match successor_of(&tx, target_list, &lo, card_id).await? {
            Some(next) => midpoint(&lo, &next)
                .unwrap_or_else(|| next_key(&lo)),
            None => next_key(&lo),
        },
        // Insert before `hi`: midpoint from the previous surviving key, or
        // hi's immediate predecessor key.
        (None, Some(hi)) => match predecessor_of(&tx, target_list, &hi, card_id).await? {
            Some(prev) => midpoint(&prev, &hi)
                .unwrap_or_else(|| prev_key(&hi)),
            None => prev_key(&hi),
        },
        // Drag to a list's end (or into an empty list).
        (None, None) => match max_rank(&tx, target_list, card_id).await? {
            Some(max) => next_key(&max),
            None => FIRST_KEY.to_string(),
        },
    };

    tx.execute(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE board.card SET list_id = $1, rank = $2 WHERE id = $3",
        [target_list.into(), new_rank.clone().into(), card_id.into()],
    ))
    .await
    .map_err(internal_db("update card"))?;

    tx.commit()
        .await
        .map_err(internal_db("commit move"))?;

    Ok(Json(MoveResponse {
        id: card_id,
        list_id: target_list,
        rank: new_rank,
    }))
}

// ---------------------------------------------------------------------------
// Rebalance
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RebalanceResponse {
    pub list_id: Uuid,
    pub cards: usize,
}

pub async fn rebalance_list(
    State(state): State<AppState>,
    Extension(ctx): Extension<ApiKeyInfo>,
    Path(list_id): Path<Uuid>,
) -> Result<Json<RebalanceResponse>, ApiError> {
    let tx = state
        .db
        .begin()
        .await
        .map_err(internal_db("begin transaction"))?;
    set_rls_session_vars(&tx, &ctx).await?;

    // Lock the list's cards in rank order, then re-space evenly by walking
    // between() towards a far-right sentinel — keys stay short and strictly
    // increasing for any practical n.
    let rows = tx
        .query_all(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT id FROM board.card WHERE list_id = $1 AND deleted_at IS NULL \
             ORDER BY rank COLLATE \"C\", id FOR UPDATE",
            [list_id.into()],
        ))
        .await
        .map_err(internal_db("lock list cards"))?;
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| IdRow::from_query_result(row, "").map(|r| r.id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal_db("read card ids"))?;

    let keys = spread_keys(ids.len());
    for (id, key) in ids.iter().zip(keys.iter()) {
        tx.execute(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE board.card SET rank = $1 WHERE id = $2",
            [key.clone().into(), (*id).into()],
        ))
        .await
        .map_err(internal_db("rebalance card"))?;
    }

    tx.commit()
        .await
        .map_err(internal_db("commit rebalance"))?;

    Ok(Json(RebalanceResponse {
        list_id,
        cards: ids.len(),
    }))
}

// ---------------------------------------------------------------------------
// Key math (lexorank crate)
// ---------------------------------------------------------------------------

fn parse_key(key: &str) -> Result<LexoRank, ApiError> {
    LexoRank::from_string(key)
        .map_err(|e| ApiError::internal(format!("corrupt rank key {key:?}: {e}")))
}

/// A key strictly between lo and hi, or None when no room exists
/// (equal keys — the rebalance-and-retry signal).
fn midpoint(lo: &str, hi: &str) -> Option<String> {
    parse_key(lo).ok()?.between(&parse_key(hi).ok()?).map(|r| r.to_string())
}

fn next_key(key: &str) -> String {
    parse_key(key)
        .map(|r| r.next().to_string())
        .unwrap_or_else(|_| FIRST_KEY.to_string())
}

fn prev_key(key: &str) -> String {
    parse_key(key)
        .map(|r| r.prev().to_string())
        .unwrap_or_else(|_| FIRST_KEY.to_string())
}

/// Evenly spaced keys for `n` cards: repeatedly between(cursor, sentinel).
fn spread_keys(n: usize) -> Vec<String> {
    let last = parse_key(LAST_KEY).unwrap_or_else(|_| unreachable!("constant parses"));
    let mut cursor = parse_key(FIRST_KEY).unwrap_or_else(|_| unreachable!("constant parses"));
    (0..n)
        .map(|_| {
            let key = cursor.between(&last).unwrap_or_else(|| cursor.next());
            let rendered = key.to_string();
            cursor = key;
            rendered
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Row reads
// ---------------------------------------------------------------------------

#[derive(FromQueryResult)]
struct CardRow {
    list_id: Uuid,
    rank: Option<String>,
}

#[derive(FromQueryResult)]
struct RankRow {
    rank: Option<String>,
}

#[derive(FromQueryResult)]
struct IdRow {
    id: Uuid,
}

async fn card_row(tx: &DatabaseTransaction, id: Uuid) -> Result<CardRow, ApiError> {
    let row = tx
        .query_one(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT list_id, rank FROM board.card WHERE id = $1 FOR UPDATE",
            [id.into()],
        ))
        .await
        .map_err(internal_db("load card"))?
        .ok_or_else(|| ApiError::not_found(format!("card {id} not found")))?;
    CardRow::from_query_result(&row, "").map_err(internal_db("decode card row"))
}

/// The rank of a card used as a direct bound. A NULL rank (a row predating
/// the insert trigger) is treated as the smallest key.
async fn bound_rank(
    tx: &DatabaseTransaction,
    id: Uuid,
    role: &str,
) -> Result<String, ApiError> {
    let row = tx
        .query_one(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT rank FROM board.card WHERE id = $1",
            [id.into()],
        ))
        .await
        .map_err(internal_db("load bound card"))?
        .ok_or_else(|| ApiError::not_found(format!("{role} card {id} not found")))?;
    let rank = RankRow::from_query_result(&row, "")
        .map_err(internal_db("decode rank row"))?
        .rank;
    Ok(rank.unwrap_or_else(|| FIRST_KEY.to_string()))
}

/// The smallest rank in `list_id` strictly greater than `lo`, excluding the
/// moving card. None when `lo` is already the list's last key.
async fn successor_of(
    tx: &DatabaseTransaction,
    list_id: Uuid,
    lo: &str,
    moving: Uuid,
) -> Result<Option<String>, ApiError> {
    rank_neighbour(
        tx,
        list_id,
        moving,
        "AND rank COLLATE \"C\" > $3 ORDER BY rank COLLATE \"C\" LIMIT 1",
        lo,
    )
    .await
}

/// The largest rank in `list_id` strictly smaller than `hi`, excluding the
/// moving card. None when `hi` is already the list's first key.
async fn predecessor_of(
    tx: &DatabaseTransaction,
    list_id: Uuid,
    hi: &str,
    moving: Uuid,
) -> Result<Option<String>, ApiError> {
    rank_neighbour(
        tx,
        list_id,
        moving,
        "AND rank COLLATE \"C\" < $3 ORDER BY rank COLLATE \"C\" DESC LIMIT 1",
        hi,
    )
    .await
}

async fn rank_neighbour(
    tx: &DatabaseTransaction,
    list_id: Uuid,
    moving: Uuid,
    tail: &str,
    bound: &str,
) -> Result<Option<String>, ApiError> {
    let sql = format!(
        "SELECT rank FROM board.card \
         WHERE list_id = $1 AND id <> $2 AND rank IS NOT NULL {tail}"
    );
    let row = tx
        .query_one(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [list_id.into(), moving.into(), bound.to_string().into()],
        ))
        .await
        .map_err(internal_db("load neighbour"))?;
    match row {
        Some(row) => Ok(RankRow::from_query_result(&row, "")
            .map_err(internal_db("decode neighbour"))?
            .rank),
        None => Ok(None),
    }
}

/// The list's largest rank, excluding the moving card. None when the list is
/// (effectively) empty.
async fn max_rank(
    tx: &DatabaseTransaction,
    list_id: Uuid,
    moving: Uuid,
) -> Result<Option<String>, ApiError> {
    let row = tx
        .query_one(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT rank FROM board.card \
             WHERE list_id = $1 AND id <> $2 AND rank IS NOT NULL \
             ORDER BY rank COLLATE \"C\" DESC LIMIT 1",
            [list_id.into(), moving.into()],
        ))
        .await
        .map_err(internal_db("load max rank"))?;
    match row {
        Some(row) => Ok(RankRow::from_query_result(&row, "")
            .map_err(internal_db("decode max rank"))?
            .rank),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Context plumbing
// ---------------------------------------------------------------------------

/// Mirrors the generated command plane's RLS context bundle
/// (`templates/ddd/command.tera`): ONE simple-query payload of set_config
/// calls plus the role flip. UUID Display output is hex — no escaping
/// needed; the role string is escaped defensively.
async fn set_rls_session_vars(
    tx: &DatabaseTransaction,
    ctx: &ApiKeyInfo,
) -> Result<(), ApiError> {
    let sql = format!(
        "SELECT set_config('app.current_api_key', '{}', true), \
         set_config('app.current_api_key_id', '{}', true), \
         set_config('app.organization_id', '{}', true), \
         set_config('app.user_id', '{}', true), \
         set_config('app.role', '{}', true); \
         SET LOCAL ROLE app_user;",
        ctx.api_key_id,
        ctx.api_key_id,
        ctx.organization_id,
        ctx.user_id,
        ctx.role.replace('\'', "''"),
    );
    tx.execute_unprepared(&sql)
        .await
        .map_err(internal_db("set rls session vars"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, message: message.into() }
    }
    fn conflict(message: impl Into<String>) -> Self {
        Self { status: StatusCode::CONFLICT, message: message.into() }
    }
    fn not_found(message: impl Into<String>) -> Self {
        Self { status: StatusCode::NOT_FOUND, message: message.into() }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: message.into() }
    }
}

fn internal_db(stage: &'static str) -> impl Fn(sea_orm::DbErr) -> ApiError {
    move |e| ApiError::internal(format!("{stage}: {e}"))
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({ "error": { "message": self.message } });
        (self.status, Json(body)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Mounted at the board domain root by the `api/router.tera` override.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/cards/{card_id}/move",
            post(move_card).guard("board:cards", "update"),
        )
        .route(
            "/lists/{list_id}/rebalance",
            post(rebalance_list).guard("board:lists", "update"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midpoint_lands_between() {
        let mid = midpoint("0|a1", "0|a3").expect("room between a1 and a3");
        assert!(mid.as_str() > "0|a1" && mid.as_str() < "0|a3", "{mid}");
    }

    #[test]
    fn midpoint_of_equal_keys_is_exhausted() {
        assert!(midpoint("0|a1", "0|a1").is_none());
    }

    #[test]
    fn next_and_prev_are_strict() {
        let next = next_key("0|h9");
        assert!(next.as_str() > "0|h9");
        let prev = prev_key(&next);
        assert!(prev.as_str() < next.as_str());
    }

    #[test]
    fn spread_keys_are_sorted_and_short() {
        let keys = spread_keys(5);
        assert_eq!(keys.len(), 5);
        for pair in keys.windows(2) {
            assert!(pair[1].as_str() > pair[0].as_str());
        }
        assert!(keys.iter().all(|k| k.len() <= 8), "keys stay short: {keys:?}");
    }
}
