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
pub const MIGRATION_LOCK_KEY: i64 = 73287328;

/// The ONE migration tracking table (issue #756), shared with every
/// generated app's boot runner (CLI-less `src/boot_migrate.rs` and the
/// admin-CLI `src/migration.rs`). `name` is the full migration filename and
/// `checksum` the FNV-1a 64-bit hex digest of the ORIGINAL file bytes.
const TRACKING_TABLE: &str = "platform._migrations";

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
#[derive(Debug, Default, Clone)]
struct MigrationPhases {
    /// Phase 0: sorted, deduplicated schema names ("platform", "api_keys_private"
    /// plus every `CREATE SCHEMA [IF NOT EXISTS] <x>` found in the files).
    schemas: Vec<String>,
    /// Phase 1: table/codelist files (FK lines are stripped per-file at apply
    /// time and deferred to phase 2).
    tables: Vec<PathBuf>,
    /// Phase 2: FK constraint lines GROUPED BY SOURCE FILE, in file order —
    /// the tracking-skip filter (issue #427) drops a skipped file's group so
    /// its constraints are not re-applied.
    fk_lines_by_file: Vec<(String, Vec<String>)>,
    /// Phase 3: `*_fts.sql`, `*_embedding.sql`, `*_process_history_view.sql`.
    aux: Vec<PathBuf>,
    /// Phase 4: `*_rls.sql`, `*_trigger.sql`.
    rls_triggers: Vec<PathBuf>,
    /// Non-SQL files in the directory, tracked for reporting only.
    ignored: Vec<PathBuf>,
}

impl MigrationPhases {
    /// Every FK constraint line, flattened in file order (the chunking input).
    fn all_fk_lines(&self) -> Vec<String> {
        self.fk_lines_by_file
            .iter()
            .flat_map(|(_, lines)| lines.iter().cloned())
            .collect()
    }

