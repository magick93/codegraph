# gRPC Support — Implementation Plan

## Overview

Add 4 new generators (`grpc_proto`, `grpc_service`, `grpc_router`, `grpc_scaffold`) producing `.proto` files and tonic-based Rust server code alongside the existing REST API. Everything rides on the existing config pipeline (`profiles.toml`, `BuildPlan`, `CapabilityRegistry`) — no new mechanisms.

---

## Phase 1 — Shared Type Mapping Helper

**Files to create:**
- `crates/codegraph/src/generate/grpc/proto_type.rs`

**Purpose:** A shared function `proto_type_from_field()` that maps `PropertyNode` classification to proto types and tonic Rust types. Used by both `grpc_proto` (proto text) and `grpc_service` (Rust code + conversions).

```rust
// Output struct
pub struct ProtoFieldType {
    pub proto_type: String,        // "string", "int32", "google.protobuf.Timestamp", etc.
    pub rust_type: String,         // "String", "i32", "prost_types::Timestamp", etc.
    pub is_import: bool,           // needs an import statement (google.protobuf.*)
    pub import_path: Option<String>, // "google/protobuf/timestamp.proto"
    pub is_message: bool,          // is a nested/complex message, not a scalar
}

pub fn proto_type_from_field(
    prop: &PropertyNode,
    db: &dyn GraphQuerier,
    entity_name: &str,
) -> ProtoFieldType;

pub fn proto_type_from_rust_type(rust_type: &str) -> ProtoFieldType;
```

Key dispatch:
- `PrimitiveWrapper` → match on `rust_field_type` → scalar proto type
- `CodelistReference` → emit proto enum or `string`
- `EntityReference` → `string` (UUID)
- `ValueObject` → nested message name = `{entity_name}{FieldPascal}`
- `CompositeWrapper` → message name = `{entity_name}{FieldPascal}`
- `MediaWrapper` → message name = `MediaContent`
- `StructuredWrapper` → `google.protobuf.Struct`
- `ArrayWrapper` → `repeated <inner>`
- `RangeWrapper` → message `{field_name. pascal}Range { start, end }`

Import tracking: accumulate `Vec<String>` of unique proto import paths (e.g. `google/protobuf/timestamp.proto`, `google/protobuf/struct.proto`).

---

## Phase 2 — `GrpcProtoGenerator` (Entity-level)

**Files to create:**
- `crates/codegraph/src/generate/grpc/mod.rs`
- `crates/codegraph/src/generate/grpc/proto.rs`
- `crates/codegraph/src/generate/grpc/proto_context.rs`
- `crates/codegraph/templates/grpc/proto_message.tera`
- `crates/codegraph/templates/grpc/proto_service.tera`
- `crates/codegraph/templates/grpc/proto_shared.tera`

### Context struct (`proto_context.rs`)

```rust
pub struct ProtoContext {
    pub package: String,                  // domain name
    pub entity_name: String,
    pub module_name: String,
    pub proto_file_name: String,
    pub imports: Vec<String>,             // unique import paths
    pub messages: Vec<ProtoMsgDef>,
    pub enums: Vec<ProtoEnumDef>,
    pub service_methods: Vec<ProtoServiceMethod>,
    pub operations: Vec<String>,
    pub has_fts: bool,
    pub has_embeddings: bool,
    pub has_workflow: bool,
    pub hierarchy_field: Option<String>,
}

pub struct ProtoMsgDef {
    pub name: String,
    pub description: Option<String>,
    pub fields: Vec<ProtoFieldDef>,
}

pub struct ProtoFieldDef {
    pub field_number: u32,
    pub name: String,                     // snake_case
    pub proto_type: String,               // "string", "int32", "repeated Foo", etc.
    pub is_optional: bool,
    pub is_repeated: bool,
    pub description: Option<String>,
}

pub struct ProtoEnumDef {
    pub name: String,                     // PascalCase
    pub values: Vec<ProtoEnumValue>,
}

pub struct ProtoEnumValue {
    pub name: String,                     // UPPER_SNAKE_CASE
    pub number: i32,
}

pub struct ProtoServiceMethod {
    pub name: String,                     // PascalCase
    pub input_type: String,
    pub output_type: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
    pub description: Option<String>,
}
```

### Context builder logic

