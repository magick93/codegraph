//! Tree-sitter IFML grammar Rust bindings.
//!
//! This module provides Rust bindings for the Tree-sitter IFML parser.
//! The generated parser is compiled as a C library by tree-sitter-cli.
//!
//! Usage:
//! ```rust
//! let mut parser = tree_sitter::Parser::new();
//! parser.set_language(tree_sitter_ifml::language())?;
//! let tree = parser.parse(source, None)?;
//! ```

/// Returns the Tree-sitter language function for IFML.
pub fn language() -> tree_sitter::Language {
    extern "C" {
        fn tree_sitter_ifml() -> tree_sitter::Language;
    }
    unsafe { tree_sitter_ifml() }
}

/// Source code for the IFML grammar.
pub const GRAMMAR: &str = include_str!("../../grammar.js");

/// Node type IDs for the IFML grammar.
pub const NODE_TYPES: &str = include_str!("../../node-types.json");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_parse_minimal_view() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(language())
            .expect("Error loading IFML grammar");

        let source = r#"view "Hello" {
            component "hi" {
                type: list;
            }
        }"#;

        let tree = parser.parse(source, None).expect("Failed to parse");
        let root = tree.root_node();

        assert_eq!(root.kind(), "source_file");
        assert_eq!(root.child_count(), 1);

        let view = root.child(0).unwrap();
        assert_eq!(view.kind(), "view_declaration");
    }

    #[test]
    fn test_parse_with_errors() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(language())
            .expect("Error loading IFML grammar");

        let source = "invalid syntax here";
        let tree = parser.parse(source, None).expect("Failed to parse");
        let root = tree.root_node();

        assert!(root.has_error());
    }
}