    /// Total migration files across the applied phases (the skip report's
    /// denominator).
    fn total_files(&self) -> usize {
        self.tables.len() + self.aux.len() + self.rls_triggers.len()
    }
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
    let mut fk_lines_by_file: Vec<(String, Vec<String>)> = Vec::new();
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
        let fk_here: Vec<String> = content
            .lines()
            .filter(|line| is_fk_line(line))
            .map(|line| line.to_string())
            .collect();
        if !fk_here.is_empty() {
            fk_lines_by_file.push((name.clone(), fk_here));
        }
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
        fk_lines_by_file,
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
/// `psql_exec_file_strict` (psql takes a path, and the FK lines must be
/// removed from the input first). Returns the temp path.
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

/// Policy for a migration run (issue #427). Defaults are STRICT: a
/// migration that errors fails the run with a per-file summary — silent
/// partial application hid a wiped basejump schema and a two-month broken
/// `0002_api_key_management.sql`. The lenient opt-out is EXPLICIT and stays
/// loud (per-file warnings + a run-summary banner + a dedicated results-JSON
/// field), never silent.
#[derive(Debug, Clone, Default)]
pub struct MigrateOptions {
    /// Post-migration grant sweep for the generated API role.
    pub grant: GrantOptions,
    /// `--migrate-lenient` / `[database.api] migrate_lenient`: per-file
    /// failures downgrade from fatal to loud warnings.
    pub lenient: bool,
    /// `--replay-all`: bypass the `platform._migrations` tracking and apply
    /// every file, checksum-matching ones included. For clean databases,
    /// idempotent sets and drift repair — over an existing band it fails
    /// loudly on non-idempotent files, which is correct.
    pub replay_all: bool,
}

impl MigrateOptions {
    /// Strict options carrying `grant` (the legacy
    /// [`run_api_migrations_with_options`] shape).
    pub fn strict(grant: GrantOptions) -> Self {
        Self {
            grant,
            lenient: false,
            replay_all: false,
        }
    }
}

/// What a migration run did. `applied` counts files actually executed this
/// run; `skipped` counts files whose recorded checksum matched (the
/// consecutive-run speed win — the second pass applies ~0 files, issue #427
/// acceptance); `failed` counts files/chunks whose errors were tolerated
/// under lenient mode (strict mode turns any failure into a hard error and
/// never returns a report).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MigrateReport {
    pub applied: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// Apply generated SQL migrations to a plain-Postgres target in dependency
/// order, then grant the API role DML on domain tables and verify the grants
/// (default [`GrantOptions`], STRICT failure semantics — issue #427).
/// Generated migrations have cross-schema deps, hence phases:
///
/// * Phase 0: `CREATE SCHEMA` — "platform" + "api_keys_private" plus every
///   `CREATE SCHEMA [IF NOT EXISTS] <x>` found in the migration files (dedup,
///   sort), applied via `psql_exec`.
/// * Phase 1: `CREATE TABLE` + codelists — every UNTRACKED table file with
///   its FK constraint lines removed.
/// * Phase 2: FK constraints — all `ALTER TABLE ... ADD CONSTRAINT ... FOREIGN
///   KEY` lines from the apply set, chunked into bounded batches.
/// * Phase 3: aux files (`*_fts.sql`, `*_embedding.sql`,
///   `*_process_history_view.sql`).
/// * Phase 4: `*_rls.sql` + `*_trigger.sql`.
///
/// Tracking (issues #756 + #427): files whose `platform._migrations` checksum
/// matches are SKIPPED (a consecutive run applies ~0 files); a checksum
/// mismatch re-applies. `--replay-all` bypasses the skip. Failure semantics
/// are STRICT by default: any per-file (phase 1/3/4) or per-chunk (phase 2)
/// error FAILS the run with a per-file summary; [`MigrateOptions::lenient`]
/// downgrades that to loud warnings. The band records in the tracking table
/// only when it applied without errors — a partial band records nothing
/// (the #756 contract).
///
/// An advisory lock (key 73287328) is acquired before phase 0 and released
/// afterwards, on both success and failure paths — the same key the
/// generated boot runner takes, so the two planes cannot interleave.
pub async fn run_api_migrations(
    migration_dir: &Path,
    target: &PgTarget,
) -> OpsResult<MigrateReport> {
    run_migrations(migration_dir, target, &MigrateOptions::default()).await
}

/// [`run_api_migrations`] with explicit [`GrantOptions`] (strict failure
/// semantics — the pre-#427 signature).
pub async fn run_api_migrations_with_options(
    migration_dir: &Path,
    target: &PgTarget,
    grant: &GrantOptions,
) -> OpsResult<MigrateReport> {
    run_migrations(
        migration_dir,
        target,
        &MigrateOptions::strict(grant.clone()),
    )
    .await
}

/// [`run_api_migrations`] with full [`MigrateOptions`] — the entry point the
/// suites call with their CLI/manifest plumbing.
pub async fn run_migrations(
    migration_dir: &Path,
    target: &PgTarget,
    options: &MigrateOptions,
) -> OpsResult<MigrateReport> {
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

    // Tracking-aware skip (issue #427): files whose recorded checksum
    // matches are not re-applied, and their FK lines are excluded from
    // phase 2. replay_all bypasses the read entirely.
    let tracked = read_tracking(target).await?;
    let (apply_view, skipped) = filter_skipped(&phases, &tracked, options.replay_all)?;

    let result = match apply_phases(&apply_view, target).await {
        Err(e) => Err(e),
        // A clean or lenient-tolerated band still gets the grant sweep (it
        // is idempotent and the pre-#427 behavior). The strict/lenient
        // decision happens on the COLLECTED per-file errors.
        Ok(errors) => {
            let grant_result = async {
                output::section("Granting API role access");
                grant_app_user_access(&apply_view, target, &options.grant).await
            };
            match grant_result.await {
                Ok(()) if errors.is_empty() => {
                    record_tracking(&apply_view, target)
                        .await
                        .map(|()| MigrateReport {
                            applied: apply_view.total_files(),
                            skipped,
                            failed: 0,
                        })
                }
                Ok(()) if options.lenient => {
                    for e in &errors {
                        output::warn(format!("  {e}"));
                    }
                    output::warn(format!(
                        "MIGRATE LENIENT: {} migration error(s) tolerated — the band is \
                         PARTIAL and was NOT recorded in platform._migrations; the run \
                         continues only because --migrate-lenient / manifest \
                         migrate_lenient opted out of the strict default",
                        errors.len()
                    ));
                    Ok(MigrateReport {
                        applied: apply_view.total_files(),
                        skipped,
                        failed: errors.len(),
                    })
                }
                Ok(()) => Err(OpsError::TestFailure(migration_error_summary(&errors))),
                Err(e) => Err(e),
            }
        }
    };

    match crate::db::advisory_unlock(target, MIGRATION_LOCK_KEY).await {
        Ok(()) => output::ok("Migration advisory lock released"),
        Err(e) => output::warn(format!("advisory unlock failed: {e}")),
    }
    result
}

/// The per-file summary a failing migrate stage carries (into the error, the
/// suite output and the results JSON). Pure so it is unit-testable.
fn migration_error_summary(errors: &[String]) -> String {
    let mut out = format!(
        "{} migration file(s)/chunk(s) failed — the database is in a PARTIAL \
         state and nothing was recorded:",
        errors.len()
    );
    for e in errors {
        out.push_str("\n  - ");
        out.push_str(e);
    }
    out
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
            // psql -t -A joins columns with `|`; the old '\t' split turned
            // every missing-grant row into one garbled "schema|table"
            // diagnostic.
            let mut parts = l.split('|');
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

/// FNV-1a 64-bit hex (16 lowercase hex chars) over the ORIGINAL file bytes —
/// the checksum every migration plane records (issue #756). Implemented
/// inline so the ops harness needs no extra dependency, matching the
/// generated apps' own helper.
fn fnv1a_64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// FNV-1a 64-bit hex over a file's ORIGINAL bytes — the checksum the
/// tracking table records and the skip filter compares (issue #756/#427).
fn file_checksum(path: &Path) -> OpsResult<String> {
    let bytes = std::fs::read(path)
        .map_err(|e| OpsError::Command(format!("read {} for checksum: {e}", path.display())))?;
    Ok(fnv1a_64_hex(&bytes))
}

/// Read `platform._migrations` as a name → checksum map. An absent table (a
/// fresh database, or after [`clear_tracking`]) is an empty map — first
/// application applies everything.
async fn read_tracking(target: &PgTarget) -> OpsResult<std::collections::HashMap<String, String>> {
    let exists = crate::db::psql_query(
        target,
        "SELECT to_regclass('platform._migrations') IS NOT NULL;",
    )
    .await
    .map(|v| v.trim().ends_with('t'))
    .unwrap_or(false);
    if !exists {
        return Ok(std::collections::HashMap::new());
    }
    let rows =
        crate::db::psql_query(target, "SELECT name, checksum FROM platform._migrations;").await?;
    Ok(rows
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            // psql -t -A joins columns with `|` (the separator every
            // psql_query caller parses; a '\t' split silently yields an
            // EMPTY map and the skip would never fire). Migration filenames
            // are generated snake_case — they never carry `|`.
            let mut parts = l.split('|');
            let name = parts.next()?;
            let checksum = parts.next()?;
            Some((name.to_string(), checksum.to_string()))
        })
        .collect())
}