1. Query `db.get_schema(title)` → `SchemaNode` for entity name, domain, etc.
2. Query `db.get_properties(title)` → `Vec<PropertyNode>`
3. For each property, call `proto_type_from_field()` to determine proto type
4. Build message definitions:
   - **Entity message** — all fields, plus `id`, `created_at`, `updated_at` synthetic fields
   - **CreateRequest** — `id` excluded, `created_at`/`updated_at` excluded, workflow exclude applied
   - **UpdateRequest** — all fields `optional`, immutable fields excluded
   - **GetRequest** — `string id = 1`
   - **DeleteRequest** — `string id = 1`
   - **ListRequest** — `int32 page_size`, `string page_token`, `string query` (if FTS), `string status` (if workflow), `repeated FilterClause filters`
   - **ListResponse** — `repeated {Entity} data`, `int32 total`, `string next_page_token`
   - **SearchRequest** (if FTS) — `string query`, `int32 page_size`, `string page_token`
   - **SearchResponse** (if FTS) — `repeated {Entity} data`, `int32 total`, `string next_page_token`
   - **SemanticSearchRequest** (if embeddings) — `string query`, `int32 limit`
   - **TransitionRequest** (if workflow) — `string id`, `string action`, `optional string comment`, `optional string assignee_id`
   - **TreeRequest** (if hierarchy) — `string id`, `optional int32 max_depth`
5. For ValueObject properties — recursively build nested messages
6. For CodelistReference/InlineEnum with small cardinality — build proto `enum` definitions
7. Parent entity FK fields — add parent `_id` field with comment
8. Collect all proto imports uniquely
9. Build `service {EntityName}Service` with methods gated by operations flags

### Generated messages per RPC operation

| Operation | Request message | Response message |
|---|---|---|
| create | `Create{Entity}Request` | `{Entity}` |
| read | `Get{Entity}Request` | `{Entity}` |
| update | `Update{Entity}Request` | `{Entity}` |
| delete | `Delete{Entity}Request` | `google.protobuf.Empty` |
| list | `List{Entity}Request` | `List{Entity}Response` |
| fts | `SearchRequest` | `SearchResponse` |
| semantic_search | `SemanticSearchRequest` | `stream SearchResult` |
| tree | `TreeRequest` | `stream {Entity}` |
| transition | `TransitionRequest` | `{Entity}` |

### `proto_message.tera` template

```proto
syntax = "proto3";
package {{ package }};

{% for imp in imports -%}
import "{{ imp }}";
{% endfor %}
import "shared.proto";
option go_package = "{{ package }}/;{{ package }}";

{% for enum in enums %}
// {{ enum.description }}
enum {{ enum.name }} {
  {% for val in enum.values %}
  {{ val.name }} = {{ val.number }};
  {% endfor %}
}
{% endfor %}

{% for msg in messages %}
message {{ msg.name }} {
  {% for field in msg.fields %}
  {% if field.description %} // {{ field.description }}{% endif %}
  {% if field.is_optional %}optional {% endif %}{{ field.proto_type }} {{ field.name }} = {{ field.field_number }};
  {% endfor %}
}
{% endfor %}
```

### `proto_service.tera` template

```proto
// ── {{ entity_name }} Service ──────────────────────────────────────────
service {{ entity_name }}Service {
  {% for method in service_methods %}
  // {{ method.description }}
  rpc {{ method.name }}({{ method.input_type }}) returns ({{ method.output_type }});
  {% endfor %}
}
```

### `proto_shared.tera` template

```proto
syntax = "proto3";
package shared;
option go_package = "shared/;shared";

message PaginationRequest {
  int32 page_size = 1;
  string page_token = 2;
}

message PaginatedResponse {
  int32 total = 1;
  string next_page_token = 2;
}

message FilterClause {
  string field = 1;
  string value = 2;
  string operator = 3;  // eq, neq, contains, gt, lt, gte, lte
}

message SearchResult {
  string id = 1;
  float score = 2;
}
```

### Output path

```
proto/{domain}/{module_name}.proto
```

### Registration

```rust
// In generate/mod.rs entity_gens vec
Box::new(grpc::proto::GrpcProtoGenerator::new(output_dir)),
// Filtered by: plan_has_entity("grpc_proto")
```

```rust
// In profile.rs capabilities()
cap("grpc_proto", Entity, Api, &["grpc_backend"], &[]),
```

---

## Phase 3 — Conversion Layer Templates

