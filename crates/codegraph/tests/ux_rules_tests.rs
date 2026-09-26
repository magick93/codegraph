//! Driver-level content pins for the ux-rules list page (issue #297).
//!
//! `ux_rules = true` is ON in the default profile, so the mox-first
//! pipeline renders ux markup into every generated list page. These tests
//! pin the #297 contract over a rich fixture model (money, quantity,
//! codelist status, timestamps, identifier):
//!
//! - chip Badge with tone variant (`-chip` testid)
//! - `text-right tabular-nums` on numeric/money header + cell
//! - `Intl.NumberFormat` currency formatting
//! - copy-chip button (`-copy` testid)
//! - row-actions dropdown (`-actions` / `-actions-menu` testids)
//! - Delete behind an AlertDialog confirm
//! - Tooltip wrapper for truncated cells
//! - zebra row shading + the `data-inactive` branch
//! - readable-first column order
//!
//! Flag OFF (the byte-identity canary's shape) renders none of these.
//! Fixture pattern: the `init_tests` mox-first lifecycle (cmd_init →
//! `driver::run` with ONLY `--mox-files` + config), no node, no DB.

use std::fs;
use std::path::{Path, PathBuf};

use codegraph::init::commands::{cmd_init, InitArgs};
use tempfile::TempDir;

/// Absolute repo root (`<repo>/crates/codegraph` → two parents up).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("codegraph crate lives at <repo>/crates/codegraph")
        .to_path_buf()
}

fn init_args(dir: &Path) -> InitArgs {
    InitArgs {
        name: Some("demo-app".to_string()),
        output_dir: dir.to_path_buf(),
        domains: vec!["common".to_string()],
        database_target: "postgres".to_string(),
        persistence_provider: "sea_orm".to_string(),
        deployment_topology: "monolith".to_string(),
        grpc: false,
        ifml: false,
        ops: true,
        rosetta: false,
        rev: Some("abc123".to_string()),
        codegraph_path: Some(repo_root()),
        force: false,
        template_dirs: vec![],
    }
}

/// A rich entity exercising every dimension the list template branches on.
const TASK_MODEL: &str = r#"package common

/// Task lifecycle status.
enum TaskStatus {
    Draft as "Draft" = 0
    Active as "Active" = 1
}

/// Primary identifier.
type UUID wraps String {
    format "uuid"
}

/// ISO date.
type Date wraps String {
    format "date"
}

/// UTC timestamp.
type Timestamp wraps String {
    format "date-time"
}

/// A trackable task.
class TaskType {
    /// Primary identifier.
    UUID id
    /// Task name.
    String name
    /// Optional longer description.
    String [0..1] description
    /// Units of work.
    Long quantity
    /// Billed amount.
    Double [0..1] total_amount
    /// When the task is due.
    Date [0..1] due_date
    /// Creation stamp.
    Timestamp [0..1] created_at
    /// Lifecycle status.
    TaskStatus status
}
"#;

/// Scaffold the fixture and replace the starter model with the rich task
/// model. Returns (project dir, mox file path).
fn fixture(dir: &TempDir) -> (PathBuf, PathBuf) {
    cmd_init(&init_args(dir.path())).unwrap();
    let project = dir.path().join("demo-app");
    let mox = project.join("model/common.mox");
    fs::write(&mox, TASK_MODEL).unwrap();
    (project, mox)
}

/// Fixture paths bound to locals so `RunArgs` can borrow them across the
/// driver await.
struct FixtureRun {
    mox_files: Vec<PathBuf>,
    config: PathBuf,
    output: PathBuf,
    profiles: PathBuf,
}

impl FixtureRun {
    fn args<'a>(&'a self, ux_rules: Option<&'a Path>) -> codegraph::driver::RunArgs<'a> {
        codegraph::driver::RunArgs {
            schemas: None,
            classifier: None,
            config_path: &self.config,
            output: &self.output,
            extension_points_path: None,
            profile_name: "default",
            variant: None,
            profiles_config_path: Some(self.profiles.clone()),
            no_post_gen: true,
            template_dir: &[],
            ifml_files: &[],
            openapi_files: &[],
            mox_files: &self.mox_files,
            rosetta_files: &[],
            ifml_framework: &[],
            ifml_components: None,
            ifml_design_system: None,
            ux_rules,
            codegraph_rev: None,
        }
    }

