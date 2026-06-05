# IFML DSL — Pest Grammar & Graph Ingestion

## Overview

A custom Domain-Specific Language (DSL) that is semantically identical to OMG IFML 1.0 but uses a modern, C-like expression-heavy syntax instead of XML/XMI. Parsed by **Pest** (Rust PEG parser) at build time for code generation, and by **Tree-sitter** at edit time for IDE features (see VS Code extension doc).

The DSL lives alongside JSON Schema as a *complementary primary input*: JSON Schema defines the data model (entities, fields, constraints), the IFML DSL defines the interaction model (views, navigation, events, data binding). Both feed into the same Grafeo graph, linked by data binding edges.

---

## Philosophy

1. **Semantic identity with IFML** — every IFML concept has a direct DSL equivalent. No lossy mapping.
2. **C-like expression syntax** — conditions, filters, parameter expressions use familiar infix/prefix syntax.
3. **Composable blocks** — view containers nest, components nest, events nest inside components.
4. **Schema-aware** — entity/field references are validated against the loaded JSON Schema models.
5. **Text-first, diagram-aligned** — the text representation is the source of truth; diagrams are derived views (see Visual Editor doc).

---

## Syntax Examples

### Minimal CRUD

```ifml
domain "sales" {
    schema "sales";
}

view "CustomerList" {
    component "grid" {
        type: list;
        data: Customer;
        fields: [name, email, phone, status];

        on select(row) -> navigate("CustomerDetail", {
            customerId: row.id
        });
    }
}

view "CustomerDetail" {
    params { customerId: Uuid };

    component "info" {
        type: details;
        data: Customer;

        on edit -> navigate("CustomerEdit", {
            customerId: params.customerId
        });
    }
}

view "CustomerEdit" {
    params { customerId: Uuid };

    component "form" {
        type: form;
        data: Customer;
        mode: edit;

        on save(values) -> action("UpdateCustomer", {
            body: values;
            on success -> navigate("CustomerDetail", {
                customerId: params.customerId
            });
            on error -> stay;
        });

        on cancel -> navigate("CustomerDetail");
    }
}
```

### Multi-Step Wizard (conditional navigation)

```ifml
view "WizardPage" {
    xor: true;

    container "Step1" {
        default: true;

        component "personalInfo" {
            type: form;
            data: Customer;
            fields: [name, email, phone];

            on submit(values) -> navigate("Step2", {
                name: values.name,
                email: values.email
            });
        }
    }

    container "Step2" {
        component "addressInfo" {
            type: form;
            data: Address;

            on submit(values) -> navigate("Step3", {
                street: values.street,
                city: values.city
            });
        }
    }

    container "Step3" {
        component "review" {
            type: details;
            data: CustomerReview;

            on confirm -> action("CreateCustomer");

            on back -> navigate("Step2");
        }
    }
}
```

### Dashboard with Data Flows

```ifml
view "Dashboard" {
    on load -> refresh("recentOrders");

    component "recentOrders" {
        type: list;
        data: Order;
        fields: [id, customerName, total, date];
        filter: date == today();
    }

    component "topProducts" {
        type: list;
        data: Product;
        fields: [name, salesCount];
        filter: salesCount > 1000;
        sort: salesCount desc;
    }
}
```

### Search → Results → Detail

```ifml
view "SearchPage" {
    component "searchForm" {
        type: form;
        data: ProductSearchQuery;

        on submit(values) -> navigate("SearchPage", {
            query: values.term
        });
    }

    component "results" {
        type: list;
        data: Product;
        fields: [name, sku, price];
        filter: name ~= params.query;

        on select(row) -> navigate("ProductDetail", {
            productId: row.id
        });
    }
}
```

### Modal Dialog

```ifml
view "DeleteConfirm" {
    modal: true;

    component "confirmForm" {
        type: form;
        data: ConfirmDelete;

        on submit(values) -> action("DeleteProduct", {
            on success -> navigate("ProductList");
            on error -> stay;
        });

        on cancel -> navigate("ProductList");
    }
}
```

---

## Complete Pest Grammar

The `.pest` grammar file (`ifml.pest`) defines the full DSL syntax. Key rule categories:

### 1. Top-level structure

```pest
// ── Entry point ──────────────────────────────────────────────────
ifml_model = {
    SOI ~
    domain_declaration* ~
    (view_declaration | action_declaration | module_declaration)* ~
    EOI
}

domain_declaration = {
    "domain" ~ string ~ "{" ~
        "schema" ~ string ~ ";" ~
    "}"
}
```

