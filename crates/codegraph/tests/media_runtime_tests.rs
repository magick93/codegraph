//! Issue #695 Stage 1 — the media plane's behavioural gate against a REAL
//! S3-compatible server (RustFS).
//!
//! The media plane (#625) shipped adapters, routes, RLS and compile pins, but
//! nothing ever drove the data path: every gate stopped at "the module
//! compiles" or "the emitted source contains X". `media_compile_tests` proves
//! the code builds; this gate proves it WORKS — bytes into a bucket, a child
//! row with the session tenant, a signed display URL that returns the bytes,
//! overwrite in place, remove, and the cross-tenant denials #690's acceptance
//! criteria rest on.
//!
//! Shape: the `authz_contract` / `child_tenant_contract_tests` pattern —
//! generate with the CURRENT generators (never a committed fixture tree),
//! migrate a per-run database, boot the emitted axum server, drive it over
//! HTTP, and prove outcomes from BOTH sides (Postgres rows + bucket objects).
//!
//! **Why `provider = "r2"` and not `"s3"`:** the R2 adapter IS the S3
//! adapter (path-style SigV4 over `MediaHttp`); Stage 1 validates it against a
//! generic store before the `s3` provider variant exists. Two env details are
//! load-bearing and each was a trap:
//!
//! - `R2_S3_ENDPOINT` points the adapter at RustFS (no `R2_ACCOUNT_ID`),
//! - `R2_S3_REGION=us-east-1` — RustFS VERIFIES the SigV4 credential scope,
//!   so R2's default `auto` fails signature validation on every request and
//!   the failure reads as "the store is broken" (Stage 0, issue #695).
//!
//! **RustFS** runs as a throwaway container: an anonymous `/data` volume (the
//! image declares it, so no host bind mount — whose ownership must be
//! `10001:10001` — is needed), S3 API only (`:9000`; the console is
//! opt-in), default credentials `rustfsadmin`/`rustfsadmin`, region
//! `us-east-1`. The bucket is created by a signed `PUT /{bucket}` (the app
//! has no bucket-create path by design — #625 OQ2), and that same signed
//! request IS the preflight: it proves reachability, credentials AND the
//! region before a single byte of app code runs.
//!
//! Needs: `docker` (with the pinned `rustfs/rustfs` image pulled — the nightly workflow
//! pulls it), `curl` ≥ 8.0 (`--aws-sigv4`), `psql`, and the **Supabase
//! postgres image** (`public.ecr.aws/supabase/postgres:17.6.1.132`) at
//! `DATABASE_URL` or via the gate container fallback (#640: a plain
//! `postgres:<n>` service is not a supported target — the generated stack
//! needs the `auth` schema).
//!
//! ```text
//! DATABASE_URL=postgres://postgres:postgres@127.0.0.1:54350/postgres \
//!   cargo test -p codegraph --test media_runtime_tests -- --ignored --nocapture
//! ```
//!
//! The build target is deliberately ISOLATED (`/tmp/opencode/ws695-target`):
//! the generated app pulls its own dependency set (axum + sea-orm + reqwest),
//! and sharing the host workspace's target dir would contend on its lock and
//! bloat it by gigabytes (`test_framework::axum_server::build_app`'s doc).

#[path = "test_framework/mod.rs"]
mod test_framework;

// The #694 source reset (shared with the other persistent-dir gates), plus its
// node-free pins.
#[path = "gate_reset.rs"]
mod gate_reset;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use codegraph_ops::db::{psql_exec, psql_query};
use codegraph_ops::pg::PgTarget;
use serde_json::Value;

use test_framework::axum_server;
use test_framework::postgres::{GateDb, resolve_base_target};
use test_framework::process;

// ── Fixture identities ──────────────────────────────────────────────────

const ORG_A: &str = "11111111-1111-4111-8111-111111111111";
const ORG_B: &str = "22222222-2222-4222-8222-222222222222";
/// Org A's record — the object under test.
const PARENT_A: &str = "aaaaaaaa-0000-4000-8000-000000000001";
/// Org B's record.
const PARENT_B: &str = "bbbbbbbb-0000-4000-8000-000000000001";
/// A second org B record, used only for the poisoned-reference leg (it must
/// never share a parent with the ordinary B upload: `display` picks the newest
/// row per parent and `remove` deletes them all).
const PARENT_B2: &str = "bbbbbbbb-0000-4000-8000-000000000002";
/// A well-formed id no row carries — the "foreign parent" legs.
const MISSING_PARENT: &str = "cccccccc-0000-4000-8000-0000000000ff";

/// The domain / table / field the fixture generates (mox `package assets`,
/// `class Asset`, `contains MediaReference[0..1] photo`).
const PARENT_TABLE: &str = "assets.asset";
const CHILD_TABLE: &str = "assets.asset_photo";
const ENTITY_PATH: &str = "/api/v1/assets/asset";
/// The generated route module (the #707 guard's home).
const ROUTE_MODULE: &str = "src/api/assets/media_asset_photo.rs";

// ── Issue #722: the nested (child) media fixture ────────────────────────
//
// A root `Person` and a CHILD `Contact` (`refers Person`, media `avatar`).
// `router.tera` mounts the child's media routes as
// `/api/v1/assets/persons/{person_id}/contacts/{contact_id}/avatar`; the
// handlers must extract the ancestor tuple and prove the row belongs to the
// URL parent — a flat `Path<Uuid>` answered 400 and an extractor without the
// ownership predicate was an IDOR.

/// `assets.person` / `assets.contact` / the child media table.
const PERSON_TABLE: &str = "assets.person";
const CONTACT_TABLE: &str = "assets.contact";
const CONTACT_CHILD_TABLE: &str = "assets.contact_avatar";
/// The generated child media route module.
const NESTED_ROUTE_MODULE: &str = "src/api/assets/media_contact_avatar.rs";
/// Org A's root record + a second root (the link-mismatch probe).
const PERSON_A1: &str = "aaaaaaaa-0000-4000-8000-000000000011";
const PERSON_A2: &str = "aaaaaaaa-0000-4000-8000-000000000012";
/// Org A's contact, linked to [`PERSON_A1`].
const CONTACT_A1: &str = "aaaaaaaa-0000-4000-8000-000000000021";
/// Org B's root record + contact (the own-tenant leg).
const PERSON_B: &str = "bbbbbbbb-0000-4000-8000-000000000011";
const CONTACT_B: &str = "bbbbbbbb-0000-4000-8000-000000000021";

/// RustFS: one container, S3 API on 9000, default credentials, `us-east-1`.
/// Pinned by DIGEST, not a mutable tag: a nightly gate must exercise the
/// store that was actually validated (the digest below is the image this
/// lane was written and run against).
const RUSTFS_IMAGE: &str =
    "rustfs/rustfs@sha256:ee717072290bec9b11abfc0365f8310b00df1cd9b3639943edae47bdef05044a";
const RUSTFS_ACCESS_KEY: &str = "rustfsadmin";
const RUSTFS_SECRET_KEY: &str = "rustfsadmin";
const RUSTFS_REGION: &str = "us-east-1";
const BUCKET: &str = "media-runtime";

