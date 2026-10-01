use std::path::Path;
use tera::Tera;

// Canonical LexiconType shape for test fixtures:
//
// Property type objects MUST use the `"type"` key (not `"variant"`) with
// lowercase values matching the LexiconType serde tag:
//   { "type": "string" }
//   { "type": "string", "format": "datetime" }
//   { "type": "integer" }
//   { "type": "number" }
//   { "type": "boolean" }
//   { "type": "bytes" }
//   { "type": "uri" }
//   { "type": "ref", "ref_name": "app.test.post" }
//   { "type": "union", "refs": ["app.test.foo"], "closed": false }
//   { "type": "array", "items": { "type": "string" } }
//   { "type": "object", "def_name": "MyObject" }
//   { "type": "unknown" }
//
// Object/record templates expect data under the `"record"` key (not `"object"`).

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
    assert!(
        !atproto.is_empty(),
        "Expected atproto templates to be loaded"
    );
    eprintln!("Loaded AT Proto templates: {:?}", atproto);
}

#[test]
fn lexicon_record_template_renders_basic_record() {
    let tera = load_tera();
    let mut ctx = tera::Context::new();

    ctx.insert(
        "lexicon",
        &serde_json::json!({
            "nsid": "app.test.post",
            "lex_type": "record",
            "key_strategy": "Tid",
            "revision": 1,
            "description": "A test record.",
            "domain": "test"
        }),
    );
    ctx.insert(
        "namespace",
        &serde_json::json!({
            "authority": "app.test"
        }),
    );
    ctx.insert(
        "record",
        &serde_json::json!({
            "name": "Post",
            "description": "A post record.",
            "properties": [
                {
                    "name": "text",
                    "type": { "type": "string" },
                    "is_required": true
                },
                {
                    "name": "createdAt",
                    "type": { "type": "datetime" },
                    "is_required": true
                }
            ],
            "required_fields": ["text", "createdAt"]
        }),
    );
    ctx.insert("defs", &serde_json::json!([]));
    ctx.insert(
        "project",
        &serde_json::json!({
            "app_name": "test-app",
            "database_target": "postgres"
        }),
    );

    let result = tera
        .render("atproto/lexicon_record.tera", &ctx)
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
    let tera = load_tera();
    let mut ctx = tera::Context::new();

    ctx.insert(
        "lexicon",
        &serde_json::json!({
            "nsid": "app.test.imageEmbed",
            "lex_type": "object",
            "key_strategy": null,
            "revision": 1,
            "description": "An embedded image.",
            "domain": "test"
        }),
    );
    ctx.insert(
        "record",
        &serde_json::json!({
            "name": "ImageEmbed",
            "description": "Image embed object.",
            "properties": [
                {
                    "name": "url",
                    "type": { "type": "uri" },
                    "is_required": true
                },
                {
                    "name": "width",
                    "type": { "type": "integer" },
                    "is_required": false
                }
            ],
            "required_fields": ["url"]
        }),
    );

    let result = tera
        .render("atproto/lexicon_object.tera", &ctx)
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
    let tera = load_tera();
    let mut ctx = tera::Context::new();

    ctx.insert(
        "lexicon",
        &serde_json::json!({
            "nsid": "app.test.status",
            "lex_type": "enum",
            "revision": 1,
            "description": "Post status enum.",
            "domain": "test"
        }),
    );
    ctx.insert(
        "codelist",
        &serde_json::json!({
            "name": "Status",
            "description": "Post status values.",
            "values": ["draft", "published", "archived"],
            "is_closed": true
        }),
    );

    let result = tera
        .render("atproto/lexicon_enum.tera", &ctx)
        .expect("lexicon_enum.tera should render");

    eprintln!("Rendered lexicon_enum:\n{}", result);
    assert!(result.contains("\"enum\""));
    assert!(result.contains("\"draft\""));
    assert!(result.contains("\"published\""));
    assert!(!result.contains("knownValues"));
}

#[test]
fn lexicon_enum_template_renders_open_enum() {
    let tera = load_tera();
    let mut ctx = tera::Context::new();

    ctx.insert(
        "lexicon",
        &serde_json::json!({
            "nsid": "app.test.tags",
            "lex_type": "enum",
            "revision": 2,
            "description": "Open tag values.",
            "domain": "test"
        }),
    );
    ctx.insert(
        "codelist",
        &serde_json::json!({
            "name": "Tags",
            "description": null,
            "values": ["tech", "news", "sports"],
            "is_closed": false
        }),
    );

    let result = tera
        .render("atproto/lexicon_enum.tera", &ctx)
        .expect("lexicon_enum.tera should render");

    eprintln!("Rendered open enum:\n{}", result);
    assert!(result.contains("\"knownValues\""));
    assert!(!result.contains("\"enum\""));
}