### 2. View containers

```pest
view_declaration = {
    "view" ~ string ~
    (parameter_block)? ~
    view_body
}

view_body = {
    "{" ~
        property_assignment* ~
        (container_declaration | component_declaration | event_handler)* ~
    "}"
}

container_declaration = {
    "container" ~ string ~
    (parameter_block)? ~
    view_body
}

parameter_block = {
    "{" ~ parameter_decl ("," ~ parameter_decl)* ~ "}"
}

parameter_decl = {
    identifier ~ ":" ~ type_ref
}
```

### 3. Components

```pest
component_declaration = {
    "component" ~ string ~ component_body
}

component_body = {
    "{" ~
        property_assignment* ~
        event_handler* ~
    "}"
}

property_assignment = {
    identifier ~ ":" ~ value_expression ~ ";"
}

value_expression = {
    identifier | string | number | boolean |
    "[" ~ (value_expression ("," ~ value_expression)*)? ~ "]" |
    identifier ~ "(" ~ (value_expression ("," ~ value_expression)*)? ~ ")"
}
```

### 4. Events

```pest
event_handler = {
    "on" ~ event_type ~ event_param? ~ "->" ~ event_action
}

event_type = {
    "select" | "submit" | "click" | "change" | "load" |
    "save" | "cancel" | "delete" | "confirm" | "back" |
    identifier
}

event_param = {
    "(" ~ identifier ("," ~ identifier)* ~ ")"
}

event_action = {
    navigate_action |
    refresh_action |
    action_invocation |
    stay_statement
}

navigate_action = {
    "navigate" ~ "(" ~ string ~ ("," ~ parameter_binding)? ~ ")"
}

refresh_action = {
    "refresh" ~ "(" ~ string ~ ("," ~ parameter_binding)? ~ ")"
}

action_invocation = {
    "action" ~ "(" ~ string ~ ("," ~ action_body)? ~ ")"
}

action_body = {
    "{" ~
        (property_assignment ~ event_handler)* ~
    "}"
}

stay_statement = { "stay" }

parameter_binding = {
    "{" ~ (parameter_binding_pair ("," ~ parameter_binding_pair)*)? ~ "}"
}

parameter_binding_pair = {
    identifier ~ ":" ~ expression
}
```

### 5. C-like expressions

```pest
expression = {
    comparison
}

comparison = {
    addition ~ (comparison_op ~ addition)*
}

comparison_op = { "==" | "!=" | "<" | "<=" | ">" | ">=" | "~=" | "!~" }

addition = {
    multiplication ~ (add_op ~ multiplication)*
}

add_op = { "+" | "-" }

multiplication = {
    unary ~ (mul_op ~ unary)*
}

mul_op = { "*" | "/" | "%" }

unary = {
    "!" ~ unary |
    "-" ~ unary |
    primary
}

primary = {
    string |
    number |
    boolean |
    identifier ~ "." ~ identifier |
    identifier |
    "(" ~ expression ~ ")"
}
```

### 6. Actions

```pest
action_declaration = {
    "action" ~ string ~ "{" ~
        property_assignment* ~
        event_handler* ~
    "}"
}
```

### 7. Modules (for reusable interaction patterns)

```pest
module_declaration = {
    "module" ~ string ~ "{" ~
        "input" ~ parameter_block ~
        "output" ~ parameter_block ~
        view_body ~
    "}"
}
```

### 8. Lexical rules

```pest
identifier = @{ ASCII_ALPHA ~ (ASCII_ALPHANUMERIC | "_")* }

string = @{ "\"" ~ (escape_char | (!("\"" | "\\") ~ ANY))* ~ "\"" }

escape_char = @{ "\\" ~ ("\"" | "\\" | "/" | "b" | "f" | "n" | "r" | "t" | "u" ~ ASCII_HEX_DIGIT{4}) }

number = @{ "-"? ~ ASCII_DIGIT+ ~ ("." ~ ASCII_DIGIT+)? }

boolean = { "true" | "false" }

type_ref = { "Uuid" | "String" | "Int" | "Float" | "Boolean" | "DateTime" | identifier }

comment = @{ "//" ~ (!"\n" ~ ANY)* }

WHITESPACE = _{ " " | "\t" | "\n" | "\r" }
COMMENT = _{ comment }
```

---

## AST Types (Rust)

Parsed Pest pairs are converted to typed AST nodes in `codegraph-ifml-dsl`:

