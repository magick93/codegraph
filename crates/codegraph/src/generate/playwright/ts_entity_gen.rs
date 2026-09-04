use crate::generate::domain_model::{
    build_entity_model, example_for_field, parse_rust_type, ts_type_for_field, EntityField,
    RustType,
};
use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;
use heck::ToLowerCamelCase;

use super::{e2e_tests_root, TsEntityContext, TsFieldDef, TsFkField};
use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};

/// Scalar (non-array) ValueObject / CompositeWrapper / MediaWrapper properties
/// are stored as flattened child columns on the main table (the Create DTO
/// mirrors this). The canonical entity model keeps them as single child-table
/// fields, so the playwright fixtures/specs expand them into their DTO-shaped
/// flat columns here — without affecting other generators (xrpc, ui, ...).
async fn expand_vo_fields(
    db: &dyn GraphQuerier,
    schema_title: &str,
    model_fields: &[EntityField],
    properties: &[PropertyNode],
) -> Result<Vec<EntityField>> {
    let mut out = Vec::new();
    for field in model_fields {
        let Some(prop) = properties.iter().find(|p| p.name == field.name) else {
            out.push(field.clone());
            continue;
        };
        let kind = prop.effective_kind();
        let expandable = matches!(
            kind,
            Some(RefClassificationKind::CompositeWrapper)
                | Some(RefClassificationKind::MediaWrapper)
                | Some(RefClassificationKind::ValueObject)
                | Some(RefClassificationKind::EntityReference)
        );
        let is_scalar_vo = !prop.is_array && expandable;
        let is_array_vo = prop.is_array
            && matches!(
                kind,
                Some(RefClassificationKind::CompositeWrapper)
                    | Some(RefClassificationKind::MediaWrapper)
                    | Some(RefClassificationKind::ValueObject)
            );
        if !is_scalar_vo && !is_array_vo {
            out.push(field.clone());
            continue;
        }

        // Composite / media wrappers expand into flattened child columns,
        // e.g. `person` (PersonReferenceType) → person_did / person_name / ...
        // Array VOs are flattened the same way when the DDL materializes their
        // columns on the main table (e.g. `recipients` → recipients_did / ...).
        if matches!(
            kind,
            Some(RefClassificationKind::CompositeWrapper)
                | Some(RefClassificationKind::MediaWrapper)
        ) || (prop.is_array && matches!(kind, Some(RefClassificationKind::ValueObject)))
        {
            if let Ok(cols) = db.get_composite_columns(&prop.name, schema_title).await {
                for col in cols {
                    let rust_name = format!("{}{}", prop.rust_field_name, col.suffix);
                    let column_name = format!("{}{}", prop.pg_column_name, col.suffix);
                    if out.iter().any(|f| f.rust_field == rust_name) {
                        continue;
                    }
                    let base_rt = parse_rust_type(&col.rust_type, prop.is_required);
                    let rust_type = if prop.is_required {
                        base_rt
                    } else {
                        RustType::Optional {
                            optional: Box::new(base_rt),
                        }
                    };
                    let is_fk = column_name.ends_with("_id");
                    out.push(EntityField {
                        name: rust_name.to_lower_camel_case(),
                        column: column_name.clone(),
                        rust_field: rust_name.clone(),
                        rust_type: rust_type.clone(),
                        sea_orm_type: col.sea_orm_type.clone(),
                        pg_type: col.pg_type.clone(),
                        ts_type: ts_type_for_field(&rust_type),
                        required: prop.is_required,
                        is_pk: false,
                        is_fk,
                        fk_target: if is_fk { col.fk_target.clone() } else { None },
                        fk_table: None,
                        classification: Some("composite_column".to_string()),
                        example_value: example_for_field(&rust_name, &col.rust_type, None),
                        label: field.label.clone(),
                        inherited: false,
                        is_child_table: false,
                        is_model_optional: !prop.is_required,
                    });
                }
                continue;
            }
        }

        // Scalar entity references — the DDL emits `{prop}_id` FK columns and
        // the DTO exposes them as flat `{prop}Id` fields. Nullability honors the
        // schema's `required` (JSON schema is the source of truth): a required
        // FK is a plain `campaignId: string` that the fixture must populate;
        // an optional FK stays `campaignId?: string | null`.
        if !prop.is_array && kind == Some(RefClassificationKind::EntityReference) {
            let fd = codegraph_core::types::resolve_field(prop);
            if out.iter().any(|f| f.rust_field == fd.rust_field_name) {
                continue;
            }
            let rust_type = if prop.is_required {
                RustType::Simple("Uuid".to_string())
            } else {
                RustType::Optional {
                    optional: Box::new(RustType::Simple("Uuid".to_string())),
                }
            };
            out.push(EntityField {
                name: fd.rust_field_name.to_lower_camel_case(),
                column: fd.column_name.clone(),
                rust_field: fd.rust_field_name.clone(),
                rust_type: rust_type.clone(),
                sea_orm_type: "Uuid".to_string(),
                pg_type: "UUID".to_string(),
                ts_type: ts_type_for_field(&rust_type),
                required: prop.is_required,
                is_pk: false,
                is_fk: true,
                fk_target: prop.ref_target.clone(),
                fk_table: None,
                classification: Some("entity_reference".to_string()),
                example_value: example_for_field(&fd.rust_field_name, "Uuid", None),
                label: field.label.clone(),
                inherited: false,
                is_child_table: false,
                is_model_optional: !prop.is_required,
            });
            continue;
        }

        // Scalar ValueObjects referencing a known entity — the DDL emits an
        // FK column, so the DTO exposes `{prop}_id`. Pure VOs stay nested in
        // the DTO (and are optional), so they keep their single-field form.
        let vo_target_is_entity = match db.get_property_ref_target(&prop.name, schema_title).await {
            Ok(Some(target)) => {
                if target.is_entity {
                    true
                } else {
                    codegraph_core::traits::find_entity_extended_by_vo(db, &target.title)
                        .await
                        .ok()
                        .flatten()
                        .is_some()
                }
            }
            _ => false,
        };
        if vo_target_is_entity {
            let fd = codegraph_core::types::resolve_field(prop);
            if out.iter().any(|f| f.rust_field == fd.rust_field_name) {
                continue;
            }
            let rust_type = RustType::Optional {
                optional: Box::new(RustType::Simple("Uuid".to_string())),
            };
            out.push(EntityField {
                name: fd.rust_field_name.to_lower_camel_case(),
                column: fd.column_name.clone(),
                rust_field: fd.rust_field_name.clone(),
                rust_type: rust_type.clone(),
                sea_orm_type: "Uuid".to_string(),
                pg_type: "UUID".to_string(),
                ts_type: ts_type_for_field(&rust_type),
                required: false,
                is_pk: false,
                is_fk: true,
                fk_target: prop.ref_target.clone(),
                fk_table: None,
                classification: Some("value_object_fk".to_string()),
                example_value: example_for_field(&fd.rust_field_name, "Uuid", None),
                label: field.label.clone(),
                inherited: false,
                is_child_table: false,
                is_model_optional: true,
            });
            continue;
        }

        out.push(field.clone());
    }
    Ok(out)
}

