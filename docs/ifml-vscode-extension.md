# IFML VS Code Extension — LSP, Tree-sitter & IDE Experience

## Overview

A VS Code extension that provides a rich editing experience for `.ifml` files. It integrates a **Tree-sitter grammar** for syntax highlighting and structural navigation, an **LSP server** (the `codegraph` binary in LSP mode) for semantic features, and a **WebView-based SvelteFlow visual editor** for diagram editing (see Visual Editor doc).

The extension is the primary developer touchpoint for the IFML DSL.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                         VS Code Host                              │
│                                                                    │
│  ┌─────────────────────────┐  ┌──────────────────────────────┐    │
│  │     Monaco Text Editor   │  │     WebView Panel            │    │
│  │   (Tree-sitter parsing)  │  │   (SvelteFlow Diagram)       │    │
│  │                          │  │                              │    │
│  │  • Syntax highlighting   │  │  • Visual IFML graph        │    │
│  │  • Code folding          │  │  • Drag-and-drop editing    │    │
│  │  • Bracket matching      │  │  • Flow visualization       │    │
│  │  • Folding ranges        │  │  • Palette/toolbox          │    │
│  └──────────┬───────────────┘  └──────────┬──────────────────┘    │
│             │         sync via             │                       │
│             │     document model           │                       │
│             └──────────────┬───────────────┘                      │
│                            │                                      │
│                    ┌───────┴────────┐                             │
│                    │  Extension     │                             │
│                    │  Controller    │                             │
│                    │  (TypeScript)  │                             │
│                    │                │                             │
│                    │  • Manages     │                             │
│                    │    LSP client  │                             │
│                    │  • Manages     │                             │
│                    │    WebView     │                             │
│                    │  • Coordinates │                             │
│                    │    sync        │                             │
│                    └───────┬────────┘                             │
└────────────────────────────┼─────────────────────────────────────┘
                             │ LSP (stdin/stdout JSON-RPC)
                             │ WebSocket (diagram sync)
┌────────────────────────────┼─────────────────────────────────────┐
│                    ┌───────┴────────┐                             │
│                    │  codegraph LSP  │  Rust binary (lsp subcmd)  │
│                    │  Server         │                             │
│                    │                 │                             │
│                    │  • Pest parses  │                             │
│                    │    .ifml files  │                             │
│                    │  • Validates    │                             │
│                    │  • Provides     │                             │
│                    │    completions  │                             │
│                    │  • Queries      │                             │
│                    │    Grafeo graph │                             │
│                    │  • Hosts sync   │                             │
│                    │    server for   │                             │
│                    │    diagram      │                             │
│                    └───────┬────────┘                             │
│                            │                                      │
│                    ┌───────┴────────┐                             │
│                    │  Grafeo Graph   │  In-memory graph DB        │
│                    │                 │                             │
│                    │  SchemaNode     │  ViewContainerNode         │
│                    │  PropertyNode   │  ViewComponentNode         │
│                    │  CodeListNode   │  EventNode                 │
│                    │  Entity flags   │  ActionNode                │
│                    │  Edge types     │  Flow/DataBinding edges    │
│                    └────────────────┘                             │
│                                                                    │
│                         Rust Backend                               │
└────────────────────────────────────────────────────────────────────┘
```

---

## Extension Structure

```
codegraph-vscode/
├── package.json              # Extension manifest
├── tsconfig.json
├── src/
│   ├── extension.ts          # Activation entry point
│   ├── lsp/
│   │   ├── client.ts         # LSP client setup & management
│   │   └── server-manager.ts # Spawns/manages codegraph LSP process
│   ├── tree-sitter/
│   │   ├── grammar.js        # Tree-sitter grammar definition
│   │   └── highlights.scm    # Syntax highlighting queries
│   ├── webview/
│   │   ├── panel.ts          # WebView panel creation & messaging
│   │   ├── sync.ts           # Bidirectional sync engine
│   │   └── ifml-diagram/     # SvelteFlow app (see Visual Editor doc)
│   ├── completion/
│   │   └── providers.ts      # Additional completion providers
│   └── commands/
│       ├── preview.ts        # "Open IFML Diagram" command
│       ├── validate.ts       # "Validate IFML" command
│       └── generate.ts       # "Generate from IFML" command
├── syntaxes/
│   ├── ifml.tmLanguage.json  # TextMate grammar (fallback highlighting)
│   └── ifml.tmLanguage.yaml # Source for the above
└── build/
    └── tree-sitter.js        # Build script for tree-sitter wasm
