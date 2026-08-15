//! Command implementations for `codegraph init` / `doctor` / `add domain`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::init::context::{ProjectFeatures, ProjectTemplateContext};

/// Non-interactive options for `codegraph init`. When `name` is None the
/// command prompts interactively on stdin.
#[derive(Debug, Clone)]
pub struct InitArgs {
    /// Project name (kebab-case). None = prompt.
    pub name: Option<String>,
    /// Directory to create the project in (default: `./{name}`).
    pub output_dir: PathBuf,
    /// Domain names (snake or kebab). Default: `["common"]`.
    pub domains: Vec<String>,
    pub database_target: String,
    pub persistence_provider: String,
    pub deployment_topology: String,
    pub grpc: bool,
    pub ifml: bool,
    pub ops: bool,
    /// Codegraph git rev to pin (default: the running binary's embedded rev).
    pub rev: Option<String>,
    /// Use local path deps for codegraph crates instead of git+rev.
    pub codegraph_path: Option<PathBuf>,
    /// Overwrite existing files.
    pub force: bool,
    /// Additional template directories (later dirs take precedence).
    pub template_dirs: Vec<PathBuf>,
}

/// Normalize a raw project name to kebab-case. Handles "My App" → "my-app",
/// "Already_Snake" → "already-snake", and passes "demo-app" through unchanged.
fn normalize_project_name(raw: &str) -> String {
    let snake: String = heck::ToSnakeCase::to_snake_case(raw.trim());
    heck::ToKebabCase::to_kebab_case(snake.as_str())
}

/// Normalize a raw domain name to snake_case ("Billing" → "billing",
/// "Order Items" → "order_items").
fn normalize_domain_name(raw: &str) -> String {
    heck::ToSnakeCase::to_snake_case(raw.trim())
}

/// Resolve the target project directory.
///
/// Convention: `--output` names the *parent* directory. The default `"."`
/// yields `./{name}`; an explicit `--output /tmp/foo` yields
/// `/tmp/foo/{name}`. When the output dir's basename already equals the
/// project name it is treated as the project dir itself.
fn resolve_target_dir(output_dir: &Path, name: &str) -> PathBuf {
    if output_dir.as_os_str().is_empty() || output_dir == Path::new(".") {
        PathBuf::from(name)
    } else if output_dir.file_name().and_then(|f| f.to_str()) == Some(name) {
        output_dir.to_path_buf()
    } else {
        output_dir.join(name)
    }
}

/// Return the final paths (relative to `target`) that already exist on disk.
fn would_overwrite(target: &Path, files: &[(PathBuf, String)]) -> Vec<PathBuf> {
    files
        .iter()
        .map(|(rel, _)| target.join(rel))
        .filter(|p| p.exists())
        .collect()
}

/// Prompt for a project name on stdin until a non-blank value is given.
fn prompt_project_name() -> Result<String> {
    loop {
        print!("Project name (kebab-case): ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            eprintln!("project name cannot be empty — try again");
            continue;
        }
        return Ok(normalize_project_name(trimmed));
    }
}

/// Print a compact grouped file listing for the scaffold summary.
fn print_file_tree(rel_paths: &[PathBuf]) {
    let mut sorted: Vec<&PathBuf> = rel_paths.iter().collect();
    sorted.sort();
    let mut last_dir: Option<PathBuf> = None;
    for rel in sorted {
        let dir = rel.parent().map(|d| d.to_path_buf()).unwrap_or_default();
        if last_dir.as_deref() != Some(dir.as_path()) {
            let label = if dir.as_os_str().is_empty() {
                ".".to_string()
            } else {
                dir.display().to_string()
            };
            println!("{label}");
            last_dir = Some(dir);
        }
        let name = rel
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        println!("  {name}");
    }
}

