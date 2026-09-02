use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::{GenerationEntry, ProjectConfig};
use codegraph_config::DomainConfig;

/// Context for the merged XRPC assembly templates.
#[derive(Debug, Serialize)]
pub struct XrpcMergeContext {
    /// Every generated per-domain router module (`{domain}_router`), sorted.
    /// `routes.rs` merges each one's `xrpc_routes()`; `mod.rs` declares them.
    pub router_modules: Vec<String>,
    /// Every generated `get_*` / `create_*` handler module, sorted. `mod.rs`
    /// declares them.
    pub handler_modules: Vec<String>,
}

/// Emits `src/atproto/xrpc/{mod.rs,routes.rs}` — the assembly point that merges
/// every generated per-domain XRPC router (`{domain}_router::xrpc_routes()`) and
/// declares every generated `get_*` / `create_*` handler module.
///
/// These two files were previously preserved hand-written merge points (a
/// `pub use self::routes::xrpc_routes;` marker kept `generate_mod_files` from
/// overwriting `mod.rs`). Hand-written copies drift: the committed `mod.rs` was
/// written before several AT Protocol domains existed, so their routers and
/// handlers were never declared or merged. Generating them from the output tree
/// (after the per-entity/per-domain XRPC generators write their files) keeps the
/// declaration list byte-for-byte in sync with what was actually generated, and
/// the `pub use` marker is reproduced so `generate_mod_files` still skips it.
pub struct AtprotoXrpcMergeEmitter {
    output_dir: PathBuf,
}

impl AtprotoXrpcMergeEmitter {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for AtprotoXrpcMergeEmitter {
    fn name(&self) -> &str {
        "atproto_xrpc_merge"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if project.atproto_authority.is_empty() {
            return Ok(Vec::new());
        }

        let xrpc_dir = self.output_dir.join("src").join("atproto").join("xrpc");
        let mut router_modules: Vec<String> = Vec::new();
        let mut handler_modules: Vec<String> = Vec::new();
        if xrpc_dir.is_dir() {
            let mut names: Vec<String> = std::fs::read_dir(&xrpc_dir)?
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                .filter(|n| n.ends_with(".rs"))
                .filter(|n| n != "mod.rs" && n != "routes.rs")
                .map(|n| n[..n.len() - 3].to_string())
                .collect();
            names.sort();
            for name in names {
                if name.ends_with("_router") {
                    router_modules.push(name);
                } else {
                    handler_modules.push(name);
                }
            }
        }

        // Nothing to assemble (no AT Protocol entities generated) — leave the
        // xrpc directory to the entity/domain generators (it may not even exist).
        if router_modules.is_empty() && handler_modules.is_empty() {
            return Ok(Vec::new());
        }

        let ctx = XrpcMergeContext {
            router_modules,
            handler_modules,
        };

        let mod_content =
            render_template_with_project(tera, "atproto/xrpc_mod.tera", &ctx, project)?;
        let routes_content =
            render_template_with_project(tera, "atproto/xrpc_routes.tera", &ctx, project)?;

        Ok(vec![
            GeneratedFile {
                path: xrpc_dir.join("mod.rs"),
                content: mod_content,
            },
            GeneratedFile {
                path: xrpc_dir.join("routes.rs"),
                content: routes_content,
            },
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The emitter writes the merge point when the xrpc directory already
    /// contains generated per-domain routers + handlers, and the emitted
    /// `mod.rs` carries the `pub use self::routes::xrpc_routes;` marker so
    /// `generate_mod_files` does not overwrite it.
    #[tokio::test]
    async fn emits_merge_point_from_existing_files() {
        let out = std::env::temp_dir().join(format!("xrpc-merge-test-{}", std::process::id()));
        let xrpc_dir = out.join("src").join("atproto").join("xrpc");
        std::fs::create_dir_all(&xrpc_dir).unwrap();
        for name in [
            "advocacy_router.rs",
            "support_router.rs",
            "get_advocacy_advocacy_case.rs",
            "create_support_support_plan.rs",
        ] {
            std::fs::write(xrpc_dir.join(name), "// placeholder\n").unwrap();
        }

        let tera = crate::generate::template_engine::create_tera(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
        )
        .unwrap();
        let project = ProjectConfig {
            atproto_authority: "community.os".to_string(),
            ..Default::default()
        };

        let emitter = AtprotoXrpcMergeEmitter::new(&out);
        let config = codegraph_config::DomainConfig {
            defaults: Default::default(),
            domains: std::collections::HashMap::new(),
        };
        let files = GlobalGenerator::generate(
            &emitter,
            &codegraph_core::mock::MockEngine::builder().build(),
            &config,
            &[],
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

        assert_eq!(files.len(), 2);
        let mod_rs = files
            .iter()
            .find(|f| f.path.ends_with("mod.rs"))
            .expect("mod.rs emitted");
        let routes_rs = files
            .iter()
            .find(|f| f.path.ends_with("routes.rs"))
            .expect("routes.rs emitted");

        assert!(
            mod_rs
                .content
                .contains("pub use self::routes::xrpc_routes;"),
            "mod.rs must keep the pub-use marker for generate_mod_files"
        );
        assert!(mod_rs.content.contains("pub mod advocacy_router;"));
        assert!(mod_rs.content.contains("pub mod support_router;"));
        assert!(mod_rs
            .content
            .contains("pub mod get_advocacy_advocacy_case;"));
        assert!(mod_rs
            .content
            .contains("pub mod create_support_support_plan;"));

        assert!(routes_rs
            .content
            .contains("super::advocacy_router::xrpc_routes()"));
        assert!(routes_rs
            .content
            .contains("super::support_router::xrpc_routes()"));

        std::fs::remove_dir_all(&out).unwrap();
    }

    /// No xrpc files on disk → nothing is emitted (the directory may not even
    /// exist for a project without AT Protocol records).
    #[tokio::test]
    async fn emits_nothing_when_no_xrpc_files() {
        let out = std::env::temp_dir().join(format!("xrpc-merge-empty-{}", std::process::id()));
        std::fs::create_dir_all(&out).unwrap();

        let tera = crate::generate::template_engine::create_tera(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
        )
        .unwrap();
        let project = ProjectConfig {
            atproto_authority: "community.os".to_string(),
            ..Default::default()
        };

        let emitter = AtprotoXrpcMergeEmitter::new(&out);
        let config = codegraph_config::DomainConfig {
            defaults: Default::default(),
            domains: std::collections::HashMap::new(),
        };
        let files = GlobalGenerator::generate(
            &emitter,
            &codegraph_core::mock::MockEngine::builder().build(),
            &config,
            &[],
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

        assert!(files.is_empty());
        std::fs::remove_dir_all(&out).unwrap();
    }
}
