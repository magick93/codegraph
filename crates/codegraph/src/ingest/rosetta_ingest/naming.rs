//! Naming/reference helpers and the function/rule node builders for the
//! rosetta bridge (split out of the `rosetta_ingest` module, #371).

use std::collections::{HashMap, HashSet};

use sigil_model::SemanticElement;

use codegraph_core::types::{
    FunctionAlias, FunctionDispatch, FunctionInput, FunctionNode, FunctionOperation,
    FunctionPostCondition, FunctionTransform, FunctionTransformKind, RuleKind, RuleNode,
};

use super::ROSETTA_ORIGIN;

/// Walk the `extends` chains of one run's functions; `Some(described
/// cycle)` when a cycle exists. The walk is deterministic (caller sorted
/// by (domain, name)); the described cycle names every member.
pub(super) fn detect_function_extends_cycle(
    func_index: &[(&sigil_model::Function, String)],
) -> Option<String> {
    let parent_of: HashMap<&str, String> = func_index
        .iter()
        .filter_map(|(f, _)| {
            f.super_function
                .as_ref()
                .map(|parent| (f.name.as_str(), referenced_title(parent)))
        })
        .collect();
    for (start, _) in func_index {
        let mut path: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut current: String = start.name.clone();
        while let Some(parent) = parent_of.get(current.as_str()).cloned() {
            if seen.insert(current.clone()) {
                path.push(current.clone());
                current = parent;
            } else {
                // Re-entered a node already on this walk: the cycle is the
                // path suffix from its first occurrence.
                let start_idx = path
                    .iter()
                    .position(|name| name == &current)
                    .unwrap_or_default();
                let mut cycle = path[start_idx..].join(" -> ");
                cycle.push_str(" -> ");
                cycle.push_str(&current);
                return Some(cycle);
            }
        }
    }
    None
}

/// One sigil `Attribute` (function input/output) → the typed
/// [`FunctionInput`]: bare type title plus cardinality collapsed to the
/// two flags codegen needs.
fn attribute_to_function_input(attribute: &sigil_model::Attribute) -> FunctionInput {
    let is_array = match attribute.cardinality.max {
        sigil_model::CardinalityMax::Unbounded => true,
        sigil_model::CardinalityMax::Finite(max) => max > 1,
    };
    FunctionInput {
        name: attribute.name.clone(),
        type_ref: referenced_title(&attribute.type_ref),
        is_array,
        is_optional: attribute.cardinality.min == 0,
    }
}

