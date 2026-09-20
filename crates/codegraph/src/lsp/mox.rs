//! `.mox` support in the codegraph LSP (MoxState, epic #228 P4 / #232 E2).
//!
//! Startup: `build_mox_state` compiles the `--mox-files` sources via
//! `rex_driver::compile_files_with_imports` (import JSON resolved relative to
//! each file, like the ingest pipeline) into a flat declaration index
//! ([`MoxState`]). Install it with [`init_mox`](super::init_mox); without it
//! every mox handler below degrades to quiet (syntax diagnostics still flow
//! — they only need the tree-sitter parse).
//!
//! Diagnostics on open/change of a `.mox` document: tree-sitter
//! ERROR/MISSING nodes always; with MoxState and a clean parse: unknown
//! names in `type:`/`superclass:` positions (rexlang primitives exempt),
//! `import schema` paths that do not exist relative to the document, and
//! ambiguity warnings for names resolving to several declarations (v1 keeps
//! a flat name index — see `resolve_mox_name`). Actor-block internals are
//! skipped (grant/capability validation is future rex-lsp territory).
//!
//! Tree-sitter queries against the stable node inventory documented in
//! `codegraph-vscode/grammar-mox/grammar.js` drive the type-position walk;
//! completions detect their context with a token scan of the line before
//! the cursor plus a brace scan for the class body (the IFML handlers'
//! scan-based approach — mid-typing the tree is often one ERROR node).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use auto_lsp::anyhow;
use auto_lsp::default::db::{BaseDatabase, BaseDb};
use auto_lsp::lsp_types::*;
use auto_lsp::tree_sitter::{self, Query, QueryCursor, StreamingIterator};
use rex_driver::{compile_files_with_imports, SchemaImports};
use rex_ir::{FeatureKind, TypeRef};

use super::state::{
    ImportAliasInfo, MoxClassInfo, MoxDatatypeInfo, MoxEnumInfo, MoxFeatureInfo, MoxState,
    MoxVocabularyInfo,
};

static MOX_LANG: LazyLock<tree_sitter::Language> = LazyLock::new(tree_sitter_mox::language);

/// Rexlang primitive type names — valid in any type position without a
/// declaration.
pub(crate) const MOX_PRIMITIVES: &[&str] = &[
    "String", "int", "long", "short", "float", "double", "boolean", "byte", "char",
];

/// Type positions to validate: the grammar's `type:` fields (capability is
/// deliberately absent — actor-block internals are rex-lsp territory) plus
/// the actor `superclass:` field.
static MOX_TYPE_REF_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(
        &MOX_LANG,
        r#"[
            (extends_clause type: (qualified_name) @type-name)
            (attribute type: (qualified_name) @type-name)
            (containment type: (qualified_name) @type-name)
            (reference type: (qualified_name) @type-name)
            (container_feature type: (qualified_name) @type-name)
            (operation_declaration type: (qualified_name) @type-name)
            (derived_declaration type: (qualified_name) @type-name)
            (parameter type: (qualified_name) @type-name)
            (facet_entry type: (qualified_name) @type-name)
            (datatype_declaration type: (qualified_name) @type-name)
            (actor_declaration superclass: (identifier) @superclass-name)
        ]"#,
    )
    .expect("Failed to create mox type ref query")
});

// ── AST root for the auto-lsp parser map ───────────────────────────────

/// Minimal auto-lsp AST root for `.mox` documents: a passthrough over the
/// tree-sitter `source_file`. The mox handlers walk the raw tree-sitter
/// tree (no generated AST for mox exists), so this only satisfies the
/// parser registry's contract.
#[derive(Debug)]
pub struct MoxSourceFile {
    _range: tree_sitter::Range,
    _id: usize,
    _parent: Option<usize>,
}

impl auto_lsp::core::ast::AstNode for MoxSourceFile {
    fn contains(node: &tree_sitter::Node) -> bool {
        node.kind() == "source_file"
    }

    fn lower(&self) -> &dyn auto_lsp::core::ast::AstNode {
        self
    }

    fn get_id(&self) -> usize {
        self._id
    }

    fn get_parent_id(&self) -> Option<usize> {
        self._parent
    }

    fn get_range(&self) -> &tree_sitter::Range {
        &self._range
    }
}

impl<'a> TryFrom<auto_lsp::core::ast::TryFromParams<'a>> for MoxSourceFile {
    type Error = auto_lsp::core::errors::AstError;

    fn try_from(
        (node, _db, _builder, id, parent_id): auto_lsp::core::ast::TryFromParams<'a>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            _range: node.range(),
            _id: id,
            _parent: parent_id,
        })
    }
}

