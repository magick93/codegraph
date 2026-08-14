//! Colored console output helpers (info/ok/warn/fail/section).

use std::sync::atomic::{AtomicBool, Ordering};

/// Global verbose flag — when set, failure paths dump log tails.
pub static VERBOSE: AtomicBool = AtomicBool::new(false);

const RED: &str = "\x1b[0;31m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[1;33m";
const CYAN: &str = "\x1b[0;36m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const NC: &str = "\x1b[0m";

pub fn info(msg: impl AsRef<str>) {
    println!("{CYAN}▸{NC} {}", msg.as_ref());
}

pub fn ok(msg: impl AsRef<str>) {
    println!("{GREEN}✓{NC} {}", msg.as_ref());
}

pub fn warn(msg: impl AsRef<str>) {
    println!("{YELLOW}⚠{NC} {}", msg.as_ref());
}

pub fn fail(msg: impl AsRef<str>) {
    println!("{RED}✗{NC} {}", msg.as_ref());
}

pub fn section(title: impl AsRef<str>) {
    println!("\n{BOLD}{}{NC}", title.as_ref());
}

pub fn bold(msg: impl AsRef<str>) -> String {
    format!("{BOLD}{}{NC}", msg.as_ref())
}

pub fn dim(msg: impl AsRef<str>) -> String {
    format!("{DIM}{}{NC}", msg.as_ref())
}

/// Set the global verbose flag (used by `--verbose`).
pub fn set_verbose(verbose: bool) {
    VERBOSE.store(verbose, Ordering::Relaxed);
}

pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

pub const GREEN_DEF: &str = GREEN;
pub const RED_DEF: &str = RED;
pub const NC_DEF: &str = NC;
