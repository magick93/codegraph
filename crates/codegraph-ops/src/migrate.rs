//! Phased SQL migration application for plain-Postgres targets.
//!
//! NOTE: depends on `crate::db::{psql_exec, psql_exec_file_ok, advisory_lock,
//! advisory_unlock, missing_extensions}` — implemented by a parallel agent
//! (db.rs). Until that lands, `cargo check -p codegraph-ops` will fail on the
//! missing `crate::db` items; that is expected mid-flight.
//!
//! Port of `hr-platform/lib/migrate.sh`, genericised: no hr-specific schema
//! list — schemas are derived from the migration files themselves.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{OpsError, OpsResult};
use crate::output;
use crate::pg::PgTarget;

/// Advisory lock key shared by all migration runs against a target, so
/// concurrent invocations do not interleave phases.
const MIGRATION_LOCK_KEY: i64 = 73287328;

/// Which phase a migration file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    /// Normal table/codelist file — applied in phase 1.
    Tables,
    /// FTS / embeddings / process-history views — phase 3.
    Aux,
    /// RLS + triggers — phase 4.
    RlsTriggers,
    /// Supabase-only or otherwise skipped entirely.
    Skip,
}

/// The full phase plan for a migration directory.
#[derive(Debug, Default)]
struct MigrationPhases {
    /// Phase 0: sorted, deduplicated schema names ("platform", "api_keys_private"
    /// plus every `CREATE SCHEMA IF NOT EXISTS <x>` found in the files).
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
}

