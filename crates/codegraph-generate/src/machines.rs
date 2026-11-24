//! Machine kernel (issue #815 — Lane 1 of epic #812).
//!
//! The compiled `.ddd` artifact embeds, per referenced machine, the
//! canonical `xstate_ir::Machine` JSON (`machine_ir`, keys sorted
//! recursively by the pherolang compiler) plus its provenance
//! (`machine_provenance`: authored path, machine id, content SHA-256).
//! This plane makes machines FIRST-CLASS serialized artifacts of a
//! generated app:
//!
//! - **Mirror** (Commit B): the blobs ride `DddDesignNode` through the
//!   graph (Mock + Grafeo), `GRAPH_FORMAT_VERSION` 14 → 15.
//! - **Fail-closed validation** (`machine_kernel_refusals`): a
//!   structurally broken machine (`xstate_core::validate_machine_json` —
//!   the restingstate fail-closed boundary) or a machine state outside
//!   its bound codelist enum (the enum correspondence is OURS — the
//!   artifact validates machine-internal structure only) refuses the run
//!   by name BEFORE any tree is written (`ValidationPass::check_machine_
//!   kernel`, the DD9 `query_refusals` posture).
//! - **Emitter** (`MachineKernelGenerator`, the `evt_events`/
//!   `media_storage` precedent — registers unconditionally,
//!   presence-gated inside): one
//!   `{domain_types}/src/machines/{domain}/{machine_id}.ir.json` per
//!   machine + a generated `machines/mod.rs` registry embedding each via
//!   `include_str!` with parse-once accessors. Static data only — no DB
//!   handle, no AppState, no async — so the module compiles on wasm32 and
//!   BOTH topologies reach it through the shared domain-types crate (the
//!   mox_ops placement precedent; worker crates already depend on
//!   domain-types).
//! - **Determinism**: the emitted JSON is the canonical serialization
//!   ([`canonical_ir_json`] — keys sorted recursively). The artifact's IR
//!   arrives canonical already, so embedding is byte-stable run to run
//!   even though the IR carries `HashMap`-borne structures (`SetupConfig`
//!   maps); Stage 2 drift checks must project through the same helper.
//!
//! Provenance (`machine_id`, authored source path, `content_sha256`) is
//! surfaced on the emitted module as consts — the drift-detection seam
//! Stages 2/3 consume. No `MachineRuntime` here (Stage 2), no workflow or
//! IFML wiring (Stages 3–4).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::DddDesignNode;
use serde_json::Value;

use crate::ProjectConfig;
use crate::error::{Error, Result};
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};

/// The canonical serialization of a machine IR: keys sorted recursively,
/// compact separators — the pherolang `machineIr` embed form (upstream
/// canonicalizes before embedding; re-projecting ANY machine JSON through
/// this helper is the determinism contract, because the IR carries
/// `HashMap`-borne structures whose iteration order varies run to run).
/// Stage 2 drift checks hash/compare THROUGH this helper, never raw
/// `serde_json` serialization.
pub fn canonical_ir_json(value: &Value) -> String {
    phero_ir::canonical_json(value)
}

/// One machine to emit, collected from the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineEntry {
    /// The authored `createMachine` id — the registry key and the file
    /// stem. Application-unique (duplicates refuse).
    pub machine_id: String,
    /// The declaring design's module — the `.ddd` module = domain
    /// convention, and the `machines/{domain}/…` directory segment.
    pub domain: String,
    /// The provenance path (the authored TS file as written in the
    /// `.ddd`), when the design carries provenance.
    pub source_path: Option<String>,
    /// The provenance SHA-256 over the canonical IR.
    pub content_sha256: Option<String>,
    /// The canonical IR JSON (the embedded bytes).
    pub ir_json: String,
}

