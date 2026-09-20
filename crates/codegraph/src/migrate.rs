//! `codegraph migrate`: convert a JSON Schema directory tree into rexlang
//! `.mox` domain sources plus the shared `codegraph_stdlib` package
//! (mox-first migration, epic #228 / sub-issue #234).
//!
//! Output is parse-verified with `rex_driver::compile_files` before anything
//! is written (the `ifml-scaffold` precedent): error-severity diagnostics
//! abort the run without writing unless `--force` is given. Every mapping
//! decision that is not confident lands in the needs-review list; anything
//! dropped entirely lands in the unsupported list.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::path::PathBuf;

use rex_driver::compile_files;

use crate::error::{Error, Result};
use crate::ingest::schema_loader::{SchemaEntry, SchemaLoader};

pub const STDLIB_PACKAGE: &str = "codegraph_stdlib";
pub const STDLIB_FILE_NAME: &str = "codegraph_stdlib.mox";
pub const STDLIB_SOURCE: &str = include_str!("migrate/stdlib.mox");

/// Hard rexlang keywords: legal identifiers only in escaped `^name` form.
const KEYWORDS: &[&str] = &[
    "package",
    "annotation",
    "as",
    "class",
    "extends",
    "interface",
    "enum",
    "type",
    "wraps",
    "opaque",
    "contains",
    "refers",
    "container",
    "opposite",
    "op",
    "derived",
    "true",
    "false",
    "vocabulary",
    "from",
    "version",
    "key",
    "facet",
    "actors",
    "actor",
    "agent",
    "capability",
    "grant",
    "permit",
    "forbid",
    "when",
    "obligation",
    "on",
    "never_both",
    "delegation",
    "purpose",
    "cedar",
    "import",
];

/// JSON Schema string formats with a `codegraph_stdlib` datatype counterpart.
const FORMAT_TYPES: &[(&str, &str)] = &[
    ("uuid", "Uuid"),
    ("date-time", "DateTime"),
    ("date", "Date"),
    ("email", "Email"),
    ("uri", "Uri"),
];

pub struct MigrateArgs<'a> {
    pub schemas: &'a Path,
    pub output: &'a Path,
    pub dry_run: bool,
    pub force: bool,
}

#[derive(Debug, Default)]
pub struct MigrateReport {
    /// Files written (or planned, under `--dry-run`), in emission order.
    pub files: Vec<PathBuf>,
    /// Top-level schemas converted to classes or enums.
    pub converted: usize,
    /// Mapping decisions taken that a human should double-check.
    pub needs_review: Vec<String>,
    /// Source constructs dropped entirely.
    pub unsupported: Vec<String>,
    /// Error-severity diagnostics from the verification compile.
    pub compile_errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclKind {
    Class,
    Enum,
}

struct PlannedDecl<'a> {
    entry: &'a SchemaEntry,
    name: String,
    kind: DeclKind,
}

/// Shared emitter state for one `migrate` run.
struct Emitter<'a> {
    loader: &'a SchemaLoader,
    /// Every registered declaration name (bare text) across all packages.
    names: &'a BTreeSet<String>,
    /// Declaration name -> kind, for default/constraint family decisions.
    kinds: &'a HashMap<String, DeclKind>,
    /// Emitted declaration lookup: entry `rel_path` and stem -> (name, kind).
    by_path: &'a HashMap<String, (String, DeclKind)>,
    by_stem: &'a HashMap<String, (String, DeclKind)>,
    /// Declared enum name -> bare literal names, for enum-typed defaults.
    enum_literals: HashMap<String, Vec<String>>,
    needs_review: Vec<String>,
    unsupported: Vec<String>,
}

