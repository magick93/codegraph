//! Fixture/route consistency guards for the entity e2e specs (issues #324,
//! #323).
//!
//! - [`fixture_supplement_for`]: the `testData()` bodies are rendered from
//!   the ui field set, which can miss create-DTO members. The supplement
//!   comes from the DTO's own surface through the IFML seed planner
//!   (`ifml::e2e_test::create_body_supplement`), so the two planes share ONE
//!   derivation of "what a create body needs".
//! - [`UiE2eTestGenerator::list_route_owner_conflict`]: the ux spec drives
//!   the entity's list route; when another entity resolves to the same route,
//!   the spec would assert one entity's columns against another's page.

use std::collections::HashSet;

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use crate::ProjectConfig;
use crate::api::api_model::resolve_path_segment;

use super::context::{EntityRefDep, FixtureSupplementEntry};
use super::generator::UiE2eTestGenerator;
use super::page::UiField;
use super::store::UiParentInfo;

/// The create-body supplement for `schema_title` plus the required fields no
/// plane can seed (`entity: field (reason)` strings). Both are empty when the
/// DTO surface is unavailable or the ui field set already covers it.
pub(super) async fn fixture_supplement_for(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    project: &ProjectConfig,
    schema_title: &str,
    create_fields: &[UiField],
    entity_ref_deps: &[EntityRefDep],
    parent: Option<&UiParentInfo>,
) -> (Vec<FixtureSupplementEntry>, Vec<String>) {
    let mut covered: HashSet<String> = create_fields
        .iter()
        .map(|f| codegraph_naming::to_snake_case(&f.name))
        .chain(
            entity_ref_deps
                .iter()
                .map(|d| codegraph_naming::to_snake_case(&d.field_name)),
        )
        .collect();
    // A nested child's parent link rides the route, never the body.
    if let Some(p) = parent {
        covered.insert(format!("{}_id", p.module_name));
        covered.insert(format!(
            "{}_id",
            codegraph_naming::to_snake_case(&p.entity_name)
        ));
    }
    let Some((entries, gaps)) = crate::ifml::e2e_test::create_body_supplement(
        db,
        config,
        &project.identity.api_version,
        schema_title,
        &covered,
    )
    .await
    else {
        return (Vec::new(), Vec::new());
    };
    (
        entries
            .into_iter()
            .map(|(name, value)| FixtureSupplementEntry { name, value })
            .collect(),
        gaps.iter().map(ToString::to_string).collect(),
    )
}

/// `/domain/parent/{id}/segment`-style rendering of a list route, for
/// diagnostics.
pub(super) fn list_route(
    domain: &str,
    path_segment: &str,
    parent: Option<&UiParentInfo>,
) -> String {
    match parent {
        None => format!("/{domain}/{path_segment}"),
        Some(p) => match &p.grandparent {
            Some(gp) => format!(
                "/{}/{}/{{{}}}/{}/{{{}}}/{path_segment}",
                gp.domain, gp.path_segment, gp.param_name, p.path_segment, p.param_name
            ),
            None => format!(
                "/{}/{}/{{{}}}/{path_segment}",
                p.domain, p.path_segment, p.param_name
            ),
        },
    }
}

impl UiE2eTestGenerator {
    /// Another entity of `domain` whose list route equals this entity's
    /// (same segment under the same parent chain), if any.
    pub(super) async fn list_route_owner_conflict(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        domain: &str,
        schema_title: &str,
        path_segment: &str,
        parent: Option<&UiParentInfo>,
    ) -> Option<String> {
        let mine = list_route(domain, path_segment, parent);
        let schemas = db.list_schemas(Some(domain)).await.ok()?;
        let domain_entry = config.domains.get(domain);
        for other in schemas {
            if other.title == schema_title || !other.is_entity || other.pg_table_name.is_empty() {
                continue;
            }
            let other_cfg = domain_entry.and_then(|d| d.get_entity_config(&other.rust_type_name));
            let other_segment = resolve_path_segment(other_cfg, &other);
            if other_segment != path_segment {
                continue;
            }
            let (other_parent, _) = self
                .resolve_parent_info(db, config, domain, &other.title, other_cfg)
                .await;
            if list_route(domain, &other_segment, other_parent.as_ref()) == mine {
                return Some(other.title);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::store::UiGrandparentInfo;

    fn parent(grandparent: bool) -> UiParentInfo {
        UiParentInfo {
            domain: "common".into(),
            path_segment: "education".into(),
            module_name: "education".into(),
            entity_name: "Education".into(),
            param_name: "education_id".into(),
            grandparent: grandparent.then(|| {
                Box::new(UiGrandparentInfo {
                    domain: "common".into(),
                    path_segment: "persons".into(),
                    entity_name: "Person".into(),
                    param_name: "person_id".into(),
                })
            }),
        }
    }

    #[test]
    fn list_route_renders_every_nesting_depth() {
        assert_eq!(list_route("common", "task", None), "/common/task");
        assert_eq!(
            list_route("common", "attendance", Some(&parent(false))),
            "/common/education/{education_id}/attendance"
        );
        assert_eq!(
            list_route("common", "attendance", Some(&parent(true))),
            "/common/persons/{person_id}/education/{education_id}/attendance"
        );
    }
}
