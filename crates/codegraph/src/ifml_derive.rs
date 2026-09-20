//! `ifml-derive`: reverse-infer an IFML DSL model from existing SvelteKit
//! `+page.svelte` files (issue #196 §9 spike: "existing Svelte applications
//! become inputs to your enterprise UI model").
//!
//! Parser: `tree-sitter-svelte-next` 0.1 (compatible with the workspace
//! tree-sitter 0.25 runtime). Script contents are only string-scanned for
//! handler names and `goto(...)` targets — no JS AST parsing.
//!
//! Supported inference subset (everything else is recorded as a skip and
//! reported on stderr):
//! - `+page.svelte` under a `routes/` directory → `view "<Name>"` (route
//!   segments PascalCased via `codegraph-naming`; root route → `Index`)
//! - a single top-level markup element → `container "Main"` wrapper; content
//!   one level inside it is scanned (flat inference, deeper nesting skipped)
//! - `<form>` → `component "form" { type: form; }`; descendant
//!   input/select/textarea elements become `field <name> -> input <type>`
//!   (name from `name=` or `bind:value=`; type from the `type` attribute,
//!   unknown/absent → text); `required` → `required: true;`
//! - `<Button onclick={fn}>` / `<button on:click={fn}>` and a form's
//!   `on:submit|mods={fn}` → `on <click|save|cancel> -> ...` (save when the
//!   handler name starts with submit/save, cancel with cancel/back); `fn` may
//!   be a `function NAME` declaration or a `const NAME = (…) => …` handler
//!   (expression bodies included), and inline arrows resolve to their named
//!   handler; when the handler body contains `goto(...)`, the event navigates
//!   to the inferred target view — query params become bindings when the
//!   value is a dotted identifier chain, projected onto the closest two
//!   segments (the DSL binds at most two); non-identifier values drop that
//!   binding. Dropped bindings and projections are reported as skips.
//!   Otherwise the event falls back to `-> action("fn")`
//! - `<tr onclick={() => fn(item)}>` → `on select(row) -> ...` with the same
//!   navigation/action resolution
//! - a single confident `fetch('...')` (plain path segment after an optional
//!   `/api/v<N>` prefix) combined with an `{#each …}` render → `component
//!   "x" { type: list; data: <Entity>; }` carrying the row select events;
//!   a single confident fetch without an each block attaches `data:` to the
//!   first component (or the view); ≥2 distinct confident fetches skip
//!   `data:` with an explicit note

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::path::Path;

use codegraph_naming::to_pascal_case;
use tree_sitter::{Node, Parser};
use walkdir::WalkDir;

use crate::error::{Error, Result};

const PAGE_FILE: &str = "+page.svelte";

pub struct IfmlDeriveArgs<'a> {
    pub from_svelte: &'a Path,
    pub output: &'a Path,
    pub name: Option<&'a str>,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DerivedField {
    pub name: String,
    pub input: &'static str,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentKind {
    Form,
    List,
}

