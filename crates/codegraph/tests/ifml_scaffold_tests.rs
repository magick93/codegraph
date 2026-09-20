//! Round-trip tests for `codegraph ifml-scaffold`: run the scaffold against
//! a temp fixture (hello-world TODO schemas), then parse the emitted DSL
//! with the real `rex-ifml` parser and assert on views, form
//! fields, and navigation events.

use std::fs;
use std::path::Path;

use codegraph::ifml_scaffold::{ifml_scaffold, IfmlScaffoldArgs};

const TODO_LIST_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "TodoListType",
  "description": "A named collection of todo items.",
  "type": "object",
  "properties": {
    "id": { "description": "Primary identifier.", "type": "string", "format": "uuid" },
    "name": { "description": "Display name of the list.", "type": "string" },
    "description": { "description": "Optional longer description.", "type": "string" }
  },
  "required": ["id", "name"]
}"#;

const TODO_ITEM_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "TodoItemType",
  "description": "A single todo item belonging to a list.",
  "type": "object",
  "properties": {
    "id": { "description": "Primary identifier.", "type": "string", "format": "uuid" },
    "list_id": { "description": "Identifier of the list this item belongs to.", "type": "string", "format": "uuid" },
    "title": { "description": "Short title of the item.", "type": "string" },
    "notes": { "description": "Optional free-form notes.", "type": "string" },
    "completed": { "description": "Whether the item is done.", "type": "boolean" },
    "due_date": { "description": "Optional due date.", "type": "string", "format": "date" },
    "contact_email": { "description": "Assignee contact.", "type": "string", "format": "email" },
    "tags": { "description": "Free-form tags.", "type": "array", "items": { "type": "string" } },
    "priority": { "description": "Priority 1-5.", "type": "integer" }
  },
  "required": ["id", "title"]
}"#;

const NOTE_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "NoteType",
  "description": "A standalone note.",
  "type": "object",
  "properties": {
    "id": { "type": "string", "format": "uuid" },
    "body": { "type": "string" }
  },
  "required": ["id"]
}"#;

fn write_fixture(dir: &Path) {
    let todo_dir = dir.join("schemas/todo");
    let misc_dir = dir.join("schemas/misc");
    fs::create_dir_all(&todo_dir).unwrap();
    fs::create_dir_all(&misc_dir).unwrap();
    fs::write(todo_dir.join("TodoListType.json"), TODO_LIST_SCHEMA).unwrap();
    fs::write(todo_dir.join("TodoItemType.json"), TODO_ITEM_SCHEMA).unwrap();
    fs::write(misc_dir.join("NoteType.json"), NOTE_SCHEMA).unwrap();

    fs::write(
        dir.join("domains.toml"),
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.todo]
label = "Todo"
schema_dir = "todo"
postgres_schema = "todo"
entities = ["TodoListType", "TodoItemType"]

[domains.misc]
label = "Misc"
schema_dir = "misc"
postgres_schema = "misc"
entities = ["NoteType"]
"#,
    )
    .unwrap();

    fs::write(dir.join("classifier.toml"), "").unwrap();
}

struct Fixture {
    dir: tempfile::TempDir,
    schemas: std::path::PathBuf,
    classifier: std::path::PathBuf,
    config: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        write_fixture(dir.path());
        let schemas = dir.path().join("schemas");
        let classifier = dir.path().join("classifier.toml");
        let config = dir.path().join("domains.toml");
        Self {
            dir,
            schemas,
            classifier,
            config,
        }
    }

    fn args<'a>(
        &'a self,
        output: &'a Path,
        force: bool,
        domains: &'a [String],
    ) -> IfmlScaffoldArgs<'a> {
        IfmlScaffoldArgs {
            schemas: &self.schemas,
            classifier: &self.classifier,
            config_path: &self.config,
            output,
            force,
            domains,
        }
    }
}

fn view_names(model: &rex_ifml::IfmlModel) -> Vec<String> {
    model.views.iter().map(|v| v.name.clone()).collect()
}

fn find_view<'a>(model: &'a rex_ifml::IfmlModel, name: &str) -> &'a rex_ifml::ViewDeclaration {
    model
        .views
        .iter()
        .find(|v| v.name == name)
        .unwrap_or_else(|| panic!("view '{name}' not found in {:?}", view_names(model)))
}

