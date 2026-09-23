//! Shared `.mox` starter model content.
//!
//! `codegraph init` renders one `model/<domain>.mox` per domain from the
//! `project/model_mox.tera` template; `codegraph add domain` renders the
//! same template for the new domain. This module is the single access point
//! for that shared starter content.

use std::path::Path;

use super::context::{ProjectFeatures, ProjectTemplateContext};

/// Render the starter `model/<domain>.mox` content for one domain from the
/// shared `project/model_mox.tera` template.
///
/// `graph_binary_hint` names the binary in the header's run hint — the
/// consumer project's wrapper when known, otherwise the codegraph binary.
pub fn starter_model_mox(
    domain: &str,
    label: &str,
    graph_binary_hint: &str,
) -> Result<String, String> {
    let ctx = ProjectTemplateContext::new(
        domain,
        &[domain.to_string()],
        "",
        None,
        "postgres",
        "sea_orm",
        "monolith",
        ProjectFeatures {
            grpc: false,
            ifml: false,
            ops: true,
            rosetta: false,
        },
    );
    let tera = crate::generate::template_engine::create_tera(Path::new("."))
        .map_err(|e| format!("load embedded templates: {e}"))?;
    let mut tctx = tera::Context::from_serialize(&ctx)
        .map_err(|e| format!("serialize starter context: {e}"))?;
    tctx.insert("domain", domain);
    tctx.insert("domain_label", label);
    tctx.insert("graph_binary", graph_binary_hint);
    tera.render(super::context::MODEL_TEMPLATE, &tctx)
        .map_err(|e| format!("render {}: {e}", super::context::MODEL_TEMPLATE))
}

/// Render the starter `model/<domain>.rosetta` content for one domain from
/// the shared `project/rosetta_model.tera` template (`--rosetta` scaffolds).
///
/// `app_name` is the existing project's snake_case app name — the Rosetta
/// namespace is `{app_name}.{domain}`, so the doctor's last-segment domain
/// match keeps working. `graph_binary_hint` names the binary in the header's
/// run hint.
pub fn starter_model_rosetta(
    domain: &str,
    label: &str,
    app_name: &str,
    graph_binary_hint: &str,
) -> Result<String, String> {
    let ctx = ProjectTemplateContext::new(
        domain,
        &[domain.to_string()],
        "",
        None,
        "postgres",
        "sea_orm",
        "monolith",
        ProjectFeatures {
            grpc: false,
            ifml: false,
            ops: true,
            rosetta: true,
        },
    );
    let tera = crate::generate::template_engine::create_tera(Path::new("."))
        .map_err(|e| format!("load embedded templates: {e}"))?;
    let mut tctx = tera::Context::from_serialize(&ctx)
        .map_err(|e| format!("serialize starter context: {e}"))?;
    tctx.insert("domain", domain);
    tctx.insert("domain_label", label);
    tctx.insert("app_name", app_name);
    tctx.insert("graph_binary", graph_binary_hint);
    tera.render(super::context::ROSETTA_MODEL_TEMPLATE, &tctx)
        .map_err(|e| format!("render {}: {e}", super::context::ROSETTA_MODEL_TEMPLATE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_model_renders_and_compiles() {
        let content = starter_model_mox("billing", "Billing", "codegraph").unwrap();
        assert!(content.contains("package billing"), "{content}");
        assert!(content.contains("class TodoListType"), "{content}");
        assert!(
            content.contains("refers TodoListType todoList"),
            "{content}"
        );
        assert!(
            !content.contains("--classifier"),
            "run hint must not reference --classifier:\n{content}"
        );
        let compilation = rex_driver::compile_files(&[("model/billing.mox".to_string(), content)]);
        assert!(
            compilation.model.is_some(),
            "starter model must compile: {:?}",
            compilation.diagnostics
        );
        assert_eq!(compilation.model.unwrap().packages[0].name, "billing");
    }

    #[test]
    fn starter_model_rosetta_parses_lowers_and_resolves() {
        let content = starter_model_rosetta("billing", "Billing", "demo_app", "codegraph").unwrap();
        assert!(
            content.contains("namespace demo_app.billing"),
            "namespace must be {{app_name}}.{{domain}}:\n{content}"
        );
        assert!(content.contains("version \"1.0.0\""), "{content}");
        assert!(content.contains("type BillingType:"), "{content}");
        assert!(content.contains("enum BillingStatus:"), "{content}");
        assert!(
            content.contains("(0..1)"),
            "starter must carry an optional attribute:\n{content}"
        );
        let check = super::super::rosetta_model::verify_rosetta_sources(&[
            super::super::rosetta_model::RosettaFileCheck {
                name: "model/billing.rosetta".to_string(),
                text: content,
            },
        ]);
        assert!(
            check.hard_errors.is_empty(),
            "starter rosetta model must pass sigil verification: {:?}",
            check.hard_errors
        );
    }
}
