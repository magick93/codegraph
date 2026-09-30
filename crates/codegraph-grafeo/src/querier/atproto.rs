use std::collections::HashMap;

use codegraph_core::error::GraphError;
use codegraph_core::types::{AtprotoNamespaceNode, CollectionNode, LexiconNode, RepositoryNode};

use super::query::{query_many, query_many_params, query_one_params};
use crate::engine::GrafeoEngine;

impl GrafeoEngine {
    // ── AT Protocol query methods ──────────────────────────────────────

    pub(super) async fn query_atproto_namespaces(
        &self,
    ) -> Result<Vec<AtprotoNamespaceNode>, GraphError> {
        query_many(
            self,
            "MATCH (n:AtprotoNamespace) RETURN n.authority, n.segment, n.domain ORDER BY n.authority",
            |reader, row| {
                Ok(AtprotoNamespaceNode {
                    authority: reader.get_string(row, "n.authority")?,
                    segment: reader.get_string(row, "n.segment")?,
                    domain: reader.get_string(row, "n.domain")?,
                })
            },
        )
        .await
    }

    pub(super) async fn query_lexicons(
        &self,
        domain: &str,
    ) -> Result<Vec<LexiconNode>, GraphError> {
        let params = HashMap::from([("domain".to_string(), grafeo::Value::String(domain.into()))]);
        query_many_params(
            self,
            "MATCH (l:Lexicon {domain: $domain}) RETURN l.nsid, l.lex_type, l.key_strategy, \
             l.revision, l.description, l.domain ORDER BY l.nsid",
            params,
            |reader, row| {
                let revision = reader.get_i64(row, "l.revision").ok();
                Ok(LexiconNode {
                    nsid: reader.get_string(row, "l.nsid")?,
                    lex_type: reader.get_string(row, "l.lex_type")?,
                    key_strategy: reader.get_string(row, "l.key_strategy")?,
                    revision,
                    description: reader.get_opt_string(row, "l.description")?,
                    domain: reader.get_string(row, "l.domain")?,
                })
            },
        )
        .await
    }

    pub(super) async fn query_lexicon_by_schema(
        &self,
        schema_title: &str,
    ) -> Result<Option<LexiconNode>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        query_one_params(
            self,
            "MATCH (:Schema {title: $title})-[:ProjectsToLexicon]->(l:Lexicon) \
             RETURN l.nsid, l.lex_type, l.key_strategy, l.revision, l.description, l.domain",
            params,
            |reader, row| {
                let revision = reader.get_i64(row, "l.revision").ok();
                Ok(LexiconNode {
                    nsid: reader.get_string(row, "l.nsid")?,
                    lex_type: reader.get_string(row, "l.lex_type")?,
                    key_strategy: reader.get_string(row, "l.key_strategy")?,
                    revision,
                    description: reader.get_opt_string(row, "l.description")?,
                    domain: reader.get_string(row, "l.domain")?,
                })
            },
        )
        .await
    }

    pub(super) async fn query_collections(
        &self,
        domain: &str,
    ) -> Result<Vec<CollectionNode>, GraphError> {
        let params = HashMap::from([("domain".to_string(), grafeo::Value::String(domain.into()))]);
        query_many_params(
            self,
            "MATCH (c:Collection {domain: $domain}) RETURN c.nsid, c.key_strategy, c.domain ORDER BY c.nsid",
            params,
            |reader, row| {
                Ok(CollectionNode {
                    nsid: reader.get_string(row, "c.nsid")?,
                    key_strategy: reader.get_string(row, "c.key_strategy")?,
                    domain: reader.get_string(row, "c.domain")?,
                })
            },
        )
        .await
    }

    pub(super) async fn query_repositories(&self) -> Result<Vec<RepositoryNode>, GraphError> {
        query_many(
            self,
            "MATCH (r:Repository) RETURN r.did, r.handle, r.pds_endpoint, \
             r.org_name, r.tenancy_mode ORDER BY r.did",
            |reader, row| {
                Ok(RepositoryNode {
                    did: reader.get_string(row, "r.did")?,
                    handle: reader.get_opt_string(row, "r.handle")?,
                    pds_endpoint: reader.get_string(row, "r.pds_endpoint")?,
                    org_name: reader.get_string(row, "r.org_name")?,
                    tenancy_mode: reader.get_string(row, "r.tenancy_mode")?,
                })
            },
        )
        .await
    }

    pub(super) async fn query_lexicon_references(
        &self,
        nsid: &str,
    ) -> Result<Vec<LexiconNode>, GraphError> {
        let params = HashMap::from([("nsid".to_string(), grafeo::Value::String(nsid.into()))]);
        query_many_params(
            self,
            "MATCH (:Lexicon {nsid: $nsid})-[:LexiconReferences]->(l:Lexicon) \
             RETURN l.nsid, l.lex_type, l.key_strategy, l.revision, l.description, l.domain ORDER BY l.nsid",
            params,
            |reader, row| {
                let revision = reader.get_i64(row, "l.revision").ok();
                Ok(LexiconNode {
                    nsid: reader.get_string(row, "l.nsid")?,
                    lex_type: reader.get_string(row, "l.lex_type")?,
                    key_strategy: reader.get_string(row, "l.key_strategy")?,
                    revision,
                    description: reader.get_opt_string(row, "l.description")?,
                    domain: reader.get_string(row, "l.domain")?,
                })
            },
        )
        .await
    }
}
