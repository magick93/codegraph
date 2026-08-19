//! Phased SQL migration application for plain-Postgres targets.
//!
//! Port of `hr-platform/lib/migrate.sh`, genericised: no hr-specific schema
//! list — schemas are derived from the migration files themselves.
//!
//! Every `*.sql` file in the migration directory is applied, in sorted order
//! within each phase, mirroring what `supabase db reset` does when the same
//! files are symlinked into a Supabase project. Platform migrations
//! (`0001_basejump_install.sql`, `0003_pgmq_setup.sql`, ...) are NOT special-
//! cased: they depend only on a Supabase-compatible Postgres (auth schema,
//! `extensions` schema, standard roles, pg_tle/http/pgmq extensions), which is
//! exactly what the harness's database targets provide.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{OpsError, OpsResult};
use crate::output;
use crate::pg::PgTarget;

/// Advisory lock key shared by all migration runs against a target, so
/// concurrent invocations do not interleave phases.
const MIGRATION_LOCK_KEY: i64 = 73287328;

/// Max SQL lines per FK-phase `psql -c` batch. Keeps the argv well below
/// the OS limit even with hundreds of migration files.
const FK_CHUNK_LINES: usize = 200;

/// Which phase a migration file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    /// Normal table/codelist file — applied in phase 1.
    Tables,
    /// FTS / embeddings / process-history views — phase 3.
    Aux,
    /// RLS + triggers — phase 4.
    RlsTriggers,
    /// Not a SQL migration file (ignored entirely).
    Skip,
}

/// The full phase plan for a migration directory.
#[derive(Debug, Default)]
struct MigrationPhases {
    /// Phase 0: sorted, deduplicated schema names ("platform", "api_keys_private"
    /// plus every `CREATE SCHEMA [IF NOT EXISTS] <x>` found in the files).
    schemas: Vec<String>,
    /// Phase 1: table/codelist files (FK lines are stripped per-file at apply
    /// time and deferred to phase 2).
    tables: Vec<PathBuf>,
    /// Phase 2: FK constraint lines, in file order.
    fk_lines: Vec<String>,
    /// Phase 3: `*_fts.sql`, `*_embedding.sql`, `*_process_history_view.sql`.
    aux: Vec<PathBuf>,
    /// Phase 4: `*_rls.sql`, `*_trigger.sql`.
    rls_triggers: Vec<PathBuf>,
    /// Non-SQL files in the directory, tracked for reporting only.
    ignored: Vec<PathBuf>,
}

/// Classify a migration basename into its application phase.
///
/// All `*.sql` files are applied. Only non-SQL files (notes, READMEs, ...) are
/// skipped. There is no special handling for platform files such as
/// `0001_basejump_install.sql` or `0003_pgmq_setup.sql` — they apply like any
/// other migration and must run in numeric-prefix order (they do: phase 1
/// iterates files sorted by name).
fn classify_file(name: &str) -> FileKind {
    if name.ends_with("_rls.sql") || name.ends_with("_trigger.sql") {
        FileKind::RlsTriggers
    } else if name.ends_with("_fts.sql")
        || name.ends_with("_embedding.sql")
        || name.ends_with("_process_history_view.sql")
    {
        FileKind::Aux
    } else if name.ends_with(".sql") {
        FileKind::Tables
    } else {
        FileKind::Skip
    }
}

/// True for FK constraint lines (excluded from phase 1, applied in phase 2).
/// Matches `ALTER TABLE ... ADD CONSTRAINT ... FOREIGN KEY` regardless of
/// leading indentation. Non-FK constraint additions (CHECK, UNIQUE, PRIMARY
/// KEY) are left in phase 1.
fn is_fk_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("ALTER TABLE")
        && trimmed.contains("ADD CONSTRAINT")
        && trimmed.contains("FOREIGN KEY")
}

/// Phase-1 content for a file: the file with FK constraint lines removed.
fn phase1_content(content: &str) -> String {
    content
        .lines()
        .filter(|line| !is_fk_line(line))
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Split SQL lines into chunks of at most `max_lines` lines each, joined by
/// newlines. Large `psql -c` payloads overflow the OS argv limit
/// ("Argument list too long"), so long inputs are applied in batches.
fn chunk_sql(lines: &[String], max_lines: usize) -> Vec<String> {
    if max_lines == 0 {
        return vec![lines.join("\n")];
    }
    lines
        .chunks(max_lines)
        .map(|chunk| chunk.join("\n"))
        .collect()
}

/// Last `max` chars of `s` for error messages (UTF-8 safe).
fn error_tail(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let skipped = chars.len() - max;
    let rest: String = chars[skipped..].iter().collect();
    format!("…[truncated {skipped} chars]\n{rest}")
}

/// First whitespace-delimited token of `s`, honouring double-quoted names
/// (which may contain spaces, e.g. `CREATE SCHEMA "some schema"`).
fn first_schema_token(s: &str) -> &str {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"').unwrap_or(rest.len());
        &rest[..end]
    } else {
        s.split_whitespace().next().unwrap_or("")
    }
}