// ── Startup: build the MoxState ────────────────────────────────────────

/// Compile the startup `--mox-files` into the LSP's mox model.
///
/// `import schema` declarations are scanned per file (the ingest pipeline's
/// line scanner), resolved against the .mox file's directory, and their JSON
/// provided to the rex compile — the same `SchemaImports` contract as
/// `ingest_mox_files`. Missing/unreadable imports and compile errors degrade
/// to stderr warnings: the file's declarations are absent from the state
/// (unknown-type checks then report against what IS loaded) but the LSP
/// never fails to start. `schema_titles` + `type_suffix` replay the ingest
/// pipeline's `wire_alias_refs` alias→title resolution (exact match, then
/// title + suffix).
pub fn build_mox_state(
    mox_paths: &[PathBuf],
    schema_titles: &HashSet<String>,
    type_suffix: &str,
) -> MoxState {
    let mut state = MoxState::default();
    let mut sources: Vec<(String, String)> = Vec::new();
    let mut schema_imports = SchemaImports::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for path in mox_paths {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !seen.insert(canonical) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!(
                "Warning: mox file '{}' could not be read — LSP mox support skips it",
                path.display()
            );
            continue;
        };
        let mox_path = path.display().to_string();
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let mut imports_resolved = true;
        for decl in crate::ingest::mox_ingest::scan_schema_imports(&text) {
            let abs_path = base_dir.join(&decl.path);
            let alias = decl
                .alias
                .clone()
                .unwrap_or_else(|| file_stem_name(&decl.path));
            state.import_aliases.push(ImportAliasInfo {
                resolved_title: resolve_alias_title(schema_titles, &alias, type_suffix),
                alias: alias.clone(),
                abs_path: abs_path.display().to_string(),
            });
            match std::fs::read_to_string(&abs_path) {
                Ok(json) => {
                    schema_imports.insert(&mox_path, &decl.path, json);
                }
                Err(e) => {
                    imports_resolved = false;
                    eprintln!(
                        "Warning: LSP mox file {mox_path}: import schema '{}' cannot be read: {e} \
                         — its declarations are excluded from completions/diagnostics",
                        decl.path
                    );
                }
            }
        }
        if imports_resolved {
            sources.push((mox_path, text));
        }
    }

    if sources.is_empty() {
        return state;
    }

    let compilation = compile_files_with_imports(&sources, &schema_imports);
    for (path, diagnostic) in &compilation.diagnostics {
        eprintln!(
            "Warning: LSP mox diagnostic in {path}: {}",
            diagnostic.message
        );
    }
    let Some(model) = compilation.model else {
        eprintln!(
            "Warning: LSP mox compilation produced no model — mox completions degrade to import aliases"
        );
        return state;
    };

    for package in &model.packages {
        for class in &package.classes {
            state.classes.push(MoxClassInfo {
                name: class.name.clone(),
                package: package.name.clone(),
                features: class
                    .features
                    .iter()
                    .filter(|f| !f.is_derived && f.kind != FeatureKind::Container)
                    .map(|f| MoxFeatureInfo {
                        name: f.name.clone(),
                        kind: feature_kind_str(f.kind).to_string(),
                    })
                    .collect(),
                extends: class.extends.iter().filter_map(bare_type_name).collect(),
            });
        }
        for enum_def in &package.enums {
            state.enums.push(MoxEnumInfo {
                name: enum_def.name.clone(),
                package: package.name.clone(),
            });
        }
        for datatype in &package.datatypes {
            state.datatypes.push(MoxDatatypeInfo {
                name: datatype.name.clone(),
                package: package.name.clone(),
                format: datatype.format.clone(),
            });
        }
        for vocabulary in &package.vocabularies {
            state.vocabularies.push(MoxVocabularyInfo {
                name: vocabulary.name.clone(),
                package: package.name.clone(),
            });
        }
    }

    state
}

fn file_stem_name(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// The `wire_alias_refs` resolution order: exact title, then title +
/// `type_suffix` (the `Customer` → `CustomerType` convention).
fn resolve_alias_title(
    schema_titles: &HashSet<String>,
    alias: &str,
    type_suffix: &str,
) -> Option<String> {
    if schema_titles.contains(alias) {
        return Some(alias.to_string());
    }
    let suffixed = format!("{alias}{type_suffix}");
    schema_titles.contains(&suffixed).then_some(suffixed)
}

fn feature_kind_str(kind: FeatureKind) -> &'static str {
    match kind {
        FeatureKind::Attribute => "attribute",
        FeatureKind::Containment => "containment",
        FeatureKind::CrossReference => "reference",
        FeatureKind::Container => "container",
    }
}