**Files to create:**
- `crates/codegraph/templates/grpc/tonic/conversions.tera`
- `crates/codegraph/templates/grpc/official/conversions.tera` (future, stub)

### `conversions.tera` (tonic)

Generates `From<ProtoCreateRequest> for CreateCommand`, `From<ProtoUpdateRequest> for UpdateCommand`, and `From<{Entity}> for {Entity}Response`.

```rust
// ── Proto → Domain: Create ─────────────────────────────────────────────
impl From<{{ entity_name }}CreateRequest> for Create{{ entity_name }}Command {
    fn from(req: {{ entity_name }}CreateRequest) -> Self {
        Self {
            {% for field in create_fields %}
            {{ field.name }}: req.{{ field.name }}{{ field.conversion }},
            {% endfor %}
        }
    }
}

// ── Proto → Domain: Update ─────────────────────────────────────────────
impl From<{{ entity_name }}UpdateRequest> for Update{{ entity_name }}Command {
    fn from(req: {{ entity_name }}UpdateRequest) -> Self {
        Self {
            {% for field in update_fields %}
            {{ field.name }}: req.{{ field.name }}{{ field.conversion }},
            {% endfor %}
        }
    }
}

// ── Domain Response → Proto Entity ─────────────────────────────────────
impl From<{{ entity_name }}Response> for {{ entity_name }} {
    fn from(resp: {{ entity_name }}Response) -> Self {
        Self {
            id: resp.id.to_string(),
            {% for field in response_fields %}
            {{ field.name }}: resp.{{ field.name }}{{ field.conversion }},
            {% endfor %}
            created_at: Some(resp.created_at.into()),
            updated_at: Some(resp.updated_at.into()),
        }
    }
}
```

Conversion logic per type:
- `Option<Uuid>` → `req.field.map(|f| f.to_string())` or `.unwrap_or_default()` for required
- `chrono::NaiveDateTime` → `prost_types::Timestamp` via `.into()`
- `rust_decimal::Decimal` → serialized as string via `.to_string()`
- Codelist enum → `.to_string()` or `.into()` based on enum strategy
- ValueObject → recursive conversion via `from_proto()` / `.into()`

---

## Phase 4 — `GrpcServiceGenerator` (Entity-level)

**Files to create:**
- `crates/codegraph/src/generate/grpc/service.rs`
- `crates/codegraph/templates/grpc/tonic/server_impl.tera`
- `crates/codegraph/templates/grpc/official/server_impl.tera` (future, stub)

### Context struct

```rust
pub struct GrpcServiceContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub package: String,
    pub operations: Vec<String>,
    pub has_create: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_list: bool,
    pub has_fts: bool,
    pub has_embeddings: bool,
    pub has_workflow: bool,
    pub parent_ref: Option<String>,
    pub parent_entity: Option<String>,
    pub parent_domain: Option<String>,
    pub parent_module_name: Option<String>,
    pub hierarchy_field: Option<String>,
    pub repo_trait: String,                      // "CandidateRepository"
    pub proto_service_mod: String,               // "candidate_service_server"
    pub proto_service_trait: String,             // "CandidateService"
    pub proto_types_prefix: String,              // "crate::api::grpc::proto::"
    pub rpc_methods: Vec<RpcMethodDef>,
    pub ordered_fields: Vec<String>,             // field names in declaration order
}
```

### `server_impl.tera` (tonic)

```rust
use tonic::{Request, Response, Status};
use {{ proto_types_prefix }}{{ package }}::{{ proto_service_mod }}::{{ proto_service_trait }};
use crate::domain::{{ domain }}::{{ module_name }}::repository::{{ repo_trait }};

pub struct {{ entity_name }}GrpcService<R: {{ repo_trait }}> {
    repo: Arc<R>,
}

impl<R: {{ repo_trait }}> {{ entity_name }}GrpcService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }
}

#[tonic::async_trait]
impl<R: {{ repo_trait }} + 'static> {{ proto_service_trait }} for {{ entity_name }}GrpcService<R> {
    {% for rpc in rpc_methods %}
    async fn {{ rpc.name_snake }}(
        &self,
        request: Request<{{ rpc.input_type }}>,
    ) -> Result<Response<{{ rpc.output_type }}>, Status> {
        {{ rpc.body }}
    }
    {% endfor %}
}
```

### RPC method body templates

