use super::*;
use std::collections::BTreeSet;
use std::path::PathBuf;

use tera::Tera;

use crate::output::{
    clean_generated_output, generator_base, prefix_migration_path, reports_config_dir,
    rust_module_name, test_mod_content, test_module_declaration,
};
use crate::traits::GeneratedFile;

/// Issue #293 acceptance: resolved ux rules serialize into the Tera
/// context so templates can read `project.ux.locale` etc., and stay
/// absent (`skip_serializing_if`) when rules are None.
#[test]
fn project_ux_is_reachable_in_tera_templates() {
    let mut tera = Tera::default();
    tera.add_raw_template("smoke", "locale={{ project.ux.format.locale }}")
        .unwrap();

    let project = ProjectConfig {
        ux: UxConfig {
            ux: Some(codegraph_config::UxRules {
                format: codegraph_config::FormatConfig {
                    locale: "de-DE".into(),
                    currency: Some("EUR".into()),
                },
                ..codegraph_config::UxRules::default()
            }),
        },
        ..ProjectConfig::default()
    };
    let rendered =
        render_template_with_project(&tera, "smoke", &serde_json::json!({}), &project).unwrap();
    assert_eq!(rendered, "locale=de-DE");

    // None keeps the key out of the context entirely (render errors).
    let project = ProjectConfig {
        ux: UxConfig { ux: None },
        ..project
    };
    assert!(
        render_template_with_project(&tera, "smoke", &serde_json::json!({}), &project).is_err(),
        "unset ux must be absent from the context"
    );
}

#[test]
fn test_reports_config_dir_prefers_domain_config_dir_over_cwd() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("reports.toml"), "[[reports]]").unwrap();

    // Supplied dir wins over cwd, so generation composition does not
    // depend on the invoking shell's cwd.
    assert!(reports_config_dir(Some(dir.path()))
        .join("reports.toml")
        .exists());

    // None falls back to the process current directory (legacy behavior).
    assert_eq!(
        reports_config_dir(None),
        std::env::current_dir().unwrap_or_default()
    );
}

#[test]
fn clean_generated_output_preserves_dto_included_in_live_dirs() {
    let dir = tempfile::TempDir::new().unwrap();
    let entity_dir = dir
        .path()
        .join("src")
        .join("domain")
        .join("hr")
        .join("worker");
    std::fs::create_dir_all(&entity_dir).unwrap();
    // dto_included.rs left over from a fullstack run must survive a
    // profile-scoped regen: handlers in the existing output still import
    // it, and generate_mod_files only declares modules whose files exist
    // on disk.
    std::fs::write(entity_dir.join("dto_included.rs"), "pub struct Stale;").unwrap();
    std::fs::write(entity_dir.join("dto_response.rs"), "pub struct Keep;").unwrap();
    // A stale entity directory (not in the generation order) that also
    // carries a dto_included.rs must be removed whole.
    let stale_dir = dir
        .path()
        .join("src")
        .join("domain")
        .join("hr")
        .join("ghost");
    std::fs::create_dir_all(&stale_dir).unwrap();
    std::fs::write(stale_dir.join("dto_included.rs"), "pub struct Ghost;").unwrap();

    let order = vec![GenerationEntry {
        schema_title: "WorkerType".to_string(),
        domain: "hr".to_string(),
        pg_schema: "hr".to_string(),
        is_cyclic: false,
    }];
    clean_generated_output(dir.path(), &order, "Type", false);

    // In-expected entity directories are preserved whole: dto_included.rs
    // is deliberately NOT deleted (an e2e-only regen after a fullstack run
    // must not destroy the fullstack-emitted file while handlers still
    // import it); generate_mod_files re-declares the module only while the
    // file exists.
    assert!(
        entity_dir.join("dto_included.rs").exists(),
        "dto_included.rs in a live entity dir must be preserved across regens"
    );
    assert!(entity_dir.join("dto_response.rs").exists());
    assert!(
        !stale_dir.exists(),
        "stale entity directory must still be removed whole"
    );
}

