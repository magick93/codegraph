//! WP1.5 — namespace probe (see docs/rosetta/findings/wp1.5-namespaces.md).
//!
//! Question: namespaces vs dir-derived domains — what does the disposition
//! table assume (#267/#268)?
//!
//! Today's model has NO namespace axis in the JSON-schema pipeline. Domain
//! identity is derived from the schema FILE PATH (`extract_domain_from_path`,
//! crates/codegraph/src/ingest/schema_loader.rs:320-330 — the path segment
//! immediately before `/json/`), carried on `SchemaNode.domain`
//! (crates/codegraph-core/src/types/schema.rs:17), and enforced by title
//! dedup in `compute_generation_order`
//! (crates/codegraph-generate/src/lib.rs:2176+): the first domain in
//! topological order to claim a title owns it, and later domains emit
//! nothing for that title. These probes pin that behavior so the Rosetta
//! disposition table can state the delta against the #267/#268 uplift model
//! (namespace = visibility/scoping axis, first-class; domain =
//! ownership/boundary axis, unchanged).
//!
//! Naming collision: a `NamespaceNode` type ALREADY EXISTS with different
//! semantics — AT-Protocol repo namespaces
//! (crates/codegraph-core/src/types/atproto.rs:4-8: authority/segment/domain,
//! wired via `EdgeType::InNamespace`). See the findings file.

use crate::support;
use codegraph::driver;
use std::fs;
use std::path::Path;

/// A customer entity. Deliberately carries NO domain/namespace marker in its
/// content — domain identity must come from the directory only.
const CUSTOMER_SCHEMA: &str = r#"{
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "CustomerType",
    "description": "A customer.",
    "type": "object",
    "properties": {
        "id": {
            "description": "Primary identifier.",
            "type": "string",
            "format": "uuid"
        },
        "name": {
            "description": "Customer name.",
            "type": "string"
        }
    },
    "required": ["id", "name"]
}"#;

/// A postal address entity living in the `common` domain.
const ADDRESS_SCHEMA: &str = r#"{
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "AddressType",
    "description": "A postal address.",
    "type": "object",
    "properties": {
        "id": {
            "description": "Primary identifier.",
            "type": "string",
            "format": "uuid"
        },
        "line1": {
            "description": "Street line.",
            "type": "string"
        }
    },
    "required": ["id", "line1"]
}"#;

/// A billing customer entity referencing the COMMON-domain address entity
/// across the directory boundary via a relative `$ref`
/// (`../common/json/AddressType.json`; `resolve_ref` normalizes `..`
/// segments in memory — schema_loader.rs:246-256, 332-345).
const BILLING_CUSTOMER_SCHEMA: &str = r#"{
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "CustomerType",
    "description": "A customer.",
    "type": "object",
    "properties": {
        "id": {
            "description": "Primary identifier.",
            "type": "string",
            "format": "uuid"
        },
        "name": {
            "description": "Customer name.",
            "type": "string"
        },
        "address": {
            "description": "Postal address.",
            "$ref": "../common/json/AddressType.json#"
        }
    },
    "required": ["id", "name", "address"]
}"#;

/// Probe 1 — domain identity is derived from the directory, not the schema
/// content. The fixture file lives at `<billing>/json/CustomerType.json` and
/// its content never mentions "billing", yet every generated artifact
/// carries the billing domain (entity module `src/domain/billing/customer/`,
/// DDL table `billing.customer`).
#[tokio::test]
async fn domain_is_dir_derived_from_path() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        !CUSTOMER_SCHEMA.contains("billing"),
        "fixture precondition: schema content must not carry the domain marker"
    );
    let files = support::run_pipeline(
        dir.path(),
        "billing",
        &["CustomerType"],
        &[("customer_type.json", CUSTOMER_SCHEMA)],
    )
    .await;
    assert!(
        files.len() > 10,
        "expected a real generated tree, got {} files",
        files.len()
    );

    // Entity artifacts live under the directory-derived domain.
    let customer_files: Vec<&String> = files
        .keys()
        .filter(|k| k.starts_with("src/domain/billing/customer/"))
        .collect();
    assert!(
        !customer_files.is_empty(),
        "expected entity artifacts under src/domain/billing/customer/, got: {customer_files:?}"
    );

    // The DDL creates the table inside the billing postgres schema.
    let ddl: Vec<(&String, &String)> = files
        .iter()
        .filter(|(k, _)| k.ends_with("_billing_customer.sql"))
        .collect();
    assert_eq!(
        ddl.len(),
        1,
        "expected exactly one billing customer DDL migration, got: {:?}",
        ddl.iter().map(|(k, _)| k).collect::<Vec<_>>()
    );
    let (ddl_path, ddl_sql) = ddl[0];
    assert!(
        ddl_sql.contains("CREATE TABLE IF NOT EXISTS billing.customer"),
        "{ddl_path} should schema-qualify the table as billing.customer:\n{ddl_sql}"
    );

    // And no OTHER domain ever appears in source layout (single-domain run;
    // `src/domain/mod.rs` is the umbrella module, not a domain).
    let rogue_domains: Vec<&String> = files
        .keys()
        .filter(|k| {
            k.starts_with("src/domain/")
                && !k.starts_with("src/domain/billing/")
                && *k != "src/domain/mod.rs"
        })
        .collect();
    assert!(
        rogue_domains.is_empty(),
        "single-domain run leaked artifacts into other domains: {rogue_domains:?}"
    );
}