fn bare_type_name(type_ref: &TypeRef) -> Option<String> {
    match type_ref {
        TypeRef::Class { name, .. }
        | TypeRef::Enum { name, .. }
        | TypeRef::Datatype { name, .. }
        | TypeRef::Interface { name, .. }
        | TypeRef::Vocabulary { name, .. } => Some(name.clone()),
        TypeRef::Primitive(_) => None,
    }
}

// ── Name resolution (flat v1 index) ────────────────────────────────────

/// What a written type name resolves to in the MoxState.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MoxNameResolution {
    /// Kind per matching declaration: `class` | `enum` | `datatype` |
    /// `vocabulary`. More than one entry means the bare name is ambiguous
    /// (v1 keeps a flat name→kinds index; callers surface a warning).
    pub kinds: Vec<&'static str>,
    /// Owning package per matching declaration, aligned with `kinds`.
    pub packages: Vec<String>,
}

/// Resolve a type name as written: exact match against the flat index
/// first, then the last dot-segment (bare name of a qualified reference).
/// Primitives are handled by the caller (`is_primitive_name`).
pub(crate) fn resolve_mox_name(state: &MoxState, text: &str) -> Option<MoxNameResolution> {
    let mut resolution = MoxNameResolution {
        kinds: Vec::new(),
        packages: Vec::new(),
    };
    let bare = text.rsplit('.').next().unwrap_or(text);
    for class in &state.classes {
        if class.name == text || class.name == bare {
            resolution.kinds.push("class");
            resolution.packages.push(class.package.clone());
        }
    }
    for enum_def in &state.enums {
        if enum_def.name == text || enum_def.name == bare {
            resolution.kinds.push("enum");
            resolution.packages.push(enum_def.package.clone());
        }
    }
    for datatype in &state.datatypes {
        if datatype.name == text || datatype.name == bare {
            resolution.kinds.push("datatype");
            resolution.packages.push(datatype.package.clone());
        }
    }
    for vocabulary in &state.vocabularies {
        if vocabulary.name == text || vocabulary.name == bare {
            resolution.kinds.push("vocabulary");
            resolution.packages.push(vocabulary.package.clone());
        }
    }
    (!resolution.kinds.is_empty()).then_some(resolution)
}

pub(crate) fn is_primitive_name(name: &str) -> bool {
    MOX_PRIMITIVES.contains(&name)
}

// ── Diagnostics ────────────────────────────────────────────────────────

pub fn compute_mox_diagnostics(db: &BaseDb, uri: &Url) -> Vec<Diagnostic> {
    let Some(file) = db.get_file(uri) else {
        return Vec::new();
    };
    let document = file.document(db);
    let source = document.as_str();
    let source_bytes = source.as_bytes();
    let root = document.tree.root_node();

    let mut diagnostics = Vec::new();
    super::handlers::collect_errors(&root, source_bytes, &mut diagnostics);

    super::state::with_mox_opt(|mox| {
        let Some(mox) = mox else {
            return;
        };
        // Semantic checks only run on cleanly parsed documents so
        // tree-sitter error recovery cannot fabricate references (IFML
        // precedent).
        if super::handlers::has_any_error(&root, source_bytes) {
            return;
        }
        validate_import_paths(source, uri, &mut diagnostics);
        validate_type_refs(source_bytes, &root, mox, &mut diagnostics);
    });

    diagnostics
}

/// Every `import schema "<path>"` target must exist relative to the
/// document's directory (the doctor/C1 hard-error semantics, as an editor
/// diagnostic).
fn validate_import_paths(source: &str, uri: &Url, diagnostics: &mut Vec<Diagnostic>) {
    let Ok(doc_path) = uri.to_file_path() else {
        return;
    };
    let base_dir = doc_path.parent().unwrap_or_else(|| Path::new("."));
    for decl in crate::ingest::mox_ingest::scan_schema_imports(source) {
        let abs_path = base_dir.join(&decl.path);
        if abs_path.exists() {
            continue;
        }
        for (line_idx, line) in source.lines().enumerate() {
            if !line.trim().starts_with("import schema") || !line.contains(&decl.path) {
                continue;
            }
            let col = (line.find(&decl.path)).unwrap_or(0) as u32;
            diagnostics.push(Diagnostic {
                range: Range::new(
                    Position::new(line_idx as u32, col),
                    Position::new(line_idx as u32, col + decl.path.len() as u32),
                ),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!(
                    "import schema '{}' not found (resolved to '{}')",
                    decl.path,
                    abs_path.display()
                ),
                source: Some("codegraph".to_string()),
                ..Default::default()
            });
            break;
        }
    }
}

