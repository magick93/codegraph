//! The `hurl_contract` domain generator (issue #463 Phase 1): per-entity
//! hurl API contract files plus the once-per-run authn/authz suite, so every
//! generated app ships integration tests the ops api suite (`testkit api`)
//! runs with zero hand-written test files.
//!
//! Emission contract (pinned byte-for-byte by
//! `crates/codegraph/tests/hurl_contract_tests.rs`):
//!
//! - one file per entity, `{nn:02}_{domain}_{module}.hurl`, numbered from 10
//!   in generation order (domain rank, then dependency-aware title order) —
//!   LIST envelope asserts, CREATE 201 with a `data.id` capture, required FK
//!   parents created first via preceding POSTs with their own captures,
//!   GET-by-id field echo, zero-uuid 404, PUT roundtrip + GET verify,
//!   DELETE 204 + GET 404, `Authorization: Bearer {{api_key}}` everywhere;
//! - the four authn/authz files, emitted once per run (by the domain that
//!   owns the FIRST entity in generation order — the anchor entity):
//!   `01_auth.hurl` (missing/garbage key → 401), `03_scope_denial_403.hurl`
//!   (read-only key: read 200, write 403 INSUFFICIENT_SCOPE),
//!   `04_cross_tenant_404.hurl` (org-B GET of an org-A row → 404), and
//!   `08_rls_isolation.hurl` — the ops api-suite stage-8 convention file,
//!   named in the ops manifest's `[hurl].skip` so the main hurl loop never
//!   runs it (stage 8 runs it with `api_key_a`/`api_key_b` only).
//!
//! Bodies carry REQUIRED fields only (the same minimal-payload rule as the
//! ops smoke check). Codelist fields use the first enum code for create and
//! the second for the PUT roundtrip. Hurl variables never appear as template
//! literals — URLs, headers, bodies and assert blocks are precomputed in the
//! context so Tera never needs to escape `{{...}}`.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{PropertyNode, SchemaNode};
use codegraph_type_contracts::RefClassificationKind;
use serde::Serialize;

use super::api_model::{resolve_entity_operations, resolve_path_segment_with_config};
use crate::error::Result;
use crate::ordering::compute_generation_order;
use crate::output::render_template;
use crate::project_config::ProjectConfig;
use crate::traits::{DomainGenerator, DomainGeneratorKind, GeneratedFile};

/// The zero UUID every generated GET-404 check requests.
pub const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

/// The stage-8 convention file — the ops manifest's `[hurl].skip` entry.
pub const ISOLATION_FILE: &str = "08_rls_isolation.hurl";

/// Default org ids — MUST stay aligned with `OpsHurl::default()`
/// (codegraph-config), which the api suite provisions keys against.
pub const ORG_ID_A: &str = "00000000-0000-0000-0000-000000000001";
pub const ORG_ID_B: &str = "00000000-0000-0000-0000-000000000002";

/// Per-entity contract files number from 10 (slots 01/03/04/08 are the
/// fixed authn/authz convention).
const ENTITY_NN_BASE: usize = 10;

pub struct HurlContractGenerator {
    output_dir: PathBuf,
}

impl HurlContractGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

/// Fully-rendered section of a contract file (a request + its response
/// expectations). No trailing newline; the template's tag newlines provide
/// the separators.
#[derive(Clone, Serialize)]
struct ContractSection {
    body: String,
}

/// Template context for one entity's contract file.
#[derive(Serialize)]
struct EntityContractCtx {
    domain: String,
    module: String,
    nn: String,
    has_list: bool,
    has_create: bool,
    has_read: bool,
    has_update: bool,
    has_delete: bool,
    /// `{{base_url}}/api/v1/{domain}/{seg}?page=0&page_size=10`
    list_url: String,
    /// `{{base_url}}/api/v1/{domain}/{seg}` — id appended per request.
    item_url_base: String,
    /// `{{base_url}}/api/v1/{domain}/{seg}` (POST target)
    create_url: String,
    /// `Authorization: Bearer {{api_key}}`
    auth_header: String,
    /// Required-FK preamble POST sections (rendered).
    fk_parent_sections: Vec<ContractSection>,
    create_body: String,
    /// Bare capture name (`[Captures]` section).
    capture_name: String,
    /// Braced hurl variable reference (`{{module}}_id`) for URL use.
    capture_ref: String,
    create_asserts: &'static str,
    echo_asserts: String,
    /// The entity's minimal create payload — reused by the auth suite.
    minimal_body: String,
    update_body: String,
    update_asserts: String,
}

/// One service-op action leg in the actions contract (issue #590).
#[derive(Serialize)]
struct OpLeg {
    op_name: String,
    service: String,
    snake: String,
    /// Expected status: 200 (wired delegation), 409 (wrong state for a
    /// machine transition), or 501 (seam).
    expect: u16,
    seam: bool,
    seam_reason: String,
    /// A replay leg proving the wrong-state 409 contract.
    wrong_state: bool,
}

