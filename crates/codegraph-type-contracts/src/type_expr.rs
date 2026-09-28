use serde::{Deserialize, Serialize};

use crate::projection::DddFieldProjection;
use crate::resolved::RefClassificationKind;
use crate::{PgType, RustType};

/// A composable abstract type expression (Morphir-inspired): property types
/// are expressions over references, primitives, and containers rather than
/// frozen per-target strings.
///
/// Lowerings to per-target types ([`PgType`], [`RustType`], proto) are pure
/// functions that return a named [`TypeExprError`] for shapes a target
/// cannot represent — never a silent fallback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeExpr {
    /// Reference to another schema type. `kind` carries the ingest-time
    /// classification; when absent, `target` may still resolve if it names a
    /// known primitive. `args` are Morphir-style reference parameters.
    Ref {
        target: String,
        kind: Option<RefClassificationKind>,
        args: Vec<TypeExpr>,
    },
    /// A built-in primitive (`"string"`, `"int64"`, `"datetime"`, ...).
    Primitive {
        name: String,
    },
    List(Box<TypeExpr>),
    Optional(Box<TypeExpr>),
    /// Reference to a codelist (lookup table / enum).
    Codelist {
        code_list: String,
    },
    /// Free-form record — lowers to JSONB / serde_json::Value / Struct.
    Record {
        fields: Vec<(String, TypeExpr)>,
    },
}

/// Errors from lowering a [`TypeExpr`] to a per-target type.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TypeExprError {
    #[error("cannot lower {expr} to a {target} type: {reason}")]
    Unsupported {
        target: &'static str,
        expr: String,
        reason: String,
    },
    #[error("unknown primitive name '{name}'")]
    UnknownPrimitive { name: String },
}

/// A primitive and its closed per-target mappings. The canonical Rust string
/// is the exact frozen `rust_field_type` spelling, so proto lowerings via
/// this string are byte-identical with the legacy string path.
struct PrimitiveDef {
    name: &'static str,
    pg: PgType,
    rust: RustType,
    rust_str: &'static str,
}

const PRIMITIVES: &[PrimitiveDef] = &[
    p("string", PgType::Text, RustType::String, "String"),
    p("text", PgType::Text, RustType::String, "String"),
    p("uuid", PgType::Uuid, RustType::Uuid, "Uuid"),
    p("boolean", PgType::Boolean, RustType::Bool, "bool"),
    p("bool", PgType::Boolean, RustType::Bool, "bool"),
    p("int16", PgType::SmallInt, RustType::I16, "i16"),
    p("smallint", PgType::SmallInt, RustType::I16, "i16"),
    p("int32", PgType::Integer, RustType::I32, "i32"),
    p("integer", PgType::Integer, RustType::I32, "i32"),
    p("int", PgType::Integer, RustType::I32, "i32"),
    p("int64", PgType::BigInt, RustType::I64, "i64"),
    p("bigint", PgType::BigInt, RustType::I64, "i64"),
    p("long", PgType::BigInt, RustType::I64, "i64"),
    p("float32", PgType::Real, RustType::F32, "f32"),
    p("float", PgType::Real, RustType::F32, "f32"),
    p("float64", PgType::DoublePrecision, RustType::F64, "f64"),
    p("double", PgType::DoublePrecision, RustType::F64, "f64"),
    p("number", PgType::DoublePrecision, RustType::F64, "f64"),
    p(
        "decimal",
        PgType::Numeric {
            precision: 19,
            scale: 4,
        },
        RustType::Decimal,
        "rust_decimal::Decimal",
    ),
    p(
        "date",
        PgType::Date,
        RustType::NaiveDate,
        "chrono::NaiveDate",
    ),
    p(
        "datetime",
        PgType::Timestamptz,
        RustType::DateTimeUtc,
        "chrono::DateTime<chrono::Utc>",
    ),
    p(
        "date-time",
        PgType::Timestamptz,
        RustType::DateTimeUtc,
        "chrono::DateTime<chrono::Utc>",
    ),
    p(
        "timestamp",
        PgType::Timestamptz,
        RustType::DateTimeUtc,
        "chrono::DateTime<chrono::Utc>",
    ),
    p("json", PgType::Jsonb, RustType::Json, "serde_json::Value"),
];

