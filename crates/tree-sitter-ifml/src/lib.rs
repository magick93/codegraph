extern "C" {
    fn tree_sitter_ifml() -> tree_sitter::Language;
}

/// The Tree-sitter `LanguageFn` for IFML.
pub const LANGUAGE: tree_sitter_language::LanguageFn = unsafe {
    tree_sitter_language::LanguageFn::from_raw(
        std::mem::transmute::<
            unsafe extern "C" fn() -> tree_sitter::Language,
            unsafe extern "C" fn() -> *const (),
        >(tree_sitter_ifml),
    )
};

/// The Tree-sitter IFML language definition.
pub fn language() -> tree_sitter::Language {
    unsafe { tree_sitter_ifml() }
}

/// The content of `node-types.json` for code generation.
pub const NODE_TYPES: &str = include_str!("./node-types.json");

/// The content of `grammar.json` (optional, for tooling).
pub const GRAMMAR_JSON: &str = include_str!("./grammar.json");

#[cfg(test)]
mod tests {
    #[test]
    fn test_can_parse_ifml() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::language())
            .expect("Error loading IFML language");

        let source = r#"view "Hello" {
            component "greeting" {
                type: list;
                data: Customer;
                fields: [name, email];
            }
        }"#;

        let tree = parser.parse(source, None).expect("Failed to parse");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(!root.has_error(), "Parse tree has errors");
        assert!(root.child_count() > 0, "Should have at least one child");
    }
}