/// Extract schema names from `CREATE SCHEMA [IF NOT EXISTS] <name>` lines.
/// Handles double-quoted names, tolerates a trailing semicolon, and skips
/// comment lines (which may mention schemas).
fn extract_create_schemas(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("--") {
                return None;
            }
            // Match the longer prefix first so `CREATE SCHEMA IF NOT EXISTS`
            // isn't consumed as `CREATE SCHEMA` + `IF ...`.
            let rest = trimmed
                .strip_prefix("CREATE SCHEMA IF NOT EXISTS")
                .or_else(|| trimmed.strip_prefix("CREATE SCHEMA"))?;
            let raw = rest.trim().trim_end_matches(';').trim();
            let name = first_schema_token(raw).trim().trim_matches('"');
            if !name.is_empty() {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Build the phase plan for a migration directory.
fn migration_phases(dir: &Path) -> OpsResult<MigrationPhases> {
    let mut schemas = vec!["platform".to_string(), "api_keys_private".to_string()];
    let mut tables = Vec::new();
    let mut fk_lines = Vec::new();
    let mut aux = Vec::new();
    let mut rls_triggers = Vec::new();
    let mut ignored = Vec::new();

    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    for path in &files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Schemas and FK lines are gathered from ALL sql files, mirroring the
        // original `grep -rh ... "$migration_dir"/*.sql`. Every `.sql` file is
        // applied; only non-SQL files are skipped.
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return Err(OpsError::Io(e)),
        };
        schemas.extend(extract_create_schemas(&content));
        fk_lines.extend(
            content
                .lines()
                .filter(|line| is_fk_line(line))
                .map(|line| line.to_string()),
        );
        match classify_file(&name) {
            FileKind::Skip => ignored.push(path.clone()),
            FileKind::Tables => tables.push(path.clone()),
            FileKind::Aux => aux.push(path.clone()),
            FileKind::RlsTriggers => rls_triggers.push(path.clone()),
        }
    }

    schemas.sort();
    schemas.dedup();

    Ok(MigrationPhases {
        schemas,
        tables,
        fk_lines,
        aux,
        rls_triggers,
        ignored,
    })
}

/// Monotonic counter for unique temp-file names within a process.
fn temp_counter() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Write phase-1 content to a temp file so it can be applied via
/// `psql_exec_file_ok` (psql takes a path, and the FK lines must be removed
/// from the input first). Returns the temp path.
fn write_phase1_file(original: &Path, content: &str) -> OpsResult<PathBuf> {
    let stem = original
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "migration".to_string());
    let path = std::env::temp_dir().join(format!(
        "codegraph-ops-phase1-{}-{}-{}.sql",
        std::process::id(),
        stem,
        temp_counter()
    ));
    let mut file = std::fs::File::create(&path)?;
    file.write_all(content.as_bytes())?;
    Ok(path)
}

