//! `generate_tests` integration test target, split into submodules (issue #377).
//! The target name is preserved: `cargo test -p codegraph --test generate_tests`.

mod ddl;
mod entity_dto;
mod fixtures;
mod handlers;
mod misc;
mod ordering;
mod repository;
mod required_fk;