const fn p(name: &'static str, pg: PgType, rust: RustType, rust_str: &'static str) -> PrimitiveDef {
    PrimitiveDef {
        name,
        pg,
        rust,
        rust_str,
    }
}

fn primitive_def(name: &str) -> Result<&'static PrimitiveDef, TypeExprError> {
    PRIMITIVES
        .iter()
        .find(|d| d.name == name)
        .ok_or_else(|| TypeExprError::UnknownPrimitive {
            name: name.to_string(),
        })
}

/// Inverse of the primitive map: the frozen `rust_field_type` spelling a
/// property carries at ingest, when it corresponds to a known primitive.
fn primitive_name_from_rust_str(rust: &str) -> Option<&'static str> {
    PRIMITIVES
        .iter()
        .find(|d| d.rust_str == rust)
        .map(|d| d.name)
}

fn pg_inverse(pg: &PgType) -> Option<&'static str> {
    match pg {
        PgType::Text => Some("string"),
        PgType::Uuid => Some("uuid"),
        PgType::Boolean => Some("boolean"),
        PgType::SmallInt => Some("int16"),
        PgType::Integer => Some("int32"),
        PgType::BigInt => Some("int64"),
        PgType::Numeric { .. } => Some("decimal"),
        PgType::Real => Some("float32"),
        PgType::DoublePrecision => Some("float64"),
        PgType::Date => Some("date"),
        PgType::Timestamptz => Some("datetime"),
        PgType::Jsonb => Some("json"),
        _ => None,
    }
}

fn unsupported(target: &'static str, expr: &TypeExpr, reason: &str) -> TypeExprError {
    TypeExprError::Unsupported {
        target,
        expr: format!("{expr:?}"),
        reason: reason.to_string(),
    }
}

fn array_pg(inner: &PgType) -> Result<PgType, TypeExprError> {
    match inner {
        PgType::Text => Ok(PgType::TextArray),
        PgType::BigInt => Ok(PgType::BigIntArray),
        PgType::Integer => Ok(PgType::IntegerArray),
        PgType::DoublePrecision => Ok(PgType::DoubleArray),
        PgType::Boolean => Ok(PgType::BoolArray),
        other => Err(TypeExprError::Unsupported {
            target: "pg",
            expr: format!("List({other:?})"),
            reason: "no closed Postgres array variant for the element type".to_string(),
        }),
    }
}

fn vec_rust(inner: RustType) -> RustType {
    match inner {
        RustType::String => RustType::VecString,
        RustType::I64 => RustType::VecI64,
        RustType::I32 => RustType::VecI32,
        RustType::F64 => RustType::VecF64,
        RustType::Bool => RustType::VecBool,
        other => RustType::DomainType(format!("Vec<{}>", other.as_rust_str())),
    }
}

impl TypeExpr {
    /// Lower to the Postgres column type. Nullability is column-level
    /// metadata (never part of the column type), so `Optional` is
    /// type-preserving here.
    pub fn lower_pg(&self) -> Result<PgType, TypeExprError> {
        match self {
            TypeExpr::Primitive { name } => Ok(primitive_def(name)?.pg.clone()),
            TypeExpr::Ref { target, kind, args } => self.lower_ref_pg(target, kind.as_ref(), args),
            TypeExpr::Codelist { .. } => Ok(PgType::Text),
            TypeExpr::List(inner) => {
                if matches!(inner.as_ref(), TypeExpr::Record { .. }) {
                    return Err(unsupported(
                        "pg",
                        self,
                        "Postgres has no array-of-record column type",
                    ));
                }
                if matches!(inner.as_ref(), TypeExpr::List(_)) {
                    return Err(unsupported(
                        "pg",
                        self,
                        "nested lists are not representable",
                    ));
                }
                array_pg(&inner.lower_pg()?)
            }
            TypeExpr::Optional(inner) => inner.lower_pg(),
            TypeExpr::Record { .. } => Ok(PgType::Jsonb),
        }
    }

