//! Git revision of the codegraph checkout this binary was built from.
//!
//! Embedded by `build.rs` via `cargo:rustc-env=CODEGRAPH_GIT_REV` (empty when
//! built outside a git checkout, e.g. from a crates.io tarball).

/// The full git SHA of the checkout this binary was built from, or an empty
/// string when the build environment has no git metadata.
pub fn codegraph_rev() -> &'static str {
    option_env!("CODEGRAPH_GIT_REV").unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rev_is_hex_sha_or_empty() {
        let rev = codegraph_rev();
        if !rev.is_empty() {
            assert_eq!(rev.len(), 40, "git SHAs are 40 hex chars: {rev}");
            assert!(
                rev.chars().all(|c| c.is_ascii_hexdigit()),
                "rev should be hex: {rev}"
            );
        }
    }
}
