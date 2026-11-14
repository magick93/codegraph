//! Issue #489 Phase 0 — codegen engine benchmark (genco vs Tera).
//!
//! Run with:
//!
//! ```bash
//! cargo test -p codegraph --test genco_bench -- --ignored --nocapture
//! ```
//!
//! # What this measures, and why
//!
//! The decision this feeds is "should Rust-emitting templates move from Tera to
//! genco", gated on measured cost. Three separate questions have to be answered
//! independently, because conflating them is how migrations get approved on
//! data that does not support them:
//!
//! 1. **How much of a generation run is template work at all?** Answered by the
//!    `CODEGRAPH_PROFILE_GEN=1` instrumentation in
//!    `codegraph_generate::gen_profile` (stderr-only, output-identical). This
//!    file cannot answer it — no synthetic loop can — so it is measured against
//!    a real pipeline run.
//! 2. **Is a typed quoter faster or slower than Tera on identical output?**
//!    Answered here, contestant-by-contestant, on the same document.
//! 3. **How much of the current cost is our own buffer, not Tera?** `wln!`
//!    used to build a `String` per line. Capturing that separately (legacy
//!    `CodeWriter` vs `push_line_fmt`) stops a buffer bug being attributed to
//!    the template engine and "fixed" by a multi-week migration.
//!
//! # Contestants
//!
//! | # | Contestant | What it represents |
//! |---|------------|--------------------|
//! | A | Tera (`domain_types/dto_response.tera`) | today's path, faithfully |
//! | B | `CodeWriter` legacy (`format!` per line) | the pre-#489 buffer |
//! | C | `CodeWriter::push_line_fmt` | the shipped #489 buffer |
//! | D | genco, literal `use` lines | byte-identical to A |
//! | E | genco, managed imports | the reason #489 exists; not byte-identical |
//!
//! A/B/C must be **byte-identical** — pinned by the non-ignored
//! `all_engines_produce_identical_output` test, so the timing comparison is
//! comparing equal work.

use std::time::{Duration, Instant};

use codegraph_generate::DtoKeyCasing;
use codegraph_generate::ProjectConfig;
use codegraph_generate::code_writer::CodeWriter;
use codegraph_generate::ddd::dto::{ChildDtoContext, DtoContext, DtoField, MoxDerivedExtra};
use codegraph_generate::render_template_with_project;
use codegraph_generate::template_engine::create_tera_with_overrides;

use genco::prelude::*;

// ---------------------------------------------------------------------------
// Workload: a realistic `domain_types/dto_response.tera` render
// ---------------------------------------------------------------------------

/// One DtoField with the defaults the real builder uses for a plain column.
fn dto_field(
    name: &str,
    rust_type: &str,
    required: bool,
    array: bool,
    entity_ref: bool,
) -> DtoField {
    DtoField {
        name: name.to_string(),
        rust_type: rust_type.to_string(),
        is_required: required,
        is_array: array,
        description: String::new(),
        render_strategy: if entity_ref { "ref" } else { "direct" }.to_string(),
        is_entity_ref: entity_ref,
        is_hierarchy_field: false,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pattern: None,
        format: None,
    }
}

