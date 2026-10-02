use std::path::PathBuf;

use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{AtprotoNamespaceNode, LexiconNode};
use tera::Tera;

use super::super::lexicon_gen::LexiconEmitter;
use super::super::scaffold_gen::LexiconScaffoldEmitter;
use super::{make_domain_config, make_primitive_prop, make_project, make_schema};
use crate::ProjectConfig;
use crate::project_config::AtprotoConfig;
use crate::traits::{EntityGenerator, GlobalGenerator};

fn make_tera() -> Tera {
    let mut tera = Tera::default();

    tera.add_raw_template(
            "atproto/lexicon_record.tera",
            r#"{"lexicon":1,"id":"{{lexicon.nsid}}","type":"{{lexicon.lex_type}}","description":"{{lexicon.description}}","defs":{"main":{"type":"record"{% if record.required_fields|length > 0 %},"required":[{% for field in record.required_fields %}"{{field}}"{% if not loop.last %},{% endif %}{% endfor %}]{% endif %},"properties":{ {% for prop in record.properties %}"{{prop.name}}":{"type":{% if prop.type is object %}"ref"{% else %}"{{prop.type.type}}"{% endif %}}{% if not loop.last %},{% endif %}{% endfor %} }}}}"#,
        )
        .unwrap();
    tera.add_raw_template(
            "atproto/lexicon_object.tera",
            r#"{"lexicon":1,"id":"{{lexicon.nsid}}","type":"{{lexicon.lex_type}}","defs":{"main":{"type":"object","properties":{}}}}"#,
        )
        .unwrap();
    tera.add_raw_template(
        "atproto/lexicon_enum.tera",
        r#"{"lexicon":1,"id":"{{lexicon.nsid}}","type":"{{lexicon.lex_type}}"}"#,
    )
    .unwrap();
    tera.add_raw_template(
            "atproto/scaffold.tera",
            r#"{"catalog":{"authority":"{{authority}}","count":{{lexicons|length}},"lexicons":[{% for l in lexicons %}{"id":"{{l.nsid}}","type":"{{l.lex_type}}"}{% if not loop.last %},{% endif %}{% endfor %}]}}"#,
        )
        .unwrap();

    tera
}