```rust
pub enum IfmlDefinition {
    Domain(DomainDeclaration),
    View(ViewDeclaration),
    Action(ActionDeclaration),
    Module(ModuleDeclaration),
}

pub struct DomainDeclaration {
    pub name: String,
    pub schema: String,
}

pub struct ViewDeclaration {
    pub name: String,
    pub label: Option<String>,
    pub params: Vec<ParameterDecl>,
    pub properties: Vec<PropertyAssignment>,
    pub containers: Vec<ContainerDeclaration>,
    pub components: Vec<ComponentDeclaration>,
    pub events: Vec<EventHandler>,
}

pub struct ContainerDeclaration {
    pub name: String,
    pub is_default: bool,
    pub params: Vec<ParameterDecl>,
    pub properties: Vec<PropertyAssignment>,
    pub components: Vec<ComponentDeclaration>,
    pub events: Vec<EventHandler>,
}

pub struct ComponentDeclaration {
    pub name: String,
    pub properties: Vec<PropertyAssignment>,
    pub events: Vec<EventHandler>,
}

pub struct PropertyAssignment {
    pub key: String,
    pub value: ValueExpression,
}

pub enum ValueExpression {
    Identifier(String),
    String(String),
    Number(f64),
    Bool(bool),
    Array(Vec<ValueExpression>),
    Call(String, Vec<ValueExpression>),     // e.g., today(), refresh("grid")
    FieldAccess(Box<ValueExpression>, String), // e.g., row.id
}

pub struct EventHandler {
    pub event_type: EventType,
    pub params: Vec<String>,
    pub action: EventAction,
}

pub enum EventType {
    Select,
    Submit,
    Click,
    Change,
    Load,
    Save,
    Cancel,
    Delete,
    Confirm,
    Back,
    Custom(String),
}

pub enum EventAction {
    Navigate { target: String, binding: Option<ParameterBinding> },
    Refresh { target: String, binding: Option<ParameterBinding> },
    ActionInvocation { name: String, body: Option<ActionBody> },
    Stay,
}

pub struct ActionBody {
    pub properties: Vec<PropertyAssignment>,
    pub handlers: Vec<EventHandler>,
}

pub struct ParameterDecl {
    pub name: String,
    pub type_ref: String,
}

pub struct ParameterBinding {
    pub pairs: Vec<(String, Expression)>,
}

pub enum Expression {
    // C-like expression tree
    Ident(String),
    StringLit(String),
    NumLit(f64),
    BoolLit(bool),
    FieldExpr(Box<Expression>, String),        // a.b
    BinOp(Box<Expression>, BinOp, Box<Expression>),
    UnaryOp(UnaryOp, Box<Expression>),
    Group(Box<Expression>),
}

pub enum BinOp { Eq, Ne, Lt, Le, Gt, Ge, RegexMatch, NegRegex, Add, Sub, Mul, Div, Mod, And, Or }
pub enum UnaryOp { Not, Neg }
```

---

## Graph Ingestion

The AST is converted to Grafeo graph nodes and edges via the existing `GraphIngestor` trait. This is a new crate `codegraph-ifml-dsl` that:

1. Accepts `.ifml` file paths
2. Parses with Pest
3. Walks the AST
4. Calls `GraphIngestor` methods to create nodes/edges

### New Node Types (extending Grafeo schema DDL)

```gql
CREATE NODE TYPE ViewContainer {
    name: String,
    label: String,
    is_xor: Boolean DEFAULT false,
    is_default: Boolean DEFAULT false,
    is_landmark: Boolean DEFAULT false,
    is_modal: Boolean DEFAULT false,
    domain: String
}

CREATE NODE TYPE ViewComponent {
    name: String,
    component_type: String,  // "list", "form", "details", "search", "tree", "chart"
    mode: String DEFAULT "view",  // "view", "edit", "create"
    domain: String
}

CREATE NODE TYPE Event {
    name: String,
    event_type: String,  // "select", "submit", "load", "action", "system", "cancel"
    domain: String
}

CREATE NODE TYPE Action {
    name: String,
    domain: String
}

CREATE NODE TYPE ParameterDefinition {
    name: String,
    direction: String DEFAULT "in",  // "in", "out", "inout"
    type_ref: String,
    domain: String
}

CREATE NODE TYPE DataBinding {
    conditional_expression: String,
    expression_language: String DEFAULT "ifml",
    domain: String
}

CREATE NODE TYPE ModuleDefinition {
    name: String,
    domain: String
}
```

### New Edge Types