    fn lower_ref_pg(
        &self,
        target: &str,
        kind: Option<&RefClassificationKind>,
        args: &[TypeExpr],
    ) -> Result<PgType, TypeExprError> {
        if !args.is_empty() {
            return Err(unsupported(
                "pg",
                self,
                "parameterized references carry no column-level semantics yet",
            ));
        }
        match kind {
            Some(RefClassificationKind::EntityReference) => Ok(PgType::Uuid),
            Some(
                RefClassificationKind::CodelistReference | RefClassificationKind::CodelistCheck,
            ) => Ok(PgType::Text),
            Some(RefClassificationKind::PrimitiveWrapper) => primitive_def(target)
                .map_err(|_| {
                    unsupported(
                        "pg",
                        self,
                        "wrapper references have no scalar column without the resolved primitive",
                    )
                })
                .map(|d| d.pg.clone()),
            Some(other) => Err(unsupported(
                "pg",
                self,
                &format!("{other:?} has no single-column representation"),
            )),
            None => primitive_def(target).map(|d| d.pg.clone()).map_err(|_| {
                unsupported("pg", self, "untyped reference to a non-primitive target")
            }),
        }
    }

    /// Lower to the canonical Rust type. Optionality is applied by callers
    /// (from `is_required`), mirroring `DddFieldProjection::format_rust_type`.
    pub fn lower_rust(&self) -> Result<RustType, TypeExprError> {
        match self {
            TypeExpr::Primitive { name } => Ok(primitive_def(name)?.rust.clone()),
            TypeExpr::Ref { target, kind, args } => {
                self.lower_ref_rust(target, kind.as_ref(), args)
            }
            TypeExpr::Codelist { .. } => Ok(RustType::String),
            TypeExpr::List(inner) => Ok(vec_rust(inner.lower_rust()?)),
            TypeExpr::Optional(inner) => inner.lower_rust(),
            TypeExpr::Record { .. } => Ok(RustType::Json),
        }
    }

    fn lower_ref_rust(
        &self,
        target: &str,
        kind: Option<&RefClassificationKind>,
        args: &[TypeExpr],
    ) -> Result<RustType, TypeExprError> {
        if !args.is_empty() {
            return Err(unsupported(
                "rust",
                self,
                "parameterized references carry no domain-level semantics yet",
            ));
        }
        match kind {
            Some(RefClassificationKind::EntityReference) => Ok(RustType::Uuid),
            Some(
                RefClassificationKind::CodelistReference | RefClassificationKind::CodelistCheck,
            ) => Ok(RustType::String),
            Some(RefClassificationKind::PrimitiveWrapper) => {
                Ok(RustType::DomainType(target.to_string()))
            }
            Some(other) => Err(unsupported(
                "rust",
                self,
                &format!("{other:?} has no scalar domain type"),
            )),
            None => primitive_def(target).map(|d| d.rust.clone()).map_err(|_| {
                unsupported("rust", self, "untyped reference to a non-primitive target")
            }),
        }
    }

    /// The canonical frozen Rust string for a primitive, for consumers that
    /// route through existing string-based mappings (keeps output identical).
    pub fn canonical_rust_str(&self) -> Result<String, TypeExprError> {
        match self {
            TypeExpr::Primitive { name } => Ok(primitive_def(name)?.rust_str.to_string()),
            _ => Err(unsupported(
                "rust",
                self,
                "only primitives carry a canonical Rust string",
            )),
        }
    }