/// Validate every type position's name against the MoxState (or the rexlang
/// primitives). Unknown names are ERRORs; names resolving to several
/// declarations (v1 flat index) get an ambiguity WARNING.
fn validate_type_refs(
    source_bytes: &[u8],
    root: &tree_sitter::Node,
    mox: &MoxState,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&MOX_TYPE_REF_QUERY, *root, source_bytes);

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let Ok(text) = capture.node.utf8_text(source_bytes) else {
                continue;
            };
            if is_primitive_name(text) {
                continue;
            }
            let range = capture.node.range();
            let range = Range::new(
                Position::new(
                    range.start_point.row as u32,
                    range.start_point.column as u32,
                ),
                Position::new(range.end_point.row as u32, range.end_point.column as u32),
            );
            let Some(resolution) = resolve_mox_name(mox, text) else {
                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("unknown type '{text}'"),
                    source: Some("codegraph".to_string()),
                    ..Default::default()
                });
                continue;
            };
            if resolution.kinds.len() > 1 {
                let kinds = resolution
                    .kinds
                    .iter()
                    .zip(&resolution.packages)
                    .map(|(k, p)| format!("{k} in {p}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("Ambiguous type name '{text}' ({kinds})"),
                    source: Some("codegraph".to_string()),
                    ..Default::default()
                });
            }
        }
    }
}

// ── Completions ────────────────────────────────────────────────────────

/// Keywords after which a type name is expected.
const TYPE_KEYWORD_CONTEXTS: &[&str] = &["refers", "contains", "container", "extends", "on"];

pub fn handle_mox_completion(
    db: &BaseDb,
    params: CompletionParams,
) -> anyhow::Result<Option<CompletionResponse>> {
    let uri = &params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let Some(file) = db.get_file(uri) else {
        return Ok(None);
    };
    let document = file.document(db);
    let source = document.as_str();

    let mut items = Vec::new();
    super::state::with_mox_opt(|mox| {
        let Some(mox) = mox else {
            return;
        };
        if mox_completion_context(source, position) {
            push_mox_type_items(mox, &mut items);
        }
    });

    if items.is_empty() {
        Ok(None)
    } else {
        Ok(Some(CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items,
        })))
    }
}

/// True when the cursor sits where a type name is expected: after one of
/// the type-keyword contexts, or on the first (partial) word of a line
/// inside a class body — an attribute type being typed.
fn mox_completion_context(source: &str, position: Position) -> bool {
    let Some(line) = source.lines().nth(position.line as usize) else {
        return false;
    };
    let byte_col = line
        .char_indices()
        .nth(position.character as usize)
        .map(|(i, _)| i)
        .unwrap_or(line.len());
    let before_cursor = &line[..byte_col.min(line.len())];
    let trimmed = before_cursor.trim();

    if let Some(last_word) = trimmed.split_whitespace().last() {
        if TYPE_KEYWORD_CONTEXTS.contains(&last_word) {
            return true;
        }
    }

    // Attribute type being typed: a single word alone on the line (the
    // mox `attribute` production starts with the bare type name), inside
    // a class body.
    if !trimmed.is_empty() && !trimmed.contains(char::is_whitespace) {
        // The tree may be one big ERROR node mid-typing (the incomplete
        // attribute breaks the class production), so detect the class body
        // by brace scan rather than tree ancestry.
        return inside_class_body(source, position);
    }

    false
}

/// Brace-scan detection of "the cursor is inside a `class` body". Tracks the
/// innermost unclosed brace and the declaration keyword that opened it.
/// Line comments are stripped; block comments and brace-bearing raw bodies
/// are balanced, so depth math stays correct.
fn inside_class_body(source: &str, pos: Position) -> bool {
    #[derive(Clone, Copy, PartialEq)]
    enum Scope {
        Class,
        Other,
    }
    let mut stack: Vec<Scope> = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        let code = line.split_once("//").map_or(line, |(c, _)| c);
        let trimmed_start = code.trim_start();
        let scope = if trimmed_start.starts_with("class ") || trimmed_start.starts_with("class\t") {
            Scope::Class
        } else {
            Scope::Other
        };
        // On the cursor's line, only scan up to the cursor column.
        let byte_end = if line_idx == pos.line as usize {
            code.char_indices()
                .nth(pos.character as usize)
                .map(|(i, _)| i)
                .unwrap_or(code.len())
        } else {
            code.len()
        };
        for ch in code[..byte_end].chars() {
            match ch {
                '{' => stack.push(scope),
                '}' => {
                    stack.pop();
                }
                _ => {}
            }
        }
        if line_idx == pos.line as usize {
            break;
        }
    }

    stack.last() == Some(&Scope::Class)
}

