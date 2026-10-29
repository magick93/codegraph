//! Function-binding API plumbing shared by the functions generator, the
//! router context, the OpenAPI generator, and the scaffold (issues
//! #450 B0 + #337 B1). One source of truth for which functions are
//! HTTP-exposed and which domains carry manual providers, so the
//! consumers cannot drift.

use codegraph_config::{DomainConfig, FunctionExposure, FunctionImplementation};
use codegraph_core::traits::GraphQuerier;
use codegraph_naming::{escape_rust_keyword, strip_suffix, to_pascal_case, to_snake_case};

use crate::error::Result;

/// One modeled function with its resolved binding (defaults applied).
#[derive(Debug, Clone)]
pub struct BoundFunction {
    pub node: codegraph_core::types::FunctionNode,
    pub implementation: FunctionImplementation,
    pub exposure: FunctionExposure,
}

impl BoundFunction {
    pub fn is_http(&self) -> bool {
        self.exposure == FunctionExposure::Http
    }

    pub fn is_manual(&self) -> bool {
        self.implementation == FunctionImplementation::Manual
    }
}

/// All functions in the graph with their effective bindings, in the
/// deterministic `(domain, name)` order of `list_functions`.
pub async fn bound_functions(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<BoundFunction>> {
    let mut out = Vec::new();
    for node in db.list_functions().await? {
        let (implementation, exposure) = match config.functions.get(&node.name) {
            Some(binding) => (binding.implementation, binding.exposure),
            None => (FunctionImplementation::Dsl, FunctionExposure::Internal),
        };
        out.push(BoundFunction {
            node,
            implementation,
            exposure,
        });
    }
    Ok(out)
}

/// The Rust function name for a modeled function (the functions-module
/// convention).
pub fn fn_snake(modeled_name: &str) -> String {
    escape_rust_keyword(&to_snake_case(modeled_name))
}

/// The `{Fn}Request` struct name for a modeled function.
pub fn request_struct(modeled_name: &str) -> String {
    format!("{}Request", to_pascal_case(modeled_name))
}

/// The `{Fn}Response` struct name for a modeled function.
pub fn response_struct(modeled_name: &str) -> String {
    format!("{}Response", to_pascal_case(modeled_name))
}

/// Domains carrying at least one HTTP-exposed function, sorted — the
/// router's `.merge(functions_routes())` gate.
pub async fn domains_with_http(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>> {
    let mut domains: Vec<String> = bound_functions(db, config)
        .await?
        .iter()
        .filter(|f| f.is_http())
        .filter_map(|f| f.node.domain.clone())
        .collect();
    domains.sort();
    domains.dedup();
    Ok(domains)
}

/// Domains carrying at least one hand-owned AND HTTP-exposed function,
/// sorted — these get an AppState provider slot (`{domain}_manual_impls`).
pub async fn domains_with_manual_http(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>> {
    let mut domains: Vec<String> = bound_functions(db, config)
        .await?
        .iter()
        .filter(|f| f.is_http() && f.is_manual())
        .filter_map(|f| f.node.domain.clone())
        .collect();
    domains.sort();
    domains.dedup();
    Ok(domains)
}

/// Domains carrying at least one `implementation = "external"` function,
/// sorted — these get the AppState provider slot (`{domain}_external_impls`)
/// and the `{domain}_service_base_url` config slot (issue #450 slice C).
pub async fn domains_with_external(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>> {
    let mut domains: Vec<String> = bound_functions(db, config)
        .await?
        .iter()
        .filter(|f| f.implementation == FunctionImplementation::External)
        .filter_map(|f| f.node.domain.clone())
        .collect();
    domains.sort();
    domains.dedup();
    Ok(domains)
}

// ── API-plane type resolution (issue #337) ──────────────────────────────

/// One API-plane-resolved type: the Rust type plus the `use` path that
/// makes it compile (None for builtins).
pub struct HttpType {
    pub rust_type: String,
    pub import: Option<String>,
}

/// Rust primitive for a rosetta builtin type title. `None` for
/// non-builtins (their titles resolve through the generated tree).
pub fn builtin_rust_type(type_ref: &str) -> Option<&'static str> {
    match type_ref {
        "number" => Some("f64"),
        "int" => Some("i64"),
        "string" => Some("String"),
        "boolean" => Some("bool"),
        // Date/time builtins carry ISO strings (the bridge's TEXT-mapping
        // precedent — codegraph-generate has no chrono by design).
        "date" | "dateTime" | "zonedDateTime" | "time" => Some("String"),
        _ => None,
    }
}

/// Resolve one function input/output type for the API plane (issue #337):
/// builtin primitives stay bare; codelist titles resolve to the generated
/// enum (`crate::codelist::{Enum}` — the #517 contract); entity titles
/// resolve to the entity's `{Title}Response` DTO (serde + ToSchema
/// derived). Value objects and unknown titles are refused — they carry no
/// generated Rust type, so a signature referencing them cannot compile.
pub async fn resolve_api_type(
    db: &dyn GraphQuerier,
    domain: &str,
    type_ref: &str,
    type_suffix: &str,
) -> std::result::Result<HttpType, String> {
    if let Some(builtin) = builtin_rust_type(type_ref) {
        return Ok(HttpType {
            rust_type: builtin.to_string(),
            import: None,
        });
    }
    for codelist in db
        .list_codelists()
        .await
        .map_err(|e| format!("codelist lookup failed: {e}"))?
    {
        if codelist.name == type_ref {
            return Ok(HttpType {
                rust_type: type_ref.to_string(),
                import: Some(format!("crate::codelist::{type_ref}")),
            });
        }
    }
    match db
        .get_schema(type_ref)
        .await
        .map_err(|e| format!("schema lookup for '{type_ref}' failed: {e}"))?
    {
        Some(schema) if schema.classification.eq_ignore_ascii_case("entity") => {
            let owner = schema.domain.clone().unwrap_or_else(|| domain.to_string());
            let bare = strip_suffix(type_ref, type_suffix);
            let module = to_snake_case(&bare);
            let rust_type = format!("{bare}Response");
            Ok(HttpType {
                import: Some(format!(
                    "crate::domain::{owner}::{module}::dto_response::{rust_type}"
                )),
                rust_type,
            })
        }
        Some(_) => Err(format!(
            "type '{type_ref}' is a value object / wrapper with no generated Rust type"
        )),
        None => Err(format!("type '{type_ref}' is not in the model")),
    }
}

/// The driver-level strict check (issues #450/#337): every HTTP-exposed
/// function must resolve ALL of its input/output types to generated Rust
/// types — primitives, codelist enums, or entity `Response` DTOs. Returns
/// human-readable error strings (empty = valid); the driver turns them
/// into a hard `Error::Config` BEFORE generation, because the
/// domain-generator phase collects errors non-fatally — a broken binding
/// must never half-generate. The functions generator re-runs the same
/// `resolve_api_type` as a defensive backstop; the two cannot drift.
pub async fn check_exposed_function_types(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Vec<String> {
    let mut errors = Vec::new();
    let bound = match bound_functions(db, config).await {
        Ok(f) => f,
        Err(e) => {
            errors.push(format!("[functions] type check failed: {e}"));
            return errors;
        }
    };
    let suffix = &config.defaults.type_suffix;
    for function in bound {
        if !function.is_http() {
            continue;
        }
        let domain = function.node.domain.clone().unwrap_or_default();
        for input in &function.node.inputs {
            if let Err(reason) = resolve_api_type(db, &domain, &input.type_ref, suffix).await {
                errors.push(format!(
                    "function '{}': input '{}' — {reason} — exposed functions need \
                     primitive, codelist, or entity-typed signatures",
                    function.node.name, input.name,
                ));
            }
        }
        if let Some(output) = &function.node.output
            && let Err(reason) = resolve_api_type(db, &domain, &output.type_ref, suffix).await
        {
            errors.push(format!(
                "function '{}': output '{}' — {reason} — exposed functions need \
                 primitive, codelist, or entity-typed signatures",
                function.node.name, output.name,
            ));
        }
    }
    errors
}
