//! Postgres provisioning for full-stack integration tests: resolves a usable
//! base target (DATABASE_URL, falling back to a dedicated docker postgres:16
//! container when the URL is unreachable or lacks CREATEDB), then creates,
//! migrates and drops unique per-run databases.

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

fn default_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string())
}

/// The tests always talk to Postgres as the (super)user in DATABASE_URL so
/// `SET LOCAL ROLE app_user` works without membership grants.
pub fn base_target() -> Option<PgTarget> {
    PgTarget::from_url(&default_database_url()).ok()
}

pub async fn database_reachable(target: &PgTarget) -> bool {
    psql_query(target, "SELECT 1").await.is_ok()
}

/// Resolve a usable base Postgres target. The configured DATABASE_URL wins
/// when it can host per-run databases (CI service containers, local docker
/// defaults). When it is unreachable or lacks CREATEDB (restricted shared
/// instances), bootstraps a dedicated `postgres:16` container on
/// 127.0.0.1:15432 and reuses it across warm runs.
pub async fn resolve_base_target() -> Result<PgTarget, String> {
    if let Some(target) = base_target() {
        if database_reachable(&target).await {
            let probe = format!("CREATE DATABASE {DB_PREFIX}_probe;");
            if psql_exec(&target, &probe).await.is_ok() {
                let _ = psql_exec(&target, &format!("DROP DATABASE {DB_PREFIX}_probe;")).await;
                return Ok(target);
            }
            println!("gate: DATABASE_URL target lacks CREATEDB — falling back to the gate postgres container");
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
    let running = docker(&[
        "ps",
        "--filter",
        &format!("name={FALLBACK_CONTAINER}"),
        "-q",
    ])
    .map(|v| !v.is_empty())
    .unwrap_or(false);
    if !running {
        let exists = docker(&[
            "ps",
            "-a",
            "--filter",
            &format!("name={FALLBACK_CONTAINER}"),
            "-q",
        ])
        .map(|v| !v.is_empty())
        .unwrap_or(false);
        if exists {
            docker(&["start", FALLBACK_CONTAINER])?;
        } else {
            docker(&[
                "run",
                "-d",
                "--name",
                FALLBACK_CONTAINER,
                "-e",
                "POSTGRES_USER=postgres",
                "-e",
                "POSTGRES_PASSWORD=postgres",
                "-p",
                &format!("127.0.0.1:{FALLBACK_PORT}:5432"),
                "postgres:16",
            ])?;
        }
    }
    let target = PgTarget {
        host: "127.0.0.1".into(),
        port: FALLBACK_PORT,
        user: "postgres".into(),
        password: "postgres".into(),
        db: "postgres".into(),
        role: DB_ROLE.into(),
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if database_reachable(&target).await {
            return Ok(target);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err("gate postgres container did not become ready".to_string())
}

/// A unique per-run database: created from a base target, migrated, and
/// force-dropped on cleanup.
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

    /// Connection URL of the per-run database.
    pub fn url(&self) -> String {
        self.target.url()
    }

    /// Plain-Postgres bootstrap the Supabase stack would otherwise provide:
    /// pgcrypto, Supabase-ish roles the generated grants reference, the pgmq
    /// schema the platform grants expect, and a no-op domain event emitter
    /// (the pgmq_setup generator is not part of test profiles).
    pub async fn apply_prelude(&self) -> Result<(), String> {
        let sql = r#"
CREATE EXTENSION IF NOT EXISTS pgcrypto;
DO $$
DECLARE
  role_name TEXT;
BEGIN
  FOREACH role_name IN ARRAY ARRAY['anon', 'authenticated', 'service_role', 'authenticator'] LOOP
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = role_name) THEN
      EXECUTE format('CREATE ROLE %I NOLOGIN', role_name);
    END IF;
  END LOOP;
END $$;
CREATE SCHEMA IF NOT EXISTS pgmq;
CREATE OR REPLACE FUNCTION public.emit_domain_event() RETURNS trigger AS $fn$
BEGIN
  RETURN NULL;
END;
$fn$ LANGUAGE plpgsql;
"#;
        psql_exec(&self.target, sql)
            .await
            .map_err(|e| e.to_string())
    }

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
