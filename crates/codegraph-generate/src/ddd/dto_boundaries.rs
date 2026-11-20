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
    /// A `Vec<T>` member: serde-required by default, so an empty list is a
    /// valid minimal body — `#[serde(default)]` keeps a caller that omits it
    /// working too (review R1).
    is_vec: bool,
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

/// Index of the graph's modeled DTO boundaries by boundary name (DD3, issue
/// #667). Boundary names are application-unique upstream, so one map is
/// enough; consumers resolve a `DddTypeRef::Dto` slot to the generated Rust
/// path without re-reading the models.
#[derive(Debug, Default, Clone)]
pub struct DtoBoundaryIndex {
    shapes: BTreeMap<String, DddDtoShapeNode>,
    /// Minimal valid INPUT bodies in the snake-case wire contract.
    snake_samples: BTreeMap<String, serde_json::Value>,
    /// Minimal valid INPUT bodies in the camelCase wire contract.
    camel_samples: BTreeMap<String, serde_json::Value>,
}

impl DtoBoundaryIndex {
    /// Build the index from every ingested `.ddd` model (no graph access:
    /// codelist samples fall back to a string, which the caller's fixtures
    /// own). Used where only the shapes are needed (OpenAPI registration).
    pub fn from_models(models: &[codegraph_core::types::DddModelGraph]) -> Self {
        let shapes: BTreeMap<String, DddDtoShapeNode> = models
            .iter()
            .flat_map(|model| model.dto_shapes.iter())
            .map(|shape| (shape.name.clone(), shape.clone()))
            .collect();
        let mut index = Self {
            shapes,
            ..Default::default()
        };
        let names: Vec<String> = index.shapes.keys().cloned().collect();
        for name in names {
            for camel in [false, true] {
                let sample = blocking_sample(&index.shapes, &name, camel, None, 0);
                index.store_sample(&name, camel, sample);
            }
        }
        index
    }

    /// Build the index with graph-backed samples (codelist values resolved
    /// through `get_enum_values`), used by the action-body plane.
    pub async fn from_graph(
        db: &dyn GraphQuerier,
        config: &DomainConfig,
    ) -> std::result::Result<Self, codegraph_core::error::GraphError> {
        let models = db.get_ddd_models().await?;
        let shapes: BTreeMap<String, DddDtoShapeNode> = models
            .iter()
            .flat_map(|model| model.dto_shapes.iter())
            .map(|shape| (shape.name.clone(), shape.clone()))
            .collect();
        let mut index = Self {
            shapes,
            ..Default::default()
        };
        let names: Vec<String> = index.shapes.keys().cloned().collect();
        for name in names {
            for camel in [false, true] {
                let sample = index
                    .graph_sample(&name, camel, db, config, 0)
                    .await
                    .unwrap_or(serde_json::Value::Null);
                index.store_sample(&name, camel, sample);
            }
        }
        Ok(index)
    }

    fn store_sample(&mut self, name: &str, camel: bool, sample: serde_json::Value) {
        if camel {
            self.camel_samples.insert(name.to_string(), sample);
        } else {
            self.snake_samples.insert(name.to_string(), sample);
        }
    }

    /// The minimal valid INPUT body for a boundary in the project's wire
    /// casing — what the emitted hurl contract sends.
    pub fn sample(&self, name: &str, camel: bool) -> Option<&serde_json::Value> {
        if camel {
            self.camel_samples.get(name)
        } else {
            self.snake_samples.get(name)
        }
    }