/// Clear the tracking table (issue #427 reset semantics): after a reset
/// wiped the domain schemas, stale rows would make the next run SKIP every
/// file against a database that no longer has them. The table is dropped
/// (the boot runner's adoption contract treats an absent table as
/// adoptable; [`record_tracking`] recreates it). Schema-absent is tolerated.
pub async fn clear_tracking(target: &PgTarget) -> OpsResult<()> {
    crate::db::psql_exec(
        target,
        "CREATE SCHEMA IF NOT EXISTS platform;\n\
         DROP TABLE IF EXISTS platform._migrations;",
    )
    .await
}

/// Split the phase plan into the files to APPLY this run (dropping every
/// file whose recorded checksum matches — the consecutive-run ~0-files
/// acceptance) and the skip count. `replay_all` bypasses the tracking read
/// entirely. A checksum MISMATCH re-applies the file and the post-band
/// upsert refreshes the recorded value. A checksum read error is a hard
/// error (fail closed — never apply blindly on a broken read). Pure over
/// `tracked` so it is unit-testable.
fn filter_skipped(
    phases: &MigrationPhases,
    tracked: &std::collections::HashMap<String, String>,
    replay_all: bool,
) -> OpsResult<(MigrationPhases, usize)> {
    if replay_all {
        return Ok((phases.clone(), 0));
    }
    let mut keep = Vec::with_capacity(phases.total_files());
    for path in phases
        .tables
        .iter()
        .chain(phases.aux.iter())
        .chain(phases.rls_triggers.iter())
    {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let keep_this = match tracked.get(&name) {
            Some(recorded) => file_checksum(path)? != *recorded,
            None => true,
        };
        keep.push(keep_this);
    }
    let mut it = keep.into_iter();
    let mut kept = |list: &[PathBuf]| -> Vec<PathBuf> {
        list.iter()
            .zip(&mut it)
            .filter(|(_, keep)| *keep)
            .map(|(p, _)| p.clone())
            .collect::<Vec<_>>()
    };
    let tables = kept(&phases.tables);
    let aux = kept(&phases.aux);
    let rls_triggers = kept(&phases.rls_triggers);
    let skipped = phases.total_files() - (tables.len() + aux.len() + rls_triggers.len());
    // A skipped file's FK lines must not re-apply either: keep only the
    // groups whose source file is still in the apply set.
    let kept_names: std::collections::HashSet<String> = tables
        .iter()
        .chain(aux.iter())
        .chain(rls_triggers.iter())
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    let fk_lines_by_file = phases
        .fk_lines_by_file
        .iter()
        .filter(|(name, _)| kept_names.contains(name))
        .cloned()
        .collect::<Vec<_>>();
    Ok((
        MigrationPhases {
            schemas: phases.schemas.clone(),
            tables,
            fk_lines_by_file,
            aux,
            rls_triggers,
            ignored: phases.ignored.clone(),
        },
        skipped,
    ))
}

