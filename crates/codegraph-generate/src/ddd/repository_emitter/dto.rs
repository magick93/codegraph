use crate::code_writer::{CodeWriter, wln};

use super::child::emit_child_reads;
use super::junction::{emit_junction_field_population, emit_junction_reads};
use super::{ChildTableInfo, EntityTree, TreeColumn};

/// Emit the mapping expression for a single entity column → DTO field.
///
/// Handles codelist enum parsing, JSONB deserialization, plain copy, and —
/// when the entity column's rust type differs from the DTO field type
/// (issue #459) — the uuid↔String adaptation the include-fetch emitter
/// spells (`.to_string()` / `.parse()`), nullable-aware.
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
    } else if col.is_array && col.dto_rust_type.is_some() {
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
    } else {
        // Scalar column: adapt when the entity column's rust type differs
        // from the DTO field type (issue #459 — e.g. a Uuid FK column
        // hydrating a TEXT-mapped String DTO field), mirroring the
        // include-fetch emitter's `emit_field_assignments_typed`. Matching
        // types keep the plain assignment (no adaptation fires).
        let col_ty = Some(col.rust_type.as_str());
        let dto_ty = col.dto_rust_type.as_deref().or(col_ty);
        let nullable = col.is_nullable;
        match (col_ty, dto_ty) {
            (Some("Uuid"), Some("String")) if nullable => wln!(
                code,
                "{pad}{dto_field}: {row_var}.{entity_field}.map(|v| v.to_string()),",
            ),
            (Some("Uuid"), Some("String")) => wln!(
                code,
                "{pad}{dto_field}: {row_var}.{entity_field}.to_string(),",
            ),
            (Some("String"), Some(other)) if other != "String" && nullable => wln!(
                code,
                "{pad}{dto_field}: {row_var}.{entity_field}.and_then(|v| v.parse().ok()),",
            ),
            (Some("String"), Some(other)) if other != "String" => wln!(
                code,
                "{pad}{dto_field}: {row_var}.{entity_field}.parse().unwrap_or_default(),",
            ),
            _ => wln!(code, "{pad}{dto_field}: {row_var}.{entity_field},"),
        }
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
    emit_response_construction_for(code, tree, "id", None);
}

/// Same, with the parent id expression named: `find_by_id` scopes child
/// reads on the `id` parameter, while design finders scope them on
/// `row.id` (issue #449).
pub(crate) fn emit_response_construction_for(
    code: &mut CodeWriter,
    tree: &EntityTree,
    row_id_expr: &str,
    projection: Option<&str>,
) {
    emit_child_reads(code, &tree.child_tables, row_id_expr, 2);
    emit_junction_reads(code, &tree.junction_tables, row_id_expr, 2);
    wln!(code);
    // A projection return builds the response as a value and maps it through
    // the boundary's `from_row` (issue #768); the entity path returns the
    // `Ok(Some({Entity}Response))` literal — byte-identical to pre-#768.
    match projection {
        Some(_) => wln!(
            code,
            "        let response = Some({}Response {{",
            tree.entity_name
        ),
        None => wln!(code, "        Ok(Some({}Response {{", tree.entity_name),
    }
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
    if !tree.append_only {
        wln!(code, "            updated_at: row.updated_at,");
    }
    // DTO fields the tree does not load (e.g. base-inherited junction arrays
    // under nested composition nodes) default to None instead of failing E0063.
    wln!(code, "            ..Default::default()");
    match projection {
        Some(name) => {
            // The projection path builds the response as a value, then maps:
            // `let response = Some({..}); Ok(response.map(P::from_row))`.
            wln!(code, "        }});");
            wln!(code, "        Ok(response.map(|r| {name}::from_row(&r)))");
        }
        None => {
            // The original single line: struct close + `Ok(Some(..))` close.
            wln!(code, "        }}))");
        }
    }
    wln!(code, "    }}");
}