/// One sigil `Function` → a [`FunctionNode`]. Every expression (alias,
/// operation, non-post condition, post-condition) persists as the
/// canonical `Expr::to_json()` payload — the generation-time transpiler's
/// input (issue #262/#263 embedding contract). `pub(crate)` so the
/// doctor's function-lifecycle checks reuse the exact bridge conversion.
pub(crate) fn function_node(function: &sigil_model::Function, domain: &str) -> FunctionNode {
    let inputs: Vec<FunctionInput> = function
        .inputs
        .iter()
        .map(attribute_to_function_input)
        .collect();
    let output = function.output.as_ref().map(attribute_to_function_input);
    let aliases: Vec<FunctionAlias> = function
        .shortcuts
        .iter()
        .map(|shortcut| FunctionAlias {
            name: shortcut.name.clone(),
            expr_json: serde_json::to_string(&shortcut.expression.to_json()).unwrap_or_default(),
        })
        .collect();
    let operations: Vec<FunctionOperation> = function
        .operations
        .iter()
        .map(|operation| FunctionOperation {
            is_add: operation.add,
            assign_root: operation.assign_root.clone(),
            path: operation
                .path
                .iter()
                .map(|segment| segment.feature.clone())
                .collect(),
            expr_json: serde_json::to_string(&operation.expression.to_json()).unwrap_or_default(),
        })
        .collect();
    let post_conditions: Vec<FunctionPostCondition> = function
        .post_conditions
        .iter()
        .enumerate()
        .map(|(idx, condition)| FunctionPostCondition {
            name: condition
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_post_{}", function.name, idx)),
            definition: condition.definition.clone(),
            expr_json: serde_json::to_string(&condition.expression.to_json()).unwrap_or_default(),
        })
        .collect();
    let transform_annotations: Vec<FunctionTransform> = function
        .transform
        .iter()
        .map(|transform| FunctionTransform {
            kind: match transform.kind {
                sigil_model::TransformKind::Ingest => FunctionTransformKind::Ingest,
                sigil_model::TransformKind::Enrich => FunctionTransformKind::Enrich,
                sigil_model::TransformKind::Projection => FunctionTransformKind::Projection,
            },
            reference: transform.reference.clone(),
        })
        .collect();

    // Open-ended metadata: origin provenance (ROSETTA_ORIGIN convention),
    // annotation refs, doc references, and the non-post conditions (they
    // cannot see the output; a structured sibling field is deferred until
    // a consumer needs them).
    let mut properties = serde_json::json!({
        "origin": ROSETTA_ORIGIN,
        "doc_references": function.doc_references.iter().map(|doc| {
            serde_json::json!({
                "body": referenced_title_str(&doc.body),
                "corpora": doc.corpora.iter().map(|c| referenced_title_str(c))
                    .collect::<Vec<_>>(),
                "segments": doc.segments.iter().map(|(segment, reference)| {
                    serde_json::json!({ "segment": referenced_title_str(segment),
                                        "reference": reference })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    });
    if !function.annotations.is_empty() {
        if let Ok(annotations) = serde_json::to_value(&function.annotations) {
            properties["annotations"] = annotations;
        }
    }
    if !function.conditions.is_empty() {
        let conditions: Vec<serde_json::Value> = function
            .conditions
            .iter()
            .enumerate()
            .map(|(idx, condition)| {
                serde_json::json!({
                    "name": condition.name.clone()
                        .unwrap_or_else(|| format!("{}_cond_{}", function.name, idx)),
                    "definition": condition.definition,
                    "expr_json": serde_json::to_string(&condition.expression.to_json())
                        .unwrap_or_default(),
                })
            })
            .collect();
        properties["conditions"] = serde_json::Value::Array(conditions);
    }

    FunctionNode {
        name: function.name.clone(),
        domain: Some(domain.to_string()),
        definition: function.definition.clone(),
        dispatch: function.dispatch.as_ref().map(|dispatch| FunctionDispatch {
            attribute: dispatch.attribute.clone(),
            enumeration: referenced_title_str(&dispatch.enumeration),
            value: dispatch.value.clone(),
        }),
        inputs,
        output,
        aliases,
        operations,
        post_conditions,
        extends: function.super_function.as_ref().map(referenced_title),
        transform_annotations,
        properties,
    }
}

/// Namespace of the file declaring `element_name` (lookup by bridged
/// element name; empty when not found — namespace recording is best-effort
/// until #268 gives namespaces nodes).
/// One sigil `Rule` → a [`RuleNode`] (issue #264). The `eligibility` bool
/// picks the kind; the `from TypeCall` input persists as its bare title;
/// the body persists as the canonical `Expr::to_json()` payload. Doc
/// references ride the properties payload AND emit `RegulatoryReference`
/// edges (owner `RegulatoryOwner::Rule`).
pub(super) fn rule_node(rule: &sigil_model::Rule, domain: &str) -> RuleNode {
    let properties = serde_json::json!({
        "origin": ROSETTA_ORIGIN,
        "doc_references": rule.doc_references.iter().map(|doc| {
            serde_json::json!({
                "body": referenced_title_str(&doc.body),
                "corpora": doc.corpora.iter().map(|c| referenced_title_str(c))
                    .collect::<Vec<_>>(),
                "segments": doc.segments.iter().map(|(segment, reference)| {
                    serde_json::json!({ "segment": referenced_title_str(segment),
                                        "reference": reference })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    });
    RuleNode {
        name: rule.name.clone(),
        domain: Some(domain.to_string()),
        definition: rule.definition.clone(),
        kind: if rule.eligibility {
            RuleKind::Eligibility
        } else {
            RuleKind::Reporting
        },
        input_type: rule.input.as_ref().map(referenced_title),
        expr_json: serde_json::to_string(&rule.expression.to_json()).unwrap_or_default(),
        properties,
    }
}

/// Namespace of the file declaring `element_name` (lookup by bridged
/// element name; empty when not found — namespace recording is best-effort
/// until #268 gives namespaces nodes).
pub(super) fn model_namespace<'a>(
    user_files: &'a [sigil_model::ModelFile],
    element_name: &str,
) -> &'a str {
    user_files
        .iter()
        .find(|model| {
            model
                .elements
                .iter()
                .any(|element| element.name() == element_name)
        })
        .map(|model| model.namespace.as_str())
        .unwrap_or("")
}

/// The element-universe dedup key. Reports are anonymous in sigil
/// (`SemanticElement::name()` returns `""`), so they key on their
/// synthesized name — structurally identical reports dedup, distinct ones
/// all land.
pub(super) fn element_dedup_key(element: &SemanticElement, name: &str) -> String {
    match element {
        SemanticElement::Report(report) => synthesize_report_name(report),
        _ => name.to_string(),
    }
}

/// A deterministic identity for an anonymous report: its regulatory
/// reference plus timing, e.g. `Report CDRBody ESMA Section1 "1.a" (T+1)`.
/// Sigil renders reports nameless in canonical JSON too (there is no
/// `RosettaReport.name`); without a synthesis, one graph key could not
/// distinguish a workspace's reports.
pub(super) fn synthesize_report_name(report: &sigil_model::Report) -> String {
    let mut name = format!("Report {}", named_ref_title(&report.regulatory.body));
    for corpus in &report.regulatory.corpora {
        name.push(' ');
        name.push_str(&named_ref_title(corpus));
    }
    for segment in &report.regulatory.segments {
        name.push_str(&format!(
            " {} \"{}\"",
            named_ref_title(&segment.segment),
            segment.reference
        ));
    }
    name.push_str(&format!(" ({})", report.timing.as_str()));
    name
}

/// Bare title of a `NamedRef` (the report's reference parts may be written
/// namespace-qualified; titles are bare, matching `referenced_title`).
pub(super) fn named_ref_title(named_ref: &sigil_model::NamedRef) -> String {
    referenced_title_str(&named_ref.name)
}

/// Bare last segment of a dotted reference name.
pub(super) fn referenced_title_str(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_string()
}

pub(super) fn element_kind_label(element: &SemanticElement) -> &'static str {
    match element {
        SemanticElement::Data(_) => "type",
        SemanticElement::Enumeration(_) => "enum",
        SemanticElement::Annotation(_) => "annotation-decl",
        SemanticElement::TypeAlias(_) => "type-alias",
        SemanticElement::BasicType(_) => "basic-type",
        SemanticElement::RecordType(_) => "record-type",
        SemanticElement::LibraryFunction(_) => "library-function",
        SemanticElement::Function(_) => "func",
        SemanticElement::Rule(_) => "rule",
        SemanticElement::Report(_) => "report",
        SemanticElement::ExternalRuleSource(_) => "external-rule-source",
        SemanticElement::Schema(_) => "schema",
        SemanticElement::Body(_) => "body",
        SemanticElement::Corpus(_) => "corpus",
        SemanticElement::Segment(_) => "segment",
        SemanticElement::MetaType(_) => "meta-type",
    }
}

/// `<namespace>/<Name>` — the mox bridge's `package/class` convention.
pub(super) fn schema_id(namespace: &str, name: &str) -> String {
    format!("{namespace}/{name}")
}

/// Bare title of a reference: the last segment of the written name (v1 —
/// Rosetta references are namespace-qualified but titles are bare; the
/// resolution pass already rejected unknown names).
pub(super) fn referenced_title(type_ref: &sigil_model::TypeRef) -> String {
    type_ref
        .name
        .rsplit('.')
        .next()
        .unwrap_or(&type_ref.name)
        .to_string()
}

pub(super) fn collect_universes(
    model: &sigil_model::ModelFile,
    types: &mut HashSet<String>,
    enums: &mut HashSet<String>,
) {
    for element in &model.elements {
        match element {
            SemanticElement::Data(data) => {
                types.insert(data.name.clone());
            }
            SemanticElement::Enumeration(enumeration) => {
                enums.insert(enumeration.name.clone());
            }
            _ => {}
        }
    }
}

/// Enum values with `extends` parents merged first (root ancestor first),
/// each level in declaration order; first occurrence of a value name wins.
pub(super) fn merged_enum_values<'a>(
    enumeration: &'a sigil_model::Enumeration,
    enum_by_name: &HashMap<&str, &'a sigil_model::Enumeration>,
) -> Vec<&'a sigil_model::EnumValue> {
    let mut chain = Vec::new();
    let mut current = Some(enumeration);
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(element) = current {
        if !visited.insert(element.name.clone()) {
            break;
        }
        chain.push(element);
        current = element
            .super_type
            .as_ref()
            .map(referenced_title)
            .and_then(|name| enum_by_name.get(name.as_str()).copied());
    }
    let mut ordered = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for element in chain.into_iter().rev() {
        for value in &element.values {
            if seen.insert(value.name.clone()) {
                ordered.push(value);
            }
        }
    }
    ordered
}