**Create:**
```rust
let req = request.into_inner();
let cmd = Create{{ entity_name }}Command::from(req);
let db = /* get db from AppState */;
let tx = db.begin().await.map_err(|e| Status::internal(e.to_string()))?;
let id = self.repo
    .create(&tx, cmd{% if parent_ref %}, req.{{ parent_ref }}{% endif %})
    .await
    .map_err(|e| Status::internal(e.to_string()))?;
tx.commit().await.map_err(|e| Status::internal(e.to_string()))?;
let response = self.repo.find_by_id(&db, id).await
    .map_err(|e| Status::internal(e.to_string()))?
    .ok_or_else(|| Status::not_found("entity not found"))?
    .into();
Ok(Response::new(response))
```

**Get:**
```rust
let req = request.into_inner();
let id = uuid::Uuid::parse_str(&req.id).map_err(|_| Status::invalid_argument("invalid id"))?;
let db = /* get db */;
let result = self.repo
    .find_by_id(&db, id{% if parent_ref %}, uuid::Uuid::parse_str(&req.{{ parent_ref }}).map_err(|_| Status::invalid_argument("invalid parent id"))?{% endif %})
    .await
    .map_err(|e| Status::internal(e.to_string()))?;
match result {
    Some(entity) => Ok(Response::new(entity.into())),
    None => Err(Status::not_found("entity not found")),
}
```

**Update:** similar to create but calls `self.repo.update()`

**Delete:**
```rust
let req = request.into_inner();
let id = uuid::Uuid::parse_str(&req.id).map_err(|_| Status::invalid_argument("invalid id"))?;
let db = /* get db */;
let tx = db.begin().await.map_err(|e| Status::internal(e.to_string()))?;
self.repo
    .delete(&tx, id)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;
tx.commit().await.map_err(|e| Status::internal(e.to_string()))?;
Ok(Response::new(prost_types::Empty::default()))
```

**List:**
```rust
let req = request.into_inner();
let db = /* get db */;
let filters: HashMap<String, String> = req.filters.into_iter()
    .map(|f| (f.field, f.value))
    .collect();
let (entities, total) = self.repo
    .list(&db, req.page_token.parse().unwrap_or(1), req.page_size as u64, &filters)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;
let data: Vec<{{ entity_name }}> = entities.into_iter().map(Into::into).collect();
Ok(Response::new(List{{ entity_name }}Response {
    data,
    total: total as i32,
    next_page_token: "".to_string(),
}))
```

### Output files

```
src/api/grpc/{{ module_name }}_grpc.rs           (server impl)
src/api/grpc/{{ module_name }}_conversions.rs    (From impls)
```

Both output paths share a single generator that produces two `GeneratedFile` entries.

### Registration

```rust
Box::new(grpc::service::GrpcServiceGenerator::new(output_dir).with_parent_candidates(parent_candidates.clone())),
// Filtered by: plan_has_entity("grpc_service")
```

```rust
cap("grpc_service", Entity, Api, &["grpc_backend"], &[]),
```

---

## Phase 5 — `GrpcRouterGenerator` (Domain-level)

**Files to create:**
- `crates/codegraph/src/generate/grpc/router.rs`
- `crates/codegraph/templates/grpc/tonic/domain_router.tera`
- `crates/codegraph/templates/grpc/official/domain_router.tera` (future, stub)

### Context struct

```rust
pub struct GrpcRouterContext {
    pub domain: String,
    pub entities: Vec<GrpcRouterEntity>,
}

pub struct GrpcRouterEntity {
    pub entity_name: String,
    pub module_name: String,
    pub proto_service_trait: String,
    pub proto_service_server: String,
    pub grpc_service_struct: String,
    pub repo_trait: String,
}
```

### Template `domain_router.tera` (tonic)

```rust
use crate::domain::{{ domain }}::*;
use crate::api::grpc::*;

pub fn grpc_router<R: Repositories>() -> tonic::transport::server::Router {
    tonic::transport::Server::builder()
        {% for entity in entities %}
        .add_service({{ entity.proto_service_server }}::new(
            {{ entity.grpc_service_struct }}::<R>::new(),
        ))
        {% endfor %}
}
```

### Output

```
src/api/grpc/{{ domain }}_router.rs
```

### Registration

```rust
Box::new(grpc::router::GrpcRouterGenerator::new(output_dir).with_parent_candidates(parent_candidates.clone())),
// Filtered by: plan_has_domain("grpc_router")
```

