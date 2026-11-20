//! Cross-generator type registry.
//!
//! Generated code references types that other generators emit (a handler
//! imports `WorkerResponse` from the DTO generator's module, a repository
//! impl imports its include targets, …). Those module paths are resolved
//! through a registry instead of hard-coded template strings.
//!
//! # Two registries, one default (issue #746)
//!
//! * **Per-run** ([`TypeRegistryHandle`]): every `run_generators_with_opts`
//!   creates ONE handle and threads it through `GeneratorContext` into the
//!   generators that register/resolve imports. Two generations in the same
//!   process (parallel or sequential) therefore never see each other's
//!   module paths.
//! * **Ambient** ([`ambient`]): the process-global registry behind the free
//!   [`register_framework_types`]/[`register_type`]/[`resolve_imports`]
//!   shims. It exists for public-API compatibility: consumers (hr-specs)
//!   call `register_framework_types()` in test setup and then construct
//!   generators directly (`HandlerGenerator::new(..)`), and the `new()`
//!   constructors default to the ambient handle so that flow keeps
//!   working. **The pipeline never reads it** — a pipeline run always uses
//!   its own per-run handle.
//!
//! `InMemoryTypeRegistry` itself stays public and usable directly (tests
//! build one and exercise `imports_needed`).

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// Canonical reference to a generated Rust type.
#[derive(Debug, Clone)]
pub struct TypeRef {
    pub name: String,
    pub module_path: Vec<String>,
}

impl TypeRef {
    /// `"crate::domain::common::worker::dto_response::WorkerResponse"`
    pub fn full_path(&self) -> String {
        let mut s = self.module_path.join("::");
        s.push_str("::");
        s.push_str(&self.name);
        s
    }

    /// `"use crate::domain::common::worker::dto_response::WorkerResponse;"`
    pub fn use_statement(&self) -> String {
        format!("use {};", self.full_path())
    }
}

/// Trait for resolving type names to their canonical import paths.
pub trait TypeRegistry: Send + Sync {
    fn register(&mut self, type_ref: TypeRef) -> Result<(), String>;
    fn resolve(&self, name: &str) -> Option<&TypeRef>;
    fn imports_needed(&self, names: &[String], caller_base: &[String]) -> Vec<String>;
}

/// In-memory HashMap-backed implementation of [`TypeRegistry`].
///
/// Enforces that each type name maps to exactly one module path.
/// Re-registering the same name with the same path is idempotent;
/// registering with a *different* path returns an error.
pub struct InMemoryTypeRegistry {
    map: std::collections::HashMap<String, TypeRef>,
}

impl InMemoryTypeRegistry {
    pub fn new() -> Self {
        Self {
            map: std::collections::HashMap::new(),
        }
    }
}

impl TypeRegistry for InMemoryTypeRegistry {
    fn register(&mut self, type_ref: TypeRef) -> Result<(), String> {
        if let Some(existing) = self.map.get(&type_ref.name) {
            if existing.module_path != type_ref.module_path {
                return Err(format!(
                    "Type '{}' already registered with path '{}', cannot register with '{}'",
                    type_ref.name,
                    existing.module_path.join("::"),
                    type_ref.module_path.join("::"),
                ));
            }
            return Ok(());
        }
        self.map.insert(type_ref.name.clone(), type_ref);
        Ok(())
    }

    fn resolve(&self, name: &str) -> Option<&TypeRef> {
        self.map.get(name)
    }

    fn imports_needed(&self, names: &[String], caller_base: &[String]) -> Vec<String> {
        // When the caller is a repository_impl, its entity's dto_response
        // types are already imported wholesale by the header's
        // `use super::dto_response::…` — emitting the full-path form for
        // the same types is an E0252 duplicate.
        let own_dto_response: Option<Vec<String>> =
            if caller_base.last().map(|s| s.as_str()) == Some("repository_impl") {
                let mut module = caller_base[..caller_base.len() - 1].to_vec();
                module.push("dto_response".into());
                Some(module)
            } else {
                None
            };
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for name in names {
            // Try exact match; if that fails, try appending "Response"
            let candidate = if self.map.contains_key(name.as_str()) {
                name.clone()
            } else {
                let with_suffix = format!("{}Response", name);
                if self.map.contains_key(&with_suffix) {
                    with_suffix
                } else {
                    continue;
                }
            };
            if let Some(tr) = self.map.get(&candidate)
                && tr.module_path != caller_base
                && Some(&tr.module_path) != own_dto_response.as_ref()
                && seen.insert(candidate.clone())
            {
                result.push(tr.use_statement());
            }
        }
        result
    }
}

