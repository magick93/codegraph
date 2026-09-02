//! Client-generic persistence abstraction for the workflow engine.
//!
//! The workflow state machine is a single engine that runs against either a
//! SeaORM `DatabaseTransaction` (native monolith) or a per-request
//! tokio-postgres client (Cloudflare Worker slice, native cornucopia). The
//! engine only needs a small SQL surface — `execute`/`query_one`/`query_all`
//! plus explicit `commit`/`rollback` — so backends implement [`WorkflowTx`]
//! and wrap their result rows in [`WfRow`].
//!
//! RLS session variables (`set_config('app.organization_id', …, true)`) use
//! `is_local = true`, which scopes them to the current transaction. Every
//! backend therefore opens a transaction around each engine operation; the
//! worker slice does this with an explicit `BEGIN`/`COMMIT`/`ROLLBACK` on a
//! per-request client (there is no long-lived connection to keep a session on).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::WorkflowError;
use crate::service::WorkflowService;
use crate::types::{
    ApprovalContext, DelegationContext, ProcessHistoryEntry, TransitionContext, WorkflowState,
};

/// A typed bind parameter for a workflow SQL statement.
#[derive(Debug, Clone)]
pub enum WfParam {
    Uuid(Uuid),
    Str(String),
    Bool(bool),
    I32(i32),
    I64(i64),
    DateTime(DateTime<Utc>),
    /// Typed SQL `NULL`; `type_hint` selects the SQL type
    /// (`"uuid"` | `"text"` | `"int4"`).
    Null(&'static str),
}

/// A materialized column value (used by the in-memory row backend).
#[derive(Debug, Clone)]
pub enum WfValue {
    Null,
    Uuid(Uuid),
    String(String),
    Bool(bool),
    I32(i32),
    I64(i64),
    DateTime(DateTime<Utc>),
    Json(serde_json::Value),
    StringVec(Vec<String>),
}

impl WfValue {
    fn as_uuid(&self) -> Option<Uuid> {
        match self {
            WfValue::Uuid(v) => Some(*v),
            _ => None,
        }
    }
    fn as_string(&self) -> Option<String> {
        match self {
            WfValue::String(v) => Some(v.clone()),
            _ => None,
        }
    }
    fn as_bool(&self) -> Option<bool> {
        match self {
            WfValue::Bool(v) => Some(*v),
            _ => None,
        }
    }
    fn as_i32(&self) -> Option<i32> {
        match self {
            WfValue::I32(v) => Some(*v),
            _ => None,
        }
    }
    fn as_datetime(&self) -> Option<DateTime<Utc>> {
        match self {
            WfValue::DateTime(v) => Some(*v),
            _ => None,
        }
    }
    fn as_json(&self) -> Option<serde_json::Value> {
        match self {
            WfValue::Json(v) => Some(v.clone()),
            _ => None,
        }
    }
    fn as_string_vec(&self) -> Option<Vec<String>> {
        match self {
            WfValue::StringVec(v) => Some(v.clone()),
            _ => None,
        }
    }
}

/// A single result row, backed by whichever database client produced it.
///
/// Extraction is lazy: the engine calls the typed getters below, which reach
/// into the underlying row of the appropriate backend. This keeps the row
/// opaque to the state machine while allowing the same engine to run on
/// SeaORM (native), tokio-postgres (worker slice) and an in-memory backend
/// (tests).
pub enum WfRow {
    /// Native SeaORM query result.
    #[cfg(not(target_arch = "wasm32"))]
    SeaOrm(sea_orm::QueryResult),
    /// tokio-postgres row (worker slice and native cornucopia).
    Postgres(tokio_postgres::Row),
    /// In-memory row of named values (tests / lightweight adapters).
    Memory(Vec<(String, WfValue)>),
}

impl WfRow {
    /// Resolve a column index by name in the tokio-postgres row.
    fn pg_col(&self, name: &str) -> Result<usize, WorkflowError> {
        let row = match self {
            WfRow::Postgres(r) => r,
            #[cfg(not(target_arch = "wasm32"))]
            WfRow::SeaOrm(_) => {
                return Err(WorkflowError::Internal(
                    "pg_col called on a SeaORM row".into(),
                ))
            }
            WfRow::Memory(_) => {
                return Err(WorkflowError::Internal(
                    "pg_col called on a memory row".into(),
                ))
            }
        };
        row.columns()
            .iter()
            .position(|c| {
                let n = c.name();
                n == name || n.rsplit('.').next() == Some(name)
            })
            .ok_or_else(|| WorkflowError::Internal(format!("column '{name}' not found").into()))
    }

    /// Resolve a column value by name in the in-memory row.
    fn mem_col(&self, name: &str) -> Result<&WfValue, WorkflowError> {
        let vals = match self {
            WfRow::Memory(vals) => vals,
            _ => {
                return Err(WorkflowError::Internal(
                    "mem_col called on a non-memory row".into(),
                ))
            }
        };
        vals.iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
            .ok_or_else(|| WorkflowError::Internal(format!("column '{name}' not found").into()))
    }

