//! Shared fixture harness for the service-ops tests (issue #590).
//!
//! Included via `#[path]` from `service_ops_tests.rs` (node-free pins) and
//! `service_ops_compile_gate.rs` (the ignored compile gate) so both drive
//! the SAME fixture — one recipe, no drift.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use codegraph_core::traits::GraphIngestor;
use tempfile::TempDir;

pub const PAY_RUN_SCHEMA: &str = r#"{
  "title": "PayRunType",
  "type": "object",
  "required": ["payRunId", "payRunStatus", "total"],
  "properties": {
    "payRunId": { "type": "string" },
    "payRunStatus": { "type": "string" },
    "total": { "type": "integer" },
    "note": { "type": "string" }
  }
}"#;

/// A refers-linked CHILD entity: `role = "child"` in the entity config with
/// `parent`/`parent_ref` — the unblocked target shape for child service ops
/// (the route-nested child with its OWN repository). The parent FK is the
/// synthetic `pay_run_id` column (config `parent_ref`), not a child-side
/// schema property.
pub const PAY_RUN_LINE_SCHEMA: &str = r#"{
  "title": "PayRunLineType",
  "type": "object",
  "required": ["lineId"],
  "properties": {
    "lineId": { "type": "string" },
    "notes": { "type": "string" }
  }
}"#;

/// The depth-2 GRANDCHILD entity (issue #629): refers-linked under the
/// CHILD — `role = "child"` with `parent = "PayRunLineType"`. The service-op
/// plane renders its action routes through the grandparent branch; the
/// #551 compile pin exists because that surface compiled NOWHERE before.
pub const PAY_RUN_LINE_ADJUSTMENT_SCHEMA: &str = r#"{
  "title": "PayRunLineAdjustmentType",
  "type": "object",
  "required": ["adjustmentId"],
  "properties": {
    "adjustmentId": { "type": "string" },
    "reason": { "type": "string" }
  }
}"#;

pub const PAY_RUN_MOX: &str = r#"
package payroll

enum PayRunStatus {
    Draft = 0
    Calculating = 1
    Approved = 2
    Reversed = 3
}

class PayRunType {
    String payRunId
    PayRunStatus payRunStatus
    int total
    String [0..1] note
}

class PayRunLineType {
    String lineId
    String [0..1] notes
}

class PayRunLineAdjustmentType {
    String adjustmentId
    String [0..1] reason
}
"#;

/// Two entity designs (a root and its refers-linked CHILD, each with a full
/// repository), a lifecycle machine on the root, and services carrying every
/// lowering shape on both entities. The CHILD service ops delegate to the
/// child repository: `adjust` (save), `remove` (delete), a
/// capability-gated save (`reprice`), and a `findAll` delegation that keeps
/// the 501 seam fail-closed (`summarize`). The child machine surface stays
/// empty — child machines are title-keyed machinery, exercised by the root.
pub const PAYROLL_DDD: &str = r#"
import "pay_run.mox";

application Payroll {
    module payroll {
        entity PayRunType scaffold repository PayRunRepository {
            findById;
            findAll;
            save;
            delete;
        }
        machine payRunStatus: PayRunStatus {
            initial Draft;
            Draft -> Calculating via Submit;
            Calculating -> Approved via Approve;
            Approved -> Reversed via Reverse;
            terminal Reversed;
        }
        service PayRunService {
            inject PayRunRepository;
            purge => PayRunRepository.delete capability ApprovePayRun;
            inspect => PayRunRepository.findById capability InspectPayRun;
            archive => PayRunRepository.save;
            reverse => PayRunRepository.save capability ApprovePayRun;
            submit => PayRunRepository.save capability SubmitPayRun when (payRunStatus == Draft);
            approve => PayRunRepository.save capability ApprovePayRun;
            boolean notify(String note) capability SubmitPayRun;
            protected String internalAdjust();
            applyPayRun(int total, String note) throws NotFound, Conflict => PayRunRepository {
                let run = self.findById("current") else error NotFound("PayRun", "current");
                guard run.total < 100 else error Conflict("total exceeds the action limit");
                run.total = total;
                let saved = self.save(run);
            }
        }
        entity PayRunLineType scaffold repository PayRunLineRepository {
            findById;
            findAll;
            save;
            delete;
        }
        service PayRunLineService {
            inject PayRunLineRepository;
            adjust => PayRunLineRepository.save;
            reprice => PayRunLineRepository.save capability AdjustPayRunLine;
            remove => PayRunLineRepository.delete;
            summarize => PayRunLineRepository.findAll;
        }
        entity PayRunLineAdjustmentType scaffold repository PayRunLineAdjustmentRepository {
            findById;
            findAll;
            save;
            delete;
        }
        service PayRunLineAdjustmentService {
            inject PayRunLineAdjustmentRepository;
            recalculate => PayRunLineAdjustmentRepository.save capability RecalculateAdjustment;
            voidOut => PayRunLineAdjustmentRepository.delete;
        }
    }
}
"#;

