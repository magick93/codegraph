//! Compile pin for the cornucopia `?include=` hydration plane (issue #781 P1,
//! the #551 compile-pin policy).
//!
//! The wiring pin (`cornucopia_include_tests`) proves the emitters are wired
//! together and `syn`-parses six emitted files — syntax only. The adapter, the
//! query-handler wrappers and the handler call sites have NEVER been compiled:
//! they call `cornucopia_queries::queries::{domain}::{table}::include_*`, a
//! crate the real `cornucopia` CLI generates against a live migrated Postgres,
//! which no CI lane owes this suite. This pin closes that window the
//! `repository_compile_tests` / `worker_wasm_compile_tests` way: assemble the
//! emitted plane into a scratch cargo crate with a STUBBED
//! `cornucopia-queries` (the shared `test_framework::cornucopia_queries_stub`
//! `GenericClient` plus per-query modules) and `cargo check` it.
//!
//! # What is assembled, verbatim from the emitted tree
//!
//! * the emitted **domain-types crate** whole (the DTOs are its source of
//!   truth; every app-side `dto_response.rs` is a re-export shim over it);
//! * the app-side `dto_response.rs` / `dto_included.rs` shims for the four
//!   fixture entities (the include fields the handler assigns into);
//! * the **include adapter block** of `cornucopia_repository_impl.rs` — the
//!   inherent `fetch_*` impl the emitter appends after the CRUD trait impl,
//!   sliced out with its own header imports (the `use crate::domain::…` /
//!   `use cornucopia_queries::queries::…` lines the emitter wrote);
//! * the **include wrappers** of `query.rs` — the `set_rls_session_vars`
//!   function and the pre-rendered `fetch_*` methods, sliced out of the
//!   emitted file and hosted on a harness struct carrying the SAME fields
//!   (`repo`, `client_source`) under the SAME names;
//! * the **handler call sites** — every `state.{domain}_{module}_queries.fetch_*`
//!   call expression the emitted handlers contain is extracted verbatim and
//!   rendered into a probe function that awaits it, so the handler↔wrapper
//!   argument contract is type-checked too.
//!
//! # Deliberately excluded (and why)
//!
//! * The **CRUD halves** of `cornucopia_repository_impl.rs` / `query.rs`: they
//!   are pre-existing planes with their own pins (`worker_wasm_compile_tests`
//!   compiles a whole cornucopia worker repository + command path). Re-checking
//!   them here would mean a ~44-function stub surface (`create`/`get`/`list`/
//!   `count`/`update`/`delete` × the `_including_deleted` twins × four tables,
//!   with the text-cast bind shapes the CRUD SQL emits) that has nothing to do
//!   with #781. The include plane is the new surface; it is the one pinned.
//! * The **whole handler module**: it needs `AppState` with every domain slot,
//!   the command plane, extractors and utoipa registration. Only the include
//!   call expressions are extracted (the type-level contract with the
//!   wrappers); their surrounding 501/hydration shape is byte-pinned by
//!   `cornucopia_include_tests`.
//! * `meta.rs` and `db_client.rs`: the fixture profile omits the monolith
//!   `scaffold` generator, so those files are not emitted. Minimal stand-ins
//!   mirror the production templates' shapes (`templates/scaffold/meta.tera`,
//!   `templates/scaffold/db_client.tera`) — the planes that consume them have
//!   their own gates.
//!
//! # Teeth
//!
//! * The stub's constructor set is pinned to the emitted SQL's `--!` include
//!   annotations in BOTH directions (a renamed query cannot silently go
//!   unchecked; a stale stub function fails too).
//! * The handler call-site probe count is pinned to that same set (one GET +
//!   one LIST call per include path), so a dropped hydration call fails.
//! * Breaking a stub row field / bind arity fails `cargo check` (validated by
//!   a mutation run recorded in the PR).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "cornucopia_include_support/mod.rs"]
#[allow(dead_code)] // `FLAG_OFF_PROFILE` belongs to the wiring pin's half.
mod support;

#[path = "test_framework/mod.rs"]
mod test_framework;

use support::{FLAG_ON_PROFILE, generate, read};
use test_framework::cornucopia_queries_stub;

/// The four fixture tables, in the order the stub modules are rendered.
const TABLES: &[&str] = &[
    "customer",
    "currency",
    "invoice",
    "organization",
    "work_order",
];

/// The include query names the fixture MUST emit (fixture facts, asserted so a
/// fixture edit cannot silently shrink the pin's coverage).
const EXPECTED_INCLUDE_QUERIES: &[&str] = &[
    "include_customer_batch_for_invoice",
    "include_customer_batch_for_work_order",
    "include_customer_for_invoice",
    "include_customer_for_work_order",
    "include_customer_org_batch_for_invoice",
    "include_customer_org_for_invoice",
    "include_invoice_batch_for_customer",
    "include_invoice_for_customer",
    "include_organization_batch_for_customer",
    "include_organization_for_customer",
    "include_work_order_batch_for_customer",
    "include_work_order_for_customer",
];

/// Fixed materialization root (the #550 convention): survives a failed run so
/// the assembled tree can be inspected. `assemble_workspace` wipes `ws` before
/// writing, so a stale tree can never be compiled.
fn tree_root() -> PathBuf {
    std::env::temp_dir().join("cornucopia-include-pin")
}

/// Build cache shared across runs (and warmed by CI's rust-cache): a subdir of
/// the workspace `target/` so it never collides with the workspace lock.
fn pin_target_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CORNUCOPIA_INCLUDE_PIN_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cornucopia-include-pin")
}