impl ComponentKind {
    fn as_str(self) -> &'static str {
        match self {
            ComponentKind::Form => "form",
            ComponentKind::List => "list",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DerivedComponent {
    pub name: String,
    pub kind: ComponentKind,
    pub data: Option<String>,
    pub fields: Vec<DerivedField>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DerivedView {
    pub name: String,
    pub route: String,
    pub container: bool,
    pub components: Vec<DerivedComponent>,
    pub events: Vec<String>,
    pub data: Option<String>,
    pub skipped: Vec<String>,
}

struct Nav {
    target: String,
    bindings: Vec<(String, String)>,
    notes: Vec<String>,
}

/// What the `<script>` block's `fetch(...)` calls say about the page's data.
enum FetchScan {
    None,
    Single { entity: String, segment: String },
    Multiple(Vec<String>),
}

/// Run the `ifml-derive` command: derive one view per `+page.svelte`, render
/// the DSL, verify it parses, and write it to `output`.
pub fn ifml_derive(args: IfmlDeriveArgs<'_>) -> Result<()> {
    let pages = discover_pages(args.from_svelte)?;
    if pages.is_empty() {
        return Err(Error::Config(format!(
            "no {PAGE_FILE} files found under {}",
            args.from_svelte.display()
        )));
    }

    let domain_name =
        sanitize_identifier(args.name.unwrap_or("app")).unwrap_or_else(|| "app".to_string());

    let mut views: Vec<DerivedView> = Vec::new();
    let mut seen_names = HashSet::new();
    let mut route_skips = 0usize;
    for (route, path) in &pages {
        let name = view_name_from_route(route);
        if !seen_names.insert(name.clone()) {
            eprintln!("  skipping '{route}': duplicate view name '{name}'");
            route_skips += 1;
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        let view = derive_view(&name, route, &source);
        for skip in &view.skipped {
            eprintln!("  skipping '{route}' (view '{}'): {skip}", view.name);
        }
        views.push(view);
    }
    views.sort_by(|a, b| a.route.cmp(&b.route));

    let content = render_model(&domain_name, &views);
    if let Err(e) = rex_ifml::parse_ifml(&content) {
        return Err(Error::Config(format!(
            "internal error: derived IFML failed to parse: {e}"
        )));
    }

    if args.output.exists() && !args.force {
        return Err(Error::Config(format!(
            "output file '{}' already exists (use --force to overwrite)",
            args.output.display()
        )));
    }
    std::fs::write(args.output, &content)?;
    println!(
        "Derived {} views from {} pages -> {}{}",
        views.len(),
        pages.len(),
        args.output.display(),
        if route_skips > 0 {
            format!(" ({route_skips} route-level skips)")
        } else {
            String::new()
        }
    );
    Ok(())
}

/// Collect `+page.svelte` files under `root` as `(route, path)` pairs. The
/// route is the path relative to the closest ancestor directory named
/// `routes` (falling back to `root`), minus the file name; the root page has
/// an empty route.
fn discover_pages(root: &Path) -> Result<Vec<(String, std::path::PathBuf)>> {
    let mut pages = Vec::new();
    for entry in WalkDir::new(root).sort_by_file_name().into_iter().flatten() {
        if !entry.file_type().is_file() || entry.file_name() != OsStr::new(PAGE_FILE) {
            continue;
        }
        let path = entry.path();
        let base = path
            .ancestors()
            .find(|a| a.file_name() == Some(OsStr::new("routes")));
        let route_path: std::path::PathBuf = match base {
            Some(routes_dir) => path
                .strip_prefix(routes_dir)
                .map_err(|e| Error::Config(e.to_string()))?
                .to_path_buf(),
            None => path
                .strip_prefix(root)
                .map_err(|e| Error::Config(e.to_string()))?
                .to_path_buf(),
        };
        let route = route_path
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if route == "." {
            pages.push((String::new(), path.to_path_buf()));
        } else {
            pages.push((route, path.to_path_buf()));
        }
    }
    Ok(pages)
}

/// Derive one view from a single `.svelte` page source.
pub fn derive_view(name: &str, route: &str, source: &str) -> DerivedView {
    let mut view = DerivedView {
        name: name.to_string(),
        route: route.to_string(),
        container: false,
        components: Vec::new(),
        events: Vec::new(),
        data: None,
        skipped: Vec::new(),
    };

    let Ok(tree) = parse_svelte(source) else {
        view.skipped
            .push("page did not parse (no markup analyzed)".to_string());
        return view;
    };
    let root = tree.root_node();

    let script = direct_child(root, "script_element")
        .and_then(|s| raw_text_of(s, source))
        .unwrap_or_default();
    let functions = extract_functions(script);
    let fetch = fetch_scan(script);
    if let FetchScan::Multiple(entities) = &fetch {
        view.skipped.push(format!(
            "multiple distinct fetch endpoints ({}); no data inferred",
            entities.join(", ")
        ));
    }
    let list = detect_list(root, source, &functions, &fetch, &mut view.skipped);
    let has_list = list.is_some();
    if let Some(comp) = list {
        view.components.push(comp);
    }

    let top: Vec<Node<'_>> = direct_children(root, "element")
        .into_iter()
        .filter(|el| tag_of(*el, source).is_none_or(|t| !t.starts_with("svelte:")))
        .collect();

    let items: Vec<Node<'_>> = if top.len() == 1 {
        view.container = true;
        direct_children(top[0], "element")
    } else {
        top
    };

    let mut form_counter = 0usize;
    let mut seen_events = HashSet::new();
    for item in items {
        let Some(tag) = tag_of(item, source) else {
            continue;
        };
        match tag {
            "form" => {
                form_counter += 1;
                let mut comp = derive_form(item, source, &functions, &mut view.skipped);
                comp.name = if form_counter == 1 {
                    "form".to_string()
                } else {
                    format!("form{form_counter}")
                };
                view.components.push(comp);
            }
            "table" => {
                if has_list && contains_each_statement(item) {
                    continue;
                }
                for line in table_select_events(item, source, &functions, &mut view.skipped) {
                    if seen_events.insert(line.clone()) {
                        view.events.push(line);
                    }
                }
            }
            t if is_button_tag(t) => {
                if let Some(line) = button_event_line(item, source, &functions, &mut view.skipped) {
                    if seen_events.insert(line.clone()) {
                        view.events.push(line);
                    }
                }
            }
            t => {
                let mut skip = format!("element <{t}> outside supported subset");
                if let Some(class) = attr_value(item, source, "class") {
                    skip.push_str(&format!(" (class=\"{class}\")"));
                }
                if let Some(name) = attr_value(item, source, "name") {
                    skip.push_str(&format!(" (name=\"{name}\")"));
                }
                view.skipped.push(skip);
            }
        }
    }

    if let FetchScan::Single { entity, .. } = &fetch {
        if !has_list {
            if let Some(comp) = view.components.first_mut() {
                comp.data = Some(entity.clone());
            } else {
                view.data = Some(entity.clone());
            }
        }
    }
    view
}

fn parse_svelte(source: &str) -> std::result::Result<tree_sitter::Tree, Error> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svelte_next::LANGUAGE.into())
        .map_err(|e| Error::Config(format!("svelte parser unavailable: {e}")))?;
    parser
        .parse(source, None)
        .ok_or_else(|| Error::Config("svelte parser produced no tree".to_string()))
}

fn derive_form(
    form: Node<'_>,
    source: &str,
    functions: &BTreeMap<String, String>,
    skipped: &mut Vec<String>,
) -> DerivedComponent {
    let mut fields: Vec<DerivedField> = Vec::new();
    let mut seen_fields = HashSet::new();
    let mut events: Vec<String> = Vec::new();
    let mut seen_events = HashSet::new();

    if let Some(line) = submit_event_line(form, source, functions, skipped) {
        if seen_events.insert(line.clone()) {
            events.push(line);
        }
    }
    for el in descendant_elements(form) {
        let Some(tag) = tag_of(el, source) else {
            continue;
        };
        if matches!(tag, "input" | "select" | "textarea") {
            if let Some(field) = field_from_element(el, source, tag) {
                if seen_fields.insert(field.name.clone()) {
                    fields.push(field);
                }
            } else {
                skipped.push(format!("form control <{tag}> without a name or bind:value"));
            }
        } else if is_button_tag(tag) {
            if let Some(line) = button_event_line(el, source, functions, skipped) {
                if seen_events.insert(line.clone()) {
                    events.push(line);
                }
            }
        }
    }

    DerivedComponent {
        name: "form".to_string(),
        kind: ComponentKind::Form,
        data: None,
        fields,
        events,
    }
}

fn field_from_element(el: Node<'_>, source: &str, tag: &str) -> Option<DerivedField> {
    let name = attr_value(el, source, "name").or_else(|| attr_value(el, source, "bind:value"))?;
    let name = sanitize_identifier(&name)?;
    let input: &'static str = match tag {
        "select" => "dropdown",
        "textarea" => "textarea",
        _ => {
            let ty = attr_value(el, source, "type").unwrap_or_default();
            match ty.to_ascii_lowercase().as_str() {
                "number" => "number",
                "checkbox" => "checkbox",
                "email" => "email",
                "password" => "password",
                "date" => "date",
                "radio" => "radio",
                _ => "text",
            }
        }
    };
    Some(DerivedField {
        name,
        input,
        required: has_attr(el, source, "required"),
    })
}

