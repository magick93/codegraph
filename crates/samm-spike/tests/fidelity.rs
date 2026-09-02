use std::path::PathBuf;

use oxirs_samm::metamodel::{CharacteristicKind, ModelElement};
use oxirs_samm::parser::parse_aspect_model;
use oxirs_samm::validator::validate_aspect;

fn sample(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join(name)
}

#[tokio::test]
async fn movement_parses_with_units_and_enumerations() {
    let aspect = parse_aspect_model(sample("Movement.ttl")).await.expect("parse");

    let speed = aspect
        .properties()
        .iter()
        .find(|p| p.name() == "speed")
        .expect("speed property");
    let c = speed.characteristic.as_ref().expect("characteristic");
    assert!(matches!(
        &c.kind,
        CharacteristicKind::Measurement { unit } if unit.contains("kilometrePerHour")
    ));

    let warning = aspect
        .properties()
        .iter()
        .find(|p| p.name() == "speedLimitWarning")
        .expect("speedLimitWarning property");
    let c = warning.characteristic.as_ref().expect("characteristic");
    assert!(matches!(
        &c.kind,
        CharacteristicKind::Enumeration { values } if values == &["green", "yellow", "red"]
    ));

    let validation = validate_aspect(&aspect).await.expect("validate");
    assert!(validation.is_valid);
}

#[tokio::test]
async fn operations_and_events_parse() {
    let op_aspect = parse_aspect_model(sample("AspectWithOperation.ttl")).await.expect("parse");
    assert_eq!(op_aspect.operations().len(), 2);
    assert_eq!(op_aspect.operations()[0].name(), "testOperation");
    assert!(op_aspect.operations()[0].output().is_some());

    let ev_aspect = parse_aspect_model(sample("AspectWithEvent.ttl")).await.expect("parse");
    assert_eq!(ev_aspect.events().len(), 1);
    assert_eq!(ev_aspect.events()[0].parameters().len(), 1);
}

#[tokio::test]
async fn entity_definitions_are_dropped_by_the_ast() {
    let aspect = parse_aspect_model(sample("Movement.ttl")).await.expect("parse");
    let position = aspect
        .properties()
        .iter()
        .find(|p| p.name() == "position")
        .expect("position property");
    let c = position.characteristic.as_ref().expect("characteristic");
    assert_eq!(
        c.data_type.as_deref(),
        Some("urn:samm:org.eclipse.esmf.examples.movement:1.0.0#SpatialPosition")
    );
    assert!(matches!(c.kind, CharacteristicKind::Trait));
    let json = serde_json::to_string_pretty(&aspect).expect("serialize");
    assert!(!json.contains("latitude"), "entity properties must be absent from the AST");
}

#[tokio::test]
async fn constraints_are_dropped_by_the_ast() {
    let aspect = parse_aspect_model(sample("SalesOrder.ttl")).await.expect("parse");
    let json = serde_json::to_string_pretty(&aspect).expect("serialize");

    for property in aspect.properties() {
        if let Some(c) = &property.characteristic {
            assert!(
                c.constraints.is_empty(),
                "constraints parsed for {}",
                property.name()
            );
        }
    }
    assert!(!json.contains("100000000"), "range constraint must be absent from the AST");
}

#[tokio::test]
async fn quantifiable_units_are_dropped() {
    let aspect = parse_aspect_model(sample("AspectWithUnit.ttl")).await.expect("parse");
    let c = aspect.properties()[0].characteristic.as_ref().expect("characteristic");
    assert!(matches!(c.kind, CharacteristicKind::Trait), "Quantifiable degrades to Trait");
}

#[tokio::test]
async fn anonymous_characteristics_crash_the_parser() {
    let result = parse_aspect_model(sample("AspectWithExtendedEntity.ttl")).await;
    assert!(result.is_err(), "blank-node characteristic must fail (currently unsupported)");
}

#[test]
fn sql_generator_has_fk_support_but_parser_cannot_produce_it() {
    use oxirs_samm::generators::{generate_sql, SqlDialect};
    use oxirs_samm::metamodel::{Aspect, Characteristic, CharacteristicKind, ElementMetadata, Property};

    let mut aspect = Aspect::new("urn:samm:com.example:1.0.0#Order".to_string());
    aspect.metadata = ElementMetadata::new("urn:samm:com.example:1.0.0#Order".to_string());
    let mut prop = Property::new("urn:samm:com.example:1.0.0#customer".to_string());
    prop.characteristic = Some(Characteristic::new(
        "urn:samm:com.example:1.0.0#CustomerRef".to_string(),
        CharacteristicKind::SingleEntity {
            entity_type: "urn:samm:com.example:1.0.0#Customer".to_string(),
        },
    ));
    aspect.add_property(prop);

    let sql = generate_sql(&aspect, SqlDialect::PostgreSql).expect("generate");
    assert!(sql.contains("CONSTRAINT fk_order_customer FOREIGN KEY (customer) REFERENCES customer (id)"));
    assert!(!sql.contains("CREATE TABLE customer"), "entity table is never emitted");

    let sqlite = generate_sql(&aspect, SqlDialect::Sqlite).expect("generate");
    assert!(sqlite.contains("FOREIGN KEY (customer) REFERENCES customer (id)"));
}

#[test]
fn sql_generator_never_emits_constraints() {
    use oxirs_samm::generators::{generate_sql, SqlDialect};
    use oxirs_samm::metamodel::{Aspect, Characteristic, CharacteristicKind, Constraint, ElementMetadata, Property};

    let mut aspect = Aspect::new("urn:samm:com.example:1.0.0#Order".to_string());
    aspect.metadata = ElementMetadata::new("urn:samm:com.example:1.0.0#Order".to_string());
    let mut prop = Property::new("urn:samm:com.example:1.0.0#amount".to_string());
    let mut char = Characteristic::new(
        "urn:samm:com.example:1.0.0#Amount".to_string(),
        CharacteristicKind::Trait,
    );
    char.data_type = Some("http://www.w3.org/2001/XMLSchema#decimal".to_string());
    char.constraints = vec![Constraint::RangeConstraint {
        min_value: Some("0.00".to_string()),
        max_value: Some("100000000.00".to_string()),
        lower_bound_definition: oxirs_samm::metamodel::BoundDefinition::AtLeast,
        upper_bound_definition: oxirs_samm::metamodel::BoundDefinition::Open,
    }];
    prop.characteristic = Some(char);
    aspect.add_property(prop);

    for dialect in [SqlDialect::PostgreSql, SqlDialect::Sqlite, SqlDialect::MySql] {
        let sql = generate_sql(&aspect, dialect).expect("generate");
        assert!(!sql.contains("CHECK"), "no CHECK constraints in {dialect:?} output");
        assert!(!sql.contains("0.00"), "constraint values absent from {dialect:?} output");
    }
}
