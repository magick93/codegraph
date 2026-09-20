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
    /// JSON schemas dir. None = not provided: in mox mode the check
    /// degrades to an info line (mox-first projects carry no schemas
    /// directory); without mox files it stays a hard failure.
    pub schemas: Option<PathBuf>,
    /// classifier.toml path. None = not provided: only a hard failure when
    /// the schemas dir (when given) contains JSON schemas.
    pub classifier: Option<PathBuf>,
    /// profiles.toml path (optional; skipped when absent and profile is default).
    pub profiles_config: Option<PathBuf>,
    /// rexlang .mox domain model files (optional). When present each
    /// package must match a domains.toml domain, and the JSON schemas
    /// check degrades to an info line (mox-first projects).
    pub mox_files: Vec<PathBuf>,
}

/// Outcome counts for a doctor run. `model_warnings` isolates the
/// model-source checks (schemas / classifier / mox): the intentional
/// mox-first new-project shape must produce ZERO of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoctorSummary {
    pub hard_failures: usize,
    pub soft_warnings: usize,
    pub model_warnings: usize,
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
        if text.contains("magick93/codegraph.git") {
            println!("WARN Cargo.toml pins codegraph crates without a git rev (branch/tag deps)");
            println!("     hint: pin codegraph deps with rev = \"<sha>\"");
            return 1;
        }
        if text.contains("codegraph") && text.contains("path =") {
            println!("PASS Cargo.toml uses local codegraph path deps (development mode)");
            return 0;
        }
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

/// Validate `--mox-files` for doctor: every file must compile with the rex
/// compiler and every package must resolve to a domains.toml domain.
/// `import schema` targets (issue #230) are validated too — a missing or
/// invalid target is a hard failure naming the import path and the .mox
/// that declares it. Returns the (hard_failures, soft_warnings) contributed.
fn check_mox_files(
    mox_files: &[PathBuf],
    domain_config: Option<&codegraph_config::config::DomainConfig>,
) -> (usize, usize) {
    let mut hard = 0;
    let mut soft = 0;
    for path in mox_files {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                hard += 1;
                println!("FAIL mox — cannot read {}: {e}", path.display());
                continue;
            }
        };
        let mox_path = path.display().to_string();
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let mut schema_imports = rex_driver::SchemaImports::new();
        let mut import_failures = 0usize;
        let mut import_count = 0usize;
        for decl in crate::ingest::mox_ingest::scan_schema_imports(&text) {
            let abs_path = base_dir.join(&decl.path);
            let json = match fs::read_to_string(&abs_path) {
                Ok(json) => json,
                Err(e) => {
                    hard += 1;
                    import_failures += 1;
                    println!(
                        "FAIL mox — {mox_path}: import schema '{}' cannot be read: {e}",
                        decl.path
                    );
                    println!(
                        "     hint: create the file or fix the path (resolved relative to the .mox file's directory)"
                    );
                    continue;
                }
            };
            if let Err(e) = serde_json::from_str::<serde_json::Value>(&json) {
                hard += 1;
                import_failures += 1;
                println!(
                    "FAIL mox — {mox_path}: import schema '{}' is not valid JSON: {e}",
                    decl.path
                );
                println!("     hint: fix the JSON syntax in {}", abs_path.display());
                continue;
            }
            schema_imports.insert(&mox_path, &decl.path, json);
            import_count += 1;
        }
        if import_failures > 0 {
            continue;
        }
        let compilation =
            rex_driver::compile_files_with_imports(&[(mox_path.clone(), text)], &schema_imports);
        for (p, diagnostic) in &compilation.diagnostics {
            println!("WARN mox diagnostic in {p}: {}", diagnostic.message);
            soft += 1;
        }
        let Some(model) = compilation.model else {
            hard += 1;
            println!("FAIL mox — {} does not compile", path.display());
            println!("     hint: fix the rexlang syntax errors reported above");
            continue;
        };
        let Some(config) = domain_config else {
            // domains.toml already reported a hard failure above.
            continue;
        };
        let unmatched: Vec<String> = model
            .packages
            .iter()
            .filter(|package| !crate::ingest::mox_ingest::resolve_domain(config, &package.name).1)
            .map(|package| package.name.clone())
            .collect();
        if unmatched.is_empty() {
            if import_count > 0 {
                println!(
                    "PASS mox — {} compiles; every package matches domains.toml; \
                     {import_count} schema import(s) resolved",
                    path.display()
                );
            } else {
                println!(
                    "PASS mox — {} compiles; every package matches domains.toml",
                    path.display()
                );
            }
        } else {
            hard += 1;
            println!(
                "FAIL mox — package(s) with no matching domains.toml entry: {}",
                unmatched.join(", ")
            );
            println!("     hint: add a [domains.<name>] entry or rename the package");
        }
    }
    (hard, soft)
}

