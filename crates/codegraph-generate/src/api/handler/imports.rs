//! Handler import resolution (the type registry), split out of the #751
//! size-budget `api/handler.rs`.

use crate::type_registry::TypeRegistryHandle;

use crate::api::include_path::ResolvedIncludePath;

pub(super) fn build_handler_imports(
    type_registry: &TypeRegistryHandle,
    operations: &[String],
    entity_name: &str,
    has_include: bool,
    include_paths: &[ResolvedIncludePath],
    domain: &str,
    module_name: &str,
) -> Vec<String> {
    let mut handler_refs: Vec<String> = vec![
        "AppState".into(),
        "AppError".into(),
        "ApiKeyInfo".into(),
        format!("{}Response", entity_name),
        format!("{}LinkedResponse", entity_name),
    ];
    if operations.contains(&"create".to_string()) {
        handler_refs.push("BulkItemError".into());
    }
    if operations.contains(&"create".to_string()) {
        handler_refs.push(format!("Create{}Request", entity_name));
    }
    if operations.contains(&"update".to_string()) {
        handler_refs.push(format!("Update{}Request", entity_name));
    }
    if has_include {
        handler_refs.push(format!("{}WithIncludeResponse", entity_name));
        handler_refs.push(format!("{}IncludedData", entity_name));
        // Add combined response types for dot-notation paths and VO response
        // types for child_table_override paths — the handler's match arms
        // reference these types directly in merge patterns.
        for path in include_paths {
            if path.segments.len() > 1 {
                handler_refs.push(format!("{}CombinedResponse", path.segments[0].entity_name));
            }
            // The override response type is only referenced textually in the
            // dot-path merge arm (handler.tera), so only register it for
            // multi-segment paths; single-segment includes use inference.
            if path.segments.len() > 1
                && let Some(over) = path
                    .segments
                    .first()
                    .and_then(|s| s.child_table_override.as_ref())
            {
                handler_refs.push(over.response_type.clone());
            }
        }
    }
    // Note: Create{entity_name}Body and {entity_name}BulkCreateResponse are
    // defined inline by handler.tera and must NOT be registered in the type
    // registry or added to handler_refs. Doing so causes E0432/E0255 when
    // multiple domains share an entity name (e.g. "Order" in screening AND
    // assessments): the second handler's registration silently fails (name
    // collision), resolve_imports generates wrong cross-module imports for
    // types that should be local, producing duplicate definitions and
    // non-existent type references in the generated code.
    let handler_caller: Vec<String> = vec![
        "crate".into(),
        "api".into(),
        domain.to_string(),
        format!("{}_handler", module_name),
    ];
    type_registry.resolve_imports(&handler_refs, &handler_caller)
}
