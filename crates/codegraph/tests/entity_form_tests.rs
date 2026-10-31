//! Node-free pin for the entity-plane TanStack form layer (issue #580,
//! `entity_form`).
//!
//! The generated `{Entity}Form` component replaces the hand-rolled
//! `$state`-bound form with TanStack Form (`@tanstack/svelte-form`) +
//! zod: the zod schema is lowered IN RUST from the shared `forms::`
//! IR (`ZodObject`/`ZodField` — one zod vocabulary for both planes),
//! per-field errors come from TanStack field state meta, the submit
//! button is driven by `form.Subscribe`, and the page wrappers map the
//! API's 422 `VALIDATION_ERROR` `details: [{field, message}]` into
//! `fieldErrors` onto the matching fields.
//!
//! Pins (flag ON):
//!
//! - the form creator scaffold (`ui/src/lib/forms/`) — `form-context.ts`
//!   (`createFormCreatorContexts`), `form.ts` (`createFormCreator`), and
//!   one field component per control kind consuming `useFieldContext()`;
//! - the TanStack `{Entity}Form` — zod schema text per control kind,
//!   `AppField`s, `form.Subscribe` submit button, NO hand-rolled
//!   `$state` submitting/error machines, the POM testid contract
//!   (`{module}-form`/`-submit-btn`/`-cancel-btn`);
//! - junction array features render `mode="array"` machinery
//!   (`pushValue`/`removeValue`) and a `z.array(z.uuid())` schema arm;
//! - page wrappers wire `onsubmit` to the remote `createItem`/`updateItem`
//!   commands and map `fieldErrors`;
//! - the remote modules' fail legs parse `body.error.details` into
//!   `fieldErrors`;
//! - `package.json` carries the pinned `@tanstack/svelte-form` + `zod`.
//!
//! Flag off — none of the above; the hand-rolled form, store/remote
//! planes, and package.json stay byte-identical.
//!
//! ```text
//! cargo test -p codegraph --test entity_form_tests
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use codegraph::init::commands::{InitArgs, cmd_init};

/// The pinned dependency versions the emitted `package.json` must carry —
/// kept in lockstep with `codegraph_generate::forms::{TANSTACK_SVELTE_FORM_VERSION, ZOD_VERSION}`.
const TANSTACK_SVELTE_FORM_VERSION: &str = "^1.33.5";
const ZOD_VERSION: &str = "^4.6.7";

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

struct Fixture {
    output: PathBuf,
    mox: PathBuf,
    config: PathBuf,
    profiles: PathBuf,
}

/// Scaffold the init fixture and force the `entity_form` flag ON (the
/// scaffold seeds it) or OFF in the emitted profiles.toml. The emitted
/// starter model gains a junction array feature (`refers …[] items`) and a
/// codelist enum so the array machinery + `z.enum` arms have carriers.
fn fixture_with_flag(dir: &tempfile::TempDir, with_flag: bool) -> Fixture {
    let dir = dir.path();
    cmd_init(&init_args(dir)).unwrap();
    let project = dir.join("demo-app");

    let profiles_path = project.join("profiles.toml");
    let profiles = fs::read_to_string(&profiles_path).unwrap();
    if with_flag {
        assert!(
            profiles.contains("entity_form = true"),
            "scaffold profiles.toml must seed entity_form = true:\n{profiles}"
        );
    } else {
        let stripped = profiles.replace("entity_form = true", "entity_form = false");
        assert_ne!(
            profiles, stripped,
            "scaffold profiles.toml must carry the entity_form flag:\n{profiles}"
        );
        fs::write(&profiles_path, stripped).unwrap();
    }

    // Extend the starter model: a junction array + a codelist enum feature
    // on the list, and bound the item's refers to `[0..1]` so it projects
    // the stored `todo_list_id` FK (plain `refers` lowers to MANY in the
    // rex driver — a junction — which the list's own form exercises).
    let model_path = project.join("model/common.mox");
    let model = fs::read_to_string(&model_path).unwrap();
    let with_features = model
        .replace(
            "    /// The list this item belongs to.\n    refers CommonTodoListType todoList\n}",
            "    /// The list this item belongs to.\n    refers CommonTodoListType[0..1] todoList\n}",
        )
        .replace(
            "    /// Optional longer description.\n    String [0..1] description\n}",
            "    /// Optional longer description.\n    String [0..1] description\n\n    /// The items on this list.\n    refers CommonTodoItemType[] items\n\n    /// Lifecycle status.\n    CommonListStatus status\n}",
        )
        .replace(
            "package common\n",
            "package common\n\n/// Lifecycle status of a todo list.\nenum CommonListStatus {\n    Open as \"Open\" = 0\n    Done as \"Done\" = 1\n}\n",
        );
    assert_ne!(
        model, with_features,
        "starter model shape drifted — update the fixture edit:\n{model}"
    );
    fs::write(&model_path, with_features).unwrap();

    Fixture {
        output: project.join("generated"),
        mox: model_path,
        config: project.join("domains.toml"),
        profiles: profiles_path,
    }
}

