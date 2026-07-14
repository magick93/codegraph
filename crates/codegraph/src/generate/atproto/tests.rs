#[cfg(test)]
mod atproto_template_tests {
    use tera::Tera;
    use std::path::Path;

    fn load_tera() -> Tera {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let glob = base.join("**/*.tera").to_string_lossy().to_string();
        Tera::new(&glob).expect("Tera should load all templates")
    }

    #[test]
    fn atproto_templates_parse_and_load() {
        let tera = load_tera();
        let atproto: Vec<_> = tera
            .get_template_names()
            .filter(|n| n.contains("atproto"))
            .collect();
        assert!(!atproto.is_empty(), "Expected atproto templates to be loaded");
        eprintln!("Loaded AT Proto templates: {:?}", atproto);
    }

    #[test]
    fn lexicon_record_template_renders_basic_record() {
        let mut tera = load_tera();
        let mut ctx = tera::Context::new();

        ctx.insert("lexicon", &serde_json::json!({
            "nsid": "app.test.post",
            "lex_type": "record",
            "key_strategy": "Tid",
            "revision": 1,
            "description": "A test record.",
            "domain": "test"
        }));
        ctx.insert("namespace", &serde_json::json!({
            "authority": "app.test"
        }));
        ctx.insert("record", &serde_json::json!({
            "name": "Post",
            "description": "A post record.",
            "properties": [
                {
                    "name": "text",
                    "type": { "variant": "String", "format": null },
                    "is_required": true
                },
                {
                    "name": "createdAt",
                    "type": { "variant": "String", "format": "DateTime" },
                    "is_required": true
                }
            ],
            "required_fields": ["text", "createdAt"]
        }));
        ctx.insert("defs", &serde_json::json!([]));
        ctx.insert("project", &serde_json::json!({
            "app_name": "test-app",
            "database_target": "postgres"
        }));

        let result = tera.render("atproto/lexicon_record.tera", &ctx)
            .expect("lexicon_record.tera should render");

        eprintln!("Rendered lexicon_record:\n{}", result);
        assert!(result.contains("\"lexicon\": 1"));
        assert!(result.contains("\"id\": \"app.test.post\""));
        assert!(result.contains("\"type\": \"record\""));
        assert!(result.contains("\"text\""));
        assert!(result.contains("\"createdAt\""));
        assert!(result.contains("\"type\": \"string\", \"format\": \"datetime\""));
    }

    #[test]
    fn lexicon_object_template_renders_basic_object() {
        let mut tera = load_tera();
        let mut ctx = tera::Context::new();

        ctx.insert("lexicon", &serde_json::json!({
            "nsid": "app.test.imageEmbed",
            "lex_type": "object",
            "key_strategy": null,
            "revision": 1,
            "description": "An embedded image.",
            "domain": "test"
        }));
        ctx.insert("object", &serde_json::json!({
            "name": "ImageEmbed",
            "description": "Image embed object.",
            "properties": [
                {
                    "name": "url",
                    "type": { "variant": "String", "format": "Uri" },
                    "is_required": true
                },
                {
                    "name": "width",
                    "type": { "variant": "Integer" },
                    "is_required": false
                }
            ],
            "required_fields": ["url"]
        }));

        let result = tera.render("atproto/lexicon_object.tera", &ctx)
            .expect("lexicon_object.tera should render");

        eprintln!("Rendered lexicon_object:\n{}", result);
        assert!(result.contains("\"lexicon\": 1"));
        assert!(result.contains("\"type\": \"object\""));
        assert!(result.contains("\"url\""));
        assert!(result.contains("\"uri\""));
        assert!(result.contains("\"type\": \"integer\""));
    }

    #[test]
    fn lexicon_enum_template_renders_closed_enum() {
        let mut tera = load_tera();
        let mut ctx = tera::Context::new();

        ctx.insert("lexicon", &serde_json::json!({
            "nsid": "app.test.status",
            "lex_type": "enum",
            "revision": 1,
            "description": "Post status enum.",
            "domain": "test"
        }));
        ctx.insert("codelist", &serde_json::json!({
            "name": "Status",
            "description": "Post status values.",
            "values": ["draft", "published", "archived"],
            "is_closed": true
        }));

        let result = tera.render("atproto/lexicon_enum.tera", &ctx)
            .expect("lexicon_enum.tera should render");

        eprintln!("Rendered lexicon_enum:\n{}", result);
        assert!(result.contains("\"enum\""));
        assert!(result.contains("\"draft\""));
        assert!(result.contains("\"published\""));
        assert!(!result.contains("knownValues"));
    }

    #[test]
    fn lexicon_enum_template_renders_open_enum() {
        let mut tera = load_tera();
        let mut ctx = tera::Context::new();

        ctx.insert("lexicon", &serde_json::json!({
            "nsid": "app.test.tags",
            "lex_type": "enum",
            "revision": 2,
            "description": "Open tag values.",
            "domain": "test"
        }));
        ctx.insert("codelist", &serde_json::json!({
            "name": "Tags",
            "description": null,
            "values": ["tech", "news", "sports"],
            "is_closed": false
        }));

        let result = tera.render("atproto/lexicon_enum.tera", &ctx)
            .expect("lexicon_enum.tera should render");

        eprintln!("Rendered open enum:\n{}", result);
        assert!(result.contains("\"knownValues\""));
        assert!(!result.contains("\"enum\""));
    }

