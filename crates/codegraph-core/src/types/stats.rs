use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct IngestStats {
    pub schema_count: usize,
    pub property_count: usize,
    pub reference_edge_count: usize,
    pub composition_edge_count: usize,
    pub codelist_count: usize,
    pub enum_value_count: usize,
    pub composite_column_count: usize,
    pub composite_range_count: usize,
    pub domain_count: usize,
    pub ifml_node_count: usize,
    pub duration: Duration,
}
