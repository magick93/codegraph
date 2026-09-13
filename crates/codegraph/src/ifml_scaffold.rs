//! `ifml-scaffold`: emit a starter IFML DSL file (CRUD views + navigation)
//! from JSON Schemas and classifier config. This is the authoring-loop entry
//! point: domain objects surface as editable IFML views, which downstream
//! IFML tooling (ingest, route generation, LSP) then consume.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use codegraph_backend::{create_backend, BackendConfig};
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{PropertyNode, SchemaNode};
use codegraph_type_contracts::RefClassificationKind;

use crate::error::{Error, Result};
use crate::ifml_control_inference::{infer_control, is_codelist_kind, MAX_CONTROL_VALUES};

const MAX_LIST_FIELDS: usize = 5;
const MAX_VO_SUBFIELDS: usize = 8;

pub struct IfmlScaffoldArgs<'a> {
    pub schemas: &'a Path,
    pub classifier: &'a Path,
    pub config_path: &'a Path,
    pub output: &'a Path,
    pub force: bool,
    pub domains: &'a [String],
}

/// A form field rendered as `field <name> -> input <input> { ... };`.
#[derive(Debug, Clone, PartialEq)]
pub struct FormFieldSpec {
    pub name: String,
    pub input: &'static str,
    pub required: bool,
    pub values: Vec<String>,
}

/// The CRUD view triple generated for one entity.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityViewSpec {
    /// IFML entity name (schema title with the type suffix stripped).
    pub name: String,
    pub list_fields: Vec<String>,
    pub form_fields: Vec<FormFieldSpec>,
}

/// `domain "name" { schema "schema"; }` header.
#[derive(Debug, Clone, PartialEq)]
pub struct DomainHeader {
    pub name: String,
    pub schema: String,
}

/// Run the `ifml-scaffold` command: load + classify schemas, derive CRUD views
/// per entity, render the DSL, verify it parses, and write it to `output`.
pub async fn ifml_scaffold(args: IfmlScaffoldArgs<'_>) -> Result<()> {
    let IfmlScaffoldArgs {
        schemas,
        classifier,
        config_path,
        output,
        force,
        domains,
    } = args;

    let domain_config = codegraph_config::config::parse_domain_config(config_path)
        .map_err(|e| Error::Config(e.to_string()))?;
    let classifier_config = codegraph_classifier::config::parse_classifier_config(classifier)
        .map_err(|e| Error::Config(e.to_string()))?;

    let backend_config = BackendConfig::default();
    let be = create_backend(&backend_config)
        .await
        .map_err(|e| Error::Config(e.to_string()))?;

    let empty_entities = HashSet::new();
    let ingest_result = crate::ingest::async_ingest::ingest_schemas(
        be.ingestor(),
        schemas,
        &classifier_config,
        &empty_entities,
        &codegraph_config::UiOverrideConfig::default(),
        &domain_config.defaults.type_suffix,
    )
    .await?;
    println!(
        "Ingested {} schemas from {}",
        ingest_result.schemas_created,
        schemas.display()
    );

    let classifier_types: HashSet<String> = classifier_config
        .primitive_wrappers
        .keys()
        .cloned()
        .chain(classifier_config.array_wrappers.keys().cloned())
        .chain(classifier_config.range_wrappers.keys().cloned())
        .chain(
            classifier_config
                .composite_wrappers
                .iter()
                .map(|cw| cw.schema.clone()),
        )
        .collect();
    let all_data = be.querier().get_classification_data().await?;
    let auto_classifier = crate::classify::AutoClassifier::new(
        classifier_types,
        classifier_config.naming_rules.clone(),
    );

    let filter: HashSet<&str> = domains.iter().map(|s| s.as_str()).collect();
    let mut selected: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut all_entity_titles: HashSet<String> = HashSet::new();

    let mut sorted_domain_names: Vec<&String> = domain_config.domains.keys().collect();
    sorted_domain_names.sort();
    for domain_name in &sorted_domain_names {
        if !filter.is_empty() && !filter.contains(domain_name.as_str()) {
            continue;
        }
        let entry = &domain_config.domains[domain_name.as_str()];
        let domain_schemas: Vec<_> = all_data
            .iter()
            .filter(|d| d.domain.as_deref() == Some(domain_name.as_str()))
            .cloned()
            .collect();
        let result = auto_classifier.classify_domain(domain_name, entry, &domain_schemas);
        let titles: BTreeSet<String> = result
            .entities
            .iter()
            .map(|e| e.title.clone())
            .chain(entry.entities.iter().cloned())
            .collect();
        all_entity_titles.extend(titles.clone());
        selected.insert((*domain_name).clone(), titles);
    }

    if all_entity_titles.is_empty() {
        return Err(Error::Config(
            "no entities found to scaffold views for — check --domains config and --domain filter"
                .to_string(),
        ));
    }

    crate::ingest::async_ingest::reclassify_with_entities(
        be.ingestor(),
        be.querier(),
        &all_entity_titles,
    )
    .await?;

    let schemas_by_title: HashMap<String, SchemaNode> = be
        .querier()
        .list_schemas(None)
        .await?
        .into_iter()
        .map(|s| (s.title.clone(), s))
        .collect();

    let suffix = &domain_config.defaults.type_suffix;
    let mut headers = Vec::new();
    let mut specs: Vec<EntityViewSpec> = Vec::new();
    for (domain_name, titles) in &selected {
        let entry = &domain_config.domains[domain_name.as_str()];
        let schema_name = if entry.postgres_schema.is_empty() {
            domain_name.clone()
        } else {
            entry.postgres_schema.clone()
        };
        headers.push(DomainHeader {
            name: domain_name.clone(),
            schema: schema_name,
        });
        for title in titles {
            let schema = schemas_by_title
                .get(title.as_str())
                .or_else(|| schemas_by_title.get(&format!("{title}{suffix}")));
            let Some(schema) = schema else {
                eprintln!("  skipping '{title}': no schema loaded with that title");
                continue;
            };
            if schema.is_codelist {
                continue;
            }
            let props = be.querier().get_properties(&schema.title).await?;
            specs.push(EntityViewSpec {
                name: entity_name(&schema.title, suffix),
                list_fields: scalar_list_fields(&props),
                form_fields: build_form_fields(be.querier(), schema, &props).await?,
            });
        }
    }
    specs.sort_by(|a, b| a.name.cmp(&b.name));
    specs.dedup_by(|a, b| a.name == b.name);

    let content = render_ifml(&headers, &specs);

    if let Err(e) = codegraph_ifml_dsl::parse_ifml(&content) {
        return Err(Error::Config(format!(
            "internal error: generated IFML failed to parse: {e}"
        )));
    }

    if output.exists() && !force {
        return Err(Error::Config(format!(
            "output file '{}' already exists (use --force to overwrite)",
            output.display()
        )));
    }
    std::fs::write(output, &content)?;
    println!(
        "Scaffolded {} entities ({} views) -> {}",
        specs.len(),
        specs.len() * 3 + usize::from(!specs.is_empty()),
        output.display()
    );
    Ok(())
}

