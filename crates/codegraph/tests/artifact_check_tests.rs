//! `pherolang artifact check` as a PR-CI gate over every committed artifact
//! fixture (issue #674).
//!
//! Upstream ships `pherolang artifact check`: it validates one serialized
//! artifact *from the artifact alone* — its root shape, its `formatVersion`,
//! and the DDD artifact-only contract — and names a 0.2-era document with a
//! migration message instead of diffing it. We commit golden artifacts
//! (`ddd/library.ddd.json`, `evt/orders.evt.json`, `graph_analytics/sod.json`),
//! so a fixture that drifts into an unreadable generation must fail CI here,
//! before a consumer hits it.
//!
//! **Why this is a reimplementation, not a call.** `phero-cli` is a
//! **binary-only** crate (`[[bin]] name = "pherolang"`, no `[lib]` target), so
//! `pherolang artifact check` cannot be invoked as a library and would turn a
//! node-free/DB-free PR-CI gate into a `cargo install` + subprocess step. The
//! check below is lifted onto the crates codegraph already depends on:
//!
//! 1. [`LEGACY_MESSAGE`] — upstream's rule 0, verbatim. It runs *first*, so a
//!    0.2-era document is named as such rather than reported as a generic
//!    `formatVersion` mismatch or an `unknown root $type`.
//! 2. Shape classification in upstream's documented precedence (see
//!    [`classify`]), so every artifact kind is recognized the same way.
//! 3. The version gate — the root's `formatVersion` is compared against
//!    `phero-ir`'s own marker const for the classified kind, and the document
//!    is then deserialized through that kind's versioned `from_json` (the
//!    reader's gate, so this check and the ingest path cannot disagree).
//! 4. The provenance check: a contract-complete DDD design carries a
//!    non-empty `pheroVersion` (`formatVersion` remains the only gate; the
//!    other artifact roots do not carry the key at all).
//!
//! The deep DDD artifact-only contract (`phero_backend_ddd::check_contract` —
//! resolved companions, aligned parallel lists, value semantics, DTO
//! boundaries) is deliberately **not** reimplemented here: it lives in a crate
//! codegraph does not depend on, and the ingest path already refuses a broken
//! design with `Error::DddModel` (pinned by `ddd_golden_tests`). This gate
//! owns what a *committed file on disk* can prove on its own.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// The one message a 0.2-era artifact gets — upstream's `LEGACY_MESSAGE`,
/// spelled verbatim (`phero-cli`'s `artifact_check::legacy`). It is a
/// migration instruction, never a diff: the document is not readable at all,
/// so showing a structural diff would be noise.
///
/// Acceptance for issue #674: a stale committed artifact must fail with THIS
/// string. [`a_0_2_era_artifact_is_named_not_diffed`] pins it over the real
/// fixture bytes.
const LEGACY_MESSAGE: &str = "0.2-era rexlang artifact — regenerate with pherolang 0.3";

/// The provenance key 0.2-era artifacts carried (renamed to `pheroVersion`).
const LEGACY_PROVENANCE_KEY: &str = "rexVersion";

/// The canonical-instance discriminator 0.2-era artifacts carried (renamed to
/// `phero.instance`).
const LEGACY_INSTANCE_TYPE: &str = "rex.instance";

/// An artifact kind upstream's `artifact check` classifies, in its documented
/// precedence order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ArtifactKind {
    /// A domain IR artifact (`phero_ir::Model`): root carries `packages`.
    DomainModel,
    /// A DDD design artifact (`phero_ir::ddd::DddModel`).
    DddModel,
    /// An event-contract artifact (`phero_ir::events::EventModel`).
    EventModel,
    /// A standalone actor-policy artifact (`phero_ir::actors::ActorModel`).
    ActorModel,
    /// A deployment artifact (`phero_ir::deploy::DeployModel`).
    DeployModel,
    /// A use-case artifact (`phero_ir::usecases::UseCaseModel`).
    UseCaseModel,
    /// A decision-model artifact (`phero_ir::dmn::DmnModel`).
    DmnModel,
    /// A case-management artifact (`phero_ir::cmmn::CmmnModel`).
    CmmnModel,
    /// A semantic graph artifact (`phero_ir::graph::GraphModel`).
    GraphModel,
    /// An interaction-flow artifact (`phero_ir::ifml::IfmlModel`).
    IfmlModel,
    /// A canonical instance document (`$type: "phero.instance"`).
    Instance,
}