/// Scaffold a new consumer project. Refuses to overwrite existing files
/// unless `force`; writes only inside `output_dir` (path containment guard).
pub fn cmd_init(args: &InitArgs) -> Result<()> {
    let name = match &args.name {
        Some(raw) => normalize_project_name(raw),
        None => prompt_project_name()?,
    };
    if name.is_empty() {
        return Err(Error::Config("project name cannot be empty".to_string()));
    }

    let target_dir = resolve_target_dir(&args.output_dir, &name);
    fs::create_dir_all(&target_dir)?;
    let canonical_target = target_dir.canonicalize().map_err(|e| {
        Error::Config(format!(
            "cannot resolve project dir '{}': {e}",
            target_dir.display()
        ))
    })?;

    let rev = match &args.rev {
        Some(r) if !r.trim().is_empty() => r.trim().to_string(),
        _ => {
            let embedded = crate::rev::codegraph_rev();
            if embedded.is_empty() {
                eprintln!(
                    "WARN: no --rev given and this binary has no embedded git rev; \
                     the generated Cargo.toml will pin no codegraph revision"
                );
            }
            embedded.to_string()
        }
    };

    let codegraph_path = match &args.codegraph_path {
        Some(p) => Some(p.canonicalize().map_err(|e| {
            Error::Config(format!(
                "--codegraph-path '{}' is not accessible: {e}",
                p.display()
            ))
        })?),
        None => None,
    };

    let domains: Vec<String> = args
        .domains
        .iter()
        .map(|d| normalize_domain_name(d))
        .collect();

    let features = ProjectFeatures {
        grpc: args.grpc,
        ifml: args.ifml,
        ops: args.ops,
    };

    let ctx = ProjectTemplateContext::new(
        &name,
        &domains,
        &rev,
        codegraph_path.as_deref(),
        &args.database_target,
        &args.persistence_provider,
        &args.deployment_topology,
        features,
    );

    let tera = if args.template_dirs.is_empty() {
        let td = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        crate::generate::template_engine::create_tera(&td)?
    } else {
        let dirs: Vec<&Path> = args.template_dirs.iter().map(|p| p.as_path()).collect();
        crate::generate::template_engine::create_tera_with_overrides(&dirs)?
    };

    let files = ctx.render(&tera).map_err(Error::Template)?;

    if !args.force {
        let existing = would_overwrite(&target_dir, &files);
        if !existing.is_empty() {
            let mut msg = String::from("refusing to overwrite existing files (use --force):\n");
            for path in &existing {
                msg.push_str(&format!("  {}\n", path.display()));
            }
            return Err(Error::Config(msg.trim_end().to_string()));
        }
    }

    let mut written = Vec::with_capacity(files.len());
    for (rel, content) in &files {
        let final_path = target_dir.join(rel);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
            let canonical_parent = parent.canonicalize().map_err(|e| {
                Error::Config(format!(
                    "cannot resolve parent dir '{}': {e}",
                    parent.display()
                ))
            })?;
            if !canonical_parent.starts_with(&canonical_target) {
                return Err(Error::Config(format!(
                    "refusing to write outside the project dir: '{}'",
                    final_path.display()
                )));
            }
        }
        fs::write(&final_path, content)?;
        written.push(rel.clone());
    }

    print_file_tree(&written);
    println!();
    println!(
        "Project scaffolded in {} ({} files). Next: just generate",
        target_dir.display(),
        written.len()
    );
    Ok(())
}

/// Options for `codegraph doctor`.
#[derive(Debug, Clone)]
pub struct DoctorArgs {
    /// domains.toml path (default: "domains.toml").
    pub config: PathBuf,
    /// JSON schemas dir (default: "schemas").
    pub schemas: PathBuf,
    /// classifier.toml path (default: "classifier.toml").
    pub classifier: PathBuf,
    /// profiles.toml path (optional; skipped when absent and profile is default).
    pub profiles_config: Option<PathBuf>,
}

/// Extract every `rev = "<sha>"` value from lines that reference the
/// magick93/codegraph.git dependency (rev may sit on the following line).
fn extract_codegraph_revs(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut revs = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !line.contains("magick93/codegraph.git") {
            continue;
        }
        let mut window = String::from(*line);
        if let Some(next) = lines.get(i + 1) {
            window.push(' ');
            window.push_str(next);
        }
        let mut rest = window.as_str();
        while let Some(pos) = rest.find("rev") {
            let after = &rest[pos + 3..];
            let Some(eq) = after.find('=') else { break };
            let after_eq = after[eq + 1..].trim_start();
            let Some(stripped) = after_eq.strip_prefix('"') else {
                break;
            };
            let Some(end) = stripped.find('"') else { break };
            if !stripped[..end].is_empty() {
                revs.push(stripped[..end].to_string());
            }
            rest = &stripped[end + 1..];
        }
    }
    revs
}