    async fn graph_sample(
        &self,
        name: &str,
        camel: bool,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        depth: usize,
    ) -> std::result::Result<serde_json::Value, codegraph_core::error::GraphError> {
        let Some(shape) = self.shapes.get(name) else {
            return Ok(serde_json::Value::Null);
        };
        let mut object = serde_json::Map::new();
        for member in &shape.members {
            if member.kind == "derive" {
                continue;
            }
            let multiplicity = member.multiplicity_json.as_ref().and_then(|json| {
                serde_json::from_value::<phero_ir::Multiplicity>(json.clone()).ok()
            });
            // A non-many optional member is omitted (the minimal body); a
            // many-valued one is ALWAYS present (the struct's `Vec` field is
            // not `Option`) — empty when the lower bound is 0.
            if multiplicity
                .as_ref()
                .is_some_and(|m| m.lower == 0 && !m.is_many())
            {
                continue;
            }
            let Ok(one) = async {
                use crate::ddd::signature::{DddSignatureType, signature_type};
                match signature_type(&member.type_json) {
                    DddSignatureType::Domain(phero_ir::TypeRef::Primitive(primitive)) => {
                        Ok(primitive_sample(&primitive))
                    }
                    DddSignatureType::Domain(phero_ir::TypeRef::Class { name, .. }) => {
                        Ok(class_sample(db, config, &name).await)
                    }
                    DddSignatureType::Domain(phero_ir::TypeRef::Enum { name, .. })
                    | DddSignatureType::Domain(phero_ir::TypeRef::Vocabulary { name, .. }) => {
                        let values = db.get_enum_values(&name).await?;
                        Ok(values
                            .first()
                            .map(|v| serde_json::json!(v.value))
                            .unwrap_or(serde_json::json!("contract-sample")))
                    }
                    DddSignatureType::Domain(phero_ir::TypeRef::Datatype { name, .. }) => {
                        Ok(named_sample(&name))
                    }
                    DddSignatureType::Domain(_) => Ok(serde_json::json!("contract-sample")),
                    DddSignatureType::Dto(nested) => {
                        if depth >= 4 || nested == shape.name {
                            return Ok(serde_json::json!({}));
                        }
                        Box::pin(self.graph_sample(&nested, camel, db, config, depth + 1)).await
                    }
                    DddSignatureType::Unresolvable(_) => Ok(serde_json::Value::Null),
                }
            }
            .await
            else {
                continue;
            };
            let value = match multiplicity.as_ref() {
                Some(m) if m.is_many() && m.lower == 0 => serde_json::json!([]),
                Some(m) if m.is_many() => serde_json::json!([one]),
                _ => one,
            };
            object.insert(wire_key(&member.name, camel), value);
        }
        Ok(serde_json::Value::Object(object))
    }

    /// The boundary with this name, when the model declares one.
    pub fn get(&self, name: &str) -> Option<&DddDtoShapeNode> {
        self.shapes.get(name)
    }

    /// Every declared boundary, in deterministic (name) order.
    pub fn shapes(&self) -> impl Iterator<Item = &DddDtoShapeNode> {
        self.shapes.values()
    }

    /// `true` when the model declares no boundaries.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// A direct index over shapes — crate-internal unit-test constructor.
    #[cfg(test)]
    pub(crate) fn for_tests(shapes: Vec<DddDtoShapeNode>) -> Self {
        let shapes: BTreeMap<String, DddDtoShapeNode> = shapes
            .into_iter()
            .map(|shape| (shape.name.clone(), shape))
            .collect();
        let mut index = Self {
            shapes,
            ..Default::default()
        };
        let names: Vec<String> = index.shapes.keys().cloned().collect();
        for name in names {
            for camel in [false, true] {
                let sample = blocking_sample(&index.shapes, &name, camel, None, 0);
                index.store_sample(&name, camel, sample);
            }
        }
        index
    }

    /// The fully-qualified Rust path of the generated boundary type in the
    /// domain-types crate (`{crate}::dto::{module}::{Name}`).
    pub fn rust_path(&self, name: &str, domain_types_crate: &str) -> Option<String> {
        let shape = self.get(name)?;
        Some(format!(
            "{domain_types_crate}::dto::{}::{}",
            codegraph_naming::to_snake_case(&shape.module),
            shape.name
        ))
    }
}

/// The wire key of a boundary member: the generated struct's `rename_all`
/// casing, never the authored spelling (the #667 review finding: the hurl
/// body must deserialize into the struct it addresses).
fn wire_key(name: &str, camel: bool) -> String {
    if camel {
        use heck::ToLowerCamelCase;
        name.to_lower_camel_case()
    } else {
        codegraph_naming::to_snake_case(name)
    }
}

fn primitive_sample(primitive: &phero_ir::PrimitiveType) -> serde_json::Value {
    use phero_ir::PrimitiveType;
    match primitive {
        PrimitiveType::String | PrimitiveType::Char => serde_json::json!("contract-sample"),
        PrimitiveType::Boolean => serde_json::json!(true),
        PrimitiveType::Int | PrimitiveType::Short | PrimitiveType::Byte => serde_json::json!(42),
        PrimitiveType::Long => serde_json::json!(42),
        PrimitiveType::Float | PrimitiveType::Double => serde_json::json!(42.0),
        PrimitiveType::Date => serde_json::json!("2026-01-01"),
    }
}