/// Strip the configured type suffix from a schema title (`TodoListType` →
/// `TodoList`); titles without the suffix pass through unchanged.
fn entity_name(title: &str, suffix: &str) -> String {
    if suffix.is_empty() {
        return title.to_string();
    }
    title.strip_suffix(suffix).unwrap_or(title).to_string()
}

/// Sanitize a name into a valid IFML identifier, or None when empty.
fn sanitize_identifier(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut out = cleaned;
    let first = out.chars().next()?;
    if first.is_ascii_digit() {
        out = format!("_{out}");
    }
    Some(out)
}

fn is_structured_kind(kind: Option<&RefClassificationKind>) -> bool {
    matches!(
        kind,
        Some(RefClassificationKind::ValueObject)
            | Some(RefClassificationKind::EntityReference)
            | Some(RefClassificationKind::CompositeWrapper)
            | Some(RefClassificationKind::StructuredWrapper)
            | Some(RefClassificationKind::RangeWrapper)
    )
}

/// Scalar properties eligible for list/detail `fields: [...]`, deduped,
/// capped at [`MAX_LIST_FIELDS`].
fn scalar_list_fields(props: &[PropertyNode]) -> Vec<String> {
    let mut seen = HashSet::new();
    props
        .iter()
        .filter(|p| p.name != "id" && !p.is_array)
        .filter(|p| {
            matches!(
                p.prop_type.as_str(),
                "string" | "integer" | "number" | "boolean"
            )
        })
        .filter(|p| !is_structured_kind(p.effective_kind().as_ref()))
        .filter_map(|p| sanitize_identifier(&p.name))
        .filter(|name| seen.insert(name.clone()))
        .take(MAX_LIST_FIELDS)
        .collect()
}

