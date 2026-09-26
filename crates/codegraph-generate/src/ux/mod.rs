//! UX rules plane (ux-rules epic, phase 2 — issues #295/#296).
//!
//! "Form follows data": every column slot gets a [`Dimension`] inferred
//! from the graph metadata already collected into [`UiField`] plus the
//! optional graph [`PropertyNode`] (Pass 1, [`dimension`]); rules then pin
//! presentation, ordering, actions and visuals into a [`UxPlan`] (Passes
//! 2–4, [`plan`]), with advisory [`UxDiagnostics`] on the side. The plan
//! is pure data consumed by the UI/IFML generators (#297+); with the
//! `ux_rules` feature off, callers pass `None` rules and nothing changes.

pub mod diagnostics;
pub mod dimension;
pub mod plan;
pub mod sort;

pub use diagnostics::{collect_diagnostics, report, UxDiagnostics};
pub use dimension::{
    infer_dimension, infer_dimension_with_hints, infer_sub_field_dimensions, DimensionHints,
    SubFieldDimension,
};
pub use plan::{
    build_ux_plan, ids, ActionPlan, ActionSpec, CollectionPlan, ColumnPlan, RowAction, RowVisuals,
    UxPlan, UxPlanInput, VerticalAlign,
};
pub use sort::{
    apply_list_scope, collect_ux_plan_context, list_order_is_pinned, resolve_ux_sort_plan,
    sort_plan_from_plan, UxSortPlan,
};
