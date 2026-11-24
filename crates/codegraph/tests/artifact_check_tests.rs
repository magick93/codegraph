//! `pherolang artifact check` as a PR-CI gate over every committed artifact
//! fixture under `crates/codegraph/tests/fixtures/` (issue #674).
//!
//! Upstream ships `pherolang artifact check`: it validates one serialized
//! artifact *from the artifact alone* — its root shape, its `formatVersion`,
//! its named collections, its structural-key spellings, and the DDD
//! artifact-only contract — and names a 0.2-era document with a migration
//! message instead of diffing it. We commit golden artifacts
//! (`ddd/library.ddd.json`, `evt/orders.evt.json`, `graph_analytics/sod.json`),
//! so a fixture that drifts into an unreadable generation must fail CI here,
//! before a consumer hits it.
//!
//! **Why this is a reimplementation, not a call.** `phero-cli` is a
//! **binary-only** crate (`[[bin]] name = "pherolang"`, no `[lib]` target), so
//! `pherolang artifact check` cannot be invoked as a library and would turn a
//! node-free/DB-free PR-CI gate into a `cargo install` + subprocess step. The
//! check below is lifted onto `phero-ir` (already a dependency) plus ONE lean
//! dev-dependency, `phero-backend-common` (closure: `anyhow` + `serde_json`),
//! for the structural-key corpus the snake_case scan binary-searches:
//!
//! 1. [`LEGACY_MESSAGE`] — upstream's rule 0, verbatim. It runs *first*, so a
//!    0.2-era document is named as such rather than reported as a generic
//!    `formatVersion` mismatch or an `unknown root $type`.
//! 2. Shape classification in upstream's documented precedence and with
//!    upstream's exact predicates (see [`classify`]) — including the strict
//!    minimal-empty rule `{"formatVersion": N}`, so a document with an
//!    unknown root key is `unrecognized`, never an actor artifact.
//! 3. The version gate, with upstream's artifact-check wording
//!    ([`format_version_error`]), which names the fix.
//! 4. The named-collection shape check ([`check_named_array`]), for the key
//!    lists the classifier carries per kind.
//! 5. The snake_case structural-key scan ([`scan_snake_case_keys`]) over
//!    `phero_backend_common::STRUCTURAL_KEYS` — the ONE corpus, binary-searched
//!    exactly as upstream does, so a `never_both` spelling that the readers
//!    silently drop is named instead of ignored.
//! 6. The provenance check: every DDD design artifact carries a non-empty
//!    `pheroVersion`. `DddModel::new()` always stamps it, so an emitted design
//!    always has one. Exactly two artifact roots carry the key at all — the DDD
//!    design (`ddd::DddModel`) and the domain `Model` (`.mox` IR); the actor,
//!    event, deployment, use-case, decision, case, graph and IFML roots do not.
//!    `formatVersion` remains the only *gate*; `pheroVersion` is provenance.
//! 7. The kind's versioned `from_json` (when the version gate passed) — the
//!    reader's own gate, so this check and the ingest path cannot disagree.
//!
//! **Named limitations** (upstream does more; this gate does not silently
//! pretend otherwise):
//!
//! - **Canonical instance documents fail closed.** `phero-runtime` (which
//!   owns the instance marker/`$id`/`$ref` rules) is not a codegraph
//!   dependency, so an instance artifact is classified and then *refused* as
//!   not covered. No instance fixture is committed; one would need a checker
//!   before it could land.
//! - **The deep DDD artifact-only contract is not mirrored.**
//!   `phero_backend_ddd::check_contract` (resolved companions, aligned parallel
//!   lists, value semantics, DTO boundaries) and upstream's
//!   `application.name` check live in crates that are not codegraph
//!   dependencies. The *emission* side is pinned instead: `ddd_golden_tests`
//!   byte-compares the compiler's output against the committed artifact, so
//!   the bytes this gate reads are exactly the bytes the compiler produced.
//! - **Per-kind deep shape checks are not mirrored** beyond the version gate,
//!   the named collections and the snake_case scan (upstream's graph
//!   identity-field / `attributes` checks, the IR `collection` marker check,
//!   and the per-kind extras). The kind's `from_json` still has to accept the
//!   document, which is the failure class a committed fixture actually hits.
//! - **Scope is `crates/codegraph/tests/fixtures/`**, the tree that holds
//!   codegraph's committed artifacts. A new artifact elsewhere is out of the
//!   walk and would not be gated; [`committed_artifact_set_is_complete`] covers
//!   the fixtures tree only.
//! - **The discovery walk fails on ANY unparseable `*.json` under the fixtures
//!   tree** (that is what stops a broken artifact from hiding from
//!   classification). A deliberately-malformed JSON fixture — a negative-case
//!   input for some other test — therefore cannot simply be dropped in: it
//!   must be exempted by a documented rule (an explicit allow-list entry in
//!   [`collect_artifacts`], justified in the fixture's own test), or this gate
//!   will fail on it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use phero_backend_common::{STRUCTURAL_KEYS, camel_case};
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