    #[test]
    fn scaffold_template_renders_shared_defs() {
        let mut tera = load_tera();
        let mut ctx = tera::Context::new();

        ctx.insert("scaffold", &serde_json::json!({
            "nsid": "app.test.shared",
            "revision": 1,
            "description": "Shared defs for app.test.",
            "defs": [
                {
                    "name": "strongRef",
                    "type": "object",
                    "description": "A strong typed reference.",
                    "properties": [
                        {
                            "name": "uri",
                            "type": { "variant": "String", "format": "AtUri" },
                            "is_required": true
                        },
                        {
                            "name": "cid",
                            "type": { "variant": "String", "format": "Cid" },
                            "is_required": true
                        }
                    ],
                    "required_fields": ["uri", "cid"]
                },
                {
                    "name": "status",
                    "type": "string",
                    "description": "Status values.",
                    "values": ["active", "inactive"],
                    "is_closed": true
                }
            ]
        }));

        let result = tera.render("atproto/scaffold.tera", &ctx)
            .expect("scaffold.tera should render");

        eprintln!("Rendered scaffold:\n{}", result);
        assert!(result.contains("\"strongRef\""));
        assert!(result.contains("\"at-uri\""));
        assert!(result.contains("\"status\""));
        assert!(result.contains("\"enum\""));
    }

    #[test]
    fn lexicon_type_renders_blob_and_bytes() {
        let mut tera = load_tera();
        let mut ctx = tera::Context::new();

        ctx.insert("lexicon", &serde_json::json!({
            "nsid": "app.test.media",
            "lex_type": "object",
            "revision": 1,
            "description": "Media test.",
            "domain": "test"
        }));
        ctx.insert("object", &serde_json::json!({
            "name": "Media",
            "description": "Media object.",
            "properties": [
                {
                    "name": "file",
                    "type": {
                        "variant": "Blob",
                        "accept": ["image/png", "image/jpeg"],
                        "max_size": 1000000
                    },
                    "is_required": true
                },
                {
                    "name": "hash",
                    "type": { "variant": "Bytes", "max_size": 32 },
                    "is_required": false
                },
                {
                    "name": "link",
                    "type": { "variant": "CidLink" },
                    "is_required": false
                },
                {
                    "name": "parent",
                    "type": { "variant": "Ref", "nsid": "app.test.post" },
                    "is_required": false
                },
                {
                    "name": "item",
                    "type": { "variant": "StrongRef", "nsid": "app.test.post" },
                    "is_required": false
                }
            ],
            "required_fields": ["file"]
        }));

        let result = tera.render("atproto/lexicon_object.tera", &ctx)
            .expect("lexicon_object.tera should render");

        eprintln!("Rendered complex types:\n{}", result);
        assert!(result.contains("\"blob\""));
        assert!(result.contains("\"accept\""));
        assert!(result.contains("\"image/png\""));
        assert!(result.contains("\"maxSize\": 1000000"));
        assert!(result.contains("\"bytes\""));
        assert!(result.contains("\"maxSize\": 32"));
        assert!(result.contains("\"cid-link\""));
        assert!(result.contains("\"ref\""));
        assert!(result.contains("\"app.test.post\""));
        assert!(result.contains("\"at-uri\""));
    }

    #[test]
    fn lexicon_type_renders_array_union_token_unknown_boolean() {
        let mut tera = load_tera();
        let mut ctx = tera::Context::new();

        ctx.insert("lexicon", &serde_json::json!({
            "nsid": "app.test.edge",
            "lex_type": "object",
            "revision": 1,
            "description": "Edge cases.",
            "domain": "test"
        }));
        ctx.insert("object", &serde_json::json!({
            "name": "Edge",
            "description": null,
            "properties": [
                {
                    "name": "tags",
                    "type": {
                        "variant": "Array",
                        "items": { "variant": "String", "format": null }
                    },
                    "is_required": false
                },
                {
                    "name": "choice",
                    "type": {
                        "variant": "Union",
                        "refs": ["app.test.foo", "app.test.bar"],
                        "closed": false
                    },
                    "is_required": false
                },
                {
                    "name": "token",
                    "type": { "variant": "Token" },
                    "is_required": false
                },
                {
                    "name": "any",
                    "type": { "variant": "Unknown" },
                    "is_required": false
                },
                {
                    "name": "flag",
                    "type": { "variant": "Boolean" },
                    "is_required": false
                }
            ],
            "required_fields": []
        }));

        let result = tera.render("atproto/lexicon_object.tera", &ctx)
            .expect("lexicon_object.tera should render");

        eprintln!("Rendered edge cases:\n{}", result);
        assert!(result.contains("\"array\""));
        assert!(result.contains("\"items\""));
        assert!(result.contains("\"union\""));
        assert!(result.contains("\"refs\""));
        assert!(result.contains("\"closed\": false"));
        assert!(result.contains("\"token\""));
        assert!(result.contains("\"unknown\""));
        assert!(result.contains("\"boolean\""));
    }

