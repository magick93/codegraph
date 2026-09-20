use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

pub static GRAFE: OnceLock<Mutex<Option<GrafeoState>>> = OnceLock::new();

pub fn init_grafe(state: GrafeoState) {
    let lock = GRAFE.get_or_init(|| Mutex::new(None));
    let mut guard = lock.lock().unwrap();
    *guard = Some(state);
}

pub fn with_grafe<F, R>(f: F) -> R
where
    F: FnOnce(&GrafeoState) -> R,
{
    let lock = GRAFE.get().expect("GRAFE not initialized");
    let guard = lock.lock().unwrap();
    match guard.as_ref() {
        Some(s) => f(s),
        None => panic!("GRAFE not initialized"),
    }
}

#[derive(Default)]
pub struct GrafeoState {
    pub entity_names: Vec<String>,
    pub schema_infos: HashMap<String, SchemaInfo>,
    pub schema_dirs: Vec<std::path::PathBuf>,
}

pub struct SchemaInfo {
    pub title: String,
    pub description: Option<String>,
    pub properties: Vec<String>,
    pub rel_path: String,
}

// ── .mox support (MoxState) ────────────────────────────────────────────
//
// The model the LSP serves `.mox` documents against: declarations compiled
// from the `--mox-files` startup flags plus the `import schema` aliases they
// declare. Serde-friendly so it can be serialized for debugging/tests.

pub static MOX: OnceLock<Mutex<Option<MoxState>>> = OnceLock::new();

/// Install (or clear) the workspace's mox model. `None` is the no-
/// `--mox-files` state: mox diagnostics and completions stay quiet.
pub fn init_mox(state: Option<MoxState>) {
    let lock = MOX.get_or_init(|| Mutex::new(None));
    let mut guard = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = state;
}

/// Run `f` with the current mox model (`None` when absent — callers degrade
/// quietly; mirrors the handlers' Option-taking grafe accessor).
pub fn with_mox_opt<F, R>(f: F) -> R
where
    F: FnOnce(Option<&MoxState>) -> R,
{
    let guard = MOX
        .get()
        .map(|l| l.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    f(guard.as_ref().and_then(|g| g.as_ref()))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MoxState {
    pub classes: Vec<MoxClassInfo>,
    pub enums: Vec<MoxEnumInfo>,
    pub datatypes: Vec<MoxDatatypeInfo>,
    pub vocabularies: Vec<MoxVocabularyInfo>,
    /// `import schema "<path>" (as <Alias>)?` declarations from the startup
    /// files, with the alias resolution replayed from the ingest pipeline's
    /// `wire_alias_refs` (exact title, then title + type suffix).
    pub import_aliases: Vec<ImportAliasInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoxClassInfo {
    pub name: String,
    pub package: String,
    /// Stored features (attribute/containment/reference) as `(name, kind)`;
    /// derived features and operations are excluded.
    pub features: Vec<MoxFeatureInfo>,
    /// `extends` target names as written (bare).
    pub extends: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoxFeatureInfo {
    pub name: String,
    /// `attribute` | `containment` | `reference`.
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoxEnumInfo {
    pub name: String,
    pub package: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoxDatatypeInfo {
    pub name: String,
    pub package: String,
    /// The `format "<hint>"` entry, when declared.
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoxVocabularyInfo {
    pub name: String,
    pub package: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportAliasInfo {
    /// The namespace name the import joins: the `as` alias, or the import
    /// path's file stem.
    pub alias: String,
    /// Absolute path resolved against the importing .mox file's directory.
    pub abs_path: String,
    /// Schema title the alias resolves to, when determinable (exact title
    /// match first, then title + type suffix — the `wire_alias_refs` order).
    pub resolved_title: Option<String>,
}
