//! Parse validation of generated SQLite DDL via `sqlglot-rust`.
//!
//! The SQLite dialect's STRICT tables accept only INT, INTEGER, REAL, TEXT,
//! BLOB, ANY as column types — anything else fails at `sqlite3` apply time,
//! not at generation time. Column types themselves are validated earlier, at
//! the DDL-context level (see
//! [`SqlDialect::validate_column_type`](super::dialect::SqlDialect)); this
//! module is the second, syntax-level gate: every generated SQLite statement
//! must parse under the SQLite dialect before the file is emitted.
//!
//! Statements that the parser cannot represent are skipped rather than
//! failed: SQLite trigger bodies (`CREATE TRIGGER ... BEGIN ... END;`) are
//! not part of sqlglot-rust's grammar. They are recognized textually and
//! excluded, so a template regression *inside* a trigger body is not caught
//! here — everything else (tables, indexes, virtual FTS5 tables, views,
//! inserts) must parse.

use sqlglot_rust::{transpile, Dialect};

use crate::error::{Error, Result};
use crate::traits::GeneratedFile;

/// Parse-validate every generated SQLite DDL file. Fails with a per-statement
/// error list naming the file when any statement does not parse.
pub fn validate_sqlite_files(files: &[GeneratedFile]) -> Result<()> {
    let mut errors = Vec::new();
    for file in files {
        for (index, stmt) in split_statements(&file.content).into_iter().enumerate() {
            if let Err(parse_err) = transpile(&stmt, Dialect::Sqlite, Dialect::Sqlite) {
                errors.push(format!(
                    "{}: statement {}: {parse_err}\n    {}",
                    file.path.display(),
                    index + 1,
                    first_line(&stmt)
                ));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(Error::Validation(format!(
            "generated SQLite DDL contains statements that do not parse:\n  - {}",
            errors.join("\n  - ")
        )))
    }
}

/// Split a SQL script into parseable statements: split on `;`, dropping
/// comment-only fragments and skipping trigger bodies (from a fragment that
/// starts a `CREATE TRIGGER` up to and including its closing `END`, which the
/// generated templates always place on a line of its own).
fn split_statements(content: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut in_trigger = false;
    for fragment in content.split(';') {
        let code = strip_comments(fragment);
        if code.is_empty() {
            continue;
        }
        let upper = code.to_uppercase();
        if in_trigger {
            if last_line_is_end(&code) {
                in_trigger = false;
            }
            continue;
        }
        if upper.starts_with("CREATE TRIGGER") {
            // The opening fragment still carries the first body statement;
            // stay in trigger mode until the closing END.
            if !last_line_is_end(&code) {
                in_trigger = true;
            }
            continue;
        }
        statements.push(code);
    }
    statements
}

/// True when the final line of the fragment is exactly `END` (the trigger
/// terminator shape used by the generated templates).
fn last_line_is_end(code: &str) -> bool {
    code.lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .is_some_and(|line| line.eq_ignore_ascii_case("END"))
}

/// Remove full-line `--` comments and trim. Inline comments are left alone —
/// generated DDL only uses full-line comments.
fn strip_comments(fragment: &str) -> String {
    let mut lines = Vec::new();
    for line in fragment.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("--") {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

/// First non-empty line of a statement, for error messages.
fn first_line(stmt: &str) -> &str {
    stmt.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("<empty>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(name: &str, content: &str) -> GeneratedFile {
        GeneratedFile {
            path: PathBuf::from(name),
            content: content.to_string(),
        }
    }

    #[test]
    fn strict_table_and_indexes_parse() {
        let files = vec![file(
            "hr_candidate.sql",
            "-- Generated. DO NOT EDIT.\n\
             CREATE TABLE IF NOT EXISTS candidate (\n    \
             id TEXT PRIMARY KEY,\n    \
             start_date TEXT,\n    \
             score REAL\n\
             ) STRICT;\n\
             CREATE INDEX IF NOT EXISTS idx_candidate_score ON candidate(score);\n",
        )];
        validate_sqlite_files(&files).expect("valid SQLite DDL must pass the gate");
    }

    #[test]
    fn fts_triggers_are_skipped_but_virtual_table_parses() {
        // The FTS5 file shape from templates/db/sqlite/fts.tera: a virtual
        // table plus BEGIN..END trigger bodies the parser cannot represent.
        let files = vec![file(
            "hr_candidate_fts.sql",
            "-- Full-text search (SQLite FTS5)\n\
             CREATE VIRTUAL TABLE IF NOT EXISTS candidate_fts USING fts5(\n    \
             name, content=candidate, content_rowid=id\n\
             );\n\
             CREATE TRIGGER IF NOT EXISTS candidate_fts_ai AFTER INSERT ON candidate BEGIN\n    \
             INSERT INTO candidate_fts(rowid, name) VALUES (new.id, new.name);\n\
             END;\n\
             CREATE TRIGGER IF NOT EXISTS candidate_fts_au AFTER UPDATE ON candidate BEGIN\n    \
             INSERT INTO candidate_fts(candidate_fts, rowid, name) VALUES ('delete', old.id, old.name);\n    \
             INSERT INTO candidate_fts(rowid, name) VALUES (new.id, new.name);\n\
             END;\n",
        )];
        validate_sqlite_files(&files).expect("FTS5 file with triggers must pass the gate");
    }

    #[test]
    fn invalid_statement_fails_naming_the_file() {
        let files = vec![file(
            "hr_broken.sql",
            "CREATE TABLE IF NOT EXISTS ok (id TEXT);\n\
             SELEKT 1 FROM ok;\n",
        )];
        let err = validate_sqlite_files(&files).expect_err("invalid SQL must fail the gate");
        let msg = err.to_string();
        assert!(msg.contains("hr_broken.sql"), "must name the file: {msg}");
        assert!(msg.contains("SELEKT"), "must quote the statement: {msg}");
    }

    #[test]
    fn comment_only_fragments_are_skipped() {
        let files = vec![file(
            "only_comments.sql",
            "-- nothing but comments\n-- and another comment line\n;\n",
        )];
        validate_sqlite_files(&files).expect("comment-only input must pass the gate");
    }

    #[test]
    fn trigger_scanner_survives_quoted_end_identifier() {
        // A body statement mentioning a quoted "end" column must not be
        // mistaken for the trigger terminator.
        let content = "CREATE TRIGGER trg AFTER INSERT ON t BEGIN\n    \
             INSERT INTO log(end) VALUES (new.\"end\");\n\
             END;\n\
             CREATE TABLE IF NOT EXISTS ok (id TEXT);\n";
        let stmts = split_statements(content);
        assert_eq!(
            stmts.len(),
            1,
            "only the trailing CREATE TABLE parses: {stmts:?}"
        );
        assert!(stmts[0].starts_with("CREATE TABLE"));
    }
}
