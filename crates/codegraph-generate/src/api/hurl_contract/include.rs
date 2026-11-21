//! Behavioural `?include=` contract legs (issue #781 P2).
//!
//! The generated hurl contracts never exercised `?include=` before this
//! module — which is exactly why the cornucopia defect (#781) stayed
//! invisible for months. Every entity whose handler hydrates includes now
//! ships deterministic include legs, resolved through the SAME gate the
//! handler uses ([`crate::api::include_path::resolve_include_paths_for_entity`])
//! so the contract cannot assert a surface the handler does not serve.
//!
//! ## Provider gate (issue #781)
//!
//! Cornucopia hydrates includes by DEFAULT (issue #781 P1 landed), so a
//! cornucopia project gets the SAME provable legs as SeaORM. The only
//! provider-level omission is [`IncludeSupport::Unsupported`] — a persistence
//! provider that cannot hydrate at all answers a guarded 501, so it gets one
//! comment line and no legs. The gate reads
//! [`crate::api::include_plan::include_support_for`], the SAME predicate the
//! handler's hydration plan reads, so the contract cannot advertise a surface
//! the handler refuses. Per-path verdicts refine it: a shape the cornucopia
//! plane does not implement gets its own omission comment (never a leg).
//!
//! ## Provability (never guess a status or a payload)
//!
//! A leg that can fail on valid data is worse than an omitted leg, so each
//! path is only asserted when the contract PROVES the hydration on the row it
//! just created (the `#662` `may_refuse` / `#663` three-verdict discipline):
//!
//! * **array / junction first segment** — the SeaORM GET block assigns
//!   `included.<seg> = Some(repo.fetch_…())` unconditionally, so
//!   `$.included.<seg>` is non-null on a fresh row (an empty vec is still a
//!   present value);
//! * **scalar first segment** — only asserted when the create body PROVABLY
//!   wires that FK (a required entity ref whose parent the preamble POST
//!   creates and whose id the body sends). A scalar include over an optional
//!   FK is not provable on a fresh row — it gets an omission comment naming
//!   the path;
//! * **dot-notation path** — the combined response types every leaf as
//!   `Option<{Leaf}Response>` ([`crate::ddd::dto`]'s `build_combined_group`),
//!   and the merge only writes it when the hop resolves, so no dot path is
//!   provable on a fresh row: the path gets an omission comment instead.
//!
//! LIST legs assert the envelope only (`$.included` exists, plus the
//! single-segment key when the path is single-segment) — the batch map is
//! keyed by source id, so per-row assertions belong to the GET leg.
//!
//! ## Soft-delete exclusion (issue #784)
//!
//! A provable ARRAY include gains one deterministic exclusion leg: POST a
//! related row wired to the captured source, DELETE it (an auditable target
//! soft-deletes), then GET the source with `?include=` and assert the
//! collection is EMPTY (`$.included.<seg> == []` — the array wrapper keeps
//! the key present). The gates mirror the emitter's own derivation: the leg
//! is emitted only when the TARGET is auditable per the repository emitter's
//! own derivation input (`EntityTree.is_auditable`, the tree the repository
//! plane builds) — otherwise the DELETE would hard-delete and the leg would
//! prove nothing — plus create/delete ops on the target and a resolvable
//! schema property carrying the reverse FK (the JSON key the target's create
//! DTO expects). Every unmet gate gets its own omission comment; nothing is
//! guessed.

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::SchemaNode;
use codegraph_type_contracts::RefClassificationKind;

use super::ContractSection;
use crate::ProjectConfig;
use crate::api::include_path::{ResolvedIncludePath, resolve_include_paths_for_entity};
use crate::api::include_plan::{IncludePathSupport, IncludeSupport, include_support_for};
use crate::error::Result;

/// The one line a non-hydrating provider emits instead of legs (issue #781).
const UNSUPPORTED_NOTE: &str = "# ?include= — omitted: this deployment's persistence provider answers a\n\
# guarded 501 for include requests (issue #781).";

/// Build the `?include=` legs for one entity's contract. Empty when the
/// entity models no include surface — byte-identical contract.
#[allow(clippy::too_many_arguments)]
pub(super) async fn include_sections(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    project: &ProjectConfig,
    domain: &str,
    schema: &SchemaNode,
    item_url_base: &str,
    capture_ref: &str,
    list_url: &str,
    create_body: &str,
    has_create: bool,
    has_read: bool,
    has_list: bool,
    api_version: &str,
) -> Result<Vec<ContractSection>> {
    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(&schema.title));
    let paths = resolve_include_paths_for_entity(
        db,
        config,
        domain,
        &schema.title,
        entity_cfg,
        project.is_workers_topology(),
    )
    .await?;
    let soft_delete_facts =
        soft_delete_leg_facts(db, config, schema, &paths, capture_ref, api_version).await?;
    Ok(sections_for_paths(
        &paths,
        &include_support_for(project),
        item_url_base,
        capture_ref,
        list_url,
        create_body,
        has_create,
        has_read,
        has_list,
        &soft_delete_facts,
    ))
}