/// Run the `codegraph migrate` command: convert every top-level schema into
/// a rexlang class or enum, one `<domain>.mox` package per schema domain.
pub fn migrate(args: MigrateArgs<'_>) -> Result<MigrateReport> {
    let loader = SchemaLoader::load(args.schemas)?;

    let mut by_domain: BTreeMap<String, Vec<&SchemaEntry>> = BTreeMap::new();
    for (_, entry) in loader.iter_top_level() {
        by_domain
            .entry(entry.domain.clone())
            .or_default()
            .push(entry);
    }
    if by_domain.is_empty() {
        return Err(Error::Config(format!(
            "no JSON schemas found under '{}'",
            args.schemas.display()
        )));
    }

    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut kinds: HashMap<String, DeclKind> = HashMap::new();
    let mut by_path: HashMap<String, (String, DeclKind)> = HashMap::new();
    let mut by_stem: HashMap<String, (String, DeclKind)> = HashMap::new();
    let mut planned: BTreeMap<String, Vec<PlannedDecl<'_>>> = BTreeMap::new();
    let mut report = MigrateReport::default();

    // Package names are per-domain sanitized qname segments; two raw domains
    // colliding after sanitization merge into one package (noted for review).
    let mut package_of_domain: HashMap<String, String> = HashMap::new();
    let mut package_owner: BTreeMap<String, String> = BTreeMap::new();
    for domain in by_domain.keys() {
        let package = match sanitize_identifier(domain) {
            Some(name) => name,
            None => {
                report.unsupported.push(format!(
                    "domain '{domain}' skipped: name is not a usable package identifier"
                ));
                continue;
            }
        };
        if let Some(first) = package_owner.get(&package) {
            report.needs_review.push(format!(
                "domain '{domain}' sanitizes to package '{package}', already used by domain '{first}'; their schemas share one file"
            ));
        } else {
            package_owner.insert(package.clone(), domain.clone());
        }
        package_of_domain.insert(domain.clone(), package);
    }

    // Pass A: register one declaration per top-level schema title. Titles are
    // deduplicated globally (first domain in sort order wins), matching the
    // cross-domain title dedup in compute_generation_order.
    let mut seen_titles: HashMap<String, String> = HashMap::new();
    for (domain, entries) in &by_domain {
        let Some(package) = package_of_domain.get(domain) else {
            continue;
        };
        let mut sorted: Vec<&&SchemaEntry> = entries.iter().collect();
        sorted.sort_by_key(|entry| title_of(entry));
        for entry in sorted {
            let title = title_of(entry);
            let Some(name) = sanitize_identifier(&title) else {
                report.unsupported.push(format!(
                    "schema '{title}' ({}) skipped: title is not a usable identifier",
                    entry.rel_path
                ));
                continue;
            };
            if let Some(first_domain) = seen_titles.get(title.as_str()) {
                report.needs_review.push(format!(
                    "schema title '{title}' already converted from domain '{first_domain}'; the copy in domain '{domain}' ({}) is skipped",
                    entry.rel_path
                ));
                continue;
            }
            if names.contains(&name) {
                report.needs_review.push(format!(
                    "schema '{title}' ({}) skipped: sanitized name '{name}' collides with an existing declaration",
                    entry.rel_path
                ));
                continue;
            }
            let kind = if is_enum_schema(&entry.schema) {
                DeclKind::Enum
            } else {
                DeclKind::Class
            };
            seen_titles.insert(title, domain.clone());
            names.insert(name.clone());
            kinds.insert(name.clone(), kind);
            by_path.insert(entry.rel_path.clone(), (name.clone(), kind));
            by_stem.insert(entry.stem.clone(), (name.clone(), kind));
            planned
                .entry(package.clone())
                .or_default()
                .push(PlannedDecl { entry, name, kind });
        }
    }
    if planned.is_empty() {
        return Err(Error::Config(
            "no schemas could be converted: every top-level schema was skipped".to_string(),
        ));
    }

    // Pass B: render every domain package.
    let mut rendered: Vec<(String, String)> = Vec::new();
    for (package, decls) in &planned {
        let mut emitter = Emitter {
            loader: &loader,
            names: &names,
            kinds: &kinds,
            by_path: &by_path,
            by_stem: &by_stem,
            enum_literals: HashMap::new(),
            needs_review: Vec::new(),
            unsupported: Vec::new(),
        };
        let mut body = String::new();
        let mut inline_enums = String::new();
        for decl in decls {
            match decl.kind {
                DeclKind::Enum => emitter.render_enum_decl(&mut body, decl),
                DeclKind::Class => emitter.render_class_decl(&mut body, &mut inline_enums, decl),
            }
            report.converted += 1;
        }
        body.push_str(&inline_enums);
        rendered.push((
            package.clone(),
            format!("package {package}\n\n{}\n", body.trim_end()),
        ));
        report.needs_review.append(&mut emitter.needs_review);
        report.unsupported.append(&mut emitter.unsupported);
    }
    report.needs_review.sort();
    report.unsupported.sort();

    let mut files: Vec<(String, String)> =
        vec![(STDLIB_FILE_NAME.to_string(), STDLIB_SOURCE.to_string())];
    files.extend(
        rendered
            .iter()
            .map(|(package, source)| (format!("{package}.mox"), source.clone())),
    );

    // Parse-verify everything (stdlib first) before writing anything.
    let sources: Vec<(String, String)> = files
        .iter()
        .map(|(name, source)| (format!("<output>/{name}"), source.clone()))
        .collect();
    let compilation = compile_files(&sources);
    for (path, diagnostic) in &compilation.diagnostics {
        if diagnostic.is_error() {
            report
                .compile_errors
                .push(format!("{path}: {}", diagnostic.message));
        } else {
            eprintln!("migrate: warning in {path}: {}", diagnostic.message);
        }
    }

    if !report.compile_errors.is_empty() && !args.dry_run && !args.force {
        return Err(Error::Config(format!(
            "generated .mox failed rex verification, nothing written:\n{}",
            report.compile_errors.join("\n")
        )));
    }

    let target_paths: Vec<PathBuf> = files
        .iter()
        .map(|(name, _)| args.output.join(name))
        .collect();
    if args.dry_run {
        report.files = target_paths;
    } else {
        if !args.force {
            let existing: Vec<String> = target_paths
                .iter()
                .filter(|path| path.exists())
                .map(|path| path.display().to_string())
                .collect();
            if !existing.is_empty() {
                return Err(Error::Config(format!(
                    "refusing to overwrite existing file(s): {} (use --force to overwrite)",
                    existing.join(", ")
                )));
            }
        }
        std::fs::create_dir_all(args.output)?;
        for ((_name, source), path) in files.iter().zip(&target_paths) {
            std::fs::write(path, source)?;
        }
        report.files = target_paths;
        if !report.compile_errors.is_empty() {
            eprintln!(
                "WARNING: --force wrote .mox files that FAILED rex verification (they will not ingest):"
            );
            for error in &report.compile_errors {
                eprintln!("  {error}");
            }
        }
        println!(
            "migrate: wrote {} file(s) to {} ({} declarations converted, {} needs review, {} unsupported)",
            files.len(),
            args.output.display(),
            report.converted,
            report.needs_review.len(),
            report.unsupported.len()
        );
    }

    for note in &report.needs_review {
        eprintln!("needs review: {note}");
    }
    for note in &report.unsupported {
        eprintln!("unsupported: {note}");
    }

    Ok(report)
}