#[tokio::test]
async fn scaffold_output_parses_with_expected_views_and_navigation() {
    let fx = Fixture::new();
    let output = fx.dir.path().join("app.ifml");
    ifml_scaffold(fx.args(&output, false, &[]))
        .await
        .expect("scaffold should succeed");

    let content = fs::read_to_string(&output).unwrap();
    let model = rex_ifml::parse_ifml(&content).expect("emitted DSL must parse");

    // Domain headers from domains.toml
    let domain_names: Vec<&str> = model.domains.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(domain_names, vec!["misc", "todo"]);

    // Views: List/Detail/Form per entity + Home landmark
    let names = view_names(&model);
    for expected in [
        "Home",
        "TodoListList",
        "TodoListDetail",
        "TodoListForm",
        "TodoItemList",
        "TodoItemDetail",
        "TodoItemForm",
        "NoteList",
        "NoteDetail",
        "NoteForm",
    ] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }

    // Home is the only landmark view
    let home = find_view(&model, "Home");
    assert!(home.is_landmark);
    assert_eq!(home.events.len(), 3, "one nav link per entity");
    for event in &home.events {
        assert!(
            matches!(&event.action, rex_ifml::EventAction::Navigate { .. }),
            "Home events should navigate"
        );
    }

    // List view: select -> navigate to Detail with id binding
    let list = find_view(&model, "TodoItemList");
    let grid = &list.components[0];
    assert_eq!(grid.component_type, Some(rex_ifml::ComponentType::List));
    let data_prop = grid
        .properties
        .iter()
        .find(|p| p.key == "data")
        .expect("list binds data");
    assert_eq!(
        data_prop.value,
        rex_ifml::ValueExpression::Identifier("TodoItem".to_string())
    );
    assert_eq!(grid.events.len(), 1);
    assert_eq!(grid.events[0].event_type, rex_ifml::EventType::Select);
    match &grid.events[0].action {
        rex_ifml::EventAction::Navigate { target, binding } => {
            assert_eq!(target, "TodoItemDetail");
            let binding = binding.as_ref().expect("id binding");
            assert_eq!(binding.pairs[0].0, "id");
        }
        other => panic!("Expected Navigate, got {other:?}"),
    }

    // Detail view: params { id: Uuid } + back navigation
    let detail = find_view(&model, "TodoItemDetail");
    assert_eq!(detail.params.len(), 1);
    assert_eq!(detail.params[0].name, "id");
    assert_eq!(detail.params[0].type_ref, "Uuid");
    let info = &detail.components[0];
    assert_eq!(info.component_type, Some(rex_ifml::ComponentType::Details));
    assert_eq!(info.events.len(), 1);
    assert_eq!(info.events[0].event_type, rex_ifml::EventType::Back);
    match &info.events[0].action {
        rex_ifml::EventAction::Navigate { target, .. } => {
            assert_eq!(target, "TodoItemList");
        }
        other => panic!("Expected Navigate, got {other:?}"),
    }

    // Form view: one field per scalar property with mapped input types
    let form = find_view(&model, "TodoItemForm");
    let form_comp = &form.components[0];
    assert_eq!(
        form_comp.component_type,
        Some(rex_ifml::ComponentType::Form)
    );
    let spec = match &form_comp.spec {
        Some(rex_ifml::ComponentSpec::Form(spec)) => spec,
        other => panic!("Expected Form spec, got {other:?}"),
    };
    let field = |name: &str| {
        spec.fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("form field '{name}' missing"))
    };

    let title = field("title");
    assert_eq!(title.input, rex_ifml::InputFieldType::Text);
    assert!(title.required, "title is required in the schema");

    assert!(!field("notes").required);
    assert_eq!(field("completed").input, rex_ifml::InputFieldType::Checkbox);
    assert_eq!(field("due_date").input, rex_ifml::InputFieldType::DateTime);
    assert_eq!(
        field("contact_email").input,
        rex_ifml::InputFieldType::Email
    );
    assert_eq!(field("tags").input, rex_ifml::InputFieldType::TextArea);
    assert_eq!(field("priority").input, rex_ifml::InputFieldType::Number);
    assert_eq!(field("list_id").input, rex_ifml::InputFieldType::Hidden);
    assert!(
        spec.fields.iter().all(|f| f.name != "id"),
        "primary key must not become a form field"
    );

    // Form navigation: save + cancel back to the list
    assert_eq!(form_comp.events.len(), 2);
    assert_eq!(form_comp.events[0].event_type, rex_ifml::EventType::Save);
    assert_eq!(form_comp.events[1].event_type, rex_ifml::EventType::Cancel);
    for event in &form_comp.events {
        match &event.action {
            rex_ifml::EventAction::Navigate { target, .. } => {
                assert_eq!(target, "TodoItemList");
            }
            other => panic!("Expected Navigate, got {other:?}"),
        }
    }

    // List fields are scalar-only and capped at 5
    let list_fields_prop = grid
        .properties
        .iter()
        .find(|p| p.key == "fields")
        .expect("list has fields");
    match &list_fields_prop.value {
        rex_ifml::ValueExpression::Array(items) => {
            assert!(items.len() <= 5, "list fields capped at 5");
        }
        other => panic!("Expected Array, got {other:?}"),
    }
}

#[tokio::test]
async fn scaffold_refuses_overwrite_without_force() {
    let fx = Fixture::new();
    let output = fx.dir.path().join("app.ifml");
    ifml_scaffold(fx.args(&output, false, &[]))
        .await
        .expect("first run succeeds");

    let err = ifml_scaffold(fx.args(&output, false, &[]))
        .await
        .unwrap_err();
    assert!(
        format!("{err}").contains("force"),
        "overwrite refusal should mention --force: {err}"
    );

    ifml_scaffold(fx.args(&output, true, &[]))
        .await
        .expect("second run with --force succeeds");
}