/// Wildcard scopes: this gate is about the MEDIA plane's tenancy, not about
/// the API-key scope plane (`authz_contract` owns that). Every key may act on
/// everything; the org on the key is what must fence the data.
const WILDCARD_SCOPES: &str = r#"[{"entity_type":"*","entity_id":"*","action":"*"}]"#;

// ── Gate workspace ──────────────────────────────────────────────────────

/// Logs + the generated fixture tree. Kept OFF the repo (a panicking run must
/// not leave a half-generated tree inside the checkout).
fn gate_root() -> PathBuf {
    PathBuf::from("/tmp/opencode/ws695-media-runtime")
}

fn logs_dir() -> PathBuf {
    gate_root().join("logs")
}

/// The generated app's `CARGO_TARGET_DIR`. ISOLATED by design (see the module
/// docs): `/tmp/opencode/ws695-target`, never the host workspace's `target/`.
fn app_target_dir() -> PathBuf {
    PathBuf::from("/tmp/opencode/ws695-target")
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/media_runtime")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("codegraph crate lives at <repo>/crates/codegraph")
        .to_path_buf()
}

/// The fixture's `profiles.toml`: the workspace `default` profile with the
/// meta the gate needs and `dependency_strategy = "path"`.
///
/// - `path` deps keep the gate network-free: a `rev` pin would fetch
///   `magick93/codegraph` at this checkout's rev, which does not exist (the
///   branch is unpushed) — the `service_ops_compile_gate` finding.
/// - `type_contracts_base` is the LOCAL checkout for the same reason: the
///   emitted domain-types crate pins `codegraph-type-contracts` by path.
/// - the gRPC generators are stripped (protoc is not a gate prerequisite).
fn write_profiles(dir: &Path) -> PathBuf {
    let workspace_root = repo_root();
    let type_contracts = workspace_root.join("crates/codegraph-type-contracts");
    let workspace_profiles = workspace_root.join("profiles.toml");
    let mut text = fs::read_to_string(&workspace_profiles).expect("workspace profiles.toml");
    text = text.replace(
        "[profiles.default.meta]\n",
        &format!(
            "[profiles.default.meta]\n\
             app_name = \"media-runtime-app\"\n\
             domain_types_base = \"crates/domain-types\"\n\
             type_contracts_base = \"{}\"\n",
            type_contracts.display()
        ),
    );
    text = text.replace(
        "[profiles.default.features]\n",
        "[profiles.default.features]\ndependency_strategy = \"path\"\n",
    );
    let filtered: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("\"grpc_"))
        .collect();
    fs::create_dir_all(dir).expect("profiles dir");
    let path = dir.join("profiles.toml");
    fs::write(&path, filtered.join("\n")).expect("write profiles.toml");
    path
}

/// Generate the fixture with the CURRENT generators into `output`. The mox
/// model is the primary source; no JSON schemas, no classifier.
async fn generate(profiles: &Path, output: &Path) -> PathBuf {
    let fixture = fixture_dir();
    let mox = fixture.join("media.mox");
    codegraph::driver::run(codegraph::driver::RunArgs {
        schemas: None,
        classifier: None,
        config_path: &fixture.join("domains.toml"),
        output,
        extension_points_path: None,
        integration_manifests: &[],
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles.to_path_buf()),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: std::slice::from_ref(&mox),
        rosetta_files: &[],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    })
    .await
    .expect("the media fixture must generate");
    output.to_path_buf()
}

// ── RustFS (the S3-compatible store) ────────────────────────────────────

