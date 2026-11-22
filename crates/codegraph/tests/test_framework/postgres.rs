//! Postgres provisioning for full-stack integration tests: resolves a usable
//! base target (DATABASE_URL, falling back to a dedicated docker supabase
//! postgres container when the URL is unreachable or lacks CREATEDB), then
//! creates, migrates and drops unique per-run databases.
//!
//! The base target is deliberately NOT assumed to be a superuser: on the
//! Supabase image `postgres` is `CREATEROLE` + `BYPASSRLS` but
//! `NOSUPERUSER`, and the app plane's `SET LOCAL ROLE app_user`
//! (NOBYPASSRLS) is what makes RLS honest. Only the prelude's reserved-role
//! block needs a superuser, so `apply_prelude` resolves a separate admin
//! target lazily — a gate that never applies the prelude needs no admin
//! (#727).

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use codegraph_ops::db::{psql_exec, psql_query};
use codegraph_ops::migrate::run_api_migrations;
use codegraph_ops::pg::PgTarget;

use super::process;

const FALLBACK_CONTAINER: &str = "ifml-gate-pg";
const FALLBACK_PORT: u16 = 15432;
/// Prefix for the probe database and per-run databases.
const DB_PREFIX: &str = "ifml_gate";
/// Role label used in log messages for per-run databases.
const DB_ROLE: &str = "gate";
/// The Supabase postgres image: the generated stack relies on the `auth`
/// schema (BaseJump sessions, `auth.uid()` RLS) which only this image
/// ships — plain `postgres:*` has no `auth` schema.
const FALLBACK_IMAGE: &str = "public.ecr.aws/supabase/postgres:17.6.1.132";
/// The image's init user (`POSTGRES_USER`). Its entrypoint runs
/// `psql -U supabase_admin` from `/docker-entrypoint-initdb.d/migrate.sh`
/// BEFORE that role exists, so any other init user makes the bootstrap exit
/// early — leaving a container that answers connections with no `auth`
/// tables, no pgcrypto and no `authenticator` role. Reusing such a container
/// silently is the #730 defect.
const FALLBACK_INIT_USER: &str = "supabase_admin";
const FALLBACK_PASSWORD: &str = "postgres";
/// The APP user on the fallback image: its bootstrap creates `postgres` (the
/// role `DATABASE_URL` names in CI) as a NON-superuser, so the tests connect
/// as a role whose row visibility is only honest because the generated plane
/// issues `SET LOCAL ROLE app_user`.
const FALLBACK_APP_USER: &str = "postgres";
/// How long a freshly created fallback container may take to accept
/// connections, and to finish the image's bootstrap (the `auth` tables land
/// later than the port).
const FALLBACK_READY_TIMEOUT: Duration = Duration::from_secs(60);
const FALLBACK_CONTRACT_TIMEOUT: Duration = Duration::from_secs(180);
/// Grace window before a REUSED container is judged stale: a container that
/// is still finishing a (re)start must not be recreated out from under itself.
const FALLBACK_REUSE_GRACE: Duration = Duration::from_secs(30);
/// Environment overrides for the role that runs the prelude's reserved-role
/// block (the Supabase image's superuser is its `POSTGRES_USER`).
const ADMIN_USER_ENV: &str = "GATE_ADMIN_USER";
const ADMIN_PASSWORD_ENV: &str = "GATE_ADMIN_PASSWORD";
/// `POSTGRES_USER=supabase_admin` is the image contract (`ifml-gate.yml`,
/// `entity-gate.yml`): reserved roles such as `supabase_realtime_admin` can
/// only be created by a superuser, and that is the image's superuser.
const DEFAULT_ADMIN_USER: &str = "supabase_admin";

fn default_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string())
}