impl Emitter<'_> {
    /// Resolve a `$ref` against a converted declaration.
    fn resolve_ref(&self, ref_str: &str, base_uri: &str) -> Option<(String, DeclKind)> {
        let (_, entry) = self.loader.resolve_ref(ref_str, base_uri).ok()?;
        self.by_path
            .get(&entry.rel_path)
            .or_else(|| self.by_stem.get(&entry.stem))
            .cloned()
    }

    fn render_enum_decl(&mut self, out: &mut String, decl: &PlannedDecl<'_>) {
        let schema = &decl.entry.schema;
        let mut body = String::new();
        let mut literals = Vec::new();
        if let Some(values) = schema.get("enum").and_then(|v| v.as_array()) {
            for (index, value) in values.iter().enumerate() {
                let Some(text) = value.as_str() else {
                    self.unsupported.push(format!(
                        "enum '{}' literal #{} is not a string and was skipped",
                        decl.name, index
                    ));
                    continue;
                };
                let Some(bare) = sanitize_identifier(text) else {
                    self.unsupported.push(format!(
                        "enum '{}' literal '{}' has no usable identifier form and was skipped",
                        decl.name, text
                    ));
                    continue;
                };
                if emit_name(&bare) == *text {
                    body.push_str(&format!("    {} = {index}\n", emit_name(&bare)));
                } else {
                    body.push_str(&format!(
                        "    {} as \"{}\" = {index}\n",
                        emit_name(&bare),
                        escape_mox_string(text)
                    ));
                }
                literals.push(bare);
            }
        }
        if body.is_empty() {
            self.unsupported.push(format!(
                "enum '{}' kept no literals and was skipped entirely",
                decl.name
            ));
            return;
        }
        if let Some(doc) = doc_line(schema.get("description")) {
            out.push_str(&format!("/// {doc}\n"));
        }
        out.push_str(&format!(
            "enum {} {{\n{}}}\n\n",
            emit_name(&decl.name),
            body
        ));
        self.enum_literals.insert(decl.name.clone(), literals);
    }

    fn render_class_decl(
        &mut self,
        out: &mut String,
        inline_enums: &mut String,
        decl: &PlannedDecl<'_>,
    ) {
        let schema = &decl.entry.schema;
        let (properties, required, extends_ref) = merged_members(schema);

        let mut extends = String::new();
        if let Some(ref_str) = &extends_ref {
            match self.resolve_ref(ref_str, &decl.entry.rel_path) {
                Some((name, DeclKind::Class)) => {
                    extends = format!(" extends {}", emit_name(&name));
                }
                Some((name, DeclKind::Enum)) => self.needs_review.push(format!(
                    "class '{}' extends '{name}', which is an enum; emitted without extends",
                    decl.name
                )),
                None => self.needs_review.push(format!(
                    "class '{}' extends target '{ref_str}' did not resolve to a converted schema; emitted without extends",
                    decl.name
                )),
            }
        } else if schema.get("allOf").is_some() {
            self.needs_review.push(format!(
                "class '{}' has an allOf with no $ref member; all member properties were merged into the class body",
                decl.name
            ));
        }

        if let Some(doc) = doc_line(schema.get("description")) {
            out.push_str(&format!("/// {doc}\n"));
        }
        out.push_str(&format!("class {}{extends} {{\n", emit_name(&decl.name)));
        for (prop_name, prop_schema) in &properties {
            if let Some(line) = self.render_feature(
                &decl.name,
                &decl.entry.rel_path,
                prop_name,
                prop_schema,
                &required,
                inline_enums,
            ) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out.push_str("}\n\n");
    }

    /// Renders one property as a class feature line, or `None` when the
    /// property had to be skipped (a note is recorded either way).
    #[allow(clippy::too_many_arguments)]
    fn render_feature(
        &mut self,
        class_name: &str,
        base_uri: &str,
        prop_name: &str,
        prop_schema: &serde_json::Value,
        required: &BTreeSet<String>,
        inline_enums: &mut String,
    ) -> Option<String> {
        let is_required = required.contains(prop_name);
        let Some(prop_ident) = sanitize_identifier(prop_name) else {
            self.unsupported.push(format!(
                "property '{class_name}.{prop_name}' skipped: name is empty"
            ));
            return None;
        };
        if prop_ident != prop_name {
            self.needs_review.push(format!(
                "property '{class_name}.{prop_name}' sanitized to '{prop_ident}'"
            ));
        }

        let is_array = plain_type(prop_schema).as_deref() == Some("array");
        let item_schema = if is_array {
            prop_schema.get("items").unwrap_or(&serde_json::Value::Null)
        } else {
            prop_schema
        };

        let slot = self.resolve_slot(
            class_name,
            base_uri,
            prop_name,
            prop_schema,
            item_schema,
            inline_enums,
        )?;

        let name = emit_name(&prop_ident);
        let doc = doc_line(prop_schema.get("description"));
        let multiplicity = match (is_array, is_required) {
            (true, true) => "[]",
            (true, false) => "[0..*]",
            (false, true) => "",
            (false, false) => "[0..1]",
        };
        let line = match slot {
            Slot::Ref(target) => {
                if schema_has_constraints(item_schema) {
                    self.needs_review.push(format!(
                        "property '{class_name}.{prop_name}' declares constraints on a reference; refers features take no constraints and they were dropped"
                    ));
                }
                format!("    refers {}{} {}", emit_name(&target), multiplicity, name)
            }
            Slot::Attr(type_expr) => {
                let default = self.render_default(class_name, prop_name, prop_schema, &type_expr);
                let constraints =
                    self.render_constraints(class_name, prop_name, item_schema, &type_expr);
                format!("    {type_expr}{multiplicity} {name}{default}{constraints}")
            }
        };
        Some(with_doc(doc, line))
    }

    /// Decide what a property is: an attribute (primitive, stdlib datatype,
    /// or enum) or a reference to a converted class.
    fn resolve_slot(
        &mut self,
        class_name: &str,
        base_uri: &str,
        prop_name: &str,
        prop_schema: &serde_json::Value,
        item_schema: &serde_json::Value,
        inline_enums: &mut String,
    ) -> Option<Slot> {
        // `$ref` may sit on the property itself or, for arrays, on `items`.
        let ref_owner = if prop_schema.get("$ref").is_some() || prop_schema.get("allOf").is_some() {
            Some(prop_schema)
        } else if item_schema.get("$ref").is_some() || item_schema.get("allOf").is_some() {
            Some(item_schema)
        } else {
            None
        };
        let ref_str = ref_owner.and_then(|owner| {
            owner
                .get("$ref")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    owner
                        .get("allOf")
                        .and_then(|v| v.as_array())
                        .and_then(|members| {
                            members
                                .iter()
                                .find_map(|member| member.get("$ref").and_then(|v| v.as_str()))
                        })
                        .map(String::from)
                })
        });
        if ref_owner == Some(prop_schema) && prop_schema.get("$ref").is_none() {
            self.needs_review.push(format!(
                "property '{class_name}.{prop_name}' uses allOf; only its first $ref was kept"
            ));
        }
        if let Some(ref_str) = ref_str {
            return match self.resolve_ref(&ref_str, base_uri) {
                Some((name, DeclKind::Class)) => Some(Slot::Ref(name)),
                Some((name, DeclKind::Enum)) => Some(Slot::Attr(name)),
                None => {
                    self.needs_review.push(format!(
                        "property '{class_name}.{prop_name}' references '{ref_str}', which is not a converted top-level schema; the property is skipped"
                    ));
                    None
                }
            };
        }

        if let Some(values) = item_schema.get("enum").and_then(|v| v.as_array()) {
            return match string_enum_values(values) {
                Some(strings) => {
                    match self.inline_enum(class_name, prop_name, &strings, inline_enums) {
                        Some(name) => Some(Slot::Attr(name)),
                        None => {
                            self.needs_review.push(format!(
                            "property '{class_name}.{prop_name}' has an enum that could not be declared; emitted as String"
                        ));
                            Some(Slot::Attr("String".to_string()))
                        }
                    }
                }
                None => {
                    self.needs_review.push(format!(
                        "property '{class_name}.{prop_name}' has an enum with non-string or empty values; emitted as String"
                    ));
                    Some(Slot::Attr("String".to_string()))
                }
            };
        }

        match plain_type(item_schema).as_deref() {
            Some("string") => {
                let format = item_schema.get("format").and_then(|v| v.as_str());
                match format.and_then(stdlib_type_for_format) {
                    Some(stdlib) => Some(Slot::Attr(format!("{STDLIB_PACKAGE}.{stdlib}"))),
                    None => {
                        if let Some(format) = format {
                            self.needs_review.push(format!(
                                "property '{class_name}.{prop_name}' has format '{format}' with no stdlib type; emitted as String"
                            ));
                        }
                        Some(Slot::Attr("String".to_string()))
                    }
                }
            }
            Some("integer") => Some(Slot::Attr("Int".to_string())),
            Some("number") => Some(Slot::Attr("Double".to_string())),
            Some("boolean") => Some(Slot::Attr("Boolean".to_string())),
            Some("object") => {
                self.needs_review.push(format!(
                    "property '{class_name}.{prop_name}' is an inline object; named-class extraction is out of scope and the property is skipped"
                ));
                None
            }
            other => {
                self.needs_review.push(format!(
                    "property '{class_name}.{prop_name}' has type {:?} with no mox mapping; the property is skipped",
                    other.map(str::to_string)
                ));
                None
            }
        }
    }

    /// Declares (once) an enum for an inline string `enum` and returns its
    /// name. Values are kept verbatim when identifier-safe; otherwise the
    /// sanitized literal carries an `as "<original>"` label. Keyword values
    /// are emitted in escaped `^name` form. Vocabulary was considered and
    /// rejected: the rex grammar requires `from "source"`.
    fn inline_enum(
        &mut self,
        class_name: &str,
        prop_name: &str,
        values: &[String],
        inline_enums: &mut String,
    ) -> Option<String> {
        let candidate = format!("{class_name}{}", to_pascal(prop_name));
        let name = sanitize_identifier(&candidate)?;
        if self.names.contains(&name) {
            return None;
        }
        let mut body = String::new();
        let mut literals = Vec::new();
        let mut used = HashSet::new();
        for (index, value) in values.iter().enumerate() {
            let Some(bare) = sanitize_identifier(value) else {
                self.unsupported.push(format!(
                    "enum '{name}' literal '{value}' has no usable identifier form and was skipped"
                ));
                continue;
            };
            if !used.insert(bare.clone()) {
                self.needs_review.push(format!(
                    "enum '{name}' literal '{value}' collides with an earlier literal after sanitization and was skipped"
                ));
                continue;
            }
            if emit_name(&bare) == *value {
                body.push_str(&format!("    {} = {index}\n", emit_name(&bare)));
            } else {
                body.push_str(&format!(
                    "    {} as \"{}\" = {index}\n",
                    emit_name(&bare),
                    escape_mox_string(value)
                ));
            }
            literals.push(bare);
        }
        if body.is_empty() {
            return None;
        }
        inline_enums.push_str(&format!("enum {} {{\n{}}}\n\n", emit_name(&name), body));
        self.enum_literals.insert(name.clone(), literals);
        Some(name)
    }

    /// Renders ` = <value>` for string/integer/boolean defaults. Enum-typed
    /// defaults bind to a literal name when the raw value matches one.
    fn render_default(
        &mut self,
        class_name: &str,
        prop_name: &str,
        prop_schema: &serde_json::Value,
        type_expr: &str,
    ) -> String {
        let Some(default) = prop_schema.get("default") else {
            return String::new();
        };
        if is_array_type(prop_schema) {
            self.needs_review.push(format!(
                "property '{class_name}.{prop_name}' has an array default; mox defaults are single-valued and it was dropped"
            ));
            return String::new();
        }
        let is_enum = self.kinds.get(type_expr) == Some(&DeclKind::Enum);
        let rendered = match default {
            serde_json::Value::String(value) => {
                if is_enum {
                    let bare = sanitize_identifier(value);
                    let binds = bare.as_ref().is_some_and(|bare| {
                        self.enum_literals
                            .get(type_expr)
                            .is_some_and(|literals| literals.contains(bare))
                    });
                    if let (true, Some(bare)) = (binds, bare) {
                        Some(format!(" = {}", emit_name(&bare)))
                    } else {
                        self.needs_review.push(format!(
                            "property '{class_name}.{prop_name}' has enum default '{value}' with no matching literal; the default was dropped"
                        ));
                        None
                    }
                } else if type_expr == "String" {
                    Some(format!(" = \"{}\"", escape_mox_string(value)))
                } else {
                    self.needs_review.push(format!(
                        "property '{class_name}.{prop_name}' has a string default on a {type_expr} attribute; only String and enum defaults are supported and it was dropped"
                    ));
                    None
                }
            }
            serde_json::Value::Number(value) => match value.as_i64() {
                Some(int) if type_expr == "Int" => Some(format!(" = {int}")),
                Some(int) => {
                    self.needs_review.push(format!(
                        "property '{class_name}.{prop_name}' has an integer default on a {type_expr} attribute; the default was dropped"
                    ));
                    let _ = int;
                    None
                }
                None => {
                    self.needs_review.push(format!(
                        "property '{class_name}.{prop_name}' has a non-integer numeric default; rex has no float literal default and it was dropped"
                    ));
                    None
                }
            },
            serde_json::Value::Bool(value) => {
                if type_expr == "Boolean" {
                    Some(format!(" = {value}"))
                } else {
                    self.needs_review.push(format!(
                        "property '{class_name}.{prop_name}' has a boolean default on a {type_expr} attribute; the default was dropped"
                    ));
                    None
                }
            }
            _ => {
                self.needs_review.push(format!(
                    "property '{class_name}.{prop_name}' has a non-scalar default; it was dropped"
                ));
                None
            }
        };
        rendered.unwrap_or_default()
    }

    /// Renders the `{ ... }` constraint block. Family rules mirror the rex
    /// lowerer, where a family mismatch is an error: string family
    /// (pattern/minLength/maxLength) only on String and stdlib-datatype
    /// attributes, numeric family (minimum/maximum) only on Int/Double, and
    /// none on Boolean/enum/reference targets.
    fn render_constraints(
        &mut self,
        class_name: &str,
        prop_name: &str,
        item_schema: &serde_json::Value,
        type_expr: &str,
    ) -> String {
        let constraints = constraint_values(item_schema);
        for note in &constraints.dropped_notes {
            self.needs_review
                .push(format!("property '{class_name}.{prop_name}': {note}"));
        }
        let string_family = type_expr == "String" || is_stdlib_type(type_expr);
        let numeric_family = type_expr == "Int" || type_expr == "Double";
        if !string_family && !numeric_family {
            if constraints.has_any() {
                self.needs_review.push(format!(
                    "property '{class_name}.{prop_name}' declares constraints on a {type_expr} attribute; rex admits no constraint families for it and they were dropped"
                ));
            }
            return String::new();
        }
        let mut parts: Vec<String> = Vec::new();
        if string_family {
            if let Some(ref pattern) = constraints.pattern {
                parts.push(format!("pattern \"{}\"", escape_mox_string(pattern)));
            }
            if let Some(min) = constraints.min_length {
                parts.push(format!("minLength {min}"));
            }
            if let Some(max) = constraints.max_length {
                parts.push(format!("maxLength {max}"));
            }
        }
        if numeric_family {
            if let Some(min) = constraints.minimum {
                parts.push(format!("minimum {min}"));
            }
            if let Some(max) = constraints.maximum {
                parts.push(format!("maximum {max}"));
            }
        }
        if constraints.has_any() && parts.is_empty() {
            self.needs_review.push(format!(
                "property '{class_name}.{prop_name}' declares constraints outside the {type_expr} attribute's family; they were dropped"
            ));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!(" {{ {} }}", parts.join(" "))
        }
    }
}

