//! The `?include_deleted=` contract leg (issue #792).
//!
//! For auditable entities with create+delete+read, the entity contract gains
//! one deterministic leg proving the soft-delete read toggle end to end:
//! POST a SECOND row (the source's deferred delete stays untouched — the
//! #784 soft-delete-leg precedent), DELETE it (204 — soft delete on both
//! providers since #791/#788), GET it with `?include_deleted=true` asserting
//! the row is served, then GET it WITHOUT the param asserting 404 (the
//! default read hides the row). The leg runs under the contract's `api_key`
//! — an API-key session, which bypasses the `manager`-or-better role gate
//! (the `permission_middleware` precedent; scopes enforce at the DB tier) —
//! so the leg is deterministic for exactly the surface the handler serves.
//!
//! Omission discipline (`#662 may_refuse`): an auditable entity missing one
//! of the gates emits its omission comment naming the gap; a non-auditable
//! entity emits NOTHING (the toggle is not served — no leg could prove
//! anything).

pub(super) enum ToggleLegFacts {
    /// The leg is provable: the entity is auditable (its own read-plane
    /// decision) and carries create + delete + read ops.
    Provable,
    /// Auditable but missing gates — the omission comment names them.
    Omitted(Vec<String>),
    /// Not auditable: the toggle is not served; no section at all.
    NotAuditable,
}

