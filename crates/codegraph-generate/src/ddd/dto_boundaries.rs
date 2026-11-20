//! Modeled DTO boundary code generation (DD3, issue #667).
//!
//! A `.ddd` `dto` design declares a transport boundary — a **mirror**
//! (`dto User`, every stored feature), a **projection**
//! (`dto BookSummary from Book { title; minutes = playLength / 60; }`), or a
//! **standalone** shape (`dto CreateBookRequest { String title; }`, no domain
//! class). Ingest mirrors every shape onto the graph
//! ([`codegraph_core::types::DddDtoShapeNode`]); this generator turns them
//! into real Rust types in the domain-types crate so a `DddTypeRef::Dto`
//! signature slot resolves to one type everywhere.
//!
//! Layout: one file per `.ddd` module — `{domain_types_base}/src/dto/{module}.rs`
//! — plus the module index. The types land in the SHARED domain-types crate
//! (not the app) because worker-topology action handlers must reach them too.
//!
//! Direction discipline: a boundary carrying any derived member is
//! `output`-only upstream, and an output boundary in a parameter slot is a
//! generation error (see `ddd::service_ops` / `ddd::body_lower`). Input
//! boundaries keep `deny_unknown_fields`, mirroring the generated create DTOs.
//!
//! Presence-gated: no `dto` designs ⇒ no files, so flag-off output is
//! byte-identical.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::DddDtoShapeNode;
use serde::Serialize;

use crate::ProjectConfig;
use crate::error::{Error, Result};
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};

/// One generated boundary.
#[derive(Debug, Serialize)]
struct BoundaryCtx {
    name: String,
    /// `"input"` or `"output"` (the shape's derived direction).
    direction: String,
    /// The mirror form: a type alias onto the source entity's response DTO.
    is_alias: bool,
    /// The alias target path (mirror form only).
    alias_target: Option<String>,
    /// The source class name as authored, for the doc comment.
    source_note: Option<String>,
    /// `true` for the camelCase wire contract (project dto casing).
    camel_case: bool,
    /// Input boundaries reject unknown fields (the create-DTO discipline).
    deny_unknown: bool,
    /// `Default` is derived only when every field can default (no plain
    /// non-Option primitive members are exempted — derivations are computed at
    /// construction, so a boundary with any field still defaults only if all
    /// are Option/Vec; mirrored by `has_default` below).
    has_default: bool,
    fields: Vec<BoundaryFieldCtx>,
}

#[derive(Debug, Serialize)]
struct BoundaryFieldCtx {
    name: String,
    rust_type: String,
    optional: bool,
    derived: bool,
    expression: Option<String>,
}

/// One import from another boundary module.
#[derive(Debug, Serialize)]
struct BoundaryImportCtx {
    module: String,
    name: String,
}

/// The per-module render context.
#[derive(Debug, Serialize)]
struct BoundaryFileCtx {
    module: String,
    has_structs: bool,
    codelist_imports: Vec<String>,
    boundary_imports: Vec<BoundaryImportCtx>,
    boundaries: Vec<BoundaryCtx>,
}

/// Generates the modeled DTO boundaries into the domain-types crate.
pub struct DddDtoBoundariesGenerator {
    src_dir: PathBuf,
}

impl DddDtoBoundariesGenerator {
    /// Creates a generator that writes under `base_dir` (the domain-types
    /// crate root), appending `src/` internally.
    pub fn new_with_base(base_dir: PathBuf) -> Self {
        Self {
            src_dir: base_dir.join("src"),
        }
    }
}