// ── Issue #661: aggregate child operations (`children <Class>`) ────────

/// The aggregate-child domain (issue #661): a `PayRunType` root with a
/// `refers PayRunLineType[0..] lines` child COLLECTION (the cross-reference
/// shape — a `contains` child is a value object with no standalone
/// repository, which upstream Rule 6 forbids and #661 excludes), and the
/// child's inverse `refers PayRunType[0..1] payRun` so the parent's array
/// resolves to the child-side `pay_run_id` FK (no junction table).
pub const CHILD_OPS_MOX: &str = r#"
package payroll

enum PayRunStatus {
    Draft = 0
    Calculated = 1
}

class PayRunType {
    String payRunId
    PayRunStatus payRunStatus
    int total
    refers PayRunLineType[] lines opposite payRun
}

class PayRunLineType {
    String lineId
    int quantity
    String [0..1] notes
    refers PayRunType[0..1] payRun opposite lines
}
"#;

/// The aggregate-child design (issue #661): the module-level
/// `children PayRunType { … => lines.<action> }` block — upstream's
/// child-operation declaration — over an aggregate root whose child class
/// is a repository-bearing entity (the #661 in-scope shape). The
/// `AddLineUnscoped` service op is the RETAINED cross-aggregate seam: a
/// capability binds it to the root while it delegates to the CHILD's
/// repository, and it carries no `childScope` — it must stay a 501 naming
/// the missing modeling construct.
pub const CHILD_OPS_DDD: &str = r#"
import "childops.mox";

application Payroll {
    module payroll {
        entity PayRunType scaffold repository PayRunRepository {
            findById;
            findAll;
            save;
            delete;
        }
        entity PayRunLineType scaffold repository PayRunLineRepository {
            findById;
            findAll;
            save;
            delete;
        }
        dto PayRunLineDraft from PayRunLineType {
            lineId;
            quantity;
            notes;
        }
        children PayRunType {
            boolean addLine(PayRunLineDraft line) => lines.add;
            boolean saveLine(PayRunLineDraft line) => lines.save;
            boolean removeLine() => lines.remove;
        }
        service PayRunService {
            inject PayRunRepository;
            addUnscoped => PayRunLineRepository.save capability AddUnscopedLine;
        }
    }
}
"#;

/// The aggregate-child actor policy: the capability that binds
/// `addUnscoped` to the ROOT entity while its delegation targets the child
/// (the cross-aggregate seam shape).
pub const CHILD_OPS_ACTOR: &str = r#"
import "childops.mox"

actors Payroll {
    actor Owner

    capability AddUnscopedLine on PayRunType

    grant Owner {
        permit AddUnscopedLine
    }
}
"#;

/// The aggregate-child config: mox-first (no `entities` key), with the
/// child entity configured as a `role = "child"` of the root so its action
/// routes nest under the parent (the WS4 shape) and the parent-scoped read
/// exists. `parent_ref` names the REAL `pay_run_id` column the mox
/// `refers … opposite` pair projects.
pub const CHILD_OPS_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "model-schemas"
postgres_schema = "payroll"

[domains.payroll.entity_config.PayRunLineType]
role = "child"
parent = "PayRunType"
parent_ref = "pay_run_id"
"#;

/// Write the aggregate-child fixture (issue #661).
pub fn write_child_ops_fixture() -> Fixture {
    write_child_ops_fixture_with(CHILD_OPS_DOMAINS_TOML)
}

/// [`write_child_ops_fixture`] with a caller-supplied config — the
/// fail-closed variant (no `role = "child"`) needs the SAME model.
pub fn write_child_ops_fixture_with(config_text: &str) -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    std::mem::forget(dir); // the fixture outlives the guard on purpose
    let schemas_dir = root.join("model-schemas");
    fs::create_dir_all(&schemas_dir).unwrap();
    let mox = root.join("childops.mox");
    fs::write(&mox, CHILD_OPS_MOX).unwrap();
    let ddd = root.join("childops.ddd");
    fs::write(&ddd, CHILD_OPS_DDD).unwrap();
    let actor = root.join("policy.actor");
    fs::write(&actor, CHILD_OPS_ACTOR).unwrap();
    let config = root.join("domains.toml");
    fs::write(&config, config_text).unwrap();
    Fixture {
        schemas_dir,
        mox,
        ddd,
        actor,
        config,
    }
}

/// Ingest the aggregate-child fixture: mox bridge → `.ddd` design → actor
/// policy (the run path's order).
pub async fn ingest_child_ops(fixture: &Fixture) -> codegraph_grafeo::GrafeoEngine {
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().unwrap();
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let mox_stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;
    assert_eq!(mox_stats.skipped, 0, "mox classes must bridge the schemas");
    codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.ddd),
        &config,
    )
    .await
    .unwrap();
    let policy = codegraph::ifml_actor_import::resolve_actor_policy(
        &["policy.actor".to_string()],
        fixture.ddd.parent().unwrap(),
    )
    .expect("the aggregate-child policy must resolve");
    engine
        .ingest_actor_policy(&policy)
        .await
        .expect("policy ingests");
    engine
}