```

---

## LSP Server Architecture

The `codegraph` binary runs in LSP mode via `codegraph lsp`. It implements the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) over stdin/stdout.

### LSP Mode Entry Point

```rust
// In crates/codegraph/src/cli.rs
enum Command {
    Lsp {
        schema_dirs: Vec<PathBuf>,     // JSON Schema directories
        classifier: Option<PathBuf>,
        config: Option<PathBuf>,
    },
    // ... existing commands
}
```

```rust
// In crates/codegraph/src/lsp/mod.rs
pub fn run_lsp(args: LspArgs) -> Result<()> {
    let backend = create_backend();

    // Load JSON Schema files into graph (read-only)
    for dir in &args.schema_dirs {
        let schemas = SchemaLoader::load(dir)?;
        ingest_schemas(&backend, &schemas)?;
    }

    // Run classification to mark entities
    if let (Some(classifier), Some(config)) = (&args.classifier, &args.config) {
        let classifier_config = ClassifierConfig::load(classifier)?;
        let domain_config = DomainConfig::load(config)?;
        classify(&backend, &classifier_config, &domain_config)?;
    }

    // Start LSP event loop
    let connection = lsp_server::Connection::stdio();
    let server_capabilities = serde_json::json!({
        "textDocumentSync": {
            "openClose": true,
            "change": { "syncKind": "Incremental" }
        },
        "completionProvider": {
            "triggerCharacters": [".", "\"", ":"],
            "completionItem": { "labelDetailsSupport": true }
        },
        "hoverProvider": true,
        "definitionProvider": true,
        "referencesProvider": true,
        "documentSymbolProvider": true,
        "workspaceSymbolProvider": true,
        "codeActionProvider": {
            "codeActionKinds": ["quickfix"]
        },
        "semanticTokensProvider": {
            "legend": { /* ... */ },
            "full": true
        },
        "diagnosticProvider": {
            "interFileDependencies": true,
            "workspaceDiagnostics": false
        }
    });

    Server::new(connection, server_capabilities)
        .run(BackendState { graph: backend })
        .unwrap();
}
```

### LSP Server State

```rust
pub struct BackendState {
    pub graph: Backend,            // Grafeo engine with loaded schemas
    pub documents: HashMap<Url, IfmlDocument>,
}

pub struct IfmlDocument {
    pub uri: Url,
    pub text: String,
    pub version: i32,
    pub ast: Option<IfmlModel>,    // Last successful parse
    pub diagnostics: Vec<Diagnostic>,
    pub needs_reparse: bool,
    pub view_container_ids: Vec<String>,  // IDs in Grafeo graph
}
```

### LSP Features

#### 1. Completions (`textDocument/completion`)

Triggered by `:`, `"`, `.` and on explicit Ctrl+Space.

```rust
fn handle_completion(
    state: &BackendState,
    params: CompletionParams,
) -> Vec<CompletionItem> {
    let doc = state.get_document(&params.text_document.uri);
    let position = params.position;
    let prefix = doc.text_before_position(position);

    if prefix.ends_with("data: \"") || prefix.ends_with("data: ") {
        // Suggest entities from the graph
        state.graph.get_entity_names().iter().map(|name| {
            CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some("Entity".into()),
                insert_text: Some(name.clone()),
                ..
            }
        }).collect()
    } else if prefix.ends_with("fields: [") || prefix.ends_with("fields: [") {
        // Suggest properties of the current entity
        suggest_properties(state, doc, position, &prefix)
    } else if prefix.contains("navigate(\"") {
        // Suggest view names
        suggest_views(state, doc)
    } else if prefix.ends_with("\"") && prefix.contains("component ") {
        // Suggest component type
        suggest_component_types()
    } else {
        vec![]
    }
}
```