    #[test]
    fn lexicon_type_renders_string_formats() {
        let mut tera = load_tera();
        let mut ctx = tera::Context::new();

        ctx.insert("lexicon", &serde_json::json!({
            "nsid": "app.test.formats",
            "lex_type": "object",
            "revision": 1,
            "description": "String format tests.",
            "domain": "test"
        }));
        ctx.insert("object", &serde_json::json!({
            "name": "Formats",
            "description": null,
            "properties": [
                { "name": "dt",       "type": { "variant": "String", "format": "DateTime" },     "is_required": false },
                { "name": "uri",      "type": { "variant": "String", "format": "AtUri" },        "is_required": false },
                { "name": "did",      "type": { "variant": "String", "format": "Did" },          "is_required": false },
                { "name": "handle",   "type": { "variant": "String", "format": "Handle" },       "is_required": false },
                { "name": "nsid",     "type": { "variant": "String", "format": "Nsid" },         "is_required": false },
                { "name": "language", "type": { "variant": "String", "format": "LanguageTag" },  "is_required": false },
                { "name": "cid",      "type": { "variant": "String", "format": "Cid" },          "is_required": false },
                { "name": "image_uri","type": { "variant": "String", "format": "Uri" },          "is_required": false },
                { "name": "plain",    "type": { "variant": "String", "format": null },           "is_required": false }
            ],
            "required_fields": []
        }));

        let result = tera.render("atproto/lexicon_object.tera", &ctx)
            .expect("lexicon_object.tera should render");

        eprintln!("Rendered string formats:\n{}", result);
        assert!(result.contains("\"format\": \"datetime\""));
        assert!(result.contains("\"format\": \"at-uri\""));
        assert!(result.contains("\"format\": \"did\""));
        assert!(result.contains("\"format\": \"handle\""));
        assert!(result.contains("\"format\": \"nsid\""));
        assert!(result.contains("\"format\": \"language\""));
        assert!(result.contains("\"format\": \"cid\""));
        assert!(result.contains("\"format\": \"uri\""));
        // Plain string: should have type but no format
        assert!(result.contains("\"type\": \"string\""));
    }

    #[test]
    fn lexicon_record_renders_defs_as_additional_types() {
        let mut tera = load_tera();
        let mut ctx = tera::Context::new();

        ctx.insert("lexicon", &serde_json::json!({
            "nsid": "app.test.post",
            "lex_type": "record",
            "key_strategy": "Tid",
            "revision": 1,
            "description": "Record with defs.",
            "domain": "test"
        }));
        ctx.insert("record", &serde_json::json!({
            "name": "Post",
            "description": "A post.",
            "properties": [
                {
                    "name": "text",
                    "type": { "variant": "String", "format": null },
                    "is_required": true
                }
            ],
            "required_fields": ["text"]
        }));
        ctx.insert("defs", &serde_json::json!([
            {
                "name": "author",
                "description": "Post author info.",
                "properties": [
                    {
                        "name": "name",
                        "type": { "variant": "String", "format": null },
                        "is_required": true
                    },
                    {
                        "name": "avatar",
                        "type": { "variant": "String", "format": "Uri" },
                        "is_required": false
                    }
                ],
                "required_fields": ["name"]
            }
        ]));

        let result = tera.render("atproto/lexicon_record.tera", &ctx)
            .expect("lexicon_record.tera should render");

        eprintln!("Rendered record with defs:\n{}", result);
        assert!(result.contains("\"main\""));
        assert!(result.contains("\"author\""));
        assert!(result.contains("\"name\""));
        assert!(result.contains("\"avatar\""));
    }

    #[test]
    fn lexicon_record_renders_key_strategy_variants() {
        let mut tera = load_tera();
        let mut ctx = tera::Context::new();

        for (strategy, expected) in &[
            ("Tid", "tid"),
            ("LiteralSelf", "#self"),
            ("Any", "*"),
            ("Nsid", "nsid"),
        ] {
            ctx.insert("lexicon", &serde_json::json!({
                "nsid": "app.test.obj",
                "lex_type": "record",
                "key_strategy": strategy,
                "revision": 1,
                "description": format!("Key strategy: {}", strategy),
                "domain": "test"
            }));
            ctx.insert("record", &serde_json::json!({
                "name": "Obj",
                "description": "Object.",
                "properties": [],
                "required_fields": []
            }));
            ctx.insert("defs", &serde_json::json!([]));

            let result = tera.render("atproto/lexicon_record.tera", &ctx)
                .unwrap_or_else(|e| panic!("Failed to render with strategy {}: {}", strategy, e));

            assert!(
                result.contains(&format!("\"key\": \"{}\"", expected)),
                "Expected key \"{}\" for strategy {}, got:\n{}",
                expected,
                strategy,
                result
            );
        }
    }

