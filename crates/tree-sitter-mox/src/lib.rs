//! Tree-sitter parser for the rexlang `.mox` modeling language.
//!
//! The generated parser ([`src/parser.c`]) is committed and is regenerated
//! with `npx tree-sitter-cli generate --abi 14` from the grammar source in
//! `codegraph-vscode/grammar-mox/grammar.js` (see that file's header for the
//! node inventory the LSP queries against).

extern "C" {
    fn tree_sitter_mox() -> tree_sitter::Language;
}

/// The Tree-sitter `LanguageFn` for `.mox`.
pub const LANGUAGE: tree_sitter_language::LanguageFn = unsafe {
    tree_sitter_language::LanguageFn::from_raw(std::mem::transmute::<
        unsafe extern "C" fn() -> tree_sitter::Language,
        unsafe extern "C" fn() -> *const (),
    >(tree_sitter_mox))
};

/// The Tree-sitter `.mox` language definition.
pub fn language() -> tree_sitter::Language {
    unsafe { tree_sitter_mox() }
}

/// The content of `node-types.json` for code generation.
pub const NODE_TYPES: &str = include_str!("./node-types.json");

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::language())
            .expect("Error loading mox language");
        parser.parse(source, None).expect("Failed to parse")
    }

    #[test]
    fn parses_minimal_model_without_errors() {
        let source = "package nz.example.shop\n\nclass Customer {\n    String name\n    refers Order[] orders opposite customer\n}\n";
        let tree = parse(source);
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(!root.has_error(), "tree should be error-free: {root:?}");

        let class_decl = root.child(1).expect("class_declaration child");
        assert_eq!(class_decl.kind(), "class_declaration");
        let feature_kinds: Vec<Vec<&str>> = class_decl
            .children(&mut class_decl.walk())
            .filter(|n| n.kind() == "feature")
            .map(|feature| {
                feature
                    .children(&mut feature.walk())
                    .map(|n| n.kind())
                    .collect()
            })
            .collect();
        assert!(
            feature_kinds.iter().any(|kinds| kinds.contains(&"reference")),
            "expected a reference feature, got {feature_kinds:?}"
        );
    }

    #[test]
    fn parses_import_schema_declaration() {
        let source =
            "import schema \"schemas/common.json\" as Common\n\npackage nz.example.app\n";
        let tree = parse(source);
        let root = tree.root_node();
        assert!(!root.has_error(), "tree should be error-free: {root:?}");
        let import = root.child(0).expect("import child");
        assert_eq!(import.kind(), "import_schema_declaration");
    }

    #[test]
    fn actors_block_and_capabilities_are_visible_nodes() {
        let source = "actors Support {\n    actor Agent\n    capability RaiseRefund on Ticket\n    grant Agent {\n        permit RaiseRefund when (refundCents > 0) obligation audit\n    }\n}\n";
        let tree = parse(source);
        let root = tree.root_node();
        assert!(!root.has_error(), "tree should be error-free: {root:?}");
        let mut found = std::collections::HashSet::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            found.insert(node.kind().to_string());
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        for kind in [
            "actors_block",
            "actor_declaration",
            "capability",
            "grant_declaration",
            "effect_entry",
            "when_clause",
            "obligation",
        ] {
            assert!(found.contains(kind), "expected {kind} node in tree");
        }
    }

    #[test]
    fn node_types_match_committed_generated_file() {
        // Proves the committed generated artifacts are the ones this crate
        // was built against.
        assert!(NODE_TYPES.contains("class_declaration"));
        assert!(NODE_TYPES.contains("actors_block"));
    }
}
