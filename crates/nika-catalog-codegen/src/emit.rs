// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared string-emission helpers used by every per-catalog codegen module.
//!
//! Lifted verbatim (semantic-equivalent) from
//! `nika-catalog/build.rs::{rstr, opt_rstr, str_slice_expr, write_str_slice}`
//! so the emitted Rust source stays byte-for-byte identical with the
//! pre-extraction baseline (Gate 10 parity).

use std::fmt::Write as _;

/// Rust-escape a string into a double-quoted source literal.
///
/// Uses `char::escape_debug` for coverage of C0 controls, DEL (0x7F),
/// C1 controls (0x80..=0x9F), and Unicode scalars in general — wider
/// than a hand-rolled match ladder. Double-quote is the only char
/// `escape_debug` leaves unescaped that Rust string syntax requires
/// escaped, so we handle it explicitly.
#[must_use]
pub(crate) fn rstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' {
            out.push_str("\\\"");
        } else {
            for ec in c.escape_debug() {
                out.push(ec);
            }
        }
    }
    out.push('"');
    out
}

/// Optional-string variant of [`rstr`] — `None` → `"None"`, `Some(s)` →
/// `"Some(\"...\")"` with proper escaping.
#[must_use]
pub(crate) fn opt_rstr(s: Option<&str>) -> String {
    match s {
        Some(v) => format!("Some({})", rstr(v)),
        None => "None".to_string(),
    }
}

/// Emit a `&[&str]` expression suitable for a `const`/`static` context.
///
/// Empty list → `"&[]"` (no allocation in generated source). Non-empty →
/// `"&[\"a\", \"b\", ...]"`.
#[must_use]
pub(crate) fn str_slice_expr(xs: &[String]) -> String {
    if xs.is_empty() {
        return "&[]".to_string();
    }
    let mut out = String::from("&[");
    for (i, s) in xs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&rstr(s));
    }
    out.push(']');
    out
}

/// Emit `<indent>field: &[<elements>],` line. Used by provider emission
/// where the indent depth varies (struct field vs nested block).
pub(crate) fn write_str_slice(out: &mut String, field: &str, xs: &[String], indent: usize) {
    let pad = " ".repeat(indent);
    if xs.is_empty() {
        let _ = writeln!(out, "{pad}{field}: &[],");
        return;
    }
    let _ = write!(out, "{pad}{field}: &[");
    for (i, s) in xs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&rstr(s));
    }
    let _ = writeln!(out, "],");
}

/// Format an `Option<f64>` as Rust source — `None` → `"None"`, `Some(v)` →
/// `"Some(<debug-of-v>)"` for round-trippable parity with the legacy
/// pricing emit (uses `{v:?}` to preserve sign/precision).
#[must_use]
pub(crate) fn opt_f64(v: Option<f64>) -> String {
    match v {
        Some(f) => format!("Some({f:?})"),
        None => "None".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rstr_handles_quote() {
        assert_eq!(rstr("hello"), "\"hello\"");
        assert_eq!(rstr("a\"b"), "\"a\\\"b\"");
    }

    #[test]
    fn rstr_handles_escapes() {
        assert_eq!(rstr("a\nb"), "\"a\\nb\"");
        assert_eq!(rstr("tab\there"), "\"tab\\there\"");
        assert_eq!(rstr("back\\slash"), "\"back\\\\slash\"");
    }

    #[test]
    fn rstr_passes_unicode_through() {
        // escape_debug leaves printable Unicode unescaped.
        assert_eq!(rstr("café"), "\"café\"");
        assert_eq!(rstr("🦋"), "\"🦋\"");
    }

    #[test]
    fn opt_rstr_handles_none() {
        assert_eq!(opt_rstr(None), "None");
        assert_eq!(opt_rstr(Some("x")), "Some(\"x\")");
    }

    #[test]
    fn str_slice_expr_empty_is_slice_literal() {
        let v: Vec<String> = vec![];
        assert_eq!(str_slice_expr(&v), "&[]");
    }

    #[test]
    fn str_slice_expr_emits_with_spaces() {
        let v = vec!["a".to_string(), "b".to_string()];
        assert_eq!(str_slice_expr(&v), "&[\"a\", \"b\"]");
    }

    #[test]
    fn write_str_slice_indents_correctly() {
        let mut out = String::new();
        let v = vec!["x".to_string()];
        write_str_slice(&mut out, "aliases", &v, 8);
        assert_eq!(out, "        aliases: &[\"x\"],\n");
    }

    #[test]
    fn write_str_slice_empty_list() {
        let mut out = String::new();
        let v: Vec<String> = vec![];
        write_str_slice(&mut out, "aliases", &v, 8);
        assert_eq!(out, "        aliases: &[],\n");
    }

    #[test]
    fn opt_f64_round_trip_debug() {
        // {v:?} on f64 preserves enough precision for byte-equal codegen.
        // Use 3.5 (not 3.14 — clippy::approx_constant would flag PI).
        let s = opt_f64(Some(3.5));
        assert!(s.starts_with("Some(3.5"), "got: {s}");
        assert_eq!(opt_f64(None), "None");
    }
}
