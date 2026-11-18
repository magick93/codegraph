//! Postgres provisioning for full-stack integration tests: resolves a usable
//! base target (DATABASE_URL, falling back to a dedicated docker supabase
//! postgres container when the URL is unreachable or lacks CREATEDB), then
//! creates, migrates and drops unique per-run databases.
//!
//! The base target is deliberately NOT assumed to be a superuser: on the
//! Supabase image `postgres` is `CREATEROLE` + `BYPASSRLS` but
//! `NOSUPERUSER`, and the app plane's `SET LOCAL ROLE app_user`
//! (NOBYPASSRLS) is what makes RLS honest. Only the prelude's reserved-role
//! block needs the image's superuser, so it alone runs on a separate admin
//! target (#727).

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

/// Resolve the ADMIN target for `base`: the role allowed to create the
/// image's reserved roles (`supabase_realtime_admin` and friends), which a
/// `NOSUPERUSER` base cannot (#727). The admin keeps the base target's
/// host/port/database and differs only in the role (and password, from
/// `GATE_ADMIN_PASSWORD`, defaulting to the base password). Failing to
/// connect as that role is a HARD error naming the overrides and the image
/// contract — never a silent fall back to a non-superuser prelude.
async fn resolve_admin_target(base: &PgTarget) -> Result<PgTarget, String> {
    let is_superuser = psql_query(
        base,
        "SELECT rolsuper::text FROM pg_roles WHERE rolname = current_user;",
    )
    .await
    .map_err(|e| {
        format!(
            "gate: cannot read the privilege of {}@{}:{}: {e}",
            base.user, base.host, base.port
        )
    })?
    .trim()
        == "t";
    let override_user = std::env::var(ADMIN_USER_ENV).ok();
    let admin_user = choose_admin_user(is_superuser, override_user.as_deref(), &base.user);
    if admin_user == base.user {
        return Ok(base.clone());
    }
    let admin = PgTarget {
        user: admin_user,
        password: std::env::var(ADMIN_PASSWORD_ENV).unwrap_or_else(|_| base.password.clone()),
        ..base.clone()
    };
    if database_reachable(&admin).await {
        return Ok(admin);
    }
    Err(format!(
        "gate: the prelude needs a superuser to create the image's reserved roles, but the base \
         target {}@{}:{} is not one and the admin role {} cannot connect either. Set \
         {ADMIN_USER_ENV} / {ADMIN_PASSWORD_ENV} to a superuser, or start the gate container with \
         POSTGRES_USER={DEFAULT_ADMIN_USER} (the documented image contract: the Supabase \
         image's superuser is its POSTGRES_USER).",
        base.user, base.host, base.port, admin.user
    ))
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
    // a half-initialised one answers connections but has no `auth` tables, no
    // pgcrypto and no `authenticator` role, so reuse must verify the contract
    // first and be LOUD when it recreates.
    wait_until_reachable(&target, FALLBACK_READY_TIMEOUT).await?;
    let inspect_env = docker(&[
        "inspect",
        FALLBACK_CONTAINER,
        "--format",
        "{{range .Config.Env}}{{println .}}{{end}}",
    ])?;
    let mut reasons: Vec<String> = fallback_init_user_error(&inspect_env).into_iter().collect();
    reasons.extend(fallback_contract_errors(&target).await);
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

/// `docker inspect`'s `.Config.Env` lines → the init-user verdict. Pure so a
/// stale contract is unit-testable without docker.
fn fallback_init_user_error(inspect_env: &str) -> Option<String> {
    match inspect_env
        .lines()
        .find_map(|line| line.strip_prefix("POSTGRES_USER="))
    {
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

/// The initialised-state half of the fallback contract: the image's bootstrap
/// is what creates the `auth` tables, installs pgcrypto and the
/// `authenticator` role. One round trip, empty when the container is usable.
async fn fallback_contract_errors(target: &PgTarget) -> Vec<String> {
    let sql = "SELECT concat_ws(',', \
               (SELECT count(*) FROM information_schema.tables WHERE table_schema = 'auth'), \
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
        errors
            .push("the `auth` schema has no tables (the image bootstrap did not complete)".into());
    }
    if counts[1] == 0 {
        errors.push("the `pgcrypto` extension is missing".into());
    }
    if counts[2] == 0 {
        errors.push("the `authenticator` role is missing".into());
    }
    errors
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
/// `target` is the APP target — the role in `DATABASE_URL` (`postgres` on the
/// Supabase image, `NOSUPERUSER`), which migrates, provisions API keys and is
/// the URL the booted app receives, so every RLS claim rests on the generated
/// handlers' `SET LOCAL ROLE app_user`; `admin` is the PRELUDE target, the
/// image's superuser (`supabase_admin`), the only role that may create the
/// image's reserved roles. Only `apply_prelude` uses `admin`.
pub struct GateDb {
    target: PgTarget,
    admin: PgTarget,
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
        // Resolve the prelude role BEFORE creating the database: a failed
        // resolution must not leave an orphan per-run database behind.
        let mut admin = resolve_admin_target(&base).await?;
        admin.db = name.clone();
        admin.role = DB_ROLE.to_string();
        let mut target = base.clone();
        target.db = name.clone();
        target.role = DB_ROLE.to_string();
        psql_exec(&base, &format!("CREATE DATABASE {name};"))
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self {
            target,
            admin,
            base,
        })
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
    /// Runs on the ADMIN target, not the app target: `supabase_realtime_admin`
    /// and its siblings are RESERVED roles on the Supabase image, so creating
    /// them requires a superuser — a `NOSUPERUSER` prelude fails with
    /// `"supabase_realtime_admin" is a reserved role, only superusers can
    /// modify it` (#727). Nothing else in the harness needs that privilege.
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
CREATE SCHEMA IF NOT EXISTS pgmq;
CREATE OR REPLACE FUNCTION public.emit_domain_event() RETURNS trigger AS $fn$
BEGIN
  RETURN NULL;
END;
$fn$ LANGUAGE plpgsql;
"#;
        psql_exec(&self.admin, sql).await.map_err(|e| e.to_string())
    }

    /// Run the generated migration set as the APP target — the role the app
    /// runs as, so the migration set is exercised under the same privileges a
    /// deployment has.
    pub async fn apply_migrations(&self, migrations_dir: &Path) -> Result<(), String> {
        run_api_migrations(migrations_dir, &self.target)
            .await
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

    /// #730: the reused-container verdict names the mismatch, so the loud
    /// recreate line says what is wrong rather than "something changed".
    #[test]
    fn fallback_init_user_contract_names_the_mismatch() {
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
    }
}