/// The current canonical-instance discriminator. Spelling it here (rather than
/// depending on `phero-runtime`) is what makes the instance branch fail closed:
/// [`ArtifactKind::Instance`] has no marker in this dependency set.
const INSTANCE_TYPE: &str = "phero.instance";

/// The DDD design fixture — named so the teeth tests below do not depend on
/// list order.
const LIBRARY_DDD: &str = "ddd/library.ddd.json";

/// The event-contract fixture.
const ORDERS_EVT: &str = "evt/orders.evt.json";

/// The actor-policy fixture (the third committed artifact — found by the
/// coverage walk, not guessed from the issue text).
const SOD_ACTOR: &str = "graph_analytics/sod.json";

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
    /// The kind's name in a diagnostic — upstream's `what` argument, so a
    /// version message reads exactly as `pherolang artifact check` writes it.
    fn what(self) -> &'static str {
        match self {
            Self::DomainModel => "IR artifact",
            Self::DddModel => "DDD design artifact",
            Self::EventModel => "event artifact",
            Self::ActorModel => "actor artifact",
            Self::DeployModel => "deployment artifact",
            Self::UseCaseModel => "use-case artifact",
            Self::DmnModel => "decision artifact",
            Self::CmmnModel => "case artifact",
            Self::GraphModel => "semantic graph artifact",
            Self::IfmlModel => "interaction-flow artifact",
            Self::Instance => "canonical instance",
        }
    }

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
            Self::Instance => return None,
        };
        Some(version)
    }

    /// The root collections whose entries must be named objects — upstream's
    /// per-kind `keys` argument. Deliberately empty for two kinds:
    /// `GraphModel`, whose collections carry `id`/`kind`/`from`/`to` and a
    /// different required-field rule (upstream's `check_graph_objects`, not
    /// mirrored — see the module's named limitations), and `Instance`, whose
    /// objects are model-defined.
    fn named_keys(self) -> &'static [&'static str] {
        match self {
            Self::DomainModel => &["packages"],
            Self::DddModel => &["modules"],
            Self::EventModel => &["events", "channels", "subscriptions"],
            Self::ActorModel => &["blocks"],
            Self::DeployModel => &["applications", "profiles", "deployments"],
            Self::UseCaseModel => &["useCases"],
            Self::DmnModel => &["decisions"],
            Self::CmmnModel => &["cases"],
            Self::GraphModel => &[],
            Self::IfmlModel => &[
                "views", "domains", "actions", "modules", "actors", "commands",
            ],
            Self::Instance => &[],
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

/// Every committed artifact fixture under `tests/fixtures/`, relative to that
/// directory, with the kind `artifact check` must classify it as.
///
/// Enumerated from the tree, not guessed: [`committed_artifact_set_is_complete`]
/// walks `tests/fixtures/` for JSON documents carrying a root-level
/// `formatVersion` and fails when one is missing from this list, so a new
/// fixture cannot land uncovered. JSON Schemas (`schemas/`, `ifml_gate/`) and
/// the OpenAPI document (`openapi/customers.json`) are model *inputs*, not
/// artifacts, and carry no `formatVersion`.
const COMMITTED_ARTIFACTS: &[(&str, ArtifactKind)] = &[
    (LIBRARY_DDD, ArtifactKind::DddModel),
    (ORDERS_EVT, ArtifactKind::EventModel),
    (SOD_ACTOR, ArtifactKind::ActorModel),
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

/// Reads a committed fixture by its `tests/fixtures/`-relative name.
fn fixture_text(relative: &str) -> String {
    let path = fixture_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
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

/// Classifies an artifact by root shape, in upstream's documented precedence
/// **and with upstream's exact predicates** (`phero-cli`'s
/// `artifact_check::check`).
///
/// Every kinded rule is gated on `formatVersion` being an unsigned integer
/// *and* on the discriminating key having the right JSON type, and the DDD
/// arm additionally requires the absence of `blocks`. The ordering is
/// load-bearing in three places, exactly as upstream records it: `decisions`
/// before `views` (a decision model also carries decision views), `views`
/// before `application`/`modules` (an IFML model may carry `modules`), and
/// `nodes` before `applications` (a graph artifact also carries
/// `applications`). The module-only IFML model is shape-ambiguous with a DDD
/// design and classifies as one — upstream's documented collision, inherited
/// here on purpose.
///
/// The final rule is upstream's **strict** minimal-empty rule: `blocks` or
/// exactly `{"formatVersion": N}`. A `formatVersion`-bearing root with any
/// other unknown key is `None` (unrecognized), never an actor artifact —
/// accepting it would validate a document whose payload the readers silently
/// drop.
fn classify(root: &Map<String, Value>) -> Option<ArtifactKind> {
    let versioned = root.get("formatVersion").is_some_and(Value::is_u64);
    let is_array = |key: &str| root.get(key).is_some_and(Value::is_array);
    let is_object = |key: &str| root.get(key).is_some_and(Value::is_object);
    let any_array = |keys: &[&str]| keys.iter().any(|key| is_array(key));

    if root.contains_key("packages") {
        return Some(ArtifactKind::DomainModel);
    }
    if versioned && is_array("decisions") {
        return Some(ArtifactKind::DmnModel);
    }
    if versioned && any_array(&["views", "domains", "actions", "commands", "imports"]) {
        return Some(ArtifactKind::IfmlModel);
    }
    if versioned
        && !root.contains_key("blocks")
        && (is_object("application") || is_array("modules"))
    {
        return Some(ArtifactKind::DddModel);
    }
    if versioned && any_array(&["events", "channels", "subscriptions"]) {
        return Some(ArtifactKind::EventModel);
    }
    if versioned && is_array("nodes") {
        return Some(ArtifactKind::GraphModel);
    }
    if versioned && any_array(&["applications", "profiles", "deployments"]) {
        return Some(ArtifactKind::DeployModel);
    }
    if versioned && is_array("useCases") {
        return Some(ArtifactKind::UseCaseModel);
    }
    if versioned && is_array("cases") {
        return Some(ArtifactKind::CmmnModel);
    }
    // The canonical block-less ActorModel is exactly `{"formatVersion": N}`:
    // a Model always emits `packages`, a DddModel always carries the design
    // keys, an EventModel/DeployModel/UseCaseModel/DmnModel/CmmnModel always
    // carries one of its arrays, an IfmlModel one of the IFML keys. Anything
    // else has no known root shape.
    if root.contains_key("blocks") || (root.len() == 1 && root.contains_key("formatVersion")) {
        return Some(ArtifactKind::ActorModel);
    }
    None
}

/// Builds the `formatVersion` diagnostic — upstream's `format_version_error`
/// wording, verbatim, so a stale fixture reads the same here as it does under
/// `pherolang artifact check` (including the regenerate instruction, which is
/// the fix).
fn format_version_error(root: &Map<String, Value>, supported: u32, what: &str) -> Option<String> {
    match root.get("formatVersion") {
        None => Some(format!(
            "missing {what} formatVersion: this tool reads formatVersion {supported}"
        )),
        Some(found) => match found.as_u64() {
            Some(found) if found == u64::from(supported) => None,
            Some(found) => Some(format!(
                "unsupported {what} formatVersion {found}: this tool reads formatVersion \
                 {supported} — regenerate the artifact with a matching pherolang version"
            )),
            None => Some(format!(
                "{what} formatVersion must be an unsigned integer, found {found}"
            )),
        },
    }
}

/// `packages`/`blocks`/`events`/… sanity: an array of objects each carrying a
/// non-empty string `name` — upstream's `check_named_array`, verbatim.
fn check_named_array(value: &Value, key: &str, errors: &mut Vec<String>) {
    let Some(items) = value.as_array() else {
        errors.push(format!("{key:?} must be an array, found {value}"));
        return;
    };
    for (index, item) in items.iter().enumerate() {
        match item.as_object() {
            None => errors.push(format!("{key}[{index}] must be an object, found {item}")),
            Some(object) => match object.get("name") {
                None => errors.push(format!("{key}[{index}] is missing \"name\"")),
                Some(name) if name.as_str().is_none_or(str::is_empty) => errors.push(format!(
                    "{key}[{index}] \"name\" must be a non-empty string, found {name}"
                )),
                _ => {}
            },
        }
    }
}

/// Reports every key whose spelling is snake_case but whose camelCase form is
/// a known structural key — the readers ignore unknown fields (forward
/// compatibility), so such an artifact loads but silently drops the value.
///
/// Upstream's `scan_snake_case_keys`, over the same corpus
/// (`phero_backend_common::STRUCTURAL_KEYS`, binary-searched). Runs on every
/// classified artifact kind: all of their keys are structural (the exempt
/// case is a canonical instance, which this gate refuses outright).
fn scan_snake_case_keys(value: &Value, errors: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key.contains('_') {
                    let camel = camel_case(key);
                    if STRUCTURAL_KEYS.binary_search(&camel.as_str()).is_ok() {
                        errors.push(format!(
                            "key {key:?} is not wire format: the camelCase key is {camel:?}"
                        ));
                    }
                }
                scan_snake_case_keys(child, errors);
            }
        }
        Value::Array(items) => {
            for item in items {
                scan_snake_case_keys(item, errors);
            }
        }
        _ => {}
    }
}

/// Validates one artifact document, returning its kind or every violation.
///
/// Mirrors `phero_cli::artifact_check::check`'s contract for the kinds
/// codegraph commits: rule 0 (legacy), the root `$type` branch, shape
/// classification, the version gate, the named collections, the snake_case
/// scan, and — for a design — the provenance check.
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
        if tag != INSTANCE_TYPE {
            return Err(vec![format!(
                "unknown root $type {tag:?}: instance documents declare $type \
                 {INSTANCE_TYPE:?}; IR and actor artifacts carry no root $type"
            )]);
        }
        return check_known_kind(ArtifactKind::Instance, root, text);
    }
    let Some(kind) = classify(root) else {
        return Err(vec![
            "unrecognized artifact: no known root shape (IR artifacts carry \"packages\", \
             actor artifacts \"blocks\", interaction-flow artifacts \
             \"views\"/\"domains\"/\"actions\"/\"commands\", DDD design artifacts \
             \"application\"/\"modules\", event artifacts \
             \"events\"/\"channels\"/\"subscriptions\", deployment artifacts \
             \"applications\"/\"profiles\"/\"deployments\", semantic graph artifacts \"nodes\", \
             use-case artifacts \"useCases\", decision artifacts \"decisions\", \
             instances declare $type \"phero.instance\")"
                .to_string(),
        ]);
    };
    check_known_kind(kind, root, text)
}