/// Run the full mox-first pipeline (`init_scaffold_runs_mox_first` shape).
async fn generate(fixture: &Fixture) -> PathBuf {
    let mox_files = vec![fixture.mox.clone()];
    codegraph::driver::run(codegraph::driver::RunArgs {
        integration_manifests: &[],
        schemas: None,
        classifier: None,
        config_path: &fixture.config,
        output: &fixture.output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(fixture.profiles.clone()),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: &mox_files,
        rosetta_files: &[],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        ux_rules: None,
        codegraph_rev: None,
        check: false,
    })
    .await
    .expect("pipeline run must succeed");
    fixture.output.join("ui")
}

fn read(base: &Path, rel: &str) -> String {
    fs::read_to_string(base.join(rel))
        .unwrap_or_else(|e| panic!("expected {rel} to be emitted: {e}"))
}

#[tokio::test]
async fn entity_form_flag_emits_the_tanstack_form_plane() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture_with_flag(&dir, true);
    let ui = generate(&fixture).await;

    // ── The form creator scaffold (once per tree) ──────────────────────
    let form_context = read(&ui, "src/lib/forms/form-context.ts");
    assert!(
        form_context.contains("createFormCreatorContexts"),
        "{form_context}"
    );
    assert!(
        form_context.contains("export const { useFieldContext, useFormContext }"),
        "{form_context}"
    );

    let form_module = read(&ui, "src/lib/forms/form.ts");
    assert!(form_module.contains("createFormCreator"), "{form_module}");
    assert!(form_module.contains("fieldComponents:"), "{form_module}");
    assert!(form_module.contains("formComponents:"), "{form_module}");
    assert!(
        form_module.contains("export const { createAppForm }"),
        "{form_module}"
    );

    // One field component per control kind, consuming the field context.
    for component in [
        "TextField",
        "EmailField",
        "NumberField",
        "CheckboxField",
        "SelectField",
        "DateField",
        "DateTimeField",
        "TextareaField",
    ] {
        let path = ui
            .join("src/lib/forms/fields")
            .join(format!("{component}.svelte"));
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("expected {component}.svelte to be emitted: {e}"));
        assert!(
            source.contains("useFieldContext"),
            "{component} must consume the field context:\n{source}"
        );
    }

    // ── The TanStack form component ────────────────────────────────────
    let list_form = read(&ui, "src/lib/components/forms/CommonTodoListForm.svelte");
    assert!(
        list_form.contains("createAppForm"),
        "the form renders through the app form creator:\n{list_form}"
    );
    assert!(list_form.contains("from '$lib/forms/form'"), "{list_form}");
    assert!(
        list_form.contains("import { z } from 'zod';"),
        "the zod schema rides the form component:\n{list_form}"
    );
    assert!(
        list_form.contains("export const commonTodoListSchema = z.object({"),
        "the zod schema is lowered in RUST and interpolated:\n{list_form}"
    );
    // Schema arms per control kind (bounded + optional + junction + enum).
    assert!(
        list_form.contains("name: z.string(),"),
        "required plain string:\n{list_form}"
    );
    assert!(
        list_form.contains("description: z.string().optional(),"),
        "optional string:\n{list_form}"
    );
    assert!(
        list_form.contains("items: z.array(z.uuid()),"),
        "junction array arm:\n{list_form}"
    );
    assert!(
        list_form.contains("z.enum([\"Open\", \"Done\"])"),
        "codelist arm:\n{list_form}"
    );
    assert!(list_form.contains("validators:"), "{list_form}");
    // TanStack machinery, not hand-rolled state machines.
    assert!(list_form.contains("AppField"), "{list_form}");
    assert!(
        list_form.contains("form.Subscribe"),
        "the submit button is driven by Subscribe:\n{list_form}"
    );
    assert!(list_form.contains("isSubmitting"), "{list_form}");
    assert!(
        !list_form.contains("let error = $state"),
        "no hand-rolled form-error machine:\n{list_form}"
    );
    assert!(
        !list_form.contains("let submitting = $state"),
        "no hand-rolled submitting machine:\n{list_form}"
    );
    assert!(!list_form.contains("let formError = $state"), "{list_form}");
    // The onsubmit contract.
    assert!(list_form.contains("onsubmit"), "{list_form}");
    assert!(
        list_form.contains("fieldErrors"),
        "server 422 details map onto fields:\n{list_form}"
    );
    // Testid contract (ux::plan::ids projection).
    assert!(
        list_form.contains("data-testid=\"common_todo_list-form\""),
        "{list_form}"
    );
    assert!(
        list_form.contains("data-testid=\"common_todo_list-submit-btn\""),
        "{list_form}"
    );
    assert!(
        list_form.contains("data-testid=\"common_todo_list-cancel-btn\""),
        "{list_form}"
    );
    // Junction array machinery: mode="array" + push/remove rows, POM testids.
    assert!(list_form.contains("mode=\"array\""), "{list_form}");
    assert!(list_form.contains("pushValue"), "{list_form}");
    assert!(list_form.contains("removeValue"), "{list_form}");
    assert!(
        list_form.contains("data-testid=\"items-add-btn\""),
        "the junction add/remove rows carry the POM array testids:\n{list_form}"
    );

    // The item form: the bounded scalar refers lands as an optional uuid
    // field (`{name}_id` — the stored-FK projection).
    let item_form = read(&ui, "src/lib/components/forms/CommonTodoItemForm.svelte");
    assert!(
        item_form.contains("todo_list_id: z.uuid().optional(),"),
        "scalar entity-ref arm:\n{item_form}"
    );
    assert!(
        item_form.contains("completed: z.boolean().optional(),"),
        "optional boolean arm:\n{item_form}"
    );

    // ── The page wrappers wire the remote commands + fieldErrors ───────
    let new_page = read(
        &ui,
        "src/routes/(app)/common/common-todo-list/new/+page.svelte",
    );
    assert!(
        new_page.contains("import { createItem } from './data.remote';"),
        "{new_page}"
    );
    assert!(new_page.contains("await createItem("), "{new_page}");
    assert!(
        new_page.contains("fieldErrors"),
        "the create page maps 422 details into fieldErrors:\n{new_page}"
    );
    assert!(new_page.contains("onsubmit="), "{new_page}");

    let edit_page = read(
        &ui,
        "src/routes/(app)/common/common-todo-list/[common_todo_list_id]/edit/+page.svelte",
    );
    assert!(
        edit_page.contains("import { updateItem } from './data.remote';"),
        "{edit_page}"
    );
    assert!(edit_page.contains("await updateItem("), "{edit_page}");
    assert!(edit_page.contains("fieldErrors"), "{edit_page}");

    // ── The remote modules parse 422 details into fieldErrors ──────────
    let create_remote = read(
        &ui,
        "src/routes/(app)/common/common-todo-list/new/data.remote.ts",
    );
    assert!(
        create_remote.contains("export const createItem"),
        "{create_remote}"
    );
    assert!(
        create_remote.contains("fieldErrors"),
        "the fail leg parses VALIDATION_ERROR details:\n{create_remote}"
    );
    assert!(create_remote.contains("details"), "{create_remote}");
    let edit_remote = read(
        &ui,
        "src/routes/(app)/common/common-todo-list/[common_todo_list_id]/edit/data.remote.ts",
    );
    assert!(
        edit_remote.contains("export const updateItem"),
        "{edit_remote}"
    );
    assert!(edit_remote.contains("fieldErrors"), "{edit_remote}");

    // ── The scaffold dependency pins ────────────────────────────────────
    let package_json = read(&ui, "package.json");
    assert!(
        package_json.contains(&format!(
            "\"@tanstack/svelte-form\": \"{TANSTACK_SVELTE_FORM_VERSION}\""
        )),
        "{package_json}"
    );
    assert!(
        package_json.contains(&format!("\"zod\": \"{ZOD_VERSION}\"")),
        "{package_json}"
    );
}