enum Slot {
    /// A plain (primitive, stdlib-datatype, or enum-typed) attribute.
    Attr(String),
    /// A reference to a converted class.
    Ref(String),
}

/// Merges the class's member properties across the schema body and allOf
/// siblings, returning (properties, required, first-allOf-$ref). The allOf
/// member that provides `extends` keeps its properties in the base class,
/// so they are not merged here.
fn merged_members(
    schema: &serde_json::Value,
) -> (
    BTreeMap<String, &serde_json::Value>,
    BTreeSet<String>,
    Option<String>,
) {
    let mut properties: BTreeMap<String, &serde_json::Value> = BTreeMap::new();
    let mut required: BTreeSet<String> = BTreeSet::new();
    merge_member_schema(schema, &mut properties, &mut required);
    let mut extends_ref = None;
    if let Some(all_of) = schema.get("allOf").and_then(|v| v.as_array()) {
        for member in all_of {
            match member.get("$ref").and_then(|v| v.as_str()) {
                Some(ref_str) if extends_ref.is_none() => extends_ref = Some(ref_str.to_string()),
                _ => merge_member_schema(member, &mut properties, &mut required),
            }
        }
    }
    (properties, required, extends_ref)
}

fn merge_member_schema<'a>(
    schema: &'a serde_json::Value,
    properties: &mut BTreeMap<String, &'a serde_json::Value>,
    required: &mut BTreeSet<String>,
) {
    if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
        for (name, value) in props {
            properties.insert(name.clone(), value);
        }
    }
    if let Some(req) = schema.get("required").and_then(|v| v.as_array()) {
        for name in req.iter().filter_map(|v| v.as_str()) {
            required.insert(name.to_string());
        }
    }
}

