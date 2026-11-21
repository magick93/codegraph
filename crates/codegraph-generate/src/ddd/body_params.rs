//! The action-body parameter plane (issue #662, extended by #667).
//!
//! Split out of `body_lower.rs` so the #751 over-limit file only shrinks as
//! the modeled-DTO payload surface grows. A parameter is either a primitive
//! (bound from the action-body JSON key) or a modeled DTO boundary — the
//! latter resolves to the generated type in the domain-types crate, and a
//! LONE boundary parameter makes the endpoint body the DTO itself.

use super::body_lower::ValKind;

/// A statically-unlowerable construct. Carries the fully-named diagnostic
/// (`service op \`X\` of service \`Y\`: …`); the caller turns it into a hard
/// generation error (the run path also surfaces it through
/// `ValidationPass::check_service_op_bodies`, before any tree is written).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl BodyParam {
    /// The value-kind classification of the bound local.
    pub(crate) fn kind(&self) -> ValKind {
        ValKind::of_rust_type(&self.rust_type)
    }
}

/// One action-body parameter, extracted from the POST body by the handler.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BodyParam {
    /// The authored parameter name — also the action-body JSON key.
    pub name: String,
    /// The Rust type the handler binds.
    pub rust_type: String,
    /// `true` for an optional (`0..1`) parameter.
    pub optional: bool,
    /// `true` when the endpoint body IS this value (a lone modeled DTO
    /// parameter) rather than an object keyed by [`Self::name`] (issue #667).
    #[serde(default)]
    pub direct_body: bool,
    /// The value the emitted hurl contract sends for it.
    pub sample: serde_json::Value,
    /// The modeled boundary type documented as the endpoint's `request_body`
    /// when this param IS the body (utoipa needs the inner type; `None`
    /// everywhere else).
    #[serde(default)]
    pub request_body: Option<String>,
}

/// Render the handler-side extraction of one action-body parameter.
pub(crate) fn param_extraction_with(param: &BodyParam, binding: &str) -> String {
    let key = &param.name;
    let ty = &param.rust_type;
    let fetch = if param.direct_body {
        // Issue #667: a lone modeled DTO parameter IS the request body. An
        // OPTIONAL parameter deserializes the INNER type — `Option<T>` from
        // `from_value` would nest (`Option<Option<T>>`) and not compile.
        let inner = if param.optional {
            param
                .rust_type
                .strip_prefix("Option<")
                .and_then(|inner| inner.strip_suffix('>'))
                .unwrap_or(&param.rust_type)
        } else {
            &param.rust_type
        };
        format!(
            "body.as_ref().and_then(|value| serde_json::from_value::<{inner}>(value.0.clone()).ok())"
        )
    } else {
        format!(
            "body.as_ref().and_then(|value| value.get(\"{key}\")).cloned().and_then(|value| serde_json::from_value(value).ok())"
        )
    };
    if param.optional {
        format!("    let {binding}: {ty} = {fetch};\n")
    } else {
        format!(
            "    let {binding}: {ty} = {fetch}\n        .ok_or_else(|| AppError::bad_request(\"missing or invalid parameter `{key}`\").with_correlation_id(correlation_id))?;\n"
        )
    }
}

/// The handler-side extraction of one action-body parameter for a
/// REQUEST-DTO construction site (issue #767).
///
/// The plain [`param_extraction_with`] answers `400 Bad Request` on a decode
/// failure — fine for #667's direct-body action, but a request DTO plane
/// answers `422 VALIDATION_ERROR` with a field detail (the
/// `json_body_rejection` shape the hurl contracts pin and the UI's
/// `fieldErrors` mapping reads). A boundary payload therefore extracts through
/// THIS variant: a missing or malformed body is an `AppError::validation`
/// naming the body, never a bare 400.
pub(crate) fn param_extraction_validation(param: &BodyParam, binding: &str) -> String {
    let ty = &param.rust_type;
    let inner = if param.optional {
        param
            .rust_type
            .strip_prefix("Option<")
            .and_then(|inner| inner.strip_suffix('>'))
            .unwrap_or(&param.rust_type)
    } else {
        &param.rust_type
    };
    let required = format!(
        "    let {binding}: {ty} = match body.as_ref() {{\n        Some(value) => \n            serde_json::from_value::<{inner}>(value.0.clone()).map_err(|e| {{\n                AppError::validation(\n                    \"Invalid request body\",\n                    vec![crate::error::FieldError {{\n                        field: \"body\".to_string(),\n                        message: e.to_string(),\n                    }}],\n                )\n                .with_correlation_id(correlation_id)\n            }})?,\n        None => {{\n            return Err(AppError::validation(\n                \"Invalid request body\",\n                vec![crate::error::FieldError {{\n                    field: \"body\".to_string(),\n                    message: \"the request body is required\".to_string(),\n                }}],\n            )\n            .with_correlation_id(correlation_id));\n        }}\n    }};\n"
    );
    if param.optional {
        // Unreachable for the #767 planes (an optional boundary is refused by
        // name), kept total for future callers: deserialize the INNER type.
        format!(
            "    let {binding}: {ty} = body\n        .as_ref()\n        .and_then(|value| serde_json::from_value::<{inner}>(value.0.clone()).ok());\n"
        )
    } else {
        required
    }
}