/// The version gate, the named collections, the snake_case scan, the provenance
/// check, and the deserializer round-trip for a classified artifact.
fn check_known_kind(
    kind: ArtifactKind,
    root: &Map<String, Value>,
    text: &str,
) -> Result<ArtifactKind, Vec<String>> {
    let Some(expected) = kind.format_version() else {
        // Fail closed: a committed instance would be a named gap, never a
        // silent pass (no codegraph dependency exposes the instance reader).
        return Err(vec![format!(
            "{} documents are not covered by codegraph's in-process artifact \
             check (no dependency exposes the instance marker); a committed \
             instance fixture needs a checker before it can land",
            kind.what()
        )]);
    };

    let mut errors = Vec::new();
    errors.extend(format_version_error(root, expected, kind.what()));
    for key in kind.named_keys() {
        if let Some(array) = root.get(*key) {
            check_named_array(array, key, &mut errors);
        }
    }
    scan_snake_case_keys(&Value::Object(root.clone()), &mut errors);
    // Provenance: EVERY DDD design artifact carries a non-empty `pheroVersion`
    // (`DddModel::new` always stamps it, so a compiler-emitted design — with or
    // without an `application` — has one). `formatVersion` remains the only
    // *gate*; this is the honesty check that the bytes came from a real
    // compiler run rather than a hand edit.
    if kind == ArtifactKind::DddModel {
        let provenance = root
            .get("pheroVersion")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if provenance.is_empty() {
            errors.push(
                "DDD design artifact carries no pheroVersion provenance: a compiler-emitted \
                 design always stamps it"
                    .to_string(),
            );
        }
    }

    if !errors.is_empty() {
        // A version-gate failure explains itself; deserializing on top of it
        // only adds a second, redundant message.
        return Err(errors);
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
                expected.what()
            ),
            Err(errors) => panic!(
                "{relative} fails `artifact check`:\n  {}",
                errors.join("\n  ")
            ),
        }
    }
}