fn submit_event_line(
    form: Node<'_>,
    source: &str,
    functions: &BTreeMap<String, String>,
    skipped: &mut Vec<String>,
) -> Option<String> {
    let expr = attr_value_matching(form, source, |name| name.starts_with("on:submit"))?;
    let fname = handler_name_from_expr(&expr)?;
    let kind = event_kind_for_handler(&fname);
    Some(format!(
        "on {kind} -> {};",
        action_for_handler(&fname, functions, skipped)
    ))
}

fn button_event_line(
    el: Node<'_>,
    source: &str,
    functions: &BTreeMap<String, String>,
    skipped: &mut Vec<String>,
) -> Option<String> {
    let expr = attr_value(el, source, "onclick")
        .or_else(|| attr_value_matching(el, source, |name| name.starts_with("on:click")))?;
    let fname = handler_name_from_expr(&expr)?;
    let kind = event_kind_for_handler(&fname);
    Some(format!(
        "on {kind} -> {};",
        action_for_handler(&fname, functions, skipped)
    ))
}

fn table_select_events(
    table: Node<'_>,
    source: &str,
    functions: &BTreeMap<String, String>,
    skipped: &mut Vec<String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for el in descendant_elements(table) {
        if tag_of(el, source) != Some("tr") {
            continue;
        }
        let Some(expr) = attr_value(el, source, "onclick") else {
            continue;
        };
        let Some(fname) = handler_name_from_expr(&expr) else {
            continue;
        };
        out.push(format!(
            "on select(row) -> {};",
            action_for_handler(&fname, functions, skipped)
        ));
        if functions
            .get(&fname)
            .and_then(|b| find_first_goto(b))
            .is_none()
        {
            skipped.push(format!(
                "row handler '{fname}' has no goto; emitted as action fallback"
            ));
        }
    }
    out
}

/// Detect a list component: a single confident fetch plus an `{#each}`
/// render. The component carries the row select events from clickable
/// elements inside the each blocks.
fn detect_list(
    root: Node<'_>,
    source: &str,
    functions: &BTreeMap<String, String>,
    fetch: &FetchScan,
    skipped: &mut Vec<String>,
) -> Option<DerivedComponent> {
    let FetchScan::Single { entity, segment } = fetch else {
        return None;
    };
    let eaches = collect_each_statements(root);
    if eaches.is_empty() {
        return None;
    }
    let named_var = eaches
        .iter()
        .find_map(|each| each_parameter(*each, source).filter(|param| !is_generic_loop_var(param)));
    let name =
        named_var.unwrap_or_else(|| singularize(&to_pascal_case(segment)).to_ascii_lowercase());
    let mut events = Vec::new();
    let mut seen = HashSet::new();
    for each in &eaches {
        for el in descendant_elements(*each) {
            let Some(tag) = tag_of(el, source) else {
                continue;
            };
            let Some(expr) = attr_value(el, source, "onclick") else {
                continue;
            };
            let Some(fname) = handler_name_from_expr(&expr) else {
                continue;
            };
            let kind = if tag == "tr" || tag == "li" {
                "select(row)".to_string()
            } else {
                event_kind_for_handler(&fname).to_string()
            };
            let line = format!(
                "on {kind} -> {};",
                action_for_handler(&fname, functions, skipped)
            );
            if seen.insert(line.clone()) {
                events.push(line);
            }
            if functions
                .get(&fname)
                .and_then(|b| find_first_goto(b))
                .is_none()
            {
                skipped.push(format!(
                    "row handler '{fname}' has no goto; emitted as action fallback"
                ));
            }
        }
    }
    Some(DerivedComponent {
        name,
        kind: ComponentKind::List,
        data: Some(entity.clone()),
        fields: Vec::new(),
        events,
    })
}

fn action_for_handler(
    fname: &str,
    functions: &BTreeMap<String, String>,
    skipped: &mut Vec<String>,
) -> String {
    match functions
        .get(fname)
        .and_then(|body| find_first_goto(body))
        .filter(|nav| !nav.target.is_empty())
    {
        Some(nav) => {
            skipped.extend(nav.notes.iter().cloned());
            render_nav(&nav)
        }
        None => format!("action(\"{fname}\")"),
    }
}

fn event_kind_for_handler(fname: &str) -> &'static str {
    let lower = fname.to_ascii_lowercase();
    if lower.starts_with("submit") || lower.starts_with("save") {
        "save"
    } else if lower.starts_with("cancel") || lower.starts_with("back") {
        "cancel"
    } else {
        "click"
    }
}

