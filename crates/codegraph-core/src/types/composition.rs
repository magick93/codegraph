use super::composite::{CompositeColumn, CompositeRange};
use codegraph_type_contracts::RefClassificationKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositionTree {
    pub root: CompositionNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FkDirection {
    OnParent { column: String },
    OnChild { column: String },
}

/// Resolved foreign-key target for a column that references another table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FkTarget {
    /// Target schema (e.g. "common", "recruiting").
    pub schema: String,
    /// Target table name (e.g. "gender_code_list").
    pub table: String,
    /// Target column (e.g. "id" or "code").
    pub column: String,
    /// ON DELETE behavior (e.g. "RESTRICT", "SET NULL").
    pub on_delete: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub description: Option<String>,
    pub rust_type: String,
    pub postgres_type: String,
    pub is_optional: bool,
    pub is_codelist_fk: bool,
    pub composite_columns: Vec<CompositeColumn>,
    /// Whether this column is a PostgreSQL array type.
    #[serde(default)]
    pub is_array: bool,
    /// Typed classification kind (mirrors PropertyNode.effective_kind()).
    #[serde(default)]
    pub classification: Option<RefClassificationKind>,
    /// Resolved FK target — populated for CodelistReference and EntityReference columns.
    #[serde(default)]
    pub fk_target: Option<FkTarget>,
    /// Enum values for CHECK constraints (CodelistCheck / InlineEnum).
    #[serde(default)]
    pub check_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositionNode {
    pub field_name: String,
    pub schema_title: String,
    pub table_schema: String,
    pub table_name: String,
    pub fk: Option<FkDirection>,
    pub is_collection: bool,
    pub columns: Vec<ColumnInfo>,
    pub jsonb_columns: Vec<ColumnInfo>,
    pub children: Vec<CompositionNode>,
    /// Composite range column collapsed from start/end fields (e.g. TSTZRANGE).
    #[serde(default)]
    pub composite_range: Option<CompositeRange>,
    /// Property names consumed by the composite range (should be excluded from columns).
    #[serde(default)]
    pub consumed_fields: Vec<String>,
}

impl CompositionTree {
    pub fn node_count(&self) -> usize {
        count_nodes(&self.root)
    }

    pub fn all_schema_titles(&self) -> Vec<String> {
        let mut titles = Vec::new();
        collect_titles(&self.root, &mut titles);
        titles
    }

    pub fn leaf_nodes(&self) -> Vec<&CompositionNode> {
        let mut leaves = Vec::new();
        collect_leaves(&self.root, &mut leaves);
        leaves
    }
}

impl CompositionNode {
    pub fn qualified_table_name(&self) -> String {
        format!("{}.{}", self.table_schema, self.table_name)
    }

    pub fn is_root(&self) -> bool {
        self.fk.is_none()
    }

    pub fn parent_fk_column(&self) -> Option<&str> {
        match &self.fk {
            Some(FkDirection::OnParent { column }) => Some(column),
            _ => None,
        }
    }

    pub fn child_fk_column(&self) -> Option<&str> {
        match &self.fk {
            Some(FkDirection::OnChild { column }) => Some(column),
            _ => None,
        }
    }

    pub fn on_parent_children(&self) -> Vec<&CompositionNode> {
        self.children
            .iter()
            .filter(|c| matches!(c.fk, Some(FkDirection::OnParent { .. })))
            .collect()
    }

    pub fn on_child_children(&self) -> Vec<&CompositionNode> {
        self.children
            .iter()
            .filter(|c| matches!(c.fk, Some(FkDirection::OnChild { .. })))
            .collect()
    }

    pub fn dedup_fields(&mut self) {
        use std::collections::HashSet;
        // Use independent sets per category — a shared set would silently
        // remove child nodes when a column and child share the same name.
        // This happens with the VO→entity allOf pattern (commit 33240aa)
        // where build_composition_node pushes both an FK column and a child
        // node for the same property.
        let mut seen_cols = HashSet::new();
        let mut seen_jsonb = HashSet::new();
        let mut seen_children = HashSet::new();
        self.columns.retain(|c| seen_cols.insert(c.name.clone()));
        self.jsonb_columns
            .retain(|c| seen_jsonb.insert(c.name.clone()));
        self.children
            .retain(|c| seen_children.insert(c.field_name.clone()));
        for child in &mut self.children {
            child.dedup_fields();
        }
    }
}

fn count_nodes(node: &CompositionNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

fn collect_titles(node: &CompositionNode, titles: &mut Vec<String>) {
    titles.push(node.schema_title.clone());
    for child in &node.children {
        collect_titles(child, titles);
    }
}

fn collect_leaves<'a>(node: &'a CompositionNode, leaves: &mut Vec<&'a CompositionNode>) {
    if node.children.is_empty() {
        leaves.push(node);
    } else {
        for child in &node.children {
            collect_leaves(child, leaves);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            description: None,
            rust_type: "String".into(),
            postgres_type: "TEXT".into(),
            is_optional: false,
            is_codelist_fk: false,
            composite_columns: vec![],
            is_array: false,
            classification: Some(RefClassificationKind::PrimitiveWrapper),
            fk_target: None,
            check_values: vec![],
        }
    }

    fn make_node(name: &str) -> CompositionNode {
        CompositionNode {
            field_name: name.into(),
            schema_title: format!("{name}_type"),
            table_schema: "recruiting".into(),
            table_name: name.into(),
            fk: None,
            is_collection: false,
            columns: vec![],
            jsonb_columns: vec![],
            children: vec![],
            composite_range: None,
            consumed_fields: vec![],
        }
    }

    fn make_child(name: &str, parent_col: &str) -> CompositionNode {
        CompositionNode {
            fk: Some(FkDirection::OnParent {
                column: parent_col.into(),
            }),
            ..make_node(name)
        }
    }

    #[test]
    fn dedup_fields_removes_duplicate_columns() {
        let mut node = make_node("test");
        node.columns = vec![make_column("a"), make_column("a"), make_column("b")];
        node.dedup_fields();
        assert_eq!(node.columns.len(), 2);
        let names: Vec<&str> = node.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
    }

    #[test]
    fn dedup_fields_removes_duplicate_jsonb_columns() {
        let mut node = make_node("test");
        node.jsonb_columns = vec![make_column("x"), make_column("x")];
        node.dedup_fields();
        assert_eq!(node.jsonb_columns.len(), 1);
    }

    #[test]
    fn dedup_fields_removes_duplicate_children() {
        let mut node = make_node("parent");
        node.children = vec![make_child("child", "parent_id"), make_child("child", "parent_id")];
        node.dedup_fields();
        assert_eq!(node.children.len(), 1);
    }

    #[test]
    fn dedup_fields_independent_hashsets_per_category() {
        let mut node = make_node("parent");
        node.columns = vec![make_column("remote_work")];
        node.children = vec![make_child("remote_work", "remote_work_id")];
        node.dedup_fields();
        assert_eq!(
            node.columns.len(),
            1,
            "column should survive when child shares same field_name"
        );
        assert_eq!(
            node.children.len(),
            1,
            "child should survive when column shares same field_name"
        );
    }

    #[test]
    fn dedup_fields_empty_node() {
        let mut node = make_node("empty");
        node.dedup_fields();
        assert!(node.columns.is_empty());
        assert!(node.jsonb_columns.is_empty());
        assert!(node.children.is_empty());
    }

    #[test]
    fn dedup_fields_recursive() {
        let mut root = make_node("root");
        let mut child = make_child("child", "root_id");
        child.columns = vec![make_column("dup"), make_column("dup"), make_column("unique")];
        root.children = vec![child];
        root.dedup_fields();
        let c = &root.children[0];
        assert_eq!(c.columns.len(), 2);
    }

    #[test]
    fn composition_tree_node_count() {
        let mut root = make_node("root");
        root.children = vec![make_child("a", "root_id"), make_child("b", "root_id")];
        let tree = CompositionTree { root };
        assert_eq!(tree.node_count(), 3);
    }

    #[test]
    fn composition_tree_leaf_nodes() {
        let mut root = make_node("root");
        let mut a = make_child("a", "root_id");
        a.children = vec![make_child("a1", "a_id")];
        root.children = vec![a, make_child("b", "root_id")];
        let tree = CompositionTree { root };
        let leaves = tree.leaf_nodes();
        assert_eq!(leaves.len(), 2);
        let names: Vec<&str> = leaves.iter().map(|n| n.field_name.as_str()).collect();
        assert!(names.contains(&"a1"));
        assert!(names.contains(&"b"));
    }

    #[test]
    fn fk_direction_on_parent_retrieves_column() {
        let n = make_child("child", "parent_id");
        assert_eq!(n.parent_fk_column(), Some("parent_id"));
        assert_eq!(n.child_fk_column(), None);
    }

    #[test]
    fn fk_direction_on_child_retrieves_column() {
        let mut n = make_node("parent");
        n.fk = Some(FkDirection::OnChild {
            column: "child_fk".into(),
        });
        assert_eq!(n.parent_fk_column(), None);
        assert_eq!(n.child_fk_column(), Some("child_fk"));
    }

    #[test]
    fn qualified_table_name() {
        let n = make_node("candidate");
        assert_eq!(n.qualified_table_name(), "recruiting.candidate");
    }

    #[test]
    fn is_root_when_no_fk() {
        let n = make_node("root");
        assert!(n.is_root());
    }

    #[test]
    fn is_not_root_when_has_fk() {
        let n = make_child("child", "parent_id");
        assert!(!n.is_root());
    }

    #[test]
    fn column_info_with_fk_target() {
        let c = ColumnInfo {
            name: "gender_id".into(),
            fk_target: Some(FkTarget {
                schema: "common".into(),
                table: "gender_code_list".into(),
                column: "code".into(),
                on_delete: "RESTRICT".into(),
            }),
            ..make_column("gender_id")
        };
        let fk = c.fk_target.unwrap();
        assert_eq!(fk.schema, "common");
        assert_eq!(fk.table, "gender_code_list");
        assert_eq!(fk.column, "code");
        assert_eq!(fk.on_delete, "RESTRICT");
    }

    #[test]
    fn composition_tree_all_schema_titles() {
        let mut root = make_node("root");
        root.children = vec![make_child("a", "root_id")];
        let tree = CompositionTree { root };
        let titles = tree.all_schema_titles();
        assert_eq!(titles.len(), 2);
        assert!(titles.contains(&"root_type".to_string()));
        assert!(titles.contains(&"a_type".to_string()));
    }
}
