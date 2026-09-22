// WP1.2 — collection-cardinality probe (see docs/rosetta/findings/wp1.2-cardinality.md).
//!
//! Question: is `min>1` collection cardinality (Rune DSL `(2..10)`)
//! representable in the codegraph graph and generated output today?
//!
//! Fixture: entity `CrewType` with a required `members` string array carrying
//! `"minItems": 2, "maxItems": 10`, plus a `seat_count` integer carrying
//! `"minimum": 2, "maximum": 10` as the scalar-bounds contrast.

use crate::support;

const CREW_JSON: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "CrewType",
  "description": "A crew with a bounded member list.",
  "type": "object",
  "properties": {
    "id": {
      "description": "Primary identifier.",
      "type": "string",
      "format": "uuid"
    },
    "members": {
      "description": "Crew members.",
      "type": "array",
      "items": { "type": "string" },
      "minItems": 2,
      "maxItems": 10
    },
    "seat_count": {
      "description": "Number of seats.",
      "type": "integer",
      "minimum": 2,
      "maximum": 10
    }
  },
  "required": ["id", "members", "seat_count"]
}"#;

fn crew_files() -> Vec<(&'static str, &'static str)> {
    vec![("CrewType.json", CREW_JSON)]
}

/// Lines mentioning `needle` that also carry a length/range validation
/// marker — used to prove no cardinality validation is emitted for arrays.
fn validation_lines_touching<'a>(content: &'a str, needle: &str) -> Vec<&'a str> {
    content
        .lines()
        .filter(|line| {
            line.contains(needle)
                && (line.contains(".len()")
                    || line.contains("array_length")
                    || line.contains("jsonb_array_length")
                    || line.contains("garde(length")
                    || line.contains("garde(range"))
        })
        .collect()
}

/// Probe 1 — does `minItems`/`maxItems` survive into the graph?
///
/// Expected current behavior: the PropertyNode is ingested with
/// `is_array: true` but the cardinality values are silently dropped; the
/// struct has no fields that could carry them (compile-time fact anchored at
/// crates/codegraph-core/src/types/property.rs:79), and nothing is smuggled
/// into the scalar-constraint fields (`min_length`/`max_length`/
/// `minimum`/`maximum`) either.
#[tokio::test]
async fn array_cardinality_dropped_from_graph() {
    let root = tempfile::tempdir().unwrap();
    let be = support::ingest_into_graph(root.path(), "crew", &["CrewType"], &crew_files()).await;
    let props = be.querier().get_properties("CrewType").await.unwrap();
    let members = props
        .iter()
        .find(|p| p.name == "members")
        .expect("members PropertyNode ingested");

    // What DOES survive: array-ness and requiredness.
    assert!(members.is_array, "members should be ingested as an array");
    assert!(
        members.is_required,
        "members should be ingested as required"
    );
    assert_eq!(members.rust_field_type, "Vec<String>");
    assert!(
        members.pg_column_type.ends_with("[]"),
        "pg column should be a PG array type, got {}",
        members.pg_column_type
    );
    println!(
        "members graph projection: rust={}, pg={}",
        members.rust_field_type, members.pg_column_type
    );

    // The gap: `minItems: 2, maxItems: 10` is gone. No scalar-constraint
    // field picked it up.
    assert_eq!(members.min_length, None);
    assert_eq!(members.max_length, None);
    assert_eq!(members.minimum, None);
    assert_eq!(members.maximum, None);

    // Runtime pin of the compile-time fact: the serialized PropertyNode
    // exposes no cardinality key whatsoever.
    let serialized = serde_json::to_value(members).unwrap();
    let keys: Vec<String> = serialized
        .as_object()
        .expect("PropertyNode serializes to an object")
        .keys()
        .cloned()
        .collect();
    for cardinality_key in ["minItems", "maxItems", "min_items", "max_items"] {
        assert!(
            !keys.iter().any(|k| k.eq_ignore_ascii_case(cardinality_key)),
            "PropertyNode unexpectedly exposes cardinality key {cardinality_key}: {keys:?}"
        );
    }
    println!("PropertyNode serialized keys: {keys:?}");
}

