//! Postgres provisioning for full-stack integration tests: resolves a usable
//! base target (`DATABASE_URL` — CI service containers, or a locally started
//! supabase image container), then creates, migrates and drops unique
//! per-run databases.
//!
//! The base target is deliberately NOT assumed to be a superuser: on the
//! Supabase image `postgres` is `CREATEROLE` + `BYPASSRLS` but
//! `NOSUPERUSER`, and the app plane's `SET LOCAL ROLE app_user`
//! (NOBYPASSRLS) is what makes RLS honest. Only the prelude's reserved-role
//! block needs a superuser, so `apply_prelude` resolves a separate admin
//! target lazily — a gate that never applies the prelude needs no admin
//! (#727).

use std::path::Path;

use codegraph_ops::db::{psql_exec, psql_query};
use codegraph_ops::migrate::run_api_migrations;
use codegraph_ops::pg::PgTarget;
use codegraph_ops::supabase::SUPABASE_POSTGRES_IMAGE;

/// Prefix for the probe database and per-run databases.
const DB_PREFIX: &str = "ifml_gate";
/// Role label used in log messages for per-run databases.
const DB_ROLE: &str = "gate";
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

/// Resolve a usable base Postgres target: `DATABASE_URL` (default
/// `postgres://postgres:postgres@localhost:5432/postgres` — CI's service
/// containers), reachable and able to host per-run databases.
///
/// There is deliberately NO container-provisioning fallback: the supabase
/// image/container is used precisely because its default database carries
/// the `auth` schema the generated migration set needs, and pointing
/// `DATABASE_URL` at it is the one-liner the error prints.
pub async fn resolve_base_target() -> Result<PgTarget, String> {
    let Some(target) = base_target() else {
        return Err(base_unavailable_error("DATABASE_URL is not set"));
    };
    if !database_reachable(&target).await {
        return Err(base_unavailable_error(format!(
            "{} is not reachable",
            default_database_url()
        )));
    }
    let probe = format!("CREATE DATABASE {DB_PREFIX}_probe;");
    if psql_exec(&target, &probe).await.is_err() {
        return Err(base_unavailable_error(format!(
            "{} lacks CREATEDB",
            default_database_url()
        )));
    }
    let _ = psql_exec(&target, &format!("DROP DATABASE {DB_PREFIX}_probe;")).await;
    Ok(target)
}

/// The actionable error for an unusable base target: name the image the
/// generated migration set needs and the one-liner that starts it.
fn base_unavailable_error(reason: impl std::fmt::Display) -> String {
    format!(
        "gate: no usable Postgres target — {reason}. The generated migration \
         set needs the Supabase bootstrap (the `auth` schema), which only \
         {SUPABASE_POSTGRES_IMAGE} provides. Point DATABASE_URL at it, e.g.:\n  \
         docker run -d --name gate-pg -e POSTGRES_USER=supabase_admin -e POSTGRES_PASSWORD=postgres \
         -p 127.0.0.1:15432:5432 {SUPABASE_POSTGRES_IMAGE}\n  \
         export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:15432/postgres"
    )
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

    /// The unresolvable-base error must be actionable: it names the image the
    /// generated migration set needs and the docker run + DATABASE_URL
    /// one-liners, so a clean machine fails forward instead of provisioning
    /// containers itself.
    #[test]
    fn base_unavailable_error_names_the_image_and_the_one_liners() {
        let err = base_unavailable_error("DATABASE_URL is not set");
        assert!(err.contains("no usable Postgres target"), "{err}");
        assert!(err.contains("SUPABASE"), "{err}");
        assert!(err.contains("docker run"), "{err}");
        assert!(err.contains("DATABASE_URL=postgres://"), "{err}");
    }
}