/// The action-body parameter list of one operation, with the Rust types the
/// handler binds and the sample values the hurl contract sends.
///
/// A modeled DTO boundary (DD3, issue #667) resolves to the generated
/// boundary type in the domain-types crate, and a LONE boundary parameter
/// makes the endpoint body the DTO itself. Output-only boundaries (any
/// derived member) are refused: the caller cannot supply a derived value.
///
/// # Errors
/// [`Refusal`] when a parameter type is not a primitive the action body can
/// carry, or a DTO boundary is unknown/output-only.
pub fn body_params(
    op_name: &str,
    params: &[codegraph_core::types::DddParam],
    boundaries: &crate::ddd::dto_boundaries::DtoBoundaryIndex,
    domain_types_crate: &str,
    casing: crate::project_config::DtoKeyCasing,
) -> Result<Vec<BodyParam>, Refusal> {
    let mut out = Vec::with_capacity(params.len());
    let mut dto_params = 0usize;
    for param in params {
        let slot = crate::ddd::signature::signature_type(&param.type_json);
        let multiplicity = param
            .multiplicity
            .as_ref()
            .and_then(|json| serde_json::from_value::<phero_ir::Multiplicity>(json.clone()).ok());
        if let Some(boundary_name) = slot.dto_name() {
            let Some(shape) = boundaries.get(boundary_name) else {
                return Err(Refusal(format!(
                    "service op `{op_name}`: parameter `{}` references DTO boundary \
                     `{boundary_name}`, which the model does not declare",
                    param.name
                )));
            };
            if shape.direction == "output" {
                let derived: Vec<&str> = shape
                    .members
                    .iter()
                    .filter(|member| member.kind == "derive")
                    .map(|member| member.name.as_str())
                    .collect();
                let detail = if derived.is_empty() {
                    "derived members".to_string()
                } else {
                    derived.join(", ")
                };
                return Err(Refusal(format!(
                    "service op `{op_name}`: parameter `{}` is the output-only DTO boundary \
                     `{boundary_name}` ({detail} cannot be supplied by a caller); use an \
                     input-capable boundary",
                    param.name
                )));
            }
            let Some(path) = boundaries.rust_path(boundary_name, domain_types_crate) else {
                return Err(Refusal(format!(
                    "service op `{op_name}`: parameter `{}` references DTO boundary \
                     `{boundary_name}`, which has no generated Rust type",
                    param.name
                )));
            };
            let optional = multiplicity.as_ref().is_some_and(|m| m.lower == 0);
            if multiplicity.as_ref().is_some_and(|m| m.is_many()) {
                return Err(Refusal(format!(
                    "service op `{op_name}`: parameter `{}` is a collection of DTO boundary \
                     `{boundary_name}` — an action body cannot bind one",
                    param.name
                )));
            }
            // Issue #767 review: a boundary whose minimal body cannot be
            // represented (a member codelist declares ZERO values, so the
            // generated enum has no variants) can never deserialize — refuse
            // the REQUIRED parameter by name instead of emitting a body that
            // is guaranteed to 422. An OPTIONAL one sends `null`, which is
            // valid, so it stays.
            if !optional && !boundaries.sample_is_wire_valid(&shape.name) {
                return Err(Refusal(format!(
                    "service op `{op_name}`: parameter `{}` uses DTO boundary `{}`, whose \
                     minimal body cannot be represented — one of its codelist members \
                     declares zero values, so the generated enum has no variants and no \
                     JSON value can deserialize into it; declare values for the codelist \
                     (or type the member differently)",
                    param.name, boundary_name
                )));
            }
            dto_params += 1;
            let inner_path = path.clone();
            out.push(BodyParam {
                name: param.name.clone(),
                rust_type: if optional {
                    format!("Option<{path}>")
                } else {
                    path
                },
                optional,
                direct_body: false,
                request_body: Some(inner_path),
                sample: if optional {
                    serde_json::Value::Null
                } else {
                    boundaries
                        .sample(
                            &shape.name,
                            casing == crate::project_config::DtoKeyCasing::Camel,
                        )
                        .cloned()
                        .unwrap_or(serde_json::Value::Null)
                },
            });
            continue;
        }
        let Some(type_ref) = slot.as_domain() else {
            return Err(Refusal(format!(
                "service op `{op_name}`: parameter `{}` is {}, which the action \
                 body cannot bind (primitives only)",
                param.name,
                slot.describe()
            )));
        };
        let Some(base) = payload_field_rust_type(type_ref) else {
            return Err(Refusal(format!(
                "service op `{op_name}`: parameter `{}` types `{}`, which the \
                 action body cannot bind (primitives only)",
                param.name,
                slot.describe()
            )));
        };
        let (rust_type, optional) = match &multiplicity {
            None => (base.clone(), false),
            Some(multiplicity) => {
                if multiplicity.upper != phero_ir::Upper::Finite(1) {
                    return Err(Refusal(format!(
                        "service op `{op_name}`: parameter `{}` is a collection — an \
                         action body cannot bind one",
                        param.name
                    )));
                }
                if multiplicity.lower == 0 {
                    (format!("Option<{base}>"), true)
                } else {
                    (base.clone(), false)
                }
            }
        };
        let sample = if optional {
            serde_json::Value::Null
        } else {
            sample_value(&base)
        };
        out.push(BodyParam {
            name: param.name.clone(),
            rust_type,
            optional,
            direct_body: false,
            sample,
            request_body: None,
        });
    }
    // A lone modeled DTO parameter is the request body itself (issue #667).
    if dto_params == 1
        && out.len() == 1
        && let Some(param) = out.first_mut()
    {
        param.direct_body = true;
    }
    Ok(out)
}