/// Compare the Cargo.toml codegraph rev pins against this binary's embedded
/// rev. Returns the number of warnings raised.
fn check_codegraph_rev() -> usize {
    let cargo_toml = match std::env::current_dir() {
        Ok(cwd) => cwd.join("Cargo.toml"),
        Err(_) => PathBuf::from("Cargo.toml"),
    };
    if !cargo_toml.is_file() {
        println!("WARN no Cargo.toml in current directory — rev check skipped");
        return 1;
    }
    let text = match fs::read_to_string(&cargo_toml) {
        Ok(t) => t,
        Err(e) => {
            println!("WARN cannot read {} — {e}", cargo_toml.display());
            return 1;
        }
    };
    let revs = extract_codegraph_revs(&text);
    if revs.is_empty() {
        println!("WARN Cargo.toml pins codegraph crates without a git rev (branch/tag deps)");
        println!("     hint: pin codegraph deps with rev = \"<sha>\"");
        return 1;
    }
    let embedded = crate::rev::codegraph_rev();
    if embedded.is_empty() {
        println!("WARN this binary has no embedded rev — cannot compare Cargo.toml pins");
        return 1;
    }
    if revs.iter().any(|r| r != embedded) {
        println!("WARN Cargo.toml pins codegraph rev {revs:?} but this binary is {embedded}");
        println!("     hint: update the rev pins to match the codegraph binary you generate with");
        1
    } else {
        println!("PASS Cargo.toml pins codegraph rev {embedded}");
        0
    }
}

/// Validate an existing consumer project. Prints pass/fail checks and
/// returns Err when any hard check fails.
pub fn cmd_doctor(args: &DoctorArgs) -> Result<()> {
    let mut hard_failures: usize = 0;
    let mut soft_warnings: usize = 0;

    println!("codegraph doctor");

    match codegraph_config::config::parse_domain_config(&args.config) {
        Ok(config) => println!(
            "PASS domains.toml — {} domain(s) configured",
            config.domains.len()
        ),
        Err(e) => {
            hard_failures += 1;
            println!("FAIL domains.toml — {e}");
            println!("     hint: fix TOML syntax in {}", args.config.display());
        }
    }

    match codegraph_classifier::config::parse_classifier_config(&args.classifier) {
        Ok(config) => println!(
            "PASS classifier.toml — {} naming rule(s)",
            config.naming_rules.len()
        ),
        Err(e) => {
            hard_failures += 1;
            println!("FAIL classifier.toml — {e}");
            println!(
                "     hint: fix TOML syntax in {}",
                args.classifier.display()
            );
        }
    }

    let profiles_path = args
        .profiles_config
        .clone()
        .unwrap_or_else(|| PathBuf::from("profiles.toml"));
    if profiles_path.exists() {
        match crate::profile::load_and_resolve_profile(&profiles_path, "default", None).and_then(
            |resolved| {
                let registry = crate::profile::CapabilityRegistry::new();
                crate::profile::BuildPlan::from_profile(&resolved, &registry)
            },
        ) {
            Ok(plan) => println!(
                "PASS profiles.toml — {} entity, {} domain, {} global generator(s)",
                plan.entity_generators.len(),
                plan.domain_generators.len(),
                plan.global_generators.len()
            ),
            Err(e) => {
                hard_failures += 1;
                println!("FAIL profiles.toml — {e}");
                println!(
                    "     hint: fix profile config in {}",
                    profiles_path.display()
                );
            }
        }
    } else {
        soft_warnings += 1;
        println!("WARN no profiles.toml — running all generators");
        println!("     hint: run `codegraph init` to scaffold one");
    }

    if args.schemas.is_dir() {
        let has_json = walkdir::WalkDir::new(&args.schemas)
            .into_iter()
            .filter_map(|e| e.ok())
            .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"));
        if has_json {
            println!(
                "PASS schemas — {} contains JSON schema(s)",
                args.schemas.display()
            );
        } else {
            hard_failures += 1;
            println!(
                "FAIL schemas — no *.json files under {}",
                args.schemas.display()
            );
            println!("     hint: add JSON schemas or run `codegraph add domain <name>`");
        }
    } else {
        hard_failures += 1;
        println!("FAIL schemas — {} does not exist", args.schemas.display());
        println!("     hint: create the directory and add JSON schemas");
    }

    let mut manifest_candidates = vec![PathBuf::from("codegraph-ops.toml")];
    if let Some(parent) = args.schemas.parent() {
        manifest_candidates.push(parent.join("codegraph-ops.toml"));
    }
    match manifest_candidates.iter().find(|p| p.is_file()) {
        Some(path) => match codegraph_ops::config::OpsConfig::load(path) {
            Ok(config) => println!(
                "PASS codegraph-ops.toml — app '{}', output {}",
                config.manifest.app_name,
                config.app_dir.display()
            ),
            Err(e) => {
                hard_failures += 1;
                println!("FAIL codegraph-ops.toml — {e}");
                println!("     hint: fix manifest syntax in {}", path.display());
            }
        },
        None => {
            soft_warnings += 1;
            println!("WARN no codegraph-ops.toml — run generation with the ops generator");
        }
    }

    soft_warnings += check_codegraph_rev();

    match codegraph_ops::env::resolve_psql() {
        Some(psql) => println!("PASS psql — {}", psql.display()),
        None => {
            soft_warnings += 1;
            println!("WARN psql not found");
            println!(
                "     hint: install postgresql-client or set PSQL_PATH (required by the api suite)"
            );
        }
    }
    match codegraph_ops::env::resolve_npx() {
        Some(npx) => println!("PASS npx — {}", npx.display()),
        None => {
            soft_warnings += 1;
            println!("WARN npx not found");
            println!("     hint: install node — npx is used for supabase/playwright");
        }
    }
    match std::process::Command::new("hurl").arg("--version").output() {
        Ok(_) => println!("PASS hurl — on PATH"),
        Err(_) => {
            soft_warnings += 1;
            println!("WARN hurl not found");
            println!("     hint: install hurl for api contract tests");
        }
    }

    println!();
    if hard_failures > 0 {
        Err(Error::Config(format!(
            "doctor: {hard_failures} hard check(s) failed, {soft_warnings} warning(s)"
        )))
    } else {
        println!("doctor: all hard checks passed ({soft_warnings} warning(s))");
        Ok(())
    }
}