fn is_button_tag(tag: &str) -> bool {
    tag == "button" || tag.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Resolve a handler function name out of an onclick expression: either a
/// bare identifier (`{save_x}`) or the call target inside an arrow
/// (`{() => open_detail(item)}`). Non-call expressions inside arrows yield
/// `None`.
fn handler_name_from_expr(expr: &str) -> Option<String> {
    let trimmed = expr.trim();
    let has_arrow = trimmed.contains("=>");
    let bytes = trimmed.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if is_ident_start(bytes[i]) {
            let len = leading_ident_len(&trimmed[i..]);
            let name = &trimmed[i..i + len];
            let rest = trimmed[i + len..].trim_start();
            if rest.starts_with('(') {
                return Some(name.to_string());
            }
            if rest.is_empty() && !has_arrow {
                return Some(name.to_string());
            }
            i += len.max(1);
        } else {
            i += 1;
        }
    }
    None
}

/// Extract named handler bodies: `function NAME(...) { ... }` declarations
/// (including `async function`) plus `const NAME = [async] (...) => ...`
/// arrow handlers, so both spellings resolve to goto/fetch analysis.
fn extract_functions(script: &str) -> BTreeMap<String, String> {
    let mut out = extract_function_decls(script);
    for (name, body) in extract_arrow_consts(script) {
        out.entry(name).or_insert(body);
    }
    out
}

fn extract_function_decls(script: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut search_from = 0usize;
    while let Some(rel) = script[search_from..].find("function") {
        let kw = search_from + rel;
        search_from = kw + "function".len();
        let prev_ok = kw == 0 || !is_ident_char(script.as_bytes()[kw - 1]);
        if !prev_ok {
            continue;
        }
        let after_kw = kw + "function".len();
        let ws = script[after_kw..].len() - script[after_kw..].trim_start().len();
        let name_start = after_kw + ws;
        let name_len = leading_ident_len(&script[name_start..]);
        if name_len == 0 || !is_ident_start(script.as_bytes()[name_start]) {
            continue;
        }
        let name = &script[name_start..name_start + name_len];
        if let Some(body) = brace_body_after(script, name_start + name_len) {
            out.entry(name.to_string()).or_insert(body);
        }
    }
    out
}

fn extract_arrow_consts(script: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let bytes = script.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = script[search_from..].find("const") {
        let kw = search_from + rel;
        search_from = kw + "const".len();
        if kw > 0 && is_ident_char(bytes[kw - 1]) {
            continue;
        }
        if search_from < bytes.len() && is_ident_char(bytes[search_from]) {
            continue;
        }
        let ws = script[search_from..].len() - script[search_from..].trim_start().len();
        let name_start = search_from + ws;
        let name_len = leading_ident_len(&script[name_start..]);
        if name_len == 0 {
            continue;
        }
        let name = &script[name_start..name_start + name_len];
        if let Some(body) = arrow_body_after(script, name_start + name_len) {
            out.entry(name.to_string()).or_insert(body);
        }
    }
    out
}