pub struct TsEntityGenerator {
    output_dir: PathBuf,
}

/// Resolve FK-target metadata for a required entity-ref field so the spec
/// generator can create a real parent row in `beforeAll`. Returns None for
/// non-FK, optional-FK, or unresolvable targets.
async fn resolve_fk_target_meta(
    db: &dyn GraphQuerier,
    schema_title: &str,
    field: &EntityField,
) -> Option<(String, String, String, String)> {
    if !field.is_fk || !field.required {
        return None;
    }
    // Map the entity field back to its source property to resolve the target.
    let props = db.get_properties(schema_title).await.unwrap_or_default();
    let prop = props.iter().find(|p| {
        let fd = codegraph_core::types::resolve_field(p);
        fd.rust_field_name == field.rust_field || p.pg_column_name == field.column
    })?;
    let target = db
        .get_property_ref_target(&prop.name, schema_title)
        .await
        .ok()
        .flatten()?;
    // Only genuine entity targets need parent-creation — scalar VOs (e.g.
    // startDate → DateType) are stored inline, not as FK rows.
    if !target.is_entity {
        return None;
    }
    let domain = target.domain.clone().unwrap_or_default();
    let path = if target.api_path_segment.is_empty() {
        target.pg_table_name.replace('_', "-")
    } else {
        target.api_path_segment.clone()
    };
    let module = format!(
        "{}_{}",
        target.domain.as_deref().unwrap_or("public"),
        target.pg_table_name
    );
    Some((domain, path, module, target.rust_type_name.clone()))
}