/// The sample for a named `.mox` datatype (`Uuid`/`DateTime`/`Decimal`/`Date`
/// and their wraps) — the wire spellings the generated DTO field deserializes.
fn named_sample(name: &str) -> serde_json::Value {
    match name {
        "Uuid" => serde_json::json!("00000000-0000-0000-0000-000000000000"),
        "DateTime" | "Instant" | "Timestamp" => serde_json::json!("2026-01-01T00:00:00Z"),
        "Date" => serde_json::json!("2026-01-01"),
        // `serde-with-str` in the domain-types crate: Decimal arrives as a string.
        "Decimal" => serde_json::json!("1.0"),
        _ => serde_json::json!("contract-sample"),
    }
}

/// A class-reference sample: an entity ref is the FK id; an unresolvable
/// class is still a Uuid (generation refuses non-entity refs, so only an
/// entity can reach a contract).
async fn class_sample(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    class: &str,
) -> serde_json::Value {
    let _ = (db, config, class);
    serde_json::json!("00000000-0000-0000-0000-000000000000")
}

/// Sync sample builder over bare shapes (no graph): codelist values fall back
/// to a string, nested boundaries recurse with a depth guard.
fn blocking_sample(
    shapes: &BTreeMap<String, DddDtoShapeNode>,
    name: &str,
    camel: bool,
    _db: Option<&dyn GraphQuerier>,
    depth: usize,
) -> serde_json::Value {
    let Some(shape) = shapes.get(name) else {
        return serde_json::Value::Null;
    };
    let mut object = serde_json::Map::new();
    for member in &shape.members {
        if member.kind == "derive" {
            continue;
        }
        let multiplicity = member
            .multiplicity_json
            .as_ref()
            .and_then(|json| serde_json::from_value::<phero_ir::Multiplicity>(json.clone()).ok());
        if multiplicity
            .as_ref()
            .is_some_and(|m| m.lower == 0 && !m.is_many())
        {
            continue;
        }
        use crate::ddd::signature::{DddSignatureType, signature_type};
        let one = match signature_type(&member.type_json) {
            DddSignatureType::Domain(phero_ir::TypeRef::Primitive(primitive)) => {
                primitive_sample(&primitive)
            }
            DddSignatureType::Domain(phero_ir::TypeRef::Class { .. }) => {
                serde_json::json!("00000000-0000-0000-0000-000000000000")
            }
            DddSignatureType::Domain(phero_ir::TypeRef::Datatype { name, .. }) => {
                named_sample(&name)
            }
            DddSignatureType::Domain(_) | DddSignatureType::Unresolvable(_) => {
                serde_json::json!("contract-sample")
            }
            DddSignatureType::Dto(nested) => {
                if depth >= 4 || nested == shape.name {
                    serde_json::json!({})
                } else {
                    blocking_sample(shapes, &nested, camel, _db, depth + 1)
                }
            }
        };
        let value = match multiplicity.as_ref() {
            Some(m) if m.is_many() && m.lower == 0 => serde_json::json!([]),
            Some(m) if m.is_many() => serde_json::json!([one]),
            _ => one,
        };
        object.insert(wire_key(&member.name, camel), value);
    }
    serde_json::Value::Object(object)
}