/// The configured base target — the APP connection. It is not assumed to be
/// a superuser: on the Supabase image `postgres` is `CREATEROLE` +
/// `BYPASSRLS` but `NOSUPERUSER`, and every RLS claim the gates make rests on
/// the generated handlers' `SET LOCAL ROLE app_user` (NOBYPASSRLS), not on
/// this role's privileges. The prelude's reserved roles are the one statement
/// set that needs the image's superuser — see `resolve_admin_target`.
pub fn base_target() -> Option<PgTarget> {
    PgTarget::from_url(&default_database_url()).ok()
}

pub async fn database_reachable(target: &PgTarget) -> bool {
    psql_query(target, "SELECT 1").await.is_ok()
}

/// Pick the role that runs the prelude's reserved-role block. A base target
/// that is already a superuser (a plain `postgres:<n>` dev container, the ops
/// lane's service) administers itself, so this is a no-op there; otherwise
/// the admin is `GATE_ADMIN_USER`, defaulting to the Supabase image's
/// `POSTGRES_USER`. Pure so the decision is unit-testable without a database.
fn choose_admin_user(
    base_is_superuser: bool,
    override_user: Option<&str>,
    base_user: &str,
) -> String {
    if base_is_superuser {
        base_user.to_string()
    } else {
        override_user.unwrap_or(DEFAULT_ADMIN_USER).to_string()
    }
}

/// Parse the `rolsuper` answer. The spelling depends on the query: a bare
/// boolean column renders `t`/`f`, while `rolsuper::text` renders
/// `true`/`false` — comparing against one form silently answered "not a
/// superuser" in the other (the B1 class). Accept both spellings, trimmed and
/// case-insensitive; ANY other answer is `false`, which takes the admin path
/// and its actionable error rather than silently claiming a non-superuser.
fn parse_superuser(raw: &str) -> bool {
    matches!(raw.trim().to_ascii_lowercase().as_str(), "t" | "true")
}

/// Resolve the ADMIN target for `base`: the role allowed to create the
/// image's reserved roles (`supabase_realtime_admin` and friends), which a
/// `NOSUPERUSER` base cannot (#727). The admin keeps the base target's
/// host/port/database and differs only in the role (and password, from
/// `GATE_ADMIN_PASSWORD`, defaulting to the base password).
///
/// Both halves of the privilege are checked, so a wrong knob fails here with
/// its own message instead of surfacing later as the raw
/// `"supabase_realtime_admin" is a reserved role` text: the admin must
/// CONNECT and must actually be a superuser. Failing either is a hard error
/// naming the overrides and the `POSTGRES_USER=supabase_admin` image
/// contract — never a silent fall back to a non-superuser prelude.
async fn resolve_admin_target(base: &PgTarget) -> Result<PgTarget, String> {
    let base_is_superuser = read_superuser(base, "the base target").await?;
    let override_user = std::env::var(ADMIN_USER_ENV).ok();
    let admin_user = choose_admin_user(base_is_superuser, override_user.as_deref(), &base.user);
    if base_is_superuser {
        if let Some(ignored) = override_user.as_deref() {
            println!(
                "gate: {ADMIN_USER_ENV}={ignored} is ignored — the base target \
                 {}@{}:{} is a superuser, so it runs the prelude itself",
                base.user, base.host, base.port
            );
        }
        return Ok(base.clone());
    }
    let admin = PgTarget {
        user: admin_user,
        password: std::env::var(ADMIN_PASSWORD_ENV).unwrap_or_else(|_| base.password.clone()),
        ..base.clone()
    };
    let admin_is_superuser = read_superuser(&admin, "the admin role")
        .await
        .map_err(|e| format!("{e} {ADMIN_HINT}"))?;
    if !admin_is_superuser {
        return Err(format!(
            "gate: {ADMIN_USER_ENV} resolves to {}, which connects but is NOT a superuser \
             (rolsuper=f), so it cannot create the image's reserved roles. {ADMIN_HINT}",
            admin.user
        ));
    }
    Ok(admin)
}