/// The property's effective type. Type unions keep the first non-null
/// member; `properties` implies object.
fn plain_type(schema: &serde_json::Value) -> Option<String> {
    match schema.get("type") {
        Some(serde_json::Value::String(t)) => Some(t.clone()),
        Some(serde_json::Value::Array(types)) => types
            .iter()
            .filter_map(|v| v.as_str())
            .find(|t| *t != "null")
            .map(String::from),
        _ => {
            if schema.get("properties").is_some() {
                Some("object".to_string())
            } else if schema.get("enum").is_some() {
                Some("string".to_string())
            } else {
                None
            }
        }
    }
}

fn is_array_type(schema: &serde_json::Value) -> bool {
    plain_type(schema).as_deref() == Some("array")
}

fn stdlib_type_for_format(format: &str) -> Option<&'static str> {
    FORMAT_TYPES
        .iter()
        .find(|(key, _)| *key == format)
        .map(|(_, ty)| *ty)
}

fn is_stdlib_type(type_expr: &str) -> bool {
    type_expr
        .strip_prefix(STDLIB_PACKAGE)
        .is_some_and(|rest| rest.starts_with('.'))
}

struct ExtractedConstraints {
    pattern: Option<String>,
    min_length: Option<i64>,
    max_length: Option<i64>,
    minimum: Option<i64>,
    maximum: Option<i64>,
    dropped_notes: Vec<String>,
}