/// Probe 2 — the SAME title in TWO domains dedupes to the domain that comes
/// first in generation order; the loser domain produces NO artifact for that
/// title. Observed winner: `common`.
///
/// Why `common` wins (compute_generation_order, lib.rs:2176-2332 +
/// registry.rs:40-169): `DomainRegistry::from_config` sorts domain names
/// ALPHABETICALLY when assigning node indices (registry.rs:47-48), Kahn's
/// algorithm then emits indices in ascending order, and `topological_order`
/// REVERSES the result (registry.rs:162-164, so dependencies sort first).
/// With no `depends_on` edges between `billing` and `common` that reversal
/// yields descending alphabetical order — [common, billing] — so `common`
/// claims `CustomerType` via `seen_titles` first. Both domains declare the
/// title explicitly in `entities = [...]`, and an explicit claim is never
/// reassigned (lib.rs:2298-2329 reassignment requires the first claim to be
/// graph-discovery-only), so billing's claim is silently dropped.
///
/// Note this means the winner is decided by the alphabetical-sort +
/// reversal mechanics, NOT by domains.toml declaration order (common is
/// also declared first here, but that is coincidence, not cause).
#[tokio::test]
async fn same_title_in_two_domains_deduped_to_first() {
    let dir = tempfile::tempdir().unwrap();
    let p = same_title_project(dir.path());
    driver::run(support::driver_args(&p)).await.unwrap();
    let files = support::collect_files(&p.output);

    // Exactly ONE customer entity module exists in the whole tree, and it
    // belongs to the winning domain (common).
    let customer_modules: Vec<&String> = files
        .keys()
        .filter(|k| k.starts_with("src/domain/") && k.contains("/customer/"))
        .collect();
    assert!(
        !customer_modules.is_empty(),
        "expected a customer entity module somewhere in src/domain/"
    );
    let loser_modules: Vec<&String> = customer_modules
        .iter()
        .filter(|k| !k.starts_with("src/domain/common/customer/"))
        .copied()
        .collect();
    assert!(
        loser_modules.is_empty(),
        "title dedup leaked customer artifacts into a losing domain: {loser_modules:?}"
    );

    // Exactly ONE customer DDL migration exists in the whole tree — the
    // common one. The billing table billing.customer is never created.
    let customer_ddl: Vec<&String> = files
        .keys()
        .filter(|k| k.ends_with("_common_customer.sql"))
        .collect();
    assert_eq!(
        customer_ddl.len(),
        1,
        "expected exactly one common customer DDL migration, got: {customer_ddl:?}"
    );
    let billing_ddl: Vec<&String> = files
        .keys()
        .filter(|k| k.ends_with("_billing_customer.sql"))
        .collect();
    assert!(
        billing_ddl.is_empty(),
        "the dedup loser must not emit DDL for the claimed title; got: {billing_ddl:?}"
    );

    // The loser domain produces NO artifact for that title — but it is not
    // erased from the tree entirely (domain-level scaffolding may still run
    // for it). Characterize exactly what billing does get:
    let billing_files: Vec<&String> = files.keys().filter(|k| k.contains("billing")).collect();
    assert!(
        billing_files.iter().all(|k| !k.contains("customer")),
        "billing must not produce any customer artifact; got: {billing_files:?}"
    );
    eprintln!(
        "wp1.5: billing (dedup loser) still produced {} non-customer artifact(s): {billing_files:?}",
        billing_files.len()
    );
}

