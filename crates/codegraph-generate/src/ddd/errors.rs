use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::ProjectConfig;
use crate::error::Result;
use crate::render_template_with_project;
use crate::traits::{DomainGenerator, DomainGeneratorKind, GeneratedFile};
use codegraph_config::DomainConfig;
use codegraph_naming;

#[derive(Debug, Serialize)]
pub struct ErrorDef {
    pub code: String,
    pub description: String,
    pub http_status: i32,
    pub variant_name: String,
    /// The declared payload fields (DD7, issue #665) — the generated
    /// variant carries them as named fields, and the 4xx body surfaces
    /// them through the error envelope's `details` entries.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ErrorFieldDef>,
}

/// One payload field of a declared error (DD7, issue #665), resolved to
/// the Rust type the generated variant carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorFieldDef {
    /// The field name as declared (also the `details` entry's `field`).
    pub name: String,
    /// The resolved Rust type (primitive mapping, codelist enum, or the
    /// entity-ref `Uuid` slot).
    pub rust_type: String,
    /// `true` for an optional (`0..1`) field.
    pub optional: bool,
}

/// Resolve one declared error's payload fields to Rust types (DD7,
/// issue #665). Closed subset, the #517 slot posture: primitives map like
/// the action-body plane (the shared `body_lower` mapping, so the two
/// planes cannot drift), enums map onto `crate::codelist::{Enum}` BY NAME
/// and class references take the `{Class}Id` `Uuid` slot — neither is
/// membership-verified here (the function has no graph access; the pinned
/// compiler guarantees every mox enum bridges to a codelist, so the
/// unverified-name class is reachable only from a stale/hand-built graph,
/// where the emitted `errors.rs` fails to compile — the compile pin
/// arbitrates). Every other slot type (collections, wraps datatypes, DTO
/// boundaries, unresolvable payloads) IS a NAMED refusal — never a silent
/// widening.
pub fn error_field_defs(
    error_name: &str,
    params: &[codegraph_core::types::DddParam],
) -> std::result::Result<Vec<ErrorFieldDef>, String> {
    use crate::ddd::signature::signature_type;
    let mut out = Vec::with_capacity(params.len());
    for param in params {
        let slot = signature_type(&param.type_json);
        let refused = |why: String| {
            Err(format!(
                "declared error `{error_name}`: payload field `{}` is {}, \
                 which the generated `{error_name}` variant cannot carry \
                 (primitives, codelist enums, and class references only)",
                param.name, why
            ))
        };
        let type_ref = match &slot {
            crate::ddd::signature::DddSignatureType::Domain(type_ref) => type_ref,
            other => return refused(other.describe()),
        };
        // The body plane's strict mapping first (primitives + the named
        // family); the error-payload plane WIDENS it with the two modeled
        // slots the #517 rules resolve — codelist enums take the generated
        // enum, class references take the `{Class}Id` Uuid slot. The
        // action-body plane keeps refusing both (bodies bind primitives
        // only), so this widening never leaks into it.
        let base = match crate::ddd::body_lower::payload_field_rust_type(type_ref) {
            Some(rust_type) => rust_type,
            None => match type_ref {
                phero_ir::TypeRef::Enum { name, .. } => {
                    let short = name.rsplit("::").next().unwrap_or(name);
                    format!(
                        "crate::codelist::{}",
                        codegraph_naming::to_pascal_case(short)
                    )
                }
                phero_ir::TypeRef::Class { .. } => "uuid::Uuid".to_string(),
                _ => return refused(slot.describe()),
            },
        };
        let multiplicity = param
            .multiplicity
            .as_ref()
            .and_then(|json| serde_json::from_value::<phero_ir::Multiplicity>(json.clone()).ok());
        let (rust_type, optional) = match &multiplicity {
            None => (base.clone(), false),
            Some(multiplicity) => {
                if multiplicity.upper != phero_ir::Upper::Finite(1) {
                    return refused("a collection".to_string());
                }
                if multiplicity.lower == 0 {
                    (format!("Option<{base}>"), true)
                } else {
                    (base.clone(), false)
                }
            }
        };
        out.push(ErrorFieldDef {
            name: param.name.clone(),
            rust_type,
            optional,
        });
    }
    Ok(out)
}

