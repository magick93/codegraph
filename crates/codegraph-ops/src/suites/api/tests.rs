use super::*;
use codegraph_config::{OpsCapabilities, OpsDatabase, OpsDbTarget, OpsManifest};

fn manifest_with(profile: Option<&str>, provider: &str) -> OpsManifest {
    OpsManifest {
        app_name: "demo-app".into(),
        graph_binary: Some("hr-graph".into()),
        schemas_dir: Some("schemas".into()),
        mox_files: Vec::new(),
        rosetta_files: Vec::new(),
        classifier: Some("classifier.toml".into()),
        domain_config: None,
        profile: profile.map(String::from),
        output_dir: "generated-app".into(),
        ui_dir: None,
        smoke: None,
        api_version: "v1".to_string(),
        servers: Default::default(),
        database: OpsDatabase {
            api: OpsDbTarget {
                host: "localhost".into(),
                port: 5432,
                user: "u".into(),
                password: "p".into(),
                database: "postgres".into(),
                reset_sql: None,
                seed_sql: None,
                grant_role: None,
                grant_strict: None,
            },
            e2e: None,
            e2e_app: None,
        },
        supabase: None,
        capabilities: OpsCapabilities {
            persistence_provider: provider.into(),
            ..Default::default()
        },
        hurl: None,
        hooks: vec![],
        extensions: vec![],
        doctor: Default::default(),
        bundle: Default::default(),
    }
}

fn config_for(manifest: OpsManifest) -> OpsConfig {
    OpsConfig::from_manifest(manifest, std::path::PathBuf::from("/tmp/repo")).unwrap()
}

#[test]
fn strips_ansi_codes() {
    assert_eq!(strip_ansi("\u{1b}[0;31mred\u{1b}[0m plain"), "red plain");
    assert_eq!(strip_ansi("no escapes"), "no escapes");
}

#[test]
fn generation_error_summary_is_fatal() {
    assert!(assert_generation_clean("Generated 10568 files | 0 errors | 0 warnings").is_ok());
    assert!(assert_generation_clean("no summary at all").is_ok());
    let err = assert_generation_clean(
        "Generated 10567 files | 1 errors | 0 warnings\nGeneration completed with errors.",
    )
    .expect_err("error summary must be fatal");
    assert!(err.to_string().contains("generation reported errors"));
    let err = assert_generation_clean("Generated 10 files | 2 errors | 0 warnings")
        .expect_err("non-zero error count must be fatal");
    assert!(err.to_string().contains("--allow-gen-errors"));
}

#[test]
fn app_pool_url_targets_app_user_role_on_same_database() {
    let db = crate::pg::PgTarget {
        host: "db.internal".into(),
        port: 5433,
        user: "postgres".into(),
        password: "secret".into(),
        db: "appdb".into(),
        role: "api".into(),
    };
    assert_eq!(
        app_pool_url(&db),
        "postgres://app_user:app_user_pass@db.internal:5433/appdb"
    );
}

#[test]
fn parses_hurl_success_and_requests() {
    assert!(parse_requests("Succeeded files: 1\nExecuted files: 1\nRequests: 12 request\n") > 0);
    assert_eq!(parse_requests("no data"), 0);
}

#[test]
fn hurl_pass_check_is_not_fooled_by_failed_files_percentage() {
    // Real hurl 7 summary lines (aligned columns preserved as-is).
    let success = "Executed files:    1\nExecuted requests: 1 (333.3/s)\n\
                       Succeeded files:   1 (100.0%)\nFailed files:      0 (0.0%)\nDuration: 3 ms";
    assert!(
        hurl_suite_passed(success),
        "a run with Failed files: 0 must pass"
    );

    // Regression: the old `contains("100.0%")` check also matched this.
    let failure = "Executed files:    1\nExecuted requests: 0 (0.0/s)\n\
                       Succeeded files:   0 (0.0%)\nFailed files:      1 (100.0%)\nDuration: 1 ms";
    assert!(
        !hurl_suite_passed(failure),
        "Failed files: 1 (100.0%) must NOT count as a pass"
    );

    let mixed = "Executed files:    3\nSucceeded files:   2 (66.7%)\nFailed files:      1 (33.3%)";
    assert!(!hurl_suite_passed(mixed));

    // Unexpected output (no summary) never claims a pass.
    assert!(!hurl_suite_passed("hurl: command not found"));
    assert!(!hurl_suite_passed(""));
}

#[test]
fn pluralizes_smoke_entity_route() {
    assert_eq!(
        pluralize_entity_route("recruiting/candidate"),
        "recruiting/candidates"
    );
    assert_eq!(
        pluralize_entity_route("compensation/pay-run"),
        "compensation/pay-runs"
    );
    assert_eq!(pluralize_entity_route("common/address"), "common/addresses");
    assert_eq!(pluralize_entity_route("common/status"), "common/statuses");
    assert_eq!(
        pluralize_entity_route("recruiting/category"),
        "recruiting/categories"
    );
    assert_eq!(pluralize_entity_route("candidate"), "candidates");
    assert_eq!(
        pluralize_entity_route("common/employment-permit"),
        "common/employment-permits"
    );
}

#[test]
fn extracts_json_field_dotted() {
    let body = r#"{"data": {"id": "abc-123"}, "meta": {}}"#;
    assert_eq!(extract_json_field(body, "data.id"), "abc-123");
    // Non-string fields return empty.
    assert_eq!(extract_json_field(body, "meta"), "");
    assert_eq!(extract_json_field(body, "nope"), "");
}

