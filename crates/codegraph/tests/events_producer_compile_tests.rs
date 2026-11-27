//! Compile pin for the events-producer publishing plane (issue #820 slice b,
//! the #551 policy): the generated app over a publishing fixture must
//! `cargo check` clean — the outgoing-event enum, the transactional
//! `update_with_events` seam, the pre/post re-read, the typed payload
//! construction, and the `publish_*_in_tx` call must compile together with
//! the rest of the app.
//!
//! Path deps (`dependency_strategy = "path"`, issue #347) keep the gate
//! network-free. Ignored: the full dep stack is slow.
//!
//! ```text
//! cargo test -p codegraph --test events_producer_compile_tests -- --ignored --nocapture
//! ```

use std::fs;
use std::path::{Path, PathBuf};

const PAYROLL_MOX: &str = r#"
package nz.example.payroll

type Instant wraps String {
    format "date-time"
}

enum PayRunStatus {
    Draft as "Draft" = 0
    Posted as "Posted" = 1
}

class PayRunType {
    int total
    PayRunStatus payRunStatus
}
"#;

const PAYROLL_DDD: &str = r#"import "payroll.mox";

application Payroll {
    base nz.example.payroll

    module payroll {
        entity PayRunType scaffold repository PayRunRepository {
            findById;
            findAll;
            save;
            delete;
        }

        machine payRunStatus: PayRunStatus {
            initial Draft;
            Draft -> Posted via Approve;
            terminal Posted;
        }

        service PayRunService {
            inject PayRunRepository;
            approve => PayRunRepository.save emits PayRunPosted transition Approve;
        }
    }
}
"#;

const PAYROLL_EVT: &str = r#"import "payroll.mox"

event PayRunPosted version "1.0.0" {
    payRunId: String;
    total: int;
    payRunStatus: PayRunStatus;
    previousState: PayRunStatus;
    nextState: PayRunStatus;
    occurredOn: Instant;
    correlationId: String;
    organizationId: String;
}

channel payroll {
    publishes PayRunPosted;
}
"#;

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
"#;

/// The workspace default profile with path deps, gRPC stripped, and the
/// producer flag ON.
fn write_profiles(dir: &Path) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let type_contracts = workspace_root.join("crates/codegraph-type-contracts");
    let mut text =
        fs::read_to_string(workspace_root.join("profiles.toml")).expect("workspace profiles");
    text = text.replace(
        "[profiles.default.meta]\n",
        &format!(
            "[profiles.default.meta]\n\
             domain_types_base = \"crates/domain-types\"\n\
             type_contracts_base = \"{}\"\n",
            type_contracts.display()
        ),
    );
    text = text.replace(
        "[profiles.default.features]\n",
        "[profiles.default.features]\ndependency_strategy = \"path\"\n",
    );
    text = text.replace("events_producer = false", "events_producer = true");
    let body: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("\"grpc_"))
        .collect();
    let path = dir.join("profiles.toml");
    fs::write(&path, body.join("\n")).unwrap();
    path
}

#[tokio::test]
#[ignore = "slow: cargo check over the generated app; needs the full dep stack"]
async fn generated_app_with_events_producer_compiles() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("payroll.mox"), PAYROLL_MOX).unwrap();
    fs::write(dir.path().join("payroll.ddd"), PAYROLL_DDD).unwrap();
    fs::write(dir.path().join("payroll.evt"), PAYROLL_EVT).unwrap();
    fs::write(dir.path().join("domains.toml"), DOMAINS_TOML).unwrap();
    let profiles = write_profiles(dir.path());

    let output = dir.path().join("app");
    fs::create_dir_all(&output).unwrap();
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &dir.path().join("domains.toml"),
        output: &output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &[dir.path().join("payroll.mox")],
        rosetta_files: &[],
        ddd_files: &[dir.path().join("payroll.ddd")],
        evt_files: &[dir.path().join("payroll.evt")],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("generation over the publishing fixture must succeed");

    let command = fs::read_to_string(output.join("src/domain/payroll/pay_run/command.rs")).unwrap();
    assert!(
        command.contains("pub enum PayRunOutgoingEvent"),
        "{command}"
    );
    assert!(
        command.contains("pub async fn update_with_events("),
        "{command}"
    );
    assert!(
        command.contains("publish_payroll_in_tx("),
        "the channel publish must emit: {command}"
    );
    assert!(
        command.contains("let publish_before = if !outgoing.is_empty()"),
        "the pre-read must emit: {command}"
    );
    assert!(
        command.contains("previous_state: publish_before.pay_run_status.clone()"),
        "the previous-state context must construct: {command}"
    );
    let handler = fs::read_to_string(output.join("src/api/payroll/pay_run_handler.rs")).unwrap();
    assert!(handler.contains("update_with_events("), "{handler}");
    assert!(
        handler.contains("PayRunOutgoingEvent::PayRunPosted"),
        "{handler}"
    );
    assert!(handler.contains("fn extract_traceparent("), "{handler}");
    let emit = fs::read_to_string(output.join("src/events/emit.rs")).unwrap();
    assert!(
        emit.contains("pub async fn publish_payroll_in_tx("),
        "{emit}"
    );

    codegraph::driver::check_generated_output(&output)
        .expect("the generated app with events publishing must compile");
}