/// Minimal JSON schema used when `project/example_schema.tera` is unavailable
/// or fails to render. Same shape: id uuid, name, description, created_at;
/// required [id, name].
fn fallback_example_schema() -> String {
    let schema = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "ItemType",
        "type": "object",
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "name": { "type": "string" },
            "description": { "type": "string" },
            "created_at": { "type": "string", "format": "date-time" }
        },
        "required": ["id", "name"]
    });
    let mut out = serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".to_string());
    out.push('\n');
    out
}

/// Render `project/example_schema.tera` for `domain_name`, falling back to a
/// minimal inline schema when the template is missing or renders invalid JSON.
fn example_schema(domain_name: &str) -> String {
    let builtin = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    if let Ok(tera) = crate::generate::template_engine::create_tera(&builtin) {
        let ctx = ProjectTemplateContext::new(
            "example",
            &[domain_name.to_string()],
            "",
            None,
            "postgres",
            "sea_orm",
            "monolith",
            ProjectFeatures {
                grpc: false,
                ifml: false,
                ops: true,
            },
        );
        if let Ok(tctx) = tera::Context::from_serialize(&ctx) {
            if let Ok(rendered) = tera.render("project/example_schema.tera", &tctx) {
                if serde_json::from_str::<serde_json::Value>(&rendered).is_ok() {
                    return rendered;
                }
            }
        }
    }
    fallback_example_schema()
}