/// The declared (graph-seeded) error variants of one domain, exactly as the
/// generated `{Domain}Error` enum carries them: `domains.toml`-scoped
/// domains' `common` definitions plus the domain's own, PascalCase-mapped,
/// with the template's always-emitted built-ins filtered out and
/// first-wins deduped by variant.
///
/// Shared by the emitter below and the #662 statement-body raise lowering —
/// a raise must name a variant the generated enum actually HAS, so the two
/// planes cannot drift.
pub async fn declared_error_defs(db: &dyn GraphQuerier, domain: &str) -> Result<Vec<ErrorDef>> {
    let all_errors = db.get_error_definitions().await.map_err(|e| {
        crate::error::Error::Config(format!("failed to query error definitions: {e}"))
    })?;

    let mut seen_codes: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Variants the template always emits itself — graph-seeded built-ins
    // (e.g. domain="common" Conflict/NotFound from the metamodel ingest)
    // must not duplicate them (E0428).
    const TEMPLATE_BUILTIN_VARIANTS: [&str; 5] = [
        "Conflict",
        "NotFound",
        "Forbidden",
        "Unprocessable",
        "InternalError",
    ];
    let mut domain_errors: Vec<ErrorDef> = all_errors
        .iter()
        .filter(|e| e.domain.as_deref() == Some(domain) || e.domain.as_deref() == Some("common"))
        .filter_map(|e| {
            let normalized = e.code.replace(['-', ' '], "_");
            let variant_name = codegraph_naming::to_pascal_case(&normalized);
            if TEMPLATE_BUILTIN_VARIANTS.contains(&variant_name.as_str()) {
                return None;
            }
            if seen_codes.insert(variant_name.clone()) {
                Some(ErrorDef {
                    code: e.code.clone(),
                    description: e.description.clone(),
                    http_status: e.http_status,
                    variant_name,
                    fields: Vec::new(),
                })
            } else {
                None
            }
        })
        .collect();

    // DD7 (issue #665): the `.ddd` declared error vocabulary folds in
    // BEHIND the config-seeded definitions. A domains.toml override keyed
    // by the declared name keeps its description/status and adopts the
    // model's payload fields; a declaration no config entry names
    // generates as a default-422 variant with the declared name as the
    // envelope code. Prelude names cannot be shadowed: the built-in
    // filter above holds for both sources.
    let models = db
        .get_ddd_models()
        .await
        .map_err(|e| crate::error::Error::Config(format!("failed to query ddd models: {e}")))?;
    for model in &models {
        let module = model.modules.iter().find(|m| m.name == domain);
        // Application-scope errors are visible to every module of their
        // application: this domain carries them only when it hosts one of
        // the model's modules. Module-scope errors are private to their
        // module (domain).
        let application_errors: &[codegraph_core::types::DddErrorDecl] = if module.is_some() {
            &model.application.errors
        } else {
            &[]
        };
        let module_errors: &[codegraph_core::types::DddErrorDecl] =
            module.map(|m| m.errors.as_slice()).unwrap_or(&[]);
        for decl in application_errors.iter().chain(module_errors) {
            let variant_name = codegraph_naming::to_pascal_case(&decl.name);
            if TEMPLATE_BUILTIN_VARIANTS.contains(&variant_name.as_str()) {
                continue;
            }
            let fields =
                error_field_defs(&decl.name, &decl.fields).map_err(crate::error::Error::Config)?;
            if let Some(existing) = domain_errors
                .iter_mut()
                .find(|e| e.variant_name == variant_name)
            {
                // A domains.toml override authored this variant: keep its
                // description/status, adopt the model's payload fields.
                // Two DECLARATIONS colliding on one variant name (two
                // applications hosting modules in one domain — names are
                // application-wide unique upstream, not globally) resolve
                // last-wins by readback order; warn so the collision is
                // visible (issue #665 review N2 — the ordering.rs
                // precedent).
                if !existing.fields.is_empty()
                    && existing.fields != fields
                    && existing.code == codegraph_naming::to_snake_case(&decl.name).to_uppercase()
                {
                    tracing::warn!(
                        domain = %domain,
                        variant = %variant_name,
                        "declared error `{}` collides across applications in domain `{domain}` — the last-read declaration's payload wins",
                        decl.name
                    );
                }
                existing.fields = fields;
                continue;
            }
            domain_errors.push(ErrorDef {
                code: codegraph_naming::to_snake_case(&decl.name).to_uppercase(),
                description: decl
                    .description
                    .clone()
                    .unwrap_or_else(|| decl.name.clone()),
                http_status: 422,
                variant_name,
                fields,
            });
        }
    }

    // Deterministic output: the graph query's row order is not guaranteed,
    // so sort after dedup and after the DD7 fold (keeping the first-wins
    // semantics above).
    domain_errors.sort_by(|a, b| a.code.cmp(&b.code));
    Ok(domain_errors)
}