#[test]
fn scaffold_template_renders_shared_defs() {
    let tera = load_tera();
    let mut ctx = tera::Context::new();

    ctx.insert(
        "scaffold",
        &serde_json::json!({
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
                            "type": { "type": "uri" },
                            "is_required": true
                        },
                        {
                            "name": "cid",
                            "type": { "type": "string" },
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
        }),
    );

    let result = tera
        .render("atproto/scaffold.tera", &ctx)
        .expect("scaffold.tera should render");

    eprintln!("Rendered scaffold:\n{}", result);
    assert!(result.contains("\"strongRef\""));
    assert!(result.contains("\"uri\""));
    assert!(result.contains("\"status\""));
    assert!(result.contains("\"enum\""));
}

#[test]
fn lexicon_type_renders_blob_and_bytes() {
    let tera = load_tera();
    let mut ctx = tera::Context::new();

    ctx.insert(
        "lexicon",
        &serde_json::json!({
            "nsid": "app.test.media",
            "lex_type": "object",
            "revision": 1,
            "description": "Media test.",
            "domain": "test"
        }),
    );
    ctx.insert(
        "record",
        &serde_json::json!({
            "name": "Media",
            "description": "Media object.",
            "properties": [
                {
                    "name": "file",
                    "type": {
                        "type": "bytes"
                    },
                    "is_required": true
                },
                {
                    "name": "hash",
                    "type": { "type": "bytes" },
                    "is_required": false
                },
                {
                    "name": "link",
                    "type": { "type": "unknown" },
                    "is_required": false
                },
                {
                    "name": "parent",
                    "type": { "type": "ref", "ref_name": "app.test.post" },
                    "is_required": false
                },
                {
                    "name": "item",
                    "type": { "type": "ref", "ref_name": "app.test.post" },
                    "is_required": false
                }
            ],
            "required_fields": ["file"]
        }),
    );

    let result = tera
        .render("atproto/lexicon_object.tera", &ctx)
        .expect("lexicon_object.tera should render");

    eprintln!("Rendered complex types:\n{}", result);
    assert!(result.contains("\"bytes\""));
    assert!(result.contains("\"ref\""));
    assert!(result.contains("\"app.test.post\""));
}

#[test]
fn lexicon_type_renders_array_union_token_unknown_boolean() {
    let tera = load_tera();
    let mut ctx = tera::Context::new();

    ctx.insert(
        "lexicon",
        &serde_json::json!({
            "nsid": "app.test.edge",
            "lex_type": "object",
            "revision": 1,
            "description": "Edge cases.",
            "domain": "test"
        }),
    );
    ctx.insert(
        "record",
        &serde_json::json!({
            "name": "Edge",
            "description": null,
            "properties": [
                {
                    "name": "tags",
                    "type": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "is_required": false
                },
                {
                    "name": "choice",
                    "type": {
                        "type": "union",
                        "refs": ["app.test.foo", "app.test.bar"],
                        "closed": false
                    },
                    "is_required": false
                },
                {
                    "name": "token",
                    "type": { "type": "unknown" },
                    "is_required": false
                },
                {
                    "name": "any",
                    "type": { "type": "unknown" },
                    "is_required": false
                },
                {
                    "name": "flag",
                    "type": { "type": "boolean" },
                    "is_required": false
                }
            ],
            "required_fields": []
        }),
    );

    let result = tera
        .render("atproto/lexicon_object.tera", &ctx)
        .expect("lexicon_object.tera should render");

    eprintln!("Rendered edge cases:\n{}", result);
    assert!(result.contains("\"array\""));
    assert!(result.contains("\"items\""));
    assert!(result.contains("\"union\""));
    assert!(result.contains("\"refs\""));
    assert!(result.contains("\"unknown\""));
    assert!(result.contains("\"boolean\""));
}

