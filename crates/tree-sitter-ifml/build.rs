fn main() {
    let src_dir = std::path::Path::new("src");
    let mut cfg = cc::Build::new();
    cfg.include(src_dir);
    cfg.file(src_dir.join("parser.c"));
    cfg.compile("tree-sitter-ifml");
    println!("cargo:rerun-if-changed=src/parser.c");
    println!("cargo:rerun-if-changed=src/tree_sitter/parser.h");
}