#[derive(Debug, Serialize)]
pub struct ErrorContext {
    pub domain: String,
    pub errors: Vec<RenderedError>,
}

/// One [`ErrorDef`] with its template fragments pre-rendered (issue #665):
/// the payload-field variant block and the `{ .. }`-aware match arms are
/// built in Rust so the template stays one line per fragment — no Tera
/// whitespace leakage, and unit variants render BYTE-IDENTICAL to the
/// pre-#665 template.
#[derive(Debug, Serialize)]
pub struct RenderedError {
    #[serde(flatten)]
    pub def: ErrorDef,
    /// The full enum-variant fragment (`PaymentDeclined {` fields `},` or
    /// `PaymentDeclined,`), rendered at the template's 4-space indent.
    pub variant_decl: String,
    /// The `code()` match arm (`Self::X { .. } => "CODE",`).
    pub code_arm: String,
    /// The `http_status()` match arm.
    pub status_arm: String,
}

fn render_error(def: ErrorDef) -> RenderedError {
    let (pattern, variant_decl) = if def.fields.is_empty() {
        (def.variant_name.clone(), format!("{},", def.variant_name))
    } else {
        let fields = def
            .fields
            .iter()
            .map(|field| format!("        {}: {},", field.name, field.rust_type))
            .collect::<Vec<_>>()
            .join("\n");
        (
            format!("{} {{ .. }}", def.variant_name),
            format!("{} {{\n{fields}\n    }},", def.variant_name),
        )
    };
    let self_pattern = format!("Self::{pattern}");
    RenderedError {
        variant_decl,
        code_arm: format!("{self_pattern} => \"{}\",", def.code),
        status_arm: format!(
            "{self_pattern} => StatusCode::from_u16({}).unwrap(),",
            def.http_status
        ),
        def,
    }
}

pub struct ErrorGenerator {
    output_dir: PathBuf,
}

impl ErrorGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl DomainGenerator for ErrorGenerator {
    fn kind(&self) -> DomainGeneratorKind {
        DomainGeneratorKind::Errors
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        _entity_titles: &[String],
        _config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // One derivation, shared with the #662 statement-body raise lowering
        // (a raise must name a variant the emitted enum actually carries).
        let domain_errors = declared_error_defs(db, domain).await?;

        // Always emit errors.rs — command/handler templates import the domain
        // error enum unconditionally. With no definitions the enum only
        // carries InternalError, which still compiles.
        let ctx = ErrorContext {
            domain: domain.to_string(),
            errors: domain_errors.into_iter().map(render_error).collect(),
        };

        let content = render_template_with_project(tera, "ddd/errors.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(domain)
                .join("errors.rs"),
            content,
        }])
    }
}
