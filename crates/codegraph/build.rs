use std::path::Path;
use std::process::Command;

/// Extract the pinned git rev of the `sigil-model` dependency from
/// Cargo.lock (the `rev=` query parameter of its git source). Empty when
/// the lock file is absent or carries no pinned sigil-model package.
fn extract_sigil_rev() -> String {
    let lock_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    let Ok(text) = std::fs::read_to_string(&lock_path) else {
        return String::new();
    };
    let mut in_sigil_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[[") {
            in_sigil_package = false;
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("name = ") {
            in_sigil_package = name.trim_matches('"') == "sigil-model";
            continue;
        }
        if !in_sigil_package {
            continue;
        }
        if let Some(source) = trimmed.strip_prefix("source = ") {
            let source = source.trim_matches('"');
            if let Some(rest) = source.split("rev=").nth(1) {
                let rev: String = rest.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
                if !rev.is_empty() {
                    return rev;
                }
            }
        }
    }
    String::new()
}

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    println!("cargo:rerun-if-changed=.git/packed-refs");

    let rev = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=CODEGRAPH_GIT_REV={rev}");

    let sigil_rev = extract_sigil_rev();
    println!("cargo:rustc-env=SIGIL_REV={sigil_rev}");
    println!("cargo:rerun-if-changed=../../Cargo.lock");
}