    #[test]
    fn lexicon_record_defaults_revision_to_1() {
        let mut tera = load_tera();
        let mut ctx = tera::Context::new();

        ctx.insert("lexicon", &serde_json::json!({
            "nsid": "app.test.obj",
            "lex_type": "record",
            "key_strategy": "Tid",
            "description": "No revision specified.",
            "domain": "test"
        }));
        ctx.insert("record", &serde_json::json!({
            "name": "Obj",
            "description": null,
            "properties": [],
            "required_fields": []
        }));
        ctx.insert("defs", &serde_json::json!([]));

        let result = tera.render("atproto/lexicon_record.tera", &ctx)
            .expect("Should render without revision set");

        assert!(result.contains("\"revision\": 1"), "Should default revision to 1");
    }
}

#[cfg(test)]
mod integration_tests {
    use std::path::PathBuf;

    use codegraph_core::mock::MockEngine;
    use codegraph_core::traits::GraphIngestor;
    use codegraph_core::types::{LexiconNode, NamespaceNode, PropertyNode, SchemaNode};
    use codegraph_type_contracts::RefClassificationKind;
    use tera::Tera;

    use super::super::lexicon_gen::LexiconEmitter;
    use super::super::scaffold_gen::LexiconScaffoldEmitter;
    use crate::generate::traits::{EntityGenerator, GlobalGenerator};
    use crate::generate::ProjectConfig;

    fn make_domain_config() -> codegraph_config::DomainConfig {
        let domains = std::collections::HashMap::new();
        codegraph_config::DomainConfig {
            defaults: Default::default(),
            domains,
        }
    }

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

    fn make_project() -> ProjectConfig {
        ProjectConfig {
            atproto_authority: "nz.gravy".to_string(),
            ..Default::default()
        }
    }

    fn make_schema(title: &str, domain: &str) -> SchemaNode {
        SchemaNode {
            schema_id: format!("id:{}", title),
            title: title.to_string(),
            description: Some(format!("The {} schema", title)),
            schema_type: "object".to_string(),
            classification: "entity".to_string(),
            domain: Some(domain.to_string()),
            rel_path: format!("{}.json", title),
            pg_type: "UUID".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: title.to_string(),
            pg_table_name: codegraph_naming::to_snake_case(title),
            api_path_segment: codegraph_naming::to_kebab_case(title),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
        }
    }

    fn make_primitive_prop(name: &str, prop_type: &str, is_required: bool) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            prop_type: prop_type.to_string(),
            description: Some(format!("The {} field", name)),
            format: None,
            is_required,
            is_nullable: false,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: codegraph_naming::to_snake_case(name),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: name.to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "String".to_string(),
            render_strategy: "primitive_wrapper".to_string(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
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

        let namespace = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_namespace(&namespace).await.unwrap();

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
        assert!(json["defs"]["main"]["required"].as_array().unwrap().contains(&serde_json::Value::String("name".to_string())));
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

        assert!(result.is_empty(), "should return empty when no lexicon mapping exists");
    }

    #[tokio::test]
    async fn test_scaffold_emitter_produces_catalog() {
        let engine = MockEngine::builder().build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

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
            .generate(
                &engine,
                &make_domain_config(),
                &[],
                &tera,
                &project,
            )
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
            atproto_authority: "".to_string(),
            ..Default::default()
        };
        let emitter = LexiconScaffoldEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                &make_domain_config(),
                &[],
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        assert!(result.is_empty(), "should return empty when authority is blank");
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

        let namespace = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "recruiting".to_string(),
        };
        engine.ingest_namespace(&namespace).await.unwrap();

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
        let json: serde_json::Value =
            serde_json::from_str(&result[0].content).expect("valid JSON");

        let props = &json["defs"]["main"]["properties"];
        assert!(props["email"].is_object());
        assert!(props["score"].is_object());
        assert!(json["defs"]["main"]["required"].as_array().unwrap().contains(&serde_json::Value::String("email".to_string())));
    }
}

#[cfg(test)]
mod atproto_client_tests {
    use std::path::PathBuf;

    use codegraph_core::mock::MockEngine;
    use codegraph_core::traits::GraphIngestor;
    use codegraph_core::types::{CollectionNode, PropertyNode, SchemaNode};
    use codegraph_type_contracts::RefClassificationKind;
    use tera::Tera;

    use super::super::client_gen::{AtprotoClientEmitter, AtprotoClientScaffoldEmitter};
    use crate::generate::traits::{EntityGenerator, GlobalGenerator};
    use crate::generate::ProjectConfig;
    use codegraph_naming;

    fn make_domain_config() -> codegraph_config::DomainConfig {
        let domains = std::collections::HashMap::new();
        codegraph_config::DomainConfig {
            defaults: Default::default(),
            domains,
        }
    }

    fn make_project() -> ProjectConfig {
        ProjectConfig {
            atproto_authority: "nz.gravy".to_string(),
            ..Default::default()
        }
    }

    fn make_schema(title: &str, domain: &str) -> SchemaNode {
        SchemaNode {
            schema_id: format!("id:{}", title),
            title: title.to_string(),
            description: Some(format!("The {} schema", title)),
            schema_type: "object".to_string(),
            classification: "entity".to_string(),
            domain: Some(domain.to_string()),
            rel_path: format!("{}.json", title),
            pg_type: "UUID".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: title.to_string(),
            pg_table_name: codegraph_naming::to_snake_case(title),
            api_path_segment: codegraph_naming::to_kebab_case(title),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
        }
    }