/// Body of a `const NAME = [async] (params) [-> RetType] => ...` initializer:
/// the balanced brace block, or the expression text up to the statement
/// terminator. Non-arrow initializers (`const x = await fetch(...)`) yield
/// `None`.
fn arrow_body_after(s: &str, from: usize) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'=' || bytes.get(i + 1) == Some(&b'=') {
        return None;
    }
    i += 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if s[i..].starts_with("async") && (i + 5 >= bytes.len() || !is_ident_char(bytes[i + 5])) {
        i += 5;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
    }
    if i >= bytes.len() || bytes[i] != b'(' {
        return None;
    }
    let mut depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    i += 1;
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if depth != 0 {
        return None;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    while i < bytes.len() && !matches!(bytes[i], b'=' | b';' | b'{') {
        i += 1;
    }
    if i + 1 >= bytes.len() || bytes[i] != b'=' || bytes[i + 1] != b'>' {
        return None;
    }
    i += 2;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'{' {
        return brace_body_after(s, i);
    }
    let start = i;
    let (mut paren, mut brace, mut bracket) = (0i32, 0i32, 0i32);
    while i < bytes.len() {
        match bytes[i] {
            b'(' => paren += 1,
            b')' => paren -= 1,
            b'{' => brace += 1,
            b'}' => brace -= 1,
            b'[' => bracket += 1,
            b']' => bracket -= 1,
            b';' if paren == 0 && brace == 0 && bracket == 0 => {
                let body = s[start..i].trim();
                return (!body.is_empty()).then(|| body.to_string());
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn brace_body_after(s: &str, from: usize) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i < bytes.len() && bytes[i] != b'{' {
        if bytes[i] == b';' {
            return None;
        }
        i += 1;
    }
    let start = i;
    let mut depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..=i].to_string());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Find the first `goto('...')` / `goto("...")` / `` goto(`...`) `` in a
/// handler body and parse its argument into a target view name plus
/// identifier bindings.
fn find_first_goto(body: &str) -> Option<Nav> {
    let bytes = body.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = body[search_from..].find("goto") {
        let at = search_from + rel;
        let after = at + "goto".len();
        search_from = after;
        let prev_ok = at == 0 || !is_ident_char(bytes[at - 1]);
        let next_ok = after < bytes.len() && !is_ident_char(bytes[after]);
        if !prev_ok || !next_ok {
            continue;
        }
        let mut i = after;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'(' {
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && !matches!(bytes[j], b'"' | b'\'' | b'`') {
            j += 1;
        }
        if j >= bytes.len() {
            continue;
        }
        let quote = bytes[j];
        let mut k = j + 1;
        while k < bytes.len() && bytes[k] != quote {
            if bytes[k] == b'\\' {
                k += 1;
            }
            k += 1;
        }
        let arg = body.get(j + 1..k.min(bytes.len()))?;
        return Some(parse_nav_arg(arg));
    }
    None
}

fn parse_nav_arg(arg: &str) -> Nav {
    let (path_raw, query) = match arg.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (arg, None),
    };
    let target = target_name(path_raw);
    let mut bindings = Vec::new();
    let mut notes = Vec::new();
    if let Some(q) = query {
        for pair in q.split('&') {
            let Some((key_raw, value)) = pair.split_once('=') else {
                notes.push(format!("binding '{pair}' dropped (missing '=')"));
                continue;
            };
            let raw = value.trim();
            let value_clean = strip_template(raw);
            if !is_simple_value(&value_clean) {
                notes.push(format!(
                    "binding '{key_raw}' dropped (unsupported value '{raw}')"
                ));
                continue;
            }
            let Some(key) = sanitize_identifier(key_raw) else {
                notes.push(format!("binding '{key_raw}' dropped (invalid key)"));
                continue;
            };
            let parts: Vec<&str> = value_clean.split('.').collect();
            if parts.len() > 2 {
                let projected = parts[parts.len() - 2..].join(".");
                notes.push(format!(
                    "binding '{key}' path '{value_clean}' projected to '{projected}' (DSL binds at most two segments)"
                ));
                bindings.push((key, projected));
            } else {
                bindings.push((key, value_clean));
            }
        }
    }
    Nav {
        target,
        bindings,
        notes,
    }
}

/// Best-effort view name from a goto path: PascalCase the alpha path
/// segments, dropping template interpolations and pure-numeric segments.
fn target_name(path: &str) -> String {
    path.split(['/', '-', '_', '.'])
        .filter(|seg| !seg.is_empty())
        .filter(|seg| {
            !seg.contains('$')
                && !seg.contains('{')
                && !seg.contains('}')
                && !seg.chars().all(|c| c.is_ascii_digit())
        })
        .map(to_pascal_case)
        .collect()
}

/// `data: X;` guess from a fetch URL: confident only when the path (after an
/// optional `/api/v<N>` prefix) is a single plain lowercase segment.
#[cfg(test)]
fn fetch_entity_from_url(url: &str) -> Option<String> {
    fetch_parts_from_url(url).map(|(entity, _)| entity)
}

/// The confident fetch's `(entity, raw url segment)` pair.
fn fetch_parts_from_url(url: &str) -> Option<(String, String)> {
    let segment = fetch_segment_from_url(url)?;
    Some((singularize(&to_pascal_case(&segment)), segment))
}

fn fetch_segment_from_url(url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(url);
    if path.contains('$') {
        // Parameterized (template-interpolated) URLs are not confident.
        return None;
    }
    let segs: Vec<&str> = path
        .split('/')
        .filter(|seg| !seg.is_empty())
        .filter(|seg| !seg.contains('$'))
        .collect();
    let mut idx = 0usize;
    if segs.first().is_some_and(|seg| *seg == "api") {
        idx = 1;
        if let Some(v) = segs.get(1) {
            if v.len() >= 2 && v.starts_with('v') && v[1..].chars().all(|c| c.is_ascii_digit()) {
                idx = 2;
            }
        }
    }
    let rest = &segs[idx..];
    if rest.len() != 1 {
        return None;
    }
    let seg = rest[0];
    if seg.len() < 3
        || !seg
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return None;
    }
    Some(seg.to_string())
}

/// The fetch story for a script: none, one confident entity (with its raw
/// URL segment), or several distinct ones.
fn fetch_scan(script: &str) -> FetchScan {
    let mut found: Vec<(String, String)> = Vec::new();
    for arg in fetch_call_args(script) {
        if let Some(parts) = fetch_parts_from_url(&arg) {
            if !found.iter().any(|(entity, _)| *entity == parts.0) {
                found.push(parts);
            }
        }
    }
    match found.len() {
        0 => FetchScan::None,
        1 => {
            let (entity, segment) = found.remove(0);
            FetchScan::Single { entity, segment }
        }
        _ => FetchScan::Multiple(found.into_iter().map(|(entity, _)| entity).collect()),
    }
}

/// The string argument of every `fetch(...)` call in a script.
fn fetch_call_args(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = script.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = script[search_from..].find("fetch") {
        let at = search_from + rel;
        let after = at + "fetch".len();
        search_from = after;
        let prev_ok = at == 0 || !is_ident_char(bytes[at - 1]);
        let next_ok = after < bytes.len() && !is_ident_char(bytes[after]);
        if !prev_ok || !next_ok {
            continue;
        }
        let mut j = after;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'(' {
            continue;
        }
        let mut q = j + 1;
        while q < bytes.len() && !matches!(bytes[q], b'"' | b'\'' | b'`') {
            q += 1;
        }
        if q >= bytes.len() {
            continue;
        }
        let quote = bytes[q];
        let mut e = q + 1;
        while e < bytes.len() && bytes[e] != quote {
            if bytes[e] == b'\\' {
                e += 1;
            }
            e += 1;
        }
        let Some(arg) = script.get(q + 1..e.min(bytes.len())) else {
            continue;
        };
        out.push(arg.to_string());
    }
    out
}

fn render_nav(nav: &Nav) -> String {
    if nav.bindings.is_empty() {
        format!("navigate(\"{}\")", nav.target)
    } else {
        let pairs: Vec<String> = nav
            .bindings
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();
        format!("navigate(\"{}\", {{ {} }})", nav.target, pairs.join(", "))
    }
}

pub fn render_model(domain_name: &str, views: &[DerivedView]) -> String {
    let mut out = String::new();
    out.push_str(
        "// IFML model reverse-derived from SvelteKit pages by `codegraph ifml-derive`.\n",
    );
    out.push_str(
        "// Spike inference subset; skipped constructs are reported on stderr at derive time.\n\n",
    );
    out.push_str(&format!(
        "domain \"{domain_name}\" {{\n    schema \"{domain_name}\";\n}}\n\n"
    ));
    for view in views {
        render_view(&mut out, view);
    }
    out
}

fn render_view(out: &mut String, view: &DerivedView) {
    out.push_str(&format!("view \"{}\" {{\n", view.name));
    out.push_str(&format!("    label \"{}\";\n\n", view.name));
    if let Some(data) = &view.data {
        out.push_str(&format!("    data: {data};\n\n"));
    }
    if view.container {
        out.push_str("    container \"Main\" {\n");
        render_body(out, "        ", view);
        out.push_str("    }\n}\n\n");
    } else {
        render_body(out, "    ", view);
        out.push_str("}\n\n");
    }
}

fn render_body(out: &mut String, indent: &str, view: &DerivedView) {
    for comp in &view.components {
        out.push_str(&format!("{indent}component \"{}\" {{\n", comp.name));
        out.push_str(&format!("{indent}    type: {};\n", comp.kind.as_str()));
        if let Some(data) = &comp.data {
            out.push_str(&format!("{indent}    data: {data};\n"));
        }
        if !comp.fields.is_empty() {
            out.push('\n');
            for field in &comp.fields {
                if field.required {
                    out.push_str(&format!(
                        "{indent}    field {} -> input {} {{ required: true; }}\n",
                        field.name, field.input
                    ));
                } else {
                    out.push_str(&format!(
                        "{indent}    field {} -> input {};\n",
                        field.name, field.input
                    ));
                }
            }
        }
        if !comp.events.is_empty() {
            out.push('\n');
            for event in &comp.events {
                out.push_str(&format!("{indent}    {event}\n"));
            }
        }
        out.push_str(&format!("{indent}}}\n"));
    }
    if !view.events.is_empty() {
        if !view.components.is_empty() {
            out.push('\n');
        }
        for event in &view.events {
            out.push_str(&format!("{indent}{event}\n"));
        }
    }
}

/// Route path → view name: dynamic `[..]` and group `(..)` segments stripped,
/// remaining segments PascalCased (via `codegraph-naming`) and joined; the
/// root route → `Index`.
pub fn view_name_from_route(route: &str) -> String {
    let cleaned = strip_group_chars(&strip_group_chars(route, '[', ']'), '(', ')');
    let joined: String = cleaned
        .split(['/', '-', '_', '.'])
        .filter(|seg| !seg.is_empty())
        .map(to_pascal_case)
        .collect();
    if joined.is_empty() {
        "Index".to_string()
    } else {
        joined
    }
}

fn strip_group_chars(s: &str, open: char, close: char) -> String {
    let mut depth = 0u32;
    let mut out = String::new();
    for c in s.chars() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth = depth.saturating_sub(1);
        } else if depth == 0 {
            out.push(c);
        }
    }
    out
}

