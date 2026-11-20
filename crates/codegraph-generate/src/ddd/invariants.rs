//! Design invariants lower to typed predicate functions (DD5, issue #666
//! slice 2).
//!
//! pherolang 0.3's `invariant <expr>;` clauses arrive PRE-TYPED: the
//! phero-driver's `check_design_invariants` types each expression with
//! `self` = the designed class — own/inherited stored features readable,
//! machine states bind as enum-typed bare names, the result must be
//! boolean, and DTO designs refuse invariants outright. Codegraph
//! re-parses the verbatim source with `phero_expr::parse` and lowers it
//! through the own-record expression subset the #590 `when` guards use —
//! over the parameter name `input` instead of `response` — producing one
//! typed predicate fn per invariant in the domain's `rules.rs` plane (the
//! same file the #338 eligibility guards emit into):
//!
//! ```rust,ignore
//! pub fn grant_application_type_invariant_0(input: &GrantApplicationResponse) -> bool {
//!     (input.amount > 0.0)
//! }
//! ```
//!
//! `index` is the declaration-order position (0-based) and IS the
//! `invariantIndex` the enforcement plane reports (slice 4's 409 detail) —
//! declaration order is the invariant's identity upstream (there is no
//! name/index in the phero-ir payload).
//!
//! Refusal discipline (the DD9 `query_refusals` / #662 `body_refusals`
//! posture): every construct outside the closed subset is a NAMED, HARD,
//! pre-generation error — [`invariant_refusals`] feeds
//! `ValidationPass::check_invariants` (check name `invariant`, error
//! severity), so nothing is half-written. A refused invariant emits NO
//! predicate fn: [`invariants_for`] returns only ever-complete plans.

use codegraph_config::DomainConfig;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_naming::{escape_rust_keyword, to_snake_case};

use crate::ddd::design::DddDesignSurface;
use crate::ddd::service_ops::{fields_for, lower_invariant};

/// One lowered design invariant — the emission surface of the domain
/// `rules.rs` plane (and, from slice 4 on, the handler call sites).
#[derive(Debug, Clone)]
pub struct InvariantPlan {
    /// Declaration-order position (0-based). This IS the `invariantIndex`
    /// the enforcement plane reports in its 409 detail (slice 4) —
    /// declaration order is the invariant's identity upstream.
    pub index: usize,
    /// The invariant expression, verbatim (the authored identity).
    pub expression: String,
    /// Rust fn name in `src/domain/{domain}/rules.rs`:
    /// `{class_snake}_invariant_{index}`.
    pub fn_name: String,
    /// The lowered Rust boolean expression over `input` (the record).
    pub predicate_code: String,
}

/// The lowerable invariants of one entity ([`InvariantPlan`], declaration
/// order). Plans are only ever COMPLETE: an invariant that cannot lower is
/// carried out by [`invariant_refusals`] instead — never half-emitted.
///
/// Degradation mirrors the `guard_refusals` posture: a design without
/// invariants, an undeclared domain, or a graph-read failure yields an
/// empty vec.
pub async fn invariants_for(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    title: &str,
) -> Vec<InvariantPlan> {
    invariants_of(db, config, domain, title).await.0
}

/// The enforcement surface of one entity (issue #666 slice 4d): the
/// DESIGNED class name — the 409 detail's `class` key — plus the
/// ever-complete predicate plans in declaration order. `plans` empty =
/// no enforcement (no invariants, an undeclared domain, or a graph-read
/// degradation — the same posture as [`invariants_for`]).
#[derive(Debug, Clone)]
pub struct InvariantEnforcement {
    /// The designed class name as authored in the `.ddd` design (NOT the
    /// schema title — the 409 detail reports what the designer wrote).
    pub class: String,
    /// The lowered predicates, declaration order.
    pub plans: Vec<InvariantPlan>,
}