#[tokio::test]
async fn test_lexicon_emitter_produces_valid_json() {
    let engine = MockEngine::builder()
        .with_schema(make_schema("Grant", "grants"))
        .with_properties(
            "Grant",
            vec![
                make_primitive_prop("name", "string", true),
                make_primitive_prop("amount", "number", false),
            ],
        )
        .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
        .build();

    let namespace = AtprotoNamespaceNode {
        authority: "nz.gravy".to_string(),
        segment: "".to_string(),
        domain: "grants".to_string(),
    };
    engine.ingest_atproto_namespace(&namespace).await.unwrap();

    let lexicon = LexiconNode {
        nsid: "nz.gravy.grants.grant".to_string(),
        lex_type: "record".to_string(),
        key_strategy: "tid".to_string(),
        revision: Some(1),
        description: Some("A grant record".to_string()),
        domain: "grants".to_string(),
    };
    engine.ingest_lexicon(&lexicon).await.unwrap();

    let tera = make_tera();
    let project = make_project();
    let emitter = LexiconEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Grant",
            "grants",
            &make_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    assert_eq!(result.len(), 1, "should produce one file");

    let file = &result[0];
    assert!(
        file.path.to_string_lossy().contains("nz.gravy"),
        "path should contain authority"
    );
    assert!(
        file.path.to_string_lossy().contains("grants"),
        "path should contain domain"
    );

    let json: serde_json::Value =
        serde_json::from_str(&file.content).expect("output should be valid JSON");

    assert_eq!(json["lexicon"], 1);
    assert_eq!(json["id"], "nz.gravy.grants.grant");
    assert_eq!(json["type"], "record");
    assert_eq!(json["defs"]["main"]["type"], "record");
    assert!(
        json["defs"]["main"]["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::Value::String("name".to_string()))
    );
    assert!(json["defs"]["main"]["properties"]["name"].is_object());
    assert!(json["defs"]["main"]["properties"]["amount"].is_object());
}

#[tokio::test]
async fn test_lexicon_emitter_skips_non_atproto_schema() {
    let engine = MockEngine::builder()
        .with_schema(make_schema("Grant", "grants"))
        .with_properties("Grant", vec![make_primitive_prop("name", "string", true)])
        .build();

    let tera = make_tera();
    let project = make_project();
    let emitter = LexiconEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Grant",
            "grants",
            &make_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    assert!(
        result.is_empty(),
        "should return empty when no lexicon mapping exists"
    );
}

#[tokio::test]
async fn test_scaffold_emitter_produces_catalog() {
    let engine = MockEngine::builder().build();

    let ns = AtprotoNamespaceNode {
        authority: "nz.gravy".to_string(),
        segment: "".to_string(),
        domain: "grants".to_string(),
    };
    engine.ingest_atproto_namespace(&ns).await.unwrap();

    let lex1 = LexiconNode {
        nsid: "nz.gravy.grants.grant".to_string(),
        lex_type: "record".to_string(),
        key_strategy: "tid".to_string(),
        revision: Some(1),
        description: Some("A grant".to_string()),
        domain: "grants".to_string(),
    };
    engine.ingest_lexicon(&lex1).await.unwrap();

    let lex2 = LexiconNode {
        nsid: "nz.gravy.grants.applicant".to_string(),
        lex_type: "object".to_string(),
        key_strategy: "did".to_string(),
        revision: Some(1),
        description: Some("An applicant".to_string()),
        domain: "grants".to_string(),
    };
    engine.ingest_lexicon(&lex2).await.unwrap();

    let tera = make_tera();
    let project = make_project();
    let emitter = LexiconScaffoldEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(&engine, &make_domain_config(), &[], &tera, &project)
        .await
        .expect("scaffold generation should succeed");

    assert_eq!(result.len(), 1);
    let file = &result[0];
    assert!(
        file.path.to_string_lossy().ends_with("_meta.json"),
        "scaffold should produce _meta.json"
    );

    let json: serde_json::Value =
        serde_json::from_str(&file.content).expect("scaffold output should be valid JSON");

    assert_eq!(json["catalog"]["authority"], "nz.gravy");
    assert_eq!(json["catalog"]["count"], 2);
}

#[tokio::test]
async fn test_atproto_authority_empty_skips_scaffold() {
    let engine = MockEngine::builder().build();
    let tera = make_tera();

    let project = ProjectConfig {
        atproto: AtprotoConfig {
            atproto_authority: "".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };
    let emitter = LexiconScaffoldEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(&engine, &make_domain_config(), &[], &tera, &project)
        .await
        .expect("generation should succeed");

    assert!(
        result.is_empty(),
        "should return empty when authority is blank"
    );
}

#[tokio::test]
async fn test_lexicon_context_types() {
    let engine = MockEngine::builder()
        .with_schema(make_schema("Candidate", "recruiting"))
        .with_properties(
            "Candidate",
            vec![
                {
                    let mut p = make_primitive_prop("email", "string", true);
                    p.format = Some("email".to_string());
                    p
                },
                {
                    let mut p = make_primitive_prop("score", "integer", false);
                    p.prop_type = "integer".to_string();
                    p
                },
            ],
        )
        .with_lexicon_mapping("Candidate", "nz.gravy.recruiting.candidate")
        .build();

    let namespace = AtprotoNamespaceNode {
        authority: "nz.gravy".to_string(),
        segment: "".to_string(),
        domain: "recruiting".to_string(),
    };
    engine.ingest_atproto_namespace(&namespace).await.unwrap();

    let lexicon = LexiconNode {
        nsid: "nz.gravy.recruiting.candidate".to_string(),
        lex_type: "record".to_string(),
        key_strategy: "tid".to_string(),
        revision: Some(1),
        description: Some("Candidate record".to_string()),
        domain: "recruiting".to_string(),
    };
    engine.ingest_lexicon(&lexicon).await.unwrap();

    let tera = make_tera();
    let project = make_project();
    let emitter = LexiconEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Candidate",
            "recruiting",
            &make_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    assert_eq!(result.len(), 1);
    let json: serde_json::Value = serde_json::from_str(&result[0].content).expect("valid JSON");

    let props = &json["defs"]["main"]["properties"];
    assert!(props["email"].is_object());
    assert!(props["score"].is_object());
    assert!(
        json["defs"]["main"]["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::Value::String("email".to_string()))
    );
}