/// Plural words that must not be singularized (irregular or invariant).
const SINGULAR_EXCEPTIONS: &[&str] = &["news", "series", "species"];

/// Table-driven plural stripping for `data:` entity stems, applied in order:
/// 1. `SINGULAR_EXCEPTIONS` stay unchanged (News, Series, Species);
/// 2. `ies` → `y` (Companies → Company);
/// 3. `ses` → `s` (Addresses → Address, Statuses → Status, Buses → Bus);
/// 4. `xes` → `x` (Boxes → Box, Taxes → Tax);
/// 5. `ss`/`us` endings and words shorter than 4 chars stay unchanged
///    (Address, Status, Bus);
/// 6. any other trailing `s` is stripped (Customers → Customer).
///
/// Accepted mis-projections: `ses` words that are not possessive plurals
/// (`houses` → `hous`) and non-`xes`/`ies` `es` plurals (`boxes`-style only
/// is handled).
fn singularize(word: &str) -> String {
    if SINGULAR_EXCEPTIONS.contains(&word.to_ascii_lowercase().as_str()) {
        return word.to_string();
    }
    if let Some(stem) = word.strip_suffix("ies") {
        if !stem.is_empty() {
            return format!("{stem}y");
        }
    }
    if word.len() >= 4 {
        if let Some(stem) = word.strip_suffix("ses") {
            return format!("{stem}s");
        }
        if let Some(stem) = word.strip_suffix("xes") {
            return format!("{stem}x");
        }
    }
    if word.ends_with("ss") || word.ends_with("us") || word.len() < 4 {
        return word.to_string();
    }
    word.strip_suffix('s').unwrap_or(word).to_string()
}

fn sanitize_identifier(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let first = cleaned.chars().next()?;
    if first.is_ascii_alphabetic() {
        Some(cleaned)
    } else {
        Some(format!("f{cleaned}"))
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

fn is_ident_char(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

fn leading_ident_len(s: &str) -> usize {
    s.bytes().take_while(|b| is_ident_char(*b)).count()
}

fn is_simple_value(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

fn strip_template(value: &str) -> String {
    value.replace("${", "").replace('}', "").trim().to_string()
}

fn direct_children<'t>(node: Node<'t>, kind: &str) -> Vec<Node<'t>> {
    direct_children_matching(node, |child_kind| child_kind == kind)
}

fn all_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    direct_children_matching(node, |_| true)
}

fn direct_child<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    direct_children(node, kind).into_iter().next()
}

/// All `element` descendants (including the node itself when applicable),
/// recursing through Svelte block constructs (`{#each}`, `{#if}`, ...).
fn descendant_elements(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.children(&mut cursor).collect();
    let mut out = Vec::new();
    for child in children {
        if child.kind() == "element" {
            out.push(child);
        }
        out.extend(descendant_elements(child));
    }
    out
}