// ── Issue #660: authored transition bindings ──────────────────────────

/// Two entities in ONE module, each carrying its own lifecycle machine, and
/// service operations that declare their transition explicitly
/// (`name => Repository.save transition <EnumType>.<Trigger>`).
///
/// The two machines force the qualified spelling upstream (E0352): a module
/// with more than one machine-bearing design requires the enum-type
/// qualifier, so this fixture pins the `TransitionRef.machine` resolution
/// too. The op names resemble NEITHER trigger (`prepareRelease`, `finalize`,
/// `prepareReview`, `clearReview`), so the heuristics can only ever reach
/// them by accident — the authored binding is the whole selector.
pub const RELEASE_MOX: &str = r#"
package release

enum ReleaseStatus {
    Held = 0
    SignedOff = 1
    Released = 2
}

enum ReviewStatus {
    Pending = 0
    Cleared = 1
}

class ReleaseType {
    String releaseId
    ReleaseStatus releaseStatus
    int total
    String [0..1] note
}

class ReviewType {
    String reviewId
    ReviewStatus reviewStatus
    String [0..1] remarks
}
"#;

pub const RELEASE_DDD: &str = r#"
import "release.mox";

application Release {
    module release {
        entity ReleaseType scaffold repository ReleaseRepository {
            findById;
            findAll;
            save;
            delete;
        }
        machine releaseStatus: ReleaseStatus {
            initial Held;
            Held -> SignedOff via SignOff;
            SignedOff -> Released via Publish;
            terminal Released;
        }
        entity ReviewType scaffold repository ReviewRepository {
            findById;
            findAll;
            save;
            delete;
        }
        machine reviewStatus: ReviewStatus {
            initial Pending;
            Pending -> Cleared via Clear;
            terminal Cleared;
        }
        service ReleaseService {
            inject ReleaseRepository;
            prepareRelease => ReleaseRepository.save transition ReleaseStatus.SignOff;
            finalize => ReleaseRepository.save transition ReleaseStatus.Publish;
            inspect => ReleaseRepository.findById;
            summarize => ReleaseRepository.findAll;
        }
        service ReviewService {
            inject ReviewRepository;
            prepareReview => ReviewRepository.save transition ReviewStatus.Clear;
            inspectReview => ReviewRepository.findById;
        }
    }
}
"#;

/// The binding fixture's config: mox-first (no `entities` key — the mox
/// bridge owns the graph), one domain.
pub const RELEASE_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.release]
label = "Release"
schema_dir = "model-schemas"
postgres_schema = "release"
"#;

// ── Issue #663: injected-scope `when` guards (`condition_paths`) ────────

/// A PayRun whose `submit` action is guarded by the RELATED Timecard's
/// machine state: `when (payRunStatus == Draft) &&
/// TimecardRepository.timecardStatus == Approved`. The injected path root
/// is the declared dependency name (the upstream `PathScan` contract —
/// the first segment must match an `inject`ed repository exactly as
/// declared), and the own-record `refers` FK (`timecard`) is the id source
/// the rendered handler loads the related record through.
pub const GUARD_MOX: &str = r#"
package guardpay

enum PayRunStatus {
    Draft = 0
    Calculating = 1
    Posted = 2
}

enum TimecardStatus {
    Pending = 0
    Approved = 1
    Rejected = 2
}

class TimecardType {
    String timecardId
    TimecardStatus timecardStatus
    int hours
}

class PayRunType {
    String payRunId
    PayRunStatus payRunStatus
    int total
    String [0..1] note
    refers TimecardType[0..1] timecard
}
"#;

pub const GUARD_DDD: &str = r#"
import "guard_pay_run.mox";

application GuardPay {
    module guardpay {
        entity PayRunType scaffold repository PayRunRepository {
            findById;
            findAll;
            save;
            delete;
        }
        machine payRunStatus: PayRunStatus {
            initial Draft;
            Draft -> Calculating via Submit;
            Calculating -> Posted via Approve;
            terminal Posted;
        }
        entity TimecardType scaffold repository TimecardRepository {
            findById;
            findAll;
            save;
        }
        machine timecardStatus: TimecardStatus {
            initial Pending;
            Pending -> Approved via Approve;
            terminal Approved;
        }
        service PayRunGuardService {
            inject PayRunRepository;
            inject TimecardRepository;
            submit => PayRunRepository.save capability SubmitGuardPayRun when (payRunStatus == Draft && TimecardRepository.timecardStatus == Approved);
        }
    }
}
"#;

pub const GUARD_ACTOR: &str = r#"
import "guard_pay_run.mox"

actors GuardPay {
    actor Owner

    capability SubmitGuardPayRun on PayRunType

    grant Owner {
        permit SubmitGuardPayRun
    }
}
"#;

