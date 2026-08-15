//! Template context for `codegraph init` — the single source of truth for the
//! `templates/project/` Tera category.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// One domain entry for the generated `domains.toml` + `schemas/` layout.
#[derive(Debug, Clone, Serialize)]
pub struct DomainSeed {
    /// snake_case name, also used as schema_dir/postgres_schema ("billing").
    pub name: String,
    /// Human label ("Billing").
    pub label: String,
    pub postgres_schema: String,
}

/// Feature flags for the generated `profiles.toml`.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectFeatures {
    pub grpc: bool,
    pub ifml: bool,
    pub ops: bool,
}

/// Everything the project templates render from.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectTemplateContext {
    /// kebab-case project dir name ("demo-app").
    pub project_name: String,
    /// snake_case app name ("demo_app").
    pub app_name: String,
    /// Wrapper binary crate name ("demo-app-graph").
    pub graph_binary: String,
    /// Git rev the workspace Cargo.toml pins codegraph crates to.
    /// Empty when `codegraph_path` is set.
    pub codegraph_rev: String,
    /// Local path to a codegraph checkout; when set, workspace deps use
    /// `{ codegraph_path }/crates/...` path dependencies instead of git+rev.
    /// Rendered verbatim into TOML — canonicalize before building the context.
    pub codegraph_path: Option<String>,
    pub domains: Vec<DomainSeed>,
    pub database_target: String,
    pub persistence_provider: String,
    pub deployment_topology: String,
    pub features: ProjectFeatures,
    pub api_port: u16,
    pub ui_port: u16,
    pub api_title: String,
}

/// Canonical template list: (template path, output path). Output paths may
/// contain `{}` placeholders: `{graph}` = graph_binary, `{domain}` = first
/// domain's name. Templates must live under `crates/codegraph/templates/`.
pub const PROJECT_TEMPLATES: &[(&str, &str)] = &[
    ("project/workspace_cargo.tera", "Cargo.toml"),
    ("project/wrapper_cargo.tera", "{graph}/Cargo.toml"),
    ("project/wrapper_main.tera", "{graph}/src/main.rs"),
    ("project/domains.tera", "domains.toml"),
    ("project/classifier.tera", "classifier.toml"),
    ("project/profiles.tera", "profiles.toml"),
    ("project/extension_points.tera", "extension-points.toml"),
    (
        "project/example_schema.tera",
        "schemas/{domain}/example.json",
    ),
    ("project/ops_manifest.tera", "codegraph-ops.toml"),
    ("project/testkit_cargo.tera", "ops/testkit/Cargo.toml"),
    ("project/testkit_main.tera", "ops/testkit/src/main.rs"),
    ("project/hurl_health.tera", "hurl/health.hurl"),
    ("project/justfile.tera", "justfile"),
    ("project/gitignore.tera", ".gitignore"),
    ("project/readme.tera", "README.md"),
    ("project/ci_yml.tera", ".github/workflows/ci.yml"),
];

impl ProjectTemplateContext {
    /// Build a context from the raw init options.
    pub fn new(
        project_name: &str,
        domains: &[String],
        codegraph_rev: &str,
        codegraph_path: Option<&Path>,
        database_target: &str,
        persistence_provider: &str,
        deployment_topology: &str,
        features: ProjectFeatures,
    ) -> Self {
        let app_name: String = heck::ToSnakeCase::to_snake_case(project_name as &str);
        let graph_binary = format!("{project_name}-graph");
        let domain_seeds: Vec<DomainSeed> = domains
            .iter()
            .map(|name| {
                let norm: String = heck::ToSnakeCase::to_snake_case(name.as_str());
                let label: String = heck::ToTitleCase::to_title_case(name as &str);
                DomainSeed {
                    postgres_schema: norm.clone(),
                    name: norm,
                    label,
                }
            })
            .collect();
        let api_title = format!(
            "{} API",
            heck::ToTitleCase::to_title_case(project_name as &str)
        );
        let codegraph_path = codegraph_path.map(|p| p.to_string_lossy().into_owned());
        Self {
            project_name: project_name.to_string(),
            app_name,
            graph_binary,
            codegraph_rev: codegraph_rev.to_string(),
            codegraph_path,
            domains: domain_seeds,
            database_target: database_target.to_string(),
            persistence_provider: persistence_provider.to_string(),
            deployment_topology: deployment_topology.to_string(),
            features,
            api_port: 3000,
            ui_port: 5173,
            api_title,
        }
    }

    /// Render all project templates; returns (output-relative path, content)
    /// pairs in deterministic template order.
    pub fn render(&self, tera: &tera::Tera) -> Result<Vec<(PathBuf, String)>, String> {
        let ctx = tera::Context::from_serialize(self)
            .map_err(|e| format!("serialize project context: {e}"))?;
        let first_domain = self
            .domains
            .first()
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "common".to_string());
        let mut out = Vec::with_capacity(PROJECT_TEMPLATES.len());
        for (template, output) in PROJECT_TEMPLATES {
            let rendered = tera
                .render(template, &ctx)
                .map_err(|e| format!("render {template}: {e}"))?;
            let path = output
                .replace("{graph}", &self.graph_binary)
                .replace("{domain}", &first_domain);
            out.push((PathBuf::from(path), rendered));
        }
        Ok(out)
    }

    /// Expected output file tree (for tests and dry-run output).
    pub fn file_tree(&self) -> Vec<PathBuf> {
        let first_domain = self
            .domains
            .first()
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "common".to_string());
        PROJECT_TEMPLATES
            .iter()
            .map(|(_, output)| {
                PathBuf::from(
                    output
                        .replace("{graph}", &self.graph_binary)
                        .replace("{domain}", &first_domain),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_names_from_project_name() {
        let ctx = ProjectTemplateContext::new(
            "demo-app",
            &["billing".to_string(), "common".to_string()],
            "",
            None,
            "postgres",
            "sea_orm",
            "monolith",
            ProjectFeatures {
                grpc: false,
                ifml: false,
                ops: true,
            },
        );
        assert_eq!(ctx.app_name, "demo_app");
        assert_eq!(ctx.graph_binary, "demo-app-graph");
        assert_eq!(ctx.domains[0].label, "Billing");
        assert_eq!(ctx.domains[1].name, "common");
        assert_eq!(ctx.api_title, "Demo App API");
    }

    #[test]
    fn file_tree_uses_graph_and_first_domain_placeholders() {
        let ctx = ProjectTemplateContext::new(
            "demo-app",
            &["billing".to_string()],
            "",
            None,
            "postgres",
            "sea_orm",
            "monolith",
            ProjectFeatures {
                grpc: true,
                ifml: true,
                ops: true,
            },
        );
        let tree = ctx.file_tree();
        assert!(tree.contains(&PathBuf::from("demo-app-graph/src/main.rs")));
        assert!(tree.contains(&PathBuf::from("schemas/billing/example.json")));
        assert!(tree.contains(&PathBuf::from("codegraph-ops.toml")));
        assert_eq!(tree.len(), PROJECT_TEMPLATES.len());
    }

    #[test]
    fn render_fails_cleanly_on_missing_template() {
        let ctx = ProjectTemplateContext::new(
            "demo-app",
            &["billing".to_string()],
            "",
            None,
            "postgres",
            "sea_orm",
            "monolith",
            ProjectFeatures {
                grpc: false,
                ifml: false,
                ops: true,
            },
        );
        let tera = tera::Tera::default();
        let err = ctx.render(&tera).unwrap_err();
        assert!(err.contains("project/workspace_cargo.tera"), "{err}");
    }
}