/// All `each_statement` descendants of a node, nested ones included.
fn collect_each_statements(node: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    if node.kind() == "each_statement" {
        out.push(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        out.extend(collect_each_statements(child));
    }
    out
}

fn contains_each_statement(node: Node<'_>) -> bool {
    if node.kind() == "each_statement" {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_each_statement(child) {
            return true;
        }
    }
    false
}

/// The `(iterated expression, loop variable)` of an `{#each X as Y}` block.
fn each_parts(each: Node<'_>, source: &str) -> Option<(Option<String>, Option<String>)> {
    let start = direct_child(each, "each_start")?;
    let field_text = |name: &str| {
        start
            .child_by_field_name(name)
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(str::trim)
            .map(String::from)
            .filter(|t| !t.is_empty())
    };
    Some((field_text("identifier"), field_text("parameter")))
}

fn each_parameter(each: Node<'_>, source: &str) -> Option<String> {
    each_parts(each, source)?.1
}

fn is_generic_loop_var(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "item"
            | "items"
            | "row"
            | "rows"
            | "entry"
            | "record"
            | "element"
            | "el"
            | "it"
            | "obj"
            | "x"
            | "data"
    )
}

fn tag_of<'a>(element: Node<'_>, source: &'a str) -> Option<&'a str> {
    let mut cursor = element.walk();
    let children: Vec<Node<'_>> = element.children(&mut cursor).collect();
    for child in children {
        if child.kind() != "start_tag" && child.kind() != "self_closing_tag" {
            continue;
        }
        let mut tag_cursor = child.walk();
        let tag_children: Vec<Node<'_>> = child.children(&mut tag_cursor).collect();
        for tag_child in tag_children {
            if tag_child.kind() == "tag_name" {
                return tag_child.utf8_text(source.as_bytes()).ok();
            }
        }
    }
    None
}

fn raw_text_of<'a>(element: Node<'_>, source: &'a str) -> Option<&'a str> {
    let mut cursor = element.walk();
    let children: Vec<Node<'_>> = element.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "raw_text" {
            return child.utf8_text(source.as_bytes()).ok();
        }
    }
    None
}