/// Build the state-aware action legs: walk the machine from its initial
/// state in declaration order. A transition valid from the current state
/// pins 200 and advances; an out-of-order transition pins 409 (the handler
/// refuses it); seams pin 501. Finally, replay one exercised non-self
/// transition from the post-sequence state to GUARANTEE a 409 leg even when
/// declaration order happens to be valid end to end.
fn state_aware_legs(ops: &[crate::ddd::service_ops::ServiceOpRoute]) -> Vec<OpLeg> {
    use crate::ddd::service_ops::{ExecutionKind, ServiceOpTransition};

    let mut current: Option<String> = ops.iter().find_map(|op| match &op.execution {
        ExecutionKind::Update { state: Some(state) } => Some(state.initial.as_str().to_string()),
        _ => None,
    });

    let mut legs = Vec::with_capacity(ops.len() + 1);
    // Delete legs run LAST: a delete removes the captured row, so any
    // read/transition after it would 404. The contract still exercises the
    // route; only the order is moved.
    let mut delete_legs = Vec::new();
    let mut exercised: Vec<(String, String, ServiceOpTransition)> = Vec::new();
    for op in ops {
        if matches!(op.execution, ExecutionKind::Delete) {
            delete_legs.push(OpLeg {
                op_name: op.op_name.clone(),
                service: op.service.clone(),
                snake: op.snake.clone(),
                expect: 200,
                seam: false,
                seam_reason: "wired".to_string(),
                wrong_state: false,
            });
            continue;
        }
        let leg = match &op.execution {
            ExecutionKind::Seam { reason } => OpLeg {
                op_name: op.op_name.clone(),
                service: op.service.clone(),
                snake: op.snake.clone(),
                expect: 501,
                seam: true,
                seam_reason: reason.clone(),
                wrong_state: false,
            },
            ExecutionKind::Update { state: Some(state) } if !state.op.is_empty() => {
                let from = current
                    .clone()
                    .unwrap_or_else(|| state.initial.as_str().to_string());
                match state.op.iter().find(|t| t.from.as_str() == from) {
                    Some(transition) => {
                        exercised.push((
                            op.op_name.clone(),
                            op.service.clone(),
                            transition.clone(),
                        ));
                        current = Some(transition.to.as_str().to_string());
                        OpLeg {
                            op_name: op.op_name.clone(),
                            service: op.service.clone(),
                            snake: op.snake.clone(),
                            expect: 200,
                            seam: false,
                            seam_reason: "wired".to_string(),
                            wrong_state: false,
                        }
                    }
                    None => OpLeg {
                        op_name: op.op_name.clone(),
                        service: op.service.clone(),
                        snake: op.snake.clone(),
                        expect: 409,
                        seam: false,
                        seam_reason: format!("no transition from the current state `{from}`"),
                        wrong_state: true,
                    },
                }
            }
            _ => OpLeg {
                op_name: op.op_name.clone(),
                service: op.service.clone(),
                snake: op.snake.clone(),
                expect: 200,
                seam: false,
                seam_reason: "wired".to_string(),
                wrong_state: false,
            },
        };
        legs.push(leg);
    }

    // Guarantee the wrong-state edge is exercised. Replaying an already
    // exercised transition from the final state is a 409 unless the
    // transition is a self-edge (which leaves the state addressable again).
    if let Some(current) = current.as_deref()
        && let Some((op_name, service, transition)) = exercised
            .iter()
            .rev()
            .find(|(_, _, t)| t.to.as_str() == current && t.from != t.to)
    {
        legs.push(OpLeg {
            op_name: op_name.clone(),
            service: service.clone(),
            snake: codegraph_naming::to_snake_case(op_name),
            expect: 409,
            seam: false,
            seam_reason: format!(
                "wrong-state replay: `{}` has no transition from `{current}`",
                transition.from
            ),
            wrong_state: true,
        });
    }
    legs.extend(delete_legs);
    legs
}

/// Context for the per-entity `actions` hurl file (issue #590): a fresh
/// CREATE fixture then one POST per service op, pinning route existence
/// and the lowering.
#[derive(Serialize)]
struct ActionsContractCtx {
    domain: String,
    module: String,
    /// Rendered required-FK preamble sections (verbatim from the entity
    /// contract's resolution).
    fk_sections: Vec<ContractSection>,
    create_url: String,
    auth_header: String,
    create_body: String,
    capture_name: String,
    capture_ref: String,
    create_asserts: &'static str,
    item_url_base: String,
    ops: Vec<OpLeg>,
}

/// Shared route context for the auth-suite files (all anchor-entity routes).
#[derive(Serialize)]
struct AuthRoutes {
    domain: String,
    module: String,
    create_url: String,
    list_url: String,
    item_url_base: String,
    create_body: String,
    /// `Authorization: Bearer {{api_key_limited}}`
    limited_header: String,
    /// `Authorization: Bearer {{api_key_a}}`
    key_a_header: String,
    /// `Authorization: Bearer {{api_key_b}}`
    key_b_header: String,
    /// Braced hurl variable: `{{cross_tenant_{module}_id}}`
    cross_tenant_ref: String,
}

/// Template context for the stage-8 `rls_isolation.tera` file.
#[derive(Serialize)]
struct RlsIsolationCtx {
    domain: String,
    module: String,
    create_url: String,
    /// `{{base_url}}/…?page=0&page_size=100` — the org-B scoping sweep.
    list_page_url: String,
    item_url_base: String,
    create_body: String,
    key_a_header: String,
    key_b_header: String,
    /// Braced hurl variable: `{{rls_{module}_id}}`
    rls_ref: String,
    /// `jsonpath "$.data[*].id" not contains "{{rls_{module}_id}}"`
    not_contains_assert: String,
}