/// Append a `[domains.<name>]` entry to `config_path` and create
/// `schemas_dir/<name>/` with an example schema. Refuses duplicate domains.
pub fn cmd_add_domain(config_path: &Path, schemas_dir: &Path, domain_name: &str) -> Result<()> {
    let name = normalize_domain_name(domain_name);
    if name.is_empty() {
        return Err(Error::Config("domain name cannot be empty".to_string()));
    }

    let existing = codegraph_config::config::parse_domain_config(config_path)
        .map_err(|e| Error::Config(format!("parse '{}': {e}", config_path.display())))?;
    if existing.domains.contains_key(&name) {
        return Err(Error::Config(format!(
            "domain '{name}' already exists in {}",
            config_path.display()
        )));
    }

    let raw = fs::read_to_string(config_path)
        .map_err(|e| Error::Config(format!("read '{}': {e}", config_path.display())))?;
    let mut value: toml::Value = toml::from_str(&raw)
        .map_err(|e| Error::Config(format!("parse '{}': {e}", config_path.display())))?;

    let root = value.as_table_mut().ok_or_else(|| {
        Error::Config(format!(
            "'{}' is not a TOML document",
            config_path.display()
        ))
    })?;
    if !root.contains_key("domains") {
        root.insert(
            "domains".to_string(),
            toml::Value::Table(toml::Table::new()),
        );
    }
    let domains_table = root
        .get_mut("domains")
        .and_then(|d| d.as_table_mut())
        .ok_or_else(|| {
            Error::Config(format!(
                "'domains' in {} is not a TOML table",
                config_path.display()
            ))
        })?;

    let label: String = heck::ToTitleCase::to_title_case(name.as_str());
    let mut entry = toml::Table::new();
    entry.insert("label".to_string(), toml::Value::String(label.clone()));
    entry.insert("schema_dir".to_string(), toml::Value::String(name.clone()));
    entry.insert(
        "postgres_schema".to_string(),
        toml::Value::String(name.clone()),
    );
    domains_table.insert(name.clone(), toml::Value::Table(entry));

    let new_content = toml::to_string_pretty(&value)
        .map_err(|e| Error::Config(format!("serialize '{}': {e}", config_path.display())))?;

    let re_parsed = codegraph_config::config::parse_domain_config_str(&new_content)
        .map_err(|e| Error::Config(format!("re-parse domains.toml after append: {e}")))?;
    if !re_parsed.domains.contains_key(&name) {
        return Err(Error::Config(format!(
            "internal error: appended domain '{name}' missing after round-trip"
        )));
    }

    fs::write(config_path, &new_content)?;

    let domain_schemas = schemas_dir.join(&name);
    fs::create_dir_all(&domain_schemas)?;
    let example_path = domain_schemas.join("example.json");
    if !example_path.exists() {
        fs::write(&example_path, example_schema(&name))?;
    }

    println!("Added domain '{name}' (label {label}, schema_dir {name}, postgres_schema {name})");
    println!("Updated {}", config_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn normalizes_project_names_to_kebab() {
        assert_eq!(normalize_project_name("My App"), "my-app");
        assert_eq!(normalize_project_name("demo-app"), "demo-app");
        assert_eq!(normalize_project_name("Already_Snake"), "already-snake");
        assert_eq!(normalize_project_name("  Spaces Around  "), "spaces-around");
    }

    #[test]
    fn normalizes_domain_names_to_snake() {
        assert_eq!(normalize_domain_name("Billing"), "billing");
        assert_eq!(normalize_domain_name("Order Items"), "order_items");
        assert_eq!(normalize_domain_name("pay-roll"), "pay_roll");
    }

    #[test]
    fn resolve_target_dir_uses_name_under_parent() {
        assert_eq!(
            resolve_target_dir(Path::new("."), "demo-app"),
            PathBuf::from("demo-app")
        );
        assert_eq!(
            resolve_target_dir(Path::new("/tmp/foo"), "demo-app"),
            PathBuf::from("/tmp/foo/demo-app")
        );
        assert_eq!(
            resolve_target_dir(Path::new("/tmp/foo/demo-app"), "demo-app"),
            PathBuf::from("/tmp/foo/demo-app")
        );
    }

    #[test]
    fn would_overwrite_only_reports_existing_files() {
        let dir = TempDir::new().unwrap();
        let existing = dir.path().join("keep.toml");
        fs::write(&existing, "x").unwrap();
        let files = vec![
            (PathBuf::from("keep.toml"), String::new()),
            (PathBuf::from("fresh.toml"), String::new()),
        ];
        let conflicts = would_overwrite(dir.path(), &files);
        assert_eq!(conflicts, vec![existing]);
    }

    #[test]
    fn extract_codegraph_revs_finds_pins_on_same_and_next_line() {
        let text = r#"
[dependencies]
codegraph = { git = "https://github.com/magick93/codegraph.git", rev = "aaaa" }
codegraph-config = { git = "https://github.com/magick93/codegraph.git",
                     rev = "bbbb" }
other = "0.1"
"#;
        let revs = extract_codegraph_revs(text);
        assert_eq!(revs, vec!["aaaa".to_string(), "bbbb".to_string()]);
    }

    #[test]
    fn add_domain_appends_and_rejects_duplicates() {
        let dir = TempDir::new().unwrap();
        let config = dir.path().join("domains.toml");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/domains.toml");
        fs::copy(&fixture, &config).unwrap();
        let schemas = dir.path().join("schemas");

        cmd_add_domain(&config, &schemas, "Billing").unwrap();

        let parsed = codegraph_config::config::parse_domain_config(&config).unwrap();
        assert!(parsed.domains.contains_key("billing"));
        assert_eq!(parsed.domains["billing"].label, "Billing");
        assert_eq!(parsed.domains["billing"].schema_dir, "billing");
        assert_eq!(parsed.domains["billing"].postgres_schema, "billing");

        let example = schemas.join("billing/example.json");
        assert!(example.is_file());
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&example).unwrap()).unwrap();
        assert_eq!(json["title"], "ItemType");
        assert!(json["properties"]["id"]["format"] == "uuid");

        let err = cmd_add_domain(&config, &schemas, "billing").unwrap_err();
        assert!(format!("{err}").contains("already exists"), "{err}");
    }
}