    /// Derive a `TypeExpr` from the data an ingest site has already frozen:
    /// the classification kind, the ref target, and the frozen Rust field
    /// type. Returns `None` when the frozen data cannot be expressed
    /// structurally without changing semantics — callers keep the legacy
    /// strings populated either way.
    pub fn from_frozen(
        kind: &RefClassificationKind,
        ref_target: Option<&str>,
        rust_field_type: &str,
    ) -> Option<TypeExpr> {
        match kind {
            RefClassificationKind::PrimitiveWrapper | RefClassificationKind::ArrayWrapper => {
                let inner = rust_field_type
                    .strip_prefix("Vec<")
                    .and_then(|s| s.strip_suffix('>'))
                    .unwrap_or(rust_field_type);
                primitive_name_from_rust_str(inner).map(|name| {
                    if inner == rust_field_type {
                        TypeExpr::Primitive {
                            name: name.to_string(),
                        }
                    } else {
                        TypeExpr::List(Box::new(TypeExpr::Primitive {
                            name: name.to_string(),
                        }))
                    }
                })
            }
            RefClassificationKind::CodelistReference
            | RefClassificationKind::CodelistCheck
            | RefClassificationKind::InlineEnum => ref_target.map(|target| TypeExpr::Codelist {
                code_list: target.to_string(),
            }),
            RefClassificationKind::EntityReference
            | RefClassificationKind::ValueObject
            | RefClassificationKind::CompositeWrapper
            | RefClassificationKind::MediaWrapper
            | RefClassificationKind::StructuredWrapper
            | RefClassificationKind::RangeWrapper => ref_target.map(|target| TypeExpr::Ref {
                target: target.to_string(),
                kind: Some(kind.clone()),
                args: vec![],
            }),
        }
    }

    /// Derive a `TypeExpr` from a [`DddFieldProjection`] — the precedence-2
    /// source for graphs whose properties predate `type_expr`. Ambiguous
    /// shapes (composite/media and range wrappers are indistinguishable in a
    /// projection) yield `None` so consumers keep the legacy string path.
    pub fn from_projection(projection: &DddFieldProjection) -> Option<TypeExpr> {
        let entity = match &projection.entity {
            crate::projection::EntityProjection::CompositeColumns { .. } => return None,
            crate::projection::EntityProjection::SingleColumn {
                column_type,
                is_fk,
                fk_target,
                ..
            } => (column_type.pg(), *is_fk, fk_target.as_ref()),
        };
        let (pg, is_fk, fk_target) = entity;

        if matches!(
            pg,
            PgType::Int4Range
                | PgType::Int8Range
                | PgType::TstzRange
                | PgType::DateRange
                | PgType::Bytea
        ) {
            return None;
        }

        if is_fk {
            return match (&projection.dto.response, fk_target) {
                (crate::projection::DtoFieldType::Codelist { enum_name }, _)
                    if !enum_name.is_empty() =>
                {
                    Some(TypeExpr::Codelist {
                        code_list: enum_name.clone(),
                    })
                }
                (crate::projection::DtoFieldType::EntityRef { entity_name }, _)
                    if !entity_name.is_empty() =>
                {
                    Some(TypeExpr::Ref {
                        target: entity_name.clone(),
                        kind: Some(RefClassificationKind::EntityReference),
                        args: vec![],
                    })
                }
                _ => None,
            };
        }

        match pg {
            PgType::Jsonb => match &projection.dto.response {
                crate::projection::DtoFieldType::Scalar(RustType::DomainType(name)) => {
                    Some(TypeExpr::Ref {
                        target: name.clone(),
                        kind: Some(RefClassificationKind::StructuredWrapper),
                        args: vec![],
                    })
                }
                crate::projection::DtoFieldType::NestedDto => Some(TypeExpr::Ref {
                    target: String::new(),
                    kind: Some(RefClassificationKind::ValueObject),
                    args: vec![],
                }),
                _ => None,
            },
            PgType::TextArray => Some(TypeExpr::List(Box::new(TypeExpr::Primitive {
                name: "string".to_string(),
            }))),
            PgType::BigIntArray => Some(TypeExpr::List(Box::new(TypeExpr::Primitive {
                name: "int64".to_string(),
            }))),
            PgType::IntegerArray => Some(TypeExpr::List(Box::new(TypeExpr::Primitive {
                name: "int32".to_string(),
            }))),
            PgType::DoubleArray => Some(TypeExpr::List(Box::new(TypeExpr::Primitive {
                name: "float64".to_string(),
            }))),
            PgType::BoolArray => Some(TypeExpr::List(Box::new(TypeExpr::Primitive {
                name: "boolean".to_string(),
            }))),
            other => pg_inverse(other).map(|name| TypeExpr::Primitive {
                name: name.to_string(),
            }),
        }
    }
}