/// Which of the three `auth.tera` files is being rendered (template-side
/// `{% if kind == ... %}` gating). Each kind also carries the marker its
/// POST body is labelled with, so a failing run identifies the file that
/// created the row.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum AuthKind {
    Auth,
    ScopeDenial,
    CrossTenant,
}

impl AuthKind {
    fn marker(&self) -> &'static str {
        match self {
            AuthKind::Auth => "auth-probe",
            AuthKind::ScopeDenial => "scope-denial",
            AuthKind::CrossTenant => "org-a",
        }
    }
}

/// Label the string fixture values in a minimal payload with a per-file
/// marker (`"contract-widget-part"` → `"scope-denial-widget-part"`) so a
/// failing run identifies which contract file created the row.
fn marked_body(minimal_body: &str, marker: &str) -> String {
    minimal_body.replace("\"contract-", &format!("\"{marker}-"))
}

/// A required FK parent of an entity: route segment, FK json field, the
/// capture name its preamble POST introduces, and its own minimal payload.
struct FkParent {
    module: String,
    route_segment: String,
    json_name: String,
    capture: String,
    create_body: String,
}

#[async_trait]
impl DomainGenerator for HurlContractGenerator {
    fn kind(&self) -> DomainGeneratorKind {
        DomainGeneratorKind::HurlContract
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        entity_titles: &[String],
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if entity_titles.is_empty() {
            return Ok(vec![]);
        }

        // Global generation order drives the deterministic numbering and the
        // auth-suite anchor (first entity of the first domain). Only
        // entities number and anchor: codelists expose routes but are
        // seed-data tables, not API contract surface.
        let order = compute_generation_order(db, config).await?;
        let mut entity_order: Vec<(String, String)> = Vec::with_capacity(order.len());
        for entry in &order {
            if let Ok(Some(schema)) = db
                .get_schema_in_domain(&entry.schema_title, &entry.domain)
                .await
                && schema.is_entity
            {
                entity_order.push((entry.schema_title.clone(), entry.domain.clone()));
            }
        }
        let number_of = |title: &str, dom: &str| -> Option<usize> {
            entity_order
                .iter()
                .position(|(t, d)| t == title && d == dom)
        };

        // The auth-suite anchor: the first ROOT entity in generation order
        // (children's routes nest under a parent, so top-level auth-suite
        // URLs would 404 against them). All-child models fall back to the
        // first entity.
        let is_child = |title: &str, dom: &str| -> bool {
            config
                .domains
                .get(dom)
                .and_then(|d| d.get_entity_config(title))
                .and_then(|ec| ec.role.clone())
                .as_deref()
                == Some("child")
        };
        let auth_anchor: Option<(String, String)> = entity_order
            .iter()
            .find(|(t, d)| !is_child(t, d))
            .or_else(|| entity_order.first())
            .cloned();

        let api_version = &config.defaults.api_version;
        let mut files = Vec::new();

        for title in entity_titles {
            let Some(schema) = db.get_schema_in_domain(title, domain).await? else {
                continue;
            };
            if schema.pg_table_name.is_empty() {
                continue;
            }
            let Some(idx) = number_of(title, domain) else {
                continue;
            };
            let nn = format!("{:02}", ENTITY_NN_BASE + idx);

            let contract =
                build_entity_contract(db, domain, &schema, api_version, &nn, config).await?;

            files.push(GeneratedFile {
                path: self
                    .output_dir
                    .join("hurl")
                    .join(format!("{nn}_{}_{}.hurl", domain, schema.pg_table_name)),
                content: render_template(tera, "hurl/entity.tera", &contract)?,
            });

            // The per-entity actions contract (issue #590): entities with
            // service ops. Children emit it too — the contract's URLs nest
            // under the parent collection (WS4), so the legs hit the real
            // mounted route.
            let ops = crate::ddd::service_ops::routes_for_entity(
                db,
                config,
                domain,
                title,
                project.codegen.dto_key_casing,
            )
            .await
            .map_err(codegraph_core::error::Error::Config)?;
            if !ops.is_empty() {
                let legs = state_aware_legs(&ops);
                let ctx = ActionsContractCtx {
                    domain: domain.to_string(),
                    module: schema.pg_table_name.clone(),
                    fk_sections: contract.fk_parent_sections.clone(),
                    create_url: contract.create_url.clone(),
                    auth_header: contract.auth_header.clone(),
                    create_body: contract.create_body.clone(),
                    capture_name: contract.capture_name.clone(),
                    capture_ref: contract.capture_ref.clone(),
                    create_asserts: contract.create_asserts,
                    item_url_base: contract.item_url_base.clone(),
                    ops: legs,
                };
                files.push(GeneratedFile {
                    path: self.output_dir.join("hurl").join(format!(
                        "{nn}_{}_{}_actions.hurl",
                        domain, schema.pg_table_name
                    )),
                    content: render_template(tera, "hurl/actions.tera", &ctx)?,
                });
            }

            // The authn/authz suite: exactly once per run, anchored at the
            // auth anchor — the first ROOT entity in generation order (its
            // domain emits it). A child anchor would emit top-level URLs
            // for nested routes, so children are skipped when any root
            // entity exists; all-child models fall back to the first
            // entity.
            if matches!(
                auth_anchor.as_ref(),
                Some((t, d)) if d == domain && t == title
            ) {
                files.extend(self.auth_suite(tera, domain, api_version, &contract)?);
            }
        }

        Ok(files)
    }
}

