//! Rosetta gap-analysis probe tests (issue #254).
//!
//! Characterization tests pinning real codegraph ingest/generator behavior
//! for each open verification question in `docs/rosetta-gap-analysis.md`.
//! If a probe fails against its expected-current behavior, the surprise IS
//! the finding: record it in `docs/rosetta/findings/`, do not force the
//! test green.
//!
//! Sigil is a TEST-ONLY dependency here (production wiring lands in #255).

#![allow(dead_code)]

#[path = "rosetta_gap_probes/support.rs"]
mod support;

#[path = "rosetta_gap_probes/inheritance_probe.rs"]
mod inheritance_probe;

#[path = "rosetta_gap_probes/cardinality_probe.rs"]
mod cardinality_probe;

#[path = "rosetta_gap_probes/choice_probe.rs"]
mod choice_probe;

#[path = "rosetta_gap_probes/meta_probe.rs"]
mod meta_probe;

#[path = "rosetta_gap_probes/namespace_probe.rs"]
mod namespace_probe;

#[path = "rosetta_gap_probes/expr_json_probe.rs"]
mod expr_json_probe;

#[path = "rosetta_gap_probes/data_plane_probe.rs"]
mod data_plane_probe;
