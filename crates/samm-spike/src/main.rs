use std::path::PathBuf;

use oxirs_samm::codegen::{JsonSchemaGenerator, OpenApiGenerator};
use oxirs_samm::generators::{generate_graphql, generate_sql, SqlDialect};
use oxirs_samm::metamodel::{CharacteristicKind, ModelElement};
use oxirs_samm::parser::parse_aspect_model;
use oxirs_samm::validator::validate_aspect;

fn version_from_urn(urn: &str) -> Option<&str> {
    urn.strip_prefix("urn:")
        .and_then(|u| u.split('#').next())
        .and_then(|u| u.split(':').nth(2))
}

fn dump_characteristic(indent: &str, c: &oxirs_samm::metamodel::Characteristic) {
    let dtype = c.data_type.as_deref().unwrap_or("<none>");
    println!("{indent}kind={:?} data_type={dtype}", c.kind);
    if !c.constraints.is_empty() {
        println!("{indent}constraints:");
        for constraint in &c.constraints {
            println!("{indent}  {constraint:?}");
        }
    }
    let nested: Option<&oxirs_samm::metamodel::Characteristic> = match &c.kind {
        CharacteristicKind::Collection { element_characteristic }
        | CharacteristicKind::List { element_characteristic }
        | CharacteristicKind::Set { element_characteristic }
        | CharacteristicKind::SortedSet { element_characteristic }
        | CharacteristicKind::TimeSeries { element_characteristic } => {
            element_characteristic.as_deref()
        }
        CharacteristicKind::Either { left, right } => {
            println!("{indent}either-left:");
            dump_characteristic(&format!("{indent}  "), left);
            println!("{indent}either-right:");
            dump_characteristic(&format!("{indent}  "), right);
            None
        }
        _ => None,
    };
    if let Some(n) = nested {
        println!("{indent}element:");
        dump_characteristic(&format!("{indent}  "), n);
    }
}

#[tokio::main]
async fn main() {
    let path: PathBuf = std::env::args()
        .nth(1)
        .expect("usage: samm-spike <aspect-model.ttl> [out-dir]")
        .into();
    let out_dir = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let aspect = match parse_aspect_model(&path).await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("parse error: {e:?}");
            std::process::exit(1);
        }
    };

    let urn = aspect.urn();
    println!("== Aspect ==");
    println!("urn:      {urn}");
    println!("name:     {}", aspect.name());
    println!("version:  {}", version_from_urn(urn).unwrap_or("<none>"));
    if let Some(desc) = aspect.metadata().get_description("en") {
        println!("desc:     {desc}");
    }
    if let Some(name) = aspect.metadata().get_preferred_name("en") {
        println!("prefname: {name}");
    }
    println!("properties: {}  operations: {}  events: {}",
        aspect.properties().len(),
        aspect.operations().len(),
        aspect.events().len());

    let validation = match validate_aspect(&aspect).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("validation error: {e:?}");
            std::process::exit(1);
        }
    };
    println!(
        "validation: {} ({} errors, {} warnings)",
        if validation.is_valid { "VALID" } else { "INVALID" },
        validation.errors.len(),
        validation.warnings.len()
    );
    for err in &validation.errors {
        println!("  error: {} (element={:?}, path={:?})",
            err.message, err.element_urn, err.property_path);
    }
    for warn in &validation.warnings {
        println!("  warning: {} (element={:?})", warn.message, warn.element_urn);
    }

    println!();
    println!("== Properties ==");
    for property in aspect.properties() {
        let name = property.name();
        let flags = format!(
            "{}{}{}",
            if property.optional { " optional" } else { "" },
            if property.is_collection { " collection" } else { "" },
            if property.is_abstract { " abstract" } else { "" }
        );
        println!("  {name}{flags}  payload={:?}  extends={:?}",
            property.payload_name, property.extends);
        if !property.example_values.is_empty() {
            println!("      examples: {:?}", property.example_values);
        }
        match &property.characteristic {
            Some(c) => dump_characteristic("      ", c),
            None => println!("      <no characteristic>"),
        }
    }

    println!();
    println!("== Operations ==");
    for op in aspect.operations() {
        println!("  {} (inputs: {}, output: {})",
            op.name(), op.input().len(), op.output().is_some());
        for input in op.input() {
            println!("      in:  {} (optional: {})", input.name(), input.optional);
        }
        if let Some(output) = op.output() {
            println!("      out: {} (optional: {})", output.name(), output.optional);
        }
    }

    println!();
    println!("== Events ==");
    for ev in aspect.events() {
        println!("  {} (params: {})", ev.name(), ev.parameters().len());
    }

    if out_dir.exists() || std::fs::create_dir_all(&out_dir).is_ok() {
        let json = serde_json::to_string_pretty(&aspect).unwrap_or_default();
        std::fs::write(out_dir.join("aspect.json"), json).ok();

        let js_gen = JsonSchemaGenerator::new();
        match js_gen.generate(&aspect) {
            Ok(schema) => {
                let s = serde_json::to_string_pretty(&schema).unwrap_or_default();
                std::fs::write(out_dir.join("aspect.schema.json"), s).ok();
                println!();
                println!("== JSON Schema generated -> aspect.schema.json ==");
            }
            Err(e) => println!("json schema generation failed: {e:?}"),
        }

        match generate_sql(&aspect, SqlDialect::PostgreSql) {
            Ok(sql) => {
                std::fs::write(out_dir.join("aspect.postgres.sql"), &sql).ok();
                println!("== SQL generated -> aspect.postgres.sql ({len} bytes) ==", len = sql.len());
            }
            Err(e) => println!("sql generation failed: {e:?}"),
        }

        match generate_sql(&aspect, SqlDialect::Sqlite) {
            Ok(sql) => {
                std::fs::write(out_dir.join("aspect.sqlite.sql"), &sql).ok();
                println!("== SQL generated -> aspect.sqlite.sql ({len} bytes) ==", len = sql.len());
            }
            Err(e) => println!("sqlite generation failed: {e:?}"),
        }

        match generate_sql(&aspect, SqlDialect::MySql) {
            Ok(sql) => {
                std::fs::write(out_dir.join("aspect.mysql.sql"), &sql).ok();
                println!("== SQL generated -> aspect.mysql.sql ({len} bytes) ==", len = sql.len());
            }
            Err(e) => println!("mysql generation failed: {e:?}"),
        }

        match generate_graphql(&aspect) {
            Ok(gql) => {
                std::fs::write(out_dir.join("aspect.graphql"), &gql).ok();
                println!("== GraphQL generated -> aspect.graphql ({len} bytes) ==", len = gql.len());
            }
            Err(e) => println!("graphql generation failed: {e:?}"),
        }

        match OpenApiGenerator::new("1.0.0", "/api").generate(&aspect) {
            Ok(api) => {
                let s = serde_json::to_string_pretty(&api).unwrap_or_default();
                std::fs::write(out_dir.join("aspect.openapi.json"), s).ok();
                println!("== OpenAPI generated -> aspect.openapi.json ==");
            }
            Err(e) => println!("openapi generation failed: {e:?}"),
        }
    }
}