/// The #784 soft-delete exclusion facts for every ARRAY include path whose
/// leg is provable: the target's POST URL and a create body wiring the
/// reverse FK to the source's capture. Paths whose facts do not resolve have
/// no entry — `sections_for_paths` renders their omission comment.
///
/// The auditable gate reads the repository emitter's OWN tree
/// ([`crate::ddd::repository_emitter::RepositoryImplEmitter::query_entity_tree`])
/// — the same `is_auditable` the include fetch filter derives from — so the
/// contract cannot pin a leg the emitted code would answer differently. A
/// non-auditable target hard-deletes on DELETE, which would make this leg
/// pass even without the #784 filter: it proves nothing and is omitted.
#[allow(clippy::too_many_arguments)]
async fn soft_delete_leg_facts(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    schema: &SchemaNode,
    paths: &[ResolvedIncludePath],
    capture_ref: &str,
    api_version: &str,
) -> Result<std::collections::BTreeMap<String, SoftDeleteLegFacts>> {
    let mut facts = std::collections::BTreeMap::new();
    let source_title = schema.title.clone();
    for path in paths {
        let Some(first) = path.segments.first() else {
            continue;
        };
        if !first.is_array {
            continue;
        }
        let Ok(target_tree) = crate::ddd::repository_emitter::RepositoryImplEmitter
            .query_entity_tree(db, &first.schema_title, &first.domain, config, None)
            .await
        else {
            facts.insert(path.alias.clone(), SoftDeleteLegFacts::Omitted);
            continue;
        };
        if !target_tree.is_auditable {
            facts.insert(path.alias.clone(), SoftDeleteLegFacts::OmittedNonAuditable);
            continue;
        }
        let ops = crate::api::api_model::resolve_entity_operations(
            db,
            config,
            &first.domain,
            &first.schema_title,
        )
        .await;
        let has_op = |op: &str| ops.iter().any(|o| o == op);
        if !(has_op("create") && has_op("delete")) {
            facts.insert(path.alias.clone(), SoftDeleteLegFacts::Omitted);
            continue;
        }
        let Some(target) = db.get_schema(&first.schema_title).await.ok().flatten() else {
            facts.insert(path.alias.clone(), SoftDeleteLegFacts::Omitted);
            continue;
        };
        // The JSON key the target's create DTO expects for the reverse FK —
        // the scalar schema property pointing back at the source, the same
        // `resolve_field` spelling `payload_fields` emits. (Junction arrays
        // are excluded: the resolver refuses them, and a junction FK never
        // lands in the create body as a scalar capture.)
        let props = db.get_properties(&target.title).await?;
        let type_suffix = &config.defaults.type_suffix;
        let fk_json_name = props
            .iter()
            .filter(|p| {
                !p.is_array && p.effective_kind() == Some(RefClassificationKind::EntityReference)
            })
            .filter_map(|p| {
                let target_name = p.ref_target.as_ref()?;
                let points_at_source = target_name == &source_title
                    || target_name == &format!("{source_title}{type_suffix}");
                points_at_source.then(|| codegraph_core::types::resolve_field(p).rust_field_name)
            })
            .next();
        let Some(fk_json_name) = fk_json_name else {
            facts.insert(path.alias.clone(), SoftDeleteLegFacts::Omitted);
            continue;
        };
        let target_body =
            super::minimal_body_for(db, config, &first.domain, &target, api_version, 0).await?;
        // Wire the reverse FK to the source's captured id (the body is
        // required-fields-only; the reverse FK may be optional).
        let target_create_body = inject_fk_capture(&target_body, &fk_json_name, capture_ref);
        let target_segment = super::route_segment_for(config, &first.domain, &target).await;
        facts.insert(
            path.alias.clone(),
            SoftDeleteLegFacts::Provable {
                target_post_url: format!(
                    "{{{{base_url}}}}/api/{api_version}/{}/{target_segment}",
                    first.domain
                ),
                target_create_body,
                capture: capture_ref.to_string(),
            },
        );
    }
    Ok(facts)
}