    pub fn get_uuid(&self, col: &str) -> Result<Uuid, WorkflowError> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            WfRow::SeaOrm(r) => r
                .try_get("", col)
                .map_err(|e| WorkflowError::Internal(Box::new(e))),
            WfRow::Postgres(r) => {
                let idx = self.pg_col(col)?;
                r.try_get::<_, Uuid>(idx)
                    .map_err(|e| WorkflowError::Internal(Box::new(e)))
            }
            WfRow::Memory(_) => self.mem_col(col)?.as_uuid().ok_or_else(|| {
                WorkflowError::Internal(format!("column '{col}' is not a uuid").into())
            }),
        }
    }

    pub fn get_string(&self, col: &str) -> Result<String, WorkflowError> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            WfRow::SeaOrm(r) => r
                .try_get("", col)
                .map_err(|e| WorkflowError::Internal(Box::new(e))),
            WfRow::Postgres(r) => {
                let idx = self.pg_col(col)?;
                r.try_get::<_, String>(idx)
                    .map_err(|e| WorkflowError::Internal(Box::new(e)))
            }
            WfRow::Memory(_) => self.mem_col(col)?.as_string().ok_or_else(|| {
                WorkflowError::Internal(format!("column '{col}' is not a string").into())
            }),
        }
    }

    pub fn get_bool(&self, col: &str) -> Result<bool, WorkflowError> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            WfRow::SeaOrm(r) => r
                .try_get("", col)
                .map_err(|e| WorkflowError::Internal(Box::new(e))),
            WfRow::Postgres(r) => {
                let idx = self.pg_col(col)?;
                r.try_get::<_, bool>(idx)
                    .map_err(|e| WorkflowError::Internal(Box::new(e)))
            }
            WfRow::Memory(_) => self.mem_col(col)?.as_bool().ok_or_else(|| {
                WorkflowError::Internal(format!("column '{col}' is not a bool").into())
            }),
        }
    }

    pub fn get_i32(&self, col: &str) -> Result<i32, WorkflowError> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            WfRow::SeaOrm(r) => r
                .try_get("", col)
                .map_err(|e| WorkflowError::Internal(Box::new(e))),
            WfRow::Postgres(r) => {
                let idx = self.pg_col(col)?;
                r.try_get::<_, i32>(idx)
                    .map_err(|e| WorkflowError::Internal(Box::new(e)))
            }
            WfRow::Memory(_) => self.mem_col(col)?.as_i32().ok_or_else(|| {
                WorkflowError::Internal(format!("column '{col}' is not an i32").into())
            }),
        }
    }

    pub fn get_datetime(&self, col: &str) -> Result<DateTime<Utc>, WorkflowError> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            WfRow::SeaOrm(r) => r
                .try_get("", col)
                .map_err(|e| WorkflowError::Internal(Box::new(e))),
            WfRow::Postgres(r) => {
                let idx = self.pg_col(col)?;
                r.try_get::<_, DateTime<Utc>>(idx)
                    .map_err(|e| WorkflowError::Internal(Box::new(e)))
            }
            WfRow::Memory(_) => self.mem_col(col)?.as_datetime().ok_or_else(|| {
                WorkflowError::Internal(format!("column '{col}' is not a datetime").into())
            }),
        }
    }

    pub fn get_json(&self, col: &str) -> Result<serde_json::Value, WorkflowError> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            WfRow::SeaOrm(r) => r
                .try_get("", col)
                .map_err(|e| WorkflowError::Internal(Box::new(e))),
            WfRow::Postgres(r) => {
                let idx = self.pg_col(col)?;
                r.try_get::<_, serde_json::Value>(idx)
                    .map_err(|e| WorkflowError::Internal(Box::new(e)))
            }
            WfRow::Memory(_) => self.mem_col(col)?.as_json().ok_or_else(|| {
                WorkflowError::Internal(format!("column '{col}' is not json").into())
            }),
        }
    }

    pub fn get_string_vec(&self, col: &str) -> Result<Vec<String>, WorkflowError> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            WfRow::SeaOrm(r) => r
                .try_get("", col)
                .map_err(|e| WorkflowError::Internal(Box::new(e))),
            WfRow::Postgres(r) => {
                let idx = self.pg_col(col)?;
                r.try_get::<_, Vec<String>>(idx)
                    .map_err(|e| WorkflowError::Internal(Box::new(e)))
            }
            WfRow::Memory(_) => self.mem_col(col)?.as_string_vec().ok_or_else(|| {
                WorkflowError::Internal(format!("column '{col}' is not a string vec").into())
            }),
        }
    }

    pub fn get_opt_string(&self, col: &str) -> Result<Option<String>, WorkflowError> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            WfRow::SeaOrm(r) => Ok(r.try_get::<Option<String>>("", col).ok().flatten()),
            WfRow::Postgres(r) => {
                let idx = self.pg_col(col)?;
                Ok(r.try_get::<_, Option<String>>(idx).ok().flatten())
            }
            WfRow::Memory(_) => Ok(self.mem_col(col)?.as_string()),
        }
    }

    pub fn get_opt_uuid(&self, col: &str) -> Result<Option<Uuid>, WorkflowError> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            WfRow::SeaOrm(r) => Ok(r.try_get::<Option<Uuid>>("", col).ok().flatten()),
            WfRow::Postgres(r) => {
                let idx = self.pg_col(col)?;
                Ok(r.try_get::<_, Option<Uuid>>(idx).ok().flatten())
            }
            WfRow::Memory(_) => Ok(self.mem_col(col)?.as_uuid()),
        }
    }

    pub fn get_opt_i32(&self, col: &str) -> Result<Option<i32>, WorkflowError> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            WfRow::SeaOrm(r) => Ok(r.try_get::<Option<i32>>("", col).ok().flatten()),
            WfRow::Postgres(r) => {
                let idx = self.pg_col(col)?;
                Ok(r.try_get::<_, Option<i32>>(idx).ok().flatten())
            }
            WfRow::Memory(_) => Ok(self.mem_col(col)?.as_i32()),
        }
    }
}