/// A `DtoContext` shaped like a real mid-sized entity: mixed required /
/// optional / array / entity-ref fields, a derived field, two child DTOs
/// (one nested), a codelist import, a structured-wrapper import, and a
/// workflow section. This is the shape the template's every branch exercises.
fn bench_context() -> DtoContext {
    let fields = vec![
        dto_field("id", "uuid::Uuid", true, false, false),
        dto_field("candidate_number", "String", true, false, false),
        dto_field("first_name", "String", true, false, false),
        dto_field("last_name", "String", true, false, false),
        dto_field("email", "String", true, false, false),
        dto_field("phone", "String", false, false, false),
        dto_field("date_of_birth", "chrono::NaiveDate", false, false, false),
        dto_field("status", "CandidateStatus", true, false, false),
        dto_field("notes", "String", false, false, false),
        dto_field("rating", "i32", false, false, false),
        dto_field("tags", "String", false, true, false),
        dto_field("screening_ids", "uuid::Uuid", false, true, true),
        dto_field("owner", "uuid::Uuid", false, false, true),
        dto_field("full_name", "String", true, false, false),
    ];

    let child_fields = vec![
        dto_field("line1", "String", true, false, false),
        dto_field("line2", "String", false, false, false),
        dto_field("city", "String", true, false, false),
        dto_field("postcode", "String", false, false, false),
    ];

    let grandchild_fields = vec![
        dto_field("kind", "String", true, false, false),
        dto_field("value", "String", false, false, false),
    ];

    let contact = ChildDtoContext {
        field_name: "contact".to_string(),
        struct_name: "Contact".to_string(),
        fields: child_fields,
        is_array: false,
        child_dtos: vec![ChildDtoContext {
            field_name: "primary_phone".to_string(),
            struct_name: "Phone".to_string(),
            fields: grandchild_fields,
            is_array: false,
            child_dtos: Vec::new(),
        }],
    };

    let addresses = ChildDtoContext {
        field_name: "addresses".to_string(),
        struct_name: "Address".to_string(),
        fields: vec![
            dto_field("line1", "String", true, false, false),
            dto_field("country", "String", true, false, false),
        ],
        is_array: true,
        child_dtos: Vec::new(),
    };

    DtoContext {
        module_name: "candidate".to_string(),
        entity_name: "Candidate".to_string(),
        domain: "recruiting".to_string(),
        fields,
        immutable_fields: vec!["candidate_number".to_string()],
        workflow_excluded_fields: Vec::new(),
        list_exclude: Vec::new(),
        list_include: Vec::new(),
        has_list_fields: false,
        operations: vec![
            "create".to_string(),
            "update".to_string(),
            "read".to_string(),
        ],
        child_dtos: vec![contact.clone(), addresses.clone()],
        all_child_dtos: vec![
            contact.clone(),
            addresses.clone(),
            contact.child_dtos[0].clone(),
        ],
        codelist_imports: vec!["CandidateStatus".to_string()],
        codelist_imports_update: vec!["CandidateStatus".to_string()],
        has_workflow: true,
        has_approval_status: true,
        structured_imports: vec!["use crate::common::contact_details::ContactDetails;".to_string()],
        has_validate: false,
        derived_fields: vec!["full_name".to_string()],
        mox_derived_extras: vec![MoxDerivedExtra {
            source_name: "score".to_string(),
            name: "score".to_string(),
            rust_type: Some("f64".to_string()),
            body: "0.0".to_string(),
        }],
    }
}

fn bench_project() -> ProjectConfig {
    let mut project = ProjectConfig::default();
    project.codegen.dto_key_casing = DtoKeyCasing::Camel;
    project
}

fn tera_engine() -> tera::Tera {
    create_tera_with_overrides(&[]).expect("built-in templates load")
}

fn render_tera(tera: &tera::Tera, ctx: &DtoContext, project: &ProjectConfig) -> String {
    render_template_with_project(tera, "domain_types/dto_response.tera", ctx, project)
        .expect("dto_response renders")
}

// ---------------------------------------------------------------------------
// Contestant B: the pre-#489 CodeWriter path (`wln!` = `format!` per line)
// ---------------------------------------------------------------------------

/// Rebuild `text` through [`CodeWriter`], one `format!` allocation per line —
/// what `wln!` did before issue #489 Phase 0.
///
/// The `format!` is the point: it reproduces the per-line allocation the
/// legacy macro paid, so the two buffers are compared on identical output
/// shape rather than on a rewritten hot loop.
#[allow(clippy::useless_format)]
fn rewrite_with_code_writer_legacy(text: &str) -> String {
    let mut w = CodeWriter::new();
    for line in text.lines() {
        w.push_line(&format!("{line}"));
    }
    w.into_string()
}