/// The guard fixture's config: mox-first (no `entities` key — the mox
/// bridge owns the graph), one domain.
pub const GUARD_DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.guardpay]
label = "GuardPay"
schema_dir = "model-schemas"
postgres_schema = "guardpay"
"#;

/// The injected-scope guard fixture (issue #663): mox-first, one domain,
/// two machine-bearing designs, and one op whose op-level `when` reads an
/// INJECTED repository's machine field across the aggregate boundary.
pub fn write_guard_fixture() -> Fixture {
    write_guard_fixture_with(GUARD_MOX)
}

/// The same design with the injected path's remainder DOTTED one level
/// deeper (`...Timecard.holder.name`) — the chain COMPILES upstream (each
/// segment is a stored field), so it reaches the lowering and must be a
/// named generation error: a single segment under the injected root is the
/// closed subset, never a silently broader guard.
pub fn guard_ddd_with_deep_injected_path() -> (String, String) {
    (
        GUARD_MOX
            .replace(
                "class TimecardType {",
                "class HolderType {\n    String holderId\n    String name\n}\n\nclass TimecardType {",
            )
            .replace(
                "    TimecardStatus timecardStatus\n",
                "    TimecardStatus timecardStatus\n    refers HolderType[1] holder\n",
            ),
        GUARD_DDD.replace(
            "TimecardRepository.timecardStatus == Approved",
            "TimecardRepository.holder.name == \"Ada\"",
        ),
    )
}

/// The same design with the injected root naming a class the entity does
/// NOT reference: the guard has no id source (no `timecard` FK) and the
/// generation error must say so — never a load of an unresolvable record.
/// The path still compiles upstream (the driver checks it against the
/// injected CLASS's fields, not the op entity's), so this reaches the
/// lowering.
pub fn guard_fixture_without_fk() -> Fixture {
    let mox = GUARD_MOX.replace("    refers TimecardType[0..1] timecard\n", "");
    write_guard_fixture_with(&mox)
}

/// The same design with TWO `refers` to the injected class: the id source
/// is ambiguous and the generation error must name both fields — never a
/// guess.
pub fn guard_fixture_with_ambiguous_fk() -> Fixture {
    let mox = GUARD_MOX.replace(
        "    refers TimecardType[0..1] timecard\n",
        "    refers TimecardType[0..1] timecard\n    refers TimecardType[0..1] reviewTimecard\n",
    );
    write_guard_fixture_with(&mox)
}

/// `write_guard_fixture` over a different mox source (the guard mutations
/// differ only in the model).
pub fn write_guard_fixture_with(mox_source: &str) -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    std::mem::forget(dir); // the fixture outlives the guard on purpose
    let schemas_dir = root.join("model-schemas");
    let mox = root.join("guard_pay_run.mox");
    fs::write(&mox, mox_source).unwrap();
    let ddd = root.join("guardpay.ddd");
    fs::write(&ddd, GUARD_DDD).unwrap();
    let actor = root.join("policy.actor");
    fs::write(&actor, GUARD_ACTOR).unwrap();
    let config = root.join("domains.toml");
    fs::write(&config, GUARD_DOMAINS_TOML).unwrap();
    Fixture {
        schemas_dir,
        mox,
        ddd,
        actor,
        config,
    }
}

/// The same design with ONE unlowerable statement body (issue #662): the
/// `let` initializer uses arithmetic, which the closed body subset refuses.
/// The run-path validation pass must surface it as a HARD error naming the
/// op and the construct — never a 501, never a partial interpretation.
pub fn payroll_ddd_with_refused_body() -> String {
    PAYROLL_DDD.replace(
        "                run.total = total;",
        "                let doubled = run.total * 2;\n                run.total = total;",
    )
}

/// Manager holds InspectPayRun unconditionally, plus the CHILD entity's
/// `AdjustPayRunLine` (unconditional — child grant-`when` typing is
/// upstream-blocked with the module's single machine; the capability gate
/// itself is what the child handler pin exercises).
pub const POLICY_ACTOR: &str = r#"
import "pay_run.mox"

actors Payroll {
    actor Owner
    actor Manager

    capability ApprovePayRun on PayRunType
    capability InspectPayRun on PayRunType
    capability SubmitPayRun on PayRunType
    capability PreparePayRun on PayRunType
    capability AdjustPayRunLine on PayRunLineType
    capability RecalculateAdjustment on PayRunLineAdjustmentType

    grant Owner {
        permit ApprovePayRun
        permit SubmitPayRun
    }

    grant Manager {
        permit InspectPayRun
        permit AdjustPayRunLine
        permit RecalculateAdjustment
    }

    never_both { PreparePayRun, ApprovePayRun }
}
"#;

/// The second import: valid alone, violating after the merge — Owner gains
/// `PreparePayRun`, the `never_both` partner of the `ApprovePayRun` the
/// first file granted.
pub const POLICY_B_ACTOR: &str = r#"
import "pay_run.mox"

