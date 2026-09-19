use std::path::Path;

use codegraph_core::traits::GraphQuerier;
use codegraph_grafeo::GrafeoEngine;

const SUPPORT_MOX: &str = r#"
package support

class Ticket {
    String title
    int refundCents
}
"#;

const SUPPORT_ACTOR: &str = r#"
import "support.mox"

actors Support {
    actor Customer
    actor Agent extends Customer
    actor Manager extends Customer

    capability ReadTicket on Ticket
    capability ResolveTicket on Ticket
    capability ApproveRefund on Ticket

    grant Customer {
        permit ReadTicket
    }

    grant Agent {
        permit ResolveTicket
    }

    grant Manager {
        permit ApproveRefund
        forbid ResolveTicket
    }

    never_both { ResolveTicket, ApproveRefund }
}
"#;

const TICKET_IFML: &str = r#"
domain "sales" { schema "sales"; }

import "policies/support.actor"
import "policies/support.actor"

view "TicketList" {
    roles: [Customer, Ghost];
    requires: [ReadTicket, MissingCapability];
    landmark: true;

    component "grid" {
        type: list;
        data: Ticket;
        fields: [title];
    }
}
"#;

fn write_file(dir: &Path, rel: &str, contents: &str) {
    let path = dir.join(rel);
    let parent = path.parent().expect("parent dir");
    std::fs::create_dir_all(parent).expect("create dirs");
    std::fs::write(path, contents).expect("write file");
}

async fn ingest_ifml_file(engine: &GrafeoEngine, ifml_path: &Path) -> usize {
    let model = codegraph_ifml_dsl::parse_ifml_file(ifml_path).expect("parse .ifml");
    let mut stats = codegraph::ingest::ifml_ingest::ingest_ifml_model(engine, &model)
        .await
        .expect("ingest model");
    stats.imported_policies =
        codegraph::ifml_actor_import::ingest_actor_imports(engine, engine, &model, ifml_path)
            .await
            .expect("ingest actor imports");
    stats.imported_policies
}

#[tokio::test]
async fn actor_source_import_ingests_policy_and_persists_requires() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_file(tmp.path(), "app.ifml", TICKET_IFML);
    write_file(tmp.path(), "policies/support.actor", SUPPORT_ACTOR);
    write_file(tmp.path(), "policies/support.mox", SUPPORT_MOX);

    let engine = GrafeoEngine::in_memory().expect("engine");
    let imported = ingest_ifml_file(&engine, &tmp.path().join("app.ifml")).await;
    assert_eq!(imported, 1, "duplicate .ifml import must ingest once");

    let actors = engine.get_actors().await.expect("actors");
    let names: Vec<String> = actors.iter().map(|a| a.name.clone()).collect();
    assert_eq!(names.len(), 3, "exactly one ingest: {names:?}");
    assert!(names.contains(&"Customer".to_string()));
    assert!(names.contains(&"Agent".to_string()));
    assert!(names.contains(&"Manager".to_string()));
    let manager = actors
        .iter()
        .find(|a| a.name == "Manager")
        .expect("Manager");
    assert_eq!(manager.extends.as_deref(), Some("Customer"));
    assert_eq!(manager.block.as_deref(), Some("Support"));

    let capabilities = engine.get_capabilities().await.expect("capabilities");
    let cap_names: Vec<String> = capabilities.iter().map(|c| c.name.clone()).collect();
    assert!(cap_names.contains(&"ReadTicket".to_string()));
    assert!(cap_names.contains(&"ResolveTicket".to_string()));
    assert!(cap_names.contains(&"ApproveRefund".to_string()));
    let read = capabilities
        .iter()
        .find(|c| c.name == "ReadTicket")
        .expect("ReadTicket");
    assert_eq!(read.class, "support::Ticket");

    let grants = engine.get_grants().await.expect("grants");
    assert_eq!(grants.len(), 4);
    let manager_forbid = grants
        .iter()
        .find(|g| g.actor == "Manager" && g.capability == "ResolveTicket")
        .expect("Manager forbid ResolveTicket");
    assert_eq!(manager_forbid.effect, "forbid");

    let policy = engine.get_actor_policy().await.expect("policy");
    let policy = policy.expect("policy node present");
    assert_eq!(policy.blocks, vec!["Support".to_string()]);
    assert_eq!(policy.never_both.len(), 1);
    assert_eq!(
        policy.never_both[0].capabilities,
        vec!["ResolveTicket".to_string(), "ApproveRefund".to_string()]
    );

    let permits = engine.effective_permits("Manager").await.expect("permits");
    let resolved: Vec<(String, String)> = permits
        .iter()
        .map(|p| (p.capability.clone(), p.effect.clone()))
        .collect();
    assert_eq!(
        resolved,
        vec![
            ("ApproveRefund".to_string(), "permit".to_string()),
            ("ReadTicket".to_string(), "permit".to_string()),
            ("ResolveTicket".to_string(), "forbid".to_string()),
        ]
    );

    let containers = engine.get_ifml_view_containers().await.expect("containers");
    assert_eq!(containers.len(), 1);
    let view = &containers[0];
    assert_eq!(view.name, "TicketList");
    assert_eq!(
        view.roles,
        Some(vec!["Customer".to_string(), "Ghost".to_string()])
    );
    assert_eq!(
        view.requires,
        Some(vec![
            "ReadTicket".to_string(),
            "MissingCapability".to_string()
        ])
    );
}