**Completion sources:**
| Context | Data source |
|---|---|
| `data:` field | `GraphQuerier.get_entity_names()` |
| `fields:` array | `GraphQuerier.get_properties(&entity)` |
| `navigate("")` target | Available `ViewContainer` nodes |
| `type:` field | Static list: list, form, details, search, tree, chart |
| `mode:` field | Static list: view, edit, create |
| `on` event type | Static list based on component type |
| `filter:` expression | Properties of the bound entity + `params.*` |
| `params { X: }` | Type suggestions: Uuid, String, Int, etc. |

#### 2. Hover (`textDocument/hover`)

```rust
fn handle_hover(
    state: &BackendState,
    params: HoverParams,
) -> Option<Hover> {
    let doc = state.get_document(&params.text_document.uri);
    let word = doc.word_at_position(params.position)?;

    // Check if it's an entity reference
    if let Some(schema) = state.graph.get_schema(&word) {
        return Some(Hover {
            contents: MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "**{}**\n\n{}\n\n| Field | Type | Required |\n|-------|------|----------|\n{}",
                    schema.title,
                    schema.description.as_deref().unwrap_or(""),
                    schema_fields_to_markdown(&state.graph, &word)
                ),
            },
            range: Some(doc.word_range(params.position)?),
        });
    }

    // Check if it's a property reference
    if let Some((entity, prop)) = doc.resolve_property_reference(&word) {
        // Show property details from JSON Schema
        // ...
    }

    None
}
```

#### 3. Diagnostics (pull-based, `textDocument/diagnostic`)

Sent on open and on every edit (debounced 500ms).

```rust
fn validate_document(
    state: &BackendState,
    uri: &Url,
) -> Vec<Diagnostic> {
    let doc = state.get_document(uri);
    let ast = match IfmlParser::parse(&doc.text) {
        Ok(ast) => ast,
        Err(errors) => return pest_errors_to_diagnostics(errors),
    };

    let mut diags = vec![];

    // Cross-reference validation (requires graph → needs reparse after graph changes)
    for def in &ast.definitions {
        match def {
            View(view) => {
                for comp in &view.components {
                    // Check entity exists
                    let entity_name = comp.get_property("data");
                    if let Some(name) = entity_name {
                        if state.graph.get_schema(name).is_none() {
                            diags.push(Diagnostic {
                                range: comp.range_of_property("data"),
                                severity: DiagnosticSeverity::ERROR,
                                message: format!("Entity '{}' not found in loaded schemas", name),
                                source: Some("codegraph".into()),
                                ..Default::default()
                            });
                        }

                        // Check fields exist
                        if let Some(fields) = comp.get_array_property("fields") {
                            for field in fields {
                                if !entity_has_property(&state.graph, name, field) {
                                    diags.push(error(...));
                                }
                            }
                        }
                    }

                    // Check navigation targets exist
                    for event in &comp.events {
                        if let Navigate { target, .. } = &event.action {
                            if !ast.has_view(target) {
                                diags.push(error(
                                    event.range(),
                                    format!("View '{}' not declared", target),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    diags
}
```

#### 4. Code Actions (`textDocument/codeAction`)

Quick fixes for common issues:

| Diagnostic | Quick Fix |
|---|---|
| Entity 'X' not found | "Import schema for domain..." / "Create entity 'X' in schema" |
| View 'X' not declared | "Create view 'X'" |
| Field 'X' not on entity | "Add field 'X' to entity schema" / "Did you mean 'Y'?" |
| Missing `data:` binding | "Add data binding from entity..." |
| Type mismatch in binding | "Cast value to target type" |

#### 5. Go to Definition (`textDocument/definition`)

- Entity name → jumps to the JSON Schema file
- View name → jumps to the view declaration
- Property name → jumps to the field in the entity's schema file
- `navigate("X")` → jumps to the target view declaration

#### 6. Document Symbols (`textDocument/documentSymbol`)

Provides a structured outline of the `.ifml` file:

