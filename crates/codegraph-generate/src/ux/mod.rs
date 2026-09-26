//! UX dimension inference (ux-rules epic, phase 2 pass 1 — issue #295).
//!
//! "Form follows data": every column slot gets a [`Dimension`] inferred
//! from the graph metadata already collected into [`UiField`] plus the
//! optional graph [`PropertyNode`]. The inference is pure and table-driven
//! testable; rule resolution over the inferred dimension happens in a later
//! sub-task.

pub mod dimension;

pub use dimension::{
    infer_dimension, infer_dimension_with_hints, infer_sub_field_dimensions, DimensionHints,
    SubFieldDimension,
};