impl Default for InMemoryTypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Per-run handle + ambient compatibility shims
// ---------------------------------------------------------------------------

/// Cheap cloneable handle to one type registry (issue #746).
///
/// The pipeline creates one per `run_generators_with_opts` and threads it
/// into the generators that register types or resolve imports. Cloning the
/// handle shares the SAME underlying registry — that is what lets a DTO
/// generator's registrations resolve in the handler generator of the same
/// run, while a different run's handle is a different registry.
#[derive(Clone)]
pub struct TypeRegistryHandle(Arc<Mutex<InMemoryTypeRegistry>>);

impl TypeRegistryHandle {
    /// An empty registry.
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(InMemoryTypeRegistry::new())))
    }

    /// An empty registry pre-seeded with the framework types
    /// (`AppState`, `AppError`, `ApiKeyInfo`, …).
    pub fn with_framework_types() -> Self {
        let handle = Self::new();
        handle.register_framework_types();
        handle
    }

    /// The process-ambient registry.
    ///
    /// This is the compatibility fallback for directly-constructed
    /// generators (consumer test harnesses, template harnesses). Pipeline
    /// runs MUST NOT use it — they thread their own [`TypeRegistryHandle`].
    pub fn ambient() -> Self {
        ambient_handle().clone()
    }

    /// Register a type by name and module path.
    ///
    /// Re-registering the same name with the same path is idempotent. A
    /// DIFFERENT path is a real modeling conflict: the first registration
    /// wins and a warning names both paths (issue #746 — this used to be
    /// swallowed silently, which let two apps in one process resolve each
    /// other's modules).
    pub fn register_type(&self, name: &str, module_path: Vec<String>) {
        let type_ref = TypeRef {
            name: name.to_string(),
            module_path,
        };
        let mut reg = self.lock();
        if let Err(conflict) = reg.register(type_ref) {
            tracing::warn!("type-registry conflict: {conflict}");
        }
    }

    /// Resolve import `use` statements for a list of type names from a
    /// caller's base path.
    pub fn resolve_imports(&self, names: &[String], caller_base: &[String]) -> Vec<String> {
        self.lock().imports_needed(names, caller_base)
    }

    /// Register framework types commonly used by generated code.
    pub fn register_framework_types(&self) {
        for (name, path) in FRAMEWORK_TYPES {
            self.register_type(name, path.iter().map(|s| (*s).to_string()).collect());
        }
    }

    fn lock(&self) -> MutexGuard<'_, InMemoryTypeRegistry> {
        // A panic while holding the lock must not poison every later
        // generation in the process — the registry is best-effort metadata.
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for TypeRegistryHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Framework types every generated app exposes, with their canonical paths.
const FRAMEWORK_TYPES: &[(&str, &[&str])] = &[
    ("AppState", &["crate", "app_state"]),
    ("AppError", &["crate", "error"]),
    ("BulkItemError", &["crate", "error"]),
    ("ApiKeyInfo", &["crate", "middleware"]),
    ("AuthInfo", &["crate", "middleware"]),
    ("Links", &["crate", "api", "links"]),
    ("Meta", &["crate", "api", "meta"]),
    ("HookRegistry", &["crate", "hooks"]),
];

/// The process-ambient registry (compatibility surface — see the module
/// docs). Created lazily on first use.
static AMBIENT_TYPE_REGISTRY: OnceLock<TypeRegistryHandle> = OnceLock::new();

fn ambient_handle() -> &'static TypeRegistryHandle {
    AMBIENT_TYPE_REGISTRY.get_or_init(TypeRegistryHandle::new)
}

