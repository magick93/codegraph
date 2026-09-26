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
    /// Child subtasks managed from the task detail page.
    refers SubTaskType[] sub_tasks
}

/// A subtask under a task (child section on the task detail page).
class SubTaskType {
    /// Primary identifier.
    UUID id
    /// Subtask title.
    String title
    /// Creation stamp.
    Timestamp [0..1] created_at
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
            "-timeline",
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

/// An explicit `[[collection]] display = "timeline"` rule renders the
/// timeline layout (issue #298): vertical rail entries with the order
/// field formatted through the shared Intl helper, DESC sort, title link,
/// preview fields through the SHARED cell formatter (chip + money), the
/// row-actions dropdown, the workflow badge, and NO table markup. The
/// shared load logic (search, create, empty state, pagination) survives.
#[test]
fn ux_rules_timeline_rule_renders_timeline_layout() {
    let dir = TempDir::new().unwrap();
    let (project, mox) = fixture(&dir);

    // Workflow on Task so the timeline badge branch renders.
    let domains_path = project.join("domains.toml");
    let mut domains = fs::read_to_string(&domains_path).unwrap();
    domains.push_str(
        "\n[domains.common.entity_config.TaskType.workflow]\n\
         status_field = \"status\"\n\
         initial_state = \"draft\"\n\
         states = [\"draft\", \"active\", \"archived\"]\n\
         terminal_states = [\"archived\"]\n\
         generate_action_endpoints = true\n",
    );
    fs::write(&domains_path, domains).unwrap();

    let ux_rules_path = project.join("ux-rules.toml");
    fs::write(
        &ux_rules_path,
        "[[collection]]\n\
         entity_pattern = \"Task*\"\n\
         display = \"timeline\"\n\
         order_by = \"created_at\"\n\
         title_field = \"name\"\n\
         preview = [\"status\", \"total_amount\"]\n",
    )
    .unwrap();

    let run = FixtureRun {
        config: domains_path,
        output: project.join("generated"),
        profiles: project.join("profiles.toml"),
        mox_files: vec![mox],
    };
    run.run(Some(ux_rules_path.as_path()));

    let pages = list_pages(&project);
    let task_page = pages
        .iter()
        .find(|p| p.to_string_lossy().ends_with("/common/task/+page.svelte"))
        .unwrap_or_else(|| panic!("task list page must be generated, pages: {pages:?}"));
    let page = fs::read_to_string(task_page).unwrap();

    // 1. Timeline root + per-entry testids (ids::TIMELINE / TIMELINE_ITEM).
    assert!(page.contains(r#"data-testid="task-timeline""#), "{page}");
    assert!(
        page.contains(r#"data-testid="task-timeline-item""#),
        "{page}"
    );

    // 2. The order field renders through the shared Intl helper.
    assert!(page.contains("function formatDate("), "{page}");
    assert!(
        page.contains("Intl.DateTimeFormat('en-NZ', { dateStyle: 'medium' })"),
        "{page}"
    );
    assert!(page.contains("const orderKey = 'created_at';"), "{page}");
    assert!(
        page.contains("{formatDate((row as Record<string, unknown>)[orderKey])}"),
        "{page}"
    );

    // 3. DESC sort (newest first) over the shared rows.
    assert!(page.contains("[...displayRows].sort"), "{page}");
    assert!(
        page.contains("const av = String((a as Record<string, unknown>)['created_at'] ?? '');"),
        "{page}"
    );
    assert!(
        page.contains("return av < bv ? 1 : av > bv ? -1 : 0;"),
        "{page}"
    );

    // 4. Title link over the entity base path.
    assert!(page.contains("const titleKey = 'name';"), "{page}");
    assert!(page.contains("href={`${basePath}/${row['id']}`}"), "{page}");

    // 5. Preview fields render through the SHARED cell formatter — the
    //    same chip/money branches the table uses, no divergent code.
    assert!(
        page.contains("const previewKeys = ['status', 'total_amount'];"),
        "{page}"
    );
    assert!(page.contains("timeline-meta"), "{page}");
    assert!(page.contains(r#"data-testid="task-chip""#), "{page}");
    assert!(page.contains("variant={toneFor(col, value)}"), "{page}");
    assert!(page.contains("formatMoney(value)"), "{page}");

    // 6. Row-actions dropdown + AlertDialog delete confirm (same pattern
    //    as the table rows).
    assert!(page.contains(r#"data-testid="task-actions""#), "{page}");
    assert!(
        page.contains(r#"data-testid="task-actions-menu""#),
        "{page}"
    );
    assert!(
        page.contains(r#"data-testid="task-action-delete""#),
        "{page}"
    );
    assert!(
        page.contains(r#"data-testid="task-delete-confirm""#),
        "{page}"
    );

    // 7. Workflow badge branch compiles in (WorkflowPanel variant ladder).
    assert!(page.contains("function workflowVariant("), "{page}");
    assert!(
        page.contains(r#"data-testid="task-workflow-state""#),
        "{page}"
    );

    // 8. Table markup absent in timeline mode.
    assert!(!page.contains(r#"data-testid="task-table""#), "{page}");
    assert!(!page.contains("<Table.Root"), "{page}");
    assert!(!page.contains("Table.Head"), "{page}");

    // 9. Shared load logic intact: search, create, empty state, no-results
    //    and pagination all keep their testids.
    for testid in [
        "task-search",
        "task-create-btn",
        "task-empty",
        "task-no-results",
        "task-pagination",
    ] {
        assert!(
            page.contains(&format!(r#"data-testid="{testid}""#)),
            "missing {testid}:\n{page}"
        );
    }
    assert!(page.contains("const displayRows = $derived("), "{page}");
    assert!(page.contains("handlePageChange"), "{page}");
}

/// An explicit `display = "table"` collection rule is a deliberate opt-OUT:
/// the timeline never renders and the table stays byte-shaped.
#[test]
fn ux_rules_explicit_table_rule_never_renders_timeline() {
    let dir = TempDir::new().unwrap();
    let (project, mox) = fixture(&dir);

    let ux_rules_path = project.join("ux-rules.toml");
    fs::write(
        &ux_rules_path,
        "[[collection]]\nentity_pattern = \"Task*\"\ndisplay = \"table\"\n",
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

    assert!(page.contains(r#"data-testid="task-table""#), "{page}");
    for needle in [
        "task-timeline",
        "timeline-item",
        "timeline-meta",
        "formatDate(",
        "orderKey",
        "titleKey",
        "previewKeys",
        "displayRowsSorted",
        "timeline-rail",
    ] {
        assert!(
            !page.contains(needle),
            "table rule must not leak {needle:?}:\n{page}"
        );
    }
}

/// An unresolvable `order_by` is a generation error naming the candidate
/// time-point fields: the ui-page generator fails for the entity and the
/// timeline layout never reaches the output tree.
#[test]
fn ux_rules_timeline_unresolvable_order_by_skips_entity_page() {
    let dir = TempDir::new().unwrap();
    let (project, mox) = fixture(&dir);

    let ux_rules_path = project.join("ux-rules.toml");
    fs::write(
        &ux_rules_path,
        "[[collection]]\n\
         entity_pattern = \"Task*\"\n\
         display = \"timeline\"\n\
         order_by = \"quantity\"\n",
    )
    .unwrap();

    let run = FixtureRun {
        config: project.join("domains.toml"),
        output: project.join("generated"),
        profiles: project.join("profiles.toml"),
        mox_files: vec![mox],
    };
    // The driver records the ui-page failure in its report and completes;
    // the plan error (with the candidate fields) is printed to stderr.
    run.run(Some(ux_rules_path.as_path()));

    let task_page = project.join("generated/ui/src/routes/(app)/common/task/+page.svelte");
    assert!(
        !task_page.exists(),
        "unresolvable order_by must fail the ui-page generation: {}",
        task_page.display()
    );
}

/// Declare `SubTaskType` as a config child of `TaskType` in the fixture's
/// `domains.toml` (the `role = "child"` + `parent` contract
/// `collect_child_sections` scans for).
fn add_child_config(project: &Path) {
    let domains_path = project.join("domains.toml");
    let mut domains = fs::read_to_string(&domains_path).unwrap();
    domains.push_str(
        "\n[domains.common.entity_config.SubTaskType]\n\
         role = \"child\"\n\
         parent = \"TaskType\"\n",
    );
    fs::write(&domains_path, domains).unwrap();
}

/// Child sections on the detail page tier their actions behind the
/// parent's ux plan (issue #299): Edit/Delete collapse into a per-item
/// dropdown (`{child}-actions` / `{child}-actions-menu`), Delete sits
/// behind an AlertDialog confirm (`{child}-delete-confirm`), and the
/// Manage → link stays the inline primary affordance. The detail header
/// keeps its primary Edit/Delete buttons, and the scaffold ships the
/// primitive install surface (`ui/PRIMITIVES.md`).
#[test]
fn ux_rules_child_sections_tier_actions_behind_menu() {
    let dir = TempDir::new().unwrap();
    let (project, mox) = fixture(&dir);
    add_child_config(&project);
    let run = FixtureRun {
        config: project.join("domains.toml"),
        output: project.join("generated"),
        profiles: project.join("profiles.toml"),
        mox_files: vec![mox],
    };
    run.run(None);

    let detail_path =
        project.join("generated/ui/src/routes/(app)/common/task/[task_id]/+page.svelte");
    let page = fs::read_to_string(&detail_path).expect("task detail page must be generated");

    // 1. Per-item actions menu testids (child module name = `sub_task`).
    assert!(page.contains(r#"data-testid="sub_task-actions""#), "{page}");
    assert!(
        page.contains(r#"data-testid="sub_task-actions-menu""#),
        "{page}"
    );

    // 2. Edit/Delete live in the menu; the accordion + add affordances
    //    stay put. (The fixture child has no grandchildren, so the Manage
    //    → link correctly does not render here; the inline-affordance pin
    //    lives in the page.rs child-section template tests, which render
    //    the section with `has_children`.)
    assert!(
        page.contains(r#"data-testid="sub_task-action-edit""#),
        "{page}"
    );
    assert!(
        page.contains(r#"data-testid="sub_task-action-delete""#),
        "{page}"
    );
    assert!(
        page.contains(r#"data-testid="child-section-sub_task-add-btn""#),
        "{page}"
    );
    // The legacy flat buttons are gone.
    assert!(
        !page.contains(r#"onclick={() => deleteChild('common', 'sub-task', child.id)}"#),
        "{page}"
    );

    // 3. Delete behind the per-section AlertDialog confirm.
    assert!(
        page.contains(r#"data-testid="sub_task-delete-confirm""#),
        "{page}"
    );
    assert!(
        page.contains(r#"data-testid="sub_task-delete-confirm-confirm""#),
        "{page}"
    );
    assert!(
        page.contains("let sub_taskDeleteId = $state<string | null>(null);"),
        "{page}"
    );
    assert!(
        page.contains("function confirmDeleteSubTaskChild() {"),
        "{page}"
    );
    assert!(page.contains("DropdownMenu.Root"), "{page}");

    // 4. The detail header keeps its primary Edit/Delete buttons.
    assert!(page.contains(r#"data-testid="task-edit-btn""#), "{page}");
    assert!(page.contains(r#"data-testid="task-delete-btn""#), "{page}");
    assert!(
        page.contains(r#"onclick={() => deleteDialogOpen = true}"#),
        "{page}"
    );

    // 5. The scaffold ships the primitive install surface listing the
    //    ux-rules primitives.
    let primitives = fs::read_to_string(project.join("generated/ui/PRIMITIVES.md"))
        .expect("PRIMITIVES.md must be emitted under the ux plane");
    assert!(primitives.contains("npx shadcn-svelte"), "{primitives}");
    assert!(primitives.contains("dropdown-menu"), "{primitives}");
    assert!(primitives.contains("tooltip"), "{primitives}");
}

/// Flag OFF: child sections keep the pre-#299 flat buttons, no menu/confirm
/// markup leaks, and the scaffold's primitive surface is absent.
#[test]
fn ux_rules_flag_off_child_sections_stay_flat() {
    let dir = TempDir::new().unwrap();
    let (project, mox) = fixture(&dir);
    add_child_config(&project);

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

    let detail_path =
        project.join("generated/ui/src/routes/(app)/common/task/[task_id]/+page.svelte");
    let page = fs::read_to_string(&detail_path).expect("task detail page must be generated");
    for needle in [
        "DropdownMenu",
        "sub_task-actions",
        "sub_task-action-edit",
        "sub_task-action-delete",
        "sub_task-delete-confirm",
        "DeleteId",
        "confirmDelete",
    ] {
        assert!(
            !page.contains(needle),
            "flag-off detail page must not contain {needle:?}:\n{page}"
        );
    }
    // The flat legacy affordances stay byte-shaped. (No Manage → pin: the
    // fixture child has no grandchildren, so the link correctly does not
    // render; the page.rs template tests pin the inline affordance.)
    assert!(
        page.contains(r#"onclick={() => editChild('common', 'sub-task', child.id)}"#),
        "{page}"
    );
    assert!(
        page.contains(r#"onclick={() => deleteChild('common', 'sub-task', child.id)}"#),
        "{page}"
    );

    // No scaffold primitive surface under the flag.
    assert!(
        !project.join("generated/ui/PRIMITIVES.md").exists(),
        "PRIMITIVES.md must not be emitted with ux_rules off"
    );
}

/// The ux sort plane on generated list endpoints (issue #306): the
/// handler parses/validates `?sort=`/`?order=` against the plan's
/// allow-list (400 naming the valid fields), the query/repository layers
/// thread the spec into a quoted ORDER BY with a deterministic `id`
/// tiebreaker, and the list page renders `aria-sort` header buttons whose
/// state threads through pagination and FTS search.
#[test]
fn ux_rules_sort_plane_wires_handler_query_and_list_page() {
    let dir = TempDir::new().unwrap();
    let (project, mox) = fixture(&dir);
    let run = FixtureRun {
        config: project.join("domains.toml"),
        output: project.join("generated"),
        profiles: project.join("profiles.toml"),
        mox_files: vec![mox],
    };
    run.run(None);

    // ── API handler: param parsing + allow-list validation ──
    let handler = fs::read_to_string(project.join("generated/src/api/common/task_handler.rs"))
        .expect("task handler generated");
    assert!(
        handler.contains("const ALLOWED_SORT_FIELDS: &[&str] = &["),
        "{handler}"
    );
    // Scope to the allow-list block: the handler body legitimately
    // mentions other field names elsewhere (filters, params).
    let sort_block = handler
        .split("const ALLOWED_SORT_FIELDS")
        .nth(1)
        .and_then(|rest| rest.split("];").next())
        .unwrap_or_default();
    for field in [
        "name",
        "description",
        "quantity",
        "total_amount",
        "due_date",
        "created_at",
    ] {
        assert!(sort_block.contains(&format!("\"{field}\",")), "{handler}");
    }
    // Identifier/status/reference columns stay out of the allow-list.
    for field in ["id", "status", "sub_tasks"] {
        assert!(
            !sort_block.contains(&format!("\"{field}\",")),
            "allow-list must not contain {field:?}:\n{handler}"
        );
    }
    // serde defaults: absent params keep the default ordering.
    assert!(handler.contains("pub sort: Option<String>,"), "{handler}");
    assert!(handler.contains("pub order: Option<String>,"), "{handler}");
    // 400 texts name the valid sortable fields (include-path style).
    assert!(
        handler.contains("Unknown sort field: {sort}. Valid sortable fields: {}"),
        "{handler}"
    );
    assert!(
        handler.contains("order requires sort; valid sortable fields: {}"),
        "{handler}"
    );
    assert!(
        handler.contains("Invalid order: {other}. Valid values: asc, desc"),
        "{handler}"
    );
    // The validated spec rides the existing list-query path (RLS/context
    // bundle untouched). The fixture entity is auditable by default, so
    // the `include_deleted` slot (`false`) sits before the sort spec.
    assert!(
        handler.contains("list_filtered(params.page, params.page_size, &filters, false, ux_sort,"),
        "{handler}"
    );

    // ── Query + repository trait: signature threading ──
    let query = fs::read_to_string(project.join("generated/src/domain/common/task/query.rs"))
        .expect("task query generated");
    assert!(query.contains("sort: Option<(String, bool)>"), "{query}");
    assert!(
        query.contains("self.repo.list(&tx, page, page_size, filters, include_deleted, sort)"),
        "{query}"
    );
    let repo_trait =
        fs::read_to_string(project.join("generated/src/domain/common/task/repository.rs"))
            .expect("task repository trait generated");
    assert!(
        repo_trait.contains("sort: Option<(String, bool)>,"),
        "{repo_trait}"
    );

    // ── Repository impl: ORDER BY with quoted identifiers + tiebreaker ──
    let repo_impl =
        fs::read_to_string(project.join("generated/src/domain/common/task/repository_impl.rs"))
            .expect("task repository impl generated");
    assert!(
        repo_impl.contains("if let Some((sort_field, sort_desc)) = sort"),
        "{repo_impl}"
    );
    // Dialect-safe quoting: fully qualified sea_query aliases.
    assert!(
        repo_impl
            .contains("Alias::new(\"common\"), sea_orm::sea_query::Alias::new(\"task\"), sea_orm::sea_query::Alias::new(\"name\")"),
        "{repo_impl}"
    );
    // Deterministic `, id ASC` tiebreaker for stable pagination.
    assert!(
        repo_impl
            .contains("ordered.order_by_asc(sea_orm::sea_query::Expr::col((sea_orm::sea_query::Alias::new(\"common\"), sea_orm::sea_query::Alias::new(\"task\"), sea_orm::sea_query::Alias::new(\"id\"))))"),
        "{repo_impl}"
    );
    // Default (no params) keeps today's created_at DESC ordering.
    assert!(
        repo_impl.contains("query.order_by_desc(crate::entity::common_task::Column::CreatedAt)"),
        "{repo_impl}"
    );

    // ── List page: sortable header buttons + aria-sort + threading ──
    let page =
        fs::read_to_string(project.join("generated/ui/src/routes/(app)/common/task/+page.svelte"))
            .unwrap();
    assert!(
        page.contains("aria-sort={col.sortable ? sortStateFor(col.key) : undefined}"),
        "{page}"
    );
    assert!(
        page.contains(r#"data-testid="task-sort-{col.key}""#),
        "{page}"
    );
    assert!(
        page.contains("onclick={() => toggleSort(col.key)}"),
        "{page}"
    );
    assert!(page.contains("function sortIndicator("), "{page}");
    assert!(page.contains("'▲'"), "{page}");
    assert!(page.contains("'▼'"), "{page}");
    // Sort survives FTS search and pagination; sort changes reset the page.
    assert!(page.contains("function withSort("), "{page}");
    assert!(
        page.contains("if (searchQuery) params.set('q', searchQuery);"),
        "{page}"
    );
    // The load function forwards the params and returns the current state.
    let load = fs::read_to_string(
        project.join("generated/ui/src/routes/(app)/common/task/+page.server.ts"),
    )
    .unwrap();
    assert!(
        load.contains("apiUrl.searchParams.set('sort', sort);"),
        "{load}"
    );
    assert!(
        load.contains("apiUrl.searchParams.set('order', order === 'desc' ? 'desc' : 'asc');"),
        "{load}"
    );
    assert!(load.contains("sort: sort ?? null,"), "{load}");
}

/// Timeline collections keep their fixed DESC date order (issue #306 v1
/// scope): no sort buttons, no aria-sort, no sort plumbing in the load.
#[test]
fn ux_rules_sort_plane_stays_out_of_timeline_layout() {
    let dir = TempDir::new().unwrap();
    let (project, mox) = fixture(&dir);

    let ux_rules_path = project.join("ux-rules.toml");
    fs::write(
        &ux_rules_path,
        "[[collection]]\n\
         entity_pattern = \"Task*\"\n\
         display = \"timeline\"\n\
         order_by = \"created_at\"\n",
    )
    .unwrap();

    let run = FixtureRun {
        config: project.join("domains.toml"),
        output: project.join("generated"),
        profiles: project.join("profiles.toml"),
        mox_files: vec![mox],
    };
    run.run(Some(ux_rules_path.as_path()));

    let page =
        fs::read_to_string(project.join("generated/ui/src/routes/(app)/common/task/+page.svelte"))
            .expect("task timeline page");
    for needle in ["aria-sort", "toggleSort", "withSort", "-sort-"] {
        assert!(
            !page.contains(needle),
            "timeline page must not contain {needle:?}:\n{page}"
        );
    }
    let load = fs::read_to_string(
        project.join("generated/ui/src/routes/(app)/common/task/+page.server.ts"),
    )
    .unwrap();
    assert!(
        !load.contains("searchParams.set('sort'"),
        "timeline load must not forward sort:\n{load}"
    );
}

/// Flag OFF: no sort param handling anywhere — the handler/query/
/// repository surface stays byte-identical to pre-#306 output.
#[test]
fn ux_rules_flag_off_emits_no_sort_plane() {
    let dir = TempDir::new().unwrap();
    let (project, mox) = fixture(&dir);

    let profiles_path = project.join("profiles.toml");
    let profiles = fs::read_to_string(&profiles_path).unwrap();
    let flag_off = profiles.replace("ux_rules = true", "ux_rules = false");
    fs::write(&profiles_path, flag_off).unwrap();

    let run = FixtureRun {
        config: project.join("domains.toml"),
        output: project.join("generated"),
        profiles: profiles_path,
        mox_files: vec![mox],
    };
    run.run(None);

    let handler = fs::read_to_string(project.join("generated/src/api/common/task_handler.rs"))
        .expect("task handler generated");
    for needle in [
        "ALLOWED_SORT_FIELDS",
        "pub sort:",
        "pub order:",
        "Unknown sort field",
    ] {
        assert!(
            !handler.contains(needle),
            "flag-off handler must not contain {needle:?}:\n{handler}"
        );
    }
    assert!(
        handler
            .contains("list_filtered(params.page, params.page_size, &filters, false, api_key_info"),
        "pre-#306 call shape must stay:\n{handler}"
    );

    let repo_impl =
        fs::read_to_string(project.join("generated/src/domain/common/task/repository_impl.rs"))
            .expect("task repository impl generated");
    assert!(
        !repo_impl.contains("if let Some((sort_field, sort_desc)) = sort"),
        "{repo_impl}"
    );

    let page =
        fs::read_to_string(project.join("generated/ui/src/routes/(app)/common/task/+page.svelte"))
            .unwrap();
    for needle in ["aria-sort", "toggleSort", "withSort", "-sort-"] {
        assert!(
            !page.contains(needle),
            "flag-off page must not contain {needle:?}:\n{page}"
        );
    }
}
