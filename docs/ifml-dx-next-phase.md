# IFML IDE Experience — Next Phase Plan

## Current State

### What works
- [x] IFML DSL parsing (Pest grammar, 20 tests)
- [x] JSON Schema ingestion + Grafeo graph
- [x] Pipeline: `codegraph run --ifml-files app.ifml` → generated SvelteKit routes
- [x] VS Code extension: syntax highlighting, commands, diagram WebView
- [x] LSP server (auto-lsp + Tree-sitter):
  - [x] File open/change/close handling
  - [x] Pull-based diagnostics (`textDocument/diagnostic`)
  - [x] Entity reference validation (with "Type" suffix bug)
  - [x] Field reference validation
  - [x] View reference validation
  - [x] Completions for `data:`, `fields:`, `navigate("``, `type:`, `mode:`, `params {``)
  - [x] Hover (entity descriptions from JSON Schema)
  - [x] Go to Definition (entity → schema file, view → declaration)
  - [x] Push diagnostics via `publishDiagnostics`
  - [x] 7 LSP tests passing

### Known issues
- Entity names include "Type" suffix (`CustomerType` instead of `Customer`)
- `CustomerSearchCriteria` flagged as error (no such schema — correct behavior)
- Parameter binding validation missing
- No Code Actions (quick fixes)
- No semantic tokens (colorization)
- No type-aware completions after `fields: [`

## Objectives

1. **Correctness**: Entity validation must use classified entity names (strip "Type" suffix), matching the codegen pipeline behavior.
2. **Productivity**: Inline errors should offer quick fixes. Lightbulb actions should create missing schemas or add missing fields.
3. **Discoverability**: Completions should be context-aware and type-safe. `data: Order` → `fields: [` suggests Order's fields.
4. **Visual clarity**: Semantic highlighting differentiates entities, views, events, types.
5. **Parameter safety**: `navigate("X", { k: v })` validates `k` is a declared param of view X.

## Work Streams

All streams are independent and can be deployed in parallel.

---

### Stream A: Entity Name Classification (High Priority)

**Problem**: `cmd_lsp` uses `list_schemas(None)` → `["CustomerType", "OrderType"]`. Validation checks `entity_names.contains("Customer")` → false.

**Fix**: Run the entity classifier in the LSP startup and use `get_entity_names()` instead of raw schema titles. The classifier strips the "Type" suffix and marks entity/VO boundaries.

**Files**:
- `crates/codegraph/src/main.rs` — `cmd_lsp` function

**Change**:
```rust
// Before:
let entity_names: Vec<String> = schemas.iter().map(|s| s.title.clone()).collect();

// After: Run classification
let classifier_types: HashSet<String> = /* ... */;
let classifier = AutoClassifier::new(classifier_types, naming_rules);
let classified = classifier.classify_domain(domain_name, domain_entry, &all_data);
let entity_names: HashSet<String> = classified.entities.iter().map(|s| s.title.clone()).collect();
```

If this is too complex for the LSP context, simpler approach: use the `type_suffix` config (default `"Type"`) and strip it from schema titles:

```rust
let suffix = "Type";
let entity_names: Vec<String> = schemas.iter()
    .map(|s| s.title.strip_suffix(suffix).unwrap_or(&s.title).to_string())
    .collect();
```

**Effort**: ~30 min (simple suffix strip) to ~2 hours (full classification)

**Tests**: Update `test_lsp_diagnostic_for_missing_entity` to verify `data: Customer` passes when `CustomerType` is loaded.

---

### Stream B: Code Actions (Medium Priority)

**Feature**: When VS Code shows a diagnostic with `source: "codegraph"`, clicking the lightbulb offers Quick Fixes. We need to register code action handlers.

**Required LSP method**: `textDocument/codeAction`

**Action types**:

| Diagnostic | Quick Fix |
|---|---|
| Entity 'X' not found | "Create schema file for entity 'X'" |
| Entity 'X' not found | "Import schema from domain..." |
| Field 'X' not found on entity | "Add field 'X' to entity schema" |
| Field 'X' not found on entity | "Did you mean 'Y'?" (suggest closest match) |
| View 'X' not declared | "Create view 'X' declaration" |

**Files**:
- `crates/codegraph/src/lsp/mod.rs` — register `CodeActionRequest` handler
- `crates/codegraph/src/lsp/handlers.rs` — add `handle_code_action` function

**Handler signature**:
```rust
pub fn handle_code_action(
    db: &BaseDb,
    params: CodeActionParams,
) -> anyhow::Result<Option<CodeActionResponse>> {
    // Find diagnostics at the given range
    let diagnostics = compute_diagnostics(db, &params.text_document.uri);
    let context_diags: Vec<&Diagnostic> = diagnostics.iter()
        .filter(|d| d.range.intersects(params.range))
        .collect();
    
    let mut actions = Vec::new();
    for diag in &context_diags {
        if diag.message.contains("Entity") && diag.message.contains("not found") {
            // Extract entity name from message
            let name = extract_entity_name(&diag.message);
            actions.push(CodeAction {
                title: format!("Create schema file for '{}'", name),
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(create_schema_workspace_edit(name)),
                diagnostics: vec![diag.clone()],
                ..Default::default()
            });
        }
    }
    
    Ok(Some(CodeActionResponse::List(actions)))
}
```

**Effort**: ~3 hours

**Tests**: Create `test_lsp_code_action_missing_entity` — request code actions at a diagnostic range and verify the response contains "Create schema file" action.