/// Validate an existing consumer project. Prints pass/fail checks and
/// returns Err when any hard check fails; Ok carries the outcome counts.
pub fn cmd_doctor(args: &DoctorArgs) -> Result<DoctorSummary> {
    let mut hard_failures: usize = 0;
    let mut soft_warnings: usize = 0;
    let mut model_warnings: usize = 0;

    println!("codegraph doctor");

    let domain_config = codegraph_config::config::parse_domain_config(&args.config);
    match &domain_config {
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

    // Scan the schemas dir (when provided) up front: the classifier verdict
    // depends on whether JSON schemas are present.
    let schemas_dir = args.schemas.as_deref();
    let mut schemas_dir_exists = false;
    let mut schemas_has_json = false;
    if let Some(dir) = schemas_dir {
        if dir.is_dir() {
            schemas_dir_exists = true;
            schemas_has_json = walkdir::WalkDir::new(dir)
                .into_iter()
                .filter_map(|e| e.ok())
                .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"));
        }
    }

    match &args.classifier {
        Some(classifier) => {
            match codegraph_classifier::config::parse_classifier_config(classifier) {
                Ok(config) => println!(
                    "PASS classifier.toml — {} naming rule(s)",
                    config.naming_rules.len()
                ),
                Err(e) => {
                    hard_failures += 1;
                    println!("FAIL classifier.toml — {e}");
                    println!("     hint: fix TOML syntax in {}", classifier.display());
                }
            }
        }
        None if schemas_has_json => {
            hard_failures += 1;
            println!("FAIL classifier.toml — not provided but JSON schemas are present");
            println!("     hint: pass --classifier (JSON schemas need classification rules)");
        }
        None => {
            println!(
                "INFO classifier.toml — not provided (no JSON schemas; \
                 mox-first projects need no classifier)"
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

    let mox_mode = !args.mox_files.is_empty();
    if schemas_dir_exists {
        if schemas_has_json {
            println!(
                "PASS schemas — {} contains JSON schema(s)",
                schemas_dir.unwrap().display()
            );
        } else if mox_mode {
            soft_warnings += 1;
            model_warnings += 1;
            println!(
                "WARN schemas — no *.json files under {} (mox-first project)",
                schemas_dir.unwrap().display()
            );
        } else {
            hard_failures += 1;
            println!(
                "FAIL schemas — no *.json files under {}",
                schemas_dir.unwrap().display()
            );
            println!("     hint: add JSON schemas or run `codegraph add domain <name>`");
        }
    } else if mox_mode {
        println!("INFO schemas — mox-first project; no JSON schemas directory");
    } else {
        hard_failures += 1;
        match schemas_dir {
            Some(dir) => println!("FAIL schemas — {} does not exist", dir.display()),
            None => println!("FAIL schemas — no schemas directory and no --mox-files"),
        }
        println!("     hint: create the directory and add JSON schemas");
    }

    if mox_mode {
        let (hard, soft) = check_mox_files(&args.mox_files, domain_config.as_ref().ok());
        hard_failures += hard;
        soft_warnings += soft;
        model_warnings += soft;
    } else if schemas_has_json {
        soft_warnings += 1;
        model_warnings += 1;
        println!("WARN no .mox files — JSON schemas are the primary model source");
        println!(
            "     hint: consider migrating: codegraph migrate --schemas {} --output <dir>",
            schemas_dir.unwrap().display()
        );
    }

    let mut manifest_candidates = vec![PathBuf::from("codegraph-ops.toml")];
    if let Some(parent) = args.config.parent().filter(|p| !p.as_os_str().is_empty()) {
        manifest_candidates.push(parent.join("codegraph-ops.toml"));
    }
    if let Some(parent) = args.schemas.as_ref().and_then(|s| s.parent()) {
        if !parent.as_os_str().is_empty() {
            manifest_candidates.push(parent.join("codegraph-ops.toml"));
        }
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

    match std::env::var("APP_DATABASE_URL") {
        Ok(_) => println!("PASS APP_DATABASE_URL — app_user pool enabled"),
        Err(_) => {
            soft_warnings += 1;
            println!("WARN APP_DATABASE_URL not set — server runs in legacy mode");
            println!(
                "     hint: set APP_DATABASE_URL to postgres://app_user:<pass>@host/db so request context rides the statement payload (#169)"
            );
        }
    }

    println!();
    if hard_failures > 0 {
        Err(Error::Config(format!(
            "doctor: {hard_failures} hard check(s) failed, {soft_warnings} warning(s)"
        )))
    } else {
        println!("doctor: all hard checks passed ({soft_warnings} warning(s))");
        Ok(DoctorSummary {
            hard_failures,
            soft_warnings,
            model_warnings,
        })
    }
}

/// Append a `[domains.<name>]` entry to `config_path` and create
/// `model/<name>.mox` with the shared starter model (compile-verified
/// before write). Refuses duplicate domains. Mox-first: no `schemas/`
/// directory is created.
pub fn cmd_add_domain(config_path: &Path, domain_name: &str) -> Result<()> {
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

    // Starter model: shared .mox template, compile-verified before write
    // (init precedent). The hint names the codegraph binary — the consumer
    // wrapper's name is not known here, and `codegraph run --mox-files …`
    // works in any project.
    let model_content = super::model_starter::starter_model_mox(&name, &label, "codegraph")
        .map_err(Error::Config)?;
    let mox_rel = format!("model/{name}.mox");
    let compilation = rex_driver::compile_files(&[(mox_rel.clone(), model_content.clone())]);
    if compilation.model.is_none() {
        return Err(Error::Config(format!(
            "starter model '{mox_rel}' does not compile — refusing to write"
        )));
    }

    let model_dir = match config_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join("model"),
        _ => PathBuf::from("model"),
    };
    fs::create_dir_all(&model_dir)?;
    let model_path = model_dir.join(format!("{name}.mox"));
    if !model_path.exists() {
        fs::write(&model_path, model_content)?;
    }

    println!("Added domain '{name}' (label {label}, schema_dir {name}, postgres_schema {name})");
    println!("Updated {}", config_path.display());
    println!("Created {}", model_path.display());
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

        cmd_add_domain(&config, "Billing").unwrap();

        let parsed = codegraph_config::config::parse_domain_config(&config).unwrap();
        assert!(parsed.domains.contains_key("billing"));
        assert_eq!(parsed.domains["billing"].label, "Billing");
        assert_eq!(parsed.domains["billing"].schema_dir, "billing");
        assert_eq!(parsed.domains["billing"].postgres_schema, "billing");

        let model = dir.path().join("model/billing.mox");
        assert!(model.is_file(), "add domain must create model/billing.mox");
        let content = fs::read_to_string(&model).unwrap();
        let compilation = rex_driver::compile_files(&[("model/billing.mox".to_string(), content)]);
        assert!(
            compilation.model.is_some(),
            "starter model must compile: {:?}",
            compilation.diagnostics
        );
        assert_eq!(compilation.model.unwrap().packages[0].name, "billing");

        assert!(!dir.path().join("schemas").exists());

        let err = cmd_add_domain(&config, "billing").unwrap_err();
        assert!(format!("{err}").contains("already exists"), "{err}");
    }
}
