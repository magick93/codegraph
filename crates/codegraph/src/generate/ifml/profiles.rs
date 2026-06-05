//! Generator capability entries for IFML generators.
//!
//! These entries are merged into the main capability registry in `profile.rs`.
//! When adding new IFML generators, add their capability descriptor here.

use crate::profile::GeneratorCapability;

/// Returns the list of IFML generator capabilities for registration.
pub fn ifml_capabilities() -> Vec<GeneratorCapability> {
    use crate::profile::{GeneratorKind, GeneratorTarget};

    let mut caps = vec![
        // Base (non-framework-specific) generators
        GeneratorCapability {
            name: "ifml_route".to_string(),
            kind: GeneratorKind::Global,
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
    ];

    // Framework-specific route generators
    for framework in &["svelte", "react", "vue", "flutter", "swiftui"] {
        caps.push(GeneratorCapability {
            name: format!("ifml_route_{framework}"),
            kind: GeneratorKind::Global,
            target: GeneratorTarget::Ui,
            features_required: vec![
                "ifml_backend".to_string(),
                format!("framework_{framework}"),
            ],
            features_optional: vec![],
        });
    }

    // Framework-specific navigation generators
    for framework in &["svelte", "react", "vue", "flutter", "swiftui"] {
        caps.push(GeneratorCapability {
            name: format!("ifml_navigation_{framework}"),
            kind: GeneratorKind::Global,
            target: GeneratorTarget::Ui,
            features_required: vec![
                "ifml_backend".to_string(),
                format!("framework_{framework}"),
            ],
            features_optional: vec![],
        });
    }

    caps
}