#[tokio::test]
async fn scaffold_domain_filter_limits_output() {
    let fx = Fixture::new();
    let output = fx.dir.path().join("todo.ifml");
    ifml_scaffold(fx.args(&output, false, &["todo".to_string()]))
        .await
        .expect("filtered scaffold should succeed");

    let content = fs::read_to_string(&output).unwrap();
    let model = rex_ifml::parse_ifml(&content).expect("emitted DSL must parse");

    let names = view_names(&model);
    assert!(names.contains(&"TodoListList".to_string()));
    assert!(
        !names.iter().any(|n| n.starts_with("Note")),
        "domain filter should exclude misc domain views, got: {names:?}"
    );
    assert!(
        !model.domains.iter().any(|d| d.name == "misc"),
        "domain filter should exclude misc domain header"
    );
}

#[tokio::test]
async fn scaffold_with_unknown_domain_filter_errors() {
    let fx = Fixture::new();
    let output = fx.dir.path().join("none.ifml");
    let err = ifml_scaffold(fx.args(&output, false, &["nope".to_string()]))
        .await
        .unwrap_err();
    assert!(
        format!("{err}").contains("no entities"),
        "unknown domain filter should error, got: {err}"
    );
    assert!(!output.exists(), "no file should be written on error");
}

// ── Workflow state guards (issue #198 v2, RED — pinned rule flagged for
//    review) ──────────────────────────────────────────────────────────────
//
// Pinned minimal rule: for an entity whose domains.toml workflow declares
// terminal states, the scaffolded FORM view carries a view-level condition
// `if item.<status_field> != "<terminal>" (&& ...)*` — editing a
// terminal-state entity is invalid, so the authoring loop starts from a
// guarded form. List and Detail views stay unguarded (browsing terminal
// items is fine). This is the minimal sensible rule; anything richer
// (per-transition view guards, details guards) needs product judgment.

const WORKFLOW_TODO_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "RefundType",
  "description": "A refund request with a workflow.",
  "type": "object",
  "properties": {
    "id": { "type": "string", "format": "uuid" },
    "title": { "type": "string" },
    "status": { "type": "string", "enum": ["draft", "submitted", "archived", "done"] }
  },
  "required": ["id", "title"]
}"#;

const PLAIN_NOTE_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "NoteType",
  "description": "A standalone note without a workflow.",
  "type": "object",
  "properties": {
    "id": { "type": "string", "format": "uuid" },
    "body": { "type": "string" }
  },
  "required": ["id"]
}"#;

#[tokio::test]
async fn scaffold_emits_terminal_state_guard_for_workflow_forms() {
    let dir = tempfile::tempdir().unwrap();
    let schemas_dir = dir.path().join("schemas/billing");
    fs::create_dir_all(&schemas_dir).unwrap();
    fs::write(schemas_dir.join("RefundType.json"), WORKFLOW_TODO_SCHEMA).unwrap();
    let misc_dir = dir.path().join("schemas/misc");
    fs::create_dir_all(&misc_dir).unwrap();
    fs::write(misc_dir.join("NoteType.json"), PLAIN_NOTE_SCHEMA).unwrap();

    fs::write(
        dir.path().join("domains.toml"),
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.billing]
label = "Billing"
schema_dir = "billing"
postgres_schema = "billing"
entities = ["RefundType"]

[domains.billing.entity_config.RefundType.workflow]
status_field = "status"
initial_state = "draft"
states = ["draft", "submitted", "archived", "done"]
terminal_states = ["archived", "done"]
generate_action_endpoints = true

[domains.billing.entity_config.RefundType.workflow.transitions]
draft = ["submitted"]
submitted = ["archived", "done"]

[domains.misc]
label = "Misc"
schema_dir = "misc"
postgres_schema = "misc"
entities = ["NoteType"]
"#,
    )
    .unwrap();
    fs::write(dir.path().join("classifier.toml"), "").unwrap();

    let output = dir.path().join("app.ifml");
    ifml_scaffold(IfmlScaffoldArgs {
        schemas: &dir.path().join("schemas"),
        classifier: &dir.path().join("classifier.toml"),
        config_path: &dir.path().join("domains.toml"),
        output: &output,
        force: false,
        domains: &[],
    })
    .await
    .expect("scaffold should succeed");

    let content = fs::read_to_string(&output).unwrap();
    let model = rex_ifml::parse_ifml(&content).expect("emitted DSL must parse");

    let form = find_view(&model, "RefundForm");
    let condition = form
        .condition
        .as_ref()
        .expect("workflow form carries a guard");
    assert_eq!(
        rex_ifml::render_expression(condition),
        r#"item.status != "archived" && item.status != "done""#,
        "the form view guards against terminal states: {content}"
    );

    for unguarded in [
        "RefundList",
        "RefundDetail",
        "NoteList",
        "NoteDetail",
        "NoteForm",
    ] {
        let view = find_view(&model, unguarded);
        assert!(
            view.condition.is_none(),
            "non-form views stay unguarded: {unguarded}: {content}"
        );
    }
}