impl ExtractedConstraints {
    fn has_any(&self) -> bool {
        self.pattern.is_some()
            || self.min_length.is_some()
            || self.max_length.is_some()
            || self.minimum.is_some()
            || self.maximum.is_some()
    }
}

fn constraint_values(schema: &serde_json::Value) -> ExtractedConstraints {
    let mut out = ExtractedConstraints {
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        dropped_notes: Vec::new(),
    };
    if let Some(value) = schema.get("pattern").and_then(|v| v.as_str()) {
        out.pattern = Some(value.to_string());
    }
    let keys = [
        ("minLength", 0),
        ("maxLength", 1),
        ("minimum", 2),
        ("maximum", 3),
    ];
    for (key, slot) in keys {
        let Some(raw) = schema.get(key) else {
            continue;
        };
        match raw.as_i64() {
            Some(int) => match slot {
                0 => out.min_length = Some(int),
                1 => out.max_length = Some(int),
                2 => out.minimum = Some(int),
                _ => out.maximum = Some(int),
            },
            None => out.dropped_notes.push(format!(
                "constraint '{key}' is not an integer, which rex constraints require, and was dropped"
            )),
        }
    }
    if let (Some(min), Some(max)) = (out.min_length, out.max_length) {
        if min > max {
            out.dropped_notes
                .push("minLength exceeds maxLength; both were dropped".to_string());
            out.min_length = None;
            out.max_length = None;
        }
    }
    if let (Some(min), Some(max)) = (out.minimum, out.maximum) {
        if min > max {
            out.dropped_notes
                .push("minimum exceeds maximum; both were dropped".to_string());
            out.minimum = None;
            out.maximum = None;
        }
    }
    out
}