fn write_file(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn copy_file(from: &Path, to: &Path) {
    fs::create_dir_all(to.parent().unwrap()).unwrap();
    fs::copy(from, to).unwrap();
}

fn copy_dir_recursive(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir_recursive(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

// ── Emitted-SQL facts (the tripwire) ──

/// Every `--!` annotation name in the fixture's emitted SQL files.
fn emitted_query_fns(output: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for table in TABLES {
        let sql_path = output
            .join("queries")
            .join("shop")
            .join(format!("{table}.sql"));
        let sql = read(&sql_path);
        names.extend(sql.lines().filter_map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix("--! ")?
                .split_whitespace()
                .next()
                .map(|name| name.trim_end_matches('(').to_string())
        }));
    }
    names.sort();
    assert!(
        !names.is_empty(),
        "the emitted SQL must carry `--!` annotations"
    );
    names
}

/// The `include_*` subset — the surface this pin compiles.
fn emitted_include_fns(output: &Path) -> Vec<String> {
    emitted_query_fns(output)
        .into_iter()
        .filter(|name| name.starts_with("include_"))
        .collect()
}

/// The stub's constructor set (`pub fn name() -> XStmt` only).
fn stub_query_fns(queries_rs: &str) -> Vec<String> {
    let mut names: Vec<String> = queries_rs
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let rest = trimmed.strip_prefix("pub fn ")?;
            let name_end = rest.find('(')?;
            if !rest[name_end..].starts_with("() ->") {
                return None;
            }
            Some(rest[..name_end].to_string())
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

// ── Extraction helpers (slice the emitted include plane) ──

/// The include adapter block: from the emitter's doc marker to EOF (the
/// emitter appends it last), with the header's non-`super::` imports kept so
/// the generated import strings are compiled too.
fn extract_include_adapter(src: &str, entity: &str) -> String {
    const MARKER: &str = "/// `?include=` hydration helpers (issue #781 P1).";
    let start = src.find(MARKER).unwrap_or_else(|| {
        panic!("{entity}: the emitted adapter must carry the include block marker")
    });
    let block = &src[start..];
    assert!(
        block.contains(&format!("impl {entity}RepositoryImpl {{")),
        "{entity}: the include block must be the inherent impl"
    );
    let imports: String = src
        .lines()
        .filter(|line| {
            line.starts_with("use ")
                && (!line.contains("super::") || line.starts_with("use super::dto_included::"))
        })
        .map(|line| format!("{line}\n"))
        .collect();
    format!(
        "//! Include adapter block, sliced verbatim from the emitted\n\
         //! `cornucopia_repository_impl.rs` by `cornucopia_include_compile_tests`.\n\
         {imports}\n\
         pub struct {entity}RepositoryImpl;\n\n\
         {block}"
    )
}

/// `async fn {name}(…) { … }` with balanced braces.
fn extract_function(src: &str, signature_prefix: &str, what: &str) -> String {
    let start = src
        .find(signature_prefix)
        .unwrap_or_else(|| panic!("{what}: `{signature_prefix}` must be emitted"));
    let rest = &src[start..];
    let open = rest
        .find('{')
        .unwrap_or_else(|| panic!("{what}: no function body"));
    let mut depth = 0usize;
    let mut end = None;
    for (i, ch) in rest[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.unwrap_or_else(|| panic!("{what}: unbalanced function body"));
    rest[..end].to_string()
}

/// The pre-rendered `fetch_*` wrapper methods: from the first include doc
/// comment to the end of the `impl {Entity}QueryHandler` block.
fn extract_include_methods(src: &str, entity: &str) -> String {
    const MARKER: &str = "    /// Eager-load ";
    let start = src.find(MARKER).unwrap_or_else(|| {
        panic!("{entity}: the emitted query handler must carry include wrappers")
    });
    let tail = src[start..].trim_end();
    let methods = tail.strip_suffix('}').unwrap_or_else(|| {
        panic!("{entity}: the include wrappers must be the tail of the impl block")
    });
    assert!(
        methods.trim().ends_with('}'),
        "{entity}: slicing must leave the last wrapper's closing brace"
    );
    methods.trim_end().to_string()
}

/// The `use crate::domain::shop::…::dto_response::…;` imports the wrappers'
/// signatures need — taken from the emitted file, not re-derived. The dot
/// wrappers additionally name the combined type from their own sibling
/// `dto_included`, which the harness hosts under the same module name.
fn extract_dto_imports(src: &str) -> String {
    src.lines()
        .filter(|line| {
            (line.starts_with("use crate::domain::shop::") && line.contains("::dto_response::"))
                || line.starts_with("use super::dto_included::")
        })
        .map(|line| format!("{line}\n"))
        .collect()
}

/// Every `state.{domain}_{module}_queries.fetch_*(…)` call expression the
/// emitted handler file contains, sliced verbatim (balanced parens).
fn extract_handler_include_calls(src: &str) -> Vec<String> {
    let mut calls = Vec::new();
    let mut search = 0usize;
    while let Some(rel) = src[search..].find("state.shop_") {
        let start = search + rel;
        let Some(open) = src[start..].find('(').map(|p| start + p) else {
            break;
        };
        if !src[start..open].contains("_queries.fetch_") {
            search = open;
            continue;
        }
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in src[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end.unwrap_or_else(|| panic!("unbalanced handler include call at {start}"));
        let expr = &src[start..end];
        if !calls.iter().any(|c: &String| c == expr) {
            calls.push(expr.to_string());
        }
        search = end;
    }
    calls
}

/// The `if let Some(val) = leaf_resp { … }` merge blocks of every dot arm in
/// the emitted handler, sliced verbatim (balanced braces). The combined
/// response's TYPES are what this pin compiles: a leaf field rename, or a
/// first-segment field whose DTO type is not the combined response (the
/// single-segment/dot alias collision), fails `cargo check`.
fn extract_dot_merge_blocks(src: &str) -> Vec<String> {
    const IF_LET: &str = "if let Some(val) = leaf_resp {";
    let mut blocks = Vec::new();
    let mut search = 0usize;
    while let Some(rel) = src[search..].find("let leaf_resp = ") {
        let start = search + rel;
        let Some(ifrel) = src[start..].find(IF_LET) else {
            break;
        };
        let ifstart = start + ifrel;
        let open = ifstart + IF_LET.len() - 1;
        assert!(src[open..].starts_with('{'), "dot merge block shape moved");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in src[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end.unwrap_or_else(|| panic!("unbalanced dot merge block at {ifstart}"));
        // The block starts at the wrapper call so the merged value's TYPE is
        // the compiled wrapper return type (not an inferred hole).
        blocks.push(src[start..end].to_string());
        search = end;
    }
    blocks
}

/// Host the extracted merge blocks on a typed probe: the real
/// `{Entity}IncludedData` and the real wrapper return type, so the handler's
/// merge is COMPILED (a text pin cannot see a field/type mismatch).
fn render_dot_merge_probe(blocks: &[String], entity: &str) -> String {
    let mut s = String::from(
        "//! Dot-notation merge probes, generated by\n\
         //! `cornucopia_include_compile_tests` from the emitted handlers.\n\
         //!\n\
         //! Each block is the handler's own `if let Some(val) = leaf_resp { … }`\n\
         //! merge, hosted against the REAL `IncludedData` + wrapper return type.\n\
         \n\
         use uuid::Uuid;\n\
         \n\
         use super::dto_included::*;\n\
         use crate::domain::shop::errors::ShopError;\n\
         use crate::error::AppError;\n\
         use crate::probe::{ApiKeyInfo, AppState};\n",
    );
    for (i, block) in blocks.iter().enumerate() {
        s.push_str(&format!(
            "\n#[allow(unused_variables)]\n\
             pub async fn dot_merge_probe_{i}(\n\
             \x20   state: &AppState,\n\
             \x20   api_key_info: &ApiKeyInfo,\n\
             \x20   included: &mut {entity}IncludedData,\n\
             \x20   id: Uuid,\n\
             \x20   correlation_id: Uuid,\n\
             ) -> Result<(), AppError> {{\n\
             {block}\n\
             \x20   Ok(())\n\
             }}\n"
        ));
    }
    s
}

/// Every `{module}::include_*` annotated-query call the emitted adapter
/// contains — the first hop's probe pair a dot path reuses is called from
/// here, not from a handler.
fn extract_adapter_include_calls(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = src;
    while let Some(pos) = rest.find("::include_") {
        let after = &rest[pos + 2..];
        let end = after.find('(').unwrap_or(after.len());
        let name = &after[..end];
        if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.is_empty() {
            names.push(name.to_string());
        }
        rest = &rest[pos + 2 + end..];
    }
    names.sort();
    names.dedup();
    names
}

// ── The stubbed `cornucopia-queries` include surface ──

/// Hand-written include query modules, shapes mirroring cornucopia's codegen:
/// a `Stmt` holder + constructor, a `bind` returning a row-query builder, and
/// an `.all()` returning the annotated row struct. Row fields follow the
/// emitted `--!` hints exactly (`source_id?` → `Option<Uuid>` for the
/// optional reverse FK, plain `Uuid` for the required one).
const QUERIES_RS: &str = r#"//! Check-time stub (test scaffold): the fixture's include query modules.
//! (This file IS the crate's `queries` module.)
//!
//! `include_*` shapes mirror `queries/shop/*.sql`'s `--!` annotations —
//! `cornucopia_include_compile_tests` pins this constructor set to them.

pub mod shop {
    /// The dot-notation source: the first hop's probe pair (emitted by the SQL
    /// plane because `InvoiceType` does not model `customer` on its own) plus
    /// the leaf hop's pair.
    pub mod invoice {
        use crate::client::GenericClient;
        use std::marker::PhantomData;

        type Error = tokio_postgres::Error;

        // --! include_customer_for_invoice (source_id) : (id, name, org_id?, created_at, updated_at)
        #[derive(Debug)]
        pub struct IncludeCustomerForInvoice {
            pub id: uuid::Uuid,
            pub name: String,
            pub org_id: Option<uuid::Uuid>,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeCustomerForInvoiceStmt;

        pub fn include_customer_for_invoice() -> IncludeCustomerForInvoiceStmt {
            IncludeCustomerForInvoiceStmt
        }

        pub struct IncludeCustomerForInvoiceQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a uuid::Uuid, &'s ())>,
        }

        impl IncludeCustomerForInvoiceStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_id: &'a uuid::Uuid,
            ) -> IncludeCustomerForInvoiceQuery<'c, 'a, 's, C> {
                IncludeCustomerForInvoiceQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> IncludeCustomerForInvoiceQuery<'c, 'a, 's, C> {
            pub async fn all(&self) -> Result<Vec<IncludeCustomerForInvoice>, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! include_customer_batch_for_invoice (source_ids) : (source_id, id, name, org_id?, created_at, updated_at)
        #[derive(Debug)]
        pub struct IncludeCustomerBatchForInvoice {
            pub source_id: uuid::Uuid,
            pub id: uuid::Uuid,
            pub name: String,
            pub org_id: Option<uuid::Uuid>,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeCustomerBatchForInvoiceStmt;

        pub fn include_customer_batch_for_invoice() -> IncludeCustomerBatchForInvoiceStmt {
            IncludeCustomerBatchForInvoiceStmt
        }

        pub struct IncludeCustomerBatchForInvoiceQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a [uuid::Uuid], &'s ())>,
        }

        impl IncludeCustomerBatchForInvoiceStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_ids: &'a [uuid::Uuid],
            ) -> IncludeCustomerBatchForInvoiceQuery<'c, 'a, 's, C> {
                IncludeCustomerBatchForInvoiceQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> IncludeCustomerBatchForInvoiceQuery<'c, 'a, 's, C> {
            pub async fn all(&self) -> Result<Vec<IncludeCustomerBatchForInvoice>, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! include_customer_org_for_invoice (source_id) : (id, currency_id?, name, created_at, updated_at)
        #[derive(Debug)]
        pub struct IncludeCustomerOrgForInvoice {
            pub id: uuid::Uuid,
            pub currency_id: Option<uuid::Uuid>,
            pub name: String,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeCustomerOrgForInvoiceStmt;

        pub fn include_customer_org_for_invoice() -> IncludeCustomerOrgForInvoiceStmt {
            IncludeCustomerOrgForInvoiceStmt
        }

        pub struct IncludeCustomerOrgForInvoiceQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a uuid::Uuid, &'s ())>,
        }

        impl IncludeCustomerOrgForInvoiceStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_id: &'a uuid::Uuid,
            ) -> IncludeCustomerOrgForInvoiceQuery<'c, 'a, 's, C> {
                IncludeCustomerOrgForInvoiceQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> IncludeCustomerOrgForInvoiceQuery<'c, 'a, 's, C> {
            pub async fn all(&self) -> Result<Vec<IncludeCustomerOrgForInvoice>, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! include_customer_org_batch_for_invoice (source_ids) : (source_id, id, currency_id?, name, created_at, updated_at)
        #[derive(Debug)]
        pub struct IncludeCustomerOrgBatchForInvoice {
            pub source_id: uuid::Uuid,
            pub id: uuid::Uuid,
            pub currency_id: Option<uuid::Uuid>,
            pub name: String,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeCustomerOrgBatchForInvoiceStmt;

        pub fn include_customer_org_batch_for_invoice() -> IncludeCustomerOrgBatchForInvoiceStmt {
            IncludeCustomerOrgBatchForInvoiceStmt
        }

        pub struct IncludeCustomerOrgBatchForInvoiceQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a [uuid::Uuid], &'s ())>,
        }

        impl IncludeCustomerOrgBatchForInvoiceStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_ids: &'a [uuid::Uuid],
            ) -> IncludeCustomerOrgBatchForInvoiceQuery<'c, 'a, 's, C> {
                IncludeCustomerOrgBatchForInvoiceQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> IncludeCustomerOrgBatchForInvoiceQuery<'c, 'a, 's, C> {
            pub async fn all(&self) -> Result<Vec<IncludeCustomerOrgBatchForInvoice>, Error> {
                unreachable!("check stub never executes")
            }
        }
    }

    /// Ditto (the scalar `organization` include's target module is imported by
    /// the customer adapter but only emits CRUD SQL).
    pub mod organization {}

    pub mod work_order {
        use crate::client::GenericClient;
        use std::marker::PhantomData;

        type Error = tokio_postgres::Error;

        // --! include_customer_for_work_order (source_id) : (id, name, org_id?, created_at, updated_at)
        #[derive(Debug)]
        pub struct IncludeCustomerForWorkOrder {
            pub id: uuid::Uuid,
            pub name: String,
            pub org_id: Option<uuid::Uuid>,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeCustomerForWorkOrderStmt;

        pub fn include_customer_for_work_order() -> IncludeCustomerForWorkOrderStmt {
            IncludeCustomerForWorkOrderStmt
        }

        pub struct IncludeCustomerForWorkOrderQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a uuid::Uuid, &'s ())>,
        }

        impl IncludeCustomerForWorkOrderStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_id: &'a uuid::Uuid,
            ) -> IncludeCustomerForWorkOrderQuery<'c, 'a, 's, C> {
                IncludeCustomerForWorkOrderQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> IncludeCustomerForWorkOrderQuery<'c, 'a, 's, C> {
            pub async fn all(&self) -> Result<Vec<IncludeCustomerForWorkOrder>, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! include_customer_batch_for_work_order (source_ids) : (source_id, id, name, org_id?, created_at, updated_at)
        #[derive(Debug)]
        pub struct IncludeCustomerBatchForWorkOrder {
            pub source_id: uuid::Uuid,
            pub id: uuid::Uuid,
            pub name: String,
            pub org_id: Option<uuid::Uuid>,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeCustomerBatchForWorkOrderStmt;

        pub fn include_customer_batch_for_work_order() -> IncludeCustomerBatchForWorkOrderStmt {
            IncludeCustomerBatchForWorkOrderStmt
        }

        pub struct IncludeCustomerBatchForWorkOrderQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a [uuid::Uuid], &'s ())>,
        }

        impl IncludeCustomerBatchForWorkOrderStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_ids: &'a [uuid::Uuid],
            ) -> IncludeCustomerBatchForWorkOrderQuery<'c, 'a, 's, C> {
                IncludeCustomerBatchForWorkOrderQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient>
            IncludeCustomerBatchForWorkOrderQuery<'c, 'a, 's, C>
        {
            pub async fn all(&self) -> Result<Vec<IncludeCustomerBatchForWorkOrder>, Error> {
                unreachable!("check stub never executes")
            }
        }
    }

    pub mod customer {
        use crate::client::GenericClient;
        use std::marker::PhantomData;

        type Error = tokio_postgres::Error;

        // --! include_invoice_for_customer (source_id) : (id, customer_id, number, total, created_at, updated_at)
        #[derive(Debug)]
        pub struct IncludeInvoiceForCustomer {
            pub id: uuid::Uuid,
            pub customer_id: uuid::Uuid,
            pub number: String,
            pub total: i32,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeInvoiceForCustomerStmt;

        pub fn include_invoice_for_customer() -> IncludeInvoiceForCustomerStmt {
            IncludeInvoiceForCustomerStmt
        }

        pub struct IncludeInvoiceForCustomerQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a uuid::Uuid, &'s ())>,
        }

        impl IncludeInvoiceForCustomerStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_id: &'a uuid::Uuid,
            ) -> IncludeInvoiceForCustomerQuery<'c, 'a, 's, C> {
                IncludeInvoiceForCustomerQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> IncludeInvoiceForCustomerQuery<'c, 'a, 's, C> {
            pub async fn all(&self) -> Result<Vec<IncludeInvoiceForCustomer>, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! include_invoice_batch_for_customer (source_ids) : (source_id, id, customer_id, number, total, created_at, updated_at)
        #[derive(Debug)]
        pub struct IncludeInvoiceBatchForCustomer {
            pub source_id: uuid::Uuid,
            pub id: uuid::Uuid,
            pub customer_id: uuid::Uuid,
            pub number: String,
            pub total: i32,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeInvoiceBatchForCustomerStmt;

        pub fn include_invoice_batch_for_customer() -> IncludeInvoiceBatchForCustomerStmt {
            IncludeInvoiceBatchForCustomerStmt
        }

        pub struct IncludeInvoiceBatchForCustomerQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a [uuid::Uuid], &'s ())>,
        }

        impl IncludeInvoiceBatchForCustomerStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_ids: &'a [uuid::Uuid],
            ) -> IncludeInvoiceBatchForCustomerQuery<'c, 'a, 's, C> {
                IncludeInvoiceBatchForCustomerQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient>
            IncludeInvoiceBatchForCustomerQuery<'c, 'a, 's, C>
        {
            pub async fn all(&self) -> Result<Vec<IncludeInvoiceBatchForCustomer>, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! include_work_order_for_customer (source_id) : (id, customer_id?, reference, total, created_at, updated_at)
        #[derive(Debug)]
        pub struct IncludeWorkOrderForCustomer {
            pub id: uuid::Uuid,
            pub customer_id: Option<uuid::Uuid>,
            pub reference: String,
            pub total: i32,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeWorkOrderForCustomerStmt;

        pub fn include_work_order_for_customer() -> IncludeWorkOrderForCustomerStmt {
            IncludeWorkOrderForCustomerStmt
        }

        pub struct IncludeWorkOrderForCustomerQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a uuid::Uuid, &'s ())>,
        }

        impl IncludeWorkOrderForCustomerStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_id: &'a uuid::Uuid,
            ) -> IncludeWorkOrderForCustomerQuery<'c, 'a, 's, C> {
                IncludeWorkOrderForCustomerQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> IncludeWorkOrderForCustomerQuery<'c, 'a, 's, C> {
            pub async fn all(&self) -> Result<Vec<IncludeWorkOrderForCustomer>, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! include_work_order_batch_for_customer (source_ids) : (source_id?, id, customer_id?, reference, total, created_at, updated_at)
        // The OPTIONAL reverse FK (`WorkOrderType.customer` is `refers …[0..1]`)
        // hints `source_id?` → `Option<Uuid>`; the adapter spells the NULL arm.
        #[derive(Debug)]
        pub struct IncludeWorkOrderBatchForCustomer {
            pub source_id: Option<uuid::Uuid>,
            pub id: uuid::Uuid,
            pub customer_id: Option<uuid::Uuid>,
            pub reference: String,
            pub total: i32,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeWorkOrderBatchForCustomerStmt;

        pub fn include_work_order_batch_for_customer() -> IncludeWorkOrderBatchForCustomerStmt {
            IncludeWorkOrderBatchForCustomerStmt
        }

        pub struct IncludeWorkOrderBatchForCustomerQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a [uuid::Uuid], &'s ())>,
        }

        impl IncludeWorkOrderBatchForCustomerStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_ids: &'a [uuid::Uuid],
            ) -> IncludeWorkOrderBatchForCustomerQuery<'c, 'a, 's, C> {
                IncludeWorkOrderBatchForCustomerQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient>
            IncludeWorkOrderBatchForCustomerQuery<'c, 'a, 's, C>
        {
            pub async fn all(&self) -> Result<Vec<IncludeWorkOrderBatchForCustomer>, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! include_organization_for_customer (source_id) : (id, currency_id?, name, created_at, updated_at)
        #[derive(Debug)]
        pub struct IncludeOrganizationForCustomer {
            pub id: uuid::Uuid,
            pub currency_id: Option<uuid::Uuid>,
            pub name: String,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeOrganizationForCustomerStmt;

        pub fn include_organization_for_customer() -> IncludeOrganizationForCustomerStmt {
            IncludeOrganizationForCustomerStmt
        }

        pub struct IncludeOrganizationForCustomerQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a uuid::Uuid, &'s ())>,
        }

        impl IncludeOrganizationForCustomerStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_id: &'a uuid::Uuid,
            ) -> IncludeOrganizationForCustomerQuery<'c, 'a, 's, C> {
                IncludeOrganizationForCustomerQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient> IncludeOrganizationForCustomerQuery<'c, 'a, 's, C> {
            pub async fn all(&self) -> Result<Vec<IncludeOrganizationForCustomer>, Error> {
                unreachable!("check stub never executes")
            }
        }

        // --! include_organization_batch_for_customer (source_ids) : (source_id, id, currency_id?, name, created_at, updated_at)
        #[derive(Debug)]
        pub struct IncludeOrganizationBatchForCustomer {
            pub source_id: uuid::Uuid,
            pub id: uuid::Uuid,
            pub currency_id: Option<uuid::Uuid>,
            pub name: String,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }

        pub struct IncludeOrganizationBatchForCustomerStmt;

        pub fn include_organization_batch_for_customer() -> IncludeOrganizationBatchForCustomerStmt {
            IncludeOrganizationBatchForCustomerStmt
        }

        pub struct IncludeOrganizationBatchForCustomerQuery<'c, 'a, 's, C: GenericClient> {
            _marker: PhantomData<(&'c C, &'a [uuid::Uuid], &'s ())>,
        }

        impl IncludeOrganizationBatchForCustomerStmt {
            pub fn bind<'c, 'a, 's, C: GenericClient>(
                &'s self,
                _client: &'c C,
                _source_ids: &'a [uuid::Uuid],
            ) -> IncludeOrganizationBatchForCustomerQuery<'c, 'a, 's, C> {
                IncludeOrganizationBatchForCustomerQuery {
                    _marker: PhantomData,
                }
            }
        }

        impl<'c, 'a, 's, C: GenericClient>
            IncludeOrganizationBatchForCustomerQuery<'c, 'a, 's, C>
        {
            pub async fn all(&self) -> Result<Vec<IncludeOrganizationBatchForCustomer>, Error> {
                unreachable!("check stub never executes")
            }
        }
    }
}
"#;

