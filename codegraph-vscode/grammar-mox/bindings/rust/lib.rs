//! Tree-sitter MOX grammar Rust bindings.
//!
//! This module provides Rust bindings for the Tree-sitter MOX parser.
//! The generated parser is compiled as a C library by tree-sitter-cli.
//!
//! Usage:
//! ```rust
//! let mut parser = tree_sitter::Parser::new();
//! parser.set_language(tree_sitter_mox::language())?;
//! let tree = parser.parse(source, None)?;
//! ```

/// Returns the Tree-sitter language function for MOX.
pub fn language() -> tree_sitter::Language {
    extern "C" {
        fn tree_sitter_mox() -> tree_sitter::Language;
    }
    unsafe { tree_sitter_mox() }
}

/// Source code for the MOX grammar.
pub const GRAMMAR: &str = include_str!("../../grammar.js");

/// Node type IDs for the MOX grammar.
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");