/// What to do about a base target that is not a superuser — the actionable
/// tail every admin-resolution failure carries.
const ADMIN_HINT: &str = "Set GATE_ADMIN_USER / GATE_ADMIN_PASSWORD to a superuser, or start \
                          the gate container with POSTGRES_USER=supabase_admin (the image \
                          contract: the Supabase image's superuser is its POSTGRES_USER).";

/// Read and parse `rolsuper` for `target`, naming `what` in the error so an
/// unreachable admin is distinguishable from an unreachable base.
async fn read_superuser(target: &PgTarget, what: &str) -> Result<bool, String> {
    psql_query(
        target,
        "SELECT rolsuper::text FROM pg_roles WHERE rolname = current_user;",
    )
    .await
    .map(|raw| parse_superuser(&raw))
    .map_err(|e| {
        format!(
            "gate: cannot connect as {what} {}@{}:{} to read its privilege: {e}",
            target.user, target.host, target.port
        )
    })
}

/// Resolve a usable base Postgres target. The configured DATABASE_URL wins
/// when it can host per-run databases (CI service containers, local docker
/// defaults). When it is unreachable or lacks CREATEDB (restricted shared
/// instances), bootstraps a dedicated supabase postgres container on
/// 127.0.0.1:15432 and reuses it across warm runs.
pub async fn resolve_base_target() -> Result<PgTarget, String> {
    if let Some(target) = base_target() {
        if database_reachable(&target).await {
            let probe = format!("CREATE DATABASE {DB_PREFIX}_probe;");
            if psql_exec(&target, &probe).await.is_ok() {
                let _ = psql_exec(&target, &format!("DROP DATABASE {DB_PREFIX}_probe;")).await;
                return Ok(target);
            }
            println!(
                "gate: DATABASE_URL target lacks CREATEDB — falling back to the gate postgres container"
            );
        } else {
            println!(
                "gate: DATABASE_URL unreachable — falling back to the gate postgres container"
            );
        }
    }
    ensure_fallback_postgres().await
}