/// Symlink every `*.sql` in `migration_dir` into `supabase_mig_dir` so
/// `supabase db reset` applies them. First removes ALL existing symlinks in
/// `supabase_mig_dir` (stale links from previous runs). Returns the count
/// linked. Logs via [`output::ok`].
pub fn link_migrations_to_supabase(
    migration_dir: &Path,
    supabase_mig_dir: &Path,
) -> OpsResult<usize> {
    if !supabase_mig_dir.is_dir() {
        std::fs::create_dir_all(supabase_mig_dir)?;
    }
    for entry in std::fs::read_dir(supabase_mig_dir)? {
        let path = entry?.path();
        if path.is_symlink() {
            std::fs::remove_file(&path)?;
        }
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(migration_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.is_file() && p.extension().map(|x| x == "sql").unwrap_or(false))
        .collect();
    files.sort();

    let mut count = 0usize;
    for file in &files {
        let name = file
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        std::os::unix::fs::symlink(file, supabase_mig_dir.join(name))?;
        count += 1;
    }
    output::ok(format!(
        "Symlinked {count} generated migrations into {}/",
        supabase_mig_dir.display()
    ));
    Ok(count)
}

/// Remove symlinks from a supabase migrations dir (used by `clean`). Real
/// files are left untouched.
pub fn remove_supabase_links(supabase_mig_dir: &Path) -> OpsResult<()> {
    if !supabase_mig_dir.is_dir() {
        output::info(format!(
            "{} does not exist — nothing to clean",
            supabase_mig_dir.display()
        ));
        return Ok(());
    }
    let mut removed = 0usize;
    for entry in std::fs::read_dir(supabase_mig_dir)? {
        let path = entry?.path();
        if path.is_symlink() {
            std::fs::remove_file(&path)?;
            removed += 1;
        }
    }
    output::ok(format!(
        "Removed {removed} symlinks from {}",
        supabase_mig_dir.display()
    ));
    Ok(())
}

/// Post-migration grant/verification options for the generated API role.
///
/// The generated migrations grant the API role (`app_user` by default) DML on
/// domain tables via `0002_api_key_management.sql` + per-table grants. This
/// harness step re-applies the same grants after every migration run (so
/// consumers with pre-fix migration files self-heal) and then verifies every
/// table in a domain schema is actually granted — warning or hard-failing
/// (`strict`) when it isn't.
#[derive(Debug, Clone)]
pub struct GrantOptions {
    /// Postgres role the generated API connects as. Defaults to `app_user`.
    pub role: String,
    /// Hard-fail the migration run when the role is missing DML on any table
    /// in a domain schema after migration (default: warn and continue).
    pub strict: bool,
}

impl Default for GrantOptions {
    fn default() -> Self {
        Self {
            role: "app_user".to_string(),
            strict: false,
        }
    }
}

/// Schemas excluded from the app-role grant sweep: system schemas, Supabase
/// infra and the schemas that are granted explicitly (basejump, api_keys_private).
const GRANT_SKIP_SCHEMAS: &[&str] = &[
    "public",
    "auth",
    "storage",
    "graphql",
    "extensions",
    "pg_catalog",
    "information_schema",
    "pg_toast",
    "basejump",
    "api_keys_private",
    "pgmq",
    "supabase_migrations",
    "pgbouncer",
    "realtime",
];

/// Apply generated SQL migrations to a plain-Postgres target in dependency
/// order, then grant the API role DML on domain tables and verify the grants
/// (default [`GrantOptions`]). Generated migrations have cross-schema deps,
/// hence phases:
///
/// * Phase 0: `CREATE SCHEMA` — "platform" + "api_keys_private" plus every
///   `CREATE SCHEMA [IF NOT EXISTS] <x>` found in the migration files (dedup,
///   sort), applied via `psql_exec`.
/// * Phase 1: `CREATE TABLE` + codelists — every table file with its FK
///   constraint lines removed, applied tolerantly (`psql_exec_file_ok`);
///   per-file errors are collected and warned, not fatal.
/// * Phase 2: FK constraints — all `ALTER TABLE ... ADD CONSTRAINT ... FOREIGN
///   KEY` lines, chunked into bounded batches and applied via `psql_exec`;
///   per-chunk failure is a warning (the run continues).
/// * Phase 3: aux files (`*_fts.sql`, `*_embedding.sql`,
///   `*_process_history_view.sql`) via `psql_exec_file_ok`.
/// * Phase 4: `*_rls.sql` + `*_trigger.sql` via `psql_exec_file_ok`.
///
/// An advisory lock (key 73287328) is acquired before phase 0 and released
/// afterwards, on both success and failure paths.
pub async fn run_api_migrations(migration_dir: &Path, target: &PgTarget) -> OpsResult<()> {
    run_api_migrations_with_options(migration_dir, target, &GrantOptions::default()).await
}

/// [`run_api_migrations`] with explicit [`GrantOptions`].
pub async fn run_api_migrations_with_options(
    migration_dir: &Path,
    target: &PgTarget,
    grant: &GrantOptions,
) -> OpsResult<()> {
    let phases = migration_phases(migration_dir)?;

    output::section("Applying migrations");
    match crate::db::advisory_lock(target, MIGRATION_LOCK_KEY).await {
        Ok(()) => output::ok("Migration advisory lock acquired"),
        Err(e) => output::warn(format!("advisory lock failed (continuing): {e}")),
    }
    match crate::db::missing_extensions(migration_dir, target).await {
        Ok(missing) if !missing.is_empty() => {
            output::warn(format!("missing extensions: {}", missing.join(", ")))
        }
        Ok(_) => {}
        Err(e) => output::warn(format!("extension check failed: {e}")),
    }

    let result = match apply_phases(&phases, target).await {
        Ok(()) => {
            output::section("Granting API role access");
            grant_app_user_access(&phases, target, grant).await
        }
        Err(e) => Err(e),
    };

    match crate::db::advisory_unlock(target, MIGRATION_LOCK_KEY).await {
        Ok(()) => output::ok("Migration advisory lock released"),
        Err(e) => output::warn(format!("advisory unlock failed: {e}")),
    }
    result
}

/// Domain schemas the app role needs DML on: every schema discovered from the
/// migration files minus the [`GRANT_SKIP_SCHEMAS`] exclusion list (and any
/// `pg_*` system schemas). Kept sorted + deduped.
fn grant_schemas(phases: &MigrationPhases) -> Vec<String> {
    let mut out = Vec::new();
    for s in &phases.schemas {
        if GRANT_SKIP_SCHEMAS.contains(&s.as_str()) || s.starts_with("pg_") {
            continue;
        }
        if !out.contains(s) {
            out.push(s.clone());
        }
    }
    out.sort();
    out
}

/// Post-migration step: grant the API role DML on every table in the domain
/// schemas (idempotent) and verify nothing is missing. Missing grants become
/// a hard error when `grant.strict` is set, otherwise a warning listing the
/// affected tables.
async fn grant_app_user_access(
    phases: &MigrationPhases,
    target: &PgTarget,
    grant: &GrantOptions,
) -> OpsResult<()> {
    let schemas = grant_schemas(phases);
    if schemas.is_empty() {
        output::info("no domain schemas to grant — skipping");
        return Ok(());
    }

    // The grant role is created by a generated migration (0002). If it isn't
    // present the profile doesn't use it (sqlite / non-api-key) — skip.
    let exists = crate::db::psql_query(
        target,
        &format!(
            "SELECT 1 FROM pg_roles WHERE rolname = {};",
            quote_literal(&grant.role)
        ),
    )
    .await
    .map(|v| !v.is_empty())
    .unwrap_or(false);
    if !exists {
        output::info(format!(
            "role {:?} not present — skipping app-role grant sweep",
            grant.role
        ));
        return Ok(());
    }

    for s in &schemas {
        let sql = format!(
            "GRANT USAGE ON SCHEMA {0} TO {1};\n\
             GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA {0} TO {1};\n\
             ALTER DEFAULT PRIVILEGES IN SCHEMA {0} GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO {1};",
            quote_ident(s),
            quote_ident(&grant.role)
        );
        crate::db::psql_exec(target, &sql)
            .await
            .map_err(|e| OpsError::Command(format!("grant sweep on {s:?} failed: {e}")))?;
    }
    output::ok(format!(
        "Granted {} DML on {} schema(s)",
        grant.role,
        schemas.len()
    ));

    let missing = verify_grant_access(target, &schemas, &grant.role).await?;
    report_missing_grants(&missing, &grant.role, grant.strict)
}

/// Return the `schema.table` list of tables in `schemas` where the role lacks
/// at least one of SELECT/INSERT/UPDATE/DELETE.
async fn verify_grant_access(
    target: &PgTarget,
    schemas: &[String],
    role: &str,
) -> OpsResult<Vec<String>> {
    let schema_array = schemas
        .iter()
        .map(|s| quote_literal(s))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT t.table_schema, t.table_name FROM information_schema.tables t \
         WHERE t.table_type = 'BASE TABLE' AND t.table_schema IN ({schema_array}) \
           AND NOT has_table_privilege({0}, quote_ident(t.table_schema) || '.' || quote_ident(t.table_name), 'SELECT,INSERT,UPDATE,DELETE') \
         ORDER BY 1,2;",
        quote_literal(role)
    );
    let out = crate::db::psql_query(target, &sql).await?;
    Ok(out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut parts = l.split('\t');
            let schema = parts.next().unwrap_or("?");
            let table = parts.next().unwrap_or("?");
            format!("{schema}.{table}")
        })
        .collect())
}