```rust
cap("grpc_router", Domain, Api, &["grpc_backend"], &[]),
```

---

## Phase 6 — `GrpcScaffoldGenerator` (Global-level)

**Files to create:**
- `crates/codegraph/src/generate/grpc/scaffold.rs`

### Context struct

```rust
pub struct GrpcScaffoldContext {
    pub has_grpc: bool,  // always true for this generator
}
```

### Logic

1. Generate `proto/shared.proto` from `grpc/proto_shared.tera`
2. Generate `src/api/grpc/mod.rs` with module declarations:
   ```rust
   // Generated by codegraph. DO NOT EDIT.
   pub mod shared;
   pub mod convert;
   {% for domain in domains %}
   pub mod {{ domain }}_router;
   {% endfor %}
   ```
   Where `convert.rs` provides shared proto↔domain conversion helpers:
   - `uuid_to_string(id: Uuid) -> String`
   - `string_to_uuid(s: &str) -> Result<Uuid>`
   - `timestamp_to_chrono(ts: prost_types::Timestamp) -> chrono::NaiveDateTime`
   - `chrono_to_timestamp(dt: chrono::NaiveDateTime) -> prost_types::Timestamp`
   - `decimal_to_string(d: rust_decimal::Decimal) -> String`
   - `string_to_decimal(s: &str) -> Result<rust_decimal::Decimal>`
3. Does NOT generate `build.rs` or `Cargo.toml` — those are handled by the scaffold generator (Phase 7).

### Registration

```rust
Box::new(grpc::scaffold::GrpcScaffoldGenerator::new(output_dir)),
// Filtered by: plan_has_global("grpc_scaffold")
```

```rust
cap("grpc_scaffold", Global, Api, &["grpc_backend"], &[]),
```

---

## Phase 7 — Scaffold Generator Integration

**Files to modify:**
- `crates/codegraph/src/generate/scaffold/gen.rs`
- `crates/codegraph/templates/scaffold/build_rs.tera`
- `crates/codegraph/templates/scaffold/cargo_toml.tera`
- `crates/codegraph/src/generate/mod.rs`

### Changes to `ScaffoldContext`

```rust
pub struct ScaffoldContext {
    // ... existing fields ...
    pub has_grpc: bool,   // NEW
}
```

### Changes to `ScaffoldGenerator::new()`

```rust
pub fn new(output_dir: &Path, has_webhooks: bool, has_reports: bool, has_grpc: bool) -> Self {
    Self {
        output_dir: output_dir.to_path_buf(),
        has_webhooks,
        has_reports,
        has_grpc,  // NEW
    }
}
```

### Changes to generator construction in `mod.rs`

```rust
let has_grpc = build_plan
    .map(|bp| bp.has_global_gen("grpc_scaffold"))
    .unwrap_or(false);

// Pass to ScaffoldGenerator
Box::new(ScaffoldGenerator::new(output_dir, has_webhooks, has_reports, has_grpc)),
```

### `build_rs.tera` — updated content

```rust
//! Build script for compile-time metadata injection.
//! Generated by hr-graph. DO NOT EDIT.

fn main() {
    shadow_rs::ShadowBuilder::builder().build().unwrap();
    {% if has_grpc %}
    compile_grpc();
    {% endif %}
}

{% if has_grpc %}
fn compile_grpc() {
    let proto_dir = std::path::Path::new("proto");
    if !proto_dir.exists() {
        return;
    }
    let mut protos = Vec::new();
    collect_protos(proto_dir, &mut protos);
    if protos.is_empty() {
        return;
    }
    tonic_build::configure()
        .build_server(true)
        .compile(&protos, &["proto"])
        .expect("protobuf compilation failed");
}

fn collect_protos(dir: &std::path::Path, protos: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_protos(&path, protos);
            } else if path.extension().map_or(false, |e| e == "proto") {
                protos.push(path);
            }
        }
    }
}
{% endif %}
```

### `cargo_toml.tera` — updated content

```toml
# Generated by codegraph. DO NOT EDIT.

[package]
name = "{{ app_name }}"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = { version = "0.8", features = ["multipart"] }
# ... existing deps unchanged ...
{% if has_grpc %}
tonic = { version = "0.14", optional = true }
prost = { version = "0.13", optional = true }
prost-types = { version = "0.13", optional = true }
{% endif %}

{% if has_grpc %}
[features]
default = []
grpc = ["tonic", "prost", "prost-types"]
{% endif %}

[build-dependencies]
shadow-rs = "1"
{% if has_grpc %}
tonic-build = "0.14"
{% endif %}
```