#[tokio::test]
async fn entity_form_off_keeps_the_hand_rolled_form() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture_with_flag(&dir, false);
    let ui = generate(&fixture).await;

    // No form-creator scaffold under the flag-off tree.
    assert!(
        !ui.join("src/lib/forms").exists(),
        "flag off must not emit ui/src/lib/forms/"
    );

    // The form component keeps the hand-rolled $state-bound shape.
    let list_form = read(&ui, "src/lib/components/forms/CommonTodoListForm.svelte");
    assert!(
        list_form.contains("= $state("),
        "the hand-rolled marker:\n{list_form}"
    );
    assert!(!list_form.contains("createAppForm"), "{list_form}");
    assert!(
        !list_form.contains("z.object("),
        "no zod schema under the flag:\n{list_form}"
    );

    // package.json carries neither dependency.
    let package_json = read(&ui, "package.json");
    assert!(
        !package_json.contains("@tanstack/svelte-form"),
        "{package_json}"
    );
    assert!(!package_json.contains("\"zod\""), "{package_json}");

    // The remote modules keep the plain discriminated fail leg (no
    // fieldErrors parsing) — shared with `entity_remote_functions`-only
    // runs, so any change here breaks flag-off identity.
    let create_remote = read(
        &ui,
        "src/routes/(app)/common/common-todo-list/new/data.remote.ts",
    );
    assert!(
        create_remote.contains("export const createItem"),
        "{create_remote}"
    );
    assert!(!create_remote.contains("fieldErrors"), "{create_remote}");
    assert!(!create_remote.contains("details"), "{create_remote}");

    // The pages keep the #574 remote-page shape (no tanstack wiring).
    let new_page = read(
        &ui,
        "src/routes/(app)/common/common-todo-list/new/+page.svelte",
    );
    assert!(new_page.contains("await createItem("), "{new_page}");
    assert!(!new_page.contains("fieldErrors"), "{new_page}");
    assert!(!new_page.contains("onsubmit="), "{new_page}");
}

