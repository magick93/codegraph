use crate::generate::code_writer::{wln, CodeWriter};

use super::child::emit_child_reads;
use super::junction::{emit_junction_field_population, emit_junction_reads};
use super::{ChildTableInfo, EntityTree, TreeColumn};

/// Emit the mapping expression for a single entity column → DTO field.
///
/// Handles codelist enum parsing, JSONB deserialization, and plain copy.
/// `pad` is the indentation prefix (e.g. `"            "`).
/// `row_var` is the variable name holding the entity row (e.g. `"row"`).
pub(crate) fn emit_entity_to_dto_field(
    code: &mut CodeWriter,
    col: &TreeColumn,
    row_var: &str,
    pad: &str,
) {
    let dto_field = col.dto_name();
    let entity_field = &col.field_name;
    // StructuredWrapper must take priority over dto_rust_type — it uses
    // serde_json::from_value() rather than .parse().
    if col.is_structured_wrapper {
        // Both array and scalar StructuredWrapper use the same serde_json conversion.
        if col.is_nullable {
            wln!(
                code,
                "{pad}{dto_field}: {row_var}.{entity_field}.and_then(|v| serde_json::from_value(v).ok()),",
            );
        } else {
            wln!(
                code,
                "{pad}{dto_field}: serde_json::from_value({row_var}.{entity_field}).unwrap_or_default(),",
            );
        }
    } else if col.dto_rust_type.is_some() {
        if col.is_array {
            if col.is_nullable {
                wln!(
                    code,
                    "{pad}{dto_field}: {row_var}.{entity_field}.map(|v| v.into_iter().filter_map(|x| x.parse().ok()).collect()),",
                );
            } else {
                wln!(
                    code,
                    "{pad}{dto_field}: {row_var}.{entity_field}.into_iter().filter_map(|v| v.parse().ok()).collect(),",
                );
            }
        } else if col.is_nullable {
            wln!(
                code,
                "{pad}{dto_field}: {row_var}.{entity_field}.and_then(|v| v.parse().ok()),",
            );
        } else {
            wln!(
                code,
                "{pad}{dto_field}: {row_var}.{entity_field}.parse().unwrap_or_default(),",
            );
        }
    } else {
        wln!(code, "{pad}{dto_field}: {row_var}.{entity_field},");
    }
}

/// Emit child table field assignments into a DTO struct literal.
///
/// For array children: `field: field_rows,`
/// For single children: `field: field_rows.into_iter().next(),`
pub(crate) fn emit_child_field_population(
    code: &mut CodeWriter,
    children: &[ChildTableInfo],
    pad: &str,
) {
    for child in children {
        if child.is_array {
            wln!(
                code,
                "{pad}{field}: {field}_rows,",
                field = child.field_name
            );
        } else {
            wln!(
                code,
                "{pad}{field}: {field}_rows.into_iter().next(),",
                field = child.field_name
            );
        }
    }
}

/// Emit the response struct construction shared by `find_by_id` and `find_by_id_scoped`.
pub(crate) fn emit_response_construction(code: &mut CodeWriter, tree: &EntityTree) {
    emit_child_reads(code, &tree.child_tables, "id", 2);
    emit_junction_reads(code, &tree.junction_tables, "id", 2);
    wln!(code);
    wln!(code, "        Ok(Some({}Response {{", tree.entity_name);
    wln!(code, "            id: row.id,");
    for col in &tree.direct_columns {
        if col.is_composite_range {
            continue;
        }
        // Schema-defined created_at/updated_at are emitted by the trailing
        // standard fields below — avoid duplicate struct fields.
        if col.field_name == "created_at" || col.field_name == "updated_at" {
            continue;
        }
        emit_entity_to_dto_field(code, col, "row", "            ");
    }
    emit_child_field_population(code, &tree.child_tables, "            ");
    emit_junction_field_population(code, &tree.junction_tables, "            ");
    if tree.has_workflow {
        wln!(code, "            workflow_state: None,");
    }
    wln!(code, "            created_at: row.created_at,");
    wln!(code, "            updated_at: row.updated_at,");
    // DTO fields the tree does not load (e.g. base-inherited junction arrays
    // under nested composition nodes) default to None instead of failing E0063.
    wln!(code, "            ..Default::default()");
    wln!(code, "        }}))");
    wln!(code, "    }}");
}