```
IFML Model
├── Domain "sales"
├── View "CustomerList"
│   ├── Component "grid" (list)
│   └── Component "searchBar" (form)
├── View "CustomerDetail"
│   ├── Params: customerId
│   └── Component "info" (details)
├── View "CustomerEdit"
│   ├── Params: customerId
│   └── Component "form" (form)
├── View "Dashboard"
│   ├── Component "recentOrders" (list)
│   └── Component "topProducts" (list)
└── Action "UpdateCustomer"
```

#### 7. Semantic Tokens

Custom semantic token types for coloring:
- `view`, `component`, `action`, `module` — keywords (blue)
- `navigate`, `refresh`, `action` — control flow (purple)
- `on`, `select`, `submit`, `load` — events (orange)
- `data:`, `type:`, `fields:` — properties (green)
- Entity names — class names (cyan)
- String literals — strings (brown)
- Numbers — numbers (green)

---

## Tree-sitter Grammar

The Tree-sitter grammar provides syntax highlighting, code folding, bracket matching, and structural navigation.

### Grammar Definition (`grammar.js`)

```javascript
module.exports = grammar({
  name: 'ifml',

  extras: $ => [/\s/, $.comment],

  conflicts: $ => [],

  rules: {
    source_file: $ => repeat(
      choice($.domain_declaration, $.view_declaration,
             $.action_declaration, $.module_declaration)
    ),

    comment: $ => token(seq('//', /.*/)),

    domain_declaration: $ => seq(
      'domain',
      field('name', $.string),
      '{',
      'schema', $.string, ';',
      '}'
    ),

    view_declaration: $ => seq(
      'view',
      field('name', $.string),
      optional($.parameter_block),
      field('body', $.view_body)
    ),

    view_body: $ => seq(
      '{',
      repeat(choice(
        $.property_assignment,
        $.container_declaration,
        $.component_declaration,
        $.event_handler
      )),
      '}'
    ),

    container_declaration: $ => seq(
      'container',
      field('name', $.string),
      optional($.parameter_block),
      field('body', $.view_body)
    ),

    component_declaration: $ => seq(
      'component',
      field('name', $.string),
      field('body', $.component_body)
    ),

    component_body: $ => seq(
      '{',
      repeat(choice(
        $.property_assignment,
        $.event_handler
      )),
      '}'
    ),

    property_assignment: $ => seq(
      field('key', $.identifier),
      ':',
      field('value', $._value_expr),
      ';'
    ),

    _value_expr: $ => choice(
      $.string,
      $.number,
      $.boolean,
      $.identifier,
      $.array_literal,
      $.call_expression
    ),

    array_literal: $ => seq(
      '[',
      commaSep($._value_expr),
      optional(','),
      ']'
    ),

    call_expression: $ => seq(
      $.identifier,
      '(',
      commaSep($._value_expr),
      ')'
    ),

    event_handler: $ => seq(
      'on',
      field('type', $.event_type),
      optional(field('params', $.event_params)),
      '->',
      field('action', $._event_action)
    ),

    event_type: $ => choice(
      'select', 'submit', 'click', 'change', 'load',
      'save', 'cancel', 'delete', 'confirm', 'back',
      $.identifier
    ),

    event_params: $ => seq(
      '(',
      commaSep($.identifier),
      ')'
    ),

    _event_action: $ => choice(
      $.navigate_action,
      $.refresh_action,
      $.action_invocation,
      'stay'
    ),

    navigate_action: $ => seq(
      'navigate',
      '(',
      field('target', $.string),
      optional(seq(',', field('bindings', $.parameter_binding))),
      ')'
    ),

    parameter_binding: $ => seq(
      '{',
      commaSep(seq(field('key', $.identifier), ':', field('value', $.expression))),
      '}'
    ),

    // C-like expressions
    expression: $ => $._comparison,

    _comparison: $ => prec.left(1, seq(
      $._addition,
      repeat(seq(
        choice('==', '!=', '<', '<=', '>', '>=', '~=', '!~'),
        $._addition
      ))
    )),

    _addition: $ => prec.left(2, seq(
      $._multiplication,
      repeat(seq(choice('+', '-'), $._multiplication))
    )),

    _multiplication: $ => prec.left(3, seq(
      $._unary,
      repeat(seq(choice('*', '/', '%'), $._unary))
    )),

    _unary: $ => prec(4, choice(
      seq('!', $._unary),
      seq('-', $._unary),
      $._primary
    )),

    _primary: $ => prec(5, choice(
      $.string,
      $.number,
      $.boolean,
      seq($.identifier, '.', $.identifier),
      $.identifier,
      seq('(', $.expression, ')')
    )),

    action_declaration: $ => seq(
      'action',
      field('name', $.string),
      field('body', $.action_body)
    ),

    action_body: $ => seq(
      '{',
      repeat($.property_assignment),
      repeat($.event_handler),
      '}'
    ),

    // Lexical rules
    string: $ => token(seq(
      '"',
      repeat(choice(
        token.immediate(/[^\\"\n]/),
        /\\./
      )),
      '"'
    )),

    number: $ => token(seq(
      optional('-'),
      /[0-9]+/,
      optional(seq('.', /[0-9]+/))
    )),

    boolean: $ => choice('true', 'false'),

    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    parameter_block: $ => seq(
      '{',
      commaSep(seq(
        field('name', $.identifier),
        ':',
        field('type', $._type_ref)
      )),
      '}'
    ),

    _type_ref: $ => choice(
      'Uuid', 'String', 'Int', 'Float', 'Boolean', 'DateTime',
      $.identifier
    ),
  },
});

function commaSep(rule) {
  return optional(seq(rule, repeat(seq(',', rule))));
}
```

