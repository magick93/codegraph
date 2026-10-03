//! Colored console output helpers (info/ok/warn/fail/section).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global verbose flag — with `--verbose`, quiet stages (dependency
/// compilation, browser downloads) also stream their child output inline.
/// Failure tails print unconditionally either way.
pub static VERBOSE: AtomicBool = AtomicBool::new(false);

/// The most recent ▸-level section title (`section()`), used by the CLI-level
/// early-results writer to record the stage a run failed in (#357). Empty
/// before the first section. `std::sync::Mutex` on purpose: sections are
/// printed before the async runtime matters and single-threaded per run.
static CURRENT_SECTION: Mutex<String> = Mutex::new(String::new());

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
    if let Ok(mut current) = CURRENT_SECTION.lock() {
        *current = title.as_ref().to_string();
    }
    println!("\n{BOLD}{}{NC}", title.as_ref());
}

/// The most recent section title (see [`section`]), or "" before the first.
/// Never locks up on poisoning: a panicking writer leaves the last value.
pub fn current_section() -> String {
    CURRENT_SECTION
        .lock()
        .map(|current| current.clone())
        .unwrap_or_default()
}

/// Print the last `n` lines of `text`, indented — failure diagnostics for
/// captured command output (regen, cargo check). Always shown, so a failed
/// stage explains itself without a --verbose rerun.
pub fn print_tail(text: &str, n: usize) {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    for line in &lines[start..] {
        println!("    {line}");
    }
}

/// Print one streamed child-process line (already label-prefixed by the
/// caller), dimmed so harness progress lines stay visually distinct.
pub fn stream_line(prefixed: &str) {
    println!("{DIM}{prefixed}{NC}");
}

/// Stage-start line for a streamed command: `[label] $ program args…`.
pub fn stream_start(prefixed: &str) {
    println!("{DIM}{prefixed}{NC}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_tracks_the_latest_title_for_the_early_results_writer() {
        // Last write wins (no other test prints sections, so this is stable
        // under parallel execution).
        section("1. Preflight");
        assert_eq!(current_section(), "1. Preflight");
        section("2. Database");
        assert_eq!(current_section(), "2. Database");
    }
}