actors Payroll {
    actor Owner

    capability PreparePayRun on PayRunType

    grant Owner {
        permit PreparePayRun
    }
}
"#;

pub const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "model-schemas"
postgres_schema = "payroll"
entities = ["PayRunType", "PayRunLineType", "PayRunLineAdjustmentType"]

[domains.payroll.entity_config.PayRunLineType]
role = "child"
parent = "PayRunType"
parent_ref = "pay_run_id"

[domains.payroll.entity_config.PayRunLineAdjustmentType]
role = "child"
parent = "PayRunLineType"
parent_ref = "pay_run_line_id"
"#;

/// The mox-first twin: NO `entities` key (author-declarative mox), no JSON
/// schemas — the mox bridge owns the graph. Same child declaration. Pins
/// the openapi service-op registration in mox-first mode (the pre-WS4 loop
/// iterated `domain_entry.entities`, which is empty here).
pub const DOMAINS_TOML_MOX_FIRST: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "model-schemas"
postgres_schema = "payroll"

[domains.payroll.entity_config.PayRunLineType]
role = "child"
parent = "PayRunType"
parent_ref = "pay_run_id"

[domains.payroll.entity_config.PayRunLineAdjustmentType]
role = "child"
parent = "PayRunLineType"
parent_ref = "pay_run_line_id"
"#;

// ── Harness ────────────────────────────────────────────────────────────

pub struct Fixture {
    pub schemas_dir: PathBuf,
    pub mox: PathBuf,
    pub ddd: PathBuf,
    pub actor: PathBuf,
    pub config: PathBuf,
}

pub fn write_fixture_in(_prefix: &str, ddd_source: &str) -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    std::mem::forget(dir); // the fixture outlives the guard on purpose
    // SchemaLoader layout: <schema_dir>/<domain>/json/*.json.
    let schemas_dir = root.join("model-schemas").join("payroll").join("json");
    fs::create_dir_all(&schemas_dir).unwrap();
    fs::write(schemas_dir.join("PayRunType.json"), PAY_RUN_SCHEMA).unwrap();
    fs::write(schemas_dir.join("PayRunLineType.json"), PAY_RUN_LINE_SCHEMA).unwrap();
    fs::write(
        schemas_dir.join("PayRunLineAdjustmentType.json"),
        PAY_RUN_LINE_ADJUSTMENT_SCHEMA,
    )
    .unwrap();
    let schemas_dir = root.join("model-schemas");
    let mox = root.join("pay_run.mox");
    fs::write(&mox, PAY_RUN_MOX).unwrap();
    let ddd = root.join("payroll.ddd");
    fs::write(&ddd, ddd_source).unwrap();
    let actor = root.join("policy.actor");
    fs::write(&actor, POLICY_ACTOR).unwrap();
    let actor_b = root.join("policy_b.actor");
    fs::write(&actor_b, POLICY_B_ACTOR).unwrap();
    let config = root.join("domains.toml");
    fs::write(&config, DOMAINS_TOML).unwrap();
    Fixture {
        schemas_dir,
        mox,
        ddd,
        actor,
        config,
    }
}

/// A mox-FIRST variant of the fixture: no JSON schemas at all, and the
/// config carries NO `entities` key (author-declarative mox). The mox
/// bridge owns the graph; the `.ddd` + `.actor` inputs are unchanged.
pub fn write_mox_first_fixture_in(_prefix: &str, ddd_source: &str) -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    std::mem::forget(dir); // the fixture outlives the guard on purpose
    let schemas_dir = root.join("model-schemas");
    let mox = root.join("pay_run.mox");
    fs::write(&mox, PAY_RUN_MOX).unwrap();
    let ddd = root.join("payroll.ddd");
    fs::write(&ddd, ddd_source).unwrap();
    let actor = root.join("policy.actor");
    fs::write(&actor, POLICY_ACTOR).unwrap();
    let actor_b = root.join("policy_b.actor");
    fs::write(&actor_b, POLICY_B_ACTOR).unwrap();
    let config = root.join("domains.toml");
    fs::write(&config, DOMAINS_TOML_MOX_FIRST).unwrap();
    Fixture {
        schemas_dir,
        mox,
        ddd,
        actor,
        config,
    }
}

/// The authored-transition-binding fixture (issue #660): mox-first, one
/// domain, two machine-bearing designs in one module (the qualifier-forcing
/// shape), and every machine-bound op declaring `transition`.
/// `write_binding_fixture` over a different `.ddd` source (the multi-machine
/// `when` mutation differs only in the design).
pub fn write_binding_fixture_with(ddd_source: &str) -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    std::mem::forget(dir); // the fixture outlives the guard on purpose
    let schemas_dir = root.join("model-schemas");
    fs::create_dir_all(&schemas_dir).unwrap();
    let mox = root.join("release.mox");
    fs::write(&mox, RELEASE_MOX).unwrap();
    let ddd = root.join("release.ddd");
    fs::write(&ddd, ddd_source).unwrap();
    // No actor policy: the fixture's ops are ungated, so no capability
    // plane is involved (the binding plane is independent of it).
    let actor = root.join("policy.actor");
    let config = root.join("domains.toml");
    fs::write(&config, RELEASE_DOMAINS_TOML).unwrap();
    Fixture {
        schemas_dir,
        mox,
        ddd,
        actor,
        config,
    }
}