#[tokio::test]
async fn entity_form_fields_carry_the_pom_testid_contract() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture_with_flag(&dir, true);
    let ui = generate(&fixture).await;

    // Field inputs resolve under the POM's formInput() locator —
    // `[data-testid="{module}-field-{name}"]` (validation e2e family).
    let list_form = read(&ui, "src/lib/components/forms/CommonTodoListForm.svelte");
    assert!(
        list_form.contains("common_todo_list-field-"),
        "field testids compose from the canonical prefix:\n{list_form}"
    );

    // The select field component renders a native select with the closed
    // value set so the POM's selectOption fill keeps working.
    let select = read(&ui, "src/lib/forms/fields/SelectField.svelte");
    assert!(select.contains("<select"), "{select}");
    assert!(
        select.contains("selectOption") || select.contains("onchange"),
        "{select}"
    );

    // The checkbox keeps the Switch aria-checked contract the POM fills
    // through.
    let checkbox = read(&ui, "src/lib/forms/fields/CheckboxField.svelte");
    assert!(checkbox.contains("aria-checked"), "{checkbox}");

    // Required fields keep the native-validity contract (validation e2e
    // family asserts `validity.valid`).
    let text = read(&ui, "src/lib/forms/fields/TextField.svelte");
    assert!(text.contains("required"), "{text}");
    assert!(text.contains("aria-required"), "{text}");
    assert!(text.contains("data-validate-message"), "{text}");
}