/// Coverage invariant: the list above is the WHOLE set of committed artifacts
/// under `tests/fixtures/`. A new JSON fixture carrying a root-level
/// `formatVersion` that is not in [`COMMITTED_ARTIFACTS`] fails here by name,
/// so the gate cannot be quietly outgrown (the `entity_specs_run_green`
/// coverage discipline, applied to artifacts).
///
/// The walk is fail-closed in the other direction too: a `*.json` file under
/// the fixtures tree that cannot be read or parsed fails by name rather than
/// being skipped — otherwise a broken new artifact could hide from discovery.
#[test]
fn committed_artifact_set_is_complete() {
    let root = fixture_root();
    let mut discovered = BTreeSet::new();
    let mut unreadable = Vec::new();
    collect_artifacts(&root, &root, &mut discovered, &mut unreadable);

    assert!(
        unreadable.is_empty(),
        "fixture JSON could not be read/parsed (it cannot be classified, so it cannot be \
         skipped silently): {unreadable:?}"
    );

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
/// (the artifact marker). Schema inputs and OpenAPI documents cannot match; a
/// file that cannot be read or parsed is recorded in `unreadable`.
fn collect_artifacts(
    root: &Path,
    dir: &Path,
    found: &mut BTreeSet<String>,
    unreadable: &mut Vec<String>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_artifacts(root, &path, found, unreadable);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                unreadable.push(format!("{relative}: {error}"));
                continue;
            }
        };
        let document = match serde_json::from_str::<Value>(&text) {
            Ok(document) => document,
            Err(error) => {
                unreadable.push(format!("{relative}: {error}"));
                continue;
            }
        };
        let Value::Object(root_object) = document else {
            continue;
        };
        if root_object.contains_key("formatVersion") {
            found.insert(relative);
        }
    }
}

