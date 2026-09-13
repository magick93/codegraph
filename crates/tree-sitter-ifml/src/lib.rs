extern "C" {
    fn tree_sitter_ifml() -> tree_sitter::Language;
}

/// The Tree-sitter `LanguageFn` for IFML.
pub const LANGUAGE: tree_sitter_language::LanguageFn = unsafe {
    tree_sitter_language::LanguageFn::from_raw(std::mem::transmute::<
        unsafe extern "C" fn() -> tree_sitter::Language,
        unsafe extern "C" fn() -> *const (),
    >(tree_sitter_ifml))
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

    #[test]
    fn test_can_parse_typed_component_statements() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::language())
            .expect("Error loading IFML language");

        let source = r#"view "Customers" {
            component "grid" {
                type: list;
                data: Customer;

                column "Name" -> field Customer.name;
                column "Status" -> lookup Customer.status via status_labels;
                column "Tenure" -> expr tenure_years(Customer.hire_date);

                field name -> input text { required: true; validations: [len(name) > 2]; }
                field email -> input email;
                field status -> input dropdown { values: ["gold", "silver"]; }

                chart bar;
                chart line { label: region; values: [revenue]; }
            }
        }"#;

        let tree = parser.parse(source, None).expect("Failed to parse");
        let root = tree.root_node();
        assert!(!root.has_error(), "Typed component statements should parse");

        let source_bytes = source.as_bytes();
        let mut found = (false, false, false);
        let mut cursor = root.walk();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            match node.kind() {
                "column_decl" => found.0 = true,
                "field_decl" => found.1 = true,
                "chart_decl" => found.2 = true,
                _ => {}
            }
            stack.extend(node.children(&mut cursor));
        }
        assert!(
            found == (true, true, true),
            "expected column_decl, field_decl and chart_decl nodes, got {found:?} in {}",
            String::from_utf8_lossy(source_bytes)
        );
    }

    #[test]
    fn test_can_parse_condition_use_actor_and_event_condition() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::language())
            .expect("Error loading IFML language");

        let source = r#"actor "Manager" { label: "mgr"; }

module "AuditTrail" {
    input { entityId: Uuid = "0" }
    output { count: Int = 0 }
}

view "Customers" {
    roles: [manager];
    messages: ["Welcome"];

    if data.enabled;

    use "AuditTrail" as audit { scope: org; };

    component "grid" {
        type: list;
        data: Customer;

        if count > 0;

        on select(row) if row.active -> navigate("Detail");
    }
}"#;

        let tree = parser.parse(source, None).expect("Failed to parse");
        let root = tree.root_node();
        assert!(
            !root.has_error(),
            "new pipeline syntax should parse cleanly, tree: {}",
            root.to_sexp()
        );

        let mut found = std::collections::HashSet::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            found.insert(node.kind().to_string());
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        for kind in [
            "actor_declaration",
            "condition_statement",
            "module_use_statement",
            "event_condition",
            "param_default",
        ] {
            assert!(found.contains(kind), "expected {kind} node in tree");
        }
    }
}