/// Probe 2 — does the DDL enforce array cardinality?
///
/// Expected current behavior: `members` lands as a plain PG array column
/// with no CHECK constraint of any kind. (Note: the `Type` suffix is
/// stripped at generation time, so the table is `crew.crew` in
/// `migrations/<nnn>_crew_crew.sql`.)
#[tokio::test]
async fn array_cardinality_not_enforced_in_ddl() {
    let root = tempfile::tempdir().unwrap();
    let files = support::run_pipeline(root.path(), "crew", &["CrewType"], &crew_files()).await;
    let ddl = support::file_by_suffix(&files, "crew_crew.sql")
        .expect("crew DDL table emitted")
        .clone();

    assert!(
        !ddl.contains("jsonb_array_length") && !ddl.contains("array_length"),
        "DDL should carry no array-length enforcement"
    );
    assert!(
        !ddl.contains("minItems") && !ddl.contains("maxItems"),
        "raw JSON Schema cardinality keywords must not leak into DDL"
    );

    let check_lines: Vec<&str> = ddl.lines().filter(|l| l.contains("CHECK")).collect();
    for line in &check_lines {
        assert!(
            !line.contains("members"),
            "unexpected CHECK constraint on members: {line}"
        );
    }

    let member_lines: Vec<&str> = ddl
        .lines()
        .filter(|l| l.contains("members"))
        .map(|l| l.trim())
        .collect();
    println!("DDL lines mentioning members: {member_lines:?}");
    println!("CHECK constraints in DDL: {check_lines:?}");

    let column_line = member_lines
        .iter()
        .find(|l| l.contains("[]"))
        .expect("members emitted as an array column");
    assert!(
        column_line.contains("TEXT[] NOT NULL"),
        "array column should be a plain NOT NULL PG array, got: {column_line}"
    );
    println!("members column definition: {column_line}");
}

/// Probe 3 — does the generated Rust validate array cardinality?
///
/// Expected current behavior: the array field is emitted as a plain
/// `Vec<String>` (entity model, DTOs) with `#[garde(skip)]` on the request
/// DTO — no length validation, and no minItems artifact anywhere in the
/// generated tree.
#[tokio::test]
async fn array_cardinality_not_validated_in_rust() {
    let root = tempfile::tempdir().unwrap();
    let files = support::run_pipeline(root.path(), "crew", &["CrewType"], &crew_files()).await;

    // The field IS emitted — as a plain Vec — in entity model and DTOs.
    let mut field_emissions: Vec<(String, String)> = Vec::new();
    for (path, content) in &files {
        for line in content.lines().filter(|l| l.contains("pub members")) {
            field_emissions.push((path.clone(), line.trim().to_string()));
        }
    }
    println!("`pub members` emissions: {field_emissions:?}");
    assert!(
        field_emissions
            .iter()
            .any(|(_, l)| l.starts_with("pub members: Vec<")),
        "entity/DTO should emit members as a Vec: {field_emissions:?}"
    );

    // No cardinality artifact anywhere in the generated tree.
    for (path, content) in &files {
        assert!(
            !content.contains("minItems") && !content.contains("maxItems"),
            "{path} leaks the minItems/maxItems keywords"
        );
        let hits = validation_lines_touching(content, "members");
        assert!(
            hits.is_empty(),
            "{path} appears to validate members cardinality: {hits:?}"
        );
    }

    // Record what the Create DTO actually emits for members: a plain Vec
    // under `#[garde(skip)]`.
    let create_dto = files
        .iter()
        .find(|(p, c)| p.ends_with("dto_create.rs") && c.contains("pub members"))
        .map(|(p, c)| (p.clone(), c.clone()))
        .expect("create DTO emitting members");
    let lines: Vec<&str> = create_dto.1.lines().collect();
    if let Some(i) = lines.iter().position(|l| l.contains("pub members")) {
        let from = i.saturating_sub(1);
        println!(
            "create DTO `{}` members block: {:?}",
            create_dto.0,
            &lines[from..=(i).min(lines.len() - 1)]
        );
    }
    let members_idx = lines
        .iter()
        .position(|l| l.contains("pub members"))
        .expect("members field in create DTO");
    assert!(
        members_idx > 0 && lines[members_idx - 1].contains("#[garde(skip)]"),
        "expected #[garde(skip)] on the members field, got: {:?}",
        &lines[members_idx.saturating_sub(1)..=members_idx]
    );
    assert!(
        lines[members_idx].contains("Vec<String>"),
        "members should be a plain Vec<String>: {}",
        lines[members_idx]
    );
}