/// Build the form fields for one entity: one per scalar property, with
/// trivially-detectable ValueObject properties expanded into
/// `{property}_{sub}` fields on the owning entity's form.
async fn build_form_fields(
    querier: &dyn GraphQuerier,
    schema: &SchemaNode,
    props: &[PropertyNode],
) -> Result<Vec<FormFieldSpec>> {
    let mut fields = Vec::new();
    let mut seen = HashSet::new();
    for prop in props {
        if prop.name == "id" || !seen.insert(prop.name.clone()) {
            continue;
        }
        if prop.effective_kind().as_ref() == Some(&RefClassificationKind::ValueObject) {
            let expanded = expand_vo_fields(querier, schema, prop).await?;
            if !expanded.is_empty() {
                fields.extend(expanded);
                continue;
            }
        }
        let Some(name) = sanitize_identifier(&prop.name) else {
            continue;
        };
        let inference =
            infer_control(prop).with_values(codelist_values(querier, schema, prop).await?);
        fields.push(FormFieldSpec {
            name,
            input: inference.input_str(),
            required: inference.required,
            values: inference.values,
        });
    }
    Ok(fields)
}

/// Expand a ValueObject-typed property into flattened form fields from the
/// referenced schema. Returns an empty Vec when the target cannot be
/// trivially resolved (the caller then falls back to a plain field).
async fn expand_vo_fields(
    querier: &dyn GraphQuerier,
    schema: &SchemaNode,
    prop: &PropertyNode,
) -> Result<Vec<FormFieldSpec>> {
    let Some(target) = querier
        .get_property_ref_target(&prop.name, &schema.title)
        .await?
    else {
        return Ok(Vec::new());
    };
    let sub_props = querier.get_properties(&target.title).await?;
    let mut out = Vec::new();
    for sub in &sub_props {
        if out.len() >= MAX_VO_SUBFIELDS {
            break;
        }
        if sub.is_array || is_structured_kind(sub.effective_kind().as_ref()) {
            continue;
        }
        if !matches!(
            sub.prop_type.as_str(),
            "string" | "integer" | "number" | "boolean"
        ) {
            continue;
        }
        let Some(name) = sanitize_identifier(&format!("{}_{}", prop.name, sub.name)) else {
            continue;
        };
        out.push(FormFieldSpec {
            name,
            input: infer_control(sub).input_str(),
            required: prop.is_required && sub.is_required,
            values: Vec::new(),
        });
    }
    Ok(out)
}

/// Resolve dropdown values for codelist-classified properties. Values are
/// only emitted when the property has a UsesCodeList edge whose codelist has
/// ingested enum values; otherwise the dropdown stays plain.
async fn codelist_values(
    querier: &dyn GraphQuerier,
    schema: &SchemaNode,
    prop: &PropertyNode,
) -> Result<Vec<String>> {
    if !is_codelist_kind(prop.effective_kind().as_ref()) {
        return Ok(Vec::new());
    }
    let Some((codelist, _)) = querier
        .get_codelist_for_property(&prop.name, &schema.title)
        .await?
    else {
        return Ok(Vec::new());
    };
    let values = querier.get_enum_values(&codelist.name).await?;
    Ok(values
        .into_iter()
        .take(MAX_CONTROL_VALUES)
        .map(|v| v.value)
        .collect())
}

pub fn render_ifml(domains: &[DomainHeader], entities: &[EntityViewSpec]) -> String {
    let mut out = String::new();
    for d in domains {
        out.push_str(&format!(
            "domain \"{}\" {{\n    schema \"{}\";\n}}\n\n",
            d.name, d.schema
        ));
    }
    for e in entities {
        render_entity_views(&mut out, e);
    }
    if !entities.is_empty() {
        render_home(&mut out, entities);
    }
    out
}