/// Record the applied band in `platform._migrations` (issue #756). Called
/// only once every phase — and the app-role grant sweep — has succeeded.
/// Re-recording updates the checksum (the band re-applied those bytes) but
/// keeps the original `applied_at`.
async fn record_tracking(phases: &MigrationPhases, target: &PgTarget) -> OpsResult<()> {
    let files = phases
        .tables
        .iter()
        .chain(phases.aux.iter())
        .chain(phases.rls_triggers.iter());

    let mut values: Vec<String> = Vec::new();
    for file in files {
        let bytes = std::fs::read(file).map_err(|e| {
            OpsError::Command(format!(
                "read {} for migration tracking: {e}",
                file.display()
            ))
        })?;
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        values.push(format!(
            "({}, {})",
            quote_literal(&name),
            quote_literal(&fnv1a_64_hex(&bytes))
        ));
    }

    crate::db::psql_exec(
        target,
        "CREATE SCHEMA IF NOT EXISTS platform;\n\
         CREATE TABLE IF NOT EXISTS platform._migrations (\n\
             name TEXT PRIMARY KEY,\n\
             checksum TEXT NOT NULL,\n\
             applied_at TIMESTAMPTZ NOT NULL DEFAULT now()\n\
         );",
    )
    .await?;

    if values.is_empty() {
        output::info("no migration files to record");
        return Ok(());
    }
    let sql = format!(
        "INSERT INTO {TRACKING_TABLE} (name, checksum) VALUES {} \n\
         ON CONFLICT (name) DO UPDATE SET checksum = EXCLUDED.checksum;",
        values.join(", ")
    );
    crate::db::psql_exec(target, &sql).await?;
    output::ok(format!(
        "Recorded {} migration(s) in {TRACKING_TABLE}",
        values.len()
    ));
    Ok(())
}