    fn make_primitive_prop(name: &str, prop_type: &str, is_required: bool) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            prop_type: prop_type.to_string(),
            description: Some(format!("The {} field", name)),
            format: None,
            is_required,
            is_nullable: false,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: codegraph_naming::to_snake_case(name),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: name.to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "String".to_string(),
            render_strategy: "primitive_wrapper".to_string(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
    }

    fn make_client_tera() -> Tera {
        let mut tera = Tera::default();

        tera.add_raw_template(
            "atproto/client.tera",
            r#"use std::sync::Arc;

pub const COLLECTION: &str = "{{ collection_nsid }}";

pub struct {{ entity_name }}Client {
    repo: Arc<dyn RepoWriter>,
    pds_endpoint: String,
}

impl {{ entity_name }}Client {
    pub fn new(repo: Arc<dyn RepoWriter>, pds_endpoint: &str) -> Self {
        Self { repo, pds_endpoint: pds_endpoint.to_string() }
    }

    pub async fn create(&self, record: &{{ record_type }}) -> Result<CreateRecordResponse, AtprotoError> {
        self.repo.create_record(
            &self.pds_endpoint,
            "{{ collection_nsid }}",
            record,
        ).await
    }

    pub async fn get(&self, rkey: &str) -> Result<Option<{{ record_type }}>, AtprotoError> {
        self.repo.get_record::<{{ record_type }}>(
            &self.pds_endpoint,
            "{{ collection_nsid }}",
            rkey,
        ).await
    }

    pub async fn delete(&self, rkey: &str) -> Result<(), AtprotoError> {
        self.repo.delete_record(
            &self.pds_endpoint,
            "{{ collection_nsid }}",
            rkey,
        ).await
    }

    pub async fn list(&self, repo_did: &str, limit: Option<u32>, cursor: Option<&str>) -> Result<ListRecordsResponse<{{ record_type }}>, AtprotoError> {
        self.repo.list_records::<{{ record_type }}>(
            &self.pds_endpoint,
            repo_did,
            "{{ collection_nsid }}",
            limit.unwrap_or(50),
            cursor,
        ).await
    }
}
"#,
        )
        .unwrap();

        tera.add_raw_template(
            "atproto/client_scaffold.tera",
            r#"{% for entity in entities %}pub mod {{ entity.module_name }}_client;
{% endfor %}
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub struct AtprotoClient {
    pub repo: Arc<dyn RepoWriter>,
    pub pds_endpoint: String,
    pub did: String,
}

impl AtprotoClient {
    pub fn new(repo: Arc<dyn RepoWriter>, pds_endpoint: &str, did: &str) -> Self {
        Self { repo, pds_endpoint: pds_endpoint.to_string(), did: did.to_string() }
    }
}

#[derive(Debug, Serialize)]
pub struct CreateRecordResponse {
    pub uri: String,
    pub cid: String,
}

#[derive(Debug, Deserialize)]
pub struct ListRecordsResponse<T> {
    pub records: Vec<T>,
    pub cursor: Option<String>,
}

pub trait RepoWriter: Send + Sync {
    async fn create_record<T: Serialize + Send + Sync>(
        &self, pds_endpoint: &str, collection: &str, record: &T,
    ) -> Result<CreateRecordResponse, AtprotoError>;

    async fn get_record<T: serde::de::DeserializeOwned + Send>(
        &self, pds_endpoint: &str, collection: &str, rkey: &str,
    ) -> Result<Option<T>, AtprotoError>;

    async fn delete_record(
        &self, pds_endpoint: &str, collection: &str, rkey: &str,
    ) -> Result<(), AtprotoError>;

    async fn list_records<T: serde::de::DeserializeOwned>(
        &self, pds_endpoint: &str, repo_did: &str, collection: &str,
        limit: u32, cursor: Option<&str>,
    ) -> Result<ListRecordsResponse<T>, AtprotoError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AtprotoError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("AT Protocol error: {0}")]
    Atproto(String),
}
"#,
        )
        .unwrap();