fn render_entity_views(out: &mut String, e: &EntityViewSpec) {
    out.push_str(&format!("view \"{}List\" {{\n", e.name));
    out.push_str(&format!("    label \"{} Management\";\n\n", e.name));
    out.push_str("    component \"grid\" {\n");
    out.push_str("        type: list;\n");
    out.push_str(&format!("        data: {};\n", e.name));
    if !e.list_fields.is_empty() {
        out.push_str(&format!(
            "        fields: [{}];\n",
            e.list_fields.join(", ")
        ));
    }
    out.push_str(&format!(
        "\n        on select(row) -> navigate(\"{}Detail\", {{ id: row.id }});\n",
        e.name
    ));
    out.push_str("    }\n}\n\n");

    out.push_str(&format!("view \"{}Detail\" {{\n", e.name));
    out.push_str("    params { id: Uuid };\n");
    out.push_str(&format!("    label \"{} Details\";\n\n", e.name));
    out.push_str("    component \"info\" {\n");
    out.push_str("        type: details;\n");
    out.push_str(&format!("        data: {};\n", e.name));
    if !e.list_fields.is_empty() {
        out.push_str(&format!(
            "        fields: [{}];\n",
            e.list_fields.join(", ")
        ));
    }
    out.push_str(&format!(
        "\n        on back -> navigate(\"{}List\");\n",
        e.name
    ));
    out.push_str("    }\n}\n\n");

    out.push_str(&format!("view \"{}Form\" {{\n", e.name));
    out.push_str(&format!("    label \"Edit {}\";\n\n", e.name));
    out.push_str("    component \"form\" {\n");
    out.push_str("        type: form;\n");
    out.push_str(&format!("        data: {};\n\n", e.name));
    for f in &e.form_fields {
        out.push_str(&render_form_field(f));
    }
    out.push_str(&format!(
        "\n        on save -> navigate(\"{}List\");\n",
        e.name
    ));
    out.push_str(&format!(
        "        on cancel -> navigate(\"{}List\");\n",
        e.name
    ));
    out.push_str("    }\n}\n\n");
}

fn render_form_field(f: &FormFieldSpec) -> String {
    let mut extras: Vec<String> = Vec::new();
    if f.required {
        extras.push("required: true;".to_string());
    }
    if !f.values.is_empty() {
        let quoted: Vec<String> = f
            .values
            .iter()
            .map(|v| format!("\"{}\"", v.replace('"', "\\\"")))
            .collect();
        extras.push(format!("values: [{}];", quoted.join(", ")));
    }
    if extras.is_empty() {
        format!("        field {} -> input {};\n", f.name, f.input)
    } else {
        format!(
            "        field {} -> input {} {{ {} }}\n",
            f.name,
            f.input,
            extras.join(" ")
        )
    }
}

fn render_home(out: &mut String, entities: &[EntityViewSpec]) {
    out.push_str("view \"Home\" {\n");
    out.push_str("    label \"Home\";\n");
    out.push_str("    landmark: true;\n\n");
    for e in entities {
        out.push_str(&format!("    on click -> navigate(\"{}List\");\n", e.name));
    }
    out.push_str("}\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, fields: &[&str]) -> EntityViewSpec {
        EntityViewSpec {
            name: name.to_string(),
            list_fields: fields.iter().map(|s| s.to_string()).collect(),
            form_fields: fields
                .iter()
                .map(|s| FormFieldSpec {
                    name: s.to_string(),
                    input: "text",
                    required: false,
                    values: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn render_output_parses_with_real_parser() {
        let domains = vec![DomainHeader {
            name: "todo".to_string(),
            schema: "todo".to_string(),
        }];
        let entities = vec![spec("TodoList", &["name", "description"])];
        let content = render_ifml(&domains, &entities);
        let model = codegraph_ifml_dsl::parse_ifml(&content).expect("scaffold output must parse");
        assert_eq!(model.domains.len(), 1);
        assert_eq!(model.domains[0].name, "todo");
        assert_eq!(model.domains[0].schema_name, "todo");
        let names: Vec<&str> = model.views.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["TodoListList", "TodoListDetail", "TodoListForm", "Home"]
        );
    }

    #[test]
    fn dropdown_values_render_as_quoted_list() {
        let mut e = spec("Product", &["status"]);
        e.form_fields[0] = FormFieldSpec {
            name: "status".to_string(),
            input: "dropdown",
            required: true,
            values: vec!["gold".to_string(), "silver".to_string()],
        };
        let content = render_ifml(&[], &[e]);
        assert!(
            content.contains("field status -> input dropdown { required: true; values: [\"gold\", \"silver\"]; }"),
            "unexpected rendering:\n{content}"
        );
        codegraph_ifml_dsl::parse_ifml(&content).expect("must parse");
    }

    #[test]
    fn entity_name_strips_suffix() {
        assert_eq!(entity_name("TodoListType", "Type"), "TodoList");
        assert_eq!(entity_name("TodoList", "Type"), "TodoList");
        assert_eq!(entity_name("TodoListType", ""), "TodoListType");
    }

    #[test]
    fn sanitize_identifier_replaces_invalid_chars() {
        assert_eq!(
            sanitize_identifier("due-date"),
            Some("due_date".to_string())
        );
        assert_eq!(sanitize_identifier("9lives"), Some("_9lives".to_string()));
        assert_eq!(sanitize_identifier(""), None);
    }
}
