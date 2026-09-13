//! Tests for generated Playwright CRUD-test entity-ref dependency setup.
//!
//! Regression coverage for the `beforeAll` dep setup that previously emitted
//! empty bodies (`.schema.json` stems not stripped), skipped ref fields, never
//! created transitive required FKs, and silently swallowed failures.
//!
//! Slices:
//! - A: scalar same-domain + cross-domain `.schema.json` refs resolve
//! - B: array refs assign an array id and testData sends the array
//! - C: required-only transitive closure is ordered leaf-first with FK wiring
//! - D: unsatisfiable/cyclic required deps throw; optional refs are not created;
//!   cleanup is reversed and array-aware

use std::path::Path;

use codegraph::generate::traits::EntityGenerator;
use codegraph::generate::ui::e2e_test::UiE2eTestGenerator;
use codegraph::generate::ProjectConfig;
use codegraph_config::config::parse_domain_config_str;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};
use codegraph_type_contracts::RefClassificationKind;

// ── Fixture helpers ─────────────────────────────────────────────────────

fn schema(
    title: &str,
    table: &str,
    rust_type_name: &str,
    domain: &str,
    segment: &str,
) -> SchemaNode {
    SchemaNode {
        custom_annotations: Default::default(),
        schema_id: format!("{domain}/json/{table}.schema.json"),
        title: title.into(),
        description: None,
        schema_type: "object".into(),
        classification: "entity_reference".into(),
        domain: Some(domain.into()),
        rel_path: format!("{domain}/json/{table}.schema.json"),
        pg_type: "UUID".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: rust_type_name.into(),
        pg_table_name: table.into(),
        api_path_segment: segment.into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    }
}