        tera
    }

    #[tokio::test]
    async fn test_client_emitter_produces_client_struct() {
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

        let collection = CollectionNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            key_strategy: "tid".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_collection(&collection).await.unwrap();

        let tera = make_client_tera();
        let project = make_project();
        let emitter = AtprotoClientEmitter::new(&PathBuf::from("/tmp/test-out"));

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

        let content = &result[0].content;

        assert!(
            content.contains("pub struct GrantClient"),
            "should contain pub struct GrantClient, got:\n{}",
            content
        );
        assert!(
            content.contains("pub const COLLECTION: &str"),
            "should contain pub const COLLECTION: &str, got:\n{}",
            content
        );
        assert!(
            content.contains("create_record"),
            "should contain create_record method, got:\n{}",
            content
        );
        assert!(
            content.contains("nz.gravy.grants.grant"),
            "should contain collection NSID, got:\n{}",
            content
        );
    }

    #[tokio::test]
    async fn test_client_emitter_skips_when_no_collection() {
        let engine = MockEngine::builder()
            .with_schema(make_schema("Grant", "grants"))
            .with_properties("Grant", vec![make_primitive_prop("name", "string", true)])
            .build();

        let tera = make_client_tera();
        let project = make_project();
        let emitter = AtprotoClientEmitter::new(&PathBuf::from("/tmp/test-out"));

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
            "should return empty when no collection exists"
        );
    }

    #[tokio::test]
    async fn test_client_emitter_skips_when_authority_empty() {
        let engine = MockEngine::builder()
            .with_schema(make_schema("Grant", "grants"))
            .build();

        let tera = make_client_tera();
        let project = ProjectConfig {
            atproto_authority: "".to_string(),
            ..Default::default()
        };
        let emitter = AtprotoClientEmitter::new(&PathBuf::from("/tmp/test-out"));

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
            "should return empty when authority is blank"
        );
    }

    #[tokio::test]
    async fn test_client_scaffold_emitter_produces_repo_writer_trait() {
        let engine = MockEngine::builder().build();

        let coll1 = CollectionNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            key_strategy: "tid".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_collection(&coll1).await.unwrap();

        let coll2 = CollectionNode {
            nsid: "nz.gravy.grants.grantee".to_string(),
            key_strategy: "tid".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_collection(&coll2).await.unwrap();

        let tera = make_client_tera();
        let project = make_project();
        let emitter = AtprotoClientScaffoldEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                &make_domain_config(),
                &[],
                &tera,
                &project,
            )
            .await
            .expect("scaffold generation should succeed");

        assert_eq!(result.len(), 1, "should produce one file");

        let content = &result[0].content;

        assert!(
            content.contains("pub struct AtprotoClient"),
            "should contain AtprotoClient struct, got:\n{}",
            content
        );
        assert!(
            content.contains("pub trait RepoWriter"),
            "should contain pub trait RepoWriter, got:\n{}",
            content
        );
        assert!(
            content.contains("async fn create_record"),
            "should contain create_record method, got:\n{}",
            content
        );
        assert!(
            content.contains("async fn get_record"),
            "should contain get_record method, got:\n{}",
            content
        );
        assert!(
            content.contains("async fn delete_record"),
            "should contain delete_record method, got:\n{}",
            content
        );
        assert!(
            content.contains("async fn list_records"),
            "should contain list_records method, got:\n{}",
            content
        );
        assert!(
            content.contains("pub mod grant_client"),
            "should contain grant_client module declaration, got:\n{}",
            content
        );
        assert!(
            content.contains("pub mod grantee_client"),
            "should contain grantee_client module declaration, got:\n{}",
            content
        );
    }

    #[tokio::test]
    async fn test_client_scaffold_skips_when_authority_empty() {
        let engine = MockEngine::builder().build();

        let coll = CollectionNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            key_strategy: "tid".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_collection(&coll).await.unwrap();

        let tera = make_client_tera();
        let project = ProjectConfig {
            atproto_authority: "".to_string(),
            ..Default::default()
        };
        let emitter = AtprotoClientScaffoldEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                &make_domain_config(),
                &[],
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        assert!(
            result.is_empty(),
            "should return empty when authority is blank"
        );
    }
}

#[cfg(test)]
mod atproto_types_tests {
    use std::path::PathBuf;

    use codegraph_core::mock::MockEngine;
    use codegraph_core::traits::GraphIngestor;
    use codegraph_core::types::{LexiconNode, NamespaceNode, PropertyNode, SchemaNode};
    use codegraph_type_contracts::RefClassificationKind;
    use tera::Tera;

    use super::super::types_gen::AtprotoTypesEmitter;
    use crate::generate::traits::EntityGenerator;
    use crate::generate::ProjectConfig;

    fn make_types_domain_config() -> codegraph_config::DomainConfig {
        let domains = std::collections::HashMap::new();
        codegraph_config::DomainConfig {
            defaults: Default::default(),
            domains,
        }
    }

    fn make_types_tera() -> Tera {
        let mut tera = Tera::default();
        tera.add_raw_template(
            "atproto/rust_type.tera",
            include_str!("../../../templates/atproto/rust_type.tera"),
        )
        .unwrap();
        tera.add_raw_template(
            "atproto/rust_record_impl.tera",
            include_str!("../../../templates/atproto/rust_record_impl.tera"),
        )
        .unwrap();
        tera.add_raw_template(
            "atproto/rust_enum.tera",
            include_str!("../../../templates/atproto/rust_enum.tera"),
        )
        .unwrap();
        tera
    }

    fn make_types_project() -> ProjectConfig {
        ProjectConfig {
            atproto_authority: "nz.gravy".to_string(),
            ..Default::default()
        }
    }

    fn types_schema(title: &str, domain: &str) -> SchemaNode {
        SchemaNode {
            schema_id: format!("id:{}", title),
            title: title.to_string(),
            description: Some(format!("The {} schema", title)),
            schema_type: "object".to_string(),
            classification: "entity".to_string(),
            domain: Some(domain.to_string()),
            rel_path: format!("{}.json", title),
            pg_type: "UUID".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: title.to_string(),
            pg_table_name: codegraph_naming::to_snake_case(title),
            api_path_segment: codegraph_naming::to_kebab_case(title),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
        }
    }