/// Insert `"fk": capture` as the first member of the minimal body's JSON
/// object (or build the object when the body carries no required fields),
/// keeping the two-space contract indentation.
fn inject_fk_capture(body: &str, fk_json_name: &str, capture_ref: &str) -> String {
    let line = format!("  \"{fk_json_name}\": \"{capture_ref}\"");
    let Some(stripped) = body
        .trim()
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
    else {
        return body.to_string();
    };
    let inner = stripped.trim();
    if inner.is_empty() {
        return format!("{{\n{line}\n}}");
    }
    // Normalize the existing members to the contract's two-space indentation.
    let members = inner
        .lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n  ");
    format!("{{\n{line},\n  {members}\n}}")
}

/// The #784 soft-delete leg's per-path resolution.
pub(super) enum SoftDeleteLegFacts {
    /// The leg is provable: POST the target, DELETE it, GET the source,
    /// assert the empty collection.
    Provable {
        target_post_url: String,
        target_create_body: String,
        capture: String,
    },
    /// Omitted: the target has no create/delete ops, no schema, or no
    /// resolvable reverse-FK property.
    Omitted,
    /// Omitted: a non-auditable target hard-deletes on DELETE — the leg
    /// would pass with OR without the #784 filter, proving nothing.
    OmittedNonAuditable,
}

/// The pure half: resolved paths + the provider verdict → rendered sections.
/// Split out so the provability rules are unit-testable without a graph.
#[allow(clippy::too_many_arguments)]
pub(super) fn sections_for_paths(
    paths: &[ResolvedIncludePath],
    support: &IncludeSupport,
    item_url_base: &str,
    capture_ref: &str,
    list_url: &str,
    create_body: &str,
    has_create: bool,
    has_read: bool,
    has_list: bool,
    soft_delete_facts: &std::collections::BTreeMap<String, SoftDeleteLegFacts>,
) -> Vec<ContractSection> {
    if paths.is_empty() {
        return Vec::new();
    }
    if !support.is_supported() {
        return vec![section(UNSUPPORTED_NOTE.to_string())];
    }

    let mut sections = Vec::new();
    for path in paths {
        // Per-path verdict (issue #781 P1): a provider that hydrates SOME
        // shapes must not advertise the ones it refuses — the handler would
        // answer a 501 for them.
        if let IncludePathSupport::Unsupported { reason } = support.path_support(path) {
            sections.push(section(omitted_unsupported_path(path, &reason)));
            continue;
        }
        let Some(first) = path.segments.first() else {
            continue;
        };
        if path.segments.len() > 1 {
            sections.push(section(omitted_dot_path(path)));
            continue;
        }
        let provable = first.is_array
            || (first.fk_is_required && create_body_wires_fk(create_body, &first.fk_column));
        if !provable {
            sections.push(section(omitted_scalar_path(path)));
            continue;
        }
        // The GET leg addresses the row the CREATE leg captured — without a
        // create operation there is no id to include on.
        if has_read && has_create {
            sections.push(section(read_leg(
                item_url_base,
                capture_ref,
                &path.alias,
                &first.module_name,
            )));
        }
        if has_list {
            sections.push(section(list_leg(list_url, &path.alias, &first.module_name)));
        }
        // The #784 soft-delete exclusion leg — array shapes only (the
        // collection wrapper keeps `$.included.<seg>` present when empty, so
        // `== []` is provable after the delete).
        if first.is_array {
            match soft_delete_facts.get(&path.alias) {
                Some(SoftDeleteLegFacts::Provable {
                    target_post_url,
                    target_create_body,
                    capture,
                }) => {
                    sections.push(section(soft_delete_leg(
                        item_url_base,
                        target_post_url,
                        target_create_body,
                        capture,
                        &path.alias,
                        &first.module_name,
                    )));
                }
                Some(SoftDeleteLegFacts::OmittedNonAuditable) => {
                    sections.push(section(omitted_soft_delete_non_auditable(path)));
                }
                Some(SoftDeleteLegFacts::Omitted) | None => {}
            }
        }
    }
    sections
}

fn section(body: String) -> ContractSection {
    ContractSection { body }
}

/// Does the contract's create body send this FK? The body is the minimal
/// required-field payload, so a FK it carries is a non-null column on the row
/// the GET leg fetches.
fn create_body_wires_fk(create_body: &str, fk_column: &str) -> bool {
    create_body.contains(&format!("\"{fk_column}\":"))
}

