use std::{fs, path::PathBuf};

fn main() {
    // src/generated/mod.rs is committed; the checked-in version is the source of
    // truth and plain builds must never touch it (regenerating on every build
    // dirtied the working tree). Only regenerate explicitly with AST_GEN=1 —
    // needed solely when the IFML grammar (tree-sitter-ifml NODE_TYPES) changes.
    println!("cargo:rerun-if-env-changed=AST_GEN");
    if std::env::var("AST_GEN").ok().as_deref() != Some("1") {
        return;
    }

    let output_path = PathBuf::from("./src/generated");

    fs::create_dir_all(&output_path).expect("Failed to create generated dir");

    let code = auto_lsp_codegen::generate(
        tree_sitter_ifml::NODE_TYPES,
        &tree_sitter_ifml::language(),
        None,
    )
    .to_string();

    fs::write(output_path.join("mod.rs"), code).expect("Failed to write generated code");
}