impl HurlContractGenerator {
    /// The four authn/authz files, all targeting the anchor entity's routes.
    fn auth_suite(
        &self,
        tera: &tera::Tera,
        domain: &str,
        _api_version: &str,
        contract: &EntityContractCtx,
    ) -> Result<Vec<GeneratedFile>> {
        let module = &contract.module;
        let auth = AuthRoutes {
            domain: domain.to_string(),
            module: module.clone(),
            create_url: contract.create_url.clone(),
            list_url: contract.list_url.clone(),
            item_url_base: contract.item_url_base.clone(),
            create_body: contract.minimal_body.clone(),
            limited_header: "Authorization: Bearer {{api_key_limited}}".to_string(),
            key_a_header: "Authorization: Bearer {{api_key_a}}".to_string(),
            key_b_header: "Authorization: Bearer {{api_key_b}}".to_string(),
            cross_tenant_ref: [
                "{{".to_string(),
                "cross_tenant_".to_string(),
                module.clone(),
                "_id".to_string(),
                "}}".to_string(),
            ]
            .concat(),
        };
        let with_kind = |kind: &AuthKind| -> serde_json::Value {
            let mut value = serde_json::to_value(&auth).expect("auth ctx serializes");
            value["create_body"] =
                serde_json::Value::String(marked_body(&contract.minimal_body, kind.marker()));
            value["kind"] = serde_json::to_value(kind).expect("kind serializes");
            value
        };
        let rls = RlsIsolationCtx {
            domain: domain.to_string(),
            module: module.clone(),
            create_url: contract.create_url.clone(),
            list_page_url: format!("{}?page=0&page_size=100", contract.item_url_base),
            item_url_base: contract.item_url_base.clone(),
            create_body: marked_body(&contract.minimal_body, "rls-isolation"),
            key_a_header: "Authorization: Bearer {{api_key_a}}".to_string(),
            key_b_header: "Authorization: Bearer {{api_key_b}}".to_string(),
            rls_ref: [
                "{{".to_string(),
                "rls_".to_string(),
                module.clone(),
                "_id".to_string(),
                "}}".to_string(),
            ]
            .concat(),
            not_contains_assert: format!(
                "jsonpath \"$.data[*].id\" not contains \"{{{{rls_{module}_id}}}}\""
            ),
        };

        Ok(vec![
            GeneratedFile {
                path: self.output_dir.join("hurl").join("01_auth.hurl"),
                content: render_template(tera, "hurl/auth.tera", &with_kind(&AuthKind::Auth))?,
            },
            GeneratedFile {
                path: self
                    .output_dir
                    .join("hurl")
                    .join("03_scope_denial_403.hurl"),
                content: render_template(
                    tera,
                    "hurl/auth.tera",
                    &with_kind(&AuthKind::ScopeDenial),
                )?,
            },
            GeneratedFile {
                path: self
                    .output_dir
                    .join("hurl")
                    .join("04_cross_tenant_404.hurl"),
                content: render_template(
                    tera,
                    "hurl/auth.tera",
                    &with_kind(&AuthKind::CrossTenant),
                )?,
            },
            GeneratedFile {
                path: self.output_dir.join("hurl").join(ISOLATION_FILE),
                content: render_template(tera, "hurl/rls_isolation.tera", &rls)?,
            },
        ])
    }
}

/// Resolve the route segment for a schema: entity_config override, then the
/// schema's `api_path_segment`, then the lowercased title.
async fn route_segment_for(config: &DomainConfig, domain: &str, schema: &SchemaNode) -> String {
    let entity_config = config
        .domains
        .get(domain)
        .and_then(|entry| entry.get_entity_config(&schema.title));
    resolve_path_segment_with_config(entity_config, schema, config)
}

/// Resolve a ref target schema: exact title first, then title +
/// `type_suffix` (the mox/JSON alias chain).
async fn resolve_ref_target(
    db: &dyn GraphQuerier,
    type_suffix: &str,
    ref_target: &str,
) -> Option<SchemaNode> {
    if let Ok(Some(schema)) = db.get_schema(ref_target).await {
        return Some(schema);
    }
    let suffixed = format!("{ref_target}{type_suffix}");
    db.get_schema(&suffixed).await.ok().flatten()
}

/// The required FK parents of an entity: required, scalar, non-null
/// entity-ref properties, resolved to parent routes + minimal payloads.
/// Skipped above depth 0 so parent payloads never recurse (self/cyclic
/// required refs terminate).
async fn resolve_fk_parents(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema: &SchemaNode,
    api_version: &str,
    depth: usize,
) -> Result<Vec<FkParent>> {
    if depth > 0 {
        return Ok(vec![]);
    }
    let type_suffix = &config.defaults.type_suffix;
    let props = db.get_properties(&schema.title).await?;
    let mut parents = Vec::new();
    for prop in props {
        if !prop.is_required || prop.is_array || prop.is_nullable {
            continue;
        }
        if prop.effective_kind() != Some(RefClassificationKind::EntityReference) {
            continue;
        }
        let Some(target) = prop.ref_target.clone() else {
            continue;
        };
        let Some(parent) = resolve_ref_target(db, type_suffix, &target).await else {
            continue;
        };
        let field = codegraph_core::types::resolve_field(&prop);
        // Boxed: the parent's payload resolution recurses back into
        // resolve_fk_parents (depth+1); the guard above terminates it but
        // the compiler still needs the future boxed.
        let parent_body = Box::pin(minimal_body_for(
            db,
            config,
            domain,
            &parent,
            api_version,
            depth + 1,
        ))
        .await?;
        let parent_segment = route_segment_for(config, domain, &parent).await;
        let capture = format!("parent_{}", field.rust_field_name);
        parents.push(FkParent {
            module: parent.pg_table_name.clone(),
            route_segment: parent_segment,
            json_name: field.rust_field_name,
            capture,
            create_body: parent_body,
        });
    }
    Ok(parents)
}