/// GET-by-id: the include must hydrate the relation on the created row.
fn read_leg(item_url_base: &str, capture_ref: &str, alias: &str, seg: &str) -> String {
    format!(
        "# ?include={alias} — GET hydrates the relation (issue #781 probe)\n\
         GET {item_url_base}/{capture_ref}?include={alias}\n\
         Authorization: Bearer {{{{api_key}}}}\n\
         \n\
         HTTP 200\n\
         [Asserts]\n\
         jsonpath \"$.included.{seg}\" != null"
    )
}

/// LIST: the batch map is present and the first segment's key exists for a
/// single-segment path (per-row membership needs the GET leg).
fn list_leg(list_url: &str, alias: &str, seg: &str) -> String {
    format!(
        "# ?include={alias} — LIST hydrates the batch map\n\
         GET {list_url}&include={alias}\n\
         Authorization: Bearer {{{{api_key}}}}\n\
         \n\
         HTTP 200\n\
         [Asserts]\n\
         jsonpath \"$.included\" exists\n\
         jsonpath \"$.included.{seg}\" exists"
    )
}

/// A scalar include whose FK the contract cannot prove is set on a fresh row.
fn omitted_scalar_path(path: &ResolvedIncludePath) -> String {
    format!(
        "# ?include={alias} — omitted: this contract's create body does not wire the\n\
         # FK, so a fresh row cannot prove a scalar include hydrates (a row-dependent\n\
         # assertion would fail in either direction — the #662 may_refuse rule).",
        alias = path.alias
    )
}

/// The #784 soft-delete exclusion leg: POST a related row wired to the
/// captured source, DELETE it (the auditable target soft-deletes), then GET
/// the source with `?include=` and assert the collection is EMPTY. The array
/// wrapper keeps `$.included.<seg>` present — `== []` proves the emitted
/// soft-delete filter rather than a missing key.
#[allow(clippy::too_many_arguments)]
fn soft_delete_leg(
    item_url_base: &str,
    target_post_url: &str,
    target_create_body: &str,
    capture: &str,
    alias: &str,
    seg: &str,
) -> String {
    let deleted_capture = format!("deleted_{seg}_id");
    format!(
        "# ?include={alias} — a soft-deleted {seg} is EXCLUDED (issue #784)\n\
         POST {target_post_url}\n\
         Authorization: Bearer {{{{api_key}}}}\n\
         {target_create_body}\n\
         \n\
         HTTP 201\n\
         [Captures]\n\
         {deleted_capture}: jsonpath \"$.data.id\"\n\
         \n\
         DELETE {target_post_url}/{{{{{deleted_capture}}}}}\n\
         Authorization: Bearer {{{{api_key}}}}\n\
         \n\
         HTTP 204\n\
         \n\
         GET {item_url_base}/{capture}?include={alias}\n\
         Authorization: Bearer {{{{api_key}}}}\n\
         \n\
         HTTP 200\n\
         [Asserts]\n\
         jsonpath \"$.included.{seg}\" == []"
    )
}

/// A provable array include whose TARGET is not auditable: the DELETE would
/// hard-delete, so the exclusion leg would pass with OR without the #784
/// filter — proving nothing.
fn omitted_soft_delete_non_auditable(path: &ResolvedIncludePath) -> String {
    format!(
        "# ?include={alias} — no soft-delete exclusion leg: the related entity is not\n\
         # auditable, so its DELETE hard-deletes and the leg would prove nothing\n\
         # (the #662 may_refuse rule).",
        alias = path.alias
    )
}

/// A path whose SHAPE the deployment's provider does not implement (issue
/// #781 P1): the handler answers a 501 for it, so the contract omits the leg
/// rather than advertising a surface that is refused.
fn omitted_unsupported_path(path: &ResolvedIncludePath, reason: &str) -> String {
    format!(
        "# ?include={alias} — omitted: this shape is not implemented on the\n\
         # deployment's persistence provider ({reason}; issue #781).",
        alias = path.alias
    )
}