// ── Stand-in modules the fixture profile does not emit ──

/// `src/db_client.rs` stand-in: the `ClientSource`/`DbClient`/transaction
/// shape from `templates/scaffold/db_client.tera` (native arm), reduced to
/// what the include wrappers call.
const DB_CLIENT_RS: &str = r#"//! Check-time stand-in for the scaffold `db_client.rs` (the fixture profile
//! omits the monolith `scaffold` generator). Mirrors the production
//! `ClientSource` shape: acquire a client, open a transaction, commit.

use async_trait::async_trait;

/// Errors raised while acquiring a cornucopia client or transaction.
#[derive(Debug, thiserror::Error)]
pub enum DbClientError {
    #[error("database client error: {0}")]
    Client(String),
}

/// A client that can open a transaction.
pub struct DbClient(pub tokio_postgres::Client);

impl DbClient {
    pub async fn transaction(
        &mut self,
    ) -> Result<tokio_postgres::Transaction<'_>, tokio_postgres::Error> {
        self.0.transaction().await
    }
}

/// Source of per-call database clients (the generated trait shape).
#[async_trait]
pub trait ClientSource: Send + Sync {
    async fn client(&self) -> Result<DbClient, DbClientError>;
}
"#;

/// `src/api/meta.rs` stand-in (`templates/scaffold/meta.tera` verbatim shape).
const API_META_RS: &str = r#"use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Shared metadata included in all API JSON responses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Meta {
    /// Correlation ID for request tracing.
    pub correlation_id: String,
}
"#;

