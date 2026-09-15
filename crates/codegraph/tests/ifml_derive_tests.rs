//! Spike tests for `codegraph ifml-derive`: reverse-inferring an IFML DSL
//! model from SvelteKit `+page.svelte` fixtures (issue #196 §9).

use std::path::Path;

use codegraph_core::traits::GraphQuerier;

use codegraph::ifml_derive::{derive_view, render_model, view_name_from_route, IfmlDeriveArgs};

/// A form page shaped like the forward pipeline's generated pages: wrapper
/// div, form with typed fields, submit + cancel buttons whose handlers
/// contain goto() calls, and a POST fetch.
const FORM_PAGE: &str = r#"<script lang="ts">
	import { goto } from '$app/navigation';

	let title = $state('');
	let count = $state(0);
	let status = $state('open');

	async function submit_editor(event: SubmitEvent) {
		event.preventDefault();
		const response = await fetch('/api/v1/todo-lists', {
			method: 'POST',
			body: JSON.stringify({ title })
		});
		if (!response.ok) return;
		goto('/todolist?customerId=abc');
	}

	function cancel_edit() {
		goto('/todolist');
	}
</script>

<svelte:head>
	<title>Edit Todo</title>
</svelte:head>

<div class="page">
	<h1>Edit Todo</h1>
	<form on:submit|preventDefault={submit_editor}>
		<label>
			Title
			<input name="title" type="text" required bind:value={title} />
		</label>
		<label>
			Count
			<input name="count" type="number" />
		</label>
		<label>
			Status
			<select name="status" bind:value={status}>
				<option value="open">Open</option>
				<option value="done">Done</option>
			</select>
		</label>
		<label>
			Secret
			<input name="secret" type="password" />
		</label>
		<input type="hidden" />
		<Button onclick={submit_editor}>Save</Button>
		<button on:click={cancel_edit}>Cancel</button>
	</form>
</div>
"#;

/// A list page at the root route: table rows whose click handler navigates,
/// a button whose handler is not defined in the script (action fallback),
/// and a list-data fetch with no component to attach to.
const LIST_PAGE: &str = r#"<script lang="ts">
	import { goto } from '$app/navigation';

	function open_detail(item: Record<string, unknown>) {
		goto(`/customerdetail?customerId=${item.id}`);
	}

	const initial = await fetch('/api/v1/customers');
</script>