/// Apply the phase plan, collecting every per-file (phases 1/3/4) and
/// per-chunk (phase 2) error — the caller decides fatal vs lenient-tolerated
/// (issue #427). Warnings print inline as they happen, so the output is
/// readable even before the summary. `Ok(errors)` with an empty vec = clean.
async fn apply_phases(phases: &MigrationPhases, target: &PgTarget) -> OpsResult<Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

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

    // Phase 1: CREATE TABLE + codelists, without FK lines.
    for file in &phases.tables {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                let line = format!("{}: read failed: {e}", file.display());
                output::warn(format!("  {line}"));
                errors.push(line);
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
                let line = format!("{}: temp write failed: {e}", file.display());
                output::warn(format!("  {line}"));
                errors.push(line);
                continue;
            }
        };
        if let Err(e) = crate::db::psql_exec_file_strict(target, &temp).await {
            let line = format!("{}: {e}", file.display());
            output::warn(format!("  {line}"));
            errors.push(line);
        }
        let _ = std::fs::remove_file(&temp);
    }
    if errors.is_empty() {
        output::ok(format!("Tables created ({} files)", phases.tables.len()));
    } else {
        output::warn(format!(
            "Table creation: {}/{} file(s) had errors",
            errors.len(),
            phases.tables.len()
        ));
    }

    // Phase 2: FK constraints, chunked into bounded batches so the argv
    // limit is never hit (669 migration files join to >128KB of SQL).
    let fk_lines = phases.all_fk_lines();
    if !fk_lines.is_empty() {
        let chunks = chunk_sql(&fk_lines, FK_CHUNK_LINES);
        let before = errors.len();
        for (index, chunk) in chunks.iter().enumerate() {
            if let Err(e) = crate::db::psql_exec(target, chunk).await {
                let line = format!(
                    "FK constraint batch {}/{}: {}",
                    index + 1,
                    chunks.len(),
                    error_tail(&e.to_string(), 400)
                );
                output::warn(format!("  {line}"));
                errors.push(line);
            }
        }
        if errors.len() == before {
            output::ok(format!(
                "FK constraints applied ({} chunk(s))",
                chunks.len()
            ));
        }
    }

    // Phase 3: FTS, embeddings, process-history views.
    let mut aux_errors = 0usize;
    for file in &phases.aux {
        if let Err(e) = crate::db::psql_exec_file_strict(target, file).await {
            let line = format!("{}: {e}", file.display());
            output::warn(format!("  {line}"));
            errors.push(line);
            aux_errors += 1;
        }
    }
    if aux_errors == 0 {
        output::ok(format!(
            "FTS + embeddings + views applied ({} files)",
            phases.aux.len()
        ));
    }

    // Phase 4: RLS + triggers.
    let mut rls_errors = 0usize;
    for file in &phases.rls_triggers {
        if let Err(e) = crate::db::psql_exec_file_strict(target, file).await {
            let line = format!("{}: {e}", file.display());
            output::warn(format!("  {line}"));
            errors.push(line);
            rls_errors += 1;
        }
    }
    if rls_errors == 0 {
        output::ok(format!(
            "RLS + triggers applied ({} files)",
            phases.rls_triggers.len()
        ));
    }

    output::info(format!(
        "applied {} migration files this run (tables/codelists: {}, FK lines: {}, FTS/embeddings/views: {}, RLS/triggers: {})",
        phases.total_files(),
        phases.tables.len(),
        fk_lines.len(),
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
    Ok(errors)
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
        let fk_lines = phases.all_fk_lines();
        assert_eq!(fk_lines.len(), 1);
        assert!(fk_lines[0].contains("ADD CONSTRAINT"));
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
            (
                "0003_pgmq_setup.sql",
                "CREATE EXTENSION IF NOT EXISTS pgmq;\nSELECT pgmq.create('events_demo');\n",
            ),
            (
                "0004_rbac_roles.sql",
                "ALTER TYPE basejump.account_role ADD VALUE 'manager';\n",
            ),
            (
                "0005_platform_schema.sql",
                "CREATE SCHEMA IF NOT EXISTS platform;\nGRANT USAGE ON SCHEMA pgmq TO app_user;\n",
            ),
        ]);
        let phases = migration_phases(dir.path()).unwrap();
        assert_eq!(phases.tables.len(), 5);
        assert!(
            phases
                .tables
                .iter()
                .all(|p| p.extension().is_some_and(|e| e == "sql"))
        );
        assert!(phases.schemas.iter().any(|s| s == "basejump"));
        assert!(phases.schemas.iter().any(|s| s == "platform"));
        let fk_lines = phases.all_fk_lines();
        assert!(
            fk_lines.iter().any(|l| l.contains("fk_owner")),
            "basejump FK lines must be deferred to phase 2, got {fk_lines:?}"
        );
        // FK lines are attributed per source file — the tracking-skip filter
        // drops a skipped file's group (issue #427).
        assert_eq!(phases.fk_lines_by_file.len(), 1);
        assert_eq!(phases.fk_lines_by_file[0].0, "0001_basejump_install.sql");
    }

    /// The tracking-skip filter (issue #427): checksum-matching files drop
    /// out of the apply view (and their FK groups with them), mismatches
    /// stay, untracked files stay, and replay_all bypasses everything.
    #[test]
    fn filter_skipped_drops_matching_and_keeps_mismatched() {
        let dir = fake_dir(&[
            (
                "001_same.sql",
                "CREATE TABLE same ();\nALTER TABLE same ADD CONSTRAINT fk1 FOREIGN KEY (a) REFERENCES b(id);\n",
            ),
            (
                "002_changed.sql",
                "CREATE TABLE changed ();\nALTER TABLE changed ADD CONSTRAINT fk2 FOREIGN KEY (a) REFERENCES b(id);\n",
            ),
            ("003_new.sql", "CREATE TABLE new ();"),
        ]);
        let phases = migration_phases(dir.path()).unwrap();
        assert_eq!(phases.total_files(), 3);

        let mut tracked = std::collections::HashMap::new();
        tracked.insert(
            "001_same.sql".to_string(),
            file_checksum(&dir.path().join("001_same.sql")).unwrap(),
        );
        tracked.insert(
            "002_changed.sql".to_string(),
            "deadbeefdeadbeef".to_string(),
        );

        let (view, skipped) = filter_skipped(&phases, &tracked, false).unwrap();
        assert_eq!(skipped, 1, "only the checksum match skips");
        let names: Vec<String> = view
            .tables
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(names, vec!["002_changed.sql", "003_new.sql"]);
        // The skipped file's FK group must not survive into phase 2.
        assert_eq!(view.fk_lines_by_file.len(), 1);
        assert_eq!(view.fk_lines_by_file[0].0, "002_changed.sql");

        // replay_all bypasses the skip entirely.
        let (all, skipped) = filter_skipped(&phases, &tracked, true).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(all.total_files(), 3);
        assert_eq!(all.all_fk_lines().len(), 2);
    }

    #[test]
    fn migration_error_summary_names_every_file() {
        let summary = migration_error_summary(&["001_a.sql: syntax error".into()]);
        assert!(summary.contains("1 migration file"), "{summary}");
        assert!(summary.contains("001_a.sql"), "{summary}");
        assert!(summary.contains("PARTIAL"), "{summary}");
        let two = migration_error_summary(&[
            "001_a.sql: syntax error".into(),
            "002_b.sql: relation missing".into(),
        ]);
        assert!(two.contains("002_b.sql"), "{two}");
    }

    #[test]
    fn migrate_options_default_to_strict_and_unless_replayed() {
        let opts = MigrateOptions::default();
        assert!(!opts.lenient, "fail-on-error is the default (issue #427)");
        assert!(!opts.replay_all);
        let strict = MigrateOptions::strict(GrantOptions::default());
        assert!(!strict.lenient && !strict.replay_all);
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

    /// The tracking checksum (issue #756) must be the SAME FNV-1a 64-bit hex
    /// the generated boot runner computes — the known-answer vectors pin the
    /// constants and the multiply/ xor order.
    #[test]
    fn tracking_checksum_is_fnv1a_64_hex() {
        assert_eq!(fnv1a_64_hex(b""), "cbf29ce484222325");
        assert_eq!(fnv1a_64_hex(b"a"), "af63dc4c8601ec8c");
        assert_eq!(fnv1a_64_hex(b"hello"), "a430d84680aabd0b");
        assert_eq!(fnv1a_64_hex(b"hello").len(), 16);
        assert_ne!(fnv1a_64_hex(b"hello"), fnv1a_64_hex(b"hello "));
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
