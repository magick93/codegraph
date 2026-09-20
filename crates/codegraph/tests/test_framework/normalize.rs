//! Environment-normalizing helpers for full-output byte-identity tests.
//!
//! WHY this module exists: generated `Cargo.toml` files embed values that are
//! *inputs* to generation, not generator output — the local checkout root
//! (path-mode deps like `path = "/home/<user>/git/codegraph/crates/..."`) and,
//! in git-rev mode, the generating binary's own rev (`CODEGRAPH_GIT_REV`).
//! PR #222's CI failure was exactly this: the flag-off byte-identity test
//! hashed raw bytes and failed on a different checkout. Any full-output
//! byte-identity test must run [`normalize_for_hash`] before hashing.

use sha2::{Digest, Sha256};

/// SHA-256 digest of `bytes` as a lowercase hex string.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Normalize environment-dependent values out of generated `Cargo.toml`
/// files before hashing: substitutes the local checkout root (resolved from
/// `env!("CARGO_MANIFEST_DIR")` ancestors) with `<CODEGRAPH_ROOT>` and
/// `rev = "<40-hex>"` git-rev pins with a 40-zero placeholder. Non-`Cargo.toml`
/// files pass through byte-identical.
///
/// WHY this exists: generated `Cargo.toml` files embed two values that are
/// *inputs* to generation, not generator output — the local checkout root
/// (path-mode deps like `path = "/home/<user>/git/codegraph/crates/..."`)
/// and, in git-rev mode, the generating binary's own rev
/// (`CODEGRAPH_GIT_REV`). PR #222's CI failure was exactly this: a
/// full-output byte-identity test hashed raw bytes and failed on a different
/// checkout. Any full-output byte-identity test must call this before
/// hashing; everything else in those files stays hash-guarded.
pub fn normalize_for_hash(path: &str, bytes: &[u8]) -> Vec<u8> {
    if !path.ends_with("Cargo.toml") {
        return bytes.to_vec();
    }
    let text = String::from_utf8_lossy(bytes);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("manifest dir two levels below workspace root")
        .to_string_lossy()
        .into_owned();
    let text = text.replace(root.trim_start_matches('/'), "<CODEGRAPH_ROOT>");
    let text = text.replace(&root, "<CODEGRAPH_ROOT>");
    // Shortest-relative paths from a differently-deep output directory embed
    // the checkout path minus its first component (see the
    // relative_path_fragments test below); replace that fragment too, or
    // hashing depends on where the workspace is checked out.
    let mut text = if let Some((_, fragment)) = root.trim_start_matches('/').split_once('/') {
        text.replace(fragment, "<CODEGRAPH_ROOT>")
    } else {
        text
    };
    // Climbing segments in front of the placeholder collapse away: outputs
    // at different depths (or absolute outputs) must hash identically —
    // the emitter emits shortest-relative paths whose dot-dot depth depends
    // on the output directory's depth.
    loop {
        let collapsed = text.replacen("../<CODEGRAPH_ROOT>", "<CODEGRAPH_ROOT>", 1);
        if collapsed == text {
            break;
        }
        text = collapsed;
    }
    let needle = "rev = \"";
    let placeholder = "0".repeat(40);
    let mut out = String::with_capacity(text.len());
    let mut rest: &str = &text;
    while let Some(i) = rest.find(needle) {
        let start = i + needle.len();
        let end = start + 40;
        let is_pin = end < rest.len()
            && rest[end..].starts_with('"')
            && rest[start..end].bytes().all(|b| b.is_ascii_hexdigit());
        if is_pin {
            out.push_str(&rest[..start]);
            out.push_str(&placeholder);
            rest = &rest[end..];
        } else {
            out.push_str(&rest[..=i]);
            rest = &rest[i + needle.len()..];
        }
    }
    out.push_str(rest);
    out.into_bytes()
}