#[tokio::test]
async fn artifact_json_import_ingests_policy() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let artifact = rex_ir::ActorModel::new().block(
        rex_ir::ActorsDef::new("Audit")
            .actor(rex_ir::ActorDef::new("Auditor").kind(rex_ir::ActorKind::Agent))
            .capability(rex_ir::CapabilityDef::new(
                "ViewAuditLog",
                rex_ir::TypeRef::Class {
                    package: "audit".to_string(),
                    name: "AuditLog".to_string(),
                },
            ))
            .grant(
                rex_ir::GrantDef::new("Auditor")
                    .entry(rex_ir::GrantEntry::permit("ViewAuditLog").when("log.level > 1")),
            ),
    );
    let json = artifact.to_json().expect("serialize artifact");
    let ifml = r#"
import "policies/audit.rex.json"

view "AuditLog" {
    roles: [Auditor];
    requires: [ViewAuditLog];

    component "grid" {
        type: list;
        data: AuditLog;
    }
}
"#;
    write_file(tmp.path(), "app.ifml", ifml);
    write_file(tmp.path(), "policies/audit.rex.json", &json);

    let engine = GrafeoEngine::in_memory().expect("engine");
    let imported = ingest_ifml_file(&engine, &tmp.path().join("app.ifml")).await;
    assert_eq!(imported, 1);

    let actors = engine.get_actors().await.expect("actors");
    assert_eq!(actors.len(), 1);
    assert_eq!(actors[0].name, "Auditor");
    assert_eq!(actors[0].kind.as_deref(), Some("agent"));

    let capabilities = engine.get_capabilities().await.expect("capabilities");
    assert_eq!(capabilities.len(), 1);
    assert_eq!(capabilities[0].class, "audit::AuditLog");

    let grants = engine.get_grants().await.expect("grants");
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].effect, "permit");
    assert_eq!(grants[0].when.as_deref(), Some("log.level > 1"));

    let permits = engine.effective_permits("Auditor").await.expect("permits");
    assert_eq!(permits.len(), 1);
    assert_eq!(permits[0].capability, "ViewAuditLog");

    let containers = engine.get_ifml_view_containers().await.expect("containers");
    assert_eq!(containers[0].name, "AuditLog");
    assert_eq!(
        containers[0].requires,
        Some(vec!["ViewAuditLog".to_string()])
    );
}

#[tokio::test]
async fn multiple_imports_merge_into_one_policy() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_file(tmp.path(), "policies/support.mox", SUPPORT_MOX);
    write_file(tmp.path(), "policies/support.actor", SUPPORT_ACTOR);
    write_file(
        tmp.path(),
        "policies/escalation.actor",
        r#"
import "support.mox"

actors Escalation {
    actor Finance

    capability EscalateTicket on Ticket

    grant Finance {
        permit EscalateTicket
    }
}
"#,
    );
    let ifml = r#"
import "policies/support.actor"
import "policies/escalation.actor"