/// A dot-notation path: the combined response's leaf is an `Option`, and the
/// merge only writes it when the hop resolves.
fn omitted_dot_path(path: &ResolvedIncludePath) -> String {
    format!(
        "# ?include={alias} — omitted: the combined response types the leaf as an\n\
         # Option and only fills it when the hop resolves, so a fresh row cannot\n\
         # prove it hydrates (the #662 may_refuse rule).",
        alias = path.alias
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::include_path::IncludeSegment;

    fn segment(
        module: &str,
        fk_column: &str,
        is_array: bool,
        fk_is_required: bool,
    ) -> IncludeSegment {
        IncludeSegment {
            entity_name: codegraph_naming::to_pascal_case(module),
            schema_title: format!("{}Type", codegraph_naming::to_pascal_case(module)),
            module_name: module.to_string(),
            domain: "inventory".to_string(),
            table: format!("\"inventory\".\"{module}\""),
            fk_column: fk_column.to_string(),
            reverse_fk_column: fk_column.to_string(),
            fk_is_required,
            reverse_fk_is_required: false,
            is_array,
            child_table_override: None,
        }
    }

    fn path(alias: &str, segments: Vec<IncludeSegment>) -> ResolvedIncludePath {
        ResolvedIncludePath {
            alias: alias.to_string(),
            segments,
            response_rust_type: "PartResponse".to_string(),
            fetch_method: "fetch_part_for_widget".to_string(),
            batch_fetch_method: "fetch_part_batch_for_widget".to_string(),
        }
    }

    const ITEM: &str = "{{base_url}}/api/v1/inventory/widget";
    const CAPTURE: &str = "{{widget_id}}";
    const LIST: &str = "{{base_url}}/api/v1/inventory/widget?page=0&page_size=10";

    fn render(paths: &[ResolvedIncludePath], create_body: &str) -> String {
        sections_for_paths(
            paths,
            &IncludeSupport::SeaOrm,
            ITEM,
            CAPTURE,
            LIST,
            create_body,
            true,
            true,
            true,
            &std::collections::BTreeMap::new(),
        )
        .into_iter()
        .map(|s| s.body)
        .collect::<Vec<_>>()
        .join("\n")
    }

    /// An array/junction first segment is unconditionally assigned on GET, so
    /// both legs are emitted with a `!= null` field assertion.
    #[test]
    fn array_include_emits_both_legs() {
        let paths = vec![path(
            "parts",
            vec![segment("part", "widget_id", true, false)],
        )];
        let out = render(&paths, "{}");

        assert!(
            out.contains("GET {{base_url}}/api/v1/inventory/widget/{{widget_id}}?include=parts"),
            "the GET leg addresses the created row with the alias:\n{out}"
        );
        assert!(
            out.contains("jsonpath \"$.included.part\" != null"),
            "an array include is non-null even when empty:\n{out}"
        );
        assert!(
            out.contains(
                "GET {{base_url}}/api/v1/inventory/widget?page=0&page_size=10&include=parts"
            ),
            "the LIST leg composes the include onto the base query:\n{out}"
        );
        assert!(
            out.contains("jsonpath \"$.included\" exists"),
            "the list envelope asserts the batch map:\n{out}"
        );
        assert!(out.contains("jsonpath \"$.included.part\" exists"), "{out}");
        assert!(!out.contains("omitted"), "no omission comment:\n{out}");
    }

    /// A scalar include over an FK the create body does not wire is NOT
    /// provable on a fresh row: an omission comment naming the path, no legs.
    #[test]
    fn scalar_include_without_a_wired_fk_is_omitted_with_a_comment() {
        let paths = vec![path("part", vec![segment("part", "part_id", false, false)])];
        let out = render(&paths, "{\n  \"name\": \"contract-widget\"\n}");

        assert!(out.contains("# ?include=part — omitted:"), "{out}");
        assert!(
            out.contains("#662 may_refuse"),
            "the omission names the verdict discipline:\n{out}"
        );
        assert!(!out.contains("GET "), "no leg is emitted:\n{out}");
        assert!(!out.contains("HTTP 200"), "no status is guessed:\n{out}");
    }

    /// The same scalar include IS provable once the create body wires the FK
    /// (a required entity ref whose row the preamble POST created).
    #[test]
    fn scalar_include_with_a_wired_required_fk_is_emitted() {
        let paths = vec![path("part", vec![segment("part", "part_id", false, true)])];
        let out = render(
            &paths,
            "{\n  \"name\": \"contract-widget\",\n  \"part_id\": \"{{parent_part_id}}\"\n}",
        );

        assert!(
            out.contains("GET {{base_url}}/api/v1/inventory/widget/{{widget_id}}?include=part"),
            "{out}"
        );
        assert!(
            out.contains("jsonpath \"$.included.part\" != null"),
            "{out}"
        );
        assert!(!out.contains("omitted"), "no omission comment:\n{out}");
    }

    /// A wired FK on a NON-required segment is still unprovable if the body
    /// carries it (optionality means the row may null it) — but the resolver
    /// decides requiredness, and the gate requires BOTH facts.
    #[test]
    fn scalar_include_requires_both_required_and_wired() {
        let paths = vec![path("part", vec![segment("part", "part_id", false, false)])];
        let out = render(&paths, "{\n  \"part_id\": \"{{parent_part_id}}\"\n}");
        assert!(out.contains("omitted"), "{out}");
        assert!(!out.contains("HTTP 200"), "{out}");
    }

    /// Dot-notation paths carry an omission comment (the combined leaf is an
    /// `Option`), never a guessed leg.
    #[test]
    fn dot_notation_path_is_omitted_with_a_comment() {
        let paths = vec![path(
            "deployment.position",
            vec![
                segment("deployment", "widget_id", true, false),
                segment("position", "deployment_id", false, true),
            ],
        )];
        let out = render(&paths, "{}");
        assert!(
            out.contains("# ?include=deployment.position — omitted:"),
            "{out}"
        );
        assert!(!out.contains("HTTP 200"), "{out}");
    }

    /// Issue #781 P1 landed: a cornucopia project hydrates by default, so it
    /// gets the SAME provable legs SeaORM does (array/junction is
    /// unconditional).
    #[test]
    fn cornucopia_emits_the_same_provable_legs_as_sea_orm() {
        let paths = vec![path(
            "parts",
            vec![segment("part", "widget_id", true, false)],
        )];
        let project = ProjectConfig {
            database: crate::project_config::DatabaseConfig {
                persistence_provider: crate::profile::PersistenceProvider::Cornucopia,
                ..Default::default()
            },
            ..Default::default()
        };
        let support = include_support_for(&project);
        assert!(support.is_supported(), "cornucopia hydrates by default");
        assert_eq!(support, IncludeSupport::CornucopiaHydration);

        let out = sections_for_paths(
            &paths,
            &support,
            ITEM,
            CAPTURE,
            LIST,
            "{}",
            true,
            true,
            true,
            &std::collections::BTreeMap::new(),
        )
        .into_iter()
        .map(|s| s.body)
        .collect::<Vec<_>>()
        .join("\n");
        assert!(
            out.contains("GET {{base_url}}/api/v1/inventory/widget/{{widget_id}}?include=parts"),
            "{out}"
        );
        assert!(
            out.contains("jsonpath \"$.included.part\" != null"),
            "{out}"
        );
        assert!(
            out.contains(
                "GET {{base_url}}/api/v1/inventory/widget?page=0&page_size=10&include=parts"
            ),
            "{out}"
        );
        assert!(
            !out.contains("guarded 501 for include requests"),
            "no provider-level omission:\n{out}"
        );
        assert!(!out.contains("omitted"), "no omission comment:\n{out}");
    }

    /// A genuinely unsupported PROVIDER (no production one exists — injected)
    /// still gets ONE comment line and NO legs, because its handler 501s.
    #[test]
    fn unsupported_provider_emits_the_781_comment_and_no_legs() {
        let paths = vec![path(
            "parts",
            vec![segment("part", "widget_id", true, false)],
        )];
        let support = IncludeSupport::Unsupported {
            reason: "persistence provider `diesel` has no include fetch plane".to_string(),
        };
        let out = sections_for_paths(
            &paths,
            &support,
            ITEM,
            CAPTURE,
            LIST,
            "{}",
            true,
            true,
            true,
            &std::collections::BTreeMap::new(),
        )
        .into_iter()
        .map(|s| s.body)
        .collect::<Vec<_>>()
        .join("\n");
        assert!(out.contains("(issue #781)"), "{out}");
        assert!(out.contains("501"), "{out}");
        assert!(!out.contains("GET "), "no legs for a 501 provider:\n{out}");
        assert!(!out.contains("HTTP 200"), "{out}");
    }

    /// Issue #781 P1: a provider that hydrates SOME shapes advertises only
    /// those — an unimplemented shape is omitted with the reason, never a leg
    /// the handler would 501.
    #[test]
    fn cornucopia_hydration_omits_only_the_unsupported_shapes() {
        let paths = vec![
            path("part", vec![segment("part", "part_id", false, true)]),
            path(
                "part.vendor.country",
                vec![
                    segment("part", "part_id", false, true),
                    segment("vendor", "part_id", false, false),
                    segment("country", "vendor_id", false, false),
                ],
            ),
        ];
        let out = sections_for_paths(
            &paths,
            &IncludeSupport::CornucopiaHydration,
            ITEM,
            CAPTURE,
            LIST,
            "{\"part_id\": \"00000000-0000-0000-0000-000000000001\"}",
            true,
            true,
            true,
            &std::collections::BTreeMap::new(),
        )
        .into_iter()
        .map(|s| s.body)
        .collect::<Vec<_>>()
        .join("\n");

        assert!(
            out.contains("GET {{base_url}}/api/v1/inventory/widget/{{widget_id}}?include=part"),
            "the implemented scalar shape gets its GET leg:\n{out}"
        );
        assert!(
            out.contains(
                "# ?include=part.vendor.country — omitted: this shape is not implemented on the"
            ),
            "the depth-3 shape is omitted with the reason:\n{out}"
        );
        assert!(
            !out.contains("include=part.vendor.country\n"),
            "no leg may address the unsupported shape:\n{out}"
        );
    }

    /// A SUPPORTED one-hop dot path is still unprovable on a fresh row (its
    /// leaf is an `Option`), so it gets the omission comment — never a leg.
    #[test]
    fn supported_dot_path_is_omitted_as_unprovable() {
        let paths = vec![path(
            "part.vendor",
            vec![
                segment("part", "part_id", false, true),
                segment("vendor", "part_id", false, false),
            ],
        )];
        let out = sections_for_paths(
            &paths,
            &IncludeSupport::CornucopiaHydration,
            ITEM,
            CAPTURE,
            LIST,
            "{}",
            true,
            true,
            true,
            &std::collections::BTreeMap::new(),
        )
        .into_iter()
        .map(|s| s.body)
        .collect::<Vec<_>>()
        .join("\n");

        assert!(
            out.contains(
                "# ?include=part.vendor — omitted: the combined response types the leaf as a"
            ),
            "{out}"
        );
        assert!(
            !out.contains("GET "),
            "no leg may address the unprovable dot shape:\n{out}"
        );
    }

    /// No modelled include surface ⇒ no sections at all (byte-identical
    /// contract).
    #[test]
    fn no_paths_emits_nothing() {
        assert!(
            sections_for_paths(
                &[],
                &IncludeSupport::SeaOrm,
                ITEM,
                CAPTURE,
                LIST,
                "{}",
                true,
                true,
                true,
                &std::collections::BTreeMap::new()
            )
            .is_empty()
        );
    }

    /// Legs follow the entity's resolved operations: a read-less entity gets
    /// only the LIST leg, a list-less entity only the GET leg.
    #[test]
    fn legs_follow_the_resolved_operations() {
        let paths = vec![path(
            "parts",
            vec![segment("part", "widget_id", true, false)],
        )];
        let only_list = sections_for_paths(
            &paths,
            &IncludeSupport::SeaOrm,
            ITEM,
            CAPTURE,
            LIST,
            "{}",
            true,
            false,
            true,
            &std::collections::BTreeMap::new(),
        );
        assert_eq!(only_list.len(), 1);
        assert!(
            only_list[0]
                .body
                .contains("GET {{base_url}}/api/v1/inventory/widget?page")
        );

        let only_read = sections_for_paths(
            &paths,
            &IncludeSupport::SeaOrm,
            ITEM,
            CAPTURE,
            LIST,
            "{}",
            true,
            true,
            false,
            &std::collections::BTreeMap::new(),
        );
        assert_eq!(only_read.len(), 1);
        assert!(only_read[0].body.contains("?include=parts"));
    }

    /// The GET leg addresses the row the CREATE leg captured: an entity that
    /// exposes no create operation gets only the LIST leg.
    #[test]
    fn read_leg_requires_a_create_operation() {
        let paths = vec![path(
            "parts",
            vec![segment("part", "widget_id", true, false)],
        )];
        let sections = sections_for_paths(
            &paths,
            &IncludeSupport::SeaOrm,
            ITEM,
            CAPTURE,
            LIST,
            "{}",
            false,
            true,
            true,
            &std::collections::BTreeMap::new(),
        );
        assert_eq!(sections.len(), 1);
        assert!(
            !sections[0].body.contains("{{widget_id}}"),
            "no item leg without a create capture:\n{}",
            sections[0].body
        );
        assert!(sections[0].body.contains("page_size=10&include=parts"));
    }

    /// The #784 soft-delete exclusion leg: a provable array include whose
    /// target facts resolve emits POST → DELETE → GET with the EMPTY
    /// collection assert (`== []` — the array wrapper keeps the key present).
    #[test]
    fn soft_delete_exclusion_leg_is_emitted_for_provable_arrays() {
        let paths = vec![path(
            "parts",
            vec![segment("part", "widget_id", true, false)],
        )];
        let facts = std::collections::BTreeMap::from([(
            "parts".to_string(),
            SoftDeleteLegFacts::Provable {
                target_post_url: "{{base_url}}/api/v1/inventory/part".to_string(),
                target_create_body:
                    "{\n  \"widget_id\": \"{{widget_id}}\",\n  \"name\": \"contract-part\"\n}"
                        .to_string(),
                capture: CAPTURE.to_string(),
            },
        )]);
        let out = sections_for_paths(
            &paths,
            &IncludeSupport::SeaOrm,
            ITEM,
            CAPTURE,
            LIST,
            "{}",
            true,
            true,
            true,
            &facts,
        )
        .into_iter()
        .map(|s| s.body)
        .collect::<Vec<_>>()
        .join("\n");

        assert!(
            out.contains("# ?include=parts — a soft-deleted part is EXCLUDED (issue #784)"),
            "{out}"
        );
        assert!(
            out.contains("POST {{base_url}}/api/v1/inventory/part"),
            "the related row is POSTed first:\n{out}"
        );
        assert!(
            out.contains("\"widget_id\": \"{{widget_id}}\""),
            "the related row's body wires the reverse FK to the source capture:\n{out}"
        );
        assert!(
            out.contains("[Captures]\ndeleted_part_id: jsonpath \"$.data.id\""),
            "{out}"
        );
        assert!(
            out.contains("DELETE {{base_url}}/api/v1/inventory/part/{{deleted_part_id}}"),
            "{out}"
        );
        assert!(
            out.contains("jsonpath \"$.included.part\" == []"),
            "the exclusion assert is the EMPTY collection, not a missing key:\n{out}"
        );
    }

    /// A non-auditable target's exclusion leg is OMITTED with a comment: a
    /// hard-deleting target would make the leg pass with OR without the #784
    /// filter — proving nothing.
    #[test]
    fn soft_delete_leg_is_omitted_for_a_non_auditable_target() {
        let paths = vec![path(
            "parts",
            vec![segment("part", "widget_id", true, false)],
        )];
        let facts = std::collections::BTreeMap::from([(
            "parts".to_string(),
            SoftDeleteLegFacts::OmittedNonAuditable,
        )]);
        let out = sections_for_paths(
            &paths,
            &IncludeSupport::SeaOrm,
            ITEM,
            CAPTURE,
            LIST,
            "{}",
            true,
            true,
            true,
            &facts,
        )
        .into_iter()
        .map(|s| s.body)
        .collect::<Vec<_>>()
        .join("\n");

        assert!(
            out.contains("# ?include=parts — no soft-delete exclusion leg"),
            "{out}"
        );
        assert!(
            out.contains("its DELETE hard-deletes"),
            "the omission names why:\n{out}"
        );
        assert!(!out.contains("HTTP 204"), "{out}");
        // The hydration legs themselves are untouched.
        assert!(
            out.contains("jsonpath \"$.included.part\" != null"),
            "{out}"
        );
    }

    /// Unresolved facts (no schema, no ops, no reverse-FK property) omit the
    /// leg silently — never a guessed leg.
    #[test]
    fn soft_delete_leg_is_omitted_when_facts_do_not_resolve() {
        let paths = vec![path(
            "parts",
            vec![segment("part", "widget_id", true, false)],
        )];
        let out = sections_for_paths(
            &paths,
            &IncludeSupport::SeaOrm,
            ITEM,
            CAPTURE,
            LIST,
            "{}",
            true,
            true,
            true,
            &std::collections::BTreeMap::new(),
        )
        .into_iter()
        .map(|s| s.body)
        .collect::<Vec<_>>()
        .join("\n");
        assert!(!out.contains("HTTP 204"), "{out}");
        assert!(
            out.contains("jsonpath \"$.included.part\" != null"),
            "{out}"
        );
    }

    /// The FK injection prepends the reverse-FK member to the target's
    /// minimal body, keeping the two-space contract indentation.
    #[test]
    fn fk_injection_prepends_the_reverse_fk_member() {
        let body = inject_fk_capture(
            "{\n  \"name\": \"contract-part\"\n}",
            "widget_id",
            "{{widget_id}}",
        );
        assert_eq!(
            body,
            "{\n  \"widget_id\": \"{{widget_id}}\",\n  \"name\": \"contract-part\"\n}"
        );
        // An empty required-field body becomes the one-member object.
        let body = inject_fk_capture("{}", "widget_id", "{{widget_id}}");
        assert_eq!(body, "{\n  \"widget_id\": \"{{widget_id}}\"\n}");
    }
}