#[async_trait]
impl GlobalGenerator for DddDtoBoundariesGenerator {
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::DddDtoBoundaries
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[crate::GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let models = db.get_ddd_models().await?;
        let shapes: Vec<&DddDtoShapeNode> =
            models.iter().flat_map(|m| m.dto_shapes.iter()).collect();
        if shapes.is_empty() {
            return Ok(Vec::new());
        }

        // DD3: boundary names are application-unique upstream and must not
        // collide with the generated entity DTO type names — a clash would
        // make two different types share one identifier in the API plane.
        let reserved = reserved_dto_names(db, config, generation_order).await;
        for shape in &shapes {
            if reserved.contains(&shape.name) {
                return Err(Error::Config(format!(
                    "ddd — modeled DTO boundary `{}` collides with the generated entity DTO \
                     type of the same name; rename the boundary (boundary names are \
                     application-unique upstream)",
                    shape.name
                )));
            }
        }

        // Group by `.ddd` module, preserving declaration order within a group
        // and sorting modules deterministically.
        let mut by_module: BTreeMap<String, Vec<&DddDtoShapeNode>> = BTreeMap::new();
        for shape in &shapes {
            by_module
                .entry(shape.module.clone())
                .or_default()
                .push(shape);
        }

        let module_of: BTreeMap<String, String> = shapes
            .iter()
            .map(|s| (s.name.clone(), codegraph_naming::to_snake_case(&s.module)))
            .collect();
        let shape_by_name: BTreeMap<String, &DddDtoShapeNode> =
            shapes.iter().map(|s| (s.name.clone(), *s)).collect();

        let src_dir = &self.src_dir;
        let mut files = Vec::new();
        let mut index_modules = Vec::new();

        for (module, module_shapes) in &by_module {
            let module_file = codegraph_naming::to_snake_case(module);
            index_modules.push(module_file.clone());
            let mut boundaries = Vec::new();
            let mut codelist_imports: BTreeSet<String> = BTreeSet::new();
            let mut boundary_imports: BTreeMap<(String, String), ()> = BTreeMap::new();

            for shape in module_shapes {
                let ctx = self
                    .boundary_context(
                        db,
                        config,
                        project,
                        shape,
                        &module_file,
                        &shape_by_name,
                        &module_of,
                        &mut codelist_imports,
                        &mut boundary_imports,
                    )
                    .await?;
                boundaries.push(ctx);
            }

            let has_structs = boundaries.iter().any(|b| !b.is_alias);
            let file_ctx = BoundaryFileCtx {
                module: module.clone(),
                has_structs,
                codelist_imports: codelist_imports.into_iter().collect(),
                boundary_imports: boundary_imports
                    .into_keys()
                    .map(|(module, name)| BoundaryImportCtx { module, name })
                    .collect(),
                boundaries,
            };
            let content = render_template_with_project(
                tera,
                "domain_types/dto_boundaries.tera",
                &file_ctx,
                project,
            )?;
            files.push(GeneratedFile {
                path: src_dir.join("dto").join(format!("{module_file}.rs")),
                content,
            });
        }

        // The module index. The crate's lib.rs declares `pub mod dto;` when the
        // graph carries boundaries (see the domain-types scaffold generator).
        let mut index = String::from(
            "//! Modeled DTO boundaries (DD3, issue #667).\n\
             //! Generated by codegraph. DO NOT EDIT.\n\n",
        );
        for module in &index_modules {
            index.push_str(&format!("pub mod {module};\n"));
        }
        files.push(GeneratedFile {
            path: src_dir.join("dto").join("mod.rs"),
            content: index,
        });

        Ok(files)
    }
}

