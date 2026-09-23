//! `condition_validations` — constraint-plane codegen (issue #261).
//!
//! Per domain, emits `src/domain/<domain>/validations.rs` containing:
//!
//! - one `validate_{entity}_items` function per schema whose array
//!   properties carry `minItems`/`maxItems` (the JSON path parses them into
//!   `PropertyNode.min_items`/`max_items`; the rosetta bridge leaves them
//!   `None` for now — rosetta `(min..max)` attribute cardinality uplifts
//!   alongside the #262 transpiler), checking `dto.<field>.len()` against
//!   the bounds;
//! - a `// TODO(#262)` marker per ConditionNode owned by the domain's
//!   schemas, where the `Expr::to_json` payload (named conditions) and the
//!   one_of option set (bridge-derived choices) will transpile into
//!   validation code.
//!
//! Gated behind the `rosetta_backend` profile feature via the capability
//! registry — OFF ⇒ the generator never runs and output is byte-identical.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{ConditionKind, PropertyNode};
use codegraph_type_contracts::RefClassificationKind;

use crate::code_writer::{wln, CodeWriter};
use crate::error::Result;
use crate::traits::{DomainGenerator, GeneratedFile};
use crate::ProjectConfig;
use codegraph_config::DomainConfig;

/// One generated `validate_*_items` check: an array field plus its bounds.
struct ItemCheck {
    field: String,
    is_required: bool,
    min_items: Option<u32>,
    max_items: Option<u32>,
}

/// One schema's validation surface.
struct EntityValidations {
    module_name: String,
    entity_name: String,
    checks: Vec<ItemCheck>,
    conditions: Vec<(String, ConditionKind, Vec<String>)>,
}

pub struct ConditionValidationsGenerator {
    output_dir: PathBuf,
}

impl ConditionValidationsGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl DomainGenerator for ConditionValidationsGenerator {
    fn name(&self) -> &str {
        "condition_validations"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        entity_titles: &[String],
        _config: &DomainConfig,
        _tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut entities: Vec<EntityValidations> = Vec::new();
        for title in entity_titles {
            if !seen.insert(title.as_str()) {
                continue;
            }
            let Some(schema) = db.get_schema_in_domain(title, domain).await? else {
                continue;
            };
            // allOf composition can duplicate properties; first wins.
            let mut deduped: HashSet<String> = HashSet::new();
            let props: Vec<PropertyNode> = db
                .get_properties(title)
                .await?
                .into_iter()
                .filter(|p| deduped.insert(p.rust_field_name.clone()))
                .collect();
            // Media fields are excluded from Create/Update DTOs (uploads go
            // through the media endpoints), so their bounds would reference
            // a nonexistent DTO field.
            let checks: Vec<ItemCheck> = props
                .iter()
                .filter(|p| p.is_array)
                .filter(|p| p.min_items.is_some() || p.max_items.is_some())
                .filter(|p| p.effective_kind() != Some(RefClassificationKind::MediaWrapper))
                .map(|p| ItemCheck {
                    field: p.rust_field_name.clone(),
                    is_required: p.is_required,
                    min_items: p.min_items,
                    max_items: p.max_items,
                })
                .collect();
            let conditions: Vec<(String, ConditionKind, Vec<String>)> = db
                .get_conditions_for_schema(title)
                .await?
                .into_iter()
                .map(|c| (c.name, c.kind, c.options))
                .collect();
            if checks.is_empty() && conditions.is_empty() {
                continue;
            }
            entities.push(EntityValidations {
                module_name: schema.pg_table_name.clone(),
                entity_name: schema.rust_type_name.clone(),
                checks,
                conditions,
            });
        }

        if entities.is_empty() {
            return Ok(Vec::new());
        }

        let content = emit_domain_validations(domain, &entities, project);
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(domain)
                .join("validations.rs"),
            content,
        }])
    }
}

/// Emit the complete `validations.rs` for one domain (generation order —
/// deterministic by construction).
fn emit_domain_validations(
    domain: &str,
    entities: &[EntityValidations],
    project: &ProjectConfig,
) -> String {
    let mut code = CodeWriter::new();
    wln!(
        code,
        "//! Item-count validations for the {domain} domain.\n\
         //! Generated by {} — DO NOT EDIT.",
        project.generator_name
    );
    wln!(code);

    let mut imported: HashSet<String> = HashSet::new();
    for entity in entities {
        if entity.checks.is_empty() || !imported.insert(entity.module_name.clone()) {
            continue;
        }
        wln!(
            code,
            "use crate::domain::{domain}::{module}::dto_create::Create{entity}Request;",
            module = entity.module_name,
            entity = entity.entity_name,
        );
    }

    for entity in entities {
        emit_entity_fn(&mut code, entity);
    }

    code.into_string()
}

