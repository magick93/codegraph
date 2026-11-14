//! Infallible text buffer for generated-code emission.
//!
//! Generators build generated Rust/SQL with `writeln!` into a `String`.
//! `io::Write for String` can never fail, yet every call needed
//! `.unwrap()`, which violates the project rule of no `unwrap()` in
//! production code. [`CodeWriter`] plus the [`wln!`] / [`w!`] macros make
//! the infallibility explicit: appends cannot fail, so there is no error
//! to plumb and nothing to unwrap.

use std::fmt;

/// Append-only text buffer for generated code.
///
/// `push_line` is the [`wln!`] equivalent of `writeln!` (always appends a
/// trailing `'\n'`); `push_str` is the `write!` equivalent (no newline).
pub struct CodeWriter {
    buf: String,
}

impl CodeWriter {
    pub fn new() -> Self {
        Self { buf: String::new() }
    }

    /// Appends `line` followed by a newline (mirrors `writeln!` semantics).
    pub fn push_line(&mut self, line: &str) {
        self.buf.push_str(line);
        self.buf.push('\n');
    }

    /// Appends `s` verbatim, without a trailing newline (mirrors `write!`).
    pub fn push_str(&mut self, s: &str) {
        self.buf.push_str(s);
    }

    /// Appends formatted output followed by a newline, **without** allocating
    /// an intermediate `String` (mirrors `writeln!` semantics).
    ///
    /// This is the [`wln!`] fast path: the previous implementation built a
    /// `format!(...)` per line and handed the result to [`Self::push_line`],
    /// costing one heap allocation per emitted line. Measured at ~1.8x the
    /// throughput of the allocating form (issue #489 Phase 0 benchmark).
    pub fn push_line_fmt(&mut self, args: fmt::Arguments<'_>) {
        // Writing into a `String` cannot fail; there is no error to plumb.
        let _ = fmt::Write::write_fmt(&mut self.buf, args);
        self.buf.push('\n');
    }

    /// Appends formatted output with no trailing newline and no intermediate
    /// allocation (mirrors `write!` semantics). The [`w!`] fast path.
    pub fn push_fmt(&mut self, args: fmt::Arguments<'_>) {
        let _ = fmt::Write::write_fmt(&mut self.buf, args);
    }

    pub fn as_str(&self) -> &str {
        &self.buf
    }

    pub fn into_string(self) -> String {
        self.buf
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

impl Default for CodeWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// `writeln!` into a [`CodeWriter`] — infallible by construction.
///
/// Always appends a trailing newline, exactly like `writeln!`:
///
/// - `wln!(w)` appends just `'\n'`
/// - `wln!(w, "literal")` appends the literal plus `'\n'`
/// - `wln!(w, "fmt {}", x)` appends the formatted text plus `'\n'`
macro_rules! wln {
    ($w:expr $(,)?) => {
        $w.push_line("")
    };
    ($w:expr, $fmt:expr $(,)?) => {
        $w.push_line_fmt(format_args!($fmt))
    };
    ($w:expr, $fmt:expr, $($arg:tt)+) => {
        $w.push_line_fmt(format_args!($fmt, $($arg)+))
    };
}

/// `write!` into a [`CodeWriter`] — infallible, no trailing newline.
macro_rules! w {
    ($w:expr, $fmt:expr $(,)?) => {
        $w.push_fmt(format_args!($fmt))
    };
    ($w:expr, $fmt:expr, $($arg:tt)+) => {
        $w.push_fmt(format_args!($fmt, $($arg)+))
    };
}

pub(crate) use w;
pub(crate) use wln;

#[cfg(test)]
mod tests {
    use super::CodeWriter;

    #[test]
    fn literal_only_line() {
        let mut w = CodeWriter::new();
        wln!(w, "hello world");
        assert_eq!(w.as_str(), "hello world\n");
    }

    #[test]
    fn interpolated_line() {
        let mut w = CodeWriter::new();
        let name = "table";
        wln!(w, "DROP {};", name);
        wln!(w, "x = {}", 1 + 1);
        assert_eq!(w.as_str(), "DROP table;\nx = 2\n");
    }

    #[test]
    fn empty_appends_bare_newline() {
        let mut w = CodeWriter::new();
        wln!(w);
        assert_eq!(w.as_str(), "\n");
        wln!(w,);
        assert_eq!(w.as_str(), "\n\n");
    }

    #[test]
    fn trailing_newline_matches_writeln_semantics() {
        let mut w = CodeWriter::new();
        wln!(w, "a");
        wln!(w, "b");
        assert!(w.as_str().ends_with('\n'));
        assert_eq!(w.as_str(), "a\nb\n");
    }

    #[test]
    fn push_str_is_raw_continuation() {
        let mut w = CodeWriter::new();
        w!(w, "vec![");
        wln!(w, "1, 2]");
        assert_eq!(w.as_str(), "vec![1, 2]\n");
    }

    #[test]
    fn escaped_braces_and_quotes_survive() {
        let mut w = CodeWriter::new();
        wln!(w, "{{}} \"quoted\"");
        assert_eq!(w.as_str(), "{} \"quoted\"\n");
    }

    #[test]
    fn into_string_consumes_buffer() {
        let mut w = CodeWriter::new();
        wln!(w, "line");
        assert!(!w.is_empty());
        let s = w.into_string();
        assert_eq!(s, "line\n");
    }

    #[test]
    fn default_is_empty() {
        let w = CodeWriter::default();
        assert!(w.is_empty());
        assert_eq!(w.as_str(), "");
    }

    #[test]
    fn push_line_fmt_matches_push_line() {
        let mut allocating = CodeWriter::new();
        allocating.push_line(&format!("id = {} name = {}", 7, "x"));

        let mut direct = CodeWriter::new();
        direct.push_line_fmt(format_args!("id = {} name = {}", 7, "x"));

        assert_eq!(allocating.as_str(), direct.as_str());
        assert_eq!(direct.as_str(), "id = 7 name = x\n");
    }

    #[test]
    fn push_fmt_has_no_trailing_newline() {
        let mut w = CodeWriter::new();
        w.push_fmt(format_args!("a{}", 1));
        w.push_fmt(format_args!("b"));
        assert_eq!(w.as_str(), "a1b");
    }

    /// The fast paths must be byte-identical to the allocating forms for the
    /// shapes generators actually emit, including escapes and multi-line
    /// interpolations.
    #[test]
    fn fmt_paths_are_byte_identical_to_allocating_paths() {
        let rows = ["alpha", "beta", "gamma"];
        let mut allocating = CodeWriter::new();
        for (i, row) in rows.iter().enumerate() {
            allocating.push_line(&format!("/// {}", row));
            allocating.push_line(&format!("pub fn {}() -> {} {{", row, i));
            allocating.push_line(&format!("    // {{}} \"quoted\" {}", i));
            allocating.push_line("}");
            allocating.push_line("");
        }

        let mut direct = CodeWriter::new();
        for (i, row) in rows.iter().enumerate() {
            direct.push_line_fmt(format_args!("/// {}", row));
            direct.push_line_fmt(format_args!("pub fn {}() -> {} {{", row, i));
            direct.push_line_fmt(format_args!("    // {{}} \"quoted\" {}", i));
            direct.push_line("}");
            direct.push_line("");
        }

        assert_eq!(allocating.as_str(), direct.as_str());
    }
}