// `#[rustfmt::skip]` preserves the pre-split body verbatim (rustfmt reflows
// the single-line `ctx.insert("record", ...)` at the shallower indent).
#[rustfmt::skip]
#[test]
fn lexicon_type_renders_string_formats() {
    let tera = load_tera();
    let mut ctx = tera::Context::new();

    ctx.insert(
        "lexicon",
        &serde_json::json!({
            "nsid": "app.test.formats",
            "lex_type": "object",
            "revision": 1,
            "description": "String format tests.",
            "domain": "test"
        }),
    );
    ctx.insert("record", &serde_json::json!({
        "name": "Formats",
        "description": null,
        "properties": [
            { "name": "dt",       "type": { "type": "datetime" },     "is_required": false },
            { "name": "uri",      "type": { "type": "uri" },          "is_required": false },
            { "name": "did",      "type": { "type": "string" },       "is_required": false },
            { "name": "handle",   "type": { "type": "string" },       "is_required": false },
            { "name": "nsid",     "type": { "type": "string" },       "is_required": false },
            { "name": "language", "type": { "type": "string" },       "is_required": false },
            { "name": "cid",      "type": { "type": "string" },       "is_required": false },
            { "name": "image_uri","type": { "type": "uri" },          "is_required": false },
            { "name": "plain",    "type": { "type": "string" },       "is_required": false }
        ],
        "required_fields": []
    }));

    let result = tera
        .render("atproto/lexicon_object.tera", &ctx)
        .expect("lexicon_object.tera should render");

    eprintln!("Rendered string formats:\n{}", result);
    assert!(result.contains("\"format\": \"datetime\""));
    assert!(result.contains("\"format\": \"uri\""));
    assert!(result.contains("\"type\": \"string\""));
}

#[test]
fn lexicon_record_renders_defs_as_additional_types() {
    let tera = load_tera();
    let mut ctx = tera::Context::new();

    ctx.insert(
        "lexicon",
        &serde_json::json!({
            "nsid": "app.test.post",
            "lex_type": "record",
            "key_strategy": "Tid",
            "revision": 1,
            "description": "Record with defs.",
            "domain": "test"
        }),
    );
    ctx.insert(
        "record",
        &serde_json::json!({
            "name": "Post",
            "description": "A post.",
            "properties": [
                {
                    "name": "text",
                    "type": { "type": "string" },
                    "is_required": true
                }
            ],
            "required_fields": ["text"]
        }),
    );
    ctx.insert(
        "defs",
        &serde_json::json!([
            {
                "name": "author",
                "description": "Post author info.",
                "properties": [
                    {
                        "name": "name",
                        "type": { "type": "string" },
                        "is_required": true
                    },
                    {
                        "name": "avatar",
                        "type": { "type": "uri" },
                        "is_required": false
                    }
                ],
                "required_fields": ["name"]
            }
        ]),
    );

    let result = tera
        .render("atproto/lexicon_record.tera", &ctx)
        .expect("lexicon_record.tera should render");

    eprintln!("Rendered record with defs:\n{}", result);
    assert!(result.contains("\"main\""));
    assert!(result.contains("\"author\""));
    assert!(result.contains("\"name\""));
    assert!(result.contains("\"avatar\""));
}

#[test]
fn lexicon_record_renders_key_strategy_variants() {
    let tera = load_tera();
    let mut ctx = tera::Context::new();

    for (strategy, expected) in &[
        ("Tid", "tid"),
        ("LiteralSelf", "#self"),
        ("Any", "*"),
        ("Nsid", "nsid"),
    ] {
        ctx.insert(
            "lexicon",
            &serde_json::json!({
                "nsid": "app.test.obj",
                "lex_type": "record",
                "key_strategy": strategy,
                "revision": 1,
                "description": format!("Key strategy: {}", strategy),
                "domain": "test"
            }),
        );
        ctx.insert(
            "record",
            &serde_json::json!({
                "name": "Obj",
                "description": "Object.",
                "properties": [],
                "required_fields": []
            }),
        );
        ctx.insert("defs", &serde_json::json!([]));

        let result = tera
            .render("atproto/lexicon_record.tera", &ctx)
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
    let tera = load_tera();
    let mut ctx = tera::Context::new();

    ctx.insert(
        "lexicon",
        &serde_json::json!({
            "nsid": "app.test.obj",
            "lex_type": "record",
            "key_strategy": "Tid",
            "description": "No revision specified.",
            "domain": "test"
        }),
    );
    ctx.insert(
        "record",
        &serde_json::json!({
            "name": "Obj",
            "description": null,
            "properties": [],
            "required_fields": []
        }),
    );
    ctx.insert("defs", &serde_json::json!([]));

    let result = tera
        .render("atproto/lexicon_record.tera", &ctx)
        .expect("Should render without revision set");

    assert!(
        result.contains("\"revision\": 1"),
        "Should default revision to 1"
    );
}
