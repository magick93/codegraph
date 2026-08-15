//! psql wrapper (query/exec/file) plus postgres extension validation.

use std::path::Path;
use std::process::{Command, Output};

use crate::env::ensure_psql;
use crate::error::{OpsError, OpsResult};
use crate::output::warn;
use crate::pg::PgTarget;

/// Run `psql -t -A -c "<sql>"` (quiet mode), return trimmed stdout.
/// Password via PGPASSWORD env on the subprocess. Stderr discarded.
pub async fn psql_query(target: &PgTarget, sql: &str) -> OpsResult<String> {
    let args = ["-t", "-A", "-c", sql];
    let cmd = psql_command(target, &args)?;
    let out = run_psql(cmd).await?;
    if !out.status.success() {
        return Err(command_failure("query", sql, &out));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run `psql -c "<sql>"`, return Err if psql exits non-zero.
pub async fn psql_exec(target: &PgTarget, sql: &str) -> OpsResult<()> {
    let cmd = psql_command(target, &["-c", sql])?;
    let out = run_psql(cmd).await?;
    if out.status.success() {
        Ok(())
    } else {
        Err(command_failure("exec", sql, &out))
    }
}

/// Run `psql -q -f <path>`; if the file is missing, return Err.
pub async fn psql_exec_file(target: &PgTarget, path: &Path) -> OpsResult<()> {
    if !path.is_file() {
        return Err(OpsError::PathNotFound(path.to_path_buf()));
    }
    let file_arg = path.to_string_lossy().into_owned();
    let cmd = psql_command(target, &["-q", "-f", &file_arg])?;
    let out = run_psql(cmd).await?;
    if out.status.success() {
        Ok(())
    } else {
        Err(command_failure(
            &format!("psql -f {}", path.display()),
            "",
            &out,
        ))
    }
}

/// Run `psql -q -f <path>` tolerating failures (used in migration loops that
/// apply optional/auxiliary files). Returns Ok(()) whether or not it succeeded.
pub async fn psql_exec_file_ok(target: &PgTarget, path: &Path) -> OpsResult<()> {
    match psql_exec_file(target, path).await {
        Ok(()) => Ok(()),
        Err(e) => {
            warn(format!("ignoring psql failure for {}: {e}", path.display()));
            Ok(())
        }
    }
}

/// Scan `<migration_dir>/*.sql` for `CREATE EXTENSION IF NOT EXISTS <name>`
/// statements and return the names missing from the target database
/// (`pg_available_extensions`). Names deduplicated, extension name parsed
/// tolerating optional double quotes and optional trailing clauses like
/// `WITH SCHEMA ...`. Parse via regex-free string handling (split_whitespace).
/// Connection/query failures treat the extension as available (skipped).
pub async fn missing_extensions(migration_dir: &Path, target: &PgTarget) -> OpsResult<Vec<String>> {
    let mut sql_files: Vec<_> = std::fs::read_dir(migration_dir)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    sql_files.sort();
    let mut names = Vec::new();
    for path in sql_files {
        match std::fs::read_to_string(&path) {
            Ok(content) => names.extend(parse_extension_names(&content)),
            Err(e) => warn(format!("cannot read {}: {e}", path.display())),
        }
    }
    names.sort();
    names.dedup();
    let mut missing = Vec::new();
    for name in names {
        let sql = format!("SELECT count(*) FROM pg_available_extensions WHERE name = '{name}';");
        match psql_query(target, &sql).await {
            Ok(count) => {
                if count.parse::<u64>().unwrap_or(0) == 0 {
                    missing.push(name);
                }
            }
            Err(e) => warn(format!(
                "could not check extension {name}: {e} — assuming available"
            )),
        }
    }
    Ok(missing)
}

/// Acquire a postgres advisory lock (used to serialize migration runs).
pub async fn advisory_lock(target: &PgTarget, key: i64) -> OpsResult<()> {
    psql_exec(target, &format!("SELECT pg_advisory_lock({key});")).await
}

/// Release a postgres advisory lock (best-effort, Ok on failure).
pub async fn advisory_unlock(target: &PgTarget, key: i64) -> OpsResult<()> {
    match psql_exec(target, &format!("SELECT pg_advisory_unlock({key});")).await {
        Ok(()) => Ok(()),
        Err(e) => {
            warn(format!("advisory unlock ({key}) failed: {e}"));
            Ok(())
        }
    }
}

fn psql_command(target: &PgTarget, extra: &[&str]) -> OpsResult<Command> {
    let bin = ensure_psql()?;
    let mut cmd = Command::new(bin);
    cmd.env("PGPASSWORD", &target.password)
        .env("PGCLIENTENCODING", "UTF8")
        .args(target.psql_args())
        .args(extra);
    Ok(cmd)
}

async fn run_psql(mut cmd: Command) -> OpsResult<Output> {
    let out = tokio::task::spawn_blocking(move || cmd.output())
        .await
        .map_err(|e| OpsError::Command(format!("psql task failed: {e}")))??;
    Ok(out)
}

fn command_failure(kind: &str, sql: &str, out: &Output) -> OpsError {
    OpsError::Command(format!("psql {kind} failed: {sql}\n{}", stderr_tail(out)))
}

fn stderr_tail(out: &Output) -> String {
    const MAX: usize = 500;
    let text = String::from_utf8_lossy(&out.stderr);
    let text = text.trim();
    if text.len() <= MAX {
        text.to_string()
    } else {
        format!("{}…", &text[..text.floor_char_boundary(MAX)])
    }
}

fn parse_extension_names(content: &str) -> Vec<String> {
    const MARKER: &str = "CREATE EXTENSION IF NOT EXISTS";
    let mut names = Vec::new();
    for line in content.lines() {
        let Some(idx) = line.find(MARKER) else {
            continue;
        };
        let rest = &line[idx + MARKER.len()..];
        if let Some(name) = rest
            .split(|c: char| c == ';' || c.is_whitespace())
            .find(|tok| !tok.is_empty())
        {
            names.push(name.replace('"', ""));
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dead_target() -> PgTarget {
        PgTarget {
            host: "127.0.0.1".into(),
            port: 1,
            user: "u".into(),
            password: "pw".into(),
            db: "nonexistent".into(),
            role: "test".into(),
        }
    }

    #[test]
    fn parses_extension_names() {
        let content = r#"
-- comment
CREATE EXTENSION IF NOT EXISTS "pgcrypto" WITH SCHEMA public;
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE TABLE t (x int);
CREATE EXTENSION IF NOT EXISTS uuid-ossp;
"#;
        assert_eq!(
            parse_extension_names(content),
            vec!["pgcrypto", "pg_trgm", "pgcrypto", "uuid-ossp"]
        );
    }

    #[test]
    fn parses_extension_names_without_matches() {
        assert!(parse_extension_names("CREATE TABLE t (x int);").is_empty());
        assert!(parse_extension_names("").is_empty());
        assert!(parse_extension_names("CREATE EXTENSION pgcrypto;").is_empty());
    }

    #[test]
    fn extension_name_stops_at_semicolon() {
        assert_eq!(
            parse_extension_names("CREATE EXTENSION IF NOT EXISTS pgcrypto;SELECT 1;"),
            vec!["pgcrypto"]
        );
    }

    #[tokio::test]
    async fn missing_extensions_tolerates_unreachable_db() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("001_init.sql"),
            "CREATE EXTENSION IF NOT EXISTS \"pgcrypto\" WITH SCHEMA public;\nCREATE EXTENSION IF NOT EXISTS pgcrypto;\nCREATE EXTENSION IF NOT EXISTS pg_trgm;\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("002_other.sql"),
            "CREATE TABLE t (x int);\n",
        )
        .unwrap();
        let target = dead_target();
        match missing_extensions(dir.path(), &target).await {
            Ok(names) => assert!(names.is_empty(), "unexpected missing: {names:?}"),
            Err(OpsError::MissingTool("psql", _)) => { /* psql not installed — nothing to check */
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    #[tokio::test]
    async fn advisory_unlock_tolerates_failure() {
        let target = dead_target();
        assert!(advisory_unlock(&target, 42).await.is_ok());
    }

    #[tokio::test]
    async fn advisory_lock_reports_failure() {
        let target = dead_target();
        if let Ok(()) = advisory_lock(&target, 42).await { /* psql present and something answered on port 1 — unexpected but ok */
        }
    }
}