impl ArtifactKind {
    /// The `formatVersion` this kind's reader accepts — the same const the
    /// deserializer gates on, so the two cannot drift.
    fn format_version(self) -> Option<u32> {
        let version = match self {
            Self::DomainModel => phero_ir::FORMAT_VERSION,
            Self::DddModel => phero_ir::ddd::DDD_MODEL_FORMAT_VERSION,
            Self::EventModel => phero_ir::events::EVENT_MODEL_FORMAT_VERSION,
            Self::ActorModel => phero_ir::ACTOR_MODEL_FORMAT_VERSION,
            Self::DeployModel => phero_ir::deploy::DEPLOY_MODEL_FORMAT_VERSION,
            Self::UseCaseModel => phero_ir::usecases::USE_CASE_MODEL_FORMAT_VERSION,
            Self::DmnModel => phero_ir::dmn::DMN_MODEL_FORMAT_VERSION,
            Self::CmmnModel => phero_ir::cmmn::CMMN_MODEL_FORMAT_VERSION,
            Self::GraphModel => phero_ir::graph::GRAPH_MODEL_FORMAT_VERSION,
            Self::IfmlModel => phero_ir::ifml::IFML_MODEL_FORMAT_VERSION,
            // The instance marker lives in `phero-runtime` (not a codegraph
            // dependency); upstream's `artifact check` is the only reader.
            // Fail closed: a committed instance would be a named gap, never a
            // silent pass.
            Self::Instance => return None,
        };
        Some(version)
    }

    /// The kind's name as the diagnostics spell it.
    fn label(self) -> &'static str {
        match self {
            Self::DomainModel => "domain IR artifact",
            Self::DddModel => "DDD design artifact",
            Self::EventModel => "event-contract artifact",
            Self::ActorModel => "actor-policy artifact",
            Self::DeployModel => "deployment artifact",
            Self::UseCaseModel => "use-case artifact",
            Self::DmnModel => "decision-model artifact",
            Self::CmmnModel => "case-management artifact",
            Self::GraphModel => "semantic graph artifact",
            Self::IfmlModel => "interaction-flow artifact",
            Self::Instance => "canonical instance",
        }
    }

    /// Round-trips the document through the kind's versioned deserializer —
    /// the reader's own gate, so a shape the ingest path would refuse cannot
    /// pass here. The parsed value is discarded: this gate proves
    /// *readability*, and `ddd_golden_tests`/`evt_golden_tests` own the
    /// byte-identity and round-trip contracts.
    fn deserialize(self, text: &str) -> Result<(), String> {
        let result = match self {
            Self::DomainModel => phero_ir::Model::from_json(text).map(|_| ()),
            Self::DddModel => phero_ir::ddd::DddModel::from_json(text).map(|_| ()),
            Self::EventModel => phero_ir::events::EventModel::from_json(text).map(|_| ()),
            Self::ActorModel => phero_ir::ActorModel::from_json(text).map(|_| ()),
            Self::DeployModel => phero_ir::deploy::DeployModel::from_json(text).map(|_| ()),
            Self::UseCaseModel => phero_ir::usecases::UseCaseModel::from_json(text).map(|_| ()),
            Self::DmnModel => phero_ir::dmn::DmnModel::from_json(text).map(|_| ()),
            Self::CmmnModel => phero_ir::cmmn::CmmnModel::from_json(text).map(|_| ()),
            Self::GraphModel => phero_ir::graph::GraphModel::from_json(text).map(|_| ()),
            Self::IfmlModel => phero_ir::ifml::IfmlModel::from_json(text).map(|_| ()),
            Self::Instance => unreachable!("the instance kind fails before deserialization"),
        };
        result.map_err(|error| error.to_string())
    }
}

/// Every committed artifact fixture, relative to `tests/fixtures/`, with the
/// kind `artifact check` must classify it as.
///
/// Enumerated from the tree, not guessed: [`committed_artifact_set_is_complete`]
/// walks `tests/fixtures/` for JSON documents carrying a root-level
/// `formatVersion` and fails when one is missing from this list, so a new
/// fixture cannot land uncovered. JSON Schemas (`schemas/`, `ifml_gate/`) and
/// the OpenAPI document (`openapi/customers.json`) are model *inputs*, not
/// artifacts, and carry no `formatVersion`.
const COMMITTED_ARTIFACTS: &[(&str, ArtifactKind)] = &[
    ("ddd/library.ddd.json", ArtifactKind::DddModel),
    ("evt/orders.evt.json", ArtifactKind::EventModel),
    ("graph_analytics/sod.json", ArtifactKind::ActorModel),
];