### Syntax Highlighting (`highlights.scm`)

```scheme
; Keywords
[
  "domain" "schema" "view" "container" "component"
  "action" "module" "params" "input" "output"
] @keyword

; Events
[
  "on" "select" "submit" "click" "change" "load"
  "save" "cancel" "delete" "confirm" "back"
] @keyword.function

; Flow control
["navigate" "refresh" "stay" "action"] @keyword.control

; Type references
(type_ref) @type

; Strings
(string) @string

; Numbers
(number) @number

; Booleans
(boolean) @boolean

; Comments
(comment) @comment

; Identifiers
(identifier) @variable

; Property keys
(property_assignment key: (identifier) @property)

; Entity references (heuristic: after "data:")
((property_assignment
  key: (identifier) @_key
  value: (identifier) @type.class)
  (#eq? @_key "data"))

; Field references (heuristic: inside array after "fields:")
; handled by pattern matching the tree structure

; Operators
[
  "==" "!=" "<" "<=" ">" ">=" "~=" "!~"
  "+" "-" "*" "/" "%" "!" "&&" "||"
] @operator

; Delimiters
["{" "}" "[" "]" "(" ")" "," ";"] @punctuation
```

### Building

The Tree-sitter grammar is compiled to WebAssembly for use in VS Code:

```bash
# Install tree-sitter CLI
npm install -g tree-sitter-cli

# Build WASM grammar
tree-sitter build-wasm

# Output: ifml.wasm → loaded by VS Code extension
```

This is loaded by the extension using `@vscode/tree-sitter-wasm`.

---

## TextMate Grammar (Fallback)

For VS Code versions that don't support WASM-based grammars, provide a TextMate grammar:

```yaml
# syntaxes/ifml.tmLanguage.yaml
scopeName: source.ifml
name: IFML
fileTypes: [ifml]

patterns:
  - include: '#comments'
  - include: '#keywords'
  - include: '#strings'
  - include: '#numbers'
  - include: '#types'

repository:
  comments:
    patterns:
      - match: '//.*'
        name: comment.line.double-slash.ifml

  keywords:
    patterns:
      - match: '\b(view|container|component|action|module|domain|schema)\b'
        name: keyword.control.ifml
      - match: '\b(on|select|submit|click|change|load|save|cancel)\b'
        name: keyword.function.ifml
      - match: '\b(navigate|refresh|stay)\b'
        name: keyword.control.ifml

  strings:
    patterns:
      - begin: '"'
        end: '"'
        name: string.quoted.double.ifml

  numbers:
    patterns:
      - match: '\b-?[0-9]+(\.[0-9]+)?\b'
        name: constant.numeric.ifml

  types:
    patterns:
      - match: '\b(Uuid|String|Int|Float|Boolean|DateTime)\b'
        name: support.type.ifml
```

