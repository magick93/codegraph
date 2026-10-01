// Test tree for the atproto generators, one module per generator/concern
// under test: `templates` (lexicon/scaffold template rendering),
// `lexicon` (LexiconEmitter + LexiconScaffoldEmitter), `client`
// (AtprotoClientEmitter + AtprotoClientScaffoldEmitter), `types`
// (AtprotoTypesEmitter). `scaffold_template_renders_shared_defs` rides with
// `templates` — it shares that module's `load_tera()` fixture.
//
// The schema/property/project builders below were byte-identical copies in
// the pre-split `tests.rs` modules; they are deduplicated here (`pub(super)`)
// so every test body stays byte-verbatim.

mod client;
mod lexicon;
mod templates;
mod types;

use codegraph_core::types::{PropertyNode, SchemaNode};
use codegraph_type_contracts::RefClassificationKind;

use crate::project_config::AtprotoConfig;
use crate::ProjectConfig;

pub(super) fn make_domain_config() -> codegraph_config::DomainConfig {
    let domains = std::collections::HashMap::new();
    codegraph_config::DomainConfig {
        namespaces: std::collections::HashMap::new(),
        defaults: Default::default(),
        domains,
        rbac: None,
    }
}

pub(super) fn make_project() -> ProjectConfig {
    ProjectConfig {
        atproto: AtprotoConfig {
            atproto_authority: "nz.gravy".to_string(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub(super) fn make_schema(title: &str, domain: &str) -> SchemaNode {
    SchemaNode {
        namespace: None,
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
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

pub(super) fn make_primitive_prop(name: &str, prop_type: &str, is_required: bool) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: prop_type.to_string(),
        description: Some(format!("The {} field", name)),
        format: None,
        is_required,
        is_nullable: false,
        is_array: false,
        min_items: None,
        max_items: None,
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
        type_expr: None,
    }
}