#[test]
fn absolute_checkout_path_pin_is_normalized() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let toml =
        format!("[dependencies]\ncodegraph-core = {{ path = \"{root}/crates/codegraph-core\" }}\n");
    let out = normalize_for_hash("Cargo.toml", toml.as_bytes());
    // The stripped-root replace runs before the rooted one, so the leading
    // `/` of an absolute pin survives — legacy behaviour the snapshots bake
    // in; do not "fix" it here.
    let expected = b"[dependencies]\ncodegraph-core = { path = \"/<CODEGRAPH_ROOT>/crates/codegraph-core\" }\n";
    assert_eq!(out, expected.to_vec());
}

#[test]
fn relative_checkout_path_occurrence_is_normalized() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let stripped = root.trim_start_matches('/');
    let toml = format!(
        "[dependencies]\ncodegraph-ops = {{ path = \"../../../{stripped}/crates/codegraph-ops\" }}\n"
    );
    let out = normalize_for_hash("Cargo.toml", toml.as_bytes());
    // The fragment is replaced AND the climbing segments collapse, so the
    // hash no longer depends on the output directory's depth (#241).
    let expected =
        b"[dependencies]\ncodegraph-ops = { path = \"<CODEGRAPH_ROOT>/crates/codegraph-ops\" }\n";
    assert_eq!(out, expected.to_vec());
}

#[test]
fn rev_pin_is_replaced_with_zero_placeholder() {
    let toml = "[dependencies]\ncodegraph-core = { git = \"https://github.com/magick93/codegraph.git\", rev = \"1234abcd1234abcd1234abcd1234abcd1234abcd\" }\n";
    let out = normalize_for_hash("Cargo.toml", toml.as_bytes());
    let expected = "[dependencies]\ncodegraph-core = { git = \"https://github.com/magick93/codegraph.git\", rev = \"0000000000000000000000000000000000000000\" }\n";
    assert_eq!(out, expected.as_bytes().to_vec());
}

#[test]
fn cargo_toml_without_environment_pins_is_untouched() {
    let toml = b"[package]\nname = \"test-app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\naxum = \"0.7\"\n";
    let out = normalize_for_hash("Cargo.toml", toml);
    assert_eq!(out, toml.to_vec());
}

#[test]
fn non_cargo_toml_file_passes_through_byte_identical() {
    let checkout_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let bytes = format!("path = \"{checkout_root}/crates/codegraph-core\"\n").into_bytes();
    let out = normalize_for_hash("src/lib.rs", &bytes);
    assert_eq!(out, bytes);
}

#[test]
fn sha256_hex_round_trips_known_digests() {
    // SHA-256 of the empty string and of "abc" (NIST test vectors).
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn relative_path_fragments_of_the_checkout_are_normalized() {
    // Shortest-relative dependency paths from a differently-deep output
    // directory embed the checkout path minus its first component:
    // /tmp/opencode/cg-rls → "../../opencode/cg-rls/crates/...". The
    // absolute-root replacement cannot catch that fragment, and hashing
    // must not depend on where the workspace is checked out
    // (magick93/codegraph#241).
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("manifest dir two levels below workspace root")
        .to_string_lossy()
        .into_owned();
    let trimmed = root.trim_start_matches('/');
    let (_, fragment) = trimmed
        .split_once('/')
        .expect("checkout path has more than one component");

    let leaked = format!(
        "[dependencies]\ncodegraph-ops = {{ path = \"../../{fragment}/crates/codegraph-ops\" }}\n"
    );
    let normalized = String::from_utf8(normalize_for_hash("testkit/Cargo.toml", leaked.as_bytes()))
        .expect("utf8");

    assert!(
        !normalized.contains(fragment),
        "checkout fragment must be normalized away: {normalized}"
    );
    assert!(normalized.contains("<CODEGRAPH_ROOT>"), "{normalized}");
    // Outputs at different depths (or absolute outputs) must hash
    // identically: climbing segments in front of the placeholder collapse
    // away. CI (output under /tmp, checkout under /home/runner) and a
    // local worktree otherwise disagree byte-for-byte (#241).
    assert!(
        !normalized.contains("../"),
        "climbing segments before <CODEGRAPH_ROOT> must collapse: {normalized}"
    );
}