```gql
CREATE EDGE TYPE ContainsViewContainer {
    from ViewContainer to ViewContainer,
    properties { sort_order: Int }
}

CREATE EDGE TYPE ContainsViewComponent {
    from ViewContainer to ViewComponent,
    properties { sort_order: Int }
}

CREATE EDGE TYPE HasEvent {
    from ViewElement to Event,    // ViewElement = ViewContainer + ViewComponent
    properties {}
}

CREATE EDGE TYPE NavigationFlow {
    from Event to ViewContainer,
    properties {
        target_param_binding: String  // JSON-encoded binding map
    }
}

CREATE EDGE TYPE DataFlow {
    from ViewElement to ViewElement,
    properties {
        source_param: String,
        target_param: String
    }
}

CREATE EDGE TYPE HasParameter {
    from InteractionFlowElement to ParameterDefinition,  // ViewElement + Event + Action
    properties { direction: String }
}

CREATE EDGE TYPE HasDataBinding {
    from ViewComponent to DataBinding,
    properties {}
}

CREATE EDGE TYPE BindsToEntity {
    from DataBinding to Schema,
    properties {}
}

CREATE EDGE TYPE BindsToProperty {
    from ViewComponent to Property,
    properties { role: String DEFAULT "display" }  // "display", "input", "filter"
}

CREATE EDGE TYPE TriggersAction {
    from Event to Action,
    properties {}
}

CREATE EDGE TYPE ActionEvent {
    from Action to Event,
    properties { outcome: String }  // "success", "error", "normal"
}

CREATE EDGE TYPE HasModuleDefinition {
    from ViewContainer to ModuleDefinition,
    properties {}
}

CREATE EDGE TYPE HasViewComponentPart {
    from ViewComponent to ViewComponent,
    properties { role: String }  // "field", "column", "button"
}
```

### Ingestion Flow

```rust
// In codegraph-ifml-dsl/src/ingest.rs
pub fn ingest_ifml(
    ingestor: &mut dyn GraphIngestor,
    ast: &IfmlModel,
    domain: &str,
) -> Result<IngestStats> {
    for def in &ast.definitions {
        match def {
            IfmlDefinition::View(view) => {
                let vc_id = ingestor.ingest_view_container(&view.into())?;
                for component in &view.components {
                    let comp_id = ingestor.ingest_view_component(&component.into())?;
                    ingestor.ingest_edge(&vc_id, &comp_id, EdgeType::ContainsViewComponent, None)?;
                    ingest_component_events(ingestor, comp_id, component)?;
                    ingest_data_binding(ingestor, comp_id, component, domain)?;
                }
                ingest_view_events(ingestor, vc_id, view)?;
            }
            IfmlDefinition::Action(action) => {
                let action_id = ingestor.ingest_action(&action.into())?;
                // ...
            }
            // ...
        }
    }
    Ok(ingestor.finalize())
}
```

---

## Pipeline Integration

In `crates/codegraph/src/cli.rs`, a new subcommand `interaction` or a flag on `run`:

```rust
enum Command {
    Run {
        schemas: PathBuf,
        ifml_files: Vec<PathBuf>,     // NEW
        classifier: PathBuf,
        config: PathBuf,
        output: PathBuf,
        profile: Option<String>,
    },
    // ...
}
```

Updated pipeline in `crates/codegraph/src/main.rs`:

```rust
fn run_pipeline(args: RunArgs) -> Result<()> {
    // Phase 1a: Ingest JSON Schema (existing)
    let backend = create_backend();
    let schemas = SchemaLoader::load(&args.schemas)?;
    ingest_schemas(&backend, &schemas)?;

    // Phase 1b: Ingest IFML DSL (NEW)
    if !args.ifml_files.is_empty() {
        let ifml_models = IfmlParser::parse_files(&args.ifml_files)?;
        ingest_ifml(&backend, &ifml_models)?;
    }

    // Phase 2-4: Classify, Validate, Generate (existing, now with enriched graph)
    classify(&backend, &args.classifier, &args.config)?;
    validate(&backend)?;
    generate(&backend, &args.output, &args.profile)?;

    Ok(())
}
```

Generators now get IFML-aware data from the graph. Existing UI generators (page, form, store) can be extended to consume view containers/components from the graph alongside entity data.

---

## New Crate Structure

