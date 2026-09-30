use std::collections::HashMap;

use codegraph_core::error::GraphError;

use crate::conversions::RowReader;
use crate::engine::GrafeoEngine;

/// Query result wrapper holding columns and rows from Grafeo.
pub(super) struct QResult {
    pub(super) columns: Vec<String>,
    pub(super) rows: Vec<Vec<grafeo::Value>>,
}

pub(super) fn query_gql(engine: &GrafeoEngine, gql: &str) -> Result<QResult, GraphError> {
    let session = engine.db().session();
    let result = session
        .execute(gql)
        .map_err(|e| GraphError::Query(format!("{e}")))?;
    let rows = result.rows().to_vec();
    Ok(QResult {
        columns: result.columns,
        rows,
    })
}

/// Execute a parameterized GQL query. Grafeo can cache query plans for
/// parameterized queries, avoiding repeated parsing of the same template.
pub(super) fn query_gql_params(
    engine: &GrafeoEngine,
    gql: &str,
    params: HashMap<String, grafeo::Value>,
) -> Result<QResult, GraphError> {
    let result = engine
        .db()
        .execute_with_params(gql, params)
        .map_err(|e| GraphError::Query(format!("{e}")))?;
    let rows = result.rows().to_vec();
    Ok(QResult {
        columns: result.columns,
        rows,
    })
}

/// Run a GQL query and map every row through `map`.
pub(super) async fn query_many<T>(
    engine: &GrafeoEngine,
    gql: &str,
    map: impl Fn(&RowReader, &[grafeo::Value]) -> Result<T, GraphError>,
) -> Result<Vec<T>, GraphError> {
    let result = query_gql(engine, gql)?;
    let reader = RowReader::from_columns(&result.columns);
    result.rows.iter().map(|row| map(&reader, row)).collect()
}

/// Run a parameterized GQL query and map every row through `map`.
pub(super) async fn query_many_params<T>(
    engine: &GrafeoEngine,
    gql: &str,
    params: HashMap<String, grafeo::Value>,
    map: impl Fn(&RowReader, &[grafeo::Value]) -> Result<T, GraphError>,
) -> Result<Vec<T>, GraphError> {
    let result = query_gql_params(engine, gql, params)?;
    let reader = RowReader::from_columns(&result.columns);
    result.rows.iter().map(|row| map(&reader, row)).collect()
}

/// Run a GQL query and map the first row through `map`; an empty result set
/// yields `Ok(None)`.
pub(super) async fn query_one<T>(
    engine: &GrafeoEngine,
    gql: &str,
    map: impl Fn(&RowReader, &[grafeo::Value]) -> Result<T, GraphError>,
) -> Result<Option<T>, GraphError> {
    let result = query_gql(engine, gql)?;
    if result.rows.is_empty() {
        return Ok(None);
    }
    let reader = RowReader::from_columns(&result.columns);
    Ok(Some(map(&reader, &result.rows[0])?))
}

/// Run a parameterized GQL query and map the first row through `map`; an
/// empty result set yields `Ok(None)`.
pub(super) async fn query_one_params<T>(
    engine: &GrafeoEngine,
    gql: &str,
    params: HashMap<String, grafeo::Value>,
    map: impl Fn(&RowReader, &[grafeo::Value]) -> Result<T, GraphError>,
) -> Result<Option<T>, GraphError> {
    let result = query_gql_params(engine, gql, params)?;
    if result.rows.is_empty() {
        return Ok(None);
    }
    let reader = RowReader::from_columns(&result.columns);
    Ok(Some(map(&reader, &result.rows[0])?))
}