/// Resolution context for one boundary file: the sibling shapes a member can
/// reference, the module each lives in, and the import sets the render
/// context collects. Grouped so member resolution stays a readable arity.
struct MemberLookup<'a> {
    db: &'a dyn GraphQuerier,
    config: &'a DomainConfig,
    module_file: &'a str,
    shape_by_name: &'a BTreeMap<String, &'a DddDtoShapeNode>,
    module_of: &'a BTreeMap<String, String>,
    codelist_imports: &'a mut BTreeSet<String>,
    boundary_imports: &'a mut BTreeMap<(String, String), ()>,
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
                        project,
                        shape,
                        &mut MemberLookup {
                            db,
                            config,
                            module_file: &module_file,
                            shape_by_name: &shape_by_name,
                            module_of: &module_of,
                            codelist_imports: &mut codelist_imports,
                            boundary_imports: &mut boundary_imports,
                        },
                    )
                    .await?;
                boundaries.push(ctx);
            }

            let has_structs = !boundaries.is_empty();
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
        project: &ProjectConfig,
        shape: &DddDtoShapeNode,
        lookup: &mut MemberLookup<'_>,
    ) -> Result<BoundaryCtx> {
        let camel_case =
            project.codegen.dto_key_casing == crate::project_config::DtoKeyCasing::Camel;
        let mut fields = Vec::with_capacity(shape.members.len());
        for member in &shape.members {
            let base = self.member_rust_type(&shape.name, member, lookup).await?;
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
                is_vec: rust_type.starts_with("Vec<"),
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
            source_note: shape_class_name(shape),
            camel_case,
            deny_unknown: shape.direction == "input",
            has_default,
            fields,
        })
    }

    /// Resolve one member's base Rust type (before multiplicity wrapping).
    async fn member_rust_type(
        &self,
        boundary: &str,
        member: &codegraph_core::types::DddDtoMember,
        lookup: &mut MemberLookup<'_>,
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
                    // A class reference to an ENTITY is a stored FK: the
                    // boundary carries the id, exactly like the generated
                    // DTOs' `_id` columns. A contained value object/structured
                    // class is NOT a scalar id — refuse it by name rather
                    // than silently typing an object as a Uuid.
                    phero_ir::TypeRef::Class { name, .. } => {
                        match resolve_class_schema(lookup.db, lookup.config, name).await {
                            Some(schema) if schema.is_entity => {
                                return Ok("uuid::Uuid".to_string());
                            }
                            Some(schema) => {
                                return Err(refuse(&format!(
                                    "the non-entity class `{}` ({}) — a contained value object \
                                     needs its own boundary shape",
                                    name, schema.classification
                                )));
                            }
                            None => {
                                return Err(refuse(&format!(
                                    "the class `{name}`, which resolves to no ingested schema"
                                )));
                            }
                        }
                    }
                    // A codelist/vocabulary renders the generated enum.
                    phero_ir::TypeRef::Enum { name, .. }
                    | phero_ir::TypeRef::Vocabulary { name, .. } => {
                        lookup.codelist_imports.insert(name.clone());
                        return Ok(name.clone());
                    }
                    phero_ir::TypeRef::Interface { .. } => {
                        return Err(refuse("an interface"));
                    }
                    phero_ir::TypeRef::Datatype { .. } => {}
                }
                crate::ddd::design::phero_type_to_rust(&type_ref)
                    .map(|rust| match rust.as_str() {
                        // The shared map returns these two BARE (the trait
                        // plane imports them); the boundary module does not.
                        "Uuid" => "uuid::Uuid".to_string(),
                        "NaiveDate" => "chrono::NaiveDate".to_string(),
                        _ => rust,
                    })
                    .ok_or_else(|| {
                        refuse(&format!(
                            "the datatype `{}`",
                            type_ref.qualified_name().unwrap_or_default()
                        ))
                    })
            }
            DddSignatureType::Dto(name) => {
                if !lookup.shape_by_name.contains_key(&name) {
                    return Err(Error::Config(format!(
                        "ddd — modeled DTO boundary member `{}` references boundary `{name}`, \
                         which the model does not declare",
                        member.name
                    )));
                }
                let target_module = lookup.module_of.get(&name).cloned().unwrap_or_default();
                if target_module == lookup.module_file {
                    Ok(name)
                } else {
                    lookup
                        .boundary_imports
                        .insert((target_module, name.clone()), ());
                    Ok(name)
                }
            }
            DddSignatureType::Unresolvable(reason) => Err(refuse(&format!(
                "an unresolvable signature slot ({reason})"
            ))),
        }
    }
}

/// Resolve a class reference to its ingested schema (exact, then the
/// configured `+type_suffix`). The leaf name is what phero-ir carries.
async fn resolve_class_schema(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    class: &str,
) -> Option<codegraph_core::types::SchemaNode> {
    let suffixed = format!("{class}{}", config.defaults.type_suffix);
    for candidate in [class.to_string(), suffixed] {
        if let Ok(Some(schema)) = db.get_schema(&candidate).await {
            return Some(schema);
        }
    }
    None
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