/// Classify a migration basename into its application phase.
fn classify_file(name: &str) -> FileKind {
    if name.contains("basejump") || name.contains("pgmq") {
        FileKind::Skip
    } else if name.ends_with("_rls.sql") || name.ends_with("_trigger.sql") {
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
fn is_fk_line(line: &str) -> bool {
    line.starts_with("ALTER TABLE") && line.contains("ADD CONSTRAINT")
}

/// Phase-1 content for a file: the file with FK constraint lines removed.
fn phase1_content(content: &str) -> String {
    content
        .lines()
        .filter(|line| !is_fk_line(line))
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Extract schema names from `CREATE SCHEMA IF NOT EXISTS <name>` lines.
fn extract_create_schemas(content: &str) -> Vec<String> {
    const PREFIX: &str = "CREATE SCHEMA IF NOT EXISTS";
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix(PREFIX) {
                let name = rest
                    .trim()
                    .trim_end_matches(';')
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
            None
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

    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.is_file() && p.extension().map(|x| x == "sql").unwrap_or(false))
        .collect();
    files.sort();

    for path in &files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return Err(OpsError::Io(e)),
        };
        // Schemas and FK lines are gathered from ALL files, mirroring the
        // original `grep -rh ... "$migration_dir"/*.sql`.
        schemas.extend(extract_create_schemas(&content));
        fk_lines.extend(
            content
                .lines()
                .filter(|line| is_fk_line(line))
                .map(|line| line.to_string()),
        );
        match classify_file(&name) {
            FileKind::Skip => {}
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
pub fn link_migrations_to_supabase(migration_dir: &Path, supabase_mig_dir: &Path) -> OpsResult<usize> {
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

/// Apply generated SQL migrations to a plain-Postgres target in dependency
/// order. Generated migrations have cross-schema deps, hence phases:
///
/// * Phase 0: `CREATE SCHEMA` — "platform" + "api_keys_private" plus every
///   `CREATE SCHEMA IF NOT EXISTS <x>` found in the migration files (dedup,
///   sort), applied via `psql_exec`.
/// * Phase 1: `CREATE TABLE` + codelists — every table file with its FK
///   constraint lines removed, applied tolerantly (`psql_exec_file_ok`);
///   per-file errors are collected and warned, not fatal.
/// * Phase 2: FK constraints — all `ALTER TABLE ... ADD CONSTRAINT` lines,
///   joined and applied via `psql_exec`; failure is a warning.
/// * Phase 3: aux files (`*_fts.sql`, `*_embedding.sql`,
///   `*_process_history_view.sql`) via `psql_exec_file_ok`.
/// * Phase 4: `*_rls.sql` + `*_trigger.sql` via `psql_exec_file_ok`.
///
/// An advisory lock (key 73287328) is acquired before phase 0 and released
/// afterwards, on both success and failure paths.
pub async fn run_api_migrations(migration_dir: &Path, target: &PgTarget) -> OpsResult<()> {
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

    let result = apply_phases(&phases, target).await;

    match crate::db::advisory_unlock(target, MIGRATION_LOCK_KEY).await {
        Ok(()) => output::ok("Migration advisory lock released"),
        Err(e) => output::warn(format!("advisory unlock failed: {e}")),
    }
    result
}

async fn apply_phases(phases: &MigrationPhases, target: &PgTarget) -> OpsResult<()> {
    // Phase 0: CREATE SCHEMA upfront so cross-schema references resolve.
    let schema_sql = phases
        .schemas
        .iter()
        .map(|s| format!("CREATE SCHEMA IF NOT EXISTS {s};"))
        .collect::<Vec<String>>()
        .join("\n");
    crate::db::psql_exec(target, &schema_sql).await.map_err(|e| {
        OpsError::Command(format!("phase 0 (CREATE SCHEMA) failed: {e}"))
    })?;
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
            "Table creation: {} file(s) had errors (continuing to FK phase)",
            errors.len()
        ));
        for e in &errors {
            output::warn(format!("  {e}"));
        }
    }

    // Phase 2: FK constraints, joined across all files.
    if !phases.fk_lines.is_empty() {
        let fk_sql = phases.fk_lines.join("\n");
        match crate::db::psql_exec(target, &fk_sql).await {
            Ok(()) => output::ok("FK constraints applied"),
            Err(e) => output::warn(format!("FK constraints: some failed: {e}")),
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
    output::info(format!("{total} total migration files applied"));
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
        assert_eq!(classify_file("recruiting_candidate_rls.sql"), FileKind::RlsTriggers);
        assert_eq!(classify_file("recruiting_candidate_trigger.sql"), FileKind::RlsTriggers);
        assert_eq!(classify_file("recruiting_candidate_fts.sql"), FileKind::Aux);
        assert_eq!(classify_file("recruiting_candidate_embedding.sql"), FileKind::Aux);
        assert_eq!(classify_file("recruiting_offer_process_history_view.sql"), FileKind::Aux);
        assert_eq!(classify_file("basejump_config.sql"), FileKind::Skip);
        assert_eq!(classify_file("pgmq_migration.sql"), FileKind::Skip);
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
        assert_eq!(phases.tables.len(), 1);
        assert_eq!(phases.aux.len(), 3);
        assert_eq!(phases.rls_triggers.len(), 2);
        assert_eq!(phases.fk_lines.len(), 1);
        assert!(phases.fk_lines[0].contains("ADD CONSTRAINT"));
        assert!(phases.tables[0].ends_with("a_core.sql"));
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
    fn is_fk_line_matches_only_constraint_additions() {
        assert!(is_fk_line("ALTER TABLE a ADD CONSTRAINT a_fk FOREIGN KEY (b) REFERENCES c(id);"));
        assert!(!is_fk_line("ALTER TABLE a DROP CONSTRAINT a_fk;"));
        assert!(!is_fk_line("ALTER TABLE a ALTER COLUMN b SET NOT NULL;"));
        assert!(!is_fk_line("ALTER TABLE a ADD COLUMN b uuid;"));
    }

    #[test]
    fn extract_create_schemas_handles_variants() {
        let content = "CREATE SCHEMA IF NOT EXISTS recruiting;\n  CREATE SCHEMA IF NOT EXISTS common;\nCREATE SCHEMA IF NOT EXISTS screening ;\nnot a schema\n";
        let mut schemas = extract_create_schemas(content);
        schemas.sort();
        assert_eq!(schemas, vec!["common", "recruiting", "screening"]);
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
}