---

## Extension Commands

### `ifml.openDiagram`

Opens the SvelteFlow diagram WebView for the current `.ifml` file.

```typescript
// src/commands/preview.ts
import { openDiagramPanel } from '../webview/panel';

export function registerOpenDiagramCommand(context: vscode.ExtensionContext) {
    return vscode.commands.registerCommand('ifml.openDiagram', () => {
        const editor = vscode.window.activeTextEditor;
        if (!editor || editor.document.languageId !== 'ifml') {
            vscode.window.showErrorMessage('Open an .ifml file first');
            return;
        }
        openDiagramPanel(context, editor.document.uri);
    });
}
```

### `ifml.validate`

Runs full validation including cross-schema checks and shows Problems panel:

```typescript
export function registerValidateCommand(context) {
    return vscode.commands.registerCommand('ifml.validate', async () => {
        const uri = vscode.window.activeTextEditor?.document.uri;
        if (!uri) return;

        // The LSP server handles this, but this command forces a re-validate
        await vscode.commands.executeCommand(
            'lsp.forceDiagnosticRefresh'
        );
    });
}
```

### `ifml.generate`

Runs the codegen pipeline from within VS Code:

```typescript
export function registerGenerateCommand(context) {
    return vscode.commands.registerCommand('ifml.generate', async () => {
        // 1. Save all open .ifml files
        await vscode.workspace.saveAll();

        // 2. Run codegraph CLI
        const terminal = vscode.window.createTerminal('codegraph');
        terminal.show();
        terminal.sendText(
            `cargo run -- run --ifml ${workspaceFiles} --schemas schemas/ ...`
        );
    });
}
```

---

## Workspace Configuration

In `package.json`:

```json
{
  "contributes": {
    "languages": [{
      "id": "ifml",
      "aliases": ["IFML", "ifml"],
      "extensions": [".ifml"],
      "configuration": "./language-configuration.json"
    }],
    "grammars": [{
      "language": "ifml",
      "scopeName": "source.ifml",
      "path": "./syntaxes/ifml.tmLanguage.json"
    }],
    "commands": [
      { "command": "ifml.openDiagram", "title": "Open IFML Diagram" },
      { "command": "ifml.validate", "title": "Validate IFML" },
      { "command": "ifml.generate", "title": "Generate from IFML" }
    ],
    "configuration": {
      "title": "IFML",
      "properties": {
        "ifml.codegraphPath": {
          "type": "string",
          "default": "codegraph",
          "description": "Path to the codegraph binary"
        },
        "ifml.schemaDirs": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Directories containing JSON Schema files"
        },
        "ifml.classifierConfig": {
          "type": "string",
          "description": "Path to classifier.toml"
        },
        "ifml.domainConfig": {
          "type": "string",
          "description": "Path to domains.toml"
        }
      }
    }
  }
}
```

---

## Language Configuration (`language-configuration.json`)

```json
{
  "comments": {
    "lineComment": "//"
  },
  "brackets": [
    ["{", "}"],
    ["[", "]"],
    ["(", ")"]
  ],
  "autoClosingPairs": [
    { "open": "{", "close": "}" },
    { "open": "[", "close": "]" },
    { "open": "(", "close": ")" },
    { "open": "\"", "close": "\"", "notIn": ["string"] }
  ],
  "folding": {
    "markers": {
      "start": "^\\s*//\\s*#region\\b",
      "end": "^\\s*//\\s*#endregion\\b"
    }
  },
  "indentationRules": {
    "increaseIndentPattern": "^.*\\{[^}\"]*$",
    "decreaseIndentPattern": "^\\s*\\}"
  },
  "surroundingPairs": [
    { "open": "{", "close": "}" },
    { "open": "[", "close": "]" },
    { "open": "\"", "close": "\"" }
  ]
}
```

---

## LSP Server Startup & Lifecycle