fn schema_has_constraints(schema: &serde_json::Value) -> bool {
    ["pattern", "minLength", "maxLength", "minimum", "maximum"]
        .iter()
        .any(|key| schema.get(*key).is_some())
}

/// True for top-level codelist-style schemas: a non-empty `enum` and no
/// member properties. Non-string enum schemas are handled at render time.
fn is_enum_schema(schema: &serde_json::Value) -> bool {
    let has_values = schema
        .get("enum")
        .and_then(|v| v.as_array())
        .is_some_and(|values| !values.is_empty());
    let has_properties = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .is_some_and(|props| !props.is_empty());
    has_values && !has_properties && schema.get("allOf").is_none()
}

/// The string values of an `enum` array, or None when any value is not a
/// string (or the array is empty).
fn string_enum_values(values: &[serde_json::Value]) -> Option<Vec<String>> {
    if values.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        out.push(value.as_str()?.to_string());
    }
    Some(out)
}

fn title_of(entry: &SchemaEntry) -> String {
    entry
        .schema
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(&entry.stem)
        .to_string()
}

/// Sanitize into a rex identifier (letters, digits, `_`, non-digit first).
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
    if first.is_ascii_digit() {
        return Some(format!("_{cleaned}"));
    }
    Some(cleaned)
}

/// The source form of a bare identifier: rex keywords need the `^` escape.
fn emit_name(bare: &str) -> String {
    if KEYWORDS.contains(&bare) {
        format!("^{bare}")
    } else {
        bare.to_string()
    }
}