view "Queue" {
    roles: [Finance];
    requires: [EscalateTicket];
}
"#;
    write_file(tmp.path(), "app.ifml", ifml);

    let engine = GrafeoEngine::in_memory().expect("engine");
    let imported = ingest_ifml_file(&engine, &tmp.path().join("app.ifml")).await;
    assert_eq!(imported, 2);

    let policy = engine
        .get_actor_policy()
        .await
        .expect("policy")
        .expect("policy present");
    assert_eq!(
        policy.blocks,
        vec!["Support".to_string(), "Escalation".to_string()]
    );
    assert_eq!(engine.get_actors().await.expect("actors").len(), 4);
    assert!(engine
        .get_capabilities()
        .await
        .expect("capabilities")
        .iter()
        .any(|c| c.name == "EscalateTicket"));
}

const DELEGATION_MOX: &str = r#"
package support

class Ticket {
    String title
    boolean internal
}
"#;

const DELEGATION_ACTOR: &str = r#"
import "support.mox"

actors Support {
    actor Customer
    agent Helper

    capability ReadTicket on Ticket

    purpose CustomerCare

    grant Helper {
        permit ReadTicket
    }

    delegation AutoRead {
        from Customer
        to Helper
        purpose CustomerCare
        permit ReadTicket when (!internal) obligation log
    }
}
"#;

#[tokio::test]
async fn delegation_and_purposes_persist_through_policy_import() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_file(
        tmp.path(),
        "app.ifml",
        "import \"policies/support.actor\"\n\nview \"Queue\" {}\n",
    );
    write_file(tmp.path(), "policies/support.actor", DELEGATION_ACTOR);
    write_file(tmp.path(), "policies/support.mox", DELEGATION_MOX);

    let engine = GrafeoEngine::in_memory().expect("engine");
    let imported = ingest_ifml_file(&engine, &tmp.path().join("app.ifml")).await;
    assert_eq!(imported, 1);

    let grants = engine.get_grants().await.expect("grants");
    assert_eq!(grants.len(), 1, "delegation entries must not become grants");
    assert_eq!(grants[0].actor, "Helper");

    let policy = engine
        .get_actor_policy()
        .await
        .expect("policy")
        .expect("policy node present");
    assert_eq!(policy.purposes, vec!["CustomerCare".to_string()]);
    assert_eq!(policy.delegations.len(), 1);
    let delegation = &policy.delegations[0];
    assert_eq!(delegation.name, "AutoRead");
    assert_eq!(delegation.from_actor, "Customer");
    assert_eq!(delegation.to_actor, "Helper");
    assert_eq!(delegation.purpose.as_deref(), Some("CustomerCare"));
    assert_eq!(delegation.entries.len(), 1);
    assert_eq!(delegation.entries[0].actor, "Customer");
    assert_eq!(delegation.entries[0].capability, "ReadTicket");
    assert_eq!(delegation.entries[0].effect, "permit");
    assert_eq!(delegation.entries[0].when.as_deref(), Some("!internal"));
    assert_eq!(delegation.entries[0].obligations, vec!["log".to_string()]);

    let permits = engine.effective_permits("Helper").await.expect("permits");
    assert_eq!(
        permits.len(),
        1,
        "only the explicit grant resolves, not the delegation entry"
    );
    assert_eq!(permits[0].capability, "ReadTicket");
    assert_eq!(
        permits[0].when, None,
        "delegation when/obligations must not merge into effective permits"
    );
    assert!(permits[0].obligations.is_empty());
}

#[tokio::test]
async fn missing_domain_file_warns_without_failing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_file(tmp.path(), "app.ifml", TICKET_IFML);
    write_file(tmp.path(), "policies/support.actor", SUPPORT_ACTOR);
    // policies/support.mox deliberately absent.

    let engine = GrafeoEngine::in_memory().expect("engine");
    let imported = ingest_ifml_file(&engine, &tmp.path().join("app.ifml")).await;
    assert_eq!(imported, 0, "compile failure yields no policy");
    assert!(engine.get_actors().await.expect("actors").is_empty());
    assert!(engine.get_actor_policy().await.expect("policy").is_none());
}

#[tokio::test]
async fn actor_type_error_yields_warning_and_no_policy() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let broken = r#"
import "support.mox"