```typescript
// src/lsp/server-manager.ts
import { ChildProcess, spawn } from 'child_process';
import * as vscode from 'vscode';

export class LspServerManager {
    private process: ChildProcess | null = null;

    start(): void {
        const config = vscode.workspace.getConfiguration('ifml');
        const binaryPath = config.get<string>('codegraphPath', 'codegraph');
        const schemaDirs = config.get<string[]>('schemaDirs', []);
        const classifierPath = config.get<string>('classifierConfig', '');
        const domainConfig = config.get<string>('domainConfig', '');

        const args = ['lsp'];
        for (const dir of schemaDirs) {
            args.push('--schemas', dir);
        }
        if (classifierPath) args.push('--classifier', classifierPath);
        if (domainConfig) args.push('--config', domainConfig);

        this.process = spawn(binaryPath, args, {
            stdio: ['pipe', 'pipe', 'pipe'],
        });

        this.process.stderr?.on('data', (data) => {
            console.error(`[codegraph-lsp] ${data}`);
        });

        this.process.on('exit', (code) => {
            console.log(`[codegraph-lsp] exited with code ${code}`);
            // Auto-restart on crash
            setTimeout(() => this.start(), 1000);
        });
    }

    stop(): void {
        if (this.process) {
            this.process.kill();
            this.process = null;
        }
    }
}
```

---

## Diagram Sync Protocol

The LSP server also hosts a WebSocket server (on a random port, communicated to the extension via LSP `workspace/configuration` or a custom notification) for synchronizing text ↔ diagram changes.

### Protocol Messages

| Direction | Message | Purpose |
|---|---|---|
| Extension → LSP | `sync/diagramChanged` | User dragged/added/removed diagram elements |
| LSP → Extension | `sync/textChanged` | Text edit caused model change → update diagram |
| Extension → LSP | `sync/requestFull` | Request full model for initial diagram render |
| LSP → Extension | `sync/fullModel` | Complete IFML model as JSON (nodes + edges) |
| Extension → LSP | `sync/selectElement` | User clicked element in diagram → highlight in text |
| LSP → Extension | `sync/selectLocation` | User clicked in text → highlight in diagram |

### Full Model Payload

```json
{
  "type": "sync/fullModel",
  "model": {
    "nodes": [
      {
        "id": "vc-customer-list",
        "type": "view-container",
        "label": "CustomerList",
        "position": { "x": 100, "y": 100 },
        "data": {
          "isModal": false,
          "isDefault": false,
          "isLandmark": true,
          "params": [{ "name": "customerId", "type": "Uuid" }]
        }
      },
      {
        "id": "comp-customer-grid",
        "type": "view-component",
        "parentId": "vc-customer-list",
        "label": "grid",
        "position": { "x": 100, "y": 200 },
        "data": {
          "componentType": "list",
          "entity": "Customer",
          "fields": ["name", "email", "phone"]
        }
      }
    ],
    "edges": [
      {
        "id": "flow-1",
        "source": "evt-grid-select",
        "target": "vc-customer-detail",
        "type": "navigation-flow",
        "label": "select → CustomerDetail",
        "data": {
          "parameterBinding": { "customerId": "row.id" }
        }
      }
    ]
  }
}
```

---

## Extension Packaging

```bash
# Build extension
npm install
npm run compile

# Package for VS Code
npx vsce package
# Output: codegraph-ifml-0.1.0.vsix

# Install locally
code --install-extension codegraph-ifml-0.1.0.vsix
```

---

## Testing Strategy

| Scope | Test | Method |
|---|---|---|
| Tree-sitter grammar | Parse test .ifml files, verify CST structure | Jest + tree-sitter |
| Syntax highlighting | Screenshot comparison against known-good | VS Code integration tests |
| LSP completions | Open .ifml, type triggers, assert completion items | VS Code extension tests |
| LSP validation | Create invalid .ifml, assert diagnostics appear | VS Code extension tests |
| LSP hover | Hover over entity name, assert markdown shown | VS Code extension tests |
| Code actions | Trigger quick fix, assert edit applied | VS Code extension tests |
| WebView | Open diagram panel, assert correct rendering | Playwright (within WebView) |
| Sync | Edit text, assert diagram updates; drag diagram, assert text updates | Integration test |
| Full pipeline | Open workspace, edit .ifml, run generate, verify output | Smoke test |