/// The fixture root. `CODEGRAPH_ARTIFACT_FIXTURES` points the gate at a copy —
/// the teeth mechanism (`#550` discipline: break the input, watch the gate
/// fail, revert) without ever editing a committed golden.
fn fixture_root() -> PathBuf {
    match std::env::var("CODEGRAPH_ARTIFACT_FIXTURES") {
        Ok(root) => PathBuf::from(root),
        Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
    }
}

/// The root-level `formatVersion` a document claims, if it is an unsigned
/// integer.
fn claimed_format_version(root: &Map<String, Value>) -> Option<u32> {
    root.get("formatVersion")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
}

/// Reports the 0.2-era marker a document root carries: the pre-rename
/// provenance key, or the pre-rename instance discriminator. Either one is
/// decisive — up to and including a document that claims a current
/// `formatVersion` but still carries the old key.
fn legacy_marker(root: &Map<String, Value>) -> Option<&'static str> {
    let legacy_provenance = root.contains_key(LEGACY_PROVENANCE_KEY);
    let legacy_instance = root.get("$type").and_then(Value::as_str) == Some(LEGACY_INSTANCE_TYPE);
    (legacy_provenance || legacy_instance).then_some(LEGACY_MESSAGE)
}

/// Classifies an artifact by root shape, in upstream's documented precedence.
///
/// The order is load-bearing in three places, exactly as upstream records it:
/// `decisions` before `views` (a decision model also carries decision views),
/// `views` before `application`/`modules` (an IFML model may carry `modules`),
/// and `nodes` before `applications` (a graph artifact also carries
/// `applications`). The module-only IFML model is shape-ambiguous with a DDD
/// design and classifies as one — upstream's documented collision, inherited
/// here on purpose.
fn classify(root: &Map<String, Value>) -> Option<ArtifactKind> {
    let has = |key: &str| root.contains_key(key);
    let versioned = claimed_format_version(root).is_some();

    if has("packages") {
        return Some(ArtifactKind::DomainModel);
    }
    if versioned && has("decisions") {
        return Some(ArtifactKind::DmnModel);
    }
    if versioned
        && ["views", "domains", "actions", "commands", "imports"]
            .iter()
            .any(|key| has(key))
    {
        return Some(ArtifactKind::IfmlModel);
    }
    if versioned && (has("application") || has("modules")) {
        return Some(ArtifactKind::DddModel);
    }
    if versioned
        && ["events", "channels", "subscriptions"]
            .iter()
            .any(|key| has(key))
    {
        return Some(ArtifactKind::EventModel);
    }
    if versioned && has("nodes") {
        return Some(ArtifactKind::GraphModel);
    }
    if versioned
        && ["applications", "profiles", "deployments"]
            .iter()
            .any(|key| has(key))
    {
        return Some(ArtifactKind::DeployModel);
    }
    if versioned && has("useCases") {
        return Some(ArtifactKind::UseCaseModel);
    }
    if versioned && has("cases") {
        return Some(ArtifactKind::CmmnModel);
    }
    if has("blocks") || versioned {
        // The minimal-empty artifact is exactly `{"formatVersion": N}` for
        // several kinds; shape-sniffing cannot tell them apart, so the marker
        // alone falls through to the actor kind (the canonical block-less
        // ActorModel), upstream's rule 11.
        return Some(ArtifactKind::ActorModel);
    }
    None
}

/// Validates one artifact document, returning its kind or every violation.
///
/// Mirrors `phero_cli::artifact_check::check`'s contract for the kinds
/// codegraph commits: rule 0 (legacy), the root `$type` branch, shape
/// classification, the version gate, and — for a design — the provenance
/// check.
fn check_artifact(text: &str) -> Result<ArtifactKind, Vec<String>> {
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => return Err(vec![format!("not valid JSON: {error}")]),
    };
    let Some(root) = value.as_object() else {
        return Err(vec!["artifact root must be a JSON object".to_string()]);
    };
    // Rule 0 — before `$type` and before shape-sniffing, so a legacy document
    // is named as such rather than misclassified.
    if let Some(message) = legacy_marker(root) {
        return Err(vec![message.to_string()]);
    }
    if let Some(tag) = root.get("$type") {
        let Some(tag) = tag.as_str() else {
            return Err(vec![format!("root $type must be a string, found {tag}")]);
        };
        if tag != "phero.instance" {
            return Err(vec![format!(
                "unknown root $type {tag:?}: instance documents declare $type \
                 \"phero.instance\"; IR and actor artifacts carry no root $type"
            )]);
        }
        return check_known_kind(ArtifactKind::Instance, root, text);
    }
    let Some(kind) = classify(root) else {
        return Err(vec!["unrecognized artifact".to_string()]);
    };
    check_known_kind(kind, root, text)
}

