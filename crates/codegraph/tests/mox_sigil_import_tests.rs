//! `import sigil` in a `.mox` model (issue #608).
//!
//! A `.mox` file may import rosetta (Rune DSL) sources —
//! `import sigil "cdm/math.rosetta"` — and refer to the types they define.
//! Before #608 the mox pass compiled with an EMPTY `SigilImports`, so every
//! lowered-type reference was an `unknown type` and the whole model failed
//! (`--rosetta-files` is a separate model source, not import satisfaction).
//! Now the mox ingest satisfies the imports from disk (relative to the
//! `.mox` file, plus the sibling `*.rosetta` pool), the lowered classes
//! bridge into the graph, and doctor compiles the same workspace.

use std::fs;
use std::path::{Path, PathBuf};

use codegraph::init::commands::{DoctorArgs, cmd_doctor};
use tempfile::TempDir;

const TRADING_MOX: &str = concat!(
    "package trading\n",
    "\n",
    "import sigil \"cdm/math.rosetta\"\n",
    "\n",
    "class TradeType {\n",
    "    String reference\n",
    "    refers Quantity[0..1] cdmQuantity\n",
    "}\n",
);

/// The named import; its `Unit` comes from a namespace only the SIBLING
/// file declares — a clean compile proves the candidate pool was supplied.
const MATH_ROSETTA: &str = concat!(
    "namespace cdm.base.math\n",
    "version \"1.0.0\"\n",
    "\n",
    "import cdm.base.units.*\n",
    "\n",
    "type Quantity:\n",
    "    amount number (1..1)\n",
    "    unit Unit (0..1)\n",
);

const UNITS_ROSETTA: &str = concat!(
    "namespace cdm.base.units\n",
    "version \"1.0.0\"\n",
    "\n",
    "type Unit:\n",
    "    code string (1..1)\n",
);

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.trading]
label = "Trading"
schema_dir = "trading"
postgres_schema = "trading"
"#;

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("model/cdm")).unwrap();
        fs::write(root.join("model/trading.mox"), TRADING_MOX).unwrap();
        fs::write(root.join("model/cdm/math.rosetta"), MATH_ROSETTA).unwrap();
        fs::write(root.join("model/cdm/units.rosetta"), UNITS_ROSETTA).unwrap();
        fs::write(root.join("domains.toml"), DOMAINS_TOML).unwrap();
        Self { _dir: dir, root }
    }

    fn mox(&self) -> PathBuf {
        self.root.join("model/trading.mox")
    }

    fn rosetta_files(&self) -> Vec<PathBuf> {
        vec![
            self.root.join("model/cdm/math.rosetta"),
            self.root.join("model/cdm/units.rosetta"),
        ]
    }

    async fn run(&self, rosetta_files: &[PathBuf]) -> Result<PathBuf, String> {
        let output = self.root.join("out");
        let mox = vec![self.mox()];
        codegraph::driver::run(codegraph::driver::RunArgs {
            integration_manifests: &[],
            schemas: None,
            classifier: None,
            config_path: &self.root.join("domains.toml"),
            output: &output,
            extension_points_path: None,
            profile_name: "default",
            variant: None,
            profiles_config_path: None,
            no_post_gen: true,
            template_dir: &[],
            ifml_files: &[],
            openapi_files: &[],
            mox_files: &mox,
            rosetta_files,
            ddd_files: &[],
            evt_files: &[],
            deploy_files: &[],
            ifml_framework: &[],
            ifml_components: None,
            ifml_design_system: None,
            codegraph_rev: None,
            check: false,
            ux_rules: None,
        })
        .await
        .map_err(|e| e.to_string())?;
        Ok(output)
    }
}

fn migrations(output: &Path) -> String {
    let mut ddl = String::new();
    let mut files: Vec<PathBuf> = fs::read_dir(output.join("migrations"))
        .expect("migrations dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .collect();
    files.sort();
    for file in files {
        ddl.push_str(&fs::read_to_string(file).unwrap());
        ddl.push('\n');
    }
    ddl
}

/// The lowered rosetta types ingest beside the mox class: `Quantity` (a
/// `refers` target, so an entity) gets its table in the IMPORTING model's
/// domain (`cdm.base.math` names no domain) and the trade references it.
#[tokio::test]
async fn sigil_imported_types_bridge_into_the_importing_domain() {
    let fx = Fixture::new();
    let output = fx
        .run(&[])
        .await
        .expect("a .mox model importing sigil must compile and generate (issue #608)");
    let ddl = migrations(&output);
    assert!(
        ddl.contains("CREATE TABLE IF NOT EXISTS trading.quantity"),
        "the sigil-lowered Quantity must land in the trading domain:\n{ddl}"
    );
    assert!(
        ddl.contains("REFERENCES trading.quantity(id)"),
        "the trade's refers Quantity must FK the lowered table:\n{ddl}"
    );
}

/// Passing the same rosetta files as a second model source stays valid: the
/// rosetta pass never overrides a node the mox pass bridged (mox wins).
#[tokio::test]
async fn sigil_imports_and_rosetta_files_ingest_each_type_once() {
    let fx = Fixture::new();
    let output = fx
        .run(&fx.rosetta_files())
        .await
        .expect("mox + the same --rosetta-files must generate");
    let ddl = migrations(&output);
    assert_eq!(
        ddl.matches("CREATE TABLE IF NOT EXISTS trading.quantity (")
            .count(),
        1,
        "exactly one quantity table:\n{ddl}"
    );
}

/// Doctor compiles the same workspace the run path does.
#[test]
fn doctor_satisfies_sigil_imports_like_the_run_path() {
    let fx = Fixture::new();
    let summary = cmd_doctor(&DoctorArgs {
        config: fx.root.join("domains.toml"),
        schemas: None,
        classifier: None,
        profiles_config: None,
        mox_files: vec![fx.mox()],
        rosetta_files: vec![],
        ddd_files: vec![],
        evt_files: vec![],
        deploy_files: vec![],
        ifml_files: vec![],
        extension_points: None,
        fail_on_findings: false,
        graph_artifact: None,
        emit_graph: None,
        graph_reachability: vec![],
    })
    .unwrap();
    assert_eq!(
        summary.hard_failures, 0,
        "doctor must compile the sigil import"
    );
}

/// A missing import target stays a named compile failure (the driver's
/// unsatisfied-import diagnostic), never a silent empty model.
#[tokio::test]
async fn a_missing_sigil_import_is_a_named_failure() {
    let fx = Fixture::new();
    fs::remove_file(fx.root.join("model/cdm/math.rosetta")).unwrap();
    let err = fx.run(&[]).await.expect_err("a missing import must fail");
    assert!(err.contains("math.rosetta"), "{err}");
}

/// Doctor makes the missing named import the same hard failure.
#[test]
fn doctor_fails_on_a_missing_sigil_import() {
    let fx = Fixture::new();
    fs::remove_file(fx.root.join("model/cdm/math.rosetta")).unwrap();
    let summary = cmd_doctor(&DoctorArgs {
        config: fx.root.join("domains.toml"),
        schemas: None,
        classifier: None,
        profiles_config: None,
        mox_files: vec![fx.mox()],
        rosetta_files: vec![],
        ddd_files: vec![],
        evt_files: vec![],
        deploy_files: vec![],
        ifml_files: vec![],
        extension_points: None,
        fail_on_findings: false,
        graph_artifact: None,
        emit_graph: None,
        graph_reachability: vec![],
    });
    // Doctor surfaces hard failures as an error (non-zero exit).
    assert!(
        summary.map_or(true, |s| s.hard_failures > 0),
        "a missing sigil import must fail doctor"
    );
}