fn scalar(name: &str, pg_type: &str, is_required: bool) -> PropertyNode {
    PropertyNode {
        name: name.into(),
        prop_type: "string".into(),
        description: None,
        format: None,
        is_required,
        is_nullable: !is_required,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.into(),
        pg_column_type: pg_type.into(),
        rust_field_name: name.into(),
        rust_field_type: "String".into(),
        sea_orm_type: "Text".into(),
        render_strategy: "direct_column".into(),
        ref_target: None,
        classification: Some("primitive_wrapper".into()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

/// A plain `format: uuid` scalar column that is NOT a `$ref`/graph edge —
/// e.g. `party.case_id`. The residual defect: these were never resolved as
/// entity refs and serialized as `'Test Case Id'` literals.
fn plain_uuid(name: &str, is_required: bool) -> PropertyNode {
    PropertyNode {
        name: name.into(),
        prop_type: "string".into(),
        description: None,
        format: Some("uuid".into()),
        is_required,
        is_nullable: !is_required,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.into(),
        pg_column_type: "UUID".into(),
        rust_field_name: name.into(),
        rust_field_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        render_strategy: "direct_column".into(),
        ref_target: None,
        classification: Some("primitive_wrapper".into()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

fn entity_ref(name: &str, ref_target: &str, is_array: bool, is_required: bool) -> PropertyNode {
    PropertyNode {
        name: name.into(),
        prop_type: if is_array {
            "array".into()
        } else {
            "object".into()
        },
        description: None,
        format: None,
        is_required,
        is_nullable: !is_required,
        is_array,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.into(),
        pg_column_type: "UUID".into(),
        rust_field_name: name.into(),
        rust_field_type: if is_array {
            "Vec<Uuid>".into()
        } else {
            "Uuid".into()
        },
        sea_orm_type: "Uuid".into(),
        render_strategy: "entity_reference".into(),
        ref_target: Some(ref_target.into()),
        classification: Some("entity_reference".into()),
        projection: None,
        classification_kind: Some(RefClassificationKind::EntityReference),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

fn config(entities: &[(&str, &str)]) -> codegraph_config::DomainConfig {
    // entities: (domain, title) pairs; all get full CRUD operations.
    let mut domains: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    for (domain, title) in entities {
        domains.entry(domain).or_default().push(title);
    }
    let mut toml = String::from(
        "[defaults]\noperations = [\"create\", \"read\", \"update\", \"delete\", \"list\"]\n\n",
    );
    for (domain, titles) in domains {
        let list = titles
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(", ");
        toml.push_str(&format!(
            "[domains.{domain}]\nlabel = \"{domain}\"\nschema_dir = \"{domain}\"\npostgres_schema = \"{domain}\"\nentities = [{list}]\n\n"
        ));
        for title in titles {
            toml.push_str(&format!(
                "[domains.{domain}.entity_config.{title}]\nrole = \"root\"\n\n"
            ));
        }
    }
    parse_domain_config_str(&toml).expect("fixture domains.toml must parse")
}

fn tera() -> tera::Tera {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    codegraph::generate::template_engine::create_tera(&template_dir).unwrap()
}

fn generate(
    engine: &MockEngine,
    config: &codegraph_config::DomainConfig,
    title: &str,
    domain: &str,
) -> String {
    let output = tempfile::TempDir::new().unwrap();
    let gen = UiE2eTestGenerator::new(output.path());
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(
            engine,
            title,
            domain,
            config,
            &tera(),
            &ProjectConfig::default(),
        ))
        .expect("UiE2eTestGenerator failed");
    files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with(".owner.crud.test.ts"))
        .expect("owner.crud.test.ts should be generated")
        .content
        .clone()
}

fn owner_crud(
    engine: &MockEngine,
    config: &codegraph_config::DomainConfig,
    title: &str,
    domain: &str,
) -> String {
    generate(engine, config, title, domain)
}

fn after_all_block(content: &str) -> &str {
    match content.find("test.afterAll") {
        Some(idx) => &content[idx..],
        None => "",
    }
}

fn before_all_block(content: &str) -> &str {
    match content.find("test.beforeAll") {
        Some(idx) => &content[idx..],
        None => "",
    }
}

// ── Slice A: scalar refs resolve via graph + `.schema.json` fallback ────

#[test]
fn scalar_refs_resolve_to_non_empty_bodies_and_api_paths() {
    let person = schema("PersonType", "person", "Person", "hr", "person");
    let organization = schema(
        "OrganizationType",
        "organization",
        "Organization",
        "platform",
        "organization",
    );
    let worker = schema("WorkerType", "worker", "Worker", "hr", "worker");
    let engine = MockEngine::builder()
        .with_schema(worker)
        .with_schema(person.clone())
        .with_schema(organization.clone())
        .with_ref_target("person", "WorkerType", person)
        .with_ref_target("organization", "WorkerType", organization)
        .with_properties(
            "WorkerType",
            vec![
                scalar("name", "TEXT", true),
                entity_ref("person", "hr/json/person.schema.json", false, true),
                entity_ref(
                    "organization",
                    "../../platform/json/organization.schema.json",
                    false,
                    true,
                ),
            ],
        )
        .with_properties("PersonType", vec![scalar("full_name", "TEXT", true)])
        .with_properties("OrganizationType", vec![scalar("legal_name", "TEXT", true)])
        .build();
    let config = config(&[
        ("hr", "WorkerType"),
        ("hr", "PersonType"),
        ("platform", "OrganizationType"),
    ]);

    let content = owner_crud(&engine, &config, "WorkerType", "hr");

    // Non-empty dependency bodies (previously `{ }`).
    assert!(
        content.contains(
            "createEntityAsAcme(orgContext, '/hr/person', { 'full_name': 'Test Full Name' })"
        ),
        "person dep body must be non-empty:\n{content}"
    );
    // Same-domain scalar ref assigns a scalar id.
    assert!(
        content.contains("depIds['person_id'] = dep_1['id'] as string;"),
        "scalar ref must assign depIds keyed by the FK field:\n{content}"
    );
    // Cross-domain `.schema.json` ref resolves to `/<domain>/<segment>`.
    assert!(
        content.contains("createEntityAsAcme(orgContext, '/platform/organization'"),
        "cross-domain .schema.json ref must resolve via the graph:\n{content}"
    );
    assert!(
        content.contains("depIds['organization_id'] = dep_2['id'] as string;"),
        "cross-domain scalar ref must assign a depIds key:\n{content}"
    );
    // testData wires the main-entity field from depIds.
    assert!(
        content.contains("...(depIds['person_id'] ? { 'person_id': depIds['person_id'] } : {}),"),
        "testData must emit the entity-ref branch from entity_ref_deps:\n{content}"
    );
    // Required steps must not be wrapped in the old swallowing try/catch.
    assert!(
        !before_all_block(&content).contains("} catch (_e) {"),
        "required dep creation must not swallow failures:\n{content}"
    );
}

// ── Slice B: array refs ─────────────────────────────────────────────────

#[test]
fn array_ref_assigns_array_and_test_data_sends_it() {
    let party = schema("PartyType", "party", "Party", "hr", "party");
    let worker = schema("WorkerType", "worker", "Worker", "hr", "worker");
    let engine = MockEngine::builder()
        .with_schema(worker)
        .with_schema(party.clone())
        .with_ref_target("parties", "WorkerType", party)
        .with_properties(
            "WorkerType",
            vec![
                scalar("name", "TEXT", true),
                entity_ref("parties", "hr/json/party.schema.json", true, true),
            ],
        )
        .with_properties("PartyType", vec![scalar("party_name", "TEXT", true)])
        .build();
    let config = config(&[("hr", "WorkerType"), ("hr", "PartyType")]);

    let content = owner_crud(&engine, &config, "WorkerType", "hr");

    assert!(
        content.contains("depIds['parties'] = [dep_1['id'] as string];"),
        "array ref must assign an array id:\n{content}"
    );
    assert!(
        content.contains("...(depIds['parties'] ? { 'parties': depIds['parties'] } : {}),"),
        "testData must send the array through depIds:\n{content}"
    );
    assert!(
        after_all_block(&content).contains("Array.isArray(ids) ? ids : [ids]"),
        "cleanup must iterate array ids:\n{content}"
    );
}

// ── Slice C: required-only transitive closure ───────────────────────────

#[test]
fn required_closure_is_ordered_leaf_first_with_fk_wiring() {
    let tenant = schema("TenantType", "tenant", "Tenant", "platform", "tenant");
    let case = schema("CaseType", "case", "Case", "hr", "case");
    let party = schema("PartyType", "party", "Party", "hr", "party");
    let enrollment = schema(
        "EnrollmentType",
        "enrollment",
        "Enrollment",
        "hr",
        "enrollment",
    );
    let engine = MockEngine::builder()
        .with_schema(enrollment)
        .with_schema(party.clone())
        .with_schema(case.clone())
        .with_schema(tenant.clone())
        .with_ref_target("party", "EnrollmentType", party)
        .with_ref_target("case", "PartyType", case)
        .with_ref_target("tenant", "PartyType", tenant.clone())
        .with_ref_target("tenant", "CaseType", tenant)
        .with_properties(
            "EnrollmentType",
            vec![
                scalar("name", "TEXT", true),
                entity_ref("party", "hr/json/party.schema.json", false, true),
            ],
        )
        .with_properties(
            "PartyType",
            vec![
                scalar("name", "TEXT", true),
                entity_ref("case", "hr/json/case.schema.json", false, true),
                entity_ref("tenant", "platform/json/tenant.schema.json", false, true),
            ],
        )
        .with_properties(
            "CaseType",
            vec![
                scalar("name", "TEXT", true),
                entity_ref("tenant", "platform/json/tenant.schema.json", false, true),
            ],
        )
        .with_properties("TenantType", vec![scalar("name", "TEXT", true)])
        .build();
    let config = config(&[
        ("hr", "EnrollmentType"),
        ("hr", "PartyType"),
        ("hr", "CaseType"),
        ("platform", "TenantType"),
    ]);

    let content = owner_crud(&engine, &config, "EnrollmentType", "hr");

    let tenant_idx = content
        .find("depIds['tenant_id'] = dep_")
        .expect("tenant dep must be created");
    let case_idx = content
        .find("depIds['case_id'] = dep_")
        .expect("case dep must be created");
    let party_idx = content
        .find("depIds['party_id'] = dep_")
        .expect("party dep must be created");
    assert!(
        tenant_idx < case_idx && case_idx < party_idx,
        "closure must be ordered tenant→case→party:\n{content}"
    );

    assert!(
        content.contains("'tenant_id': depIds['tenant_id']"),
        "case body must fill its tenant FK:\n{content}"
    );
    assert!(
        content.contains("'case_id': depIds['case_id']"),
        "party body must fill its case FK:\n{content}"
    );
    assert!(
        content.contains("'tenant_id': depIds['tenant_id']"),
        "party body must fill its tenant FK:\n{content}"
    );

    // Cleanup runs in reverse creation order.
    let after = after_all_block(&content);
    let after_party = after.find("depIds['party_id']").expect("party cleanup");
    let after_case = after.find("depIds['case_id']").expect("case cleanup");
    let after_tenant = after.find("depIds['tenant_id']").expect("tenant cleanup");
    assert!(
        after_party < after_case && after_case < after_tenant,
        "cleanup must delete in reverse order:\n{after}"
    );
}

// ── Slice D: fail loudly, skip optionals, array-aware cleanup ───────────

#[test]
fn unsatisfiable_required_field_throws() {
    let person = schema("PersonType", "person", "Person", "hr", "person");
    let worker = schema("WorkerType", "worker", "Worker", "hr", "worker");
    let engine = MockEngine::builder()
        .with_schema(worker)
        .with_schema(person.clone())
        .with_ref_target("person", "WorkerType", person)
        .with_properties(
            "WorkerType",
            vec![
                scalar("name", "TEXT", true),
                entity_ref("person", "hr/json/person.schema.json", false, true),
            ],
        )
        // Geometry has no fabricable test value → unsatisfiable required field.
        .with_properties("PersonType", vec![scalar("shape", "GEOMETRY", true)])
        .build();
    let config = config(&[("hr", "WorkerType"), ("hr", "PersonType")]);

    let content = owner_crud(&engine, &config, "WorkerType", "hr");

    assert!(
        content.contains("throw new Error("),
        "unsatisfiable required dep must throw loudly:\n{content}"
    );
    assert!(
        content.contains("required field 'shape' on PersonType"),
        "throw message must be actionable:\n{content}"
    );
    assert!(
        !before_all_block(&content).contains("} catch (_e) {"),
        "required steps must not swallow failures:\n{content}"
    );
}

#[test]
fn required_cycle_throws() {
    let b = schema("BType", "b", "B", "hr", "b");
    let a = schema("AType", "a", "A", "hr", "a");
    let engine = MockEngine::builder()
        .with_schema(a.clone())
        .with_schema(b.clone())
        .with_ref_target("b", "AType", b)
        .with_ref_target("a", "BType", a)
        .with_properties(
            "AType",
            vec![
                scalar("name", "TEXT", true),
                entity_ref("b", "hr/json/b.schema.json", false, true),
            ],
        )
        .with_properties(
            "BType",
            vec![
                scalar("name", "TEXT", true),
                entity_ref("a", "hr/json/a.schema.json", false, true),
            ],
        )
        .build();
    let config = config(&[("hr", "AType"), ("hr", "BType")]);

    let content = owner_crud(&engine, &config, "AType", "hr");

    assert!(
        content.contains("throw new Error("),
        "required cycle must throw loudly:\n{content}"
    );
    assert!(
        content.to_lowercase().contains("cycle"),
        "throw message must mention the cycle:\n{content}"
    );
}

#[test]
fn optional_refs_are_not_created() {
    let person = schema("PersonType", "person", "Person", "hr", "person");
    let worker = schema("WorkerType", "worker", "Worker", "hr", "worker");
    let engine = MockEngine::builder()
        .with_schema(worker)
        .with_schema(person.clone())
        .with_ref_target("person", "WorkerType", person)
        .with_properties(
            "WorkerType",
            vec![
                scalar("name", "TEXT", true),
                entity_ref("person", "hr/json/person.schema.json", false, false),
            ],
        )
        .with_properties("PersonType", vec![scalar("full_name", "TEXT", true)])
        .build();
    let config = config(&[("hr", "WorkerType"), ("hr", "PersonType")]);

    let content = owner_crud(&engine, &config, "WorkerType", "hr");

    assert!(
        !content.contains("createEntityAsAcme(orgContext, '/hr/person'"),
        "optional refs must not be created:\n{content}"
    );
    assert!(
        !content.contains("dep_1"),
        "optional refs must not produce dependency steps:\n{content}"
    );
}

// ── Slice E: convention-resolved plain-uuid FK columns ──────────────────

#[test]
fn convention_uuid_fk_resolves_to_entity() {
    let case = schema("CaseType", "case", "Case", "hr", "case");
    let party = schema("PartyType", "party", "Party", "hr", "party");
    let engine = MockEngine::builder()
        .with_schema(party)
        .with_schema(case)
        .with_properties(
            "PartyType",
            vec![scalar("name", "TEXT", true), plain_uuid("case_id", true)],
        )
        .with_properties("CaseType", vec![scalar("name", "TEXT", true)])
        .build();
    let config = config(&[("hr", "PartyType"), ("hr", "CaseType")]);

    let content = owner_crud(&engine, &config, "PartyType", "hr");

    assert!(
        content.contains("createEntityAsAcme(orgContext, '/hr/case'"),
        "required plain-uuid case_id must resolve to the Case entity:\n{content}"
    );
    assert!(
        content.contains("depIds['case_id'] = dep_"),
        "convention ref must assign a depIds key:\n{content}"
    );
    assert!(
        content.contains("...(depIds['case_id'] ? { 'case_id': depIds['case_id'] } : {}),"),
        "testData must emit the entity-ref branch from the convention ref:\n{content}"
    );
    assert!(
        !content.contains("'Test Case Id'"),
        "resolved convention ref must not emit a literal:\n{content}"
    );
}

#[test]
fn convention_uuid_fk_optional_not_created() {
    let case = schema("CaseType", "case", "Case", "hr", "case");
    let party = schema("PartyType", "party", "Party", "hr", "party");
    let engine = MockEngine::builder()
        .with_schema(party)
        .with_schema(case)
        .with_properties(
            "PartyType",
            vec![scalar("name", "TEXT", true), plain_uuid("case_id", false)],
        )
        .with_properties("CaseType", vec![scalar("name", "TEXT", true)])
        .build();
    let config = config(&[("hr", "PartyType"), ("hr", "CaseType")]);

    let content = owner_crud(&engine, &config, "PartyType", "hr");

    assert!(
        !content.contains("createEntityAsAcme(orgContext, '/hr/case'"),
        "optional convention ref must not be created:\n{content}"
    );
    assert!(
        !content.contains("depIds['case_id']"),
        "optional convention ref must not produce a dep:\n{content}"
    );
}

#[test]
fn convention_uuid_fk_ambiguous_not_resolved() {
    // Two entities normalize to the same `case` stem in different domains;
    // the main entity is in neither domain, so the match stays ambiguous.
    let case_hr = schema("CaseType", "case", "Case", "hr", "case");
    let case_platform = schema("CaseRecordType", "case", "CaseRecord", "platform", "case");
    let party = schema("PartyType", "party", "Party", "identity", "party");
    let engine = MockEngine::builder()
        .with_schema(party)
        .with_schema(case_hr)
        .with_schema(case_platform)
        .with_properties(
            "PartyType",
            vec![scalar("name", "TEXT", true), plain_uuid("case_id", true)],
        )
        .with_properties("CaseType", vec![scalar("name", "TEXT", true)])
        .with_properties("CaseRecordType", vec![scalar("name", "TEXT", true)])
        .build();
    let config = config(&[
        ("identity", "PartyType"),
        ("hr", "CaseType"),
        ("platform", "CaseRecordType"),
    ]);

    let content = owner_crud(&engine, &config, "PartyType", "identity");

    assert!(
        !content.contains("createEntityAsAcme(orgContext, '/hr/case'")
            && !content.contains("createEntityAsAcme(orgContext, '/platform/case'"),
        "ambiguous convention ref must not create a dependency:\n{content}"
    );
    assert!(
        !content.contains("depIds['case_id']"),
        "ambiguous convention ref must not produce a dep:\n{content}"
    );
    assert!(
        !content.contains("'Test Case Id'"),
        "unresolved uuid must fall through to the uuid literal:\n{content}"
    );
}

#[test]
fn unresolved_required_uuid_emits_valid_uuid_literal() {
    let review = schema(
        "ReviewDecisionType",
        "review_decision",
        "ReviewDecision",
        "compliance",
        "review-decision",
    );
    let engine = MockEngine::builder()
        .with_schema(review)
        .with_properties(
            "ReviewDecisionType",
            vec![
                scalar("name", "TEXT", true),
                plain_uuid("reviewer_id", true),
            ],
        )
        .build();
    let config = config(&[("compliance", "ReviewDecisionType")]);

    let content = owner_crud(&engine, &config, "ReviewDecisionType", "compliance");

    assert!(
        !content.contains("'Test Reviewer Id'"),
        "unresolved required uuid must not emit a placeholder string:\n{content}"
    );
    assert!(
        content.contains("00000000-0000-4000-8000-"),
        "unresolved required uuid must emit a UUID-shaped literal:\n{content}"
    );
}