/// The version gate, the provenance check, and the deserializer round-trip for
/// a classified artifact.
fn check_known_kind(
    kind: ArtifactKind,
    root: &Map<String, Value>,
    text: &str,
) -> Result<ArtifactKind, Vec<String>> {
    let Some(expected) = kind.format_version() else {
        return Err(vec![format!(
            "{} documents are not covered by codegraph's in-process artifact \
             check (no dependency exposes the instance marker); a committed \
             instance fixture needs a checker before it can land",
            kind.label()
        )]);
    };
    let Some(found) = claimed_format_version(root) else {
        return Err(vec![format!(
            "{}: formatVersion must be an unsigned integer",
            kind.label()
        )]);
    };
    if found != expected {
        // The reader's own wording (`IrError::UnsupportedFormatVersion`), so a
        // stale fixture reads the same here as it would at ingest.
        return Err(vec![format!(
            "unsupported artifact formatVersion: found {found}, expected {expected}"
        )]);
    }
    // Provenance: a contract-complete design carries a non-empty
    // `pheroVersion`. `formatVersion` is the only *gate*; this is the honesty
    // check that the artifact was emitted by a real compiler run.
    if kind == ArtifactKind::DddModel && root.contains_key("application") {
        let provenance = root
            .get("pheroVersion")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if provenance.is_empty() {
            return Err(vec![
                "contract-complete design artifact carries no pheroVersion provenance".to_string(),
            ]);
        }
    }
    kind.deserialize(text).map_err(|error| vec![error])?;
    Ok(kind)
}

/// Reads and checks one fixture, prefixing every violation with its path —
/// upstream's `check_str` shape.
fn check_file(path: &Path) -> Result<ArtifactKind, Vec<String>> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| vec![format!("{}: {error}", path.display())])?;
    check_artifact(&text).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| format!("{}: {error}", path.display()))
            .collect()
    })
}

/// The issue's headline acceptance criterion: every committed artifact passes
/// the artifact-alone check, and classifies as the kind it was committed as.
#[test]
fn every_committed_artifact_passes_the_artifact_alone_check() {
    let root = fixture_root();
    for (relative, expected) in COMMITTED_ARTIFACTS {
        let path = root.join(relative);
        match check_file(&path) {
            Ok(kind) => assert_eq!(
                kind,
                *expected,
                "{relative} must classify as {}",
                expected.label()
            ),
            Err(errors) => panic!(
                "{relative} fails `artifact check`:\n  {}",
                errors.join("\n  ")
            ),
        }
    }
}

/// Coverage invariant: the list above is the WHOLE set of committed artifacts.
/// A new JSON fixture carrying a root-level `formatVersion` that is not in
/// [`COMMITTED_ARTIFACTS`] fails here by name, so the gate cannot be quietly
/// outgrown (the `entity_specs_run_green` coverage discipline, applied to
/// artifacts).
#[test]
fn committed_artifact_set_is_complete() {
    let root = fixture_root();
    let mut discovered = BTreeSet::new();
    collect_artifacts(&root, &root, &mut discovered);
    let covered: BTreeSet<String> = COMMITTED_ARTIFACTS
        .iter()
        .map(|(relative, _)| (*relative).to_string())
        .collect();

    let missing: Vec<&String> = discovered.difference(&covered).collect();
    assert!(
        missing.is_empty(),
        "committed artifact fixture(s) missing from COMMITTED_ARTIFACTS: {missing:?}"
    );
    let stale: Vec<&String> = covered.difference(&discovered).collect();
    assert!(
        stale.is_empty(),
        "COMMITTED_ARTIFACTS names fixture(s) that are not committed artifacts: {stale:?}"
    );
}

