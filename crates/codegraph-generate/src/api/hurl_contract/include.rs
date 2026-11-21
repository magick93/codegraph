//! Behavioural `?include=` contract legs (issue #781 P2).
//!
//! The generated hurl contracts never exercised `?include=` before this
//! module — which is exactly why the cornucopia defect (#781) stayed
//! invisible for months. Every entity whose handler hydrates includes now
//! ships deterministic include legs, resolved through the SAME gate the
//! handler uses ([`crate::api::include_path::resolve_include_paths_for_entity`])
//! so the contract cannot assert a surface the handler does not serve.
//!
//! ## Provider gate (issue #781 P0)
//!
//! A cornucopia project answers a guarded 501 for an include request, so it
//! gets NO legs — just one comment line naming #781. The gate reads
//! [`crate::api::include_plan::include_support_for`], the SAME predicate the
//! handler's hydration plan reads: when the P1 implementation lands, flipping
//! that one predicate flips the contract too.
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

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::SchemaNode;

use super::ContractSection;
use crate::ProjectConfig;
use crate::api::include_path::{ResolvedIncludePath, resolve_include_paths_for_entity};
use crate::api::include_plan::{IncludeSupport, include_support_for};
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
    ))
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
) -> Vec<ContractSection> {
    if paths.is_empty() {
        return Vec::new();
    }
    if !support.is_supported() {
        return vec![section(UNSUPPORTED_NOTE.to_string())];
    }

    let mut sections = Vec::new();
    for path in paths {
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

    /// Issue #781 P0: a non-hydrating provider gets ONE comment line naming
    /// the issue and NO legs — a request would answer a guarded 501.
    #[test]
    fn cornucopia_emits_the_781_comment_and_no_legs() {
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
        assert!(!support.is_supported(), "cornucopia must be unsupported");

        let out = sections_for_paths(
            &paths, &support, ITEM, CAPTURE, LIST, "{}", true, true, true,
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
                true
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
        );
        assert_eq!(sections.len(), 1);
        assert!(
            !sections[0].body.contains("{{widget_id}}"),
            "no item leg without a create capture:\n{}",
            sections[0].body
        );
        assert!(sections[0].body.contains("page_size=10&include=parts"));
    }
}