fn push_mox_type_items(mox: &MoxState, items: &mut Vec<CompletionItem>) {
    for class in &mox.classes {
        items.push(CompletionItem {
            label: class.name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some(format!("class {}", class.package)),
            ..Default::default()
        });
    }
    for enum_def in &mox.enums {
        items.push(CompletionItem {
            label: enum_def.name.clone(),
            kind: Some(CompletionItemKind::ENUM),
            detail: Some(format!("enum {}", enum_def.package)),
            ..Default::default()
        });
    }
    for datatype in &mox.datatypes {
        items.push(CompletionItem {
            label: datatype.name.clone(),
            kind: Some(CompletionItemKind::STRUCT),
            detail: Some(format!(
                "datatype {}{}",
                datatype.package,
                datatype
                    .format
                    .as_deref()
                    .map(|f| format!(" (format {f})"))
                    .unwrap_or_default()
            )),
            ..Default::default()
        });
    }
    for vocabulary in &mox.vocabularies {
        items.push(CompletionItem {
            label: vocabulary.name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some(format!("vocabulary {}", vocabulary.package)),
            ..Default::default()
        });
    }
    for primitive in MOX_PRIMITIVES {
        items.push(CompletionItem {
            label: (*primitive).to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("primitive".to_string()),
            ..Default::default()
        });
    }
}

// ── Hover ──────────────────────────────────────────────────────────────

pub fn handle_mox_hover(db: &BaseDb, params: HoverParams) -> anyhow::Result<Option<Hover>> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let Some(file) = db.get_file(uri) else {
        return Ok(None);
    };
    let document = file.document(db);
    let source = document.as_str();

    let Some(line) = source.lines().nth(position.line as usize) else {
        return Ok(None);
    };
    let Some(word) = super::handlers::get_word_at_position(line, position.character as usize)
    else {
        return Ok(None);
    };

    let mut hover = None;
    super::state::with_mox_opt(|mox| {
        let Some(mox) = mox else {
            return;
        };
        if !is_type_position(&document.tree.root_node(), position) {
            return;
        }
        let Some(resolution) = resolve_mox_name(mox, &word) else {
            return;
        };
        hover = Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: mox_hover_markdown(mox, &word, &resolution),
            }),
            range: Some(Range::new(
                Position::new(
                    position.line,
                    position.character.saturating_sub(word.len() as u32),
                ),
                Position::new(position.line, position.character),
            )),
        });
    });

    Ok(hover)
}

/// True when the cursor sits on a `type:`/`superclass:` field value (the
/// node at the position, or its `qualified_name` ancestor, is one).
fn is_type_position(root: &tree_sitter::Node, pos: Position) -> bool {
    let point = tree_sitter::Point {
        row: pos.line as usize,
        column: pos.character as usize,
    };
    let Some(mut node) = root.descendant_for_point_range(point, point) else {
        return false;
    };
    loop {
        if let Some(parent) = node.parent() {
            for field in ["type", "superclass"] {
                if parent.child_by_field_name(field) == Some(node) {
                    return true;
                }
            }
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => return false,
        }
    }
}

fn mox_hover_markdown(mox: &MoxState, name: &str, resolution: &MoxNameResolution) -> String {
    let mut md = String::new();
    for (kind, package) in resolution.kinds.iter().zip(&resolution.packages) {
        md.push_str(&format!("**{name}** `{kind}`\n\npackage `{package}`\n\n"));
        if *kind == "class" {
            if let Some(class) = mox.classes.iter().find(|c| c.name == name) {
                if !class.features.is_empty() {
                    md.push_str("features:\n");
                    for feature in &class.features {
                        md.push_str(&format!("- `{}` ({})\n", feature.name, feature.kind));
                    }
                    md.push('\n');
                }
                if !class.extends.is_empty() {
                    md.push_str(&format!("extends: {}\n", class.extends.join(", ")));
                }
            }
        }
        if *kind == "datatype" {
            if let Some(datatype) = mox.datatypes.iter().find(|d| d.name == name) {
                if let Some(format) = &datatype.format {
                    md.push_str(&format!("format: `{format}`\n"));
                }
            }
        }
    }
    md
}