actors Broken {
    actor Agent
    capability BreakThings on UnknownClass
}
"#;
    write_file(tmp.path(), "app.ifml", TICKET_IFML);
    write_file(tmp.path(), "policies/support.actor", broken);
    write_file(tmp.path(), "policies/support.mox", SUPPORT_MOX);

    let engine = GrafeoEngine::in_memory().expect("engine");
    let imported = ingest_ifml_file(&engine, &tmp.path().join("app.ifml")).await;
    assert_eq!(imported, 0, "type error must not ingest a policy");
    assert!(engine.get_actors().await.expect("actors").is_empty());
    assert!(engine.get_capabilities().await.expect("caps").is_empty());
    assert!(engine.get_actor_policy().await.expect("policy").is_none());
}

#[tokio::test]
async fn invalid_artifact_json_warns_without_failing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ifml = r#"
import "policies/broken.rex.json"

view "Plain" {}
"#;
    write_file(tmp.path(), "app.ifml", ifml);
    write_file(tmp.path(), "policies/broken.rex.json", "{ not json");

    let engine = GrafeoEngine::in_memory().expect("engine");
    let imported = ingest_ifml_file(&engine, &tmp.path().join("app.ifml")).await;
    assert_eq!(imported, 0);
    assert!(engine.get_actor_policy().await.expect("policy").is_none());
}

#[tokio::test]
async fn no_imports_leaves_roles_inert() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ifml = r#"
view "Guarded" {
    roles: [Nobody];
    requires: [Nothing];
}
"#;
    write_file(tmp.path(), "app.ifml", ifml);

    let engine = GrafeoEngine::in_memory().expect("engine");
    let imported = ingest_ifml_file(&engine, &tmp.path().join("app.ifml")).await;
    assert_eq!(imported, 0);
    assert!(engine.get_actors().await.expect("actors").is_empty());
    let containers = engine.get_ifml_view_containers().await.expect("containers");
    assert_eq!(containers[0].roles, Some(vec!["Nobody".to_string()]));
    assert_eq!(containers[0].requires, Some(vec!["Nothing".to_string()]));
}

/// An `.actor` policy over a domain that declares `import schema`: the
/// domain files' imports must be resolved and provided to the rex compiler
/// (issue #230) or the plain compile errors with `imported schema '…' was
/// not provided`. The capability binds the import alias; the imported
/// schema's graph node is the JSON pipeline's business, not the policy's.
#[tokio::test]
async fn actor_over_domain_with_schema_imports_resolves_capability() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // The .mox lives in policies/ and its import resolves relative to its
    // own directory: policies/schemas/todo_item.json.
    write_file(
        tmp.path(),
        "policies/schemas/todo_item.json",
        r#"{
  "title": "TodoItemType",
  "type": "object",
  "properties": { "id": { "type": "string", "format": "uuid" } },
  "required": ["id"]
}"#,
    );
    write_file(
        tmp.path(),
        "policies/support.mox",
        r#"
package support

import schema "schemas/todo_item.json" as TodoItem

class Ticket {
    String title
}
"#,
    );
    write_file(
        tmp.path(),
        "policies/support.actor",
        r#"
import "support.mox"

actors Support {
    actor Agent

    capability ViewTodoItems on TodoItem
    capability CloseTicket on Ticket

    grant Agent {
        permit ViewTodoItems
        permit CloseTicket
    }
}
"#,
    );
    write_file(
        tmp.path(),
        "app.ifml",
        r#"
import "policies/support.actor"

view "TodoBoard" {
    roles: [Agent];
    requires: [ViewTodoItems];
}
"#,
    );

    let engine = GrafeoEngine::in_memory().expect("engine");
    let imported = ingest_ifml_file(&engine, &tmp.path().join("app.ifml")).await;
    assert_eq!(imported, 1, "policy over an importing domain must compile");

    let capabilities = engine.get_capabilities().await.expect("capabilities");
    let view = capabilities
        .iter()
        .find(|c| c.name == "ViewTodoItems")
        .expect("capability on the imported name");
    assert_eq!(view.class, "support::TodoItem");
    let close = capabilities
        .iter()
        .find(|c| c.name == "CloseTicket")
        .expect("capability on a declared class");
    assert_eq!(close.class, "support::Ticket");

    let grants = engine.get_grants().await.expect("grants");
    assert_eq!(grants.len(), 2);
    assert!(grants
        .iter()
        .all(|g| g.effect == "permit" && g.actor == "Agent"));

    let permits = engine.effective_permits("Agent").await.expect("permits");
    assert_eq!(permits.len(), 2);
}