impl InvariantEnforcement {
    /// `true` when the entity carries no enforceable invariants — the
    /// byte-identity gate every handler call site uses.
    pub fn is_empty(&self) -> bool {
        self.plans.is_empty()
    }
}

/// The handler-plane enforcement surface for one entity (issue #666
/// slice 4d): the designed class plus its complete predicate plans. The
/// `HandlerGenerator` consumes THIS — never re-derives the lowering —
/// exactly as it consumes `eligibility_guards_for`.
pub async fn enforcement_for(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    title: &str,
) -> InvariantEnforcement {
    let (class, plans, _) = lower_full(db, config, domain, title).await;
    InvariantEnforcement {
        class: class.unwrap_or_else(|| title.to_string()),
        plans,
    }
}

/// Every refusal for one entity's invariants, sorted + deduped (the
/// `guard_refusals` posture). Each refusal names the domain, the class,
/// the invariant index, and the offending construct, and states that the
/// invariant is not emitted — the fail-closed contract. Empty = every
/// invariant of the design lowers.
pub async fn invariant_refusals(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    title: &str,
) -> Vec<String> {
    let mut refusals = invariants_of(db, config, domain, title).await.1;
    refusals.sort();
    refusals.dedup();
    refusals
}

/// The all-models refusal walk (the [`ValidationPass`] entry — the
/// `body_refusals` shape): every resolved design's invariants must lower,
/// and a design that resolved to NO schema may not silently drop them
/// either. Empty = clean; each entry becomes a HARD `ValidationPass` error
/// (check name `invariant`) BEFORE any tree is written.
///
/// Early-returns when no ingested design carries invariants — the common
/// graph pays one models read and nothing else.
pub async fn all_invariant_refusals(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    if no_modeled_invariants(&models) {
        return Ok(Vec::new());
    }
    let surface = match DddDesignSurface::from_graph(db).await {
        Ok(surface) => surface,
        Err(_) => return Ok(Vec::new()),
    };
    let mut refusals = Vec::new();
    // Resolved designs: lower per title (BTreeSet — deterministic order).
    let mut titles: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for model in &models {
        for design in &model.designs {
            if let Some(title) = design.resolved_title.as_deref()
                && !design.invariants.is_empty()
            {
                titles.insert(title);
            }
        }
    }
    for title in titles {
        // The domain rides the schema (the `body_refusals` resolution).
        let Ok(Some(schema)) = db.get_schema(title).await else {
            continue;
        };
        let Some(domain) = schema.domain.clone() else {
            continue;
        };
        let (_, _, refused) = lower_with_surface(&surface, db, config, &domain, title).await;
        refusals.extend(refused);
    }
    // Unresolved designs: the class named no ingested schema, so its
    // invariants cannot lower against any record. Named — never silently
    // dropped (the fail-closed posture).
    for model in &models {
        for design in &model.designs {
            if design.resolved_title.is_none() && !design.invariants.is_empty() {
                refusals.push(format!(
                    "application `{app}`: design `{class}` resolves to no \
                     ingested schema, so its {n} invariant(s) cannot lower \
                     to a predicate — the invariant(s) are not emitted \
                     (fail-closed)",
                    app = model.application.name,
                    class = design.class,
                    n = design.invariants.len(),
                ));
            }
        }
    }
    refusals.sort();
    refusals.dedup();
    Ok(refusals)
}

/// Lower one entity's invariants into `(complete plans, refusals)`,
/// building the design surface. See [`lower_with_surface`].
async fn invariants_of(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    title: &str,
) -> (Vec<InvariantPlan>, Vec<String>) {
    let (_, plans, refusals) = lower_full(db, config, domain, title).await;
    (plans, refusals)
}