    fn types_primitive_prop(name: &str, prop_type: &str, is_required: bool) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            prop_type: prop_type.to_string(),
            description: Some(format!("The {} field", name)),
            format: None,
            is_required,
            is_nullable: false,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: codegraph_naming::to_snake_case(name),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: name.to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "String".to_string(),
            render_strategy: "primitive_wrapper".to_string(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
    }

    fn types_prop_with_kind(
        name: &str,
        prop_type: &str,
        is_required: bool,
        kind: RefClassificationKind,
    ) -> PropertyNode {
        let mut p = types_primitive_prop(name, prop_type, is_required);
        p.classification_kind = Some(kind);
        p
    }

    fn types_prop_with_format(
        name: &str,
        prop_type: &str,
        is_required: bool,
        format: &str,
    ) -> PropertyNode {
        let mut p = types_primitive_prop(name, prop_type, is_required);
        p.format = Some(format.to_string());
        p
    }

    // ── Test 1: Basic record struct generation ────────────────────────

    #[tokio::test]
    async fn generates_pub_struct_for_record() {
        let engine = MockEngine::builder()
            .with_schema(types_schema("Grant", "grants"))
            .with_properties(
                "Grant",
                vec![
                    types_primitive_prop("name", "string", true),
                    types_primitive_prop("amount", "integer", false),
                ],
            )
            .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex = LexiconNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A grant record".to_string()),
            domain: "grants".to_string(),
        };
        engine.ingest_lexicon(&lex).await.unwrap();

        let tera = make_types_tera();
        let project = make_types_project();
        let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "Grant",
                "grants",
                &make_types_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        assert_eq!(result.len(), 1, "should produce one file");
        let content = &result[0].content;

        eprintln!("Generated types:\n{}", content);