/// Teeth: the real fixture bytes, rewritten into a 0.2-era document in a temp
/// dir, fail with upstream's exact migration message — not a diff wall. The
/// committed file is never touched (`CODEGRAPH_ARTIFACT_FIXTURES` is the
/// copy-the-tree hook the shell-level teeth run uses).
#[test]
fn a_0_2_era_artifact_is_named_not_diffed() {
    let mut legacy: Value =
        serde_json::from_str(&fixture_text(LIBRARY_DDD)).expect("fixture is JSON");
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
/// with upstream's artifact-check wording — which also says how to fix it —
/// over a temp copy of the real bytes.
#[test]
fn a_newer_format_version_names_the_upgrade() {
    let mut bumped: Value =
        serde_json::from_str(&fixture_text(LIBRARY_DDD)).expect("fixture is JSON");
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
        errors[0].contains(
            "unsupported DDD design artifact formatVersion 99: this tool reads formatVersion 3 \
             — regenerate the artifact with a matching pherolang version"
        ),
        "the version gate is upstream's wording (naming the fix): {errors:?}"
    );
}

/// A design stripped of its provenance is a contract failure, not a pass:
/// `pheroVersion` is how a consumer tells a compiler-emitted design from a
/// hand-edited document. Checked on EVERY design artifact (no `application`
/// precondition) because `DddModel::new` always stamps it.
#[test]
fn a_design_without_provenance_is_rejected() {
    let mut stripped: Value =
        serde_json::from_str(&fixture_text(LIBRARY_DDD)).expect("fixture is JSON");
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

    // A modules-only design (no `application`) is held to the same rule — the
    // check is not gated on the design carrying an application.
    let modules_only = r#"{"formatVersion": 2, "modules": []}"#;
    let errors = check_artifact(modules_only).expect_err("modules-only design still needs it");
    assert!(
        errors.iter().any(|e| e.contains("pheroVersion")),
        "the modules-only arm is covered too: {errors:?}"
    );
}