impl TsEntityGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for TsEntityGenerator {
    fn name(&self) -> &str {
        "playwright_ts_entity"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let model =
            build_entity_model(db, schema_title, domain, config, &project.atproto_authority)
                .await?;

        if model.entity_module.is_empty() {
            return Ok(Vec::new());
        }

        let properties = db.get_properties_in_domain(schema_title, domain).await?;
        let fields = expand_vo_fields(db, schema_title, &model.fields, &properties).await?;

        let mut create_fields: Vec<TsFieldDef> = Vec::with_capacity(fields.len());
        for f in &fields {
            let (fk_target_domain, fk_target_path, fk_target_module, fk_target_entity_name) =
                match resolve_fk_target_meta(db, schema_title, f).await {
                    Some((d, p, m, e)) => (Some(d), Some(p), Some(m), Some(e)),
                    None => (None, None, None, None),
                };
            create_fields.push(TsFieldDef {
                name: f.name.clone(),
                label: f.label.clone(),
                ts_type: f.ts_type.clone(),
                required: f.required,
                example_value: f.example_value.clone(),
                fk_target_domain,
                fk_target_path,
                fk_target_module,
                fk_target_entity_name: fk_target_entity_name.clone(),
                js_var: fk_target_entity_name.map(|_| f.name.to_lower_camel_case()),
            });
        }

        // Full-text search detection — mirrors ui/e2e_test.rs.
        let entity_cfg = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(&model.name));

        let has_fts = entity_cfg
            .map(|ec| {
                !ec.search.fts_weights.is_empty()
                    || ec
                        .search
                        .fts_columns
                        .as_ref()
                        .map(|c| !c.is_empty())
                        .unwrap_or(false)
            })
            .unwrap_or(false);

        let (fts_search_field, fts_search_field_required, fts_secondary_field) = if has_fts {
            let ec = entity_cfg.expect("fts implies entity config");

            // Map a DB column name to its create-DTO field (camelCase, flattened).
            let find_field = |column: &str| -> Option<&EntityField> {
                fields.iter().find(|f| f.column == column).or_else(|| {
                    let camel = column.to_lower_camel_case();
                    fields.iter().find(|f| f.name == camel)
                })
            };

            // Highest-weight column (A > B > C > D). Deterministic: walk
            // fts_columns in config order and pick the first column carrying
            // the best weight; fall back to the weight map (auto-discovery)
            // when fts_columns is absent.
            let primary_col = {
                let weight_rank = |col: &str| {
                    let w = ec
                        .search
                        .fts_weights
                        .get(col)
                        .map(|v| v.as_str())
                        .unwrap_or("D");
                    ["A", "B", "C", "D"]
                        .iter()
                        .position(|x| *x == w)
                        .unwrap_or(3) as u8
                };
                let mut best: Option<(u8, String)> = None;
                if let Some(cols) = ec.search.fts_columns.as_ref() {
                    for col in cols {
                        let rank = weight_rank(col);
                        if best.as_ref().map(|(br, _)| rank < *br).unwrap_or(true) {
                            best = Some((rank, col.clone()));
                        }
                    }
                }
                best.map(|(_, c)| c)
                    .or_else(|| {
                        ["A", "B", "C", "D"].iter().find_map(|w| {
                            ec.search
                                .fts_weights
                                .iter()
                                .find(|(_, v)| v.as_str() == *w)
                                .map(|(k, _)| k.clone())
                        })
                    })
                    .unwrap_or_default()
            };

            let primary = find_field(&primary_col);
            let search_field = primary.map(|f| f.name.clone());
            let search_field_required = primary.map(|f| f.required).unwrap_or(false);

            // Secondary (non-A weight) column usable in a create payload. Free-text
            // only — codelist/FK columns (e.g. `region`) cannot hold test terms.
            let a_cols: std::collections::HashSet<&String> = ec
                .search
                .fts_weights
                .iter()
                .filter(|(_, v)| v.as_str() == "A")
                .map(|(k, _)| k)
                .collect();
            let secondary = ec
                .search
                .fts_columns
                .as_ref()
                .into_iter()
                .flatten()
                .filter(|c| !a_cols.contains(*c) && Some(c.as_str()) != Some(primary_col.as_str()))
                .find_map(|c| {
                    let f = find_field(c)?;
                    if !f.ts_type.contains("string") {
                        return None;
                    }
                    if f.name.contains("region") || f.name.contains("Region") {
                        return None;
                    }
                    Some(f.name.clone())
                })
                .unwrap_or_default();

            (
                search_field.unwrap_or_default(),
                search_field_required,
                secondary,
            )
        } else {
            (String::new(), false, String::new())
        };