fn make_migration(name: &str) -> GeneratedFile {
    GeneratedFile {
        path: PathBuf::from("output/migrations").join(name),
        content: String::new(),
    }
}

#[test]
fn test_prefix_migration_path_codelist_range() {
    // Codelists use seq = idx + 10, so range is 10..99
    for idx in 0..20u16 {
        let file = make_migration("common_some_codelist.sql");
        let result = prefix_migration_path(file, (idx + 10) as usize);
        let name = result.path.file_name().unwrap().to_str().unwrap();
        // Must start with 6-digit zero-padded prefix
        let prefix: String = name.chars().take(6).collect();
        assert!(
            prefix.chars().all(|c| c.is_ascii_digit()),
            "codelist prefix must be 6 digits, got: {name}"
        );
        let num: u16 = prefix.parse().unwrap();
        assert!(
            (10..100).contains(&num),
            "codelist seq {num} out of range 10..99 for idx {idx}"
        );
        assert_eq!(&name[6..7], "_", "7th char must be underscore: {name}");
    }
}

#[test]
fn test_prefix_migration_path_entity_range() {
    // Entities start at seq 500 to avoid codelist overlap (codelists use 10..200)
    for idx in 0..20u16 {
        let file = make_migration("recruiting_candidate.sql");
        let result = prefix_migration_path(file, (idx + 500) as usize);
        let name = result.path.file_name().unwrap().to_str().unwrap();
        let prefix: String = name.chars().take(6).collect();
        assert!(
            prefix.chars().all(|c| c.is_ascii_digit()),
            "entity prefix must be 6 digits, got: {name}"
        );
        let num: u16 = prefix.parse().unwrap();
        assert!(
            num >= 500,
            "entity seq {num} should be >= 500 for idx {idx}"
        );
        assert_eq!(&name[6..7], "_", "7th char must be underscore: {name}");
    }
}

#[test]
fn test_prefix_migration_path_sorts_across_five_digit_boundary() {
    // Regression: with 4-digit padding, an entity DDL at seq 9996 and its
    // child RLS at seq 10000 produced `9996_...` vs `10000_...`, which sort
    // in the WRONG order lexicographically — supabase db reset then applies
    // the RLS migration before the table exists (SQLSTATE 42P01).
    // 6-digit padding keeps lexicographic == numeric across the boundary.
    let ddl = prefix_migration_path(make_migration("benefits_dependent.sql"), 9996);
    let rls = prefix_migration_path(
        make_migration("benefits_dependent_child_address_rls.sql"),
        10000,
    );
    let ddl_name = ddl.path.file_name().unwrap().to_str().unwrap();
    let rls_name = rls.path.file_name().unwrap().to_str().unwrap();
    assert_eq!(ddl_name, "009996_benefits_dependent.sql");
    assert_eq!(rls_name, "010000_benefits_dependent_child_address_rls.sql");
    assert!(
        ddl_name < rls_name,
        "DDL must sort before its child RLS across the 9999/10000 boundary"
    );
}

#[test]
fn test_platform_files_use_four_digit_prefix() {
    let platform_files = [
        "0000_extensions.sql",
        "0001_basejump_install.sql",
        "0002_api_key_management.sql",
        "0003_pgmq_setup.sql",
        "0005_platform_schema.sql",
        "0006_workflow_seed.sql",
    ];
    for name in &platform_files {
        let prefix: String = name.chars().take(4).collect();
        assert!(
            prefix.chars().all(|c| c.is_ascii_digit()),
            "platform file must have 4-digit prefix: {name}"
        );
        let num: u16 = prefix.parse().unwrap();
        assert!(
            num < 10,
            "platform file seq {num} should be in 0..9: {name}"
        );
    }
}

