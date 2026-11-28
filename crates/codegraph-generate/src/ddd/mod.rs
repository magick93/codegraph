pub mod aggregate;
pub mod body_lower;
pub mod body_params;
pub mod boundary_derive;
pub mod boundary_map;
pub mod command;
pub mod cornucopia_repo;
pub mod cornucopia_repo_include;
pub mod design;
pub mod dto;
pub mod dto_boundaries;
mod dto_fields;
pub mod errors;
pub mod event;
pub mod functions;
pub mod functions_api;
pub mod functions_sql;
pub mod invariants;
pub mod kernel_probe;
pub mod loaders;
pub mod mox_ops;
pub mod query;
pub mod regulatory_report;
pub mod repository;
pub mod repository_emitter;
pub mod repository_query;
pub mod rules;
pub mod service_ops;
pub mod signature;
pub mod validations;

#[cfg(test)]
mod design_generation_tests;

#[cfg(test)]
mod refers_junction_tests;

#[cfg(test)]
mod repository_error_type_tests;