/// The full lowering for one entity: `(designed class, complete plans,
/// refusals)`. The class rides out for the enforcement surface's 409
/// detail (`class` names the DESIGNED class, not the schema title).
async fn lower_full(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    title: &str,
) -> (Option<String>, Vec<InvariantPlan>, Vec<String>) {
    // An undeclared domain has no rules plane to emit into.
    if !config.domains.contains_key(domain) {
        return (None, Vec::new(), Vec::new());
    }
    let Ok(surface) = DddDesignSurface::from_graph(db).await else {
        return (None, Vec::new(), Vec::new());
    };
    lower_with_surface(&surface, db, config, domain, title).await
}

/// The lowering core over a pre-built surface: the per-entity shape both
/// the public resolvers and the all-models walk share. Returns `(designed
/// class, complete plans, refusals)`.
///
/// Upstream typing guarantees (pherolang `check_design_invariants`) are
/// re-proven defensively where a stale graph could break them: a dto
/// design carrying invariants and a non-boolean root both refuse. The
/// record type must resolve exactly like the #338 eligibility guards' —
/// `{Entity}Response` out of the entity's `dto_response` module — because
/// the predicate fn takes it as its parameter; a value-object projection
/// (no stored table) is a named deferral.
async fn lower_with_surface(
    surface: &DddDesignSurface,
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    title: &str,
) -> (Option<String>, Vec<InvariantPlan>, Vec<String>) {
    let mut plans = Vec::new();
    let mut refusals = Vec::new();
    // An undeclared domain has no rules plane to emit into.
    if !config.domains.contains_key(domain) {
        return (None, plans, refusals);
    }
    let Some(design) = surface.design_for(title) else {
        return (None, plans, refusals);
    };
    if design.invariants.is_empty() {
        return (None, plans, refusals);
    }
    let class = design.class.clone();
    // Upstream refuses invariants on dto designs (pherolang
    // E_DTO_INVARIANT); a stale graph that carries them is refused, never
    // emitted.
    if design.stereotype == "dto" {
        for index in 0..design.invariants.len() {
            refusals.push(format!(
                "domain `{domain}`: invariant {index} of `{class}`: the \
                 design is a dto — a transport shape carries no invariants; \
                 the invariant is not emitted (fail-closed)"
            ));
        }
        return (Some(class), plans, refusals);
    }
    // The predicate fn takes `&{Entity}Response`; a title with no stored
    // table emits no such record (a value-object projection) — named
    // deferral, never a guessed signature.
    let record_resolves = match crate::ddd::rules::record_type_for(db, domain, title).await {
        Ok(resolved) => resolved.is_some(),
        Err(_) => return (Some(class), plans, refusals),
    };
    if !record_resolves {
        for index in 0..design.invariants.len() {
            refusals.push(format!(
                "domain `{domain}`: invariant {index} of `{class}` has no \
                 record plane to predicate over (`{title}` emits no \
                 `{title}Response` — value-object projections are a \
                 recorded deferral, issue #666); the invariant is not \
                 emitted (fail-closed)"
            ));
        }
        return (Some(class), plans, refusals);
    }
    let Ok(props) = db.get_properties(title).await else {
        return (Some(class), plans, refusals);
    };
    let fields = fields_for(&props);
    let states = surface.machine_states_for(title);
    let class_snake = escape_rust_keyword(&to_snake_case(class.as_str()));
    for (index, invariant) in design.invariants.iter().enumerate() {
        let subject = format!("invariant {index} of `{class}` in domain `{domain}`");
        match lower_invariant(
            &invariant.expression,
            &subject,
            title,
            "input",
            &fields,
            &states,
        ) {
            Ok(predicate_code) => plans.push(InvariantPlan {
                index,
                expression: invariant.expression.clone(),
                fn_name: format!("{class_snake}_invariant_{index}"),
                predicate_code,
            }),
            Err(reason) => refusals.push(format!(
                "{reason} The invariant is not emitted (fail-closed): no \
                 predicate fn exists for it."
            )),
        }
    }
    (Some(class), plans, refusals)
}