#[test]
fn test_migration_sort_order() {
    let mut files = vec![
        // Platform band (0-9)
        "0000_extensions.sql",
        "0001_basejump_install.sql",
        "0002_api_key_management.sql",
        "0003_pgmq_setup.sql",
        "0005_platform_schema.sql",
        "0006_workflow_seed.sql",
        // Codelist band (10-99)
        "0010_common_some_codelist.sql",
        "0011_common_other_codelist.sql",
        // Entity band (100+)
        "0100_recruiting_candidate.sql",
        "0101_recruiting_application.sql",
    ];
    let expected = files.clone();
    files.sort();
    assert_eq!(
        files, expected,
        "migration files must sort in platform < codelist < entity order"
    );
}

#[test]
fn test_prefix_skips_already_numbered_files() {
    // Files that already start with a digit should not be double-prefixed
    let file = make_migration("0003_pgmq_setup.sql");
    let result = prefix_migration_path(file, 42);
    let name = result.path.file_name().unwrap().to_str().unwrap();
    assert_eq!(
        name, "0003_pgmq_setup.sql",
        "already-prefixed file should not be modified"
    );
}

#[test]
fn test_non_migration_file_unchanged() {
    let file = GeneratedFile {
        path: PathBuf::from("output/src/model.rs"),
        content: String::new(),
    };
    let result = prefix_migration_path(file, 10);
    let name = result.path.file_name().unwrap().to_str().unwrap();
    assert_eq!(
        name, "model.rs",
        "non-migration file should not be prefixed"
    );
}

#[test]
fn test_rust_module_name_passthrough() {
    // Names already valid as identifiers must be returned unchanged.
    assert_eq!(rust_module_name("case_test"), "case_test");
    assert_eq!(rust_module_name("individual_profile"), "individual_profile");
}

#[test]
fn test_rust_module_name_sanitizes() {
    // Dashes/spaces become underscores, keywords get _mod, digits get n_.
    assert_eq!(rust_module_name("my-domain"), "my_domain");
    assert_eq!(rust_module_name("my domain"), "my_domain");
    assert_eq!(rust_module_name("type"), "type_mod");
    assert_eq!(rust_module_name("3d"), "n_3d");
}

#[test]
fn test_module_declaration_plain_and_path() {
    // Valid identifier: plain mod declaration.
    assert_eq!(test_module_declaration("core", true), "mod core;");
    // Invalid identifier: #[path] keeps the on-disk name (dir vs file form).
    assert_eq!(
        test_module_declaration("my-domain", true),
        "#[path = \"my-domain\"]\nmod my_domain;"
    );
    assert_eq!(
        test_module_declaration("type", false),
        "#[path = \"type.rs\"]\nmod type_mod;"
    );
}

#[test]
fn test_test_mod_content_joins_declarations() {
    let modules: BTreeSet<String> = [
        "mod case_test;".to_string(),
        "mod case_dto_test;".to_string(),
    ]
    .into_iter()
    .collect();
    let content = test_mod_content(&modules);
    let mut lines: Vec<&str> = content.lines().collect();
    lines.sort_unstable();
    assert_eq!(lines, vec!["mod case_dto_test;", "mod case_test;"]);
    assert!(content.ends_with('\n'), "mod content must end with newline");
}

// ── Workers topology output routing ────────────────────────────────

#[test]
fn test_all_domains_for_generation_includes_entityless_custom_routes() {
    use codegraph_config::config::parse_domain_config_str;
    let config = parse_domain_config_str(
        r#"
[domains.crm]
label = "CRM"
schema_dir = "crm"
postgres_schema = "crm"
entities = ["PersonRecordType"]

[domains.platform]
label = "Platform"
schema_dir = "platform"
postgres_schema = "platform"
entities = []
auto_discover = false
custom_routes = true
"#,
    )
    .unwrap();

    let entries = vec![GenerationEntry {
        schema_title: "PersonRecordType".into(),
        domain: "crm".into(),
        pg_schema: "crm".into(),
        is_cyclic: false,
    }];

    let grouped = all_domains_for_generation(&config, &entries);
    let names: Vec<&str> = grouped.iter().map(|(d, _)| d.as_str()).collect();
    assert!(names.contains(&"crm"));
    // Entity-less custom-routes domain is included with an empty entity list.
    assert!(names.contains(&"platform"));
    let platform = grouped
        .iter()
        .find(|(d, _)| d == "platform")
        .expect("platform should be present");
    assert!(platform.1.is_empty());
}

