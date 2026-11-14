//! Codegen throughput benchmark and buffer regression pin (issue #489).
//!
//! Run the numbers with:
//!
//! ```bash
//! cargo test --release -p codegraph --test codegen_bench -- --include-ignored --nocapture
//! ```
//!
//! # Background
//!
//! Issue #489 proposed replacing Tera with the `genco` quasiquoter. Phase 0
//! measured that head-to-head and declined the migration — the results are
//! recorded in [`docs/genco-evaluation.md`] and on the issue. The measurement
//! itself needed three things kept apart, and this file keeps them apart:
//!
//! 1. **Context serialization vs render.** `tera::Context::from_serialize`
//!    does not appear in any template-level profile, yet it turned out to be
//!    ~74% of the current template path. Comparing a typed emitter against
//!    Tera's *whole* path misattributes that cost to "Tera is slow".
//! 2. **Our own buffer vs the template engine.** `wln!` used to build a
//!    `String` per emitted line. That is our defect, not Tera's, and fixing it
//!    is a ~20-line change.
//! 3. **Equal work.** Every contestant here produces byte-identical output,
//!    pinned by [`all_engines_produce_identical_output`], so the timings
//!    compare the same document rather than "a similar one".
//!
//! # Contestants
//!
//! | # | Contestant | What it represents |
//! |---|------------|--------------------|
//! | A | Tera — `Context::from_serialize` + render | today's template path |
//! | A2 | Tera — render only (context prebuilt) | the template engine alone |
//! | B | `CodeWriter` legacy (`format!` per line) | the pre-#489 buffer |
//! | C | `CodeWriter::push_line_fmt` | the shipped #489 buffer |
//!
//! [`docs/genco-evaluation.md`]: ../docs/genco-evaluation.md
//!
//! The `genco` contestants that produced the #489 finding are not carried in
//! the repo: keeping a benchmark-only dependency was judged not worth it, and
//! the measured figures (0.92x Tera's renderer, 4.7x slower than the shipped
//! `CodeWriter`) are recorded in the evaluation doc and on the issue.

use std::time::{Duration, Instant};

use codegraph_generate::DtoKeyCasing;
use codegraph_generate::ProjectConfig;
use codegraph_generate::code_writer::CodeWriter;
use codegraph_generate::ddd::dto::{ChildDtoContext, DtoContext, DtoField, MoxDerivedExtra};
use codegraph_generate::render_template_with_project;
use codegraph_generate::template_engine::create_tera_with_overrides;

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

    let contact = ChildDtoContext {
        field_name: "contact".to_string(),
        struct_name: "Contact".to_string(),
        fields: vec![
            dto_field("line1", "String", true, false, false),
            dto_field("line2", "String", false, false, false),
            dto_field("city", "String", true, false, false),
            dto_field("postcode", "String", false, false, false),
        ],
        is_array: false,
        child_dtos: vec![ChildDtoContext {
            field_name: "primary_phone".to_string(),
            struct_name: "Phone".to_string(),
            fields: vec![
                dto_field("kind", "String", true, false, false),
                dto_field("value", "String", false, false, false),
            ],
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
// Byte-identity pin (runs in normal PR CI)
// ---------------------------------------------------------------------------

/// The buffer fast path must be byte-identical to both the allocating form
/// and the template it replaces. This is the regression guard for the #489
/// `CodeWriter` change: it would fail the moment the two paths diverge.
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
}

/// The document the benchmark measures is a realistic DTO response: structs,
/// serde attributes, nested child DTOs, and a workflow section. If the
/// template regresses into something trivial the benchmark stops being
/// representative, so pin the shape rather than the exact bytes.
#[test]
fn benchmark_workload_covers_the_template_branches() {
    let tera = tera_engine();
    let ctx = bench_context();
    let out = render_tera(&tera, &ctx, &bench_project());

    for needle in [
        "pub struct CandidateResponse {",
        "pub struct ContactResponse {",
        "pub struct AddressResponse {",
        "pub struct PhoneResponse {",
        "pub struct CandidateWorkflowState {",
        "#[serde(rename_all = \"camelCase\")]",
        "#[serde(skip_serializing_if = \"Option::is_none\")]",
        "pub screening_ids: Option<Vec<uuid::Uuid>>,",
        "pub addresses: Vec<AddressResponse>,",
        "pub fn score(&self, res: &crate::recruiting::mox_types::Resource) -> f64",
    ] {
        assert!(out.contains(needle), "workload is missing {needle:?}");
    }
}

// ---------------------------------------------------------------------------
// The benchmark (opt-in: produces numbers, never fails on timing)
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

#[test]
#[ignore = "benchmark: run explicitly with --include-ignored --nocapture"]
fn codegen_benchmark() {
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
    ];

    let tera_us = us_per_iter(&samples[0]);

    eprintln!();
    eprintln!("== codegen engine benchmark (issue #489) ==");
    eprintln!(
        "workload: domain_types/dto_response.tera, {} lines / {} bytes per file",
        reference_lines, reference_bytes
    );
    eprintln!("iterations: {ITERATIONS} (warmup {WARMUP})");
    eprintln!();
    eprintln!(
        "{:<40} {:>10} {:>11} {:>11} {:>9}",
        "contestant", "us/file", "vs full", "MB/s", "bytes"
    );
    for s in &samples {
        eprintln!(
            "{:<40} {:>10.2} {:>10.2}x {:>11.1} {:>9}",
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
            "{:<40} {:>10.2} ms total",
            s.label,
            us_per_iter(s) * file_count / 1000.0
        );
    }
    eprintln!();
    eprintln!(
        "NOTE: A is the only contestant that also builds the serialized context\n\
         (~74% of A); B and C re-emit A's bytes, so they measure buffer cost alone.\n\
         genco figures from the #489 evaluation are recorded in docs/genco-evaluation.md."
    );
    eprintln!();
}
