use std::{fs, path::PathBuf};

fn main() {
    if std::env::var("AST_GEN").unwrap_or("1".to_string()) == "0" {
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