/// `src/domain/shop/errors.rs` stand-in: the `ShopError` surface the include
/// wrappers use (`from_repo_err`, `http_status`, Display). The generated
/// errors module is its own plane and pulls sea-orm/deadpool for conversions
/// this pin does not exercise.
const SHOP_ERROR_RS: &str = r#"//! Check-time stand-in for the generated domain error (only the surface the
//! include wrappers touch: `from_repo_err` + `http_status` + Display).

use axum::http::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum ShopError {
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    InternalError(String),
}

impl ShopError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Forbidden(_) => "FORBIDDEN",
            Self::InternalError(_) => "INTERNAL_ERROR",
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn from_repo_err(e: impl std::fmt::Display) -> Self {
        let msg = e.to_string();
        if msg.contains("INSUFFICIENT_SCOPE")
            || msg.contains("ROLE_FORBIDDEN")
            || msg.contains("violates row-level security policy")
        {
            Self::Forbidden(msg)
        } else {
            Self::InternalError(msg)
        }
    }
}

impl From<tokio_postgres::Error> for ShopError {
    fn from(e: tokio_postgres::Error) -> Self {
        Self::InternalError(e.to_string())
    }
}
"#;

/// `src/error.rs` stand-in: only the surface the dot merge's `map_err` calls
/// (`from_domain_error` + `with_correlation_id`). The generated `error.rs` is
/// its own plane.
const APP_ERROR_RS: &str = r#"//! Check-time stand-in for the generated `AppError` (the dot merge surface).