/// Walks `dir` for `.json` files whose root object carries a `formatVersion`
/// (the artifact marker). Schema inputs and OpenAPI documents cannot match.
fn collect_artifacts(root: &Path, dir: &Path, found: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|e| e.path())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_artifacts(root, &path, found);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(Value::Object(root_object)) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if !root_object.contains_key("formatVersion") {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(root) {
            found.insert(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Teeth: the real fixture bytes, rewritten into a 0.2-era document in a temp
/// dir, fail with upstream's exact migration message — not a diff wall. The
/// committed file is never touched (`CODEGRAPH_ARTIFACT_FIXTURES` is the
/// copy-the-tree hook the shell-level teeth run uses).
#[test]
fn a_0_2_era_artifact_is_named_not_diffed() {
    let artifact = COMMITTED_ARTIFACTS[0].0;
    let text = std::fs::read_to_string(fixture_root().join(artifact)).expect("fixture readable");
    let mut legacy: Value = serde_json::from_str(&text).expect("fixture is JSON");
    let root = legacy.as_object_mut().expect("artifact root is an object");
    root.insert(
        LEGACY_PROVENANCE_KEY.to_string(),
        Value::String("0.2.0".to_string()),
    );
    root.insert("formatVersion".to_string(), Value::from(1));

    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = dir.path().join("legacy.ddd.json");
    std::fs::write(&path, serde_json::to_string_pretty(&legacy).unwrap()).expect("write copy");

    let errors = check_file(&path).expect_err("a 0.2-era artifact must fail");
    assert_eq!(
        errors.len(),
        1,
        "a legacy artifact gets ONE message, never a violation list: {errors:?}"
    );
    assert!(
        errors[0].ends_with(LEGACY_MESSAGE),
        "the message is upstream's, verbatim: {errors:?}"
    );
    assert!(
        errors[0].starts_with(&path.display().to_string()),
        "the violation names the file: {errors:?}"
    );
}

/// The other legacy marker: the pre-rename instance discriminator is decisive
/// on its own, even with no `rexVersion` key.
#[test]
fn a_0_2_era_instance_discriminator_is_named() {
    let text =
        format!(r#"{{"$type": "{LEGACY_INSTANCE_TYPE}", "formatVersion": 1, "objects": []}}"#);
    let errors = check_artifact(&text).expect_err("a legacy instance must fail");
    assert_eq!(errors, vec![LEGACY_MESSAGE.to_string()]);
}

/// Teeth: a fixture whose marker moves ahead of the pinned `phero-ir` is named
/// with the found/expected versions (the reader's wording), again over a temp
/// copy of the real bytes.
#[test]
fn a_newer_format_version_names_the_upgrade() {
    let artifact = COMMITTED_ARTIFACTS[0].0;
    let text = std::fs::read_to_string(fixture_root().join(artifact)).expect("fixture readable");
    let mut bumped: Value = serde_json::from_str(&text).expect("fixture is JSON");
    bumped
        .as_object_mut()
        .expect("artifact root is an object")
        .insert("formatVersion".to_string(), Value::from(99));

    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = dir.path().join("bumped.ddd.json");
    std::fs::write(&path, serde_json::to_string_pretty(&bumped).unwrap()).expect("write copy");

    let errors = check_file(&path).expect_err("a newer artifact must fail");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("unsupported artifact formatVersion: found 99, expected 2"),
        "the version gate names both versions: {errors:?}"
    );
}

/// A design stripped of its provenance is a contract failure, not a pass:
/// `pheroVersion` is how a consumer tells a compiler-emitted design from a
/// hand-edited document.
#[test]
fn a_design_without_provenance_is_rejected() {
    let artifact = COMMITTED_ARTIFACTS[0].0;
    let text = std::fs::read_to_string(fixture_root().join(artifact)).expect("fixture readable");
    let mut stripped: Value = serde_json::from_str(&text).expect("fixture is JSON");
    let root = stripped
        .as_object_mut()
        .expect("artifact root is an object");
    assert!(
        root.remove("pheroVersion").is_some(),
        "the committed design carries provenance to strip"
    );

    let errors = check_artifact(&serde_json::to_string(&stripped).unwrap())
        .expect_err("a provenance-less design must fail");
    assert!(
        errors.iter().any(|e| e.contains("pheroVersion")),
        "the violation names the missing provenance: {errors:?}"
    );
}

/// Anything that is not a known artifact fails closed rather than passing as
/// "unrecognized".
#[test]
fn an_unrecognized_document_fails_closed() {
    let errors = check_artifact(r#"{"hello": "world"}"#).expect_err("not an artifact");
    assert_eq!(errors, vec!["unrecognized artifact".to_string()]);

    let errors = check_artifact("not json at all").expect_err("not JSON");
    assert!(errors[0].starts_with("not valid JSON:"), "{errors:?}");
}