// ---------------------------------------------------------------------------
// Contestant C: the shipped CodeWriter path (`push_line_fmt`, no allocation)
// ---------------------------------------------------------------------------

/// Rebuild `text` through [`CodeWriter::push_line_fmt`] — the shipped fast
/// path, writing straight into the destination buffer.
fn rewrite_with_code_writer(text: &str) -> String {
    let mut w = CodeWriter::new();
    for line in text.lines() {
        w.push_line_fmt(format_args!("{line}"));
    }
    w.into_string()
}

// ---------------------------------------------------------------------------
// Contestants D / E: genco
// ---------------------------------------------------------------------------

/// Emit a line verbatim.
///
/// genco's Rust language mode rewrites `///` / `//!` comments into
/// `#[doc = "..."]` attributes, and interpolates nothing inside them. Passing
/// the whole line as an interpolated literal is the only way to reproduce the
/// existing byte output — this is one of the concrete migration costs #489
/// predicted.
fn doc(s: &str) -> String {
    s.to_string()
}

/// genco port of `domain_types/dto_response.tera`, emitting `use` lines as
/// literals so the output is byte-identical to the Tera render.
fn genco_literal_imports(ctx: &DtoContext, camel: bool) -> rust::Tokens {
    let entity = ctx.entity_name.as_str();
    let domain = ctx.domain.as_str();
    let structured = &ctx.structured_imports;
    let codelists = &ctx.codelist_imports;
    let fields = &ctx.fields;
    let derived = &ctx.derived_fields;
    let child_dtos = &ctx.child_dtos;
    let children = &ctx.all_child_dtos;
    let extras = &ctx.mox_derived_extras;

    quote! {
        $(doc(&format!("//! Response DTO for {}.", ctx.entity_name)))
        $(doc("//! Generated by codegraph. DO NOT EDIT."))

        use serde::{Deserialize, Serialize};
        use utoipa::ToSchema;
        $(for import in structured => $(&*import.clone())$['\r'])
        $(for import in codelists => pub use crate::codelist::$(&*import.clone());$['\r'])

        $(doc(&format!("/// Response body for {}.", ctx.entity_name)))
        #[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
        $(if camel => #[serde(rename_all = "camelCase")])
        pub struct $(entity)Response {
            pub id: uuid::Uuid,
            $(for field in fields join ($['\r']) =>
                $(if !field.name.eq("id") {
                    $(if derived.contains(&field.name) {
                        $(doc("/// Computed by the mox domain model (derived) — read-only."))
                    })
                    $(if field.is_entity_ref {
                        $(if field.is_array {
                            $(if field.is_required {
                                pub $(&field.name): Vec<uuid::Uuid>,
                            } else {
                                #[serde(skip_serializing_if = "Option::is_none")]
                                pub $(&field.name): Option<Vec<uuid::Uuid>>,
                            })
                        } else {
                            $(if field.is_required {
                                pub $(&field.name): uuid::Uuid,
                            } else {
                                #[serde(skip_serializing_if = "Option::is_none")]
                                pub $(&field.name): Option<uuid::Uuid>,
                            })
                        })
                    } else {
                        $(if field.is_required {
                            $(if field.is_array {
                                pub $(&field.name): Vec<$(&field.rust_type)>,
                            } else {
                                pub $(&field.name): $(&field.rust_type),
                            })
                        } else {
                            #[serde(skip_serializing_if = "Option::is_none")]
                            $(if field.is_array {
                                pub $(&field.name): Option<Vec<$(&field.rust_type)>>,
                            } else {
                                pub $(&field.name): Option<$(&field.rust_type)>,
                            })
                        })
                    })
                })
            )
            $(for child in child_dtos join ($['\r']) =>
                $(if child.is_array {
                    pub $(&child.field_name): Vec<$(&child.struct_name)Response>,
                } else {
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub $(&child.field_name): Option<$(&child.struct_name)Response>,
                })
            )
            $(if ctx.has_workflow {
                $(doc("/// Current workflow state for this entity."))
                #[serde(skip_serializing_if = "Option::is_none")]
                pub workflow_state: Option<$(entity)WorkflowState>,
            })
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub updated_at: chrono::DateTime<chrono::Utc>,
            $(for extra in extras join ($['\r']) =>
                $(if let Some(rust_type) = &extra.rust_type {
                    $(doc("/// Computed by the mox domain model (derived) — read-only."))
                    #[serde(default)]
                    pub $(&extra.name): $rust_type,
                })
            )
        }
        $(for child in children {
            $['\n']
            #[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
            $(if camel => #[serde(rename_all = "camelCase")])
            pub struct $(&child.struct_name)Response {
                $(for field in &child.fields join ($['\r']) =>
                    $(if field.is_required {
                        pub $(&field.name): $(&field.rust_type),
                    } else {
                        #[serde(skip_serializing_if = "Option::is_none")]
                        pub $(&field.name): Option<$(&field.rust_type)>,
                    })
                )
                $(for nested in &child.child_dtos join ($['\r']) =>
                    $(if nested.is_array {
                        pub $(&nested.field_name): Vec<$(&nested.struct_name)Response>,
                    } else {
                        #[serde(skip_serializing_if = "Option::is_none")]
                        pub $(&nested.field_name): Option<$(&nested.struct_name)Response>,
                    })
                )
            }
        })
        $(if ctx.has_workflow {
            $['\n']
            $(doc("/// Workflow state summary embedded in the response DTO."))
            #[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
            $(if camel => #[serde(rename_all = "camelCase")])
            pub struct $(entity)WorkflowState {
                $(doc("/// Current workflow status value."))
                pub current_state: String,
                $(if ctx.has_approval_status {
                    $(doc("/// Current approval status value."))
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub approval_state: Option<String>,
                })
                $(doc("/// Whether the entity is in a terminal workflow state."))
                pub is_terminal: bool,
                $(doc("/// Valid target states from the current state."))
                pub available_transitions: Vec<String>,
            }
        })
        $(if !extras.is_empty() {
            $['\n']
            impl $(entity)Response {
                $(for extra in extras join ($['\r']) =>
                    $(doc(&format!("/// Computed accessor for the mox derived feature `{}`.", extra.source_name)))
                    $(doc("/// The `res` arena (issue #508 R4) is the domain's real"))
                    $(doc(&format!("/// `crate::{}::mox_types::Resource`.", ctx.domain)))
                    $(if let Some(rust_type) = &extra.rust_type {
                        pub fn $(&extra.name)(&self, res: &crate::$(domain)::mox_types::Resource) -> $rust_type {
                            $(&extra.body)
                        }
                    } else {
                        pub fn $(&extra.name)(&self, res: &crate::$(domain)::mox_types::Resource) -> () {
                            $(&extra.body)
                        }
                    })
                )
            }
        })
    }
}

/// genco port using `rust::import` for the crate-level imports — the feature
/// #489 is actually about. Output is NOT byte-identical to the Tera render
/// (genco owns grouping/ordering), so it is timed and reported separately.
fn genco_managed_imports(ctx: &DtoContext, camel: bool) -> rust::Tokens {
    let serde_de = rust::import("serde", "Deserialize");
    let serde_ser = rust::import("serde", "Serialize");
    let schema = rust::import("utoipa", "ToSchema");
    let uuid = &rust::import("uuid", "Uuid");
    let chrono_utc = &rust::import("chrono", "Utc");
    let chrono_datetime = &rust::import("chrono", "DateTime");

    let entity = ctx.entity_name.as_str();
    let fields = &ctx.fields;

    quote! {
        $(doc(&format!("/// Response body for {}.", ctx.entity_name)))
        #[derive(Debug, Clone, Default, $serde_ser, $serde_de, $schema)]
        $(if camel => #[serde(rename_all = "camelCase")])
        pub struct $(entity)Response {
            pub id: $uuid,
            $(for field in fields join ($['\r']) =>
                $(if !field.name.eq("id") {
                    $(if field.is_required {
                        pub $(&field.name): $(&*field.rust_type),
                    } else {
                        #[serde(skip_serializing_if = "Option::is_none")]
                        pub $(&field.name): Option<$(&*field.rust_type)>,
                    })
                })
            )
            pub created_at: $chrono_datetime<$chrono_utc>,
            pub updated_at: $chrono_datetime<$chrono_utc>,
        }
    }
}

// ---------------------------------------------------------------------------
// Timing harness
// ---------------------------------------------------------------------------

const WARMUP: usize = 5;
const ITERATIONS: usize = 200;

struct Sample {
    label: &'static str,
    elapsed: Duration,
    bytes: usize,
}

fn time_it<F: FnMut() -> String>(label: &'static str, mut f: F) -> Sample {
    for _ in 0..WARMUP {
        std::hint::black_box(f());
    }
    let started = Instant::now();
    let mut bytes = 0usize;
    for _ in 0..ITERATIONS {
        let out = f();
        bytes = out.len();
        std::hint::black_box(out);
    }
    Sample {
        label,
        elapsed: started.elapsed(),
        bytes,
    }
}

fn us_per_iter(s: &Sample) -> f64 {
    s.elapsed.as_secs_f64() * 1e6 / ITERATIONS as f64
}

fn mb_per_sec(s: &Sample) -> f64 {
    let total_bytes = (s.bytes * ITERATIONS) as f64;
    total_bytes / s.elapsed.as_secs_f64() / (1024.0 * 1024.0)
}

// ---------------------------------------------------------------------------
// Correctness pins (run in normal PR CI)
// ---------------------------------------------------------------------------

#[test]
fn all_engines_produce_identical_output() {
    let tera = tera_engine();
    let ctx = bench_context();
    let project = bench_project();

    let from_tera = render_tera(&tera, &ctx, &project);
    assert!(!from_tera.is_empty(), "tera render must not be empty");

    let via_legacy = rewrite_with_code_writer_legacy(&from_tera);
    assert_eq!(
        from_tera, via_legacy,
        "legacy CodeWriter must round-trip the rendered document"
    );

    let via_code_writer = rewrite_with_code_writer(&from_tera);
    assert_eq!(
        from_tera, via_code_writer,
        "CodeWriter::push_line_fmt must round-trip the rendered document"
    );

    let genco_literal = genco_literal_imports(&ctx, true)
        .to_file_string()
        .expect("genco renders");
    assert_eq!(
        normalize(&from_tera),
        normalize(&genco_literal),
        "genco (literal imports) must produce the same document;\n--- tera ---\n{}\n--- genco ---\n{}",
        from_tera,
        genco_literal
    );
}

/// Pins what genco's import management actually achieves on *this* emitters'
/// shape — and, more importantly, what it does not.
///
/// The finding: `rust::import` only manages types you route through it. Our
/// emitters build type strings dynamically (`field.rust_type`), so those bypass
/// management entirely and stay fully qualified. Retiring `resolve_imports`
/// would therefore require re-plumbing every dynamically-derived type through
/// `rust::import` — a much larger change than "swap the template engine".
#[test]
fn genco_managed_imports_reach_the_same_type_surface() {
    let ctx = bench_context();
    let out = genco_managed_imports(&ctx, true)
        .to_file_string()
        .expect("genco renders");

    // Imports routed through `rust::import` are generated, deduped, grouped by
    // path, and re-ORDERED (genco sorts by path — chrono before serde, where
    // the template emitted serde first). Ordering is a byte-level diff that
    // every converted generator would have to absorb.
    for needle in [
        "use serde::{Deserialize, Serialize};",
        "use utoipa::ToSchema;",
        "use uuid::Uuid;",
        "use chrono::{DateTime, Utc};",
        "pub struct CandidateResponse {",
    ] {
        assert!(
            out.contains(needle),
            "managed-import genco output is missing {needle:?}:\n{out}"
        );
    }

    let chrono_at = out.find("use chrono::").expect("chrono block emitted");
    let serde_at = out.find("use serde::").expect("serde block emitted");
    assert!(
        chrono_at < serde_at,
        "genco sorts import blocks by path, which reorders the template's order:\n{out}"
    );

    // The limitation, pinned: a dynamic type string is emitted verbatim and
    // gets no import at all.
    assert!(
        out.contains("Option<chrono::NaiveDate>"),
        "expected the dynamic type string to appear fully qualified:\n{out}"
    );
    assert!(
        !out.contains("use chrono::NaiveDate;"),
        "genco must not have magically imported the dynamic type:\n{out}"
    );
}

/// Collapse whitespace so structural equivalence can be asserted without
/// pinning genco's exact indentation/blank-line choices.
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// The benchmark (opt-in: produces numbers, never fails on timing)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "benchmark: run explicitly with --ignored --nocapture"]
fn genco_vs_tera_benchmark() {
    let tera = tera_engine();
    let ctx = bench_context();
    let project = bench_project();

    let reference = render_tera(&tera, &ctx, &project);
    let reference_lines = reference.lines().count();
    let reference_bytes = reference.len();

    let mut prebuilt_context = tera::Context::from_serialize(&ctx).expect("context serializes");
    prebuilt_context.insert("project", &project);

    let samples = vec![
        time_it("A tera (serialize+render)", || {
            render_tera(&tera, &ctx, &project)
        }),
        time_it("A2 tera (render only, ctx prebuilt)", || {
            tera.render("domain_types/dto_response.tera", &prebuilt_context)
                .expect("dto_response renders")
        }),
        time_it("B CodeWriter legacy (format!/line)", || {
            rewrite_with_code_writer_legacy(&reference)
        }),
        time_it("C CodeWriter push_line_fmt (shipped)", || {
            rewrite_with_code_writer(&reference)
        }),
        time_it("D genco (literal imports)", || {
            genco_literal_imports(&ctx, true)
                .to_file_string()
                .expect("genco renders")
        }),
        time_it("E genco (managed imports)", || {
            genco_managed_imports(&ctx, true)
                .to_file_string()
                .expect("genco renders")
        }),
    ];

    let tera_us = us_per_iter(&samples[0]);

    eprintln!();
    eprintln!("== issue #489 phase 0: genco vs tera ==");
    eprintln!(
        "workload: domain_types/dto_response.tera, {} lines / {} bytes per file",
        reference_lines, reference_bytes
    );
    eprintln!("iterations: {ITERATIONS} (warmup {WARMUP})");
    eprintln!();
    eprintln!(
        "{:<34} {:>10} {:>11} {:>11} {:>9}",
        "contestant", "us/file", "vs tera", "MB/s", "bytes"
    );
    for s in &samples {
        eprintln!(
            "{:<34} {:>10.2} {:>10.2}x {:>11.1} {:>9}",
            s.label,
            us_per_iter(s),
            tera_us / us_per_iter(s),
            mb_per_sec(s),
            s.bytes
        );
    }

    // Projected cost at the review fixture's scale (418 Rust files / 1.46 MB).
    let file_count = 418.0f64;
    eprintln!();
    eprintln!("projected over {file_count:.0} files (review-fixture scale):");
    for s in &samples {
        eprintln!(
            "{:<34} {:>10.2} ms total",
            s.label,
            us_per_iter(s) * file_count / 1000.0
        );
    }
    eprintln!();
    eprintln!(
        "NOTE: A is the only contestant that also builds the serialized context;\n\
         B and C re-emit A's bytes, so they measure buffer cost alone."
    );
    eprintln!();
}
