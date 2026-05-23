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
        let mut seen = HashSet::new();
        self.columns.retain(|c| seen.insert(c.name.clone()));
        self.jsonb_columns.retain(|c| seen.insert(c.name.clone()));
        self.children.retain(|c| seen.insert(c.field_name.clone()));
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