/// Probe 3 — the versioned HR-Open layout `<version>/<domain>/json/...`
/// maps to the DOMAIN segment (the one immediately before `/json/`), not the
/// version segment. Pinned at `schemas/v1/billing/json/CustomerType.json` →
/// all artifacts under the billing domain.
#[tokio::test]
async fn versioned_layout_maps_to_domain_segment() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_versioned_project(dir.path());
    driver::run(support::driver_args(&p)).await.unwrap();
    let files = support::collect_files(&p.output);
    assert!(
        files.len() > 10,
        "expected a real generated tree, got {} files",
        files.len()
    );

    let customer_files: Vec<&String> = files
        .keys()
        .filter(|k| k.starts_with("src/domain/billing/customer/"))
        .collect();
    assert!(
        !customer_files.is_empty(),
        "versioned layout v1/billing/json must map to domain 'billing'; got customer artifacts: {customer_files:?}"
    );
    let versioned_leak: Vec<&String> = files
        .keys()
        .filter(|k| k.starts_with("src/domain/v1/") || k.contains("_v1_"))
        .collect();
    assert!(
        versioned_leak.is_empty(),
        "the version segment must not become a domain; leaked: {versioned_leak:?}"
    );
}

/// Probe 4 (SURPRISE FINDING) — a cross-domain `$ref` (billing CustomerType →
/// common AddressType) with NO `depends_on` declaration is NOT rejected:
/// `driver::run` succeeds and the cross-schema FK is generated anyway.
///
/// This pins a dead validation path: `fk_target_undeclared_dependency`
/// (crates/codegraph/src/validate.rs:143-214) looks up the property's
/// `ref_target` in a title-keyed entity→domain map, but the JSON-schema
/// ingest stores `ref_target` as the RAW `$ref` string
/// (`"../common/json/AddressType.json#"` — async_ingest.rs:1019), never the
/// resolved title, so the lookup at validate.rs:192 always misses and the
/// check is silently skipped (validate.rs:209). Cross-domain visibility is
/// therefore completely unenforced today — exactly the gap the #267/#268
/// namespace-visibility model would close.
#[tokio::test]
async fn undeclared_cross_domain_fk_is_not_flagged_and_still_generates_fk() {
    let dir = tempfile::tempdir().unwrap();
    let p = cross_domain_project(dir.path(), false);

    // NO validation failure — the fk_target_undeclared_dependency check
    // cannot fire for path-style refs (see doc comment above).
    driver::run(support::driver_args(&p))
        .await
        .expect("surprise: undeclared cross-domain FK generates without error");

    let files = support::collect_files(&p.output);
    let customer_ddl: Vec<(&String, &String)> = files
        .iter()
        .filter(|(k, _)| k.ends_with("_billing_customer.sql"))
        .collect();
    assert_eq!(
        customer_ddl.len(),
        1,
        "expected exactly one billing customer DDL migration"
    );
    let (ddl_path, ddl_sql) = customer_ddl[0];
    assert!(
        ddl_sql.contains("REFERENCES common.address("),
        "even without depends_on, {ddl_path} carries the cross-schema FK:\n{ddl_sql}"
    );
}

/// Probe 4b — with `depends_on = ["common"]` declared, the same model
/// generates the billing customer table with an FK into the common address
/// table (`REFERENCES common.address(id)`). There is NO generated
/// "DomainDepends" artifact: the dependency edge lives only in domains.toml
/// and in the migration ordering; nothing in the generated tree records it
/// (the sole `depends_on` occurrence is incidental docker-compose service
/// wiring in the e2e scaffold, not a domain-dependency artifact).
#[tokio::test]
async fn cross_domain_reference_generates_fk() {
    let dir = tempfile::tempdir().unwrap();
    let p = cross_domain_project(dir.path(), true);
    driver::run(support::driver_args(&p)).await.unwrap();
    let files = support::collect_files(&p.output);

    // Both sides exist, in their own domains.
    assert!(
        files
            .keys()
            .any(|k| k.starts_with("src/domain/common/address/")),
        "expected the common address entity module"
    );
    assert!(
        files
            .keys()
            .any(|k| k.starts_with("src/domain/billing/customer/")),
        "expected the billing customer entity module"
    );

    // The billing customer DDL carries the cross-schema FK.
    let customer_ddl: Vec<(&String, &String)> = files
        .iter()
        .filter(|(k, _)| k.ends_with("_billing_customer.sql"))
        .collect();
    assert_eq!(
        customer_ddl.len(),
        1,
        "expected exactly one billing customer DDL migration"
    );
    let (ddl_path, ddl_sql) = customer_ddl[0];
    assert!(
        ddl_sql.contains("REFERENCES common.address("),
        "{ddl_path} should FK into the common address table:\n{ddl_sql}"
    );

    // And the dependency is NOT reified anywhere in the output: no
    // DomainDepends artifact, no generated manifest of depends_on.
    let depends_artifacts: Vec<&String> = files
        .iter()
        .filter(|(k, content)| {
            (content.contains("depends_on") || content.contains("depends-on"))
                && !k.starts_with("e2e-tests/docker-compose")
        })
        .map(|(k, _)| k)
        .collect();
    assert!(
        depends_artifacts.is_empty(),
        "depends_on should stay config-only; surfaced in: {depends_artifacts:?}"
    );
}