#[test]
fn test_all_domains_for_generation_skips_entityless_without_flag() {
    use codegraph_config::config::parse_domain_config_str;
    let config = parse_domain_config_str(
        r#"
[domains.crm]
label = "CRM"
schema_dir = "crm"
postgres_schema = "crm"
entities = ["PersonRecordType"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = []
"#,
    )
    .unwrap();

    let entries = vec![GenerationEntry {
        schema_title: "PersonRecordType".into(),
        domain: "crm".into(),
        pg_schema: "crm".into(),
        is_cyclic: false,
    }];

    let grouped = all_domains_for_generation(&config, &entries);
    let names: Vec<&str> = grouped.iter().map(|(d, _)| d.as_str()).collect();
    assert_eq!(names, vec!["crm"]);
}

#[test]
fn test_worker_routed_generator_classification() {
    let routed_entity = [
        "sea_orm_entity",
        "cornucopia_repo",
        "repository",
        "command",
        "query",
        "event",
        "dto",
        "handler",
        "workflow_action",
        "media_route",
    ];
    for name in routed_entity {
        assert!(
            is_worker_routed_entity_generator(name),
            "{name} must route into the per-domain worker crate"
        );
    }
    let root_entity = [
        "ddl",
        "test",
        "ui-page",
        "ui-form",
        "ui-store",
        "ui-e2e-test",
        "playwright-entity",
        "ui-descriptor",
        "ui-shell",
        "lifecycle_trait",
        "domain_types_dto",
        "domain_types_query_service",
        "cli_command",
        "grpc_proto",
        "grpc_service",
    ];
    for name in root_entity {
        assert!(
            !is_worker_routed_entity_generator(name),
            "{name} must stay root-anchored (shared migrations/ui/cli/tests)"
        );
    }

    for name in ["errors", "router", "links"] {
        assert!(
            is_worker_routed_domain_generator(name),
            "{name} must route into the per-domain worker crate"
        );
    }
    for name in ["ui-domain-layout", "cli_domain", "grpc_router"] {
        assert!(
            !is_worker_routed_domain_generator(name),
            "{name} must stay root-anchored"
        );
    }
}

#[test]
fn test_generator_base_workers_and_monolith() {
    let root = PathBuf::from("/out");
    let worker = PathBuf::from("/out/workers/recruiting");

    // Workers topology: routed → worker crate, everything else → root.
    assert_eq!(generator_base(&root, Some(&worker), true), worker);
    assert_eq!(generator_base(&root, Some(&worker), false), root);

    // Monolith topology (no worker base): always the root.
    assert_eq!(generator_base(&root, None, true), root);
    assert_eq!(generator_base(&root, None, false), root);
}

#[test]
fn test_workers_routing_layout_paths() {
    let root = PathBuf::from("/out");
    let worker = root.join("workers").join("recruiting");

    let routed_base = generator_base(&root, Some(&worker), true);
    assert_eq!(
        routed_base.join("src").join("api").join("recruiting"),
        PathBuf::from("/out/workers/recruiting/src/api/recruiting")
    );
    assert_eq!(
        routed_base
            .join("src")
            .join("domain")
            .join("recruiting")
            .join("candidate"),
        PathBuf::from("/out/workers/recruiting/src/domain/recruiting/candidate")
    );
    assert_eq!(
        routed_base.join("src").join("entity").join("candidate.rs"),
        PathBuf::from("/out/workers/recruiting/src/entity/candidate.rs")
    );

    // Root-anchored generators keep the shared output root.
    let root_base = generator_base(&root, Some(&worker), false);
    assert_eq!(
        root_base.join("migrations"),
        PathBuf::from("/out/migrations")
    );
    assert_eq!(root_base.join("ui"), PathBuf::from("/out/ui"));
    assert_eq!(root_base.join("tests"), PathBuf::from("/out/tests"));
}