<div class="page">
	<table>
		<tbody>
			{#each data.items as item}
				<tr onclick={() => open_detail(item)}>
					<td>{item.name}</td>
				</tr>
			{/each}
		</tbody>
	</table>
	<button on:click={refresh_list}>Refresh</button>
</div>
"#;

/// An odd page exercising the documented skips: nested layout containers,
/// unknown elements, an orphan input outside any form, and a multi-segment
/// fetch that is not confident.
const ODD_PAGE: &str = r#"<script lang="ts">
	const rollup = await fetch('/api/v1/reports/monthly/summary');
</script>

<div class="shell">
	<div class="toolbar">
		<button on:click={deep_handler}>Deep</button>
	</div>
	<canvas data-chart="bar"></canvas>
	<svelte:window on:keydown={noop} />
	<input name="orphan" type="checkbox" />
</div>
"#;

/// A form page whose handlers are `const` arrow functions instead of
/// `function` declarations (issue #203 upgrade 1): handler bodies must still
/// resolve to goto()/fetch() analysis, including from inline arrow handlers.
/// The save handler's goto binding is three segments deep (issue #203
/// upgrade 4): it must project onto the closest two segments so the DSL
/// (which binds at most two) accepts it.
const ARROW_PAGE: &str = r#"<script lang="ts">
	import { goto } from '$app/navigation';

	let title = $state('');

	const save = async () => {
		const response = await fetch('/api/v1/todo-lists', {
			method: 'POST',
			body: JSON.stringify({ title })
		});
		if (!response.ok) return;
		goto('/todolist?backTo=form.draft.origin');
	};

	const cancel = () => goto('/todolist');
</script>

<div class="page">
	<form on:submit|preventDefault={save}>
		<label>
			Title
			<input name="title" type="text" required bind:value={title} />
		</label>
		<Button onclick={() => save(title)}>Save</Button>
		<button on:click={() => cancel()}>Cancel</button>
	</form>
</div>
"#;

/// A page with two distinct confident fetches (issue #203 upgrade 5): still
/// no `data:` inference, but the skip must be observable.
const MULTI_FETCH_PAGE: &str = r#"<script lang="ts">
	const customers = await fetch('/api/v1/customers');
	const vendors = await fetch('/api/v1/vendors');
</script>

<div class="page">
	<p>Overview of both feeds.</p>
</div>
"#;

fn derived(content: &str, route: &str) -> (String, codegraph::ifml_derive::DerivedView) {
    let name = view_name_from_route(route);
    let view = derive_view(&name, route, content);
    let rendered = render_model("todo", std::slice::from_ref(&view));
    codegraph_ifml_dsl::parse_ifml(&rendered).expect("derived model must parse");
    (rendered, view)
}

#[test]
fn form_page_derives_form_view_with_fields_and_navigations() {
    let (out, view) = derived(FORM_PAGE, "todo-editor");
    assert_eq!(view.name, "TodoEditor");
    assert!(view.container, "single top-level div becomes a container");

    assert!(out.contains(r#"view "TodoEditor""#));
    assert!(out.contains(r#"container "Main""#));
    assert!(out.contains("type: form;"));
    // fetch('/api/v1/todo-lists') -> data on the form component
    assert!(out.contains("data: TodoList;"));
    assert!(out.contains("field title -> input text { required: true; }"));
    assert!(out.contains("field count -> input number;"));
    assert!(out.contains("field status -> input dropdown;"));
    assert!(out.contains("field secret -> input password;"));
    // goto('/todolist?customerId=abc') with a simple identifier value
    assert!(out.contains(r#"on save -> navigate("Todolist", { customerId: abc });"#));
    assert!(out.contains(r#"on cancel -> navigate("Todolist");"#));
    // submit_editor resolves identically from on:submit and Button onclick;
    // the duplicate is deduped.
    assert_eq!(out.matches("submit_editor").count(), 0);
    // A single non-array fetch without an each block is details-shaped
    // (issue #203 upgrade 2): data on the form, never a list component.
    assert!(!out.contains("type: list;"));
}

#[test]
fn arrow_const_handlers_resolve_to_bodies_with_deep_binding_projection() {
    let (out, view) = derived(ARROW_PAGE, "todo-editor");
    assert_eq!(view.name, "TodoEditor");
    // const save = async () => { ... goto(...) } must resolve through the
    // inline arrow `onclick={() => save(title)}` (issue #203 upgrade 1).
    assert!(
        out.contains(r#"on save -> navigate("Todolist", { backTo: draft.origin });"#),
        "const arrow handler must resolve to its goto analysis:\n{out}"
    );
    // const cancel = () => goto('/todolist') — expression-bodied arrow.
    assert!(
        out.contains(r#"on cancel -> navigate("Todolist");"#),
        "expression-bodied const arrow must resolve:\n{out}"
    );
    assert!(
        !out.contains(r#"action("save")"#),
        "no action fallback:\n{out}"
    );
    assert!(
        !out.contains(r#"action("cancel")"#),
        "no action fallback:\n{out}"
    );
    // fetch('/api/v1/todo-lists') inside the const body still feeds data:.
    assert!(out.contains("data: TodoList;"));
    // The 3-segment binding form.draft.origin projects onto draft.origin
    // (issue #203 upgrade 4) and the projection is observable.
    assert!(
        view.skipped
            .iter()
            .any(|s| s.contains("form.draft.origin") && s.contains("draft.origin")),
        "deep binding projection must be noted: {:?}",
        view.skipped
    );
}

#[test]
fn form_page_skips_nameless_controls() {
    let (_, view) = derived(FORM_PAGE, "todo-editor");
    assert!(
        view.skipped
            .iter()
            .any(|s| s.contains("input") && s.contains("name")),
        "nameless hidden input should be recorded as skipped: {:?}",
        view.skipped
    );
    assert!(!view.skipped.iter().any(|s| s.contains("<form>")));
}

#[test]
fn list_page_derives_list_component_with_row_select() {
    let (out, view) = derived(LIST_PAGE, "");
    assert_eq!(view.name, "Index");
    assert!(out.contains(r#"view "Index""#));
    // A single confident fetch + {#each} render ⇒ a list component
    // (issue #203 upgrade 2); the each variable is the generic `item`, so the
    // name falls back to the singularized fetch stem.
    let comp_at = out
        .find(r#"component "customer" {"#)
        .expect("list component");
    let type_at = out.find("type: list;").expect("list type");
    let data_at = out.find("data: Customer;").expect("list data");
    let select_at = out
        .find(r#"on select(row) -> navigate("Customerdetail", { customerId: item.id });"#)
        .expect("row select inside the list component");
    assert!(comp_at < type_at && type_at < data_at && data_at < select_at);
    // The select event lives on the component, not duplicated at view level.
    assert_eq!(out.matches("on select(row)").count(), 1);
    let list_comp = view
        .components
        .iter()
        .find(|c| c.name == "customer")
        .expect("list component in view struct");
    assert_eq!(list_comp.data.as_deref(), Some("Customer"));
    assert_eq!(list_comp.events.len(), 1);
    // Non-row buttons stay view-level (undefined handler → action fallback).
    assert!(
        out.contains(r#"on click -> action("refresh_list");"#),
        "undefined handler falls back to action:\n{out}"
    );
}

#[test]
fn multi_fetch_records_observable_skip_note() {
    let (out, view) = derived(MULTI_FETCH_PAGE, "overview");
    // Two distinct confident fetches stay skipped for `data:`, but the skip
    // is now asserted and reported (issue #203 upgrade 5).
    let note = view
        .skipped
        .iter()
        .find(|s| s.contains("multiple distinct fetch"))
        .expect("multi-fetch skip must be recorded");
    assert!(
        note.contains("Customer"),
        "note names both entities: {note}"
    );
    assert!(note.contains("Vendor"), "note names both entities: {note}");
    assert!(!out.contains("data:"), "no data inferred:\n{out}");
}

#[test]
fn odd_page_records_skips_and_emits_stub_view() {
    let (out, view) = derived(ODD_PAGE, "odd-bit");
    assert_eq!(view.name, "OddBit");
    assert!(out.contains(r#"view "OddBit""#));
    assert!(out.contains(r#"container "Main""#), "wrapper kept:\n{out}");

    // Nested toolbar button must NOT surface (flat inference).
    assert!(!out.contains("deep_handler"));
    assert!(view.skipped.iter().any(|s| s.contains("toolbar")));
    // Unknown elements are skipped but listed.
    assert!(view.skipped.iter().any(|s| s.contains("canvas")));
    assert!(view.skipped.iter().any(|s| s.contains("svelte:window")));
    // Inputs outside forms are not fields.
    assert!(!out.contains("field orphan"));
    assert!(view.skipped.iter().any(|s| s.contains("orphan")));
    // Multi-segment fetch is not confident: no data emitted.
    assert!(!out.contains("data:"));
}

#[test]
fn full_model_parses_and_has_all_views() {
    let views: Vec<_> = [
        ("todo-editor", FORM_PAGE),
        ("", LIST_PAGE),
        ("odd-bit", ODD_PAGE),
    ]
    .into_iter()
    .map(|(route, src)| derive_view(&view_name_from_route(route), route, src))
    .collect();
    let content = render_model("todo", &views);
    let model = codegraph_ifml_dsl::parse_ifml(&content).expect("combined model must parse");
    assert_eq!(model.views.len(), 3);
    let mut names: Vec<&str> = model.views.iter().map(|v| v.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["Index", "OddBit", "TodoEditor"]);
    assert_eq!(model.domains.len(), 1);
    assert_eq!(model.domains[0].name, "todo");
}

/// Round-trip: derived model is not just parseable but graph-compatible.
#[tokio::test]
async fn derived_model_ingests_into_mock_graph() {
    let views: Vec<_> = [("todo-editor", FORM_PAGE), ("", LIST_PAGE)]
        .into_iter()
        .map(|(route, src)| derive_view(&view_name_from_route(route), route, src))
        .collect();
    let content = render_model("todo", &views);
    let model = codegraph_ifml_dsl::parse_ifml(&content).unwrap();

    let engine = codegraph_core::mock::MockEngine::new();
    codegraph::ingest::ifml_ingest::ingest_ifml_model(&engine, &model)
        .await
        .expect("derived model must ingest");

    let containers = engine.get_ifml_view_containers().await.unwrap();
    // 2 view containers + the nested "Main" container (both pages nest a
    // container of the same name, which dedupes to one node).
    assert_eq!(containers.len(), 3);
    let components = engine.get_ifml_view_components("Main").await.unwrap();
    // The form page contributes its form component and the list page its
    // list component; both live under the shared "Main" container id.
    let mut names: Vec<&str> = components.iter().map(|c| c.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["customer", "form"]);
}

fn write_page(root: &Path, route_dir: &str, content: &str) {
    let dir = root.join("src/routes").join(route_dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("+page.svelte"), content).unwrap();
}

#[test]
fn cli_discovers_routes_writes_and_respects_force() {
    let tmp = tempfile::tempdir().unwrap();
    write_page(tmp.path(), "", LIST_PAGE);
    write_page(tmp.path(), "todo-editor", FORM_PAGE);
    write_page(tmp.path(), "odd-bit", ODD_PAGE);
    // Non-page files are ignored.
    std::fs::write(tmp.path().join("src/routes/+layout.svelte"), "<slot />").unwrap();
    std::fs::write(
        tmp.path().join("src/routes/todo-editor/+page.server.ts"),
        "export {}",
    )
    .unwrap();

    let output = tmp.path().join("derived.ifml");
    codegraph::ifml_derive::ifml_derive(IfmlDeriveArgs {
        from_svelte: tmp.path(),
        output: &output,
        name: Some("todo"),
        force: false,
    })
    .expect("derive should succeed");

    let content = std::fs::read_to_string(&output).unwrap();
    let model = codegraph_ifml_dsl::parse_ifml(&content).expect("emitted file must parse");
    assert_eq!(model.domains[0].name, "todo");
    assert_eq!(model.views.len(), 3);

    let err = codegraph::ifml_derive::ifml_derive(IfmlDeriveArgs {
        from_svelte: tmp.path(),
        output: &output,
        name: Some("todo"),
        force: false,
    })
    .expect_err("second run without --force must fail");
    assert!(err.to_string().contains("--force"));

    codegraph::ifml_derive::ifml_derive(IfmlDeriveArgs {
        from_svelte: tmp.path(),
        output: &output,
        name: Some("todo"),
        force: true,
    })
    .expect("second run with --force must succeed");
}

#[test]
fn cli_fails_when_no_pages_found() {
    let tmp = tempfile::tempdir().unwrap();
    let err = codegraph::ifml_derive::ifml_derive(IfmlDeriveArgs {
        from_svelte: tmp.path(),
        output: &tmp.path().join("out.ifml"),
        name: None,
        force: false,
    })
    .expect_err("no pages must be an error");
    assert!(err.to_string().contains("no +page.svelte"));
}