/// Attributes of an element as `(name, Option<value>)` pairs. Values are the
/// quoted text for plain attributes and the inner expression text for
/// `{...}` interpolations; bare attributes have `None`.
fn element_attributes(element: Node<'_>, source: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let tags = direct_children_matching(element, |kind| {
        kind == "start_tag" || kind == "self_closing_tag"
    });
    for tag in tags {
        for attr in direct_children(tag, "attribute") {
            let mut name: Option<String> = None;
            let mut value: Option<String> = None;
            for part in all_children(attr) {
                match part.kind() {
                    "attribute_name" => {
                        name = part.utf8_text(source.as_bytes()).ok().map(String::from);
                    }
                    "quoted_attribute_value" => {
                        for inner in all_children(part) {
                            if inner.kind() == "attribute_value" {
                                value = inner.utf8_text(source.as_bytes()).ok().map(String::from);
                            }
                        }
                    }
                    "expression" => {
                        for inner in all_children(part) {
                            if inner.kind() == "svelte_raw_text" {
                                value = inner.utf8_text(source.as_bytes()).ok().map(String::from);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if let Some(name) = name {
                out.push((name, value));
            }
        }
    }
    out
}

fn direct_children_matching<'t>(node: Node<'t>, pred: impl Fn(&str) -> bool) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| pred(child.kind()))
        .collect()
}

fn attr_value(element: Node<'_>, source: &str, name: &str) -> Option<String> {
    element_attributes(element, source)
        .into_iter()
        .find(|(key, _)| key == name)
        .and_then(|(_, value)| value)
}

fn attr_value_matching(
    element: Node<'_>,
    source: &str,
    pred: impl Fn(&str) -> bool,
) -> Option<String> {
    element_attributes(element, source)
        .into_iter()
        .find(|(key, _)| pred(key))
        .and_then(|(_, value)| value)
}

fn has_attr(element: Node<'_>, source: &str, name: &str) -> bool {
    element_attributes(element, source)
        .iter()
        .any(|(key, _)| key == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_name_pascal_cases_path_segments() {
        assert_eq!(target_name("/customer-detail"), "CustomerDetail");
        assert_eq!(target_name("/todolist"), "Todolist");
        assert_eq!(target_name("/customers/42"), "Customers");
        assert_eq!(target_name("/customers/${row.id}"), "Customers");
    }

    #[test]
    fn view_names_from_routes() {
        assert_eq!(view_name_from_route(""), "Index");
        assert_eq!(view_name_from_route("todo-editor"), "TodoEditor");
        assert_eq!(view_name_from_route("customers/new"), "CustomersNew");
        assert_eq!(view_name_from_route("orders/[id]"), "Orders");
        assert_eq!(
            view_name_from_route("customer_addresses"),
            "CustomerAddresses"
        );
    }

    #[test]
    fn target_names_route_through_naming_helpers() {
        assert_eq!(target_name("/customer-detail"), "CustomerDetail");
        assert_eq!(target_name("/todo_editor"), "TodoEditor");
    }

    #[test]
    fn fetch_entities_single_segment_only() {
        assert_eq!(
            fetch_entity_from_url("/api/v1/todo-lists"),
            Some("TodoList".into())
        );
        assert_eq!(
            fetch_entity_from_url("/api/v2/customers"),
            Some("Customer".into())
        );
        assert_eq!(fetch_entity_from_url("/customers"), Some("Customer".into()));
        assert_eq!(fetch_entity_from_url("/api/v1/x/y"), None);
        assert_eq!(fetch_entity_from_url("/api/v1/customers/${id}"), None);
        assert_eq!(
            fetch_entity_from_url("/api/v1/status"),
            Some("Status".into())
        );
    }

    #[test]
    fn fetch_entity_singularization_is_table_driven() {
        assert_eq!(
            fetch_entity_from_url("/api/v1/addresses"),
            Some("Address".into())
        );
        assert_eq!(
            fetch_entity_from_url("/api/v1/statuses"),
            Some("Status".into())
        );
        assert_eq!(
            fetch_entity_from_url("/api/v1/companies"),
            Some("Company".into())
        );
        assert_eq!(
            fetch_entity_from_url("/api/v1/tax-boxes"),
            Some("TaxBox".into())
        );
        assert_eq!(fetch_entity_from_url("/api/v1/news"), Some("News".into()));
    }

    #[test]
    fn handler_event_kinds() {
        assert_eq!(event_kind_for_handler("submit_editor"), "save");
        assert_eq!(event_kind_for_handler("save_x"), "save");
        assert_eq!(event_kind_for_handler("cancel_edit"), "cancel");
        assert_eq!(event_kind_for_handler("back_to_list"), "cancel");
        assert_eq!(event_kind_for_handler("refresh_list"), "click");
    }

    #[test]
    fn handler_names_from_expressions() {
        assert_eq!(
            handler_name_from_expr("submit_editor"),
            Some("submit_editor".to_string())
        );
        assert_eq!(
            handler_name_from_expr("() => open_detail(item)"),
            Some("open_detail".to_string())
        );
        assert_eq!(handler_name_from_expr("() => flag = !flag"), None);
    }

    #[test]
    fn goto_bindings_require_simple_identifiers() {
        let nav = find_first_goto("function f() { goto('/customerdetail?customerId=${row.id}'); }")
            .expect("goto found");
        assert_eq!(nav.target, "Customerdetail");
        assert_eq!(
            nav.bindings,
            vec![("customerId".to_string(), "row.id".to_string())]
        );

        let nav = find_first_goto("goto('/todolist')").expect("goto found");
        assert_eq!(nav.target, "Todolist");
        assert!(nav.bindings.is_empty());

        let nav = find_first_goto("goto('/x?a=1+2')").expect("goto found");
        assert_eq!(nav.target, "X");
        assert!(
            nav.bindings.is_empty(),
            "non-identifier values drop bindings"
        );
    }

    #[test]
    fn singularize_naive_rules() {
        assert_eq!(singularize("Customers"), "Customer");
        assert_eq!(singularize("TodoLists"), "TodoList");
        assert_eq!(singularize("Status"), "Status");
    }

    /// The singularization table (issue #203 upgrade 3), applied in order:
    /// 1. exceptions (irregular / already singular despite the trailing s)
    ///    stay unchanged: news, series, species;
    /// 2. `ies` → `y` (Companies → Company);
    /// 3. `ses` → `s` (Addresses → Address, Statuses → Status, Buses → Bus)
    ///    — note `houses` → `hous` is an accepted spike mis-projection;
    /// 4. `xes` → `x` (Boxes → Box, Taxes → Tax);
    /// 5. `ss`/`us` endings and words shorter than 4 chars stay unchanged
    ///    (Address, Status, Bus);
    /// 6. other trailing `s` is stripped (Customers → Customer).
    #[test]
    fn singularize_table_rules() {
        assert_eq!(singularize("Addresses"), "Address");
        assert_eq!(singularize("Statuses"), "Status");
        assert_eq!(singularize("Buses"), "Bus");
        assert_eq!(singularize("Companies"), "Company");
        assert_eq!(singularize("Boxes"), "Box");
        assert_eq!(singularize("Taxes"), "Tax");
        assert_eq!(singularize("News"), "News");
        assert_eq!(singularize("Series"), "Series");
        assert_eq!(singularize("Species"), "Species");
        assert_eq!(singularize("Address"), "Address");
        assert_eq!(singularize("Bus"), "Bus");
        assert_eq!(singularize("Campus"), "Campus");
        assert_eq!(singularize("Todo"), "Todo");
    }

    #[test]
    fn arrow_const_handlers_are_extracted() {
        let script = r#"
	let title = $state('');
	const save = async (event: SubmitEvent) => {
		goto('/todolist');
	};
	const cancel = () => goto('/todolist');
	const initial = await fetch('/api/v1/customers');
	const flagged = items.filter((x) => x.on);
"#;
        let functions = extract_functions(script);
        let save = functions
            .get("save")
            .expect("const arrow handler must be extracted");
        assert!(save.contains("goto('/todolist')"));
        let cancel = functions
            .get("cancel")
            .expect("expression-bodied const arrow must be extracted");
        assert!(cancel.contains("goto('/todolist')"));
        assert!(
            !functions.contains_key("initial"),
            "non-arrow consts are not handlers"
        );
        assert!(
            !functions.contains_key("flagged"),
            "nested arrows are not top-level handlers"
        );
    }

    #[test]
    fn deep_bindings_project_to_last_two_segments_with_notes() {
        let nav = find_first_goto("goto('/x?rid=${row.org.primaryId}')").expect("goto found");
        assert_eq!(nav.target, "X");
        assert_eq!(
            nav.bindings,
            vec![("rid".to_string(), "org.primaryId".to_string())]
        );
        assert!(
            nav.notes
                .iter()
                .any(|n| n.contains("row.org.primaryId") && n.contains("org.primaryId")),
            "projection must be noted: {:?}",
            nav.notes
        );

        let nav = find_first_goto("goto('/x?a=1+2&b=ok')").expect("goto found");
        assert_eq!(
            nav.bindings,
            vec![("b".to_string(), "ok".to_string())],
            "only the non-identifier binding drops"
        );
        assert!(
            nav.notes
                .iter()
                .any(|n| n.contains("'a'") && n.contains("1+2")),
            "dropped binding must be noted: {:?}",
            nav.notes
        );
    }

    #[test]
    fn simple_value_checks() {
        assert!(is_simple_value("abc"));
        assert!(is_simple_value("row.id"));
        assert!(!is_simple_value("a + b"));
        assert!(!is_simple_value(""));
        assert!(!is_simple_value("9abc"));
    }
}
