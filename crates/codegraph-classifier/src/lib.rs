pub mod classify;
pub mod config;
pub mod projection_builder;

use codegraph_type_contracts::{ColumnType, DddFieldProjection, RefClassificationKind};

/// Result of classifying a $ref target or schema property.
/// All fields are typed — no stringly-typed type triples.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClassificationResult {
    pub kind: RefClassificationKind,
    pub column_type: Option<ColumnType>,
    pub projection: DddFieldProjection,
    pub open_end: bool,
}
// Note: `render_strategy` is removed. `RefClassificationKind` fully supersedes it.
// Note: `classification` string is removed. `kind` is the typed replacement.
// Note: `pg_type`, `rust_type`, `sea_orm_type` strings are removed. Live in `column_type` and `projection`.
// Note: `columns` is removed. Composite column data lives in `projection.entity` as `CompositeColumns`.