/// Probe 4 — scalar bounds, for contrast.
///
/// HISTORY: this probe originally pinned gap-analysis defect #1 — bounds
/// were parsed but never persisted by the Grafeo INSERT, so they
/// round-tripped as None and dto_create's garde(range) branch was dead
/// code. The persistence fix (Property DDL columns + INSERT params +
/// RETURN cols) resolved the defect; the probe now pins the FIXED behavior:
/// bounds round-trip and garde(range) fires. Array-cardinality (min>1)
/// remains unrepresentable — that uplift is #261.
#[tokio::test]
async fn scalar_bounds_survive_for_contrast() {
    // Graph side: scalar bounds are ALSO dropped by the graph round-trip.
    let root = tempfile::tempdir().unwrap();
    let be = support::ingest_into_graph(root.path(), "crew", &["CrewType"], &crew_files()).await;
    let props = be.querier().get_properties("CrewType").await.unwrap();
    let seat = props
        .iter()
        .find(|p| p.name == "seat_count")
        .expect("seat_count PropertyNode ingested");
    // Post-fix: bounds persist through the graph round-trip (defect #1
    // resolved). The JSON fixture carries minimum 2 / maximum 10.
    assert_eq!(seat.minimum, Some(rust_decimal::Decimal::from(2)));
    assert_eq!(seat.maximum, Some(rust_decimal::Decimal::from(10)));
    println!(
        "seat_count after graph round-trip: minimum={:?} maximum={:?} rust={}",
        seat.minimum, seat.maximum, seat.rust_field_type
    );

    // DDL side: no numeric CHECK either.
    let root2 = tempfile::tempdir().unwrap();
    let files = support::run_pipeline(root2.path(), "crew", &["CrewType"], &crew_files()).await;
    let ddl = support::file_by_suffix(&files, "crew_crew.sql")
        .expect("crew DDL table emitted")
        .clone();
    let seat_checks: Vec<&str> = ddl
        .lines()
        .filter(|l| l.contains("CHECK") && l.contains("seat_count"))
        .collect();
    assert!(
        seat_checks.is_empty(),
        "unexpected numeric CHECK on seat_count: {seat_checks:?}"
    );
    let seat_lines: Vec<&str> = ddl
        .lines()
        .filter(|l| l.contains("seat_count"))
        .map(|l| l.trim())
        .collect();
    println!("DDL lines mentioning seat_count: {seat_lines:?}");

    // Rust side: the DTO emits plain garde(skip) — the garde(range) branch
    // of dto_create.tera never fires in real runs.
    let create_dto = files
        .iter()
        .find(|(p, c)| p.ends_with("dto_create.rs") && c.contains("pub seat_count"))
        .map(|(p, c)| (p.clone(), c.clone()))
        .expect("create DTO emitting seat_count");
    let lines: Vec<&str> = create_dto.1.lines().collect();
    let seat_idx = lines
        .iter()
        .position(|l| l.contains("pub seat_count"))
        .expect("seat_count field in create DTO");
    println!(
        "create DTO `{}` seat_count block: {:?}",
        create_dto.0,
        &lines[seat_idx.saturating_sub(1)..=seat_idx]
    );
    // Post-fix: the garde(range) branch fires with the persisted bounds.
    assert!(
        create_dto.1.contains("garde(range(min = 2, max = 10))"),
        "garde(range) must fire with the persisted bounds: {}",
        create_dto.1
    );
    assert!(
        !lines[seat_idx - 1].contains("#[garde(skip)]"),
        "seat_count must no longer be garde(skip), got: {}",
        lines[seat_idx - 1]
    );
}