#[test]
fn parses_api_key_from_json() {
    assert_eq!(
        parse_api_key_json(r#"{"key":"k-123","org_id":"o"}"#).as_deref(),
        Some("k-123")
    );
    assert_eq!(parse_api_key_json("(0 rows)"), None);
    assert_eq!(parse_api_key_json(""), None);
}

#[test]
fn detects_files_by_name_and_suffix() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("0002_api_key_management.sql"), "x").unwrap();
    std::fs::write(dir.path().join("0100_candidate_rls.sql"), "x").unwrap();
    std::fs::write(dir.path().join("0200_table.sql"), "x").unwrap();
    assert!(find_file(dir.path(), "api_key"));
    assert_eq!(count_files_with_suffix(dir.path(), "_rls.sql"), 1);
}

#[test]
fn parses_status_and_body_split() {
    let text = "body line 1\nbody line 2\n200".to_string();
    let (body, status) = split_status_body(&text);
    assert_eq!(status, "200");
    assert_eq!(body, "body line 1\nbody line 2");
}

#[test]
fn retry_decision_respects_budget() {
    // No retries: even the first failure is final.
    assert!(!should_retry(1, 0));
    // max 3: attempts 1..3 may retry, attempt 4 is final.
    assert!(should_retry(1, 3));
    assert!(should_retry(2, 3));
    assert!(should_retry(3, 3));
    assert!(!should_retry(4, 3));
    // Overflow-safe: max value still allows the documented total.
    assert!(should_retry(1, u32::MAX));
}

#[test]
fn regenerate_args_pass_the_manifest_profile() {
    let cfg = config_for(manifest_with(Some("default"), "sea_orm"));
    let args = regenerate_args(&cfg, "hr-graph");
    let profile_pos = args
        .iter()
        .position(|a| a == "--profile")
        .expect("--profile must be present");
    assert_eq!(args[profile_pos + 1], "default");
    // The other flags stay in place around it.
    assert!(args.contains(&"--schemas".to_string()));
    assert!(args.contains(&"--classifier".to_string()));
    assert!(args.contains(&"--output".to_string()));
    assert!(args.contains(&"run".to_string()));
}

#[test]
fn regenerate_args_omit_profile_when_unset() {
    let cfg = config_for(manifest_with(None, "sea_orm"));
    let args = regenerate_args(&cfg, "hr-graph");
    assert!(!args.contains(&"--profile".to_string()));
}

#[test]
fn cornucopia_env_only_for_cornucopia_provider() {
    let cfg = config_for(manifest_with(Some("default"), "cornucopia"));
    let (key, value) = cornucopia_db_env(&cfg).expect("cornucopia needs the env");
    assert_eq!(key, "CORNUCOPIA_DATABASE_URL");
    assert_eq!(value, cfg.api_db.url());
    let cfg = config_for(manifest_with(Some("default"), "sea_orm"));
    assert!(cornucopia_db_env(&cfg).is_none());
}

#[test]
fn hurl_error_excerpt_keeps_context_after_error_lines() {
    let output = "\
1 | GET http://x/missing
   \u{2022} 01_candidate_crud.hurl:12:3
error: Assert status code
  actual:   500
  expected: 201
  locator: 2
detail 1
detail 2
detail 3
detail 4
detail 5
detail 6
detail 7
detail 8
detail 9
detail 10 (window now exhausted)
2 | GET http://x/ok
   \u{2022} succeeded
error: second failure
  actual:   b
  expected: a
trailing context line
";
    let excerpt = hurl_error_excerpt(output);
    let joined = excerpt.join("\n");
    // The `error:` lines and their following context survive...
    assert!(joined.contains("error: Assert status code"));
    assert!(joined.contains("actual:   500"));
    assert!(joined.contains("expected: 201"));
    assert!(joined.contains("locator: 2"));
    assert!(joined.contains("error: second failure"));
    assert!(joined.contains("actual:   b"));
    // ...while blocks outside every 10-line context window do not.
    assert!(!joined.contains("succeeded"));
    assert!(!joined.contains("window now exhausted"));
}

#[test]
fn hurl_error_excerpt_empty_for_clean_output() {
    assert!(hurl_error_excerpt("all good\nnothing to see").is_empty());
    assert!(hurl_error_excerpt("").is_empty());
}

#[test]
fn hurl_log_lands_in_test_results_dir() {
    let cfg = config_for(manifest_with(None, "sea_orm"));
    let path = hurl_log_path(&cfg, "01_candidate_crud.hurl");
    assert_eq!(
        path,
        std::path::PathBuf::from("/tmp/repo/test-results/hurl/01_candidate_crud.hurl.log")
    );
}

#[test]
fn write_hurl_log_creates_parent_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a/b/c/run.log");
    write_hurl_log(&path, "combined output").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "combined output");
}

#[test]
fn scope_constants_are_well_formed_json() {
    let full: serde_json::Value = serde_json::from_str(FULL_WILDCARD_SCOPES)
        .expect("FULL_WILDCARD_SCOPES must be valid JSON");
    assert_eq!(full[0]["entity_type"], "*");
    assert_eq!(full[0]["action"], "*");

    let read_only: serde_json::Value =
        serde_json::from_str(READ_ONLY_SCOPES).expect("READ_ONLY_SCOPES must be valid JSON");
    assert_eq!(read_only[0]["entity_type"], "*");
    assert_eq!(read_only[0]["action"], "read");
}

#[test]
fn create_api_key_sql_embeds_scopes_verbatim() {
    let scopes = READ_ONLY_SCOPES;
    let sql = format!(
        "SELECT public.create_api_key('{}'::uuid, 'n', '{scopes}'::jsonb);",
        "00000000-0000-0000-0000-000000000001"
    );
    assert!(sql.contains(r#"[{"entity_type":"*","entity_id":"*","action":"read"}]"#));
    assert!(sql.ends_with("'::jsonb);"));
}
