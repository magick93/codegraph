use auto_lsp::configure_parsers;
use auto_lsp::default::db::{BaseDatabase, BaseDb, FileManager};
use auto_lsp::lsp_types::Url;
use auto_lsp::texter::core::text::Text;

use crate::generated::SourceFile;

configure_parsers!(
    IFML_PARSERS,
    "ifml" => {
        language: tree_sitter_ifml::LANGUAGE,
        ast_root: SourceFile
    }
);

pub fn create_ifml_db(source_code: &[&str]) -> impl BaseDatabase {
    let mut db = BaseDb::default();
    for (i, code) in source_code.iter().enumerate() {
        let url = Url::parse(&format!("file:///test{i}.ifml")).expect("Failed to parse URL");

        let parsers: &'static auto_lsp::core::parsers::Parsers = IFML_PARSERS
            .get("ifml")
            .expect("IFML parser not found");

        let texter = Text::new(code.to_string());

        db.add_file_from_texter(parsers, &url, texter)
            .expect("Failed to add file");
    }
    db
}