        assert!(
            content.contains("pub struct GrantRecord"),
            "should contain pub struct GrantRecord"
        );
        assert!(
            content.contains("#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]"),
            "should have serde derives"
        );
        assert!(
            content.contains("#[serde(rename_all = \"camelCase\")]"),
            "should have camelCase rename"
        );
    }

    // ── Test 2: NSID constant and $type field ──────────────────────────

    #[tokio::test]
    async fn record_has_nsid_and_type_field() {
        let engine = MockEngine::builder()
            .with_schema(types_schema("Grant", "grants"))
            .with_properties(
                "Grant",
                vec![types_primitive_prop("name", "string", true)],
            )
            .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex = LexiconNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A grant record".to_string()),
            domain: "grants".to_string(),
        };
        engine.ingest_lexicon(&lex).await.unwrap();

        let tera = make_types_tera();
        let project = make_types_project();
        let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "Grant",
                "grants",
                &make_types_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        let content = &result[0].content;

        assert!(content.contains("pub const NSID: &str"), "should have NSID constant");
        assert!(
            content.contains("nz.gravy.grants.grant"),
            "NSID should have the correct value"
        );
        assert!(
            content.contains("fn type_default()"),
            "should have type_default function"
        );
        assert!(
            content.contains("#[serde(rename = \"$type\", default = \"type_default\")]"),
            "should have $type field with serde rename"
        );
        assert!(
            content.contains("pub r#type: String"),
            "should have r#type field"
        );
    }

    // ── Test 3: Option wrapping for non-required fields ────────────────

    #[tokio::test]
    async fn non_required_fields_are_options() {
        let engine = MockEngine::builder()
            .with_schema(types_schema("Grant", "grants"))
            .with_properties(
                "Grant",
                vec![
                    types_primitive_prop("name", "string", true),
                    types_primitive_prop("description", "string", false),
                ],
            )
            .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex = LexiconNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A grant record".to_string()),
            domain: "grants".to_string(),
        };
        engine.ingest_lexicon(&lex).await.unwrap();

        let tera = make_types_tera();
        let project = make_types_project();
        let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "Grant",
                "grants",
                &make_types_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        let content = &result[0].content;

        eprintln!("Generated with optional field:\n{}", content);

        assert!(
            content.contains("pub description: Option<String>"),
            "non-required string field should be Option<String>"
        );
        assert!(
            content.contains("pub name: String"),
            "required string field should be String (not Option)"
        );
    }

    // ── Test 4: DateTime field maps to chrono::DateTime<chrono::Utc> ──

    #[tokio::test]
    async fn datetime_field_maps_to_chrono() {
        let engine = MockEngine::builder()
            .with_schema(types_schema("Grant", "grants"))
            .with_properties(
                "Grant",
                vec![
                    types_primitive_prop("name", "string", true),
                    types_prop_with_format("createdAt", "string", true, "date-time"),
                ],
            )
            .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex = LexiconNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A grant record".to_string()),
            domain: "grants".to_string(),
        };
        engine.ingest_lexicon(&lex).await.unwrap();

        let tera = make_types_tera();
        let project = make_types_project();
        let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "Grant",
                "grants",
                &make_types_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        let content = &result[0].content;

        eprintln!("Generated with datetime:\n{}", content);
        assert!(
            content.contains("chrono::DateTime<chrono::Utc>"),
            "date-time format should map to chrono::DateTime<chrono::Utc>"
        );
    }

    // ── Test 5: Bytes field maps to Vec<u8> with serde_bytes ──────────

    #[tokio::test]
    async fn bytes_field_maps_to_vec_u8_with_serde_bytes() {
        let engine = MockEngine::builder()
            .with_schema(types_schema("Media", "media"))
            .with_properties(
                "Media",
                vec![types_prop_with_format("hash", "string", false, "byte")],
            )
            .with_lexicon_mapping("Media", "nz.gravy.media.hash")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "media".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex = LexiconNode {
            nsid: "nz.gravy.media.hash".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A media record".to_string()),
            domain: "media".to_string(),
        };
        engine.ingest_lexicon(&lex).await.unwrap();

        let tera = make_types_tera();
        let project = make_types_project();
        let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "Media",
                "media",
                &make_types_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        let content = &result[0].content;

        eprintln!("Generated with bytes:\n{}", content);
        assert!(
            content.contains("Vec<u8>"),
            "bytes should map to Vec<u8>"
        );
        assert!(
            content.contains("#[serde(with = \"serde_bytes\")]"),
            "bytes field should have serde_bytes with attribute"
        );
    }

    // ── Test 6: Boolean and integer field mappings ─────────────────────

    #[tokio::test]
    async fn boolean_and_integer_field_mappings() {
        let engine = MockEngine::builder()
            .with_schema(types_schema("Flags", "test"))
            .with_properties(
                "Flags",
                vec![
                    types_primitive_prop("active", "boolean", true),
                    types_primitive_prop("count", "integer", false),
                ],
            )
            .with_lexicon_mapping("Flags", "nz.gravy.test.flag")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "test".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex = LexiconNode {
            nsid: "nz.gravy.test.flag".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("Flags record".to_string()),
            domain: "test".to_string(),
        };
        engine.ingest_lexicon(&lex).await.unwrap();

        let tera = make_types_tera();
        let project = make_types_project();
        let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "Flags",
                "test",
                &make_types_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        let content = &result[0].content;

        eprintln!("Generated with bool and int:\n{}", content);
        assert!(content.contains("pub active: bool"), "boolean should be bool");
        assert!(
            content.contains("pub count: Option<i64>"),
            "integer should be i64, optional when not required"
        );
    }

    // ── Test 7: Media/Blob field generates BlobRef ─────────────────────

    #[tokio::test]
    async fn media_field_generates_blob_ref() {
        let engine = MockEngine::builder()
            .with_schema(types_schema("File", "files"))
            .with_properties(
                "File",
                vec![types_prop_with_kind("avatar", "string", true, RefClassificationKind::MediaWrapper)],
            )
            .with_lexicon_mapping("File", "nz.gravy.files.avatar")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "files".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex = LexiconNode {
            nsid: "nz.gravy.files.avatar".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A file record".to_string()),
            domain: "files".to_string(),
        };
        engine.ingest_lexicon(&lex).await.unwrap();

        let tera = make_types_tera();
        let project = make_types_project();
        let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "File",
                "files",
                &make_types_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        let content = &result[0].content;

        eprintln!("Generated with blob:\n{}", content);
        assert!(
            content.contains("pub struct BlobRef"),
            "should contain BlobRef struct when a blob field is present"
        );
        assert!(
            content.contains("pub avatar: BlobRef"),
            "media field should use BlobRef type"
        );
    }

    // ── Test 8: Entity reference maps to String ────────────────────────

    #[tokio::test]
    async fn entity_reference_maps_to_string() {
        let engine = MockEngine::builder()
            .with_schema(types_schema("Grant", "grants"))
            .with_properties(
                "Grant",
                vec![
                    types_primitive_prop("name", "string", true),
                    types_prop_with_kind("grantee", "string", false, RefClassificationKind::EntityReference),
                ],
            )
            .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
            .build();

        let ns = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "".to_string(),
            domain: "grants".to_string(),
        };
        engine.ingest_namespace(&ns).await.unwrap();

        let lex = LexiconNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A grant record".to_string()),
            domain: "grants".to_string(),
        };
        engine.ingest_lexicon(&lex).await.unwrap();

        let tera = make_types_tera();
        let project = make_types_project();
        let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "Grant",
                "grants",
                &make_types_domain_config(),
                &tera,
                &project,
            )
            .await
            .expect("generation should succeed");

        let content = &result[0].content;

        eprintln!("Generated with entity ref:\n{}", content);
        assert!(
            content.contains("pub grantee: Option<String>"),
            "entity reference field should map to Option<String>"
        );
    }

    // ── Test 9: Skips non-atproto schemas ──────────────────────────────

    #[tokio::test]
    async fn skips_non_atproto_schema() {
        let engine = MockEngine::builder()
            .with_schema(types_schema("Grant", "grants"))
            .with_properties(
                "Grant",
                vec![types_primitive_prop("name", "string", true)],
            )
            .build();

        let tera = make_types_tera();
        let project = make_types_project();
        let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

        let result = emitter
            .generate(
                &engine,
                "Grant",
                "grants",
                &make_types_domain_config(),
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
}
