//! Generator capability entries for IFML generators.
//!
//! These entries are merged into the main capability registry in `profile.rs`.
//! When adding new IFML generators, add their capability descriptor here.

use crate::profile::GeneratorCapability;

/// Returns the list of IFML generator capabilities for registration.
pub fn ifml_capabilities() -> Vec<GeneratorCapability> {
    use crate::profile::{GeneratorKind, GeneratorTarget};

    vec![
        GeneratorCapability {
            name: "ifml_route".to_string(),
            kind: GeneratorKind::Entity,
            target: GeneratorTarget::Ui,
            features_required: vec!["ifml_backend".to_string()],
            features_optional: vec![],
        },
        GeneratorCapability {
            name: "ifml_navigation".to_string(),
            kind: GeneratorKind::Global,
            target: GeneratorTarget::Ui,
            features_required: vec!["ifml_backend".to_string()],
            features_optional: vec![],
        },
    ]
}