---

## Phase 8 — Module wiring + Capability Registration

**Files to modify:**
- `crates/codegraph/src/generate/mod.rs`
- `crates/codegraph/src/profile.rs`
- `profiles.toml`

### `generate/mod.rs` — register new generators

Add to entity gens vec (alongside existing API generators):
```rust
Box::new(grpc::proto::GrpcProtoGenerator::new(output_dir)),
Box::new(grpc::service::GrpcServiceGenerator::new(output_dir).with_parent_candidates(parent_candidates.clone())),
```

Add to domain gens vec:
```rust
Box::new(grpc::router::GrpcRouterGenerator::new(output_dir)),
```

Add to global gens vec:
```rust
Box::new(grpc::scaffold::GrpcScaffoldGenerator::new(output_dir)),
```

Add `pub mod grpc;` to `crates/codegraph/src/generate/mod.rs`.

### `profile.rs` — capability entries

```rust
cap("grpc_proto",            Entity,  Api, &["grpc_backend"], &[]),
cap("grpc_service",          Entity,  Api, &["grpc_backend"], &[]),
cap("grpc_router",           Domain,  Api, &["grpc_backend"], &[]),
cap("grpc_scaffold",         Global,  Api, &["grpc_backend"], &[]),
```

### `profiles.toml` — add gRPC generators

Append to the `[profiles.default.api]` generator list:
```toml
# gRPC
"grpc_proto", "grpc_service", "grpc_router", "grpc_scaffold",
```

And add to the features:
```toml
[profiles.default.features]
grpc_backend = "tonic"
```

---

## Dependency Graph

```
Phase 1 (proto_type.rs)
    │
    ▼
Phase 2 (GrpcProtoGenerator) ─► Phase 6 (GrpcScaffoldGenerator)
    │                                    │
    ▼                                    ▼
Phase 3 (conversion templates)    Phase 7 (scaffold integration)
    │
    ▼
Phase 4 (GrpcServiceGenerator)
    │
    ▼
Phase 5 (GrpcRouterGenerator)
    │
    ▼
Phase 8 (module wiring + profiles)
```

### Per-phase test strategy

| Phase | Test approach |
|---|---|
| 1 | Unit test `proto_type_from_field()` with each `RefClassificationKind` variant |
| 2 | Integration: run `grpc_proto` against a known schema, verify `.proto` output |
| 3 | Unit test conversion template rendering with known context |
| 4 | Integration: run `grpc_service` + conversion, verify `.rs` output compiles |
| 5 | Integration: verify `grpc_router` output registers all domain entities |
| 6 | Integration: verify `proto/shared.proto` and `mod.rs` are correctly generated |
| 7 | End-to-end: run full pipeline with `grpc` profile, verify `build.rs` + `Cargo.toml` |
| 8 | Unit: profile validation passes with `grpc_backend = "tonic"` feature |
| E2E | Full `cargo run -- run --profile grpc` + `cargo check` on generated output |

---

## Open Questions for Implementation

1. **Field numbering** — Proto fields require sequential unique numbers. Strategy: `id` = 1, `created_at` = 998, `updated_at` = 999, entity fields = 2..N. Must persist between generations (or use alphabetical hash).

2. **Codelist enums in proto** — When should we generate a proto `enum` vs leave as `string`? Recommendation: generate enum for `InlineEnum` and any `CodelistReference` with ≤ 20 values; use `string` for larger codelists.

3. **Database connection injection** — The gRPC service needs access to the database pool. Currently `AppState` holds it. The generated tonic service should accept `Arc<AppState>` or `Arc<dyn DatabaseConnection>` in its constructor. The router template must generate the wiring.

4. **Feature name** — The Cargo feature is `grpc` in the generated project's `Cargo.toml`. The profile feature is `grpc_backend`. Ensure these don't collide conceptually.

5. **Export visibility** — Generated proto types from tonic-build are in `TONIC_BUILD_OUT_DIR`. The `mod.rs` in `src/api/grpc/` needs to include them with:
   ```rust
   tonic::include_proto!("{{ package }}");
   ```
   This requires `tonic` to be importable (hence the optional dep).