pub fn write_binding_fixture() -> Fixture {
    write_binding_fixture_with(RELEASE_DDD)
}

/// The deep-path guard mutation as a ready fixture: BOTH files carry the
/// mutation (the mox gains the `HolderType` chain, the guard reads it).
pub fn guard_fixture_with_deep_path() -> Fixture {
    let (mox, ddd) = guard_ddd_with_deep_injected_path();
    let fixture = write_guard_fixture_with(&mox);
    std::fs::write(&fixture.ddd, ddd).unwrap();
    fixture
}

/// Ingest the binding fixture: the mox bridge + the `.ddd` design, with no
/// actor policy (nothing in the fixture is capability-gated).
pub async fn ingest_binding(fixture: &Fixture) -> codegraph_grafeo::GrafeoEngine {
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().unwrap();
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let mox_stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;
    assert_eq!(mox_stats.skipped, 0, "mox classes must bridge the schemas");
    let ddd_stats = codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.ddd),
        &config,
    )
    .await
    .unwrap();
    assert_eq!(
        ddd_stats.services, 2,
        "both services must ingest (release + review)"
    );
    engine
}

/// Ingest the injected-scope guard fixture (issue #663): mox + `.ddd` + the
/// actor policy (the capability binds the guarded op to the entity).
pub async fn ingest_guard(fixture: &Fixture) -> codegraph_grafeo::GrafeoEngine {
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().unwrap();
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let mox_stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;
    assert_eq!(mox_stats.skipped, 0, "mox classes must bridge the schemas");
    let ddd_stats = codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.ddd),
        &config,
    )
    .await
    .unwrap();
    assert_eq!(
        ddd_stats.services, 1,
        "the guard service must ingest"
    );
    let policy = codegraph::ifml_actor_import::resolve_actor_policy(
        &["policy.actor".to_string()],
        fixture.ddd.parent().unwrap(),
    )
    .expect("the actor policy must resolve");
    engine
        .ingest_actor_policy(&policy)
        .await
        .expect("policy ingests");
    engine
}

/// Scratch dir for the node-free service-ops pins: small, generation-only
/// trees that never build a Rust crate, so the default temp dir is fine.
pub fn tempdir() -> TempDir {
    TempDir::new().unwrap()
}

/// Resolve the compile gate's scratch root (issue #734).
///
/// The gate's scratch trees each carry a FULL cargo `target/` (a generated
/// app plus its dependency graph — ~1 GB per test with `cargo check`, and
/// up to ~15 GB peak reported for the whole parallel run). On the standard
/// dev/CI host `std::env::temp_dir()` is `/tmp`, a 31 GiB tmpfs with a user
/// quota: the parallel run filled it and died with `Disk quota exceeded
/// (os error 122)` from a dependency's build script — a disk artifact that
/// read as a code regression in `just ci-local`. So the scratch root must
/// land on the DISK-BACKED cargo target dir. Relocating, not serialising:
/// the three tests stay parallel.
///
/// Resolution order:
/// 1. `cargo_target_tmpdir` — Cargo sets `CARGO_TARGET_TMPDIR` for
///    integration tests to `<target-dir>/tmp` (the workspace root `target/`,
///    NOT the tracked `crates/codegraph/target/` — issue #683a);
/// 2. `<manifest>/../../target/gate-tmp` — the workspace target dir derived
///    from `CARGO_MANIFEST_DIR`, for any runner that does not set it.
///
/// Pure so the priority order is unit-testable without cargo's env.
fn gate_temp_root(cargo_target_tmpdir: Option<&str>, manifest_dir: &Path) -> PathBuf {
    match cargo_target_tmpdir {
        Some(dir) => PathBuf::from(dir),
        None => {
            let workspace_root = manifest_dir
                .parent()
                .and_then(Path::parent)
                .unwrap_or(manifest_dir);
            workspace_root.join("target").join("gate-tmp")
        }
    }
}

