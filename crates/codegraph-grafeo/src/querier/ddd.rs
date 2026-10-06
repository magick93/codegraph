use std::collections::{HashMap, HashSet};

use codegraph_core::error::GraphError;
use codegraph_core::types::{
    DddApplicationNode, DddDesignNode, DddModelGraph, DddModuleNode, DddRepositoryNode,
    DddRepositoryOperation, DddSearchNode, DddServiceNode,
};

use super::query::query_gql;
use crate::conversions::RowReader;
use crate::engine::GrafeoEngine;

fn parse_json<T: serde::de::DeserializeOwned + Default>(raw: Option<String>) -> T {
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

impl GrafeoEngine {
    pub(super) async fn query_ddd_models(&self) -> Result<Vec<DddModelGraph>, GraphError> {
        // Applications: one DddModelGraph each, sorted by name.
        let result = query_gql(
            self,
            "MATCH (a:DddApplication) \
             RETURN a.name AS name, a.base AS base, a.source_path AS source_path",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut apps: Vec<(String, DddApplicationNode)> = result
            .rows
            .iter()
            .map(|row| {
                let name = reader.get_string(row, "name")?;
                let base = reader.get_opt_string(row, "base")?;
                let source_path = reader.get_string(row, "source_path")?;
                Ok((
                    name.clone(),
                    DddApplicationNode {
                        name,
                        base,
                        source_path,
                    },
                ))
            })
            .collect::<Result<Vec<_>, GraphError>>()?;
        apps.sort_by(|a, b| a.0.cmp(&b.0));
        if apps.is_empty() {
            return Ok(Vec::new());
        }

        // Modules per application, via DddHasModule.
        let result = query_gql(
            self,
            "MATCH (a:DddApplication)-[:DddHasModule]->(m:DddModule) \
             RETURN a.name AS app, m.name AS name, m.ordinal AS ordinal",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut modules: HashMap<String, Vec<DddModuleNode>> = HashMap::new();
        // (application, module name) → module ordinal: designs/services/
        // searches restore in module-major declaration order.
        let mut module_ordinals: HashMap<(String, String), usize> = HashMap::new();
        for row in &result.rows {
            let app = reader.get_string(row, "app")?;
            let name = reader.get_string(row, "name")?;
            let ordinal = reader.get_usize(row, "ordinal")?;
            module_ordinals.insert((app.clone(), name.clone()), ordinal);
            modules.entry(app.clone()).or_default().push(DddModuleNode {
                application: app,
                name,
                ordinal,
            });
        }

        // Designs per application, via DddHasModule → DddHasDesign.
        let result = query_gql(
            self,
            "MATCH (a:DddApplication)-[:DddHasModule]->(m:DddModule)-[:DddHasDesign]->(d:DddDesign) \
             RETURN a.name AS app, d.module AS module, d.class AS class, \
             d.resolved_title AS resolved_title, d.stereotype AS stereotype, \
             d.is_abstract AS is_abstract, d.flags_json AS flags_json, d.ordinal AS ordinal",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut designs: HashMap<String, Vec<DddDesignNode>> = HashMap::new();
        for row in &result.rows {
            let app = reader.get_string(row, "app")?;
            designs.entry(app.clone()).or_default().push(DddDesignNode {
                application: app,
                module: reader.get_string(row, "module")?,
                class: reader.get_string(row, "class")?,
                resolved_title: reader.get_opt_string(row, "resolved_title")?,
                stereotype: reader.get_string(row, "stereotype")?,
                is_abstract: reader.get_bool(row, "is_abstract")?,
                flags: parse_json(reader.get_opt_string(row, "flags_json")?),
                ordinal: reader.get_usize(row, "ordinal")?,
            });
        }

        // Services per application, via DddHasModule → DddHasService.
        let result = query_gql(
            self,
            "MATCH (a:DddApplication)-[:DddHasModule]->(m:DddModule)-[:DddHasService]->(s:DddService) \
             RETURN a.name AS app, s.module AS module, s.name AS name, \
             s.description AS description, s.dependencies_json AS dependencies_json, \
             s.operations_json AS operations_json, s.ordinal AS ordinal",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut services: HashMap<String, Vec<DddServiceNode>> = HashMap::new();
        for row in &result.rows {
            let app = reader.get_string(row, "app")?;
            services
                .entry(app.clone())
                .or_default()
                .push(DddServiceNode {
                    application: app,
                    module: reader.get_string(row, "module")?,
                    name: reader.get_string(row, "name")?,
                    description: reader.get_opt_string(row, "description")?,
                    dependencies: parse_json(reader.get_opt_string(row, "dependencies_json")?),
                    operations: parse_json(reader.get_opt_string(row, "operations_json")?),
                    ordinal: reader.get_usize(row, "ordinal")?,
                });
        }

        // Searches per application, via DddHasModule → DddHasSearch.
        let result = query_gql(
            self,
            "MATCH (a:DddApplication)-[:DddHasModule]->(m:DddModule)-[:DddHasSearch]->(x:DddSearch) \
             RETURN a.name AS app, x.module AS module, x.name AS name, \
             x.description AS description, x.entity_class AS entity_class, \
             x.entity_title AS entity_title, x.text_json AS text_json, \
             x.filters_json AS filters_json, x.sorts_json AS sorts_json, \
             x.document_json AS document_json, x.ranking AS ranking, x.analyzer AS analyzer, \
             x.pagination_json AS pagination_json, x.capabilities_json AS capabilities_json, \
             x.ordinal AS ordinal",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut searches: HashMap<String, Vec<DddSearchNode>> = HashMap::new();
        for row in &result.rows {
            let app = reader.get_string(row, "app")?;
            searches
                .entry(app.clone())
                .or_default()
                .push(DddSearchNode {
                    application: app,
                    module: reader.get_string(row, "module")?,
                    name: reader.get_string(row, "name")?,
                    description: reader.get_opt_string(row, "description")?,
                    entity_class: reader.get_string(row, "entity_class")?,
                    entity_title: reader.get_opt_string(row, "entity_title")?,
                    text: parse_json(reader.get_opt_string(row, "text_json")?),
                    filters: parse_json(reader.get_opt_string(row, "filters_json")?),
                    sorts: parse_json(reader.get_opt_string(row, "sorts_json")?),
                    document: parse_json(reader.get_opt_string(row, "document_json")?),
                    ranking: reader.get_opt_string(row, "ranking")?,
                    analyzer: reader.get_opt_string(row, "analyzer")?,
                    pagination: parse_json(reader.get_opt_string(row, "pagination_json")?),
                    capabilities: parse_json(reader.get_opt_string(row, "capabilities_json")?),
                    ordinal: reader.get_usize(row, "ordinal")?,
                });
        }

        // Repositories per application, via DddHasRepository from their
        // designing design. Deduplicated by (application, name): a class
        // designed in two modules fans the edge out.
        let result = query_gql(
            self,
            "MATCH (d:DddDesign)-[:DddHasRepository]->(r:DddRepository) \
             RETURN r.application AS app, r.name AS name, r.design_class AS design_class",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut repositories: HashMap<String, Vec<DddRepositoryNode>> = HashMap::new();
        let mut seen_repos: HashSet<(String, String)> = HashSet::new();
        for row in &result.rows {
            let app = reader.get_string(row, "app")?;
            let name = reader.get_string(row, "name")?;
            if !seen_repos.insert((app.clone(), name.clone())) {
                continue;
            }
            repositories
                .entry(app.clone())
                .or_default()
                .push(DddRepositoryNode {
                    application: app,
                    name,
                    design_class: reader.get_string(row, "design_class")?,
                    operations: Vec::new(),
                });
        }

        // Repository operations per repository, via DddHasOperation.
        let result = query_gql(
            self,
            "MATCH (r:DddRepository)-[:DddHasOperation]->(o:DddRepositoryOperation) \
             RETURN r.application AS app, r.name AS repository, o.name AS name, \
             o.builtin AS builtin, o.return_type_json AS return_type_json, \
             o.return_multiplicity AS return_multiplicity, o.params_json AS params_json, \
             o.ordinal AS ordinal",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut operations: HashMap<(String, String), Vec<DddRepositoryOperation>> = HashMap::new();
        let mut seen_ops: HashSet<(String, String, String, usize)> = HashSet::new();
        for row in &result.rows {
            let app = reader.get_string(row, "app")?;
            let repository = reader.get_string(row, "repository")?;
            let name = reader.get_string(row, "name")?;
            let ordinal = reader.get_usize(row, "ordinal")?;
            if !seen_ops.insert((app.clone(), repository.clone(), name.clone(), ordinal)) {
                continue;
            }
            let return_type_json = reader.get_opt_string(row, "return_type_json")?;
            let return_multiplicity_json = reader.get_opt_string(row, "return_multiplicity")?;
            operations
                .entry((app, repository))
                .or_default()
                .push(DddRepositoryOperation {
                    name,
                    builtin: reader.get_opt_string(row, "builtin")?,
                    return_type: return_type_json.and_then(|s| serde_json::from_str(&s).ok()),
                    return_multiplicity: return_multiplicity_json
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    params: parse_json(reader.get_opt_string(row, "params_json")?),
                    ordinal,
                });
        }

        let models = apps
            .into_iter()
            .map(|(app_name, application)| {
                // Declaration order is module-major: designs/services/
                // searches sort by (module ordinal, own ordinal); modules
                // missing from the map (defensive) sink to the end.
                let module_rank = |name: &str| {
                    module_ordinals
                        .get(&(app_name.clone(), name.to_string()))
                        .copied()
                        .unwrap_or(usize::MAX)
                };
                let mut modules = modules.remove(&app_name).unwrap_or_default();
                modules.sort_by_key(|m| m.ordinal);
                let mut designs = designs.remove(&app_name).unwrap_or_default();
                designs.sort_by_key(|d| (module_rank(&d.module), d.ordinal));
                let mut services = services.remove(&app_name).unwrap_or_default();
                services.sort_by_key(|s| (module_rank(&s.module), s.ordinal));
                let mut searches = searches.remove(&app_name).unwrap_or_default();
                searches.sort_by_key(|x| (module_rank(&x.module), x.ordinal));
                let mut repositories = repositories.remove(&app_name).unwrap_or_default();
                repositories.sort_by(|a, b| a.name.cmp(&b.name));
                for repo in &mut repositories {
                    let mut ops = operations
                        .remove(&(repo.application.clone(), repo.name.clone()))
                        .unwrap_or_default();
                    ops.sort_by_key(|o| o.ordinal);
                    repo.operations = ops;
                }
                DddModelGraph {
                    source_path: application.source_path.clone(),
                    application,
                    modules,
                    designs,
                    repositories,
                    services,
                    searches,
                }
            })
            .collect();

        Ok(models)
    }
}