        // Fallback when the searchable column isn't a create-DTO field: seed the
        // term in the first required string field so the fixture payload is valid.
        let (fts_search_field, fts_search_field_required) =
            if has_fts && fts_search_field.is_empty() {
                match create_fields
                    .iter()
                    .find(|f| f.required && f.ts_type.contains("string"))
                {
                    Some(f) => (f.name.clone(), true),
                    None => (String::new(), false),
                }
            } else {
                (fts_search_field, fts_search_field_required)
            };

        let has_required_fields = create_fields.iter().any(|f| f.required);

        // Permission-gated entities deny requests whose actor has no DID, so the
        // generated spec/fixture must use a DID persona token and stamp the
        // persona DID into did-carrying fields.
        let permission_scope = config
            .domains
            .get(&model.domain)
            .and_then(|d| d.get_entity_config(&model.name))
            .and_then(|c| c.permissions.scope.clone());
        let use_persona_token = permission_scope.is_some();
        let permission_record_scoped = config
            .domains
            .get(&model.domain)
            .and_then(|d| d.get_entity_config(&model.name))
            .map(|c| c.permissions.record_scoped)
            .unwrap_or(false);
        let persona_did = "did:plc:test.generated".to_string();

        let fk_fields: Vec<TsFkField> = create_fields
            .iter()
            .filter(|f| f.fk_target_entity_name.is_some())
            .map(|f| TsFkField {
                name: f.name.clone(),
                entity_name: f.fk_target_entity_name.clone().unwrap_or_default(),
                target_domain: f.fk_target_domain.clone().unwrap_or_default(),
                target_path: f.fk_target_path.clone().unwrap_or_default(),
                target_module: f.fk_target_module.clone().unwrap_or_default(),
                js_var: f.name.to_lower_camel_case(),
            })
            .collect();

        let ctx = TsEntityContext {
            entity_name: model.name.clone(),
            module_name: model.entity_module.clone(),
            domain: model.domain.clone(),
            path_segment: model.api_path.clone(),
            nsid: model.nsid.clone(),
            has_create: model.operations.create,
            has_read: model.operations.read,
            has_update: model.operations.update,
            has_delete: model.operations.delete,
            has_list: model.operations.list,
            create_fields,
            has_required_fields,
            fk_fields,
            schema_name: model.entity_module.clone(),
            has_fts,
            fts_search_field: fts_search_field.clone(),
            fts_search_field_required,
            fts_secondary_field: fts_secondary_field.clone(),
            use_persona_token,
            permission_record_scoped,
            persona_did,
        };

        // The harness lives at the repo root (hand-extended under
        // specs/manual/), not inside the generated tree. Spec/fixture/api
        // subpaths mirror the old layout so relative imports and
        // `testDir: './specs'` keep working.
        let e2e_dir = e2e_tests_root(&self.output_dir);
        let spec_dir = e2e_dir.join("specs").join(domain);
        let fixture_dir = e2e_dir.join("fixtures").join(domain);
        let api_dir = e2e_dir.join("api").join(domain);

        let mut files = vec![
            GeneratedFile {
                path: spec_dir.join(format!("{}.spec.ts", model.entity_module)),
                content: render_template_with_project(
                    tera,
                    "playwright/ts_spec.tera",
                    &ctx,
                    project,
                )?,
            },
            GeneratedFile {
                path: fixture_dir.join(format!("{}.ts", model.entity_module)),
                content: render_template_with_project(
                    tera,
                    "playwright/ts_fixture.tera",
                    &ctx,
                    project,
                )?,
            },
            GeneratedFile {
                path: api_dir.join(format!("{}.ts", model.entity_module)),
                content: render_template_with_project(
                    tera,
                    "playwright/ts_api_client.tera",
                    &ctx,
                    project,
                )?,
            },
        ];

        // Full-text search spec — one per FTS-enabled entity with create+delete.
        if has_fts
            && model.operations.create
            && model.operations.delete
            && !fts_search_field.is_empty()
        {
            files.push(GeneratedFile {
                path: spec_dir.join(format!("{}.search.spec.ts", model.entity_module)),
                content: render_template_with_project(
                    tera,
                    "playwright/ts_search_spec.tera",
                    &ctx,
                    project,
                )?,
            });
        }

        Ok(files)
    }
}