---

### Stream C: Type-Aware Completions (Medium Priority)

**Current behavior**: `fields: [` suggests ALL properties from ALL known schemas (via backward scan for `data:`). Works but fragile — backward scan fails on multi-line components or when `data:` is after `fields:`.

**Target behavior**: Use Tree-sitter AST to find the current component node, extract its `data:` property value, then suggest fields from that specific entity.

**Implementation**:
```rust
static COMPLETION_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(
        &IFML_LANG,
        r"((component_body) @body
          (#contains? @body data:))"
    ).unwrap()
});
```

Better approach: Walk the Tree-sitter tree upward from the cursor position to find the enclosing `component_body` node, then search its children for a `property_assignment` with `key: (identifier)` matching `"data"`. Extract the `identifier` child of the value.

**Files**:
- `crates/codegraph/src/lsp/handlers.rs` — `handle_completion` function, specifically the `fields:` branch

**Effort**: ~2 hours

**Tests**: Verify that `fields: [` after `data: Order` suggests `["date", "total"]` (Order's fields) and not `["name", "email"]` (Customer's fields).

---

### Stream D: Semantic Tokens (Low Priority)

**Feature**: Colorize IFML code by semantic role:
- Entity references (`data: Customer`) → ClassName color (blue)
- View names (`navigate("CustomerList")`) → Link color (purple)
- Component types (`list`, `form`) → Keyword color (cyan)
- Property names (`type:`, `data:`, `fields:`) → Property color (green)
- Event types (`select`, `submit`) → Event color (orange)

**Required LSP method**: `textDocument/semanticTokens/full`

**Implementation**: Define a `SemanticTokensLegend` with token types and modifiers. Walk the Tree-sitter tree and emit tokens for each node type.

```rust
pub fn handle_semantic_tokens_full(
    db: &BaseDb,
    params: SemanticTokensParams,
) -> anyhow::Result<Option<SemanticTokensResult>> {
    let file = db.get_file(&params.text_document.uri).ok_or(...)?;
    let document = file.document(db);
    let source = document.as_str();
    let root = document.tree.root_node();
    
    let mut builder = SemanticTokensBuilder::new();
    walk_and_emit(&root, source, &mut builder);
    Ok(Some(SemanticTokensResult::Tokens(builder.build())))
}
```

**Files**:
- `crates/codegraph/src/lsp/handlers.rs` — new `handle_semantic_tokens_full`
- `crates/codegraph/src/lsp/mod.rs` — register handler and add legend to capabilities

**Effort**: ~2 hours

**Tests**: Verify semantic tokens are emitted for a known IFML file.

---

### Stream E: Parameter Binding Validation (Low Priority)

**Feature**: Validate that `navigate("CustomerDetail", { customerId: row.id })` — `customerId` must be a declared param of `CustomerDetail`.

```ifml
view "CustomerDetail" {
    params { customerId: Uuid };
    // ...
}
```

**Implementation**: When encountering `navigate("ViewName", { bindings })`, parse the target view's `params` block and check each binding key matches a declared param name.

**Files**:
- `crates/codegraph/src/lsp/handlers.rs` — `compute_diagnostics` function
- Query: Find view declarations by name and extract their params

**Effort**: ~2 hours

**Tests**: `test_lsp_diagnostic_invalid_param_binding` — verify error for `navigate("CustomerDetail", { wrongKey: value })` when `CustomerDetail` has `params { customerId: Uuid }`.

---

### Stream F: Extract Entity Names from Schema Titles (Quick Fix)

**Same as Stream A.** Listed separately if we want to apply the simplest fix immediately without restructuring classification.

**Approach**: Strip `type_suffix` (default `"Type"`) from schema titles when building `entity_names`:

```rust
let suffix = &domain_config.defaults.type_suffix; // "Type"
let entity_names: Vec<String> = schemas.iter()
    .map(|s| {
        let title = &s.title;
        if let Some(stripped) = title.strip_suffix(suffix) {
            stripped.to_string()
        } else {
            title.clone()
        }
    })
    .collect();
```

**Effort**: ~15 minutes

---

## Dependency Graph

```
Stream A ──┬── Stream C (depends on correct entity names)
           └── Stream F (simplified version of A)

Stream B  (independent — uses diagnostics from compute_diagnostics)
Stream D  (independent — walks Tree-sitter tree)
Stream E  (independent — parses params blocks)
```

Streams A/F, B, D, E can all run in parallel. Stream C depends on A/F for correct entity names in completions.

## Testing Strategy

| Stream | Test approach |
|---|---|
| A/F | Verify `data: Customer` passes when schema is `CustomerType`; `data: Nonexistent` fails |
| B | Request code actions at diagnostic → verify quick fix titles |
| C | Verify `fields: [` after `data: Order` suggests Order's fields only |
| D | Verify semantic tokens contain entity references at expected ranges |
| E | Verify `navigate("X", { wrongKey })` warns when X has no `wrongKey` param |

## Effort Summary

| Stream | Effort | Priority |
|--------|--------|----------|
| F — Entity name suffix strip | 15 min | High (quick fix for immediate bug) |
| A — Full classification in LSP | 2 hr | High |
| B — Code Actions | 3 hr | Medium |
| C — Type-aware completions | 2 hr | Medium |
| D — Semantic tokens | 2 hr | Low |
| E — Param binding validation | 2 hr | Low |

**Total remaining**: ~9-11 hours of work, can be parallelized into 4-5 simultaneous streams.