/// The graph-native Rust type of one primitive/class/enum payload slot.
/// Shared by the action-body parameter plane and the declared-error payload
/// plane (issue #665) — one mapping, so the two planes cannot drift. It
/// must agree with the entity DTO field mapping (`int`→`i32`,
/// `long`→`i64`, `float`→`f32`, `double`→`f64`) so a body assignment
/// unifies; the compile pin arbitrates.
pub(crate) fn payload_field_rust_type(type_ref: &phero_ir::TypeRef) -> Option<String> {
    use phero_ir::{PrimitiveType, TypeRef};
    match type_ref {
        TypeRef::Primitive(primitive) => match primitive {
            PrimitiveType::String | PrimitiveType::Char => Some("String".to_string()),
            PrimitiveType::Boolean => Some("bool".to_string()),
            PrimitiveType::Int | PrimitiveType::Short | PrimitiveType::Byte => {
                Some("i32".to_string())
            }
            PrimitiveType::Long => Some("i64".to_string()),
            PrimitiveType::Float => Some("f32".to_string()),
            PrimitiveType::Double => Some("f64".to_string()),
            PrimitiveType::Date => Some("chrono::NaiveDate".to_string()),
        },
        TypeRef::Class { name, .. }
        | TypeRef::Enum { name, .. }
        | TypeRef::Datatype { name, .. }
        | TypeRef::Interface { name, .. }
        | TypeRef::Vocabulary { name, .. } => match name.as_str() {
            "Uuid" => Some("uuid::Uuid".to_string()),
            "DateTime" | "Instant" | "Timestamp" => {
                Some("chrono::DateTime<chrono::Utc>".to_string())
            }
            "Decimal" => Some("rust_decimal::Decimal".to_string()),
            "Date" => Some("chrono::NaiveDate".to_string()),
            _ => None,
        },
    }
}

/// The value the emitted hurl contract sends for one parameter type.
fn sample_value(rust_type: &str) -> serde_json::Value {
    match rust_type {
        "String" => serde_json::Value::String("contract-value".to_string()),
        "bool" => serde_json::Value::Bool(true),
        "f32" | "f64" | "rust_decimal::Decimal" => serde_json::json!(1.0),
        "uuid::Uuid" => serde_json::json!("00000000-0000-0000-0000-000000000000"),
        "chrono::NaiveDate" => serde_json::json!("2026-01-01"),
        "i32" | "i64" | "i16" | "i8" => serde_json::json!(1),
        _ => serde_json::Value::Null,
    }
}