    fn run(&self, ux_rules: Option<&Path>) {
        tokio::runtime::Runtime::new()
            .expect("tokio runtime")
            .block_on(async { codegraph::driver::run(self.args(ux_rules)).await.unwrap() });
    }
}

/// Every generated list page under `generated/ui`.
fn list_pages(project: &Path) -> Vec<PathBuf> {
    let ui = project.join("generated/ui/src/routes");
    walkdir::WalkDir::new(&ui)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .filter(|p| p.file_name().and_then(|n| n.to_str()) == Some("+page.svelte"))
        .collect()
}

/// The default profile (flag ON) renders the full ux contract into the
/// task list page.
#[test]
fn ux_rules_default_profile_renders_list_page_pins() {
    let dir = TempDir::new().unwrap();
    let (project, mox) = fixture(&dir);
    let run = FixtureRun {
        config: project.join("domains.toml"),
        output: project.join("generated"),
        profiles: project.join("profiles.toml"),
        mox_files: vec![mox],
    };
    run.run(None);

    let pages = list_pages(&project);
    assert!(
        !pages.is_empty(),
        "flag-on generation must produce list pages"
    );
    let task_page = pages
        .iter()
        .find(|p| p.to_string_lossy().ends_with("/common/task/+page.svelte"))
        .unwrap_or_else(|| panic!("the task list page must be generated, pages: {pages:?}"));
    let page = fs::read_to_string(task_page).unwrap();

    // 1. Chips: Badge with the tone-mapped variant + stable testid.
    assert!(page.contains(r#"data-testid="task-chip""#), "{page}");
    assert!(page.contains("variant={toneFor(col, value)}"), "{page}");

    // 2. Numeric/money alignment on header AND cells.
    assert_eq!(
        page.matches("text-right tabular-nums").count(),
        2,
        "one header + one cell occurrence:\n{page}"
    );

    // 3. Locale/currency formatting via Intl.
    assert!(page.contains("Intl.NumberFormat"), "{page}");
    assert!(page.contains("currency: 'NZD'"), "{page}");
    assert!(
        page.contains("Intl.DateTimeFormat('en-NZ', { dateStyle: 'medium', timeStyle: 'short' })"),
        "{page}"
    );

    // 4. Copy chips.
    assert!(page.contains(r#"data-testid="task-copy""#), "{page}");

    // 5. Row actions dropdown.
    assert!(page.contains(r#"data-testid="task-actions""#), "{page}");
    assert!(
        page.contains(r#"data-testid="task-actions-menu""#),
        "{page}"
    );

    // 6. Delete behind the AlertDialog confirm.
    assert!(page.contains("AlertDialog"), "{page}");
    assert!(
        page.contains(r#"data-testid="task-delete-confirm""#),
        "{page}"
    );
    assert!(page.contains("requestDelete"), "{page}");
    assert!(page.contains("apiDelete"), "{page}");
    assert!(page.contains("invalidateAll"), "{page}");

    // 7. Tooltip wrapper on truncated cells.
    assert!(page.contains("Tooltip.Root"), "{page}");
    assert!(page.contains("truncate(value, 8)"), "{page}");

    // 8. Zebra rows + the inactive-row branch (auditable entities carry
    //    the deleted_at marker by default).
    assert!(page.contains("bg-muted/50"), "{page}");
    assert!(page.contains("data-inactive="), "{page}");
    assert!(
        page.contains("if (row['deleted_at'] != null) return true;"),
        "{page}"
    );

    // 9. Readable-first column order: `name` leads, `id` trails,
    //    `created_at` is last.
    let name_pos = page.find("key: 'name'").expect("name column");
    let id_pos = page.find("key: 'id'").expect("id column");
    let created_pos = page.find("key: 'created_at'").expect("created_at column");
    assert!(name_pos < id_pos, "name must lead:\n{page}");
    assert!(id_pos < created_pos, "audit stamp must trail:\n{page}");

    // Pre-existing behaviors stay intact.
    assert!(page.contains(r#"data-testid="task-table""#), "{page}");
    assert!(page.contains("paraglide/messages.js"), "{page}");
    assert!(page.contains("m.common_"), "{page}");
    assert!(page.contains("handleRowClick"), "{page}");
    assert!(page.contains("onkeydown"), "{page}");
    assert!(page.contains("focus-visible"), "{page}");
    assert!(page.contains(r#"data-testid="task-create-btn""#), "{page}");
    assert!(page.contains(r#"data-testid="task-search""#), "{page}");
    assert!(page.contains(r#"data-testid="task-empty""#), "{page}");
    assert!(page.contains(r#"data-testid="task-pagination""#), "{page}");
}

/// Flag OFF: none of the ux markup renders (the content-level counterpart
/// of the byte-identity canary).
#[test]
fn ux_rules_flag_off_renders_no_ux_markup() {
    let dir = TempDir::new().unwrap();
    let (project, mox) = fixture(&dir);

    let profiles_path = project.join("profiles.toml");
    let profiles = fs::read_to_string(&profiles_path).unwrap();
    let flag_off = profiles.replace("ux_rules = true", "ux_rules = false");
    assert_ne!(profiles, flag_off, "scaffold must ship the ux_rules flag");
    fs::write(&profiles_path, flag_off).unwrap();

    let run = FixtureRun {
        config: project.join("domains.toml"),
        output: project.join("generated"),
        profiles: profiles_path,
        mox_files: vec![mox],
    };
    run.run(None);

    let pages = list_pages(&project);
    assert!(!pages.is_empty(), "flag-off generation still emits pages");
    // Only ENTITY LIST pages are ux-rendered; scaffold-authored pages and
    // detail pages legitimately carry AlertDialog/dropdowns already.
    let list_pages_only: Vec<&PathBuf> = pages
        .iter()
        .filter(|p| !p.to_string_lossy().contains("/["))
        .collect();
    for page_path in list_pages_only {
        let page = fs::read_to_string(page_path).unwrap();
        for needle in [
            "-chip",
            "-actions",
            "-copy",
            "Intl.NumberFormat",
            "data-inactive",
            "Tooltip",
            "DropdownMenu",
            "AlertDialog",
            "invalidateAll",
            "tabular-nums",
            "rowClass",
        ] {
            assert!(
                !page.contains(needle),
                "flag-off page {} must not contain {needle:?}:\n{page}",
                page_path.display()
            );
        }
    }
}

/// `--ux-rules` merges project rules ahead of the pack: a project rule
/// pinning the money column's display is honored in the rendered page.
#[test]
fn ux_rules_project_file_overrides_pack_display() {
    let dir = TempDir::new().unwrap();
    let (project, mox) = fixture(&dir);

    let ux_rules_path = project.join("ux-rules.toml");
    fs::write(
        &ux_rules_path,
        "[[column]]\nname_pattern = \"total_amount\"\ndisplay = \"chip\"\n\n[actions]\ninline_max = 2\n",
    )
    .unwrap();

    let run = FixtureRun {
        config: project.join("domains.toml"),
        output: project.join("generated"),
        profiles: project.join("profiles.toml"),
        mox_files: vec![mox],
    };
    run.run(Some(ux_rules_path.as_path()));

    let pages = list_pages(&project);
    let task_page = pages
        .iter()
        .find(|p| p.to_string_lossy().ends_with("/common/task/+page.svelte"))
        .expect("task list page");
    let page = fs::read_to_string(task_page).unwrap();

    // The money column renders as a chip (project rule beats the pack's
    // copy-chip/raw tiering)...
    let amount_pos = page
        .find("key: 'total_amount'")
        .expect("total_amount column");
    let amount_slice = &page[amount_pos..];
    let line_end = amount_slice.find('\n').unwrap_or(amount_slice.len());
    assert!(
        amount_slice[..line_end].contains("display: 'chip'"),
        "project rule pins the money column to a chip:\n{}",
        &amount_slice[..line_end]
    );
    // ...and the pack's delete-confirmation default survives the merge
    // (the project file replaced only inline_max). Note the action-tiering
    // itself is runtime state (Svelte branches), so only the confirm
    // dialog — generation-time markup — is content-pinned here.
    assert!(
        page.contains(r#"data-testid="task-action-delete""#),
        "{page}"
    );
    assert!(
        page.contains(r#"data-testid="task-delete-confirm""#),
        "{page}"
    );
}
