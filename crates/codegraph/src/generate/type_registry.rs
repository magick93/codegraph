use std::sync::{Mutex, OnceLock};

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
            if let Some(tr) = self.map.get(&candidate) {
                if tr.module_path != caller_base && seen.insert(candidate.clone()) {
                    result.push(tr.use_statement());
                }
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
// Global singleton — same pattern as ProjectConfig in mod.rs
// ---------------------------------------------------------------------------

static TYPE_REGISTRY: OnceLock<Mutex<InMemoryTypeRegistry>> = OnceLock::new();

/// Must be called at the start of the generation pipeline before any generator runs.
pub fn init_type_registry() {
    TYPE_REGISTRY
        .set(Mutex::new(InMemoryTypeRegistry::new()))
        .ok();
}

fn with_registry<F, R>(f: F) -> R
where
    F: FnOnce(&mut InMemoryTypeRegistry) -> R,
{
    let lock = TYPE_REGISTRY.get_or_init(|| Mutex::new(InMemoryTypeRegistry::new()));
    let mut reg = lock.lock().unwrap();
    f(&mut reg)
}

/// Register a type by name and module path for cross-generator import resolution.
pub fn register_type(name: &str, module_path: Vec<String>) {
    with_registry(|reg| {
        let type_ref = TypeRef {
            name: name.to_string(),
            module_path,
        };
        reg.register(type_ref).ok();
    });
}

/// Resolve import `use` statements for a list of type names from a caller's base path.
pub fn resolve_imports(names: &[String], caller_base: &[String]) -> Vec<String> {
    with_registry(|reg| reg.imports_needed(names, caller_base))
}

/// Register framework types commonly used by generated code.
/// Called at pipeline start; also callable from test setup.
pub fn register_framework_types() {
    register_type("AppState", vec!["crate".into(), "app_state".into()]);
    register_type("AppError", vec!["crate".into(), "error".into()]);
    register_type("BulkItemError", vec!["crate".into(), "error".into()]);
    register_type("ApiKeyInfo", vec!["crate".into(), "middleware".into()]);
    register_type("AuthInfo", vec!["crate".into(), "middleware".into()]);
    register_type("Links", vec!["crate".into(), "api".into(), "links".into()]);
    register_type("Meta", vec!["crate".into(), "api".into(), "meta".into()]);
    register_type("HookRegistry", vec!["crate".into(), "hooks".into()]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> InMemoryTypeRegistry {
        let mut reg = InMemoryTypeRegistry::new();
        reg.register(TypeRef {
            name: "WorkerResponse".into(),
            module_path: vec!["crate".into(), "domain".into(), "common".into(), "worker".into(), "dto_response".into()],
        })
        .unwrap();
        reg.register(TypeRef {
            name: "WorkerRepository".into(),
            module_path: vec!["crate".into(), "domain".into(), "common".into(), "worker".into(), "repository".into()],
        })
        .unwrap();
        reg.register(TypeRef {
            name: "OrderResponse".into(),
            module_path: vec!["crate".into(), "domain".into(), "assessments".into(), "order".into(), "dto_response".into()],
        })
        .unwrap();
        reg
    }

    #[test]
    fn test_type_ref_full_path() {
        let tr = TypeRef {
            name: "WorkerResponse".into(),
            module_path: vec!["crate".into(), "domain".into(), "common".into(), "worker".into(), "dto_response".into()],
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
            module_path: vec!["crate".into(), "domain".into(), "common".into(), "worker".into(), "dto_response".into()],
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
            module_path: vec!["crate".into(), "domain".into(), "test".into(), "foo".into(), "dto_response".into()],
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
            module_path: vec!["crate".into(), "domain".into(), "test".into(), "foo".into(), "dto_response".into()],
        })
        .unwrap();
        let err = reg.register(TypeRef {
            name: "FooResponse".into(),
            module_path: vec!["crate".into(), "domain".into(), "test".into(), "bar".into(), "dto_response".into()],
        });
        assert!(err.is_err());
    }

    #[test]
    fn test_register_duplicate_same() {
        let mut reg = InMemoryTypeRegistry::new();
        reg.register(TypeRef {
            name: "FooResponse".into(),
            module_path: vec!["crate".into(), "domain".into(), "test".into(), "foo".into(), "dto_response".into()],
        })
        .unwrap();
        let result = reg.register(TypeRef {
            name: "FooResponse".into(),
            module_path: vec!["crate".into(), "domain".into(), "test".into(), "foo".into(), "dto_response".into()],
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_nonexistent() {
        let reg = InMemoryTypeRegistry::new();
        assert!(reg.resolve("DoesNotExist").is_none());
    }

    #[test]
    fn test_imports_needed_same_module() {
        let reg = make_registry();
        let caller: Vec<String> = vec!["crate".into(), "domain".into(), "common".into(), "worker".into(), "dto_response".into()];
        let imports = reg.imports_needed(&["WorkerResponse".into()], &caller);
        assert!(imports.is_empty(), "same module should produce no imports");
    }

    #[test]
    fn test_imports_needed_cross_module() {
        let reg = make_registry();
        let caller: Vec<String> = vec!["crate".into(), "domain".into(), "common".into(), "worker".into(), "dto_included".into()];
        let imports = reg.imports_needed(&["OrderResponse".into()], &caller);
        assert_eq!(imports.len(), 1);
        assert_eq!(
            imports[0],
            "use crate::domain::assessments::order::dto_response::OrderResponse;"
        );
    }
}