/// The machine id exactly as the design records it: the provenance's
/// `machineId` when present, else the IR's own `id`.
fn machine_id_of(design: &DddDesignNode) -> Option<String> {
    if let Some(id) = design
        .machine_provenance_json
        .as_ref()
        .and_then(|prov| prov.get("machineId"))
        .and_then(Value::as_str)
    {
        return Some(id.to_string());
    }
    design
        .machine_ir_json
        .as_ref()
        .and_then(|ir| ir.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The bound enum type name of the design's DSL machine mirror
/// (`machine <field>: <Enum>`), when the design carries one.
fn bound_enum_of(design: &DddDesignNode) -> Option<String> {
    let machine = design.machine_json.as_ref()?;
    let enum_type = machine.get("enumType")?;
    match enum_type {
        Value::String(name) => Some(name.clone()),
        Value::Object(map) => map
            .get("value")
            .and_then(|v| v.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

/// A machine id must be a safe file stem: no separators, no `..`, no
/// control characters — it becomes `{machine_id}.ir.json` under
/// `machines/{domain}/`.
fn machine_id_is_file_safe(machine_id: &str) -> bool {
    !machine_id.is_empty()
        && machine_id != "."
        && machine_id != ".."
        && !machine_id.contains(['/', '\\'])
        && !machine_id.chars().any(char::is_control)
        && !machine_id.contains("..")
}

/// Collect the machine entries of the graph, sorted by (domain,
/// machine_id), refusing duplicates.
async fn collect_machine_entries(
    db: &dyn GraphQuerier,
) -> std::result::Result<Vec<MachineEntry>, GraphError> {
    let models = db.get_ddd_models().await?;
    let mut entries: Vec<MachineEntry> = Vec::new();
    for model in &models {
        for design in &model.designs {
            let Some(ir) = &design.machine_ir_json else {
                continue;
            };
            let Some(machine_id) = machine_id_of(design) else {
                // The refusals walker names the defect; collection skips.
                continue;
            };
            let provenance = design.machine_provenance_json.as_ref();
            entries.push(MachineEntry {
                machine_id,
                domain: design.module.clone(),
                source_path: provenance
                    .and_then(|p| p.get("path"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                content_sha256: provenance
                    .and_then(|p| p.get("contentSha256"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                ir_json: canonical_ir_json(ir),
            });
        }
    }
    entries.sort_by(|a, b| (&a.domain, &a.machine_id).cmp(&(&b.domain, &b.machine_id)));
    Ok(entries)
}

/// Fail-closed validation (issue #815): every referenced machine must be
/// structurally valid (the restingstate boundary), its provenance must
/// agree with the IR, its id must be a safe file stem, and every machine
/// state must be a value of the bound codelist enum. Returned messages
/// are HARD pre-generation errors (`ValidationPass::check_machine_kernel`)
/// AND the emitter's own gate — a broken machine never reaches a tree.
pub async fn machine_kernel_refusals(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> std::result::Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    let suffix = &config.defaults.type_suffix;
    let mut refusals = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut const_names: BTreeMap<String, String> = BTreeMap::new();

    for model in &models {
        for design in &model.designs {
            let Some(ir) = &design.machine_ir_json else {
                continue;
            };
            let where_ = format!(
                "machine `{}` of class `{}` (domain `{}`)",
                machine_id_of(design).unwrap_or_else(|| "<unresolved id>".to_string()),
                design.class,
                design.module,
            );

            // (a) The IR must be an xstate-ir `Machine` AND structurally
            // valid — `validate_machine_json` is the fail-closed boundary
            // restingstate ships (`Parse` + `Invalid` arms both named).
            let typed: std::result::Result<xstate_core::xstate_ir::Machine, _> =
                serde_json::from_value(ir.clone());
            let machine = match typed {
                Ok(machine) => machine,
                Err(e) => {
                    refusals.push(format!(
                        "{where_}: the embedded machine_ir is not a valid xstate-ir Machine \
                         ({e}) — the artifact is stale or hand-edited; recompile the model",
                    ));
                    continue;
                }
            };
            let canonical = canonical_ir_json(ir);
            if let Err(e) = xstate_core::validate_machine_json(&canonical) {
                refusals.push(format!(
                    "{where_}: the machine is structurally invalid ({e}) — fix the authored \
                     TypeScript/XState file and recompile",
                ));
                continue;
            }

            // Provenance agreement: the recorded machineId must be the
            // IR's own id (a mismatch is a tampered/stale embed).
            if let Some(prov) = &design.machine_provenance_json {
                let prov_id = prov.get("machineId").and_then(Value::as_str);
                if prov_id != Some(machine.id.as_str()) {
                    refusals.push(format!(
                        "{where_}: the provenance machineId `{}` does not match the machine \
                         IR's id `{}` — the artifact is stale or tampered",
                        prov_id.unwrap_or("<absent>"),
                        machine.id,
                    ));
                    continue;
                }
            }

            // File-safety: the id becomes the data file's stem.
            if !machine_id_is_file_safe(&machine.id) {
                refusals.push(format!(
                    "{where_}: the machine id is not a safe file stem — use an identifier \
                     (letters, digits, `-`/`_`)",
                ));
                continue;
            }

            // Registry-key + const-name uniqueness.
            if !seen_ids.insert(machine.id.clone()) {
                refusals.push(format!(
                    "{where_}: duplicate machine id `{}` — machine ids are application-unique \
                     registry keys; rename one of the machines",
                    machine.id,
                ));
                continue;
            }
            let const_name = registry_const_name(&machine.id);
            if let Some(other) = const_names.get(&const_name) {
                refusals.push(format!(
                    "{where_}: machine id `{}` and `{}` both render the registry const \
                     `{const_name}` — rename one of the machines",
                    other, machine.id,
                ));
                continue;
            }
            const_names.insert(const_name, machine.id.clone());

            // A machine may not live in a domain named after the kernel
            // home — the domain's entity modules and the kernel registry
            // would share one directory (`src/machines/`).
            if design.module == "machines" {
                refusals.push(format!(
                    "{where_}: the declaring module is named `machines` — that name is the \
                     machine kernel's directory inside the domain-types crate \
                     (src/machines/); rename the module",
                ));
                continue;
            }

            // (b) The enum correspondence (OUR check): every machine
            // state must be a value of the bound codelist enum. The bound
            // enum comes from the design's DSL mirror (`machine <field>:
            // <Enum>`); a design carrying only the IR has no enum to
            // check against — the arm is skipped, never guessed.
            let Some(enum_name) = bound_enum_of(design) else {
                continue;
            };
            let codelist = resolve_codelist(db, &enum_name, suffix).await?;
            let Some(codelist) = codelist else {
                refusals.push(format!(
                    "{where_}: the bound enum `{enum_name}` resolves to no codelist in the \
                     graph — declare the enum in the domain model (the machine's states are \
                     its literals)",
                ));
                continue;
            };
            let values = db.get_enum_values(&codelist).await?;
            let known: BTreeSet<&str> = values.iter().map(|v| v.value.as_str()).collect();
            for state in &machine.states {
                if !known.contains(state.name.as_str()) {
                    refusals.push(format!(
                        "{where_}: state `{}` is not a value of the bound enum `{}` — the \
                         machine's states must be literals of the enum the state field binds \
                         (declared values: {})",
                        state.name,
                        enum_name,
                        known.iter().copied().collect::<Vec<_>>().join(", ")
                    ));
                }
            }
        }
    }
    refusals.sort();
    Ok(refusals)
}

/// Resolve the authored enum name to a codelist name: exact, then the
/// suffix-stripped spelling, then title + suffix (the
/// `resolve_with_suffix` convention, both directions).
async fn resolve_codelist(
    db: &dyn GraphQuerier,
    enum_name: &str,
    suffix: &str,
) -> std::result::Result<Option<String>, GraphError> {
    let candidates = [
        enum_name.to_string(),
        codegraph_naming::strip_suffix(enum_name, suffix),
        format!("{enum_name}{suffix}"),
    ];
    for candidate in candidates {
        if db.get_codelist(&candidate).await?.is_some() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// The registry const prefix for a machine id: SCREAMING_SNAKE, forced
/// into a valid Rust identifier (a leading digit gains the `MACHINE_`
/// prefix).
fn registry_const_name(machine_id: &str) -> String {
    let mut name = codegraph_naming::to_snake_case(machine_id).to_uppercase();
    if name.is_empty() {
        name = "MACHINE".to_string();
    }
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        name = format!("MACHINE_{name}");
    }
    name
}

/// Emits the machine kernel into the domain-types crate: one canonical IR
/// JSON per machine under `src/machines/{domain}/` plus the registry
/// module `src/machines/mod.rs` (static data, parse-once accessors,
/// provenance consts). Registers unconditionally (the `functions_sql`/
/// `media_storage` precedent) and is presence-gated inside: a graph with
/// no machine IR emits ZERO files.
pub struct MachineKernelGenerator {
    /// The domain-types crate root — `src/` is appended internally (the
    /// `dto_boundaries`/`mox_ops` placement: the domain-types crate is
    /// sea_orm-free and wasm32-safe, and worker crates depend on it, so
    /// one home serves both topologies).
    src_dir: PathBuf,
}

impl MachineKernelGenerator {
    /// Creates a generator that writes under `base_dir` (the domain-types
    /// crate root), appending `src/` internally.
    pub fn new_with_base(base_dir: PathBuf) -> Self {
        Self {
            src_dir: base_dir.join("src"),
        }
    }
}

#[async_trait]
impl GlobalGenerator for MachineKernelGenerator {
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::MachineKernel
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[crate::GenerationEntry],
        _tera: &tera::Tera,
        _project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Fail closed BEFORE any tree: the same walk the pre-generation
        // validation pass runs. The domain-generator phase collects its
        // own errors non-fatally, so both gates are what make a broken
        // machine refuse the run.
        let refusals = machine_kernel_refusals(db, config).await?;
        if !refusals.is_empty() {
            return Err(Error::Config(format!(
                "machine kernel — {} referenced machine(s) refused:\n{}",
                refusals.len(),
                refusals
                    .iter()
                    .map(|r| format!("  - {r}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )));
        }

        let entries = collect_machine_entries(db).await?;
        let machines_dir = self.src_dir.join("machines");
        if entries.is_empty() {
            // Presence gating: no machine IR ⇒ nothing emitted. A stale
            // kernel from a previous run is swept — but ONLY kernel-shaped
            // files (mod.rs + *.ir.json), never a real domain directory
            // that happens to be named `machines`.
            sweep_stale_kernel(&machines_dir, &BTreeSet::new());
            return Ok(Vec::new());
        }

        let mut keep: BTreeSet<String> = BTreeSet::new();
        let mut files = Vec::new();
        for entry in &entries {
            let rel = format!("machines/{}/{}.ir.json", entry.domain, entry.machine_id);
            keep.insert(rel.clone());
            files.push(GeneratedFile {
                path: self.src_dir.join(&rel),
                content: format!("{}\n", entry.ir_json),
            });
        }
        files.push(GeneratedFile {
            path: machines_dir.join("mod.rs"),
            content: registry_module(&entries),
        });
        keep.insert("machines/mod.rs".to_string());
        sweep_stale_kernel(&machines_dir, &keep);

        Ok(files)
    }
}

/// Sweeps kernel-shaped files from a previous run that the current model
/// no longer carries: `mod.rs` and `**/*.ir.json` under the kernel dir.
/// Anything else (a real domain directory's entity modules) is never
/// touched.
fn sweep_stale_kernel(machines_dir: &Path, keep: &BTreeSet<String>) {
    let Ok(entries) = fs::read_dir(machines_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let rel = format!("machines/{}", entry.file_name().to_string_lossy());
            if rel == "machines/mod.rs" && !keep.contains(&rel) {
                let _ = fs::remove_file(&path);
            }
            continue;
        }
        // A domain subdirectory: remove only when it carries nothing but
        // `.ir.json` files (and none of them are kept).
        let Ok(domain_entries) = fs::read_dir(&path) else {
            continue;
        };
        let mut ir_files = Vec::new();
        let mut foreign = false;
        for de in domain_entries.flatten() {
            let p = de.path();
            if p.is_file()
                && p.extension().and_then(|e| e.to_str()) == Some("json")
                && p.to_string_lossy().ends_with(".ir.json")
            {
                ir_files.push(p);
            } else {
                foreign = true;
                break;
            }
        }
        if foreign {
            continue;
        }
        for p in ir_files {
            let rel = format!(
                "machines/{}/{}",
                entry.file_name().to_string_lossy(),
                p.file_name().unwrap().to_string_lossy()
            );
            if !keep.contains(&rel) {
                let _ = fs::remove_file(&p);
            }
        }
        // Drop the domain dir when it is now empty.
        let _ = fs::remove_dir(&path); // fails harmlessly when non-empty
    }
}

/// Renders the registry module: provenance consts, `include_str!` embeds,
/// and parse-once accessors over a sorted `BTreeMap`. Static data only —
/// no DB handle, no AppState, no async — so it compiles on wasm32 and
/// both topologies reach it through the shared domain-types crate.
fn registry_module(entries: &[MachineEntry]) -> String {
    let mut out = String::new();
    out.push_str(
        "//! Machine kernel (issue #815): the compiled `.ddd` artifact's\n\
         //! machine IRs, embedded as static data with a parse-once registry.\n\
         //!\n\
         //! Generated by codegraph. DO NOT EDIT.\n\
         //!\n\
         //! Each embed is the CANONICAL xstate-ir `Machine` JSON the compiler\n\
         //! produced (`machineIr` — keys sorted recursively), validated at\n\
         //! generation time (`xstate_core::validate_machine_json`) and checked\n\
         //! against its bound codelist enum. Provenance consts are the\n\
         //! drift-detection seam: the authored source path and the content\n\
         //! SHA-256 over the canonical IR, recomputable from the artifact\n\
         //! alone.\n\
         //!\n\
         //! Static data only — no DB handle, no AppState, no async — so this\n\
         //! module compiles on wasm32 and both deployment topologies reach it\n\
         //! through the shared domain-types crate.\n\n\
         use std::collections::BTreeMap;\n\
         use std::sync::LazyLock;\n\n",
    );

    // Provenance consts (both emitted per machine — the accessor arm
    // below references them unconditionally; an absent provenance half
    // rides the empty string).
    out.push_str("// --- PROVENANCE (issue #815) — the drift-detection seam ---\n");
    for entry in entries {
        let name = registry_const_name(&entry.machine_id);
        let path = entry.source_path.as_deref().unwrap_or_default();
        let hash = entry.content_sha256.as_deref().unwrap_or_default();
        if !path.is_empty() {
            out.push_str(&format!(
                "/// Machine `{id}` was authored at `{path}`.\n",
                id = entry.machine_id,
                path = path,
            ));
        }
        out.push_str(&format!(
            "pub const {name}_SOURCE_PATH: &str = {path_literal};\n",
            name = name,
            path_literal = rust_str_literal(path),
        ));
        if !hash.is_empty() {
            out.push_str(&format!(
                "/// Machine `{id}` — SHA-256 over the canonical IR JSON.\n",
                id = entry.machine_id,
            ));
        }
        out.push_str(&format!(
            "pub const {name}_CONTENT_SHA256: &str = {hash_literal};\n",
            name = name,
            hash_literal = rust_str_literal(hash),
        ));
    }

    // Embeds.
    out.push_str("\n// --- EMBEDS: canonical xstate-ir Machine JSON, verbatim ---\n");
    for entry in entries {
        let name = registry_const_name(&entry.machine_id);
        let rel = format!("{}/{}.ir.json", entry.domain, entry.machine_id);
        out.push_str(&format!(
            "const {name}_IR_JSON: &str = include_str!({rel_literal});\n",
            name = name,
            rel_literal = rust_str_literal(&rel),
        ));
    }

    // Registry + accessors.
    out.push_str(
        "\n/// The machine registry: machine id → embedded canonical IR JSON.\n\
         static IR_JSONS: LazyLock<BTreeMap<&'static str, &'static str>> = LazyLock::new(|| {\n\
         \x20   BTreeMap::from([\n",
    );
    for entry in entries {
        let name = registry_const_name(&entry.machine_id);
        out.push_str(&format!(
            "        ({id_literal}, {name}_IR_JSON),\n",
            id_literal = rust_str_literal(&entry.machine_id),
            name = name,
        ));
    }
    out.push_str(
        "    ])\n\
         });\n\n\
         /// Every machine id in the application, sorted.\n\
         pub fn machine_ids() -> Vec<&'static str> {\n\
         \x20   IR_JSONS.keys().copied().collect()\n\
         }\n\n\
         /// The embedded canonical IR JSON for `machine_id`, verbatim from\n\
         /// the compiled artifact.\n\
         pub fn machine_json(machine_id: &str) -> Option<&'static str> {\n\
         \x20   IR_JSONS.get(machine_id).copied()\n\
         }\n\n\
         /// The typed machine: the embedded IR deserialized into the\n\
         /// xstate-ir `Machine`. Generation validated the embed, so `Err`\n\
         /// is a defect seam, never an expected path.\n\
         pub fn machine(\n\
         \x20   machine_id: &str,\n\
         ) -> Option<Result<xstate_core::xstate_ir::Machine, serde_json::Error>> {\n\
         \x20   machine_json(machine_id).map(serde_json::from_str)\n\
         }\n\n\
         /// Provenance for `machine_id`: `(authored source path, content\n\
         /// sha256)` — the Stage 2/3 drift-detection seam.\n\
         pub fn machine_provenance(machine_id: &str) -> Option<(&'static str, &'static str)> {\n\
         \x20   match machine_id {\n",
    );
    for entry in entries {
        let name = registry_const_name(&entry.machine_id);
        out.push_str(&format!(
            "        {id_literal} => Some(({name}_SOURCE_PATH, {name}_CONTENT_SHA256)),\n",
            id_literal = rust_str_literal(&entry.machine_id),
            name = name,
        ));
    }
    out.push_str(
        "        _ => None,\n\
         \x20   }\n\
         }\n\n\
         /// Structural validation mirroring the generation-time gate —\n\
         /// re-runnable at runtime. `None` when the id is unknown; `Err`\n\
         /// carries the fail-closed diagnostic.\n\
         pub fn validate_machine(\n\
         \x20   machine_id: &str,\n\
         ) -> Option<Result<(), xstate_core::ValidateError>> {\n\
         \x20   machine_json(machine_id).map(xstate_core::validate_machine_json)\n\
         }\n",
    );
    out
}

/// A string as a Rust literal (the `ts_string_literal` discipline —
/// escape backslash + quote; the literals here are machine ids, authored
/// paths, and hex hashes, all quote-free in practice).
fn rust_str_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_ir_json_sorts_keys_recursively() {
        // The determinism contract: HashMap-borne structures (`setup`
        // maps) serialize byte-identically regardless of insertion order.
        let a = serde_json::json!({
            "z": 1,
            "a": { "y": { "n": true, "m": 1 }, "b": 3 },
        });
        let b = serde_json::json!({
            "a": { "b": 3, "y": { "m": 1, "n": true } },
            "z": 1,
        });
        assert_eq!(canonical_ir_json(&a), canonical_ir_json(&b));
        assert_eq!(
            canonical_ir_json(&a),
            r#"{"a":{"b":3,"y":{"m":1,"n":true}},"z":1}"#,
        );
    }

    #[test]
    fn registry_const_names_are_valid_identifiers() {
        assert_eq!(registry_const_name("payrun"), "PAYRUN");
        assert_eq!(registry_const_name("pay-run"), "PAY_RUN");
        assert_eq!(registry_const_name("2fa"), "MACHINE_2FA");
    }

    #[test]
    fn machine_ids_must_be_file_safe() {
        assert!(machine_id_is_file_safe("payrun"));
        assert!(machine_id_is_file_safe("pay-run_2"));
        assert!(!machine_id_is_file_safe("../escape"));
        assert!(!machine_id_is_file_safe("a/b"));
        assert!(!machine_id_is_file_safe(""));
        assert!(!machine_id_is_file_safe("a\nb"));
    }
}