/// The compile gate's resolved scratch root (issue #734). `option_env!`, not
/// `env!`: the fallback must stay reachable (and buildable) for any harness
/// that compiles this file without Cargo's integration-test env.
fn gate_scratch_root() -> PathBuf {
    gate_temp_root(
        option_env!("CARGO_TARGET_TMPDIR"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
}

/// Disk-backed scratch dir for the compile gate (issue #734): a full
/// generated app plus its cargo `target/` lives under here, so it must never
/// be `std::env::temp_dir()`. Prints the resolved root once per test so a
/// future disk-exhaustion report names the filesystem it hit.
pub fn gate_tempdir() -> TempDir {
    let root = gate_scratch_root();
    if let Err(e) = fs::create_dir_all(&root) {
        panic!("create compile-gate scratch root {}: {e}", root.display());
    }
    eprintln!("scratch: {}", root.display());
    TempDir::new_in(&root)
        .unwrap_or_else(|e| panic!("create compile-gate temp dir under {}: {e}", root.display()))
}

pub async fn ingest(fixture: &Fixture) -> codegraph_grafeo::GrafeoEngine {
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().unwrap();
    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();
    let classifier = codegraph_classifier::config::parse_classifier_config(Path::new(
        "tests/fixtures/classifier.toml",
    ))
    .unwrap();
    let entity_names: std::collections::HashSet<String> = config
        .domains
        .values()
        .flat_map(|d| d.entities.iter().cloned())
        .collect();
    if !entity_names.is_empty() {
        codegraph::ingest::async_ingest::ingest_schemas(
            &engine,
            &fixture.schemas_dir,
            &classifier,
            &entity_names,
            &codegraph_config::UiOverrideConfig::default(),
            &config.defaults.type_suffix,
        )
        .await
        .unwrap();
    }
    let mox_stats = codegraph::ingest::mox_ingest::ingest_mox_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.mox),
        &config,
        &config.defaults.type_suffix,
    )
    .await
    .unwrap()
    .stats;
    assert_eq!(mox_stats.skipped, 0, "mox classes must bridge the schemas");
    let ddd_stats = codegraph::ingest::ddd_ingest::ingest_ddd_files(
        &engine,
        &engine,
        std::slice::from_ref(&fixture.ddd),
        &config,
    )
    .await
    .unwrap();
    assert_eq!(
        ddd_stats.services, 3,
        "all three services must ingest (root, child, grandchild)"
    );
    let policy = codegraph::ifml_actor_import::resolve_actor_policy(
        &["policy.actor".to_string(), "policy_b.actor".to_string()],
        fixture.ddd.parent().unwrap(),
    )
    .expect("the actor policies must resolve");
    engine
        .ingest_actor_policy(&policy)
        .await
        .expect("policy ingests");
    engine
}

pub fn profiles_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .join("profiles.toml")
}

pub async fn run_generation(
    engine: &codegraph_grafeo::GrafeoEngine,
    fixture: &Fixture,
    output_dir: &Path,
    domain_types_dir: &Path,
    hooks_tmp: &Path,
) -> codegraph_generate::report::GenerationReport {
    let tera = codegraph::generate::template_engine::create_tera(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
    )
    .unwrap();

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let type_contracts_path = workspace_root
        .join("crates")
        .join("codegraph-type-contracts");
    let workflow_path = workspace_root.join("crates").join("codegraph-workflow");
    let profiles = profiles_path();

    let registry = codegraph::profile::CapabilityRegistry::new();
    let mut resolved =
        codegraph::profile::load_and_resolve_profile(&profiles, "default", None).unwrap();
    for section in resolved.sections.values_mut() {
        section.generators.retain(|g| !g.starts_with("grpc_"));
    }
    let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry).unwrap();

    let project_config = codegraph::generate::ProjectConfig {
        identity: codegraph::generate::IdentityConfig {
            app_name: "test-app".into(),
            domain_types_crate: "domain_types".into(),
            generator_name: "codegraph-test".into(),
            ..Default::default()
        },
        paths: codegraph::generate::PathsConfig {
            type_contracts_base: type_contracts_path.to_string_lossy().to_string(),
            codegraph_workflow_base: workflow_path.to_string_lossy().to_string(),
            domain_types_base: "domain-types".into(),
            ..Default::default()
        },
        codegen: codegraph::generate::CodegenConfig {
            types_import_prefix: "crate::types".into(),
            ..Default::default()
        },
        ..Default::default()
    };

    let config = codegraph_config::config::parse_domain_config(&fixture.config).unwrap();

    codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
        db: engine,
        config: &config,
        output_dir,
        tera: &tera,
        ui_overrides: &Default::default(),
        ui_domains: &Default::default(),
        schema_base_dir: Path::new(""),
        seed_config: None,
        domain_types_base: Some(domain_types_dir),
        hooks_base: Some(hooks_tmp),
        ext_points: None,
        integration_manifests: None,
        build_plan: Some(&plan),
        profile_name: "default",
        ifml_frameworks: vec![],
        ifml_components: None,
        ux_rules: None,
        project_config: Some(&project_config),
        emdash_plugins: None,
        domain_config_dir: None,
    })
    .await
    .unwrap()
}

pub fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[cfg(test)]
mod gate_temp_root_tests {
    use super::{gate_scratch_root, gate_temp_root};
    use std::path::{Path, PathBuf};