fn docker(args: &[&str]) -> Result<String, String> {
    let out = Command::new("docker")
        .args(args)
        .output()
        .map_err(|e| format!("spawn docker: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "docker {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

async fn ensure_fallback_postgres() -> Result<PgTarget, String> {
    if !process::have_tool("docker") {
        return Err("no usable DATABASE_URL and docker is not available".to_string());
    }
    let target = PgTarget {
        host: "127.0.0.1".into(),
        port: FALLBACK_PORT,
        user: FALLBACK_APP_USER.into(),
        password: FALLBACK_PASSWORD.into(),
        db: "postgres".into(),
        role: DB_ROLE.into(),
    };
    let running = docker(&[
        "ps",
        "--filter",
        &format!("name={FALLBACK_CONTAINER}"),
        "-q",
    ])
    .map(|v| !v.is_empty())
    .unwrap_or(false);
    let exists = running
        || docker(&[
            "ps",
            "-a",
            "--filter",
            &format!("name={FALLBACK_CONTAINER}"),
            "-q",
        ])
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if !exists {
        run_fallback_container()?;
        wait_for_fallback_contract(&target, FALLBACK_CONTRACT_TIMEOUT).await?;
        return Ok(target);
    }
    if !running {
        docker(&["start", FALLBACK_CONTAINER])?;
    }
    // A container this run did not create is never trusted silently (#730):
    // a half-initialised one answers connections but has no `auth.users`, no
    // pgcrypto and no `authenticator` role, so reuse must verify the contract
    // first and recreate when it does not match. A container that is still
    // finishing a (re)start gets the same grace window the create path has
    // (M4), and the recreate is announced with one stdout line — visible
    // because the lanes run `--nocapture` (there is no log channel).
    wait_until_reachable(&target, FALLBACK_READY_TIMEOUT).await?;
    let inspect_env = docker(&[
        "inspect",
        FALLBACK_CONTAINER,
        "--format",
        "{{range .Config.Env}}{{println .}}{{end}}",
    ])?;
    let image = docker(&[
        "inspect",
        FALLBACK_CONTAINER,
        "--format",
        "{{.Config.Image}}",
    ])?;
    let mut reasons: Vec<String> = fallback_reuse_contract_errors(&inspect_env, &image);
    reasons.extend(wait_for_contract_grace(&target, FALLBACK_REUSE_GRACE).await);
    if reasons.is_empty() {
        println!("gate: reusing {FALLBACK_CONTAINER} (contract verified)");
        return Ok(target);
    }
    println!(
        "gate: recreating {FALLBACK_CONTAINER} — its contract is not the gate's: {}",
        reasons.join("; ")
    );
    docker(&["rm", "-f", FALLBACK_CONTAINER])?;
    run_fallback_container()?;
    wait_for_fallback_contract(&target, FALLBACK_CONTRACT_TIMEOUT).await?;
    Ok(target)
}

/// Start the fallback container with the image contract (#730): the Supabase
/// image's bootstrap only completes when its init user is `supabase_admin`.
fn run_fallback_container() -> Result<(), String> {
    docker(&[
        "run",
        "-d",
        "--name",
        FALLBACK_CONTAINER,
        "-e",
        &format!("POSTGRES_USER={FALLBACK_INIT_USER}"),
        "-e",
        &format!("POSTGRES_PASSWORD={FALLBACK_PASSWORD}"),
        "-p",
        &format!("127.0.0.1:{FALLBACK_PORT}:5432"),
        FALLBACK_IMAGE,
    ])?;
    println!(
        "gate: created {FALLBACK_CONTAINER} \
         (POSTGRES_USER={FALLBACK_INIT_USER}, app user {FALLBACK_APP_USER})"
    );
    Ok(())
}

/// The DOCKER-side half of the reuse contract: the init user and the image.
/// Pure so a stale container is unit-testable without docker.
fn fallback_reuse_contract_errors(inspect_env: &str, image: &str) -> Vec<String> {
    fallback_init_user_error(inspect_env)
        .into_iter()
        .chain(fallback_image_error(image))
        .collect()
}

/// `docker inspect`'s `.Config.Env` lines → the init-user verdict. Pure so a
/// stale contract is unit-testable without docker.
fn fallback_init_user_error(inspect_env: &str) -> Option<String> {
    // `split_once` rather than `strip_prefix`: `POSTGRES_USERX=…` must not
    // read as `POSTGRES_USER=…`.
    let value = inspect_env.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "POSTGRES_USER").then_some(value)
    });
    match value {
        Some(value) if value.trim() == FALLBACK_INIT_USER => None,
        Some(value) => Some(format!(
            "its POSTGRES_USER is {} (must be {FALLBACK_INIT_USER}: the image's bootstrap runs \
             `psql -U {FALLBACK_INIT_USER}` before that role exists)",
            value.trim()
        )),
        None => Some(format!(
            "it sets no POSTGRES_USER (must be {FALLBACK_INIT_USER})"
        )),
    }
}

/// The container's image must be the pinned one — a container built from an
/// older tag is recreated rather than reused.
fn fallback_image_error(image: &str) -> Option<String> {
    let image = image.trim();
    if image == FALLBACK_IMAGE {
        None
    } else {
        Some(format!("its image is {image} (must be {FALLBACK_IMAGE})"))
    }
}

/// The initialised-state half of the fallback contract. Deliberately a
/// PRELUDE-FREE reuse check: it proves the image's bootstrap ran far enough to
/// create `auth.users`, install pgcrypto and create the `authenticator` role —
/// it says nothing about the prelude's reserved roles (admin-only), the
/// `pgmq` schema, or the app role's attributes, which a prelude-using gate
/// exercises for itself. One round trip; empty when the container is usable.
async fn fallback_contract_errors(target: &PgTarget) -> Vec<String> {
    let sql = "SELECT concat_ws(',', \
               (SELECT count(*) FROM information_schema.tables \
                 WHERE table_schema = 'auth' AND table_name = 'users'), \
               (SELECT count(*) FROM pg_extension WHERE extname = 'pgcrypto'), \
               (SELECT count(*) FROM pg_roles WHERE rolname = 'authenticator'));";
    let out = match psql_query(target, sql).await {
        Ok(value) => value,
        Err(e) => return vec![format!("cannot read the Postgres catalog: {e}")],
    };
    let counts: Vec<i64> = out
        .trim()
        .split(',')
        .filter_map(|value| value.trim().parse().ok())
        .collect();
    if counts.len() != 3 {
        return vec![format!("unexpected contract probe answer: {out}")];
    }
    let mut errors = Vec::new();
    if counts[0] == 0 {
        errors.push(
            "the `auth.users` table is missing (the image bootstrap did not complete)".into(),
        );
    }
    if counts[1] == 0 {
        errors.push("the `pgcrypto` extension is missing".into());
    }
    if counts[2] == 0 {
        errors.push("the `authenticator` role is missing".into());
    }
    errors
}

/// Probe the catalog half of the reuse contract for up to `grace`, so a
/// container still finishing a (re)start is not recreated out from under
/// itself. Returns the LAST reasons (empty when the contract landed).
async fn wait_for_contract_grace(target: &PgTarget, grace: Duration) -> Vec<String> {
    let deadline = Instant::now() + grace;
    loop {
        let reasons = fallback_contract_errors(target).await;
        if reasons.is_empty() || Instant::now() >= deadline {
            return reasons;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_until_reachable(target: &PgTarget, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if database_reachable(target).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(format!(
        "gate: {FALLBACK_CONTAINER} did not accept connections within {}s",
        timeout.as_secs()
    ))
}

async fn wait_for_fallback_contract(target: &PgTarget, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let mut reasons = vec!["it never accepted connections".to_string()];
    while Instant::now() < deadline {
        if database_reachable(target).await {
            reasons = fallback_contract_errors(target).await;
            if reasons.is_empty() {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(format!(
        "gate: {FALLBACK_CONTAINER} did not reach the expected contract within {}s: {}",
        timeout.as_secs(),
        reasons.join("; ")
    ))
}

/// A unique per-run database: created from a base target, migrated, and
/// force-dropped on cleanup.
///
/// Two connection targets ride a run, and the split is the point (#727):
/// `target` is the APP target — the role the base names (`postgres` on the
/// Supabase image, `NOSUPERUSER`), which migrates, provisions API keys and is
/// the URL the booted app receives, so every RLS claim rests on the generated
/// handlers' `SET LOCAL ROLE app_user`; the PRELUDE target (the image's
/// superuser) is resolved from `base` inside `apply_prelude` and never
/// stored. Resolving it there is deliberate: only gates that call
/// `apply_prelude` require an admin role, so a gate that never does (the
/// child-tenancy and media runtime gates) still runs on a base whose role has
/// CREATEDB but no reachable admin.
pub struct GateDb {
    target: PgTarget,
    base: PgTarget,
}

impl GateDb {
    pub async fn create(base: PgTarget) -> Result<Self, String> {
        let name = format!(
            "{DB_PREFIX}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis()
        );
        let mut target = base.clone();
        target.db = name.clone();
        target.role = DB_ROLE.to_string();
        psql_exec(&base, &format!("CREATE DATABASE {name};"))
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self { target, base })
    }

    /// Connection URL of the per-run database (the APP target the booted app
    /// uses).
    pub fn url(&self) -> String {
        self.target.url()
    }

    /// Plain-Postgres bootstrap the Supabase stack would otherwise provide:
    /// the `supabase_admin` superuser the image's extension scripts grant
    /// to, the Supabase-ish roles the generated grants reference, pgcrypto,
    /// the pgmq schema the platform grants expect, and a no-op domain event
    /// emitter (the pgmq_setup generator is not part of test profiles).
    ///
    /// Runs on an ADMIN target resolved from the base target HERE (not in
    /// `create`), because `supabase_realtime_admin` and its siblings are
    /// RESERVED roles on the Supabase image: creating them requires a
    /// superuser, and only prelude-needing gates should require one — a
    /// `NOSUPERUSER` prelude fails with `"supabase_realtime_admin" is a
    /// reserved role, only superusers can modify it` (#727). A resolution or
    /// statement failure leaves the per-run database behind for the caller's
    /// `cleanup()`: that is the same post-`create` failure shape as any other
    /// prelude error, not a window the ordering closes.
    pub async fn apply_prelude(&self) -> Result<(), String> {
        let sql = r#"
DO $$
DECLARE
  role_name TEXT;
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'supabase_admin') THEN
    EXECUTE 'CREATE ROLE supabase_admin LOGIN SUPERUSER NOINHERIT';
  END IF;
  FOREACH role_name IN ARRAY ARRAY['anon', 'authenticated', 'service_role', 'authenticator',
                                   'supabase_auth_admin', 'supabase_storage_admin',
                                   'supabase_realtime_admin', 'supabase_read_only_user'] LOOP
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = role_name) THEN
      EXECUTE format('CREATE ROLE %I NOLOGIN', role_name);
    END IF;
  END LOOP;
END $$;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
-- Owned by the ADMIN role, so the app role cannot create tables in it:
-- `CREATE TABLE pgmq.t` as `postgres` answers "permission denied for schema
-- pgmq". No gate combines this prelude with the generated `pgmq_setup`
-- today, so the constraint is inert here; an app-plane writer of pgmq tables
-- needs an owner/GRANT decision and is out of scope (#727 review M6).
CREATE SCHEMA IF NOT EXISTS pgmq;
CREATE OR REPLACE FUNCTION public.emit_domain_event() RETURNS trigger AS $fn$
BEGIN
  RETURN NULL;
END;
$fn$ LANGUAGE plpgsql;
"#;
        let mut admin = resolve_admin_target(&self.base).await?;
        admin.db = self.target.db.clone();
        admin.role = self.target.role.clone();
        psql_exec(&admin, sql).await.map_err(|e| e.to_string())
    }

    /// Run the generated migration set as the APP target (the role the base
    /// names). On the Supabase image that role is `BYPASSRLS` +
    /// `CREATEROLE` — privileges a deployment's `app_user` does not have — so
    /// treat this as the migration OWNER path: it proves the set applies and
    /// that the owner can grant `app_user`, not that a runtime role could run
    /// it. The RLS assertions in the gates go through `SET LOCAL ROLE
    /// app_user` for exactly that reason.
    pub async fn apply_migrations(&self, migrations_dir: &Path) -> Result<(), String> {
        run_api_migrations(migrations_dir, &self.target)
            .await
            .map(|_report| ())
            .map_err(|e| e.to_string())
    }

    /// Provision an `sk_...` API key tests can inject into `/api` requests
    /// (the generated IFML specs post relative URLs with no auth headers of
    /// their own).
    pub async fn provision_api_key(&self) -> Result<String, String> {
        let out = psql_query(
            &self.target,
            "SELECT public.create_api_key('00000000-0000-0000-0000-000000000001'::uuid, 'ifml-gate', \
             '[{\"entity_type\":\"*\",\"entity_id\":\"*\",\"action\":\"*\"}]'::jsonb)::text;",
        )
        .await
        .map_err(|e| e.to_string())?;
        let value: serde_json::Value = serde_json::from_str(out.trim())
            .map_err(|e| format!("create_api_key returned non-JSON ({e}): {out}"))?;
        value
            .get("key")
            .and_then(|k| k.as_str())
            .map(str::to_string)
            .ok_or_else(|| format!("create_api_key returned no key: {out}"))
    }

    pub async fn cleanup(self) {
        let _ = psql_exec(
            &self.base,
            &format!("DROP DATABASE IF EXISTS {} WITH (FORCE);", self.target.db),
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #727: the prelude's admin role follows the BASE target's privilege, so
    /// a superuser base (plain `postgres:<n>`, the ops lane) keeps running the
    /// prelude itself and the Supabase image's `NOSUPERUSER` `postgres` hands
    /// it to `supabase_admin` (or the explicit override).
    #[test]
    fn admin_role_follows_the_base_targets_privilege() {
        assert_eq!(
            choose_admin_user(true, Some("someone_else"), "postgres"),
            "postgres"
        );
        assert_eq!(
            choose_admin_user(false, None, "postgres"),
            DEFAULT_ADMIN_USER
        );
        assert_eq!(
            choose_admin_user(false, Some("custom_admin"), "postgres"),
            "custom_admin"
        );
    }

    /// B1 (#727 review): the SQL→bool derivation was the untested step. A
    /// bare boolean column renders `t`/`f`, `rolsuper::text` renders
    /// `true`/`false` — a parser that bet on one spelling answered "not a
    /// superuser" for the other and broke the plain-postgres ops lane. Any
    /// unrecognised answer must fall to `false` (the admin path, whose error
    /// is actionable), never to `true`.
    ///
    /// TEETH: reverting `parse_superuser` to `raw.trim() == "t"` fails the
    /// `"true"` / `"TRUE"` cases; reverting it to `== "true"` fails `"t"`.
    #[test]
    fn parse_superuser_accepts_both_boolean_text_spellings() {
        for truthy in ["t", "T", "true", "TRUE", "True", " true ", "\tt\n"] {
            assert!(
                parse_superuser(truthy),
                "{truthy:?} must read as a superuser"
            );
        }
        for falsy in ["f", "F", "false", "FALSE", "", " ", "yes", "1", "supabase"] {
            assert!(
                !parse_superuser(falsy),
                "{falsy:?} must not read as a superuser"
            );
        }
    }

    /// #730: the reused-container verdict names the mismatch, so the recreate
    /// line says what is wrong rather than "something changed".
    #[test]
    fn fallback_reuse_contract_names_the_mismatch() {
        assert_eq!(
            fallback_init_user_error(
                "POSTGRES_USER=supabase_admin\nPGDATA=/var/lib/postgresql/data"
            ),
            None
        );
        let stale =
            fallback_init_user_error("POSTGRES_PASSWORD=postgres\nPOSTGRES_USER=postgres\n")
                .expect("a stale init user must be reported");
        assert!(stale.contains("POSTGRES_USER is postgres"), "{stale}");
        assert!(stale.contains(FALLBACK_INIT_USER), "{stale}");
        let missing =
            fallback_init_user_error("PGDATA=/var/lib/postgresql/data").expect("must be reported");
        assert!(missing.contains("sets no POSTGRES_USER"), "{missing}");
        // A longer key must not match the prefix.
        assert_eq!(
            fallback_init_user_error("POSTGRES_USERX=supabase_admin"),
            fallback_init_user_error("PGDATA=/x"),
            "POSTGRES_USERX must not satisfy the POSTGRES_USER contract"
        );
    }

    /// A container built from another image tag is recreated, not reused.
    #[test]
    fn fallback_reuse_contract_requires_the_pinned_image() {
        assert_eq!(fallback_image_error(FALLBACK_IMAGE), None);
        assert_eq!(fallback_image_error(&format!(" {FALLBACK_IMAGE}\n")), None);
        let stale = fallback_image_error("postgres:15").expect("a stale image must be reported");
        assert!(stale.contains("postgres:15"), "{stale}");
        assert!(stale.contains(FALLBACK_IMAGE), "{stale}");
        assert!(
            fallback_reuse_contract_errors("POSTGRES_USER=supabase_admin", "postgres:15")
                .iter()
                .any(|reason| reason.contains("image")),
            "the combined verdict must carry the image mismatch"
        );
    }
}
