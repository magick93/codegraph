// Generated crate — do not edit.
#![allow(clippy::module_inception, unused_imports, ambiguous_glob_reexports)]

pub mod codelist;
pub mod context;
pub mod query;

pub use context::{SourceContext, SourceOrigin};
pub use query::{ListParams, PagedResult, QueryError, SortOrder};

// --- GENERATED DOMAIN MODULES ---
pub mod assessments;
pub mod benefits;
pub mod common;
pub mod compensation;
pub mod interviewing;
pub mod payroll;
pub mod recruiting;
pub mod screening;
pub mod timecard;
pub mod wellness;
