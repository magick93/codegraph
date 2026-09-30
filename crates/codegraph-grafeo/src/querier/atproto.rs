use std::collections::HashMap;

use codegraph_core::error::GraphError;
use codegraph_core::types::{AtprotoNamespaceNode, CollectionNode, LexiconNode, RepositoryNode};

use super::{query_gql, query_gql_params};
use crate::conversions::RowReader;
use crate::engine::GrafeoEngine;

impl GrafeoEngine {
    // ── AT Protocol query methods ──────────────────────────────────────

    pub(super) async fn query_atproto_namespaces(&self) -> Result<Vec<AtprotoNamespaceNode>, GraphError> {
        let result = query_gql(
            self,
            "MATCH (n:AtprotoNamespace) RETURN n.authority, n.segment, n.domain ORDER BY n.authority",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(AtprotoNamespaceNode {
                authority: reader.get_string(row, "n.authority")?,
                segment: reader.get_string(row, "n.segment")?,
                domain: reader.get_string(row, "n.domain")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_lexicons(&self, domain: &str) -> Result<Vec<LexiconNode>, GraphError> {
        let params = HashMap::from([("domain".to_string(), grafeo::Value::String(domain.into()))]);
        let result = query_gql_params(
            self,
            "MATCH (l:Lexicon {domain: $domain}) RETURN l.nsid, l.lex_type, l.key_strategy, \
             l.revision, l.description, l.domain ORDER BY l.nsid",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            let revision = reader.get_i64(row, "l.revision").ok();
            nodes.push(LexiconNode {
                nsid: reader.get_string(row, "l.nsid")?,
                lex_type: reader.get_string(row, "l.lex_type")?,
                key_strategy: reader.get_string(row, "l.key_strategy")?,
                revision,
                description: reader.get_opt_string(row, "l.description")?,
                domain: reader.get_string(row, "l.domain")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_lexicon_by_schema(
        &self,
        schema_title: &str,
    ) -> Result<Option<LexiconNode>, GraphError> {
        let escaped = schema_title.replace('\'', "\\'");
        let gql = format!(
            "MATCH (:Schema {{title: '{escaped}'}})-[:ProjectsToLexicon]->(l:Lexicon) \
             RETURN l.nsid, l.lex_type, l.key_strategy, l.revision, l.description, l.domain"
        );
        let result = query_gql(self, &gql)?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        let revision = reader.get_i64(&result.rows[0], "l.revision").ok();
        Ok(Some(LexiconNode {
            nsid: reader.get_string(&result.rows[0], "l.nsid")?,
            lex_type: reader.get_string(&result.rows[0], "l.lex_type")?,
            key_strategy: reader.get_string(&result.rows[0], "l.key_strategy")?,
            revision,
            description: reader.get_opt_string(&result.rows[0], "l.description")?,
            domain: reader.get_string(&result.rows[0], "l.domain")?,
        }))
    }

    pub(super) async fn query_collections(&self, domain: &str) -> Result<Vec<CollectionNode>, GraphError> {
        let params = HashMap::from([("domain".to_string(), grafeo::Value::String(domain.into()))]);
        let result = query_gql_params(
            self,
            "MATCH (c:Collection {domain: $domain}) RETURN c.nsid, c.key_strategy, c.domain ORDER BY c.nsid",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(CollectionNode {
                nsid: reader.get_string(row, "c.nsid")?,
                key_strategy: reader.get_string(row, "c.key_strategy")?,
                domain: reader.get_string(row, "c.domain")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_repositories(&self) -> Result<Vec<RepositoryNode>, GraphError> {
        let result = query_gql(
            self,
            "MATCH (r:Repository) RETURN r.did, r.handle, r.pds_endpoint, \
             r.org_name, r.tenancy_mode ORDER BY r.did",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(RepositoryNode {
                did: reader.get_string(row, "r.did")?,
                handle: reader.get_opt_string(row, "r.handle")?,
                pds_endpoint: reader.get_string(row, "r.pds_endpoint")?,
                org_name: reader.get_string(row, "r.org_name")?,
                tenancy_mode: reader.get_string(row, "r.tenancy_mode")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_lexicon_references(&self, nsid: &str) -> Result<Vec<LexiconNode>, GraphError> {
        let escaped = nsid.replace('\'', "\\'");
        let gql = format!(
            "MATCH (:Lexicon {{nsid: '{escaped}'}})-[:LexiconReferences]->(l:Lexicon) \
             RETURN l.nsid, l.lex_type, l.key_strategy, l.revision, l.description, l.domain ORDER BY l.nsid"
        );
        let result = query_gql(self, &gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            let revision = reader.get_i64(row, "l.revision").ok();
            nodes.push(LexiconNode {
                nsid: reader.get_string(row, "l.nsid")?,
                lex_type: reader.get_string(row, "l.lex_type")?,
                key_strategy: reader.get_string(row, "l.key_strategy")?,
                revision,
                description: reader.get_opt_string(row, "l.description")?,
                domain: reader.get_string(row, "l.domain")?,
            });
        }
        Ok(nodes)
    }
}