use axum::http::StatusCode;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub correlation_id: Option<Uuid>,
}

impl AppError {
    pub fn from_domain_error(status: StatusCode, message: String, _context: &str) -> Self {
        Self {
            status,
            code: "DOMAIN_ERROR",
            message,
            details: None,
            correlation_id: None,
        }
    }

    pub fn with_correlation_id(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }
}
"#;

const APP_CARGO_TOML: &str = r#"[package]
name = "cornucopia-include-pin-app"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
name = "cornucopia_include_pin_app"
path = "src/lib.rs"

[dependencies]
async-trait = "0.1"
axum = "0.8"
chrono = { version = "0.4", features = ["serde"] }
cornucopia-queries = { path = "../cornucopia-queries" }
domain-types = { path = "../domain-types" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio-postgres = { version = "0.7", default-features = false, features = ["with-chrono-0_4", "with-uuid-1", "with-serde_json-1"] }
utoipa = { version = "6", features = ["chrono", "uuid", "decimal"] }
uuid = { version = "1", features = ["v4", "serde"] }
"#;

/// The app-side `pub mod` index for one fixture entity.
fn entity_mod_rs(entity: &str, with_includes: bool, has_dot: bool) -> String {
    let mut s = String::from("pub mod dto_response;\n");
    if with_includes {
        s.push_str("pub mod dto_included;\n");
        s.push_str("pub mod include_adapter;\n");
        s.push_str("pub mod include_queries;\n");
        if has_dot {
            s.push_str("pub mod dot_merge;\n");
        }
    }
    format!("//! {entity} module (check-time assembly).\n{s}")
}

/// The include-wrappers harness module: the emitted `set_rls_session_vars`
/// function + wrapper methods, hosted on a struct carrying the SAME field
/// names the generated `{Entity}QueryHandler` carries.
fn render_include_queries(emitted_query_rs: &str, entity: &str, error_domain: &str) -> String {
    let session_fn = extract_function(
        emitted_query_rs,
        "async fn set_rls_session_vars(",
        &format!("{entity} query.rs set_rls_session_vars"),
    );
    let methods = extract_include_methods(emitted_query_rs, entity);
    let dto_imports = extract_dto_imports(emitted_query_rs);
    format!(
        "//! Include query-handler wrappers, sliced verbatim from the emitted\n\
         //! `query.rs` by `cornucopia_include_compile_tests`.\n\
         \n\
         use std::sync::Arc;\n\
         \n\
         use cornucopia_queries::client;\n\
         use uuid::Uuid;\n\
         \n\
         use crate::db_client::ClientSource;\n\
         use super::include_adapter::{entity}RepositoryImpl;\n\
         use crate::domain::{error_domain}::errors::ShopError;\n\
         {dto_imports}\n\
         {session_fn}\n\n\
         /// Same field names + types as the generated `{entity}QueryHandler`.\n\
         pub struct {entity}Queries {{\n\
         \x20   pub repo: Arc<{entity}RepositoryImpl>,\n\
         \x20   pub client_source: Arc<dyn ClientSource>,\n\
         }}\n\
         \n\
         impl {entity}Queries {{\n\
         {methods}\n\
         }}\n"
    )
}

/// Render the handler call-site probes: one function per emitted include call
/// expression, awaited so the wrapper signature is type-checked against it.
fn render_probe(calls: &[String]) -> String {
    let mut s = String::from(
        "//! Handler include call-site probes, generated by\n\
         //! `cornucopia_include_compile_tests` from the emitted handlers.\n\
         //!\n\
         //! Each `state.{domain}_{module}_queries.fetch_*(…)` expression is\n\
         //! awaited against the compiled wrapper — the handler↔wrapper\n\
         //! argument contract.\n\
         \n\
         use uuid::Uuid;\n\
         \n\
         use crate::domain::shop::customer::include_queries::CustomerQueries;\n\
         use crate::domain::shop::invoice::include_queries::InvoiceQueries;\n\
         use crate::domain::shop::work_order::include_queries::WorkOrderQueries;\n\
         \n\
         #[derive(Clone)]\n\
         pub struct ApiKeyInfo {\n\
         \x20   pub api_key_id: Uuid,\n\
         \x20   pub organization_id: Uuid,\n\
         \x20   pub user_id: Uuid,\n\
         \x20   pub role: String,\n\
         }\n\
         \n\
         pub struct AppState {\n\
         \x20   pub shop_customer_queries: CustomerQueries,\n\
         \x20   pub shop_invoice_queries: InvoiceQueries,\n\
         \x20   pub shop_work_order_queries: WorkOrderQueries,\n\
         }\n",
    );
    for (i, call) in calls.iter().enumerate() {
        s.push_str(&format!(
            "\n#[allow(unused_variables)]\n\
             pub async fn include_call_{i}(state: &AppState, api_key_info: &ApiKeyInfo) {{\n\
             \x20   let id: Uuid = Uuid::nil();\n\
             \x20   let ids: Vec<Uuid> = Vec::new();\n\
             \x20   let _ = {call}.await;\n\
             }}\n"
        ));
    }
    s
}

// ── Assembly ──

/// Assemble the scratch workspace and return the app manifest path.
fn assemble_workspace(output: &Path, ws: &Path) -> PathBuf {
    let _ = fs::remove_dir_all(ws);
    fs::create_dir_all(ws).unwrap();

    // ── cornucopia-queries: the shared GenericClient stub + include modules.
    write_file(
        &ws.join("cornucopia-queries/Cargo.toml"),
        cornucopia_queries_stub::CARGO_TOML,
    );
    write_file(
        &ws.join("cornucopia-queries/build.rs"),
        cornucopia_queries_stub::BUILD_RS,
    );
    write_file(
        &ws.join("cornucopia-queries/src/lib.rs"),
        &format!("{}\npub mod queries;\n", cornucopia_queries_stub::LIB_RS),
    );
    write_file(&ws.join("cornucopia-queries/src/queries.rs"), QUERIES_RS);

    // ── domain-types: the emitted crate, whole. The fixture profile leaves
    // `domain_types_base` unset, so the run writes the crate at the output
    // root (`Cargo.toml` + `src/{lib.rs,shop,codelist,context.rs,query.rs}`).
    copy_file(
        &output.join("Cargo.toml"),
        &ws.join("domain-types/Cargo.toml"),
    );
    for file in ["lib.rs", "context.rs", "query.rs"] {
        copy_file(
            &output.join("src").join(file),
            &ws.join("domain-types/src").join(file),
        );
    }
    copy_dir_recursive(&output.join("src/shop"), &ws.join("domain-types/src/shop"));
    copy_dir_recursive(
        &output.join("src/codelist"),
        &ws.join("domain-types/src/codelist"),
    );
    // The emitted manifest pins `codegraph-type-contracts` to the checkout's
    // git rev; assemble it as a path dep so the pin needs no network (the
    // emitted DTOs do not reference the crate — the dep keeps the manifest
    // shape faithful to production).
    let manifest_path = ws.join("domain-types/Cargo.toml");
    let manifest = read(&manifest_path);
    let contracts_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("codegraph-type-contracts");
    let rewritten: String = manifest
        .lines()
        .map(|line| {
            if line.starts_with("codegraph-type-contracts") {
                format!(
                    "codegraph-type-contracts = {{ path = \"{}\" }}",
                    contracts_path.display()
                )
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    write_file(&manifest_path, &rewritten);

    // ── app crate.
    let app = ws.join("app");
    write_file(&app.join("Cargo.toml"), APP_CARGO_TOML);
    write_file(
        &app.join("src/lib.rs"),
        "pub mod api;\npub mod db_client;\npub mod domain;\npub mod error;\npub mod probe;\n",
    );
    write_file(&app.join("src/db_client.rs"), DB_CLIENT_RS);
    write_file(&app.join("src/error.rs"), APP_ERROR_RS);
    write_file(
        &app.join("src/api/mod.rs"),
        "pub mod links;\npub mod meta;\n",
    );
    copy_file(
        &output.join("src/api/links.rs"),
        &app.join("src/api/links.rs"),
    );
    write_file(&app.join("src/api/meta.rs"), API_META_RS);

    write_file(&app.join("src/domain/mod.rs"), "pub mod shop;\n");
    write_file(
        &app.join("src/domain/shop/mod.rs"),
        "pub mod currency;\npub mod customer;\npub mod errors;\npub mod invoice;\npub mod organization;\npub mod work_order;\n",
    );
    write_file(&app.join("src/domain/shop/errors.rs"), SHOP_ERROR_RS);

    // Per-entity app modules: response shims (all four) + the include plane
    // (customer + work_order — the fixture's only include-bearing sources).
    let mut handler_calls: Vec<String> = Vec::new();
    for table in TABLES {
        let emitted_root = output.join("src/domain/shop").join(table);
        let app_root = app.join("src/domain/shop").join(table);
        copy_file(
            &emitted_root.join("dto_response.rs"),
            &app_root.join("dto_response.rs"),
        );
        let entity = match *table {
            "customer" => "Customer",
            "currency" => "Currency",
            "invoice" => "Invoice",
            "organization" => "Organization",
            "work_order" => "WorkOrder",
            other => panic!("unexpected table {other}"),
        };
        let with_includes = matches!(*table, "customer" | "invoice" | "work_order");
        let handler_src = read(&output.join(format!("src/api/shop/{table}_handler.rs")));
        let dot_blocks = if with_includes {
            extract_dot_merge_blocks(&handler_src)
        } else {
            Vec::new()
        };
        write_file(
            &app_root.join("mod.rs"),
            &entity_mod_rs(entity, with_includes, !dot_blocks.is_empty()),
        );
        if !with_includes {
            continue;
        }
        copy_file(
            &emitted_root.join("dto_included.rs"),
            &app_root.join("dto_included.rs"),
        );
        let adapter_src = read(&emitted_root.join("cornucopia_repository_impl.rs"));
        write_file(
            &app_root.join("include_adapter.rs"),
            &extract_include_adapter(&adapter_src, entity),
        );
        let query_src = read(&emitted_root.join("query.rs"));
        write_file(
            &app_root.join("include_queries.rs"),
            &render_include_queries(&query_src, entity, "shop"),
        );
        handler_calls.extend(extract_handler_include_calls(&handler_src));
        if !dot_blocks.is_empty() {
            write_file(
                &app_root.join("dot_merge.rs"),
                &render_dot_merge_probe(&dot_blocks, entity),
            );
        }
    }

    // ── handler include call-site probes.
    assert!(
        !handler_calls.is_empty(),
        "the emitted handlers must carry include hydration calls"
    );
    write_file(&app.join("src/probe.rs"), &render_probe(&handler_calls));

    app.join("Cargo.toml")
}

fn cargo_check(workspace_manifest: &Path, label: &str) {
    let out = Command::new("cargo")
        .arg("check")
        .arg("--manifest-path")
        .arg(workspace_manifest)
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TARGET_DIR", pin_target_dir())
        .output()
        .expect("cargo check must run");
    assert!(
        out.status.success(),
        "the {label} must compile in isolation:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Issue #781 P1 / #551: the cornucopia include plane — include SQL adapter
/// calls, session-owning query wrappers, handler call sites — compiles
/// against a stubbed `cornucopia-queries` carrying the emitted `--!` surface.
#[tokio::test]
async fn cornucopia_include_plane_compiles_in_isolation() {
    let (_fixture, output) = generate(FLAG_ON_PROFILE).await;

    // ── Tripwires BEFORE the (slow) compile, so drift is named. ──
    let include_fns = emitted_include_fns(&output);
    let expected: Vec<String> = EXPECTED_INCLUDE_QUERIES
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        include_fns, expected,
        "the fixture's emitted include `--!` surface moved — update \
         EXPECTED_INCLUDE_QUERIES (and the stub/rows) with it"
    );
    let stub_fns = stub_query_fns(QUERIES_RS);
    assert_eq!(
        stub_fns, include_fns,
        "the stub query surface must cover EXACTLY the emitted include `--!` \
         annotations (update QUERIES_RS when the fixture or the SQL emitter \
         changes)"
    );

    // Every include query must be CALLED somewhere: the handler calls the
    // path fetch wrappers (GET + LIST), and the adapter calls the first hop's
    // probe pair a dot path reuses. A dropped call is a broken plane; an
    // emitted-but-uncalled query is dead SQL.
    let mut handler_calls: Vec<String> = Vec::new();
    for table in ["customer", "invoice", "work_order"] {
        let handler = read(&output.join(format!("src/api/shop/{table}_handler.rs")));
        handler_calls.extend(extract_handler_include_calls(&handler));
    }
    let mut call_methods: Vec<String> = handler_calls
        .iter()
        .filter_map(|call| {
            let rest = call.split(".fetch_").nth(1)?;
            let name = rest.split('(').next()?;
            Some(format!("include_{name}"))
        })
        .collect();
    for table in ["customer", "invoice", "work_order"] {
        let adapter = read(&output.join(format!(
            "src/domain/shop/{table}/cornucopia_repository_impl.rs"
        )));
        call_methods.extend(extract_adapter_include_calls(&adapter));
    }
    call_methods.sort();
    call_methods.dedup();
    assert_eq!(
        call_methods, include_fns,
        "every emitted include query must be called by a handler (GET + \
         LIST) or by the adapter (a dot path's first hop) — a dropped call \
         is a broken plane"
    );

    // ── Assemble + compile. ──
    let ws = tree_root().join("ws");
    let app_manifest = assemble_workspace(&output, &ws);
    cargo_check(&app_manifest, "cornucopia include plane");
}