/// Emit one `validate_{module}_items` function (item checks + #262 markers).
fn emit_entity_fn(code: &mut CodeWriter, entity: &EntityValidations) {
    wln!(code);
    if entity.checks.is_empty() {
        // Conditions only — no DTO to take, so the marker block stands alone.
        emit_condition_markers(code, entity, "    ");
        return;
    }
    wln!(
        code,
        "/// Item-count constraints for {} (create path).",
        entity.entity_name
    );
    wln!(
        code,
        "pub fn validate_{module}_items(dto: &Create{entity}Request) -> Result<(), String> {{",
        module = entity.module_name,
        entity = entity.entity_name,
    );
    for check in &entity.checks {
        emit_item_check(code, check, &entity.entity_name);
    }
    emit_condition_markers(code, entity, "    ");
    wln!(code, "    Ok(())");
    wln!(code, "}}");
}

/// Emit one array bound check. Optional arrays (`Option<Vec<T>>`) only
/// validate when present — JSON Schema `minItems` does not apply to an
/// absent value.
fn emit_item_check(code: &mut CodeWriter, check: &ItemCheck, entity_name: &str) {
    let field = check.field.as_str();
    let mut bounds: Vec<Bound> = Vec::new();
    if let Some(min) = check.min_items {
        bounds.push(Bound {
            direction: "at least",
            value: min,
            cond: if check.is_required {
                format!("dto.{field}.len() < {min}")
            } else {
                format!("items.len() < {min}")
            },
            len_expr: if check.is_required {
                format!("dto.{field}.len()")
            } else {
                "items.len()".to_string()
            },
        });
    }
    if let Some(max) = check.max_items {
        bounds.push(Bound {
            direction: "at most",
            value: max,
            cond: if check.is_required {
                format!("dto.{field}.len() > {max}")
            } else {
                format!("items.len() > {max}")
            },
            len_expr: if check.is_required {
                format!("dto.{field}.len()")
            } else {
                "items.len()".to_string()
            },
        });
    }
    for bound in &bounds {
        if check.is_required {
            emit_bound(code, "    ", bound, entity_name, field);
        } else {
            // Optional array: each bound wraps in its own `if let Some` — an
            // absent array passes (JSON Schema minItems/maxItems apply only
            // when present).
            wln!(code, "    if let Some(items) = &dto.{field} {{");
            emit_bound(code, "        ", bound, entity_name, field);
            wln!(code, "    }}");
        }
    }
}

/// One resolved bound check ready for emission.
struct Bound {
    direction: &'static str,
    value: u32,
    cond: String,
    len_expr: String,
}

/// Emit one bound check block: `if <cond> { return Err(format!(...)); }`.
fn emit_bound(code: &mut CodeWriter, pad: &str, bound: &Bound, entity: &str, field: &str) {
    let msg = format!(
        "{entity}.{field} allows {direction} {value} item(s), got {{}}",
        direction = bound.direction,
        value = bound.value,
    );
    wln!(code, "{pad}if {} {{", bound.cond);
    wln!(
        code,
        "{pad}    return Err(format!(\"{msg}\", {}));",
        bound.len_expr
    );
    wln!(code, "{pad}}}");
}

/// Emit the `// TODO(#262)` transpilation markers for one schema's
/// ConditionNodes (named conditions + bridge-derived one_of).
fn emit_condition_markers(code: &mut CodeWriter, entity: &EntityValidations, pad: &str) {
    for (name, kind, options) in &entity.conditions {
        match kind {
            ConditionKind::Condition => {
                wln!(
                    code,
                    "{pad}// TODO(#262): transpile condition '{name}' from its Expr::to_json payload"
                );
            }
            ConditionKind::OneOf => {
                wln!(
                    code,
                    "{pad}// TODO(#262): transpile one_of '{name}' from its options [{}]",
                    options.join(", ")
                );
            }
        }
    }
}