/// The MODELED invariant counts across the graph: `(entities with
/// invariants, total invariants)` — the workers-topology WARN's facts
/// (slice 4d). `None` when no design carries invariants (the common graph:
/// the WARN must not even fire its early return).
pub async fn modeled_invariant_counts(db: &dyn GraphQuerier) -> Option<(usize, usize)> {
    let models = db.get_ddd_models().await.ok()?;
    let mut entities = 0usize;
    let mut total = 0usize;
    let mut seen = std::collections::BTreeSet::new();
    for model in &models {
        for design in &model.designs {
            if design.invariants.is_empty() {
                continue;
            }
            total += design.invariants.len();
            if seen.insert(
                design
                    .resolved_title
                    .clone()
                    .unwrap_or_else(|| design.class.clone()),
            ) {
                entities += 1;
            }
        }
    }
    (total > 0).then_some((entities, total))
}

/// `true` when no ingested design carries invariants — the shared
/// early-return gate (one models read, nothing else).
fn no_modeled_invariants(models: &[codegraph_core::types::DddModelGraph]) -> bool {
    models
        .iter()
        .all(|model| model.designs.iter().all(|d| d.invariants.is_empty()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use codegraph_config::DomainConfig;
    use codegraph_core::mock::MockEngine;
    use codegraph_core::traits::GraphIngestor;
    use codegraph_core::types::{
        DddDesignFlags, DddDesignNode, DddInvariant, DddModelGraph, PropertyNode, SchemaNode,
    };
    use codegraph_naming::to_snake_case;
    use codegraph_type_contracts::RefClassificationKind;

    use super::*;
    use crate::ddd::service_ops::{FieldInfo, FieldKind, fields_for, lower_invariant};

    const SUBJECT: &str = "invariant 0 of `GrantApplicationType` in domain `grants`";
    const CLASS: &str = "GrantApplicationType";

    fn fields(pairs: &[(&str, FieldKind, bool)]) -> HashMap<String, FieldInfo> {
        let mut map = HashMap::new();
        for (name, kind, optional) in pairs {
            let info = FieldInfo {
                rust_name: to_snake_case(name),
                kind: *kind,
                optional: *optional,
            };
            map.insert(name.to_string(), info);
        }
        map
    }

    fn lower(source: &str, f: &HashMap<String, FieldInfo>) -> Result<String, String> {
        lower_invariant(source, SUBJECT, CLASS, "input", f, &[])
    }

    // ── lowering KATs (byte-shaped like the #590 when-guards over `input`) ──

    #[test]
    fn numeric_comparison_lowers_typed_over_input() {
        let f = fields(&[("amount", FieldKind::Float, false)]);
        assert_eq!(lower("amount > 0.0", &f).unwrap(), "(input.amount > 0.0)");
    }

    #[test]
    fn machine_state_comparison_binds_the_state_bare_name() {
        let f = fields(&[("payRunStatus", FieldKind::Codelist, false)]);
        let states = vec!["Draft".to_string(), "Approved".to_string()];
        assert_eq!(
            lower_invariant(
                "payRunStatus == Draft",
                "invariant 0 of `PayRunType` in domain `payroll`",
                "PayRunType",
                "input",
                &f,
                &states,
            )
            .unwrap(),
            r#"(input.pay_run_status.to_string() == "Draft")"#,
        );
    }

    #[test]
    fn optional_comparisons_use_the_when_option_idiom() {
        let f = fields(&[
            ("count", FieldKind::Integer, true),
            ("note", FieldKind::String, true),
        ]);
        assert_eq!(
            lower("count < 5", &f).unwrap(),
            "(input.count.is_some_and(|v| v < 5))"
        );
        assert_eq!(
            lower(r#"note != "void""#, &f).unwrap(),
            r#"(input.note.as_deref() != Some("void"))"#
        );
    }

    #[test]
    fn codelist_date_and_string_compare_via_the_when_spellings() {
        let f = fields(&[
            ("status", FieldKind::Codelist, false),
            ("filedOn", FieldKind::Date, false),
            ("title", FieldKind::String, false),
        ]);
        assert_eq!(
            lower(r#"status == "Approved""#, &f).unwrap(),
            r#"(input.status.to_string() == "Approved")"#
        );
        assert_eq!(
            lower(r#"filedOn == date("2026-01-15")"#, &f).unwrap(),
            r#"(input.filed_on.to_string() == "2026-01-15")"#
        );
        assert_eq!(
            lower(r#"title == "Grant""#, &f).unwrap(),
            r#"(input.title == "Grant")"#
        );
    }

    #[test]
    fn compound_conditions_conjoin_with_the_when_shapes() {
        let f = fields(&[
            ("amount", FieldKind::Float, false),
            ("closed", FieldKind::Bool, false),
        ]);
        assert_eq!(
            lower(r#"amount > 0.0 && closed == true"#, &f).unwrap(),
            "((input.amount > 0.0) && (input.closed == true))"
        );
        assert_eq!(
            lower(
                r#"!(note == null)"#,
                &fields(&[("note", FieldKind::String, true)])
            )
            .unwrap(),
            "(!((input.note.is_none())))"
        );
    }

    // ── refusal KATs (each names the class, the index, and the construct) ──

    #[test]
    fn refusals_name_the_class_index_and_construct() {
        let f = fields(&[("amount", FieldKind::Float, false)]);

        // Arithmetic.
        let e = lower("amount + 1 > 0", &f).unwrap_err();
        assert!(
            e.contains("invariant 0 of `GrantApplicationType` in domain `grants`"),
            "the refusal must name the class, index and domain: {e}"
        );
        assert!(e.contains("arithmetic (`+`)"), "{e}");

        // Cross-aggregate navigation (dotted access under a non-injected
        // receiver — the own-record subset has no scopes).
        let e = lower("child.x == 1", &f).unwrap_err();
        assert!(e.contains("`child`.x"), "{e}");
        assert!(e.contains("not an own-record"), "{e}");

        // Collection algebra and literals. A list literal on the literal
        // side of a known field refuses naming the construct (an unknown
        // collection field refuses one step earlier, on the unknown name).
        let e = lower("lines.size() > 0", &f).unwrap_err();
        assert!(e.contains("collection algebra"), "{e}");
        let e = lower("amount == [1, 2]", &f).unwrap_err();
        assert!(e.contains("list literals"), "{e}");

        // Lambdas.
        let e = lower("x => x > 0", &f).unwrap_err();
        assert!(e.contains("a lambda"), "{e}");

        // let / if / calls.
        assert!(
            lower("let x = amount; x > 0", &f)
                .unwrap_err()
                .contains("`let`")
        );
        assert!(
            lower("if amount > 0 { true } else { false }", &f)
                .unwrap_err()
                .contains("`if`")
        );
        assert!(
            lower("approve(amount)", &f)
                .unwrap_err()
                .contains("an operation call")
        );

        // Unparseable source.
        let e = lower("amount >", &f).unwrap_err();
        assert!(e.contains("not valid pherolang expression syntax"), "{e}");

        // Unknown field.
        let e = lower("noSuchField == 1", &f).unwrap_err();
        assert!(e.contains("noSuchField"), "{e}");
        assert!(e.contains("not a field of `GrantApplicationType`"), "{e}");

        // Ordering on a codelist field (the #590 semantics carry over).
        let e = lower(
            r#"status > "Draft""#,
            &fields(&[("status", FieldKind::Codelist, false)]),
        )
        .unwrap_err();
        assert!(e.contains("ordering comparison on `codelist`"), "{e}");
    }

    #[test]
    fn non_boolean_roots_refuse() {
        let f = fields(&[("amount", FieldKind::Float, false)]);
        let e = lower("amount", &f).unwrap_err();
        assert!(
            e.contains("boolean"),
            "a bare field is not a predicate: {e}"
        );
        let e = lower(r#""Posted""#, &f).unwrap_err();
        assert!(
            e.contains("boolean"),
            "a bare literal is not a predicate: {e}"
        );
    }

    // ── resolver KATs (MockEngine: `invariants_for` / `invariant_refusals`) ──

    fn grants_config() -> DomainConfig {
        toml::from_str(
            r#"
[domains.grants]
label = "Grants"
schema_dir = "schemas/grants"
postgres_schema = "grants"
"#,
        )
        .unwrap()
    }

    fn grants_schema(title: &str, table: &str) -> SchemaNode {
        SchemaNode {
            schema_id: format!("grants/{title}"),
            title: title.to_string(),
            description: None,
            schema_type: "object".to_string(),
            classification: "entity".to_string(),
            domain: Some("grants".to_string()),
            namespace: None,
            rel_path: format!("grants/{title}.json"),
            pg_type: "TABLE".to_string(),
            rust_type: "GrantApplication".to_string(),
            sea_orm_type: "Entity".to_string(),
            rust_type_name: "GrantApplication".to_string(),
            pg_table_name: table.to_string(),
            api_path_segment: "grant-applications".to_string(),
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

    fn amount_prop() -> PropertyNode {
        PropertyNode {
            name: "amount".to_string(),
            prop_type: "number".to_string(),
            description: None,
            format: None,
            is_required: true,
            is_nullable: false,
            is_id: false,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "amount".to_string(),
            pg_column_type: "NUMERIC".to_string(),
            rust_field_name: "amount".to_string(),
            rust_field_type: "f64".to_string(),
            sea_orm_type: "Decimal".to_string(),
            render_strategy: "primitive".to_string(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
            type_expr: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
    }

    fn design(class: &str, title: Option<&str>, invariants: Vec<DddInvariant>) -> DddDesignNode {
        DddDesignNode {
            application: "Grants".to_string(),
            module: "grants".to_string(),
            class: class.to_string(),
            resolved_title: title.map(str::to_string),
            stereotype: "entity".to_string(),
            is_abstract: false,
            flags: DddDesignFlags::default(),
            machine_json: None,
            child_operations: Vec::new(),
            aggregate_boundary: None,
            invariants,
            ordinal: 0,
        }
    }

    fn model(designs: Vec<DddDesignNode>) -> DddModelGraph {
        DddModelGraph {
            source_path: "grants.ddd".to_string(),
            application: codegraph_core::types::DddApplicationNode {
                name: "Grants".to_string(),
                base: None,
                source_path: "grants.ddd".to_string(),
                errors: Vec::new(),
            },
            modules: Vec::new(),
            designs,
            repositories: Vec::new(),
            services: Vec::new(),
            searches: Vec::new(),
            dto_shapes: Vec::new(),
        }
    }

    async fn mock(designs: Vec<DddDesignNode>) -> MockEngine {
        let db = MockEngine::builder()
            .with_schema(grants_schema("GrantApplicationType", "grant_application"))
            .with_properties("GrantApplicationType", vec![amount_prop()])
            .build();
        db.ingest_ddd_model(&model(designs)).await.unwrap();
        db
    }

    #[tokio::test]
    async fn plans_carry_the_declaration_order_index_and_fn_name() {
        let db = mock(vec![design(
            "GrantApplicationType",
            Some("GrantApplicationType"),
            vec![
                DddInvariant {
                    expression: "amount > 0.0".to_string(),
                },
                DddInvariant {
                    expression: "amount < 1000.0".to_string(),
                },
            ],
        )])
        .await;
        let config = grants_config();
        let plans = invariants_for(&db, &config, "grants", "GrantApplicationType").await;
        assert_eq!(plans.len(), 2, "both invariants lower: {plans:?}");
        assert_eq!(plans[0].index, 0);
        assert_eq!(plans[1].index, 1);
        assert_eq!(plans[0].fn_name, "grant_application_type_invariant_0");
        assert_eq!(plans[1].fn_name, "grant_application_type_invariant_1");
        assert_eq!(plans[0].expression, "amount > 0.0");
        assert_eq!(plans[0].predicate_code, "(input.amount > 0.0)");
        assert_eq!(plans[1].predicate_code, "(input.amount < 1000.0)");
        assert!(
            invariant_refusals(&db, &config, "grants", "GrantApplicationType")
                .await
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_refused_invariant_leaves_the_plans_complete() {
        let db = mock(vec![design(
            "GrantApplicationType",
            Some("GrantApplicationType"),
            vec![
                DddInvariant {
                    expression: "amount > 0.0".to_string(),
                },
                DddInvariant {
                    expression: "amount + 1 > 0".to_string(),
                },
            ],
        )])
        .await;
        let config = grants_config();
        let plans = invariants_for(&db, &config, "grants", "GrantApplicationType").await;
        assert_eq!(
            plans.len(),
            1,
            "only the lowerable invariant emits a plan: {plans:?}"
        );
        assert_eq!(plans[0].index, 0);
        let refusals = invariant_refusals(&db, &config, "grants", "GrantApplicationType").await;
        assert_eq!(refusals.len(), 1, "{refusals:?}");
        assert!(
            refusals[0].contains("invariant 1 of `GrantApplicationType` in domain `grants`"),
            "the refusal names the domain, class and index: {refusals:?}"
        );
        assert!(refusals[0].contains("arithmetic"), "{refusals:?}");
        assert!(
            refusals[0].contains("not emitted"),
            "the refusal states the fail-closed contract: {refusals:?}"
        );
    }

    #[tokio::test]
    async fn all_invariant_refusals_walks_every_design_and_names_unresolved_classes() {
        let db = mock(vec![
            design(
                "GrantApplicationType",
                Some("GrantApplicationType"),
                vec![DddInvariant {
                    expression: "amount > 0.0".to_string(),
                }],
            ),
            // An unresolved class: its invariant cannot lower against any
            // record — named, never silently dropped.
            design(
                "GhostType",
                None,
                vec![DddInvariant {
                    expression: "amount > 0.0".to_string(),
                }],
            ),
        ])
        .await;
        let config = grants_config();
        let refusals = all_invariant_refusals(&db, &config).await.unwrap();
        assert_eq!(
            refusals.len(),
            1,
            "the resolved design lowers: {refusals:?}"
        );
        assert!(
            refusals[0].contains("design `GhostType` resolves to no ingested schema"),
            "the unresolved class is named: {refusals:?}"
        );
    }

    #[tokio::test]
    async fn no_invariants_means_no_plans_no_refusals() {
        let db = mock(vec![design(
            "GrantApplicationType",
            Some("GrantApplicationType"),
            Vec::new(),
        )])
        .await;
        let config = grants_config();
        assert!(
            invariants_for(&db, &config, "grants", "GrantApplicationType")
                .await
                .is_empty()
        );
        assert!(
            invariant_refusals(&db, &config, "grants", "GrantApplicationType")
                .await
                .is_empty()
        );
        assert!(
            all_invariant_refusals(&db, &config)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn undeclared_domains_refuse_nothing_and_plan_nothing() {
        let db = mock(vec![design(
            "GrantApplicationType",
            Some("GrantApplicationType"),
            vec![DddInvariant {
                expression: "amount > 0.0".to_string(),
            }],
        )])
        .await;
        let config = grants_config();
        assert!(
            invariants_for(&db, &config, "elsewhere", "GrantApplicationType")
                .await
                .is_empty()
        );
    }

    #[test]
    fn fields_for_keys_the_authored_name_and_the_snake_spelling() {
        let fields = fields_for(&[amount_prop()]);
        assert!(fields.contains_key("amount"));
        let info = &fields["amount"];
        assert_eq!(info.rust_name, "amount");
        assert!(!info.optional);
    }
}