impl DddDtoBoundariesGenerator {
    /// Build one boundary's render context, resolving each member's Rust type.
    #[allow(clippy::too_many_arguments)]
    async fn boundary_context(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        project: &ProjectConfig,
        shape: &DddDtoShapeNode,
        module_file: &str,
        shape_by_name: &BTreeMap<String, &DddDtoShapeNode>,
        module_of: &BTreeMap<String, String>,
        codelist_imports: &mut BTreeSet<String>,
        boundary_imports: &mut BTreeMap<(String, String), ()>,
    ) -> Result<BoundaryCtx> {
        let camel_case =
            project.codegen.dto_key_casing == crate::project_config::DtoKeyCasing::Camel;
        // The mirror form is every member a plain single-segment selection of
        // its own name: the source entity's response model, which upstream
        // spells `dto User` (no `from`, no derived member).
        let is_mirror = shape.source_json.is_some()
            && !shape.members.is_empty()
            && shape.members.iter().all(|m| {
                m.kind == "select"
                    && m.path.len() == 1
                    && m.path.first() == Some(&m.name)
                    && m.expression.is_none()
            });
        if is_mirror && let Some((domain, title)) = resolve_source_entity(db, config, shape).await {
            let entity = db
                .get_schema_in_domain(&title, &domain)
                .await
                .ok()
                .flatten()
                .map(|s| s.rust_type_name)
                .unwrap_or_else(|| {
                    codegraph_naming::to_pascal_case(&config.defaults.strip_suffix(&title))
                });
            let entity_module =
                codegraph_naming::to_snake_case(&config.defaults.strip_suffix(&title));
            return Ok(BoundaryCtx {
                name: shape.name.clone(),
                direction: shape.direction.clone(),
                is_alias: true,
                alias_target: Some(format!(
                    "crate::{domain}::{entity_module}::dto_response::{entity}Response"
                )),
                source_note: Some(shape_class_name(shape).unwrap_or_else(|| title.clone())),
                camel_case,
                deny_unknown: false,
                has_default: false,
                fields: Vec::new(),
            });
        }

        let mut fields = Vec::with_capacity(shape.members.len());
        for member in &shape.members {
            let base = self.member_rust_type(
                &shape.name,
                member,
                module_file,
                shape_by_name,
                module_of,
                codelist_imports,
                boundary_imports,
            )?;
            let (rust_type, optional) = wrap_multiplicity(&base, member.multiplicity_json.as_ref())
                .map_err(|reason| {
                    Error::Config(format!(
                        "ddd — modeled DTO boundary `{}` member `{}`: {reason}",
                        shape.name, member.name
                    ))
                })?;
            if member.kind == "derive" && member.expression.is_none() {
                return Err(Error::Config(format!(
                    "ddd — modeled DTO boundary `{}` member `{}` is a derivation with no \
                     expression source; the artifact is malformed",
                    shape.name, member.name
                )));
            }
            fields.push(BoundaryFieldCtx {
                name: codegraph_naming::to_snake_case(&member.name),
                rust_type,
                optional,
                derived: member.kind == "derive",
                expression: member.expression.clone(),
            });
        }

        // `Default` only when every field can default: Option/Vec members do,
        // a required scalar does not. This mirrors the response DTO's derive
        // discipline without inventing defaults for required inputs.
        let has_default = fields
            .iter()
            .all(|f| f.optional || f.rust_type.starts_with("Vec<") || f.derived);

        Ok(BoundaryCtx {
            name: shape.name.clone(),
            direction: shape.direction.clone(),
            is_alias: false,
            alias_target: None,
            source_note: shape_class_name(shape),
            camel_case,
            deny_unknown: shape.direction == "input",
            has_default,
            fields,
        })
    }