/// Render the toggle-leg sections for resolved facts. One provable leg; one
/// omission comment per missing-gate shape; silence for non-auditable.
pub(super) fn sections_for_facts(
    facts: &ToggleLegFacts,
    entity_url: &str,
    module: &str,
    create_body: &str,
) -> Vec<super::ContractSection> {
    let capture = format!("{{{{{module}_deleted_id}}}}");
    match facts {
        ToggleLegFacts::NotAuditable => Vec::new(),
        ToggleLegFacts::Omitted(missing) => {
            let named = match missing.as_slice() {
                [one] => format!("no `{one}` operation"),
                [a, b] => format!("no `{a}`, no `{b}` operation"),
                more => {
                    let list = more
                        .iter()
                        .map(|op| format!("`{op}`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("none of the operations {list}")
                }
            };
            vec![super::ContractSection {
                body: format!(
                    "# ?include_deleted=true — no toggle leg: the entity is auditable but\n\
                     # carries {named}, so the contract cannot create and\n\
                     # soft-delete a second row to prove the toggle."
                ),
            }]
        }
        ToggleLegFacts::Provable => vec![super::ContractSection {
            body: format!(
                "\
# ?include_deleted=true — the soft-delete read toggle (issue #792). The role
# gate: JWT callers need a `manager`-or-better role; API-key sessions pass
# (their scopes enforce at the DB tier).
POST {entity_url}
Authorization: Bearer {{{{api_key}}}}
{create_body}

HTTP 201
[Captures]
{module}_deleted_id: jsonpath \"$.data.id\"

DELETE {entity_url}/{capture}
Authorization: Bearer {{{{api_key}}}}

HTTP 204

GET {entity_url}/{capture}?include_deleted=true
Authorization: Bearer {{{{api_key}}}}

HTTP 200
[Asserts]
jsonpath \"$.data.id\" == \"{capture}\"

GET {entity_url}/{capture}
Authorization: Bearer {{{{api_key}}}}

HTTP 404"
            ),
        }],
    }
}

/// Resolve the toggle-leg facts for one entity and render its sections.
/// The auditable decision is the entity's OWN read-plane resolution
/// (`resolve_is_auditable` — the same decision that gates the query layer's
/// `include_deleted` parameter and the handler's forwarding), so the
/// contract cannot pin a leg the emitted code would answer differently.
// The arg list mirrors `include::include_sections` (the sibling plane's
// identical shape, same allow).
#[allow(clippy::too_many_arguments)]
pub(super) async fn include_deleted_sections(
    db: &dyn codegraph_core::traits::GraphQuerier,
    config: &codegraph_config::DomainConfig,
    domain: &str,
    schema: &codegraph_core::types::SchemaNode,
    entity_url: &str,
    module: &str,
    create_body: &str,
    has_create: bool,
    has_read: bool,
    has_delete: bool,
) -> crate::error::Result<Vec<super::ContractSection>> {
    let auditable =
        crate::api::handler::resolve_is_auditable(db, &schema.title, config, domain).await?;
    let facts = if !auditable {
        ToggleLegFacts::NotAuditable
    } else {
        let mut missing = Vec::new();
        if !has_create {
            missing.push("create".to_string());
        }
        if !has_read {
            missing.push("read".to_string());
        }
        if !has_delete {
            missing.push("delete".to_string());
        }
        if missing.is_empty() {
            ToggleLegFacts::Provable
        } else {
            ToggleLegFacts::Omitted(missing)
        }
    };
    Ok(sections_for_facts(&facts, entity_url, module, create_body))
}

#[cfg(test)]
mod tests {
    use super::{ToggleLegFacts, include_deleted_sections, sections_for_facts};
    use crate::api::hurl_contract::ContractSection;

    const ENTITY_URL: &str = "{{base_url}}/api/v1/inventory/widget";
    const MODULE: &str = "widget";
    const BODY: &str = "{\n  \"name\": \"contract-widget\"\n}";

    fn render(sections: Vec<ContractSection>) -> String {
        sections
            .into_iter()
            .map(|s| s.body)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The full leg, byte-exact: second-row POST (the contract's own body, FK
    /// captures included) → soft-delete 204 → the toggle GET serves the row →
    /// the default GET 404s.
    #[test]
    fn provable_facts_emit_the_deterministic_toggle_leg() {
        let out = render(sections_for_facts(
            &ToggleLegFacts::Provable,
            ENTITY_URL,
            MODULE,
            BODY,
        ));

        let expected = "\
# ?include_deleted=true — the soft-delete read toggle (issue #792). The role
# gate: JWT callers need a `manager`-or-better role; API-key sessions pass
# (their scopes enforce at the DB tier).
POST {{base_url}}/api/v1/inventory/widget
Authorization: Bearer {{api_key}}
{
  \"name\": \"contract-widget\"
}

HTTP 201
[Captures]
widget_deleted_id: jsonpath \"$.data.id\"

DELETE {{base_url}}/api/v1/inventory/widget/{{widget_deleted_id}}
Authorization: Bearer {{api_key}}

HTTP 204

GET {{base_url}}/api/v1/inventory/widget/{{widget_deleted_id}}?include_deleted=true
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath \"$.data.id\" == \"{{widget_deleted_id}}\"

GET {{base_url}}/api/v1/inventory/widget/{{widget_deleted_id}}
Authorization: Bearer {{api_key}}

HTTP 404";
        assert_eq!(out, expected);
    }

    /// An auditable entity missing a gate emits ONLY its omission comment,
    /// naming the gap (the `may_refuse` discipline).
    #[test]
    fn missing_gates_emit_the_omission_comment() {
        let out = render(sections_for_facts(
            &ToggleLegFacts::Omitted(vec!["delete".to_string()]),
            ENTITY_URL,
            MODULE,
            BODY,
        ));
        assert_eq!(
            out,
            "# ?include_deleted=true — no toggle leg: the entity is auditable but\n\
             # carries no `delete` operation, so the contract cannot create and\n\
             # soft-delete a second row to prove the toggle."
        );

        let out = render(sections_for_facts(
            &ToggleLegFacts::Omitted(vec!["create".to_string(), "delete".to_string()]),
            ENTITY_URL,
            MODULE,
            BODY,
        ));
        assert!(
            out.contains("no `create`, no `delete` operation"),
            "every missing gate is named:\n{out}"
        );
    }

    /// A non-auditable entity emits NOTHING — the toggle is not served, so
    /// no leg (and no comment) belongs in its contract.
    #[test]
    fn non_auditable_facts_emit_nothing() {
        let sections = sections_for_facts(&ToggleLegFacts::NotAuditable, ENTITY_URL, MODULE, BODY);
        assert!(sections.is_empty());
    }

    /// The db-backed facts half over the production derivation: the entity's
    /// own `resolve_is_auditable` decision plus the resolved operation set.
    #[tokio::test]
    async fn facts_follow_the_entity_auditable_decision_and_ops() {
        use codegraph_config::config::parse_domain_config_str;
        use codegraph_core::mock::MockEngine;
        use codegraph_core::types::SchemaNode;

        fn widget_schema() -> SchemaNode {
            SchemaNode {
                namespace: None,
                schema_id: "inventory/json/WidgetType.json".to_string(),
                title: "WidgetType".to_string(),
                description: None,
                schema_type: "object".to_string(),
                classification: "entity_reference".to_string(),
                domain: Some("inventory".to_string()),
                rel_path: "inventory/json/WidgetType.json".to_string(),
                pg_type: "UUID".to_string(),
                rust_type: "Widget".to_string(),
                sea_orm_type: "Uuid".to_string(),
                rust_type_name: "Widget".to_string(),
                pg_table_name: "widget".to_string(),
                api_path_segment: "widget".to_string(),
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

        let auditable = parse_domain_config_str(
            "[defaults]\noperations = [\"create\", \"read\", \"update\", \"delete\", \"list\"]\n\n[domains.inventory]\nlabel = \"Inventory\"\nschema_dir = \"inventory\"\npostgres_schema = \"inventory\"\nentities = [\"WidgetType\"]\n",
        )
        .unwrap();
        let non_auditable = parse_domain_config_str(
            "[defaults]\noperations = [\"create\", \"read\", \"update\", \"delete\", \"list\"]\n\n[domains.inventory]\nlabel = \"Inventory\"\nschema_dir = \"inventory\"\npostgres_schema = \"inventory\"\nauditable = false\nentities = [\"WidgetType\"]\n",
        )
        .unwrap();
        let no_delete = parse_domain_config_str(
            "[defaults]\noperations = [\"create\", \"read\", \"update\", \"list\"]\n\n[domains.inventory]\nlabel = \"Inventory\"\nschema_dir = \"inventory\"\npostgres_schema = \"inventory\"\nentities = [\"WidgetType\"]\n",
        )
        .unwrap();

        let schema = widget_schema();
        let db = MockEngine::builder().with_schema(schema.clone()).build();

        // Auditable + create/read/delete: the leg is provable.
        let sections = include_deleted_sections(
            &db,
            &auditable,
            "inventory",
            &schema,
            ENTITY_URL,
            MODULE,
            BODY,
            true,
            true,
            true,
        )
        .await
        .unwrap();
        assert_eq!(sections.len(), 1);
        assert!(
            sections[0].body.contains("?include_deleted=true"),
            ":\n{}",
            sections[0].body
        );

        // Non-auditable: silent (the toggle is not served).
        let sections = include_deleted_sections(
            &db,
            &non_auditable,
            "inventory",
            &schema,
            ENTITY_URL,
            MODULE,
            BODY,
            true,
            true,
            true,
        )
        .await
        .unwrap();
        assert!(sections.is_empty());

        // Auditable but no delete op: the omission comment names the gap.
        let sections = include_deleted_sections(
            &db,
            &no_delete,
            "inventory",
            &schema,
            ENTITY_URL,
            MODULE,
            BODY,
            true,
            true,
            false,
        )
        .await
        .unwrap();
        assert_eq!(sections.len(), 1);
        assert!(
            sections[0].body.contains("no `delete` operation"),
            ":\n{}",
            sections[0].body
        );
    }
}
