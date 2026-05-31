use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub static GRAFE: OnceLock<Mutex<Option<GrafeoState>>> = OnceLock::new();

pub fn init_grafe(state: GrafeoState) {
    let lock = GRAFE.get_or_init(|| Mutex::new(None));
    *lock.lock().unwrap() = Some(state);
}

pub fn with_grafe<F, R>(f: F) -> R
where
    F: FnOnce(&GrafeoState) -> R,
{
    let lock = GRAFE.get().expect("GRAFE not initialized");
    let guard = lock.lock().unwrap();
    f(guard.as_ref().expect("GRAFE not initialized"))
}

pub struct GrafeoState {
    pub entity_names: Vec<String>,
    pub schema_infos: HashMap<String, SchemaInfo>,
    pub schema_dirs: Vec<std::path::PathBuf>,
}

impl Default for GrafeoState {
    fn default() -> Self {
        Self {
            entity_names: Vec::new(),
            schema_infos: HashMap::new(),
            schema_dirs: Vec::new(),
        }
    }
}

pub struct SchemaInfo {
    pub title: String,
    pub description: Option<String>,
    pub properties: Vec<String>,
    pub rel_path: String,
}