fn to_pascal(name: &str) -> String {
    name.split(['_', '-', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn escape_mox_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' | '\r' | '\t' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn doc_line(description: Option<&serde_json::Value>) -> Option<String> {
    let text = description?.as_str()?;
    let single = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!single.is_empty()).then_some(single)
}

fn with_doc(doc: Option<String>, line: String) -> String {
    match doc {
        Some(doc) => format!("/// {doc}\n{line}"),
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rex_driver::compile_str;

    #[test]
    fn stdlib_compiles_with_seven_datatypes() {
        let compilation = compile_str(STDLIB_FILE_NAME, STDLIB_SOURCE);
        let messages: Vec<String> = compilation
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect();
        assert!(
            compilation.diagnostics.is_empty(),
            "stdlib diagnostics: {messages:?}"
        );
        let model = compilation.model.expect("stdlib must lower");
        assert_eq!(model.packages.len(), 1);
        assert_eq!(model.packages[0].name, STDLIB_PACKAGE);
        assert_eq!(model.packages[0].datatypes.len(), 7);
        let names: Vec<&str> = model.packages[0]
            .datatypes
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["Uuid", "DateTime", "Date", "Email", "Uri", "Json", "Decimal"]
        );
    }

    #[test]
    fn qualified_stdlib_refs_resolve_across_packages() {
        let domain = "package sales\n\nclass OrderType {\n    codegraph_stdlib.Uuid id\n    codegraph_stdlib.DateTime[0..1] placedAt\n    Int total { minimum 0 }\n}";
        let compilation = compile_files(&[
            (STDLIB_FILE_NAME.to_string(), STDLIB_SOURCE.to_string()),
            ("sales.mox".to_string(), domain.to_string()),
        ]);
        let errors: Vec<String> = compilation
            .diagnostics
            .iter()
            .filter(|(_, d)| d.is_error())
            .map(|(_, d)| d.message.clone())
            .collect();
        assert!(errors.is_empty(), "qualified refs must resolve: {errors:?}");
        assert!(compilation.model.is_some());
    }

    #[test]
    fn sanitize_identifier_rules() {
        assert_eq!(
            sanitize_identifier("due-date"),
            Some("due_date".to_string())
        );
        assert_eq!(sanitize_identifier("9lives"), Some("_9lives".to_string()));
        assert_eq!(sanitize_identifier(""), None);
    }

    #[test]
    fn constraint_extraction_orders_and_drops() {
        let schema: serde_json::Value = serde_json::from_str(
            r#"{"minLength": 8, "maxLength": 2, "pattern": "^x", "minimum": 1.5}"#,
        )
        .expect("fixture must parse");
        let constraints = constraint_values(&schema);
        assert!(constraints.min_length.is_none());
        assert!(constraints.max_length.is_none());
        assert_eq!(constraints.pattern.as_deref(), Some("^x"));
        assert!(constraints.minimum.is_none());
        assert_eq!(constraints.dropped_notes.len(), 2);
    }
}