    /// Resolve one member's base Rust type (before multiplicity wrapping).
    fn member_rust_type(
        &self,
        boundary: &str,
        member: &codegraph_core::types::DddDtoMember,
        module_file: &str,
        shape_by_name: &BTreeMap<String, &DddDtoShapeNode>,
        module_of: &BTreeMap<String, String>,
        codelist_imports: &mut BTreeSet<String>,
        boundary_imports: &mut BTreeMap<(String, String), ()>,
    ) -> Result<String> {
        use crate::ddd::signature::{DddSignatureType, signature_type};
        let refuse = |what: &str| {
            Error::Config(format!(
                "ddd — modeled DTO boundary `{boundary}` member `{}` types {what}, which \
                 the boundary generator cannot express; use a primitive, a codelist, a \
                 class reference, or another modeled boundary",
                member.name
            ))
        };
        match signature_type(&member.type_json) {
            DddSignatureType::Domain(type_ref) => {
                match &type_ref {
                    phero_ir::TypeRef::Primitive(_) => {}
                    // A class reference is a stored FK: the boundary carries
                    // the id, exactly like the generated DTOs' `_id` columns.
                    phero_ir::TypeRef::Class { .. } => return Ok("uuid::Uuid".to_string()),
                    // A codelist/vocabulary renders the generated enum.
                    phero_ir::TypeRef::Enum { name, .. }
                    | phero_ir::TypeRef::Vocabulary { name, .. } => {
                        codelist_imports.insert(name.clone());
                        return Ok(name.clone());
                    }
                    phero_ir::TypeRef::Interface { .. } => {
                        return Err(refuse("an interface"));
                    }
                    phero_ir::TypeRef::Datatype { .. } => {}
                }
                crate::ddd::design::phero_type_to_rust(&type_ref).ok_or_else(|| {
                    refuse(&format!(
                        "the datatype `{}`",
                        type_ref.qualified_name().unwrap_or_default()
                    ))
                })
            }
            DddSignatureType::Dto(name) => {
                if !shape_by_name.contains_key(&name) {
                    return Err(Error::Config(format!(
                        "ddd — modeled DTO boundary member `{}` references boundary `{name}`, \
                         which the model does not declare",
                        member.name
                    )));
                }
                let target_module = module_of.get(&name).cloned().unwrap_or_default();
                if target_module == module_file {
                    Ok(name)
                } else {
                    boundary_imports.insert((target_module, name.clone()), ());
                    Ok(name)
                }
            }
            DddSignatureType::Unresolvable(reason) => Err(refuse(&format!(
                "an unresolvable signature slot ({reason})"
            ))),
        }
    }
}

/// The generated entity DTO type names a boundary must not shadow.
async fn reserved_dto_names(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    generation_order: &[crate::GenerationEntry],
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for entry in generation_order {
        let entity = db
            .get_schema_in_domain(&entry.schema_title, &entry.domain)
            .await
            .ok()
            .flatten()
            .map(|s| s.rust_type_name)
            .unwrap_or_else(|| {
                codegraph_naming::to_pascal_case(&config.defaults.strip_suffix(&entry.schema_title))
            });
        for name in [
            format!("{entity}Response"),
            format!("{entity}LinkedResponse"),
            format!("Create{entity}Request"),
            format!("Update{entity}Request"),
        ] {
            names.insert(name);
        }
    }
    names
}

/// The authored source class name of a boundary (`nz.example.library.Book` →
/// `Book`), for doc comments.
fn shape_class_name(shape: &DddDtoShapeNode) -> Option<String> {
    let source = shape.source_json.as_ref()?;
    let value = source.get("value")?;
    value
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Resolve a boundary's source class to its `(domain, schema title)`, trying
/// the exact leaf and the configured `+type_suffix` spelling in every domain.
async fn resolve_source_entity(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    shape: &DddDtoShapeNode,
) -> Option<(String, String)> {
    let leaf = shape_class_name(shape)?;
    let suffixed = format!("{leaf}{}", config.defaults.type_suffix);
    for domain in config.domains.keys() {
        for candidate in [&leaf, &suffixed] {
            if db
                .get_schema_in_domain(candidate, domain)
                .await
                .ok()
                .flatten()
                .is_some()
            {
                return Some((domain.clone(), candidate.clone()));
            }
        }
    }
    None
}

/// Wrap a base type by the phero-ir multiplicity: `[0..1]` → `Option<T>`,
/// a collection → `Vec<T>`, a plain single → `T`.
fn wrap_multiplicity(
    base: &str,
    multiplicity_json: Option<&serde_json::Value>,
) -> std::result::Result<(String, bool), String> {
    let Some(json) = multiplicity_json else {
        return Ok((base.to_string(), false));
    };
    let multiplicity: phero_ir::Multiplicity = serde_json::from_value(json.clone())
        .map_err(|e| format!("unresolvable multiplicity ({e})"))?;
    if multiplicity.is_many() {
        Ok((format!("Vec<{base}>"), false))
    } else if multiplicity.lower == 0 {
        Ok((format!("Option<{base}>"), true))
    } else {
        Ok((base.to_string(), false))
    }
}