/// Teeth: renaming a camelCase structural key to snake_case is caught. The
/// readers ignore unknown fields, so without this scan `never_both` would load
/// and the `neverBoth` policy would be silently dropped.
#[test]
fn a_snake_case_structural_key_is_named() {
    let mut renamed: Value =
        serde_json::from_str(&fixture_text(SOD_ACTOR)).expect("fixture is JSON");
    let blocks = renamed
        .get_mut("blocks")
        .and_then(Value::as_array_mut)
        .expect("the actor fixture carries blocks");
    let block = blocks[0].as_object_mut().expect("a block is an object");
    let never_both = block
        .remove("neverBoth")
        .expect("the actor fixture declares neverBoth");
    block.insert("never_both".to_string(), never_both);

    let errors = check_artifact(&serde_json::to_string(&renamed).unwrap())
        .expect_err("a snake_case structural key must fail");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("never_both") && e.contains("neverBoth")),
        "the violation names both spellings: {errors:?}"
    );
}

/// Teeth (against upstream's rule 11): a `formatVersion`-bearing root whose
/// only key is an unknown one is `unrecognized`, never an actor artifact —
/// accepting it would validate a document whose payload the readers drop.
#[test]
fn an_unknown_root_shape_is_unrecognized() {
    let errors = check_artifact(r#"{"formatVersion": 2, "nobodyKnowsMe": true}"#)
        .expect_err("an unknown root shape must not pass as an actor artifact");
    assert!(
        errors[0].starts_with("unrecognized artifact: no known root shape"),
        "{errors:?}"
    );

    // The canonical minimal-empty actor artifact (the marker alone) IS
    // recognized — the strictness above must not swallow upstream's rule 11.
    assert_eq!(
        check_artifact(r#"{"formatVersion": 2}"#).expect("the marker alone is the actor artifact"),
        ArtifactKind::ActorModel
    );

    let errors = check_artifact(r#"{"hello": "world"}"#).expect_err("not an artifact");
    assert!(errors[0].starts_with("unrecognized artifact"), "{errors:?}");

    let errors = check_artifact("not json at all").expect_err("not JSON");
    assert!(errors[0].starts_with("not valid JSON:"), "{errors:?}");
}

/// Teeth: a named collection whose entry carries no usable `name` is rejected
/// (upstream's `check_named_array`) — the readers would accept it and the
/// entity would be nameless downstream.
#[test]
fn a_named_array_entry_without_a_name_is_rejected() {
    let errors = check_artifact(r#"{"formatVersion": 2, "events": [{"name": ""}]}"#)
        .expect_err("a nameless event must fail");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("events[0] \"name\" must be a non-empty string")),
        "the violation names the collection, its index and the field: {errors:?}"
    );

    let errors = check_artifact(r#"{"formatVersion": 2, "events": [{}]}"#)
        .expect_err("an event with no name key at all must fail");
    assert!(
        errors.iter().any(|e| e.contains("is missing \"name\"")),
        "{errors:?}"
    );
}