/// The create/PUT payload lines and GET echo asserts for one entity.
///
/// Returns `(create_lines, create_echo, update_lines, update_echo)` —
/// deterministic per type: strings get `-v2` on update, numbers increment,
/// booleans flip, codelists move to the second enum code. Required fields
/// only, MINUS the workflow-managed status/approval fields and configured
/// `dto.immutable_fields`: the generated Create/Update DTOs carry
/// `deny_unknown_fields` and exclude those fields, so a contract body
/// carrying them 422s (the WS2 live-gate finding — the exclusion set
/// mirrors `ddd::dto`'s workflow_excluded_fields and
/// `service_ops::state_field_is_dto_updatable`).
#[allow(clippy::type_complexity)]
async fn payload_fields(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema: &SchemaNode,
    fk_captures: &[(String, String)],
) -> Result<(Vec<String>, Vec<String>, Vec<String>, Vec<String>)> {
    let props = db.get_properties(&schema.title).await?;
    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(&schema.title));
    let mut excluded: Vec<String> = entity_cfg
        .and_then(|ec| ec.workflow.as_ref())
        .map(|wf| {
            let mut fields = vec![wf.status_field.clone()];
            if let Some(ref approval) = wf.approval_status_field {
                fields.push(approval.clone());
            }
            fields
        })
        .unwrap_or_default();
    if let Some(ec) = entity_cfg {
        excluded.extend(ec.dto.immutable_fields.iter().cloned());
    }
    let is_excluded = |name: &str| {
        excluded
            .iter()
            .any(|f| codegraph_naming::to_snake_case(f) == name)
    };

    let mut create = Vec::new();
    let mut echo = Vec::new();
    let mut update = Vec::new();
    let mut update_echo = Vec::new();

    for prop in &props {
        let name = &prop.rust_field_name;
        if matches!(name.as_str(), "id" | "created_at" | "updated_at") {
            continue;
        }
        if prop.is_array {
            continue;
        }
        if is_excluded(name) {
            continue;
        }
        if prop.is_nullable || !prop.is_required {
            continue;
        }
        match prop.effective_kind() {
            Some(RefClassificationKind::ValueObject)
            | Some(RefClassificationKind::StructuredWrapper) => continue,
            Some(RefClassificationKind::EntityReference) => {
                let json_name = codegraph_core::types::resolve_field(prop).rust_field_name;
                if let Some((_, capture)) = fk_captures.iter().find(|(n, _)| *n == json_name) {
                    // JSON string carrying a hurl variable reference:
                    // "{{capture}}" (plain literals — no format-string
                    // brace escaping).
                    let value = "\"{{".to_string() + capture + "}}\"";
                    create.push(format!(r#"  "{json_name}": {value}"#));
                    echo.push(format!(r#"jsonpath "$.data.{json_name}" == {value}"#));
                    update.push(format!(r#"  "{json_name}": {value}"#));
                    update_echo.push(format!(r#"jsonpath "$.data.{json_name}" == {value}"#));
                }
            }
            Some(RefClassificationKind::CodelistReference)
            | Some(RefClassificationKind::InlineEnum)
            | Some(RefClassificationKind::CodelistCheck) => {
                // Both codelist FKs and inline enums serialize as the enum's
                // code string in the DTO plane — the values live on the
                // ref target's graph node either way (the mox bridge and
                // the JSON codelist path both ingest EnumValues). The raw
                // ref target may be a $ref (`codelist/X.json#`) — normalize
                // to the bare schema title (the shared helper).
                let target = codegraph_core::types::codelist_enum_name_from_ref(&prop.ref_target);
                let Some(target) = target else {
                    continue;
                };
                let values = db.get_enum_values(&target).await.unwrap_or_default();
                if values.is_empty() {
                    continue;
                }
                let first = values[0].value.clone();
                let second = values
                    .get(1)
                    .map(|v| v.value.clone())
                    .unwrap_or_else(|| first.clone());
                create.push(format!(r#"  "{name}": "{first}""#));
                echo.push(format!(r#"jsonpath "$.data.{name}" == "{first}""#));
                update.push(format!(r#"  "{name}": "{second}""#));
                update_echo.push(format!(r#"jsonpath "$.data.{name}" == "{second}""#));
            }
            _ => {
                let (create_value, update_value) = fixture_pair(&schema.pg_table_name, prop);
                create.push(format!(r#"  "{name}": {create_value}"#));
                echo.push(format!(r#"jsonpath "$.data.{name}" == {create_value}"#));
                update.push(format!(r#"  "{name}": {update_value}"#));
                update_echo.push(format!(r#"jsonpath "$.data.{name}" == {update_value}"#));
            }
        }
    }

    Ok((create, echo, update, update_echo))
}

/// Deterministic create/update fixture values for a scalar property.
fn fixture_pair(module: &str, prop: &PropertyNode) -> (String, String) {
    let pg = prop.pg_column_type.to_uppercase();
    let rust = prop.rust_field_type.to_uppercase();
    if pg.contains("BOOL") || rust.contains("BOOL") {
        return ("true".into(), "false".into());
    }
    if pg.contains("UUID") {
        return (
            "\"00000000-0000-0000-0000-000000000001\"".into(),
            "\"00000000-0000-0000-0000-000000000001\"".into(),
        );
    }
    if ["BIGINT", "INT", "INTEGER", "SMALLINT"]
        .iter()
        .any(|p| pg.starts_with(p))
        || rust.contains("I64")
        || rust.contains("I32")
    {
        return ("42".into(), "43".into());
    }
    if ["NUMERIC", "DECIMAL", "DOUBLE", "REAL", "FLOAT"]
        .iter()
        .any(|p| pg.starts_with(p))
    {
        return ("42.5".into(), "43.5".into());
    }
    if pg.starts_with("DATE") {
        return ("\"2026-01-01\"".into(), "\"2026-01-01\"".into());
    }
    if pg.starts_with("TIMESTAMP") {
        return (
            "\"2026-01-01T00:00:00Z\"".into(),
            "\"2026-01-01T00:00:00Z\"".into(),
        );
    }
    (
        format!("\"contract-{module}\""),
        format!("\"contract-{module}-v2\""),
    )
}

/// The minimal JSON create payload (required fields only) for a schema —
/// used for FK-parent preamble POSTs and the auth-suite bodies.
async fn minimal_body_for(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema: &SchemaNode,
    api_version: &str,
    depth: usize,
) -> Result<String> {
    let (create_body, _, _, _, _) =
        payload_sections(db, config, domain, schema, api_version, depth).await?;
    Ok(create_body)
}

/// An entity's payload sections:
/// `(create_body, echo_asserts, update_body, update_asserts, fk_parents)`.
#[allow(clippy::type_complexity)]
async fn payload_sections(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema: &SchemaNode,
    api_version: &str,
    depth: usize,
) -> Result<(String, String, String, String, Vec<FkParent>)> {
    let fk_parents = resolve_fk_parents(db, config, domain, schema, api_version, depth).await?;
    let captures: Vec<(String, String)> = fk_parents
        .iter()
        .map(|p| (p.json_name.clone(), p.capture.clone()))
        .collect();
    let (create, echo, update, update_echo) =
        payload_fields(db, config, domain, schema, &captures).await?;

    let body = |lines: &[String]| -> String {
        if lines.is_empty() {
            "{}".to_string()
        } else {
            format!("{{\n{}\n}}", lines.join(",\n"))
        }
    };
    Ok((
        body(&create),
        echo.join("\n"),
        body(&update),
        update_echo.join("\n"),
        fk_parents,
    ))
}

/// One hop in a child entity's nesting chain: the parent row the nested
/// route runs under, the capture its preamble POST introduces, and the
/// parent's own create URL + minimal payload.
struct NestingHop {
    module: String,
    route_segment: String,
    /// The child-side FK column the config declares (`parent_ref`), which
    /// doubles as the child-schema property name when the schema carries
    /// the FK — payload fields reference the hop's capture through it.
    fk_json_name: String,
    capture_name: String,
    create_url: String,
    create_body: String,
}

/// Walk a child entity's config-declared parent chain (child → root),
/// outermost parent first. Cycles stop the walk; depth is bounded. A root
/// entity yields an empty chain.
async fn resolve_nesting_chain(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema: &SchemaNode,
    api_version: &str,
) -> Result<Vec<NestingHop>> {
    let mut hops: Vec<NestingHop> = Vec::new();
    let mut visited = std::collections::HashSet::new();
    visited.insert(schema.title.clone());
    let mut current = schema.clone();
    while hops.len() < 4 {
        let ec = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(&current.title));
        let Some(parent_title) = ec.and_then(|e| e.parent.clone()) else {
            break;
        };
        if ec.and_then(|e| e.role.as_deref()) != Some("child") {
            break;
        }
        let parent = match db.get_schema_in_domain(&parent_title, domain).await {
            Ok(Some(parent)) => parent,
            _ => break,
        };
        if !visited.insert(parent.title.clone()) {
            break; // cycle — stop walking
        }
        let fk_json_name = ec.and_then(|e| e.parent_ref.clone()).unwrap_or_else(|| {
            format!(
                "{}_id",
                codegraph_naming::to_snake_case(parent.rust_type_name.as_str())
            )
        });
        let segment = route_segment_for(config, domain, &parent).await;
        // The parent's own minimal payload; depth 1 skips ITS FK preamble
        // recursion (the guard in `resolve_fk_parents`) — a parent with
        // required FKs is seeded by its own contract file.
        let create_body = minimal_body_for(db, config, domain, &parent, api_version, 1).await?;
        hops.push(NestingHop {
            capture_name: format!("{}_id", parent.pg_table_name),
            module: parent.pg_table_name.clone(),
            route_segment: segment,
            fk_json_name,
            create_url: format!(
                "{{{{base_url}}}}/api/{api_version}/{domain}/{}",
                route_segment_for(config, domain, &parent).await
            ),
            create_body,
        });
        current = parent;
    }
    hops.reverse(); // outermost parent first
    Ok(hops)
}

/// Assemble one entity's full template context.
async fn build_entity_contract(
    db: &dyn GraphQuerier,
    domain: &str,
    schema: &SchemaNode,
    api_version: &str,
    nn: &str,
    config: &DomainConfig,
) -> Result<EntityContractCtx> {
    // The config-declared nesting chain (empty for roots): each hop's
    // preamble POST creates the parent row and captures its id, and the
    // entity's own URLs nest under the parent's collection.
    let nesting = resolve_nesting_chain(db, config, domain, schema, api_version).await?;

    // Required-FK parents of the entity itself. The nesting chain owns the
    // parent FK capture when the schema carries that property — drop it
    // from the FK resolution so the parent is created exactly once.
    let mut fk_parents = resolve_fk_parents(db, config, domain, schema, api_version, 0).await?;
    let child_cfg = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(&schema.title));
    let child_parent_ref = child_cfg.and_then(|ec| ec.parent_ref.clone());
    if let Some(ref parent_ref) = child_parent_ref {
        fk_parents.retain(|p| p.json_name != *parent_ref);
    }

    let operations = resolve_entity_operations(db, config, domain, &schema.title).await;
    let has = |op: &str| operations.iter().any(|o| o == op);

    let segment = route_segment_for(config, domain, schema).await;
    let module = &schema.pg_table_name;
    let domain_base = format!("{{{{base_url}}}}/api/{api_version}/{domain}");
    let mut entity_url = domain_base.clone();
    for hop in &nesting {
        entity_url.push_str(&format!(
            "/{}/{{{{{}}}}}",
            hop.route_segment, hop.capture_name
        ));
    }
    entity_url.push_str(&format!("/{segment}"));

    let capture_name = format!("{module}_id");

    let mut fk_parent_sections = Vec::new();
    for hop in &nesting {
        fk_parent_sections.push(ContractSection {
            body: format!(
                "# Parent {} — the nested route requires a parent row, id captured\n\
                 POST {}\n\
                 Authorization: Bearer {{{{api_key}}}}\n\
                 {}\n\
                 \n\
                 HTTP 201\n\
                 [Captures]\n\
                 {}: jsonpath \"$.data.id\"",
                hop.module, hop.create_url, hop.create_body, hop.capture_name,
            ),
        });
    }
    for parent in &fk_parents {
        let parent_url = format!("{domain_base}/{}", parent.route_segment);
        fk_parent_sections.push(ContractSection {
            body: format!(
                "# Required FK parent {} — created first, id captured\n\
                 POST {}\n\
                 Authorization: Bearer {{{{api_key}}}}\n\
                 {}\n\
                 \n\
                 HTTP 201\n\
                 [Captures]\n\
                 {}: jsonpath \"$.data.id\"",
                parent.module, parent_url, parent.create_body, parent.capture,
            ),
        });
    }

    // Payload fields: the FK-parent captures plus the nesting chain's
    // parent captures (a child schema carrying the parent-FK property
    // references the nesting preamble's capture).
    let mut captures: Vec<(String, String)> = fk_parents
        .iter()
        .map(|p| (p.json_name.clone(), p.capture.clone()))
        .collect();
    for hop in &nesting {
        captures.push((hop.fk_json_name.clone(), hop.capture_name.clone()));
    }
    let (create, echo, update, update_echo) =
        payload_fields(db, config, domain, schema, &captures).await?;
    let body = |lines: &[String]| -> String {
        if lines.is_empty() {
            "{}".to_string()
        } else {
            format!("{{\n{}\n}}", lines.join(",\n"))
        }
    };
    let create_body = body(&create);
    let echo_asserts = echo.join("\n");
    let update_body = body(&update);
    let update_asserts = update_echo.join("\n");

    Ok(EntityContractCtx {
        domain: domain.to_string(),
        module: module.clone(),
        nn: nn.to_string(),
        has_list: has("list"),
        has_create: has("create"),
        has_read: has("read"),
        has_update: has("update"),
        has_delete: has("delete"),
        list_url: format!("{entity_url}?page=0&page_size=10"),
        item_url_base: entity_url.clone(),
        create_url: entity_url,
        auth_header: "Authorization: Bearer {{api_key}}".to_string(),
        fk_parent_sections,
        create_body: create_body.clone(),
        capture_name: capture_name.clone(),
        capture_ref: ["{{".to_string(), capture_name, "}}".to_string()].concat(),
        create_asserts: r#"jsonpath "$.data.id" exists"#,
        echo_asserts,
        minimal_body: create_body,
        update_body,
        update_asserts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::mock::MockEngine;
    use codegraph_core::traits::GraphQuerier;

    fn task_schema() -> SchemaNode {
        SchemaNode {
            namespace: None,
            schema_id: "work/json/TaskType.json".to_string(),
            title: "TaskType".to_string(),
            description: None,
            schema_type: "object".to_string(),
            classification: "entity_reference".to_string(),
            domain: Some("work".to_string()),
            rel_path: "work/json/TaskType.json".to_string(),
            pg_type: "UUID".to_string(),
            rust_type: "Task".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: "Task".to_string(),
            pg_table_name: "task".to_string(),
            api_path_segment: "task".to_string(),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
            access: None,
            annotations: None,
        }
    }

    fn text_prop(name: &str, required: bool) -> PropertyNode {
        PropertyNode {
            is_id: false,
            name: name.to_string(),
            prop_type: "string".to_string(),
            description: None,
            format: None,
            is_required: required,
            is_nullable: !required,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: codegraph_naming::to_snake_case(name),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: codegraph_naming::to_snake_case(name),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        }
    }

    fn status_prop() -> PropertyNode {
        let mut prop = text_prop("status", true);
        prop.ref_target = Some("TaskStatusList".to_string());
        prop.classification_kind =
            Some(codegraph_type_contracts::RefClassificationKind::CodelistReference);
        prop
    }

    /// The dual-status pattern: a second workflow-managed field.
    fn approval_status_prop() -> PropertyNode {
        let mut prop = text_prop("approval_status", true);
        prop.ref_target = Some("TaskStatusList".to_string());
        prop.classification_kind =
            Some(codegraph_type_contracts::RefClassificationKind::CodelistReference);
        prop
    }

    fn workflow_config(status_field: &str, approval_field: Option<&str>) -> DomainConfig {
        let approval_line = approval_field
            .map(|f| format!("approval_status_field = \"{f}\"\n"))
            .unwrap_or_default();
        toml::from_str(&format!(
            r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.work]
label = "Work"
schema_dir = "work"
postgres_schema = "work"
entities = ["TaskType"]

[domains.work.entity_config.TaskType.workflow]
status_field = "{status_field}"
{approval_line}states = ["Draft", "Posted"]
initial_state = "Draft"
terminal_states = ["Posted"]
"#,
        ))
        .unwrap()
    }

    fn plain_config() -> DomainConfig {
        toml::from_str(
            r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.work]
label = "Work"
schema_dir = "work"
postgres_schema = "work"
entities = ["TaskType"]
"#,
        )
        .unwrap()
    }

    async fn task_db(with_approval: bool) -> MockEngine {
        let mut props = vec![text_prop("title", true), status_prop()];
        if with_approval {
            props.push(approval_status_prop());
        }
        MockEngine::builder()
            .with_schema(task_schema())
            .with_properties("TaskType", props)
            .with_enum_values(
                "TaskStatusList",
                vec![
                    codegraph_core::types::EnumValue {
                        value: "Draft".to_string(),
                        display_name: Some("Draft".to_string()),
                        sort_order: 0,
                    },
                    codegraph_core::types::EnumValue {
                        value: "Posted".to_string(),
                        display_name: Some("Posted".to_string()),
                        sort_order: 1,
                    },
                ],
            )
            .build()
    }

    /// Bug fix (WS2 live-gate finding): the hurl create/PUT payloads must
    /// exclude workflow-managed status fields — the generated Create/Update
    /// DTOs carry `deny_unknown_fields` and the status field is excluded
    /// there, so a contract body carrying it 422s on every consumer with a
    /// `domains.toml` workflow.
    #[tokio::test]
    async fn payload_fields_exclude_workflow_managed_status_fields() {
        let db = task_db(false).await;
        let schema = db.get_schema("TaskType").await.unwrap().unwrap();
        let config = workflow_config("status", None);

        let (create, _echo, update, _update_echo) =
            payload_fields(&db, &config, "work", &schema, &[])
                .await
                .unwrap();
        let create = create.join("\n");
        let update = update.join("\n");

        // The workflow-managed status field is absent from BOTH payloads.
        assert!(
            !create.contains("\"status\""),
            "the create body must not carry the workflow-managed status field:\n{create}"
        );
        assert!(
            !update.contains("\"status\""),
            "the update body must not carry the workflow-managed status field:\n{update}"
        );
        // The non-workflow required field survives.
        assert!(create.contains("\"title\""), "{create}");
        assert!(update.contains("\"title\""), "{update}");
    }

    /// The approval-status field (dual-status pattern) is excluded too.
    #[tokio::test]
    async fn payload_fields_exclude_approval_status_fields() {
        let db = task_db(true).await;
        let schema = db.get_schema("TaskType").await.unwrap().unwrap();
        let config = workflow_config("status", Some("approval_status"));

        let (create, _echo, update, _update_echo) =
            payload_fields(&db, &config, "work", &schema, &[])
                .await
                .unwrap();
        let create = create.join("\n");
        let update = update.join("\n");
        assert!(
            !create.contains("\"approval_status\""),
            "the approval status field must be excluded from create:\n{create}"
        );
        assert!(
            !update.contains("\"approval_status\""),
            "the approval status field must be excluded from update:\n{update}"
        );
        assert!(create.contains("\"title\""), "{create}");
    }

    /// Without a configured workflow the codelist status field stays in the
    /// payloads (the machine-bound create body convention — the initial
    /// state rides the create body).
    #[tokio::test]
    async fn payload_fields_keep_codelist_fields_without_a_workflow() {
        let db = task_db(false).await;
        let schema = db.get_schema("TaskType").await.unwrap().unwrap();
        let config = plain_config();

        let (create, echo, update, _update_echo) =
            payload_fields(&db, &config, "work", &schema, &[])
                .await
                .unwrap();
        let create = create.join("\n");
        assert!(
            create.contains("\"status\": \"Draft\""),
            "no workflow ⇒ the codelist field stays (first enum code):\n{create}"
        );
        assert!(
            echo.join("\n").contains("\"$.data.status\" == \"Draft\""),
            "{create}"
        );
        assert!(
            update.join("\n").contains("\"status\": \"Posted\""),
            "the PUT roundtrip moves to the second enum code"
        );
    }
}