/// A scoped database transaction against which the workflow engine runs.
///
/// Implementations must open a transaction (so `set_config(..., true)` is
/// scoped correctly) and only persist on [`WorkflowTx::commit`].
#[async_trait]
pub trait WorkflowTx: Send + Sync {
    async fn execute(&self, sql: &str, params: &[WfParam]) -> Result<u64, WorkflowError>;

    async fn query_one(
        &self,
        sql: &str,
        params: &[WfParam],
    ) -> Result<Option<WfRow>, WorkflowError>;

    async fn query_all(&self, sql: &str, params: &[WfParam]) -> Result<Vec<WfRow>, WorkflowError>;

    async fn commit(&mut self) -> Result<(), WorkflowError>;

    async fn rollback(&mut self) -> Result<(), WorkflowError>;
}

/// Source of workflow transactions.
#[async_trait]
pub trait WorkflowClient: Send + Sync {
    async fn begin(&self) -> Result<Box<dyn WorkflowTx>, WorkflowError>;
}

/// Client-generic workflow service: one state machine for every backend.
///
/// The engine functions live in [`crate::engine`], [`crate::approval`],
/// [`crate::delegation`] and [`crate::timer`]; this type just opens a
/// transaction and dispatches.
pub struct GenericWorkflowService<C: WorkflowClient> {
    client: C,
}

impl<C: WorkflowClient> GenericWorkflowService<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<C: WorkflowClient> WorkflowService for GenericWorkflowService<C> {
    async fn transition(&self, ctx: TransitionContext) -> Result<WorkflowState, WorkflowError> {
        let mut tx = self.client.begin().await?;
        let result = crate::engine::transition(tx.as_mut(), &ctx).await;
        if result.is_err() {
            let _ = tx.rollback().await;
        }
        result
    }

    async fn approval_action(&self, ctx: ApprovalContext) -> Result<WorkflowState, WorkflowError> {
        let mut tx = self.client.begin().await?;
        let result = crate::engine::approval_action(tx.as_mut(), &ctx).await;
        if result.is_err() {
            let _ = tx.rollback().await;
        }
        result
    }

    async fn get_state(
        &self,
        tenant_id: Uuid,
        domain: &str,
        entity_table: &str,
        entity_id: Uuid,
    ) -> Result<WorkflowState, WorkflowError> {
        let mut tx = self.client.begin().await?;
        let result =
            crate::engine::get_state(tx.as_mut(), tenant_id, domain, entity_table, entity_id).await;
        if result.is_err() {
            let _ = tx.rollback().await;
        }
        result
    }

    async fn delegate(&self, ctx: DelegationContext) -> Result<(), WorkflowError> {
        let mut tx = self.client.begin().await?;
        let result = crate::delegation::execute_delegation(tx.as_mut(), &ctx).await;
        if result.is_err() {
            let _ = tx.rollback().await;
        }
        result
    }

    async fn get_history(
        &self,
        tenant_id: Uuid,
        domain: &str,
        entity_table: &str,
        entity_id: Uuid,
    ) -> Result<Vec<ProcessHistoryEntry>, WorkflowError> {
        let mut tx = self.client.begin().await?;
        let result =
            crate::engine::get_history(tx.as_mut(), tenant_id, domain, entity_table, entity_id)
                .await;
        if result.is_err() {
            let _ = tx.rollback().await;
        }
        result
    }
}