fn docker(args: &[&str]) -> Result<String, String> {
    let out = Command::new("docker")
        .args(args)
        .output()
        .map_err(|e| format!("spawn docker: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "docker {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// A throwaway RustFS container. Dropped (and force-removed WITH its
/// anonymous volumes) on scope exit, so a panicking leg leaves nothing behind.
struct RustFs {
    name: String,
    port: u16,
}

impl RustFs {
    async fn start() -> Result<Self, String> {
        if !process::have_tool("docker") {
            return Err("docker is required to boot RustFS".to_string());
        }
        if let Err(e) = docker(&["image", "inspect", RUSTFS_IMAGE]) {
            return Err(format!(
                "the RustFS image must be present (`docker pull {RUSTFS_IMAGE}`): {e}"
            ));
        }
        let port = process::free_port();
        let name = format!("ws695-rustfs-{port}");
        // Anonymous volumes only: the image declares /data, so a host bind
        // mount (which must be owned 10001:10001) is unnecessary.
        docker(&[
            "run",
            "-d",
            "--name",
            &name,
            "-p",
            &format!("127.0.0.1:{port}:9000"),
            "-e",
            &format!("RUSTFS_ACCESS_KEY={RUSTFS_ACCESS_KEY}"),
            "-e",
            &format!("RUSTFS_SECRET_KEY={RUSTFS_SECRET_KEY}"),
            "-e",
            &format!("RUSTFS_REGION={RUSTFS_REGION}"),
            RUSTFS_IMAGE,
        ])?;
        let store = Self { name, port };
        println!(
            "gate: RustFS container `{}` booting on 127.0.0.1:{} (image {RUSTFS_IMAGE}, anonymous volumes)",
            store.name, store.port
        );
        store.await_ready()?;
        Ok(store)
    }

    fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// One SigV4-signed S3 request — the same credential scope the generated
    /// adapter signs with (`us-east-1`), so a green preflight here means the
    /// app's requests will validate too.
    fn s3(&self, method: &str, path: &str, extra: &[String]) -> Result<(u16, Vec<u8>), String> {
        let dir = logs_dir().join("s3");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let tag = path.replace(['/', '?', '&', '='], "_");
        let body = dir.join(format!("{}-{tag}.body", method.to_lowercase()));
        let out = Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "30",
                "-o",
                &body.to_string_lossy(),
                "-w",
                "%{http_code}",
                "-X",
                method,
                "--aws-sigv4",
                "aws:amz:us-east-1:s3",
                "--user",
                &format!("{RUSTFS_ACCESS_KEY}:{RUSTFS_SECRET_KEY}"),
            ])
            .args(extra)
            .arg(format!("{}{path}", self.endpoint()))
            .output()
            .map_err(|e| format!("spawn curl: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "curl {method} {path} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let status: u16 = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .map_err(|e| format!("curl status: {e}"))?;
        let bytes = fs::read(&body).unwrap_or_default();
        Ok((status, bytes))
    }

    /// Readiness AND credential/region validation in one: a signed
    /// `ListBuckets` only answers 200 when RustFS accepts the SigV4 scope.
    fn await_ready(&self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut last = String::new();
        while Instant::now() < deadline {
            match self.s3("GET", "/", &[]) {
                Ok((200, _)) => return Ok(()),
                Ok((status, body)) => {
                    last = format!(
                        "{status}: {}",
                        String::from_utf8_lossy(&body)
                            .lines()
                            .next()
                            .unwrap_or_default()
                    )
                }
                Err(e) => last = e,
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Err(format!(
            "RustFS did not accept a signed request within 60s (last: {last}) — a 403 here means \
             the credentials or the `{RUSTFS_REGION}` SigV4 region are wrong, not that the store is down"
        ))
    }

    /// The app has NO bucket-create path by design (#625 OQ2): the platform
    /// owns buckets. This signed PUT is the gate's platform side.
    fn create_bucket(&self) -> Result<(), String> {
        let (status, body) = self.s3("PUT", &format!("/{BUCKET}"), &[])?;
        if status != 200 {
            return Err(format!(
                "bucket create answered {status}: {}",
                String::from_utf8_lossy(&body)
            ));
        }
        Ok(())
    }

    fn get_object(&self, key: &str) -> Result<(u16, Vec<u8>), String> {
        self.s3("GET", &format!("/{BUCKET}/{key}"), &[])
    }

    fn stop(&self) {
        // `-v` removes the container's anonymous volumes (the data volume),
        // so nothing survives the run.
        let _ = docker(&["rm", "-f", "-v", &self.name]);
    }
}

impl Drop for RustFs {
    fn drop(&mut self) {
        self.stop();
    }
}

// ── Per-run database with a panic-safe drop ─────────────────────────────

struct ScratchDb {
    db: Option<GateDb>,
    name: String,
    base_url: String,
}

impl ScratchDb {
    async fn create() -> Result<Self, String> {
        let base = resolve_base_target().await.map_err(|e| {
            format!("a Postgres base target (DATABASE_URL or the gate container): {e}")
        })?;
        // Product decision (#640): the supported target is the Supabase image
        // — the generated stack needs the `auth` schema. Fail LOUD, never skip:
        // a plain `postgres:<n>` service would report a media-plane failure
        // that is really the fixture environment.
        let supabase_image = psql_query(
            &base,
            "SELECT count(*) FROM pg_namespace WHERE nspname = 'auth';",
        )
        .await
        .map(|v| v.trim() != "0")
        .unwrap_or(false);
        if !supabase_image {
            return Err(format!(
                "this gate needs the Supabase postgres image (the `auth` schema and the pgmq \
                 functions the generated migrations install): the target at {}:{} has no `auth` \
                 schema. Plain `postgres:<n>` is not a supported target (#640).",
                base.host, base.port
            ));
        }
        let base_url = base.url();
        let db = GateDb::create(base).await?;
        let name = db.url().rsplit('/').next().unwrap_or_default().to_string();
        Ok(Self {
            db: Some(db),
            name,
            base_url,
        })
    }

    fn db(&self) -> &GateDb {
        self.db.as_ref().expect("db present until cleanup")
    }

    async fn cleanup(mut self) {
        if let Some(db) = self.db.take() {
            db.cleanup().await;
        }
    }
}

impl Drop for ScratchDb {
    fn drop(&mut self) {
        if let Some(_db) = self.db.take() {
            // Panic path: no async context left. Best-effort force-drop via
            // psql so a failing run does not accumulate `ifml_gate_*` DBs.
            let _ = Command::new("psql")
                .arg(&self.base_url)
                .arg("-c")
                .arg(format!(
                    "DROP DATABASE IF EXISTS {} WITH (FORCE);",
                    self.name
                ))
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

// ── HTTP against the booted app ─────────────────────────────────────────

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    location: Option<String>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("expected a JSON body, got {e}: {}", self.text()))
    }
}

/// `curl` against the app, capturing status + headers + body. Every request
/// goes through here so a failure prints the same diagnostic shape.
fn http(url: &str, args: &[String], tag: &str) -> Result<HttpResponse, String> {
    let dir = logs_dir().join("http");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let body_path = dir.join(format!("{tag}.body"));
    let head_path = dir.join(format!("{tag}.head"));
    let out = Command::new("curl")
        .args(["-sS", "--max-time", "60", "-D"])
        .arg(&head_path)
        .arg("-o")
        .arg(&body_path)
        .args(["-w", "%{http_code}"])
        .args(args)
        .arg(url)
        .output()
        .map_err(|e| format!("spawn curl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "curl {tag} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let status: u16 = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .map_err(|e| format!("curl status for {tag}: {e}"))?;
    let headers = fs::read_to_string(&head_path).unwrap_or_default();
    let location = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("location")
            .then(|| value.trim().to_string())
    });
    Ok(HttpResponse {
        status,
        location,
        body: fs::read(&body_path).unwrap_or_default(),
    })
}

fn bearer(key: &str) -> String {
    format!("Authorization: Bearer {key}")
}

/// Multipart `PUT …/{id}/photo` (through-app upload).
fn upload(base: &str, id: &str, key: &str, bytes: &Path, mime: &str, tag: &str) -> HttpResponse {
    http(
        &format!("{base}{ENTITY_PATH}/{id}/photo"),
        &[
            "-X".to_string(),
            "PUT".to_string(),
            "-H".to_string(),
            bearer(key),
            "-F".to_string(),
            format!("file=@{};type={mime}", bytes.display()),
        ],
        tag,
    )
    .expect("upload request must run")
}

fn display(base: &str, id: &str, key: &str, tag: &str) -> HttpResponse {
    http(
        &format!("{base}{ENTITY_PATH}/{id}/photo"),
        &["-H".to_string(), bearer(key)],
        tag,
    )
    .expect("display request must run")
}

fn remove(base: &str, id: &str, key: &str, tag: &str) -> HttpResponse {
    http(
        &format!("{base}{ENTITY_PATH}/{id}/photo"),
        &[
            "-X".to_string(),
            "DELETE".to_string(),
            "-H".to_string(),
            bearer(key),
        ],
        tag,
    )
    .expect("remove request must run")
}

fn write_bytes(name: &str, bytes: &[u8]) -> PathBuf {
    let path = logs_dir().join("payloads").join(name);
    fs::create_dir_all(path.parent().expect("payload dir")).expect("payload dir");
    fs::write(&path, bytes).expect("payload write");
    path
}

// ── Issue #722: nested route helpers ────────────────────────────────────

/// The router's nested child media path (`.nest("/{person_id}/contacts", …)`).
fn nested_url(base: &str, person: &str, contact: &str) -> String {
    format!("{base}/api/v1/assets/persons/{person}/contacts/{contact}/avatar")
}

fn nested_upload(
    base: &str,
    person: &str,
    contact: &str,
    key: &str,
    bytes: &Path,
    mime: &str,
    tag: &str,
) -> HttpResponse {
    http(
        &nested_url(base, person, contact),
        &[
            "-X".to_string(),
            "PUT".to_string(),
            "-H".to_string(),
            bearer(key),
            "-F".to_string(),
            format!("file=@{};type={mime}", bytes.display()),
        ],
        tag,
    )
    .expect("nested upload request must run")
}

fn nested_display(base: &str, person: &str, contact: &str, key: &str, tag: &str) -> HttpResponse {
    http(
        &nested_url(base, person, contact),
        &["-H".to_string(), bearer(key)],
        tag,
    )
    .expect("nested display request must run")
}

fn nested_remove(base: &str, person: &str, contact: &str, key: &str, tag: &str) -> HttpResponse {
    http(
        &nested_url(base, person, contact),
        &[
            "-X".to_string(),
            "DELETE".to_string(),
            "-H".to_string(),
            bearer(key),
        ],
        tag,
    )
    .expect("nested remove request must run")
}

/// `count|url|mime|tenant` for one contact's media rows.
async fn contact_media_rows(target: &PgTarget, contact: &str) -> String {
    psql_query(
        target,
        &format!(
            "SELECT count(*)::text || '|' || coalesce(max(url), '') || '|' || \
             coalesce(max(mime_type), '') || '|' || \
             coalesce(max(platform_organization_id::text), '') \
             FROM {CONTACT_CHILD_TABLE} WHERE contact_id = '{contact}';"
        ),
    )
    .await
    .expect("contact media row probe")
    .trim()
    .to_string()
}

// ── Postgres probes ─────────────────────────────────────────────────────

/// `count|url|mime|tenant` for one parent's media rows — the psql side of
/// every leg. One line, `|`-joined, so an assertion reads as the whole fact.
async fn media_rows(target: &PgTarget, parent: &str) -> String {
    psql_query(
        target,
        &format!(
            "SELECT count(*)::text || '|' || coalesce(max(url), '') || '|' || \
             coalesce(max(mime_type), '') || '|' || \
             coalesce(max(platform_organization_id::text), '') \
             FROM {CHILD_TABLE} WHERE asset_id = '{parent}';"
        ),
    )
    .await
    .expect("media row probe")
    .trim()
    .to_string()
}

fn assert_rows(actual: &str, expected: &str, context: &str) {
    assert_eq!(
        actual, expected,
        "{context}: expected `{expected}` (count|url|mime|tenant), got `{actual}`"
    );
}

/// Insert one parent row as the OWNER connection (BYPASSRLS), with an explicit
/// tenant. The media routes only ever address EXISTING parents; the API's own
/// create path is covered elsewhere.
async fn insert_parent(target: &PgTarget, id: &str, org: &str, name: &str) {
    psql_exec(
        target,
        &format!(
            "INSERT INTO {PARENT_TABLE} (id, platform_organization_id, name) \
             VALUES ('{id}', '{org}', '{name}');"
        ),
    )
    .await
    .expect("insert parent row");
}

/// A root `Person` row (issue #722's nested anchor).
async fn insert_person(target: &PgTarget, id: &str, org: &str, name: &str) {
    psql_exec(
        target,
        &format!(
            "INSERT INTO {PERSON_TABLE} (id, platform_organization_id, name) \
             VALUES ('{id}', '{org}', '{name}');"
        ),
    )
    .await
    .expect("insert person row");
}

/// A `Contact` row owned by `person` — the nested media route's addressed
/// record. The FK is what the #722 ownership gate verifies.
async fn insert_contact(target: &PgTarget, id: &str, org: &str, person: &str, name: &str) {
    psql_exec(
        target,
        &format!(
            "INSERT INTO {CONTACT_TABLE} (id, platform_organization_id, person_id, name) \
             VALUES ('{id}', '{org}', '{person}', '{name}');"
        ),
    )
    .await
    .expect("insert contact row");
}

/// Another organization's `sk_…` key (`public.create_api_key` is the
/// `authz_contract` / IFML-gate provisioning path).
async fn create_api_key(target: &PgTarget, org: &str, name: &str) -> String {
    let out = psql_query(
        target,
        &format!(
            "SELECT public.create_api_key('{org}'::uuid, '{name}', '{WILDCARD_SCOPES}'::jsonb)::text;"
        ),
    )
    .await
    .expect("create_api_key");
    let value: Value = serde_json::from_str(out.trim())
        .unwrap_or_else(|e| panic!("create_api_key returned non-JSON ({e}): {out}"));
    value
        .get("key")
        .and_then(|k| k.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| panic!("create_api_key returned no key: {out}"))
}

/// An API key whose scopes do NOT cover the media entity: the generated
/// `scope_enforced_*` RESTRICTIVE policies must deny it at the parent gate
/// (P0403 → 403). This is the leg that fails if the media handlers stop
/// riding the #169 session bundle (#690's own defect class).
async fn create_scoped_api_key(target: &PgTarget, org: &str, name: &str, scopes: &str) -> String {
    let out = psql_query(
        target,
        &format!("SELECT public.create_api_key('{org}'::uuid, '{name}', '{scopes}'::jsonb)::text;"),
    )
    .await
    .expect("create_api_key (scoped)");
    let value: Value = serde_json::from_str(out.trim())
        .unwrap_or_else(|e| panic!("create_api_key returned non-JSON ({e}): {out}"));
    value
        .get("key")
        .and_then(|k| k.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| panic!("create_api_key returned no key: {out}"))
}

// ── The gate ────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "boots RustFS (docker) + a generated axum app; needs the Supabase postgres image"]
async fn media_plane_end_to_end_over_rustfs() {
    fs::create_dir_all(logs_dir()).expect("gate logs dir");
    println!("gate: logs at {}", logs_dir().display());

    // ── 0. The store, its bucket, and the preflight ─────────────────────
    //
    // A 30-second check that localises "is RustFS reachable and are the
    // credentials/region right" away from "is the generated app right".
    let rustfs = RustFs::start()
        .await
        .unwrap_or_else(|e| panic!("RustFS: {e}"));
    rustfs
        .create_bucket()
        .unwrap_or_else(|e| panic!("RustFS bucket create: {e}"));
    println!(
        "gate: RustFS ready on {} — signed ListBuckets + bucket `{BUCKET}` created \
         (creds {RUSTFS_ACCESS_KEY}/…, region {RUSTFS_REGION})",
        rustfs.endpoint()
    );

    // ── 1. Generate + migrate ───────────────────────────────────────────
    let profiles = write_profiles(&gate_root().join("fixture"));
    let app_dir = gate_root().join("app");
    // Issue #694: the gate root persists (logs, the fixture), and the app's
    // cargo cache is a sibling (`/tmp/opencode/ws695-target`) — but the
    // generated SOURCE must be regenerated from clean, or a file an older
    // generation emitted is asserted/typechecked as this run's.
    gate_reset::reset_generated_source(&app_dir).expect("reset the generated app dir");
    generate(&profiles, &app_dir).await;
    let route_module =
        fs::read_to_string(app_dir.join(ROUTE_MODULE)).expect("the media route module must emit");
    assert!(
        route_module.contains("pub async fn upload"),
        "the fixture must emit the through-app upload handler"
    );
    // The #707 object-key guard belongs to the emitted handler (#717, merged
    // before this gate landed). Assert it is there rather than detecting it:
    // a gate that silently degrades when the guard disappears is a gate that
    // cannot fail, which is the class #708 exists to kill.
    assert!(
        route_module.contains("const KEY_PREFIX_FORMAT"),
        "the emitted media route must carry the #707 object-key guard — without it \n         display/remove sign and delete ANY stored key"
    );

    // Issue #722: the CHILD media module must extract the ancestor tuple and
    // gate ownership. Asserted before boot so a template regression fails
    // with the root cause, not as a 400/IDOR in a leg.
    let nested_route_module = fs::read_to_string(app_dir.join(NESTED_ROUTE_MODULE))
        .expect("the child media route module must emit");
    assert!(
        nested_route_module.contains("Path((parent_id, id)): Path<(Uuid, Uuid)>,"),
        "the child media handler must extract the ancestor tuple (a flat Path<Uuid> is the \
         #722 400):\n{nested_route_module}"
    );
    assert!(
        nested_route_module.contains("crate::media::session::parent_exists_linked("),
        "the child media handler must gate ownership through the link column:\n{nested_route_module}"
    );
    assert!(
        nested_route_module.contains(r#"const PARENT_LINK_COLUMN: &str = "person_id";"#),
        "the configured parent_ref is the ownership predicate:\n{nested_route_module}"
    );

    let db = ScratchDb::create()
        .await
        .unwrap_or_else(|e| panic!("database: {e}"));
    let target = PgTarget::from_url(&db.db().url()).expect("parse the gate db url");
    // NO `apply_prelude()`: the image treats `supabase_realtime_admin` (and
    // its siblings) as RESERVED roles that only a superuser may create, and
    // this gate runs as `postgres` (`NOSUPERUSER`) — so the prelude is skipped
    // deliberately. The generated `0003_pgmq_setup.sql` installs pgmq +
    // `emit_domain_event` itself — the tenant-table event triggers the
    // migration band creates depend on it, so a missing extension fails the
    // parent INSERT below.
    db.db()
        .apply_migrations(&app_dir.join("migrations"))
        .await
        .expect("the generated migration set must run");

    // A and B each own a record; B2 is the poisoned-reference probe's parent.
    insert_parent(&target, PARENT_A, ORG_A, "A").await;
    insert_parent(&target, PARENT_B, ORG_B, "B").await;
    insert_parent(&target, PARENT_B2, ORG_B, "B2").await;
    // Issue #722: the nested (child) media records — org A owns a person and
    // its contact (plus a SECOND person for the link-mismatch probe), org B
    // owns its own pair.
    insert_person(&target, PERSON_A1, ORG_A, "A1").await;
    insert_person(&target, PERSON_A2, ORG_A, "A2").await;
    insert_contact(&target, CONTACT_A1, ORG_A, PERSON_A1, "A1 contact").await;
    insert_person(&target, PERSON_B, ORG_B, "B").await;
    insert_contact(&target, CONTACT_B, ORG_B, PERSON_B, "B contact").await;
    let key_a = create_api_key(&target, ORG_A, "media-gate-a").await;
    let key_b = create_api_key(&target, ORG_B, "media-gate-b").await;
    println!(
        "gate: per-run db {} migrated; org A + org B keys provisioned",
        db.name
    );

    // ── 2. Build + boot the generated app ───────────────────────────────
    let bin = axum_server::build_app(
        &app_dir.join("Cargo.toml"),
        &repo_root(),
        &app_target_dir(),
        &logs_dir(),
        "media-runtime-app",
    )
    .expect("the generated media app must build");
    let database_url = db.db().url();
    let rustfs_endpoint = rustfs.endpoint();
    let server = axum_server::boot_server(
        &bin,
        &[
            ("DATABASE_URL", database_url.as_str()),
            ("SUPABASE_JWT_SECRET", "media-runtime-gate-secret"),
            ("APP_NAME", "media-runtime-app"),
            // The R2 adapter's env contract (templates/media/r2.tera). The
            // REGION is the Stage 0 fix: RustFS verifies the credential scope.
            ("R2_ACCESS_KEY_ID", RUSTFS_ACCESS_KEY),
            ("R2_SECRET_ACCESS_KEY", RUSTFS_SECRET_KEY),
            ("R2_S3_ENDPOINT", rustfs_endpoint.as_str()),
            ("R2_S3_REGION", RUSTFS_REGION),
        ],
        "media-runtime-app",
        &logs_dir(),
    )
    .await
    .unwrap_or_else(|e| panic!("boot the generated app: {e}"));
    let base = format!("http://127.0.0.1:{}", server.port);
    println!("gate: app booted on {base}");

    // #690's own defect class, as an app-visible tooth: a key whose scopes
    // exclude the entity must be denied AT THE PARENT GATE. If the handlers
    // ever stop riding the #169 session bundle, this leg is what notices.
    let key_scoped_out = create_scoped_api_key(
        &target,
        ORG_A,
        "scoped-out",
        r#"[{"entity_type": "not_asset", "entity_id": "*", "action": "*"}]"#,
    )
    .await;
    run_legs(&base, &target, &rustfs, &key_a, &key_b, &key_scoped_out).await;
    // Issue #722: the nested (child) media plane — the route the pre-#722
    // handler answered 400 on, and the ownership predicate that keeps it from
    // being an IDOR.
    run_nested_legs(&base, &target, &rustfs, &key_a, &key_b).await;
    // Issue #744: the end-to-end soft-deleted-parent leg (#699) — the
    // tier-local pins (plan flag, template SQL, compile) hold or fail
    // independently of the REAL handler behaviour; this leg proves the
    // running app serves nothing for a soft-deleted parent and leaves the
    // reference row + the object intact.
    run_soft_delete_leg(&base, &target, &rustfs, &key_a).await;

    // ── Issue #744: the soft-deleted parent serves no media (#699) ──────
    // #699's pins are tier-local (the plan flag cross-checked against the
    // DDL, the gate SQL bodies, the compile); this leg runs the REAL
    // handler over a REAL soft-delete: upload → soft-delete the parent
    // row → display/remove both 404 (the `AND deleted_at IS NULL` gate),
    // and the reference row + the stored object both INTACT (a 404 must
    // not delete anything).
    async fn run_soft_delete_leg(base: &str, target: &PgTarget, rustfs: &RustFs, key_a: &str) {
        // Pre-state: the parent is ALIVE and serves its media (run_legs
        // left ONE row + object on PARENT_A — leg 4 removed, then the
        // cross-tenant preamble re-uploaded payload A).
        let alive = display(base, PARENT_A, key_a, "softdel-pre-display");
        assert_eq!(
            alive.status,
            307,
            "pre-state: the live parent must serve (got: {})",
            alive.text()
        );
        // Soft-delete the parent the way the auditable CRUD path would.
        psql_exec(
            target,
            &format!("UPDATE {PARENT_TABLE} SET deleted_at = NOW() WHERE id = '{PARENT_A}';"),
        )
        .await
        .expect("soft-delete the parent row");
        // Display and remove both 404 — the #699 gate (`parent_exists`
        // carries `AND deleted_at IS NULL` when the plan says the parent
        // soft-deletes) answers before any storage call.
        let d = display(base, PARENT_A, key_a, "softdel-display");
        assert_eq!(
            d.status,
            404,
            "a soft-deleted parent must serve no media (got: {})",
            d.text()
        );
        let r = remove(base, PARENT_A, key_a, "softdel-remove");
        assert_eq!(
            r.status,
            404,
            "remove must 404 too — never destructively delete past the gate (got: {})",
            r.text()
        );
        // Nothing was destroyed: the reference row survives (count 1) and
        // the object is still in the bucket (200).
        let rows = media_rows(target, PARENT_A).await;
        let url = format!("{ORG_A}/assets/asset/{PARENT_A}/photo");
        assert_rows(
            &rows,
            &format!("1|{url}|image/png|{ORG_A}"),
            "the soft delete must not touch the reference row",
        );
        let (status, bytes) = rustfs.get_object(&url).expect("bucket read");
        assert_eq!(
            status, 200,
            "the stored object must survive the soft-deleted parent"
        );
        // The object is run_legs' RE-UPLOAD (leg 4 removed, then the
        // cross-tenant preamble re-uploaded payload A) — assert it
        // survived the soft delete.
        let payload_a = b"\x89PNG\r\n\x1a\nmedia-runtime gate payload A".to_vec();
        assert_eq!(
            bytes, payload_a,
            "the object is run_legs' re-uploaded payload"
        );
        println!(
            "leg soft-delete: parent deleted → display/remove 404, row intact (`{rows}`), bucket 200 + {} bytes",
            bytes.len()
        );
    }

    // ── Cleanup (explicit; ScratchDb/RustFs also guard the panic path) ──
    drop(server);
    rustfs.stop();
    println!("gate: RustFS container + its anonymous /data volume removed");
    db.cleanup().await;
    println!("gate: per-run database dropped");
    // The build tree stays: deleting it would make every run pay the full
    // dependency build again (report its size instead).
    let size = Command::new("du")
        .args(["-sh", &app_target_dir().to_string_lossy()])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("gate: isolated app target dir {}", size);
}

/// The legs. Kept out of the test body so setup/teardown stay readable.
async fn run_legs(
    base: &str,
    target: &PgTarget,
    rustfs: &RustFs,
    key_a: &str,
    key_b: &str,
    key_scoped_out: &str,
) {
    // Payloads: distinct bytes so "the object is the NEW bytes" is provable.
    let payload_a = b"\x89PNG\r\n\x1a\nmedia-runtime gate payload A".to_vec();
    let payload_b = b"\x89PNG\r\n\x1a\nmedia-runtime gate payload B (overwrite)".to_vec();
    let file_a = write_bytes("a.png", &payload_a);
    let file_b = write_bytes("b.png", &payload_b);

    // ── LEG 1 — upload: child row lands AND the bytes land in the bucket ──
    let r = upload(base, PARENT_A, key_a, &file_a, "image/png", "upload-a");
    assert_eq!(r.status, 200, "org A upload must succeed: {}", r.text());
    let body = r.json();
    let key_a_obj = body
        .get("key")
        .and_then(|k| k.as_str())
        .expect("the upload response carries the object key")
        .to_string();
    assert_eq!(
        key_a_obj,
        format!("{ORG_A}/assets/asset/{PARENT_A}/photo"),
        "the key is the domain's key template rendered from the SESSION org"
    );

    let rows = media_rows(target, PARENT_A).await;
    assert_rows(
        &rows,
        &format!("1|{key_a_obj}|image/png|{ORG_A}"),
        "upload: exactly one reference row, url=key, mime, and the SESSION org",
    );
    let (status, bytes) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(status, 200, "the uploaded object must exist in the bucket");
    assert_eq!(bytes, payload_a, "the bucket holds the uploaded bytes");
    println!(
        "leg 1 upload: 200, row `{rows}`, bucket 200 + {} bytes",
        bytes.len()
    );

    // ── LEG 2 — display: 307 → the signed URL RETURNS the bytes ──────────
    let r = display(base, PARENT_A, key_a, "display-a");
    assert_eq!(r.status, 307, "display redirects (got: {})", r.text());
    let location = r.location.expect("the 307 must carry a Location header");
    assert!(
        location.starts_with(&rustfs.endpoint()),
        "the Location is the provider's signed URL (at the RustFS endpoint): {location}"
    );
    // Fetch it WITHOUT credentials: a presigned GET must stand alone.
    let signed = http(&location, &[], "display-a-signed").expect("signed GET must run");
    assert_eq!(
        signed.status,
        200,
        "the signed display URL must serve the object: {}",
        signed.text()
    );
    assert_eq!(signed.body, payload_a, "the signed URL returns the bytes");
    println!(
        "leg 2 display: 307 → signed GET 200 + {} bytes",
        signed.body.len()
    );

    // ── LEG 3 — overwrite in place: ONE row, the NEW bytes ──────────────
    let r = upload(
        base,
        PARENT_A,
        key_a,
        &file_b,
        "image/png",
        "upload-a-overwrite",
    );
    assert_eq!(r.status, 200, "overwrite upload: {}", r.text());
    let rows = media_rows(target, PARENT_A).await;
    assert_rows(
        &rows,
        &format!("1|{key_a_obj}|image/png|{ORG_A}"),
        "overwrite in place: still ONE row on the same key",
    );
    let (status, bytes) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(status, 200);
    assert_eq!(bytes, payload_b, "the object is the NEW bytes");
    println!(
        "leg 3 overwrite: one row, bucket body replaced ({} bytes)",
        bytes.len()
    );

    // ── LEG 4 — remove: row gone AND object gone ────────────────────────
    let r = remove(base, PARENT_A, key_a, "remove-a");
    assert_eq!(r.status, 204, "remove answers 204 (got: {})", r.text());
    let rows = media_rows(target, PARENT_A).await;
    assert_rows(&rows, "0|||", "remove: the reference row is gone");
    let (status, _) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(status, 404, "remove: the object is gone from the bucket");
    println!("leg 4 remove: 204, row gone, bucket 404");

    // A holds a row + object again — the cross-tenant legs need something to
    // attack.
    let r = upload(
        base,
        PARENT_A,
        key_a,
        &file_a,
        "image/png",
        "upload-a-again",
    );
    assert_eq!(
        r.status,
        200,
        "re-upload for the cross-tenant legs: {}",
        r.text()
    );
    let rows = media_rows(target, PARENT_A).await;
    assert_rows(
        &rows,
        &format!("1|{key_a_obj}|image/png|{ORG_A}"),
        "org A owns one reference row before the cross-tenant legs",
    );

    // ── LEG 5a — a FOREIGN PARENT ID answers 404 on every handler ───────
    //
    // The #690 parent gate: the owning row is resolved under the session
    // organization BEFORE any storage or child-row work, and a foreign id
    // must be indistinguishable from a missing one (404, never 403).
    for (tag, id) in [("missing", MISSING_PARENT), ("org-a", PARENT_A)] {
        let r = upload(
            base,
            id,
            key_b,
            &file_a,
            "image/png",
            &format!("cross-{tag}-upload"),
        );
        assert_eq!(
            r.status,
            404,
            "org B upload against parent {tag} must 404: {}",
            r.text()
        );
        let r = display(base, id, key_b, &format!("cross-{tag}-display"));
        assert_eq!(
            r.status,
            404,
            "org B display against parent {tag} must 404: {}",
            r.text()
        );
        let r = remove(base, id, key_b, &format!("cross-{tag}-remove"));
        assert_eq!(
            r.status,
            404,
            "org B remove against parent {tag} must 404: {}",
            r.text()
        );
    }
    // …and the denied attempts changed nothing: A's row and A's object are
    // exactly as they were.
    let rows = media_rows(target, PARENT_A).await;
    assert_rows(
        &rows,
        &format!("1|{key_a_obj}|image/png|{ORG_A}"),
        "org B's denied requests must not touch org A's reference row",
    );
    let (status, bytes) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(
        status, 200,
        "org A's object survives org B's denied requests"
    );
    assert_eq!(bytes, payload_a, "org A's bytes are unchanged");
    println!("leg 5a foreign parent: upload/display/remove all 404; A's row + object intact");

    // ── LEG 5b — org B cannot overwrite org A's row ─────────────────────
    //
    // B uploads against its OWN parent. The key is org-prefixed, the child
    // row is B's, and A's row/object must be untouched.
    let r = upload(
        base,
        PARENT_B,
        key_b,
        &file_b,
        "image/png",
        "cross-b-own-upload",
    );
    assert_eq!(
        r.status,
        200,
        "org B's own upload must succeed: {}",
        r.text()
    );
    let key_b_obj = format!("{ORG_B}/assets/asset/{PARENT_B}/photo");
    let rows_b = media_rows(target, PARENT_B).await;
    assert_rows(
        &rows_b,
        &format!("1|{key_b_obj}|image/png|{ORG_B}"),
        "org B's upload lands under its OWN org + key",
    );
    let rows_a = media_rows(target, PARENT_A).await;
    assert_rows(
        &rows_a,
        &format!("1|{key_a_obj}|image/png|{ORG_A}"),
        "org B's upload must not touch org A's row",
    );
    let (status, bytes) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(status, 200);
    assert_eq!(
        bytes, payload_a,
        "org B's upload leaves org A's bytes unchanged"
    );
    let (status, bytes_b) = rustfs.get_object(&key_b_obj).expect("bucket read");
    assert_eq!(status, 200, "org B's own object exists");
    assert_eq!(bytes_b, payload_b, "org B's object holds B's bytes");
    println!("leg 5b own-parent upload: B scoped to its own key; A untouched");

    // ── LEG 5c — org B's OWN row naming org A's object key ──────────────
    //
    // The row is inserted out-of-band (the owner connection) to simulate a
    // writer that named a foreign key — the exact shape the pre-#707
    // create/update DTOs allowed. `display` signs ANY key and `remove`
    // deletes ANY key with the SERVICE credential, so the handler must refuse
    // a key outside the session org's own rendering, and it must do so BEFORE
    // any storage call.
    psql_exec(
        target,
        &format!(
            "INSERT INTO {CHILD_TABLE} (asset_id, platform_organization_id, url, mime_type) \
             VALUES ('{PARENT_B2}', '{ORG_B}', '{key_a_obj}', 'image/png');"
        ),
    )
    .await
    .expect("insert the poisoned reference row");

    let r = display(base, PARENT_B2, key_b, "cross-poisoned-display");
    let r = (r.status, r.text());
    let d = remove(base, PARENT_B2, key_b, "cross-poisoned-remove");
    let d = (d.status, d.text());

    assert_eq!(
        r.0, 404,
        "display of a reference pointing outside the session org's key prefix must 404 \
         (never 403, never a signed foreign URL): {}",
        r.1
    );
    assert_eq!(
        d.0, 404,
        "remove of a reference pointing outside the session org's key prefix must 404: {}",
        d.1
    );
    // The guard runs BEFORE the storage/row work, so BOTH A's object and
    // B's (poisoned) row survive.
    let (status, bytes) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(
        status, 200,
        "org A's object survives the poisoned-reference attempts"
    );
    assert_eq!(bytes, payload_a, "org A's bytes are unchanged");
    let rows = media_rows(target, PARENT_B2).await;
    assert_rows(
        &rows,
        &format!("1|{key_a_obj}|image/png|{ORG_B}"),
        "the refused remove must not delete the child row",
    );
    // Clean up B's poisoned row so the remaining legs start from a known state.
    psql_exec(
        target,
        &format!("DELETE FROM {CHILD_TABLE} WHERE asset_id = '{PARENT_B2}';"),
    )
    .await
    .expect("clear the poisoned row");
    println!("leg 5c poisoned reference: display/remove 404; A's object + B's row intact");

    // ── LEG 6 — 413 / 415 against the corrected contract (#691) ────────
    //
    // `max_bytes = 1024` in the fixture: the router scopes the upload layer
    // to `max_bytes + 64 KiB` (issue #710), so a 2048-byte file fits under
    // the layer's slack and the HANDLER's own `max_bytes` check is the
    // boundary — the generated envelope 413 asserted below. A body beyond
    // the layer maps its lazy extractor 413 to the same envelope.
    let oversize = write_bytes("oversize.png", &vec![0u8; 2048]);
    let r = upload(base, PARENT_A, key_a, &oversize, "image/png", "oversize");
    assert_eq!(r.status, 413, "an oversize body answers 413: {}", r.text());
    assert_eq!(
        r.json()["error"]["code"],
        Value::String("PAYLOAD_TOO_LARGE".into()),
        "413 carries the generated error envelope"
    );

    let wrong_mime = write_bytes("wrong.bin", b"not an image");
    let r = upload(
        base,
        PARENT_A,
        key_a,
        &wrong_mime,
        "text/plain",
        "wrong-mime",
    );
    assert_eq!(r.status, 415, "a wrong MIME answers 415: {}", r.text());
    assert_eq!(
        r.json()["error"]["code"],
        Value::String("UNSUPPORTED_MEDIA_TYPE".into()),
        "415 carries the generated error envelope"
    );
    // The rejections are before any write: A's row and object are unchanged.
    let rows = media_rows(target, PARENT_A).await;
    assert_rows(
        &rows,
        &format!("1|{key_a_obj}|image/png|{ORG_A}"),
        "413/415 must not touch the stored reference",
    );
    let (status, bytes) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(status, 200);
    assert_eq!(bytes, payload_a, "413/415 must not rewrite the object");
    println!("leg 6 413/415: correct statuses + error envelopes; nothing written");

    // ── LEG 7 — the session bundle itself (#690's defect class) ─────────
    //
    // An API key scoped to a DIFFERENT entity must be refused at the parent
    // gate by the generated RESTRICTIVE `scope_enforced_*` policies (P0403 →
    // 403). This is the only leg that observes `begin_scoped`/`SET LOCAL ROLE
    // app_user`: every other leg is reproduced by the explicit `AND tenant =
    // $2` predicates alone, so a handler that stopped riding the bundle would
    // still pass them — exactly the hole #690 was opened for.
    let r = upload(
        base,
        PARENT_A,
        key_scoped_out,
        &file_a,
        "image/png",
        "scoped-upload",
    );
    assert_eq!(
        r.status,
        403,
        "an out-of-scope key must be refused by the parent gate (403), not served: {}",
        r.text()
    );
    let r = display(base, PARENT_A, key_scoped_out, "scoped-display");
    assert_eq!(r.status, 403, "display is scope-gated too: {}", r.text());
    let r = remove(base, PARENT_A, key_scoped_out, "scoped-remove");
    assert_eq!(r.status, 403, "remove is scope-gated too: {}", r.text());
    // All three were refused BEFORE any work: org A's row and object are
    // exactly as leg 1..6 left them.
    let rows = media_rows(target, PARENT_A).await;
    assert_rows(
        &rows,
        &format!("1|{key_a_obj}|image/png|{ORG_A}"),
        "the denied calls must not touch the stored reference",
    );
    let (status, bytes) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(status, 200, "org A's object survives the denied calls");
    assert_eq!(bytes, payload_a, "org A's bytes are unchanged");
    println!("leg 7 scope denial: upload/display/remove 403; row + object intact");
}

/// Issue #722 — the nested (child-entity) media data path.
///
/// The route is the router's own
/// `/api/v1/assets/persons/{person_id}/contacts/{contact_id}/avatar`. Before
/// #722 every one of these answered **400** (a flat extractor on a two-param
/// route); the fix extracts `(parent_id, id)` AND requires the contact row's
/// `person_id` to equal the URL parent — otherwise a caller could pair its own
/// person id with another org's contact id (an IDOR).
async fn run_nested_legs(base: &str, target: &PgTarget, rustfs: &RustFs, key_a: &str, key_b: &str) {
    let payload_a = b"\x89PNG\r\n\x1a\nnested media payload A".to_vec();
    let payload_b = b"\x89PNG\r\n\x1a\nnested media payload B".to_vec();
    let file_a = write_bytes("nested-a.png", &payload_a);
    let file_b = write_bytes("nested-b.png", &payload_b);

    // ── LEG N1 — upload under the nested route ──────────────────────────
    let r = nested_upload(
        base,
        PERSON_A1,
        CONTACT_A1,
        key_a,
        &file_a,
        "image/png",
        "nested-upload-a",
    );
    assert_eq!(
        r.status,
        200,
        "a nested upload must succeed (the pre-#722 handler answered 400): {}",
        r.text()
    );
    let key_a_obj = r
        .json()
        .get("key")
        .and_then(|k| k.as_str())
        .expect("the upload response carries the object key")
        .to_string();
    assert_eq!(
        key_a_obj,
        format!("{ORG_A}/assets/contact/{CONTACT_A1}/avatar"),
        "the key renders the CHILD entity + its id under the session org"
    );
    let rows = contact_media_rows(target, CONTACT_A1).await;
    assert_rows(
        &rows,
        &format!("1|{key_a_obj}|image/png|{ORG_A}"),
        "nested upload: one child reference row under the session org",
    );
    let (status, bytes) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(status, 200, "the nested upload lands in the bucket");
    assert_eq!(bytes, payload_a, "the bucket holds the nested bytes");
    println!("leg N1 nested upload: 200, row `{rows}`");

    // ── LEG N2 — display: 307 → the signed URL returns the bytes ────────
    let r = nested_display(base, PERSON_A1, CONTACT_A1, key_a, "nested-display-a");
    assert_eq!(r.status, 307, "nested display redirects: {}", r.text());
    let location = r.location.expect("the 307 must carry a Location header");
    let signed = http(&location, &[], "nested-display-a-signed").expect("signed GET runs");
    assert_eq!(signed.status, 200, "{}", signed.text());
    assert_eq!(
        signed.body, payload_a,
        "the signed URL serves the nested bytes"
    );
    println!("leg N2 nested display: 307 → signed GET 200");

    // ── LEG N3 — remove: row gone AND object gone ───────────────────────
    let r = nested_remove(base, PERSON_A1, CONTACT_A1, key_a, "nested-remove-a");
    assert_eq!(r.status, 204, "nested remove answers 204: {}", r.text());
    let rows = contact_media_rows(target, CONTACT_A1).await;
    assert_rows(&rows, "0|||", "nested remove: the child row is gone");
    let (status, _) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(status, 404, "nested remove: the object is gone");
    println!("leg N3 nested remove: 204, row + object gone");

    // Re-upload so the cross-tenant legs have something to attack.
    let r = nested_upload(
        base,
        PERSON_A1,
        CONTACT_A1,
        key_a,
        &file_a,
        "image/png",
        "nested-upload-a-again",
    );
    assert_eq!(r.status, 200, "{}", r.text());
    let rows = contact_media_rows(target, CONTACT_A1).await;
    assert_rows(
        &rows,
        &format!("1|{key_a_obj}|image/png|{ORG_A}"),
        "org A owns one nested row before the denials",
    );

    // ── LEG N4 — cross-tenant: org B's key + org A's ids ⇒ 404 ──────────
    //
    // Both the parent gate (session org) and the link predicate must refuse;
    // the row/object must be untouched. A nested route that only checked the
    // org of the CHILD row would still leak org A's record to a caller who
    // guesses the parent id, so all three handlers are exercised.
    let r = nested_upload(
        base,
        PERSON_A1,
        CONTACT_A1,
        key_b,
        &file_b,
        "image/png",
        "nested-cross-upload",
    );
    assert_eq!(r.status, 404, "org B nested upload must 404: {}", r.text());
    let r = nested_display(base, PERSON_A1, CONTACT_A1, key_b, "nested-cross-display");
    assert_eq!(r.status, 404, "org B nested display must 404: {}", r.text());
    let r = nested_remove(base, PERSON_A1, CONTACT_A1, key_b, "nested-cross-remove");
    assert_eq!(r.status, 404, "org B nested remove must 404: {}", r.text());
    let rows = contact_media_rows(target, CONTACT_A1).await;
    assert_rows(
        &rows,
        &format!("1|{key_a_obj}|image/png|{ORG_A}"),
        "org B's denied nested calls must not touch org A's row",
    );
    let (status, bytes) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(status, 200, "org A's nested object survives");
    assert_eq!(bytes, payload_a, "org A's nested bytes are unchanged");
    println!("leg N4 nested cross-tenant: upload/display/remove all 404; A intact");

    // ── LEG N5 — link mismatch: org A's contact under a DIFFERENT parent ─
    //
    // This is the IDOR the ownership predicate closes: the id and the session
    // org are both valid, but `CONTACT_A1` belongs to `PERSON_A1`, not
    // `PERSON_A2`. The extractor alone would have served the record.
    for (tag, req) in [
        (
            "upload",
            nested_upload(
                base,
                PERSON_A2,
                CONTACT_A1,
                key_a,
                &file_b,
                "image/png",
                "nested-mismatch-upload",
            ),
        ),
        (
            "display",
            nested_display(
                base,
                PERSON_A2,
                CONTACT_A1,
                key_a,
                "nested-mismatch-display",
            ),
        ),
        (
            "remove",
            nested_remove(base, PERSON_A2, CONTACT_A1, key_a, "nested-mismatch-remove"),
        ),
    ] {
        assert_eq!(
            req.status,
            404,
            "a contact addressed under the WRONG parent must 404 on {tag}: {}",
            req.text()
        );
    }
    let rows = contact_media_rows(target, CONTACT_A1).await;
    assert_rows(
        &rows,
        &format!("1|{key_a_obj}|image/png|{ORG_A}"),
        "the link-mismatch denials must not touch the row",
    );
    let (status, bytes) = rustfs.get_object(&key_a_obj).expect("bucket read");
    assert_eq!(status, 200);
    assert_eq!(
        bytes, payload_a,
        "the link-mismatch denials must not rewrite the object"
    );
    println!("leg N5 nested link mismatch: upload/display/remove all 404; row + object intact");

    // ── LEG N6 — org B's OWN nested record still works ──────────────────
    let r = nested_upload(
        base,
        PERSON_B,
        CONTACT_B,
        key_b,
        &file_b,
        "image/png",
        "nested-b-own-upload",
    );
    assert_eq!(r.status, 200, "org B's own nested upload: {}", r.text());
    let key_b_obj = format!("{ORG_B}/assets/contact/{CONTACT_B}/avatar");
    let rows_b = contact_media_rows(target, CONTACT_B).await;
    assert_rows(
        &rows_b,
        &format!("1|{key_b_obj}|image/png|{ORG_B}"),
        "org B's nested upload lands under its own org + key",
    );
    let (status, bytes_b) = rustfs.get_object(&key_b_obj).expect("bucket read");
    assert_eq!(status, 200);
    assert_eq!(bytes_b, payload_b);
    println!("leg N6 nested own-tenant: B scoped to its own contact key");
}