/// Decide what to do with a list of tables missing the app-role DML grants:
/// hard error when `strict`, otherwise a warning. Pure so it can be unit-tested.
fn report_missing_grants(missing: &[String], role: &str, strict: bool) -> OpsResult<()> {
    if missing.is_empty() {
        output::ok(format!("{role} has DML on all domain tables"));
        return Ok(());
    }
    let msg = format!(
        "{role} is missing SELECT/INSERT/UPDATE/DELETE on {} table(s): {}",
        missing.len(),
        missing.join(", ")
    );
    if strict {
        Err(OpsError::Command(format!(
            "{msg} (strict grant verification)"
        )))
    } else {
        output::warn(format!(
            "{msg} — the API will fail with permission denied when connected as {role}"
        ));
        Ok(())
    }
}

/// Quote an identifier for interpolation into DDL (`GRANT ... ON SCHEMA ...`).
fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Quote a string literal for interpolation into SQL (`WHERE rolname = ...`).
fn quote_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

async fn apply_phases(phases: &MigrationPhases, target: &PgTarget) -> OpsResult<()> {
    // Phase 0: CREATE SCHEMA upfront so cross-schema references resolve.
    let schema_sql = phases
        .schemas
        .iter()
        .map(|s| format!("CREATE SCHEMA IF NOT EXISTS {s};"))
        .collect::<Vec<String>>()
        .join("\n");
    crate::db::psql_exec(target, &schema_sql)
        .await
        .map_err(|e| OpsError::Command(format!("phase 0 (CREATE SCHEMA) failed: {e}")))?;
    output::ok("All schemas created");

    // Phase 1: CREATE TABLE + codelists, without FK lines. Errors are
    // collected and warned — the FK phase may still succeed.
    let mut errors: Vec<String> = Vec::new();
    for file in &phases.tables {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{}: read failed: {e}", file.display()));
                continue;
            }
        };
        let filtered = phase1_content(&content);
        if filtered.trim().is_empty() {
            continue;
        }
        let temp = match write_phase1_file(file, &filtered) {
            Ok(p) => p,
            Err(e) => {
                errors.push(format!("{}: temp write failed: {e}", file.display()));
                continue;
            }
        };
        if let Err(e) = crate::db::psql_exec_file_ok(target, &temp).await {
            errors.push(format!("{}: {e}", file.display()));
        }
        let _ = std::fs::remove_file(&temp);
    }
    if errors.is_empty() {
        output::ok(format!("Tables created ({} files)", phases.tables.len()));
    } else {
        output::warn(format!(
            "Table creation: {}/{} file(s) had errors (continuing to FK phase)",
            errors.len(),
            phases.tables.len()
        ));
        for e in &errors {
            output::warn(format!("  {e}"));
        }
    }

    // Phase 2: FK constraints, chunked into bounded batches so the argv
    // limit is never hit (669 migration files join to >128KB of SQL).
    if !phases.fk_lines.is_empty() {
        let chunks = chunk_sql(&phases.fk_lines, FK_CHUNK_LINES);
        let mut failed = 0usize;
        for chunk in &chunks {
            match crate::db::psql_exec(target, chunk).await {
                Ok(()) => {}
                Err(e) => {
                    failed += 1;
                    output::warn(format!(
                        "FK constraint batch failed (continuing): {}",
                        error_tail(&e.to_string(), 400)
                    ));
                }
            }
        }
        if failed == 0 {
            output::ok(format!(
                "FK constraints applied ({} chunk(s))",
                chunks.len()
            ));
        } else {
            output::warn(format!(
                "FK constraints: {failed}/{} chunk(s) failed",
                chunks.len()
            ));
        }
    }

    // Phase 3: FTS, embeddings, process-history views.
    let mut aux_count = 0usize;
    for file in &phases.aux {
        if let Err(e) = crate::db::psql_exec_file_ok(target, file).await {
            output::warn(format!("{}: {e}", file.display()));
        }
        aux_count += 1;
    }
    output::ok(format!(
        "FTS + embeddings + views applied ({aux_count} files)"
    ));

    // Phase 4: RLS + triggers.
    let mut rls_count = 0usize;
    for file in &phases.rls_triggers {
        if let Err(e) = crate::db::psql_exec_file_ok(target, file).await {
            output::warn(format!("{}: {e}", file.display()));
        }
        rls_count += 1;
    }
    output::ok(format!("RLS + triggers applied ({rls_count} files)"));

    let total = phases.tables.len() + phases.aux.len() + phases.rls_triggers.len();
    output::info(format!(
        "applied {total} migration files (tables/codelists: {}, FK lines: {}, FTS/embeddings/views: {}, RLS/triggers: {})",
        phases.tables.len(),
        phases.fk_lines.len(),
        phases.aux.len(),
        phases.rls_triggers.len(),
    ));
    if !phases.ignored.is_empty() {
        let names: Vec<String> = phases
            .ignored
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        output::warn(format!(
            "ignored {} non-SQL file(s): {}",
            phases.ignored.len(),
            names.join(", ")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_dir(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let mut f = std::fs::File::create(dir.path().join(name)).unwrap();
            f.write_all(content.as_bytes()).unwrap();
        }
        dir
    }

    #[test]
    fn classifies_files_by_suffix() {
        assert_eq!(classify_file("common_core.sql"), FileKind::Tables);
        assert_eq!(
            classify_file("recruiting_candidate_rls.sql"),
            FileKind::RlsTriggers
        );
        assert_eq!(
            classify_file("recruiting_candidate_trigger.sql"),
            FileKind::RlsTriggers
        );
        assert_eq!(
            classify_file("recruiting_candidate_event_trigger.sql"),
            FileKind::RlsTriggers
        );
        assert_eq!(classify_file("recruiting_candidate_fts.sql"), FileKind::Aux);
        assert_eq!(
            classify_file("recruiting_candidate_embedding.sql"),
            FileKind::Aux
        );
        assert_eq!(
            classify_file("recruiting_offer_process_history_view.sql"),
            FileKind::Aux
        );
        // Platform files are NOT special-cased — they apply like any other
        // migration (they are not Supabase-only; the harness targets are
        // Supabase-compatible Postgres).
        assert_eq!(classify_file("0001_basejump_install.sql"), FileKind::Tables);
        assert_eq!(classify_file("basejump_config.sql"), FileKind::Tables);
        assert_eq!(classify_file("pgmq_migration.sql"), FileKind::Tables);
        assert_eq!(classify_file("readme.txt"), FileKind::Skip);
    }

    #[test]
    fn partitions_phases_by_suffix() {
        let dir = fake_dir(&[
            (
                "a_core.sql",
                "CREATE SCHEMA IF NOT EXISTS common;\nCREATE TABLE a ();\nALTER TABLE a ADD CONSTRAINT a_fk FOREIGN KEY (b) REFERENCES c(id);\n",
            ),
            ("b_rls.sql", "ALTER POLICY ..."),
            ("c_fts.sql", "CREATE INDEX ..."),
            ("d_embedding.sql", "SELECT pg_embed(...)"),
            ("e_process_history_view.sql", "CREATE VIEW ..."),
            ("f_trigger.sql", "CREATE TRIGGER ..."),
            ("pgmq_extra.sql", "SELECT pgmq.create(...)"),
            ("notes.txt", "not sql"),
        ]);
        let phases = migration_phases(dir.path()).unwrap();
        assert_eq!(phases.tables.len(), 2, "a_core.sql + pgmq_extra.sql");
        assert_eq!(phases.aux.len(), 3);
        assert_eq!(phases.rls_triggers.len(), 2);
        assert_eq!(phases.fk_lines.len(), 1);
        assert!(phases.fk_lines[0].contains("ADD CONSTRAINT"));
        assert!(phases.tables[0].ends_with("a_core.sql"));
        assert!(phases.tables[1].ends_with("pgmq_extra.sql"));
        assert_eq!(phases.ignored.len(), 1);
        assert!(phases.ignored[0].ends_with("notes.txt"));
    }

    #[test]
    fn basejump_and_pgmq_platform_files_apply_in_tables_phase() {
        // The exact generated platform band must land in phase 1 (in numeric
        // order) and contribute its schemas + FK lines, matching what
        // `supabase db reset` produces via link_migrations_to_supabase.
        let dir = fake_dir(&[
            (
                "0000_extensions.sql",
                "CREATE EXTENSION IF NOT EXISTS pgcrypto;\n",
            ),
            (
                "0001_basejump_install.sql",
                "CREATE SCHEMA IF NOT EXISTS basejump;\nCREATE TABLE basejump.accounts (id uuid primary key);\nALTER TABLE basejump.accounts ADD CONSTRAINT fk_owner FOREIGN KEY (primary_owner_user_id) REFERENCES auth.users(id);\n",
            ),
            ("0003_pgmq_setup.sql", "CREATE EXTENSION IF NOT EXISTS pgmq;\nSELECT pgmq.create('events_demo');\n"),
            ("0004_rbac_roles.sql", "ALTER TYPE basejump.account_role ADD VALUE 'manager';\n"),
            ("0005_platform_schema.sql", "CREATE SCHEMA IF NOT EXISTS platform;\nGRANT USAGE ON SCHEMA pgmq TO app_user;\n"),
        ]);
        let phases = migration_phases(dir.path()).unwrap();
        assert_eq!(phases.tables.len(), 5);
        assert!(phases
            .tables
            .iter()
            .all(|p| p.extension().is_some_and(|e| e == "sql")));
        assert!(phases.schemas.iter().any(|s| s == "basejump"));
        assert!(phases.schemas.iter().any(|s| s == "platform"));
        assert!(
            phases.fk_lines.iter().any(|l| l.contains("fk_owner")),
            "basejump FK lines must be deferred to phase 2, got {:?}",
            phases.fk_lines
        );
    }

    #[test]
    fn schemas_are_deduped_sorted_and_include_defaults() {
        let dir = fake_dir(&[
            (
                "common_core.sql",
                "CREATE SCHEMA IF NOT EXISTS common;\nCREATE SCHEMA IF NOT EXISTS recruiting;\n",
            ),
            (
                "recruiting_jobs.sql",
                "CREATE SCHEMA IF NOT EXISTS recruiting;\nCREATE TABLE ...;\n",
            ),
        ]);
        let phases = migration_phases(dir.path()).unwrap();
        assert_eq!(
            phases.schemas,
            vec![
                "api_keys_private".to_string(),
                "common".to_string(),
                "platform".to_string(),
                "recruiting".to_string(),
            ]
        );
    }

    #[test]
    fn phase1_content_strips_fk_lines() {
        let content = "CREATE TABLE x ();\nALTER TABLE x ADD CONSTRAINT fk FOREIGN KEY (a) REFERENCES y(id);\nINSERT INTO codelist ...;\n";
        let filtered = phase1_content(content);
        assert!(filtered.contains("CREATE TABLE x"));
        assert!(filtered.contains("INSERT INTO codelist"));
        assert!(!filtered.contains("ADD CONSTRAINT"));
    }

    #[test]
    fn is_fk_line_matches_only_fk_constraint_additions() {
        assert!(is_fk_line(
            "ALTER TABLE a ADD CONSTRAINT a_fk FOREIGN KEY (b) REFERENCES c(id);"
        ));
        assert!(is_fk_line(
            "  ALTER TABLE a ADD CONSTRAINT a_fk FOREIGN KEY (b) REFERENCES c(id);"
        ));
        assert!(!is_fk_line("ALTER TABLE a DROP CONSTRAINT a_fk;"));
        assert!(!is_fk_line("ALTER TABLE a ALTER COLUMN b SET NOT NULL;"));
        assert!(!is_fk_line("ALTER TABLE a ADD COLUMN b uuid;"));
        // Non-FK constraint additions stay in phase 1.
        assert!(!is_fk_line(
            "ALTER TABLE a ADD CONSTRAINT a_ck CHECK (x > 0);"
        ));
        assert!(!is_fk_line("ALTER TABLE a ADD CONSTRAINT a_uq UNIQUE (b);"));
        assert!(!is_fk_line(
            "ALTER TABLE a ADD CONSTRAINT a_pk PRIMARY KEY (id);"
        ));
    }

    #[test]
    fn chunk_sql_splits_into_bounded_batches() {
        let lines: Vec<String> = (0..5)
            .map(|i| {
                format!("ALTER TABLE x{i} ADD CONSTRAINT c{i} FOREIGN KEY (b) REFERENCES y(id);")
            })
            .collect();
        let chunks = chunk_sql(&lines, 2);
        assert_eq!(chunks.len(), 3);
        assert_eq!(
            chunks[0],
            "ALTER TABLE x0 ADD CONSTRAINT c0 FOREIGN KEY (b) REFERENCES y(id);\nALTER TABLE x1 ADD CONSTRAINT c1 FOREIGN KEY (b) REFERENCES y(id);"
        );
        assert_eq!(chunks[1].lines().count(), 2);
        assert_eq!(chunks[2].lines().count(), 1);
        for chunk in &chunks {
            assert!(chunk.lines().count() <= 2);
        }
    }

    #[test]
    fn chunk_sql_fits_small_input_in_one_chunk() {
        let lines: Vec<String> = vec!["ALTER TABLE a ADD CONSTRAINT c1;".to_string()];
        let chunks = chunk_sql(&lines, 200);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "ALTER TABLE a ADD CONSTRAINT c1;");
    }

    #[test]
    fn chunk_sql_empty_input_yields_empty() {
        assert!(chunk_sql(&[], 200).is_empty());
    }

    #[test]
    fn error_tail_truncates_to_max_chars() {
        let long = "a".repeat(1000);
        let t = error_tail(&long, 100);
        assert!(t.contains("[truncated 900 chars]"), "{t}");
        assert!(t.ends_with(&"a".repeat(100)));
        assert_eq!(error_tail("short", 100), "short");
    }

    #[test]
    fn extract_create_schemas_handles_variants() {
        let content = "CREATE SCHEMA IF NOT EXISTS recruiting;\n  CREATE SCHEMA IF NOT EXISTS common;\nCREATE SCHEMA IF NOT EXISTS screening ;\nCREATE SCHEMA plain;\nCREATE SCHEMA IF NOT EXISTS \"quoted schema\";\n-- CREATE SCHEMA IF NOT EXISTS commented;\nnot a schema\n";
        let mut schemas = extract_create_schemas(content);
        schemas.sort();
        assert_eq!(
            schemas,
            vec![
                "common",
                "plain",
                "quoted schema",
                "recruiting",
                "screening"
            ]
        );
    }

    #[test]
    fn extract_create_schemas_ignores_basejump_style_tail() {
        // Statements after the schema name (AUTHORIZATION, etc.) must not
        // corrupt the extracted name.
        let schemas = extract_create_schemas("CREATE SCHEMA IF NOT EXISTS basejump;");
        assert_eq!(schemas, vec!["basejump"]);
    }

    #[test]
    fn write_phase1_file_writes_filtered_content() {
        let dir = tempfile::tempdir().unwrap();
        let orig = dir.path().join("x_core.sql");
        std::fs::write(&orig, "CREATE TABLE x ();\nALTER TABLE x ADD CONSTRAINT fk FOREIGN KEY (a) REFERENCES y(id);\n").unwrap();
        let content = phase1_content(&std::fs::read_to_string(&orig).unwrap());
        let temp = write_phase1_file(&orig, &content).unwrap();
        let written = std::fs::read_to_string(&temp).unwrap();
        assert!(!written.contains("ADD CONSTRAINT"));
        assert!(written.contains("CREATE TABLE x"));
        std::fs::remove_file(&temp).unwrap();
    }

    #[test]
    fn link_migrations_creates_symlinks_and_replaces_stale() {
        let mig = tempfile::tempdir().unwrap();
        std::fs::write(mig.path().join("a.sql"), "CREATE TABLE a ();").unwrap();
        std::fs::write(mig.path().join("b.sql"), "CREATE TABLE b ();").unwrap();
        std::fs::write(mig.path().join("readme.txt"), "not sql").unwrap();
        let sup = tempfile::tempdir().unwrap();
        let sup_mig = sup.path().join("supabase").join("migrations");
        std::fs::create_dir_all(&sup_mig).unwrap();
        std::os::unix::fs::symlink("/tmp/nonexistent-old.sql", sup_mig.join("stale.sql")).unwrap();

        let count = link_migrations_to_supabase(mig.path(), &sup_mig).unwrap();
        assert_eq!(count, 2);
        assert!(sup_mig.join("a.sql").is_symlink());
        assert!(sup_mig.join("b.sql").is_symlink());
        assert!(!sup_mig.join("stale.sql").exists());
        assert!(!sup_mig.join("readme.txt").exists());
    }

    #[test]
    fn remove_supabase_links_cleans_symlinks_only() {
        let sup = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/tmp/x.sql", sup.path().join("a.sql")).unwrap();
        std::fs::write(sup.path().join("real.sql"), "CREATE TABLE x ();").unwrap();
        remove_supabase_links(sup.path()).unwrap();
        assert!(!sup.path().join("a.sql").exists());
        assert!(sup.path().join("real.sql").exists());
    }

    #[test]
    fn remove_supabase_links_tolerates_missing_dir() {
        let missing = tempfile::tempdir().unwrap().path().join("nope");
        remove_supabase_links(&missing).unwrap();
    }

    #[test]
    fn grant_schemas_filters_system_and_infra_schemas() {
        let phases = MigrationPhases {
            schemas: vec![
                "platform".to_string(),
                "api_keys_private".to_string(),
                "basejump".to_string(),
                "common".to_string(),
                "recruiting".to_string(),
                "public".to_string(),
                "auth".to_string(),
                "pg_temp_3".to_string(),
                "pgmq".to_string(),
                "platform_integrations".to_string(),
                "platform".to_string(), // duplicate
            ],
            ..Default::default()
        };
        assert_eq!(
            grant_schemas(&phases),
            vec![
                "common".to_string(),
                "platform".to_string(),
                "platform_integrations".to_string(),
                "recruiting".to_string(),
            ]
        );
    }

    #[test]
    fn report_missing_grants_warns_unless_strict() {
        let missing = vec![
            "common.widget".to_string(),
            "platform.webhook_endpoint".to_string(),
        ];
        // Non-strict: warns and continues.
        assert!(report_missing_grants(&missing, "app_user", false).is_ok());
        // Strict: hard error.
        let err = report_missing_grants(&missing, "app_user", true).unwrap_err();
        let text = format!("{err}");
        assert!(text.contains("common.widget"), "{text}");
        assert!(text.contains("platform.webhook_endpoint"), "{text}");
        assert!(text.contains("strict grant verification"), "{text}");
        // Nothing missing is always Ok.
        assert!(report_missing_grants(&[], "app_user", true).is_ok());
    }

    #[test]
    fn quote_helpers_escape_identifiers_and_literals() {
        assert_eq!(quote_ident("app_user"), "\"app_user\"");
        assert_eq!(quote_ident("weird\"name"), "\"weird\"\"name\"");
        assert_eq!(quote_literal("app_user"), "'app_user'");
        assert_eq!(quote_literal("o'brien"), "'o''brien'");
    }
}