    fn manifest_dir() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    /// The workspace root: `<manifest>/../../` (`crates/codegraph` →
    /// `crates` → workspace).
    fn workspace_root() -> PathBuf {
        manifest_dir()
            .parent()
            .and_then(Path::parent)
            .expect("the crate lives two levels below the workspace root")
            .to_path_buf()
    }

    #[test]
    fn cargo_target_tmpdir_wins_when_provided() {
        let provided = "/somewhere/else/target/tmp";
        assert_eq!(
            gate_temp_root(Some(provided), manifest_dir()),
            PathBuf::from(provided),
            "an explicit CARGO_TARGET_TMPDIR must be used verbatim"
        );
    }

    #[test]
    fn fallback_is_the_workspace_target_gate_tmp() {
        assert_eq!(
            gate_temp_root(None, manifest_dir()),
            workspace_root().join("target").join("gate-tmp"),
            "the fallback must be <manifest>/../../target/gate-tmp"
        );
    }

    /// Issue #683a: `crates/codegraph/target/**` is TRACKED in git — nothing
    /// the gate writes may land there, on either resolution path.
    #[test]
    fn resolved_roots_never_land_in_the_tracked_crate_target_dir() {
        let cargo_tmpdir = workspace_root().join("target").join("tmp");
        let tracked = manifest_dir().join("target");
        for resolved in [
            gate_temp_root(Some(cargo_tmpdir.to_str().unwrap()), manifest_dir()),
            gate_temp_root(None, manifest_dir()),
        ] {
            assert!(
                !resolved.starts_with(&tracked),
                "{} must never resolve under the TRACKED crate target dir {} (issue #683a)",
                resolved.display(),
                tracked.display()
            );
        }
    }

    /// Issue #734: the whole point — the scratch root is disk-backed, never
    /// the (tmpfs) system temp dir. Mutating the resolver to prefer
    /// `std::env::temp_dir()` must fail here.
    ///
    /// Skipped when the WORKSPACE itself lives under the temp dir (a checkout
    /// in `/tmp`, or the common `TMPDIR=$HOME` habit): then the disk-backed
    /// root is legitimately under `std::env::temp_dir()` and this pin has
    /// nothing to say — asserting anyway would build a false red of exactly
    /// the class #734 removes. The hazard the pin exists for (a resolver
    /// picking `env::temp_dir()` while the workspace is elsewhere) stays red.
    #[test]
    fn resolved_roots_are_not_the_system_temp_dir() {
        let sys_tmp = std::env::temp_dir();
        if workspace_root().starts_with(&sys_tmp) {
            eprintln!(
                "skip: workspace {} is under the system temp dir {}",
                workspace_root().display(),
                sys_tmp.display()
            );
            return;
        }
        let cargo_tmpdir = workspace_root().join("target").join("tmp");
        for resolved in [
            gate_temp_root(Some(cargo_tmpdir.to_str().unwrap()), manifest_dir()),
            gate_temp_root(None, manifest_dir()),
        ] {
            assert!(
                !resolved.starts_with(&sys_tmp),
                "{} is under the system temp dir {} — the compile gate must stay disk-backed (issue #734)",
                resolved.display(),
                sys_tmp.display()
            );
        }
    }

    /// Issue #734 (review): the pins above feed SYNTHETIC inputs, so they
    /// cannot see the value the gate actually uses. `gate_scratch_root()`
    /// consumes `CARGO_TARGET_TMPDIR` at compile time — with
    /// `CARGO_TARGET_DIR` pointed at tmpfs, the gate would put the same
    /// multi-GB trees back on `/tmp` while every synthetic pin stayed green
    /// (and the docs/justfile claim "disk-backed"). This pin inspects the
    /// EFFECTIVE root (the wrapper's own return value, asserted to be what
    /// cargo's value resolves to), so a wrapper that ignores the input and
    /// hands back `env::temp_dir()` fails here too.
    ///
    /// Guarded like [`resolved_roots_are_not_the_system_temp_dir`]: a
    /// workspace under the temp dir has a legitimately temp-dir root, so
    /// there is nothing to assert.
    #[test]
    fn the_effective_root_is_not_the_temp_dir() {
        let sys_tmp = std::env::temp_dir();
        if workspace_root().starts_with(&sys_tmp) {
            eprintln!(
                "skip: workspace {} is under the system temp dir {}",
                workspace_root().display(),
                sys_tmp.display()
            );
            return;
        }
        let root = gate_scratch_root();
        assert!(
            !root.starts_with(&sys_tmp),
            "the gate's effective scratch root {} is under the system temp dir {} \
             (a CARGO_TARGET_DIR on tmpfs?) — the compile gate must stay disk-backed (issue #734)",
            root.display(),
            sys_tmp.display()
        );
        if let Some(dir) = option_env!("CARGO_TARGET_TMPDIR") {
            assert_eq!(
                root,
                gate_temp_root(Some(dir), manifest_dir()),
                "the gate must use cargo's CARGO_TARGET_TMPDIR ({dir}) verbatim"
            );
        }
    }
}
