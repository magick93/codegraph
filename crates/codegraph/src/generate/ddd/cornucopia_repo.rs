use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PersistenceColumnRole;

use crate::error::Result;
use crate::generate::persistence::build_persistence_entity;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use crate::generate::ProjectConfig;
use codegraph_config::DomainConfig;

pub struct CornucopiaRepoGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<codegraph_core::types::ParentCandidate>,
}

impl CornucopiaRepoGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            parent_candidates: Vec::new(),
        }
    }

    pub fn with_parent_candidates(
        mut self,
        candidates: Vec<codegraph_core::types::ParentCandidate>,
    ) -> Self {
        self.parent_candidates = candidates;
        self
    }
}

#[async_trait]
impl EntityGenerator for CornucopiaRepoGenerator {
    fn name(&self) -> &str {
        "cornucopia_repo"
    }

    fn supported_targets(&self) -> Option<Vec<crate::generate::db::dialect::DatabaseTarget>> {
        None
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        _tera: &tera::Tera,
        _project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let entity = build_persistence_entity(
            db,
            schema_title,
            domain,
            config,
            &self.parent_candidates,
        )
        .await?;

        if entity.table_name.is_empty() {
            return Ok(Vec::new());
        }

        let entity_name = codegraph_naming::strip_suffix(
            &entity.rust_type_name,
            &config.defaults.type_suffix,
        );
        let snake_name = codegraph_naming::to_snake_case(&entity_name);
        let module_name = format!("{}_{}", domain, entity.table_name);

        let has_soft_delete = entity.policies.soft_delete.is_some();
        let tenant_col = entity
            .columns
            .iter()
            .find(|c| c.role == PersistenceColumnRole::TenantScope);
        let has_tenant = tenant_col.is_some();

        let data_cols: Vec<_> = entity
            .columns
            .iter()
            .filter(|c| matches!(c.role, PersistenceColumnRole::Data | PersistenceColumnRole::ForeignKey { .. }))
            .collect();

        let mut code = String::with_capacity(4096);

        // ── Header ─────────────────────────────────────────────────────
        code.push_str(&format!(
            "// Generated repository adapter for {} (Cornucopia backend)\n\
             // schema: {}, domain: {}\n\n",
            entity_name, entity.title, domain,
        ));

        code.push_str("use async_trait::async_trait;\n");
        code.push_str("use uuid::Uuid;\n");
        code.push_str(&format!(
            "use super::super::repository::{}Repository;\n",
            entity_name,
        ));
        code.push_str(&format!(
            "use cornucopia_queries::queries::{}::{};\n\n",
            domain, entity.table_name,
        ));

        // ── Repository struct ──────────────────────────────────────────
        code.push_str(&format!(
            "#[derive(Clone)]\n\
             pub struct {0}RepositoryImpl;\n\n",
            entity_name,
        ));

        // ── Trait implementation ───────────────────────────────────────
        code.push_str(&format!(
            "#[async_trait]\n\
             impl {0}Repository for {0}RepositoryImpl {{\n",
            entity_name,
        ));

        // find_by_id
        if has_tenant {
            code.push_str(&format!(
                "    async fn find_by_id(\n\
                 &self,\n\
                 db: &impl cornucopia_queries::client::GenericClient,\n\
                 id: Uuid,\n\
                 ) -> Result<Option<{0}Response>, String> {{\n\
                 let row = get_{1}().bind(db, &id).opt().await\n\
                 .map_err(|e| e.to_string())?;\n\
                 Ok(row.map(|r| r.into()))\n\
                 }}\n\n",
                entity_name, snake_name,
            ));
        } else {
            code.push_str(&format!(
                "    async fn find_by_id(\n\
                 &self,\n\
                 db: &impl cornucopia_queries::client::GenericClient,\n\
                 id: Uuid,\n\
                 ) -> Result<Option<{0}Response>, String> {{\n\
                 let row = get_{1}().bind(db, &id).opt().await\n\
                 .map_err(|e| e.to_string())?;\n\
                 Ok(row.map(|r| r.into()))\n\
                 }}\n\n",
                entity_name, snake_name,
            ));
        }

        // create — placeholder (adapter in user space)
        code.push_str(&format!(
            "    async fn create(\n\
                 &self,\n\
                 client: &impl cornucopia_queries::client::GenericClient,\n\
                 cmd: Create{0}Request,\n\
                 ) -> Result<Uuid, String> {{\n\
                 let row = create_{1}().bind(\n\
                 client,\n\
                 // ... bind fields from cmd ...\n\
                 ).one().await.map_err(|e| e.to_string())?;\n\
                 Ok(row.id)\n\
                 }}\n\n",
            entity_name, snake_name,
        ));

        // update
        code.push_str(&format!(
            "    async fn update(\n\
                 &self,\n\
                 client: &impl cornucopia_queries::client::GenericClient,\n\
                 id: Uuid,\n\
                 cmd: Update{0}Request,\n\
                 ) -> Result<(), String> {{\n\
                 update_{1}().bind(\n\
                 client,\n\
                 &id,\n\
                 // ... bind fields from cmd ...\n\
                 ).await.map_err(|e| e.to_string())?;\n\
                 Ok(())\n\
                 }}\n\n",
            entity_name, snake_name,
        ));

        // delete
        if has_soft_delete {
            code.push_str(&format!(
                "    async fn delete(\n\
                 &self,\n\
                 client: &impl cornucopia_queries::client::GenericClient,\n\
                 id: Uuid,\n\
                 ) -> Result<(), String> {{\n\
                 delete_{0}().bind(client, &id).await.map_err(|e| e.to_string())?;\n\
                 Ok(())\n\
                 }}\n\n",
                snake_name,
            ));
        } else {
            code.push_str(&format!(
                "    async fn delete(\n\
                 &self,\n\
                 client: &impl cornucopia_queries::client::GenericClient,\n\
                 id: Uuid,\n\
                 ) -> Result<(), String> {{\n\
                 delete_{0}().bind(client, &id).await.map_err(|e| e.to_string())?;\n\
                 Ok(())\n\
                 }}\n\n",
                snake_name,
            ));
        }

        // list
        if has_tenant {
            code.push_str(&format!(
                "    async fn list(\n\
                 &self,\n\
                 client: &impl cornucopia_queries::client::GenericClient,\n\
                 tenant_id: Uuid,\n\
                 page: u64,\n\
                 page_size: u64,\n\
                 ) -> Result<(Vec<{0}Response>, u64), String> {{\n\
                 let all = list_{1}().bind(client, &tenant_id).all().await\n\
                 .map_err(|e| e.to_string())?;\n\
                 // Paginate in application layer or add LIMIT/OFFSET params to query\n\
                 let total = all.len() as u64;\n\
                 let paged = all.into_iter()\n\
                 .skip(((page - 1) * page_size) as usize)\n\
                 .take(page_size as usize)\n\
                 .map(|r| r.into())\n\
                 .collect();\n\
                 Ok((paged, total))\n\
                 }}\n\n",
                entity_name,
                snake_name,
            ));
        } else {
            code.push_str(&format!(
                "    async fn list(\n\
                 &self,\n\
                 client: &impl cornucopia_queries::client::GenericClient,\n\
                 page: u64,\n\
                 page_size: u64,\n\
                 ) -> Result<(Vec<{0}Response>, u64), String> {{\n\
                 // For entities without tenant scoping, list without tenant filter\n\
                 // This is a placeholder — actual impl depends on the entity's needs\n\
                 let all = list_{1}().bind(client).all().await\n\
                 .map_err(|e| e.to_string())?;\n\
                 let total = all.len() as u64;\n\
                 let paged = all.into_iter()\n\
                 .skip(((page - 1) * page_size) as usize)\n\
                 .take(page_size as usize)\n\
                 .map(|r| r.into())\n\
                 .collect();\n\
                 Ok((paged, total))\n\
                 }}\n\n",
                entity_name,
                snake_name,
            ));
        }

        code.push_str("}\n");

        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(domain)
                .join(&module_name)
                .join("cornucopia_repository_impl.rs"),
            content: code,
        }])
    }
}