```
crates/codegraph-ifml-dsl/
├── Cargo.toml
├── src/
│   ├── lib.rs            # Public API: IfmlParser, ingest_ifml
│   ├── ast.rs            # AST type definitions
│   ├── parser.rs         # Pest parser wrapper
│   ├── ingest.rs         # GraphIngester bridge (AST → graph)
│   ├── validation.rs     # DSL-level validation (cross-refs, etc.)
│   └── grammar/
│       └── ifml.pest     # Pest grammar file
├── tests/
│   ├── fixtures/         # Sample .ifml files
│   └── integration.rs    # Parse → ingest → query round-trip tests
└── README.md
```

---

## Expression Evaluation

C-like expressions appear in:

| Context | Example | Evaluation |
|---|---|---|
| Filter conditions | `filter: status == "active"` | SQL WHERE clause generation |
| Parameter bindings | `navigate("Detail", { id: row.id })` | Route parameter mapping |
| Conditional navigation | `if amount > 10000 -> navigate("Review")` | Branching logic in page controller |
| Default values | `sort: createdAt desc` | Query ordering |

Expression evaluation is **deferred to generation time** — the AST stores the expression tree, and each generator translates it to the target language (SQL, Svelte, Rust, etc.).

---

## Validation Rules (LSP-relevant)

| Rule | Description |
|---|---|
| Entity exists | Every `data: X` ref must match a classified entity in the graph |
| Property exists | Every field in `fields: [...]` must exist on the referenced entity |
| View exists | Every `navigate("ViewName")` must target a declared view |
| Param match | Parameter bindings must match target view's declared params |
| Type compatibility | `params { id: Uuid }` + binding `row.id` must both resolve to Uuid |
| Component-type validity | `list` requires `fields`, `form` cannot have `sort`, etc. |
| Event-type validity | `on select` requires list component, `on submit` requires form |
| Cycle detection | Navigation flows must not form cycles (warn, not error) |
| Domain alignment | Cross-domain navigation must reference allowed domain boundaries |

---

## DSL + JSON Schema Symbiosis

The IFML DSL is designed to feel like a *companion* to JSON Schema:

```ifml
// JSON Schema defines "Customer" with fields: name, email, phone, status
// The IFML DSL references these by name:

component "grid" {
    type: list;
    data: Customer;           // references SchemaNode where is_entity = true
    fields: [name, email, phone];  // references PropertyNode on Customer
    filter: status == "active";     // expression validated against field types
}
```

The graph ensures referential integrity across the two input formats. The `domain "sales" { schema "sales"; }` declaration at the top of each `.ifml` file establishes which JSON Schema domain provides the entity definitions.

---

## Grammar Sync Strategy (Pest ↔ Tree-sitter)

Both parse the same language but use different formalisms:

| Aspect | Pest | Tree-sitter |
|---|---|---|
| Algorithm | PEG (recursive descent) | GLR (Generalized LR) |
| Purpose | One-shot parsing for codegen | Incremental parsing for IDE |
| Location | Rust crate (`codegraph-ifml-dsl`) | JS package (`ifml-tree-sitter`) |
| Error handling | Panic on first error | Produces ERROR nodes, recovers |
| Performance | Very fast | Extremely fast (C library) |

**Sync approach**: Both grammars target the *same language spec*. Changes to the DSL syntax are first drafted as a grammar spec document, then implemented in both Pest and Tree-sitter. CI enforces that both parse a shared test fixture set identically. In practice, the Pest grammar is the authoritative reference since it drives codegen; Tree-sitter is derived.

---

## Crate Dependency Graph

```
codegraph-ifml-dsl
  ├── pest (parser)
  ├── pest_derive (grammar compile)
  ├── serde (AST serialization)
  ├── codegraph-core (GraphIngestor trait, types)
  └── codegraph-naming (identifier conventions)

codegraph (main binary)
  └── codegraph-ifml-dsl
      └── codegraph-core (GraphQuerier for validation)
          └── codegraph-grafeo (engine)
```

---

## Testing Strategy

| Scope | Test | Method |
|---|---|---|
| Grammar | Valid `.ifml` files parse without errors | Pest `parse()` |
| Grammar | Invalid `.ifml` files produce expected errors | Pest error reporting |
| AST | Parse → AST → re-serialize round-trip | Debug format comparison |
| Graph ingestion | Parse → ingest → query back the IFML elements | GraphQuerier assertions |
| Cross-schema validation | Ingest JSON Schema + IFML → validate bindings | Validation pass |
| Expression parsing | C-like expressions produce correct AST trees | Unit test |
| Integration | Full pipeline with `--ifml` flag | `cargo run -- run` test |