/// Register a type by name and module path in the AMBIENT registry.
///
/// Compatibility shim for consumers that register before constructing
/// generators directly. Pipeline runs register into their per-run handle
/// and never read this.
pub fn register_type(name: &str, module_path: Vec<String>) {
    ambient_handle().register_type(name, module_path);
}

/// Resolve import `use` statements from the AMBIENT registry.
///
/// Compatibility shim — pipeline runs resolve through their per-run handle.
pub fn resolve_imports(names: &[String], caller_base: &[String]) -> Vec<String> {
    ambient_handle().resolve_imports(names, caller_base)
}

/// Register framework types commonly used by generated code into the
/// AMBIENT registry.
///
/// Kept for public-API compatibility: hr-specs calls this in test setup
/// before direct-construction generators (`HandlerGenerator::new(..)`),
/// which default to the ambient handle. Pipeline runs seed their own
/// per-run handle via [`TypeRegistryHandle::with_framework_types`].
pub fn register_framework_types() {
    ambient_handle().register_framework_types();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_needed_skips_caller_own_dto_response_types() {
        // The caller is worker's repository_impl; its sibling dto_response
        // module is imported wholesale by the header's
        // `use super::dto_response::…` glob — emitting the full-path form
        // for the same types is an E0252 duplicate.
        let mut reg = InMemoryTypeRegistry::new();
        reg.register(TypeRef {
            name: "WorkerResponse".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "core".into(),
                "worker".into(),
                "dto_response".into(),
            ],
        })
        .unwrap();
        reg.register(TypeRef {
            name: "OrganizationResponse".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "core".into(),
                "organization".into(),
                "dto_response".into(),
            ],
        })
        .unwrap();
        let caller_base = vec![
            "crate".into(),
            "domain".into(),
            "core".into(),
            "worker".into(),
            "repository_impl".into(),
        ];
        let imports = reg.imports_needed(
            &["WorkerResponse".into(), "OrganizationResponse".into()],
            &caller_base,
        );
        assert_eq!(imports.len(), 1, "got {imports:?}");
        assert!(
            imports[0].contains("organization"),
            "the cross-module import must survive: {imports:?}"
        );
    }

    fn make_registry() -> InMemoryTypeRegistry {
        let mut reg = InMemoryTypeRegistry::new();
        reg.register(TypeRef {
            name: "WorkerResponse".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "common".into(),
                "worker".into(),
                "dto_response".into(),
            ],
        })
        .unwrap();
        reg.register(TypeRef {
            name: "WorkerRepository".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "common".into(),
                "worker".into(),
                "repository".into(),
            ],
        })
        .unwrap();
        reg.register(TypeRef {
            name: "OrderResponse".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "assessments".into(),
                "order".into(),
                "dto_response".into(),
            ],
        })
        .unwrap();
        reg
    }

    #[test]
    fn test_type_ref_full_path() {
        let tr = TypeRef {
            name: "WorkerResponse".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "common".into(),
                "worker".into(),
                "dto_response".into(),
            ],
        };
        assert_eq!(
            tr.full_path(),
            "crate::domain::common::worker::dto_response::WorkerResponse"
        );
    }

    #[test]
    fn test_type_ref_use_statement() {
        let tr = TypeRef {
            name: "WorkerResponse".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "common".into(),
                "worker".into(),
                "dto_response".into(),
            ],
        };
        assert_eq!(
            tr.use_statement(),
            "use crate::domain::common::worker::dto_response::WorkerResponse;"
        );
    }

    #[test]
    fn test_in_memory_register_and_resolve() {
        let mut reg = InMemoryTypeRegistry::new();
        reg.register(TypeRef {
            name: "FooResponse".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "test".into(),
                "foo".into(),
                "dto_response".into(),
            ],
        })
        .unwrap();
        let resolved = reg.resolve("FooResponse");
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().name, "FooResponse");
    }

    #[test]
    fn test_register_duplicate_mismatch() {
        let mut reg = InMemoryTypeRegistry::new();
        reg.register(TypeRef {
            name: "FooResponse".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "test".into(),
                "foo".into(),
                "dto_response".into(),
            ],
        })
        .unwrap();
        let err = reg.register(TypeRef {
            name: "FooResponse".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "test".into(),
                "bar".into(),
                "dto_response".into(),
            ],
        });
        assert!(err.is_err());
    }

    #[test]
    fn test_register_duplicate_same() {
        let mut reg = InMemoryTypeRegistry::new();
        reg.register(TypeRef {
            name: "FooResponse".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "test".into(),
                "foo".into(),
                "dto_response".into(),
            ],
        })
        .unwrap();
        let result = reg.register(TypeRef {
            name: "FooResponse".into(),
            module_path: vec![
                "crate".into(),
                "domain".into(),
                "test".into(),
                "foo".into(),
                "dto_response".into(),
            ],
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_nonexistent() {
        let reg = InMemoryTypeRegistry::new();
        assert!(reg.resolve("DoesNotExist").is_none());
    }

    #[test]
    fn separate_handles_do_not_share_registrations() {
        // Issue #746: the whole point of the handle — two runs in one
        // process must resolve their own module paths only.
        let run_a = TypeRegistryHandle::new();
        let run_b = TypeRegistryHandle::new();
        run_a.register_type(
            "PayRunResponse",
            ["crate", "domain", "payroll", "pay_run", "dto_response"]
                .map(str::to_string)
                .to_vec(),
        );
        assert_eq!(
            run_b.resolve_imports(&["PayRunResponse".into()], &[]),
            Vec::<String>::new(),
            "a sibling run must not see the first run's registrations"
        );
        assert_eq!(
            run_a.resolve_imports(&["PayRunResponse".into()], &[]),
            vec!["use crate::domain::payroll::pay_run::dto_response::PayRunResponse;"]
        );
    }

    #[test]
    fn cloned_handle_shares_the_same_registry() {
        let run = TypeRegistryHandle::new();
        let same_run = run.clone();
        run.register_type(
            "PayRunResponse",
            ["crate", "domain", "payroll", "pay_run", "dto_response"]
                .map(str::to_string)
                .to_vec(),
        );
        assert_eq!(
            same_run.resolve_imports(&["PayRunResponse".into()], &[]),
            vec!["use crate::domain::payroll::pay_run::dto_response::PayRunResponse;"]
        );
    }

    #[test]
    fn with_framework_types_resolves_app_state() {
        let registry = TypeRegistryHandle::with_framework_types();
        assert_eq!(
            registry.resolve_imports(&["AppState".into()], &[]),
            vec!["use crate::app_state::AppState;"]
        );
    }

    #[test]
    fn conflicting_registration_keeps_the_first_path() {
        // A real modeling conflict stays a warning (not a hard error inside
        // generation) — but the first registrant must not be silently
        // REPLACED either.
        let registry = TypeRegistryHandle::new();
        registry.register_type(
            "SharedResponse",
            ["crate", "domain", "a", "shared", "dto_response"]
                .map(str::to_string)
                .to_vec(),
        );
        registry.register_type(
            "SharedResponse",
            ["crate", "domain", "b", "shared", "dto_response"]
                .map(str::to_string)
                .to_vec(),
        );
        assert_eq!(
            registry.resolve_imports(&["SharedResponse".into()], &[]),
            vec!["use crate::domain::a::shared::dto_response::SharedResponse;"]
        );
    }

    #[test]
    fn test_imports_needed_same_module() {
        let reg = make_registry();
        let caller: Vec<String> = vec![
            "crate".into(),
            "domain".into(),
            "common".into(),
            "worker".into(),
            "dto_response".into(),
        ];
        let imports = reg.imports_needed(&["WorkerResponse".into()], &caller);
        assert!(imports.is_empty(), "same module should produce no imports");
    }

    #[test]
    fn test_imports_needed_cross_module() {
        let reg = make_registry();
        let caller: Vec<String> = vec![
            "crate".into(),
            "domain".into(),
            "common".into(),
            "worker".into(),
            "dto_included".into(),
        ];
        let imports = reg.imports_needed(&["OrderResponse".into()], &caller);
        assert_eq!(imports.len(), 1);
        assert_eq!(
            imports[0],
            "use crate::domain::assessments::order::dto_response::OrderResponse;"
        );
    }
}
