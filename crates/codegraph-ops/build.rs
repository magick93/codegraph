fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    println!("cargo:rerun-if-changed=.git/packed-refs");

    // Embed the checkout's git rev (mirrors crates/codegraph/build.rs). The
    // generated testkit crate pins codegraph-ops at the SAME rev as the other
    // codegraph crates, so this is the pinned-generator rev the freshness
    // check compares against `.codegraph-manifest.json`'s `codegraphCommit`.
    // Empty when git fails (path-dep / non-git builds) — the freshness check
    // then warns instead of comparing (never errors).
    let rev = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=CODEGRAPH_OPS_REV={rev}");
}