/// Project with schemas at `<root>/schemas/v1/billing/json/…` (version
/// segment BEFORE the domain, per extract_domain_from_path's contract).
fn write_versioned_project(root: &Path) -> support::SchemaProject {
    let schemas = root.join("schemas");
    let json_dir = schemas.join("v1").join("billing").join("json");
    fs::create_dir_all(&json_dir).unwrap();
    fs::write(
        schemas.join("domains.toml"),
        support::domains_toml("billing", &["CustomerType"]),
    )
    .unwrap();
    fs::write(schemas.join("classifier.toml"), support::CLASSIFIER_TOML).unwrap();
    fs::write(json_dir.join("customer_type.json"), CUSTOMER_SCHEMA).unwrap();
    support::SchemaProject {
        root: root.to_path_buf(),
        config: schemas.join("domains.toml"),
        schemas: schemas.clone(),
        classifier: schemas.join("classifier.toml"),
        output: root.join("generated"),
    }
}

/// Two-domain project where BOTH domains carry a `CustomerType.json` and
/// both declare it explicitly as an entity (no depends_on either way).
/// Written with a local TOML (support::add_domain_entry appends a second
/// `[defaults]` header, which TOML rejects as a duplicate key).
fn same_title_project(root: &Path) -> support::SchemaProject {
    let schemas = root.join("schemas");
    let common_json = schemas.join("common").join("json");
    let billing_json = schemas.join("billing").join("json");
    fs::create_dir_all(&common_json).unwrap();
    fs::create_dir_all(&billing_json).unwrap();
    fs::write(
        schemas.join("domains.toml"),
        "[defaults]\n\
         operations = [\"create\", \"read\", \"update\", \"delete\", \"list\"]\n\n\
         [domains.common]\n\
         label = \"common\"\n\
         schema_dir = \"common\"\n\
         postgres_schema = \"common\"\n\
         entities = [\"CustomerType\"]\n\n\
         [domains.billing]\n\
         label = \"billing\"\n\
         schema_dir = \"billing\"\n\
         postgres_schema = \"billing\"\n\
         entities = [\"CustomerType\"]\n",
    )
    .unwrap();
    fs::write(schemas.join("classifier.toml"), support::CLASSIFIER_TOML).unwrap();
    fs::write(common_json.join("customer_type.json"), CUSTOMER_SCHEMA).unwrap();
    fs::write(billing_json.join("customer_type.json"), CUSTOMER_SCHEMA).unwrap();
    support::SchemaProject {
        root: root.to_path_buf(),
        config: schemas.join("domains.toml"),
        schemas: schemas.clone(),
        classifier: schemas.join("classifier.toml"),
        output: root.join("generated"),
    }
}

/// Two-domain project: common owns AddressType, billing owns CustomerType
/// which `$ref`s `../common/json/AddressType.json`. `depends_on` toggles the
/// declared cross-domain dependency.
fn cross_domain_project(root: &Path, depends_on: bool) -> support::SchemaProject {
    let schemas = root.join("schemas");
    let common_json = schemas.join("common").join("json");
    let billing_json = schemas.join("billing").join("json");
    fs::create_dir_all(&common_json).unwrap();
    fs::create_dir_all(&billing_json).unwrap();

    let mut billing = String::from(
        "[domains.billing]\nlabel = \"billing\"\nschema_dir = \"billing\"\npostgres_schema = \"billing\"\n",
    );
    if depends_on {
        billing.push_str("depends_on = [\"common\"]\n");
    }
    billing.push_str("entities = [\"CustomerType\"]\n");

    let toml = format!(
        "[defaults]\noperations = [\"create\", \"read\", \"update\", \"delete\", \"list\"]\n\n\
         [domains.common]\nlabel = \"common\"\nschema_dir = \"common\"\npostgres_schema = \"common\"\nentities = [\"AddressType\"]\n\n{billing}"
    );
    fs::write(schemas.join("domains.toml"), toml).unwrap();
    fs::write(schemas.join("classifier.toml"), support::CLASSIFIER_TOML).unwrap();
    fs::write(common_json.join("address_type.json"), ADDRESS_SCHEMA).unwrap();
    fs::write(
        billing_json.join("customer_type.json"),
        BILLING_CUSTOMER_SCHEMA,
    )
    .unwrap();

    support::SchemaProject {
        root: root.to_path_buf(),
        config: schemas.join("domains.toml"),
        schemas: schemas.clone(),
        classifier: schemas.join("classifier.toml"),
        output: root.join("generated"),
    }
}
