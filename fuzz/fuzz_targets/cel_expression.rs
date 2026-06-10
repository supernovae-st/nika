//! Fuzz target 2 · the CEL v0.1-subset expression grammar.
//!
//! Invariant under fuzz · the recursive-descent parser NEVER panics,
//! overflows the stack, or hangs on arbitrary expression text — it returns
//! `Ok(Expr)` or a typed `ExprError`. The template island scanner gets the
//! same guarantee on arbitrary template strings (`${{ }}` islands ·
//! `\${{` escapes · unclosed delimiters).
//!
//! Grammar under test · spec/03-dag.md §Formal grammar (cel-subset/0.1).
//! Corpus · `fuzz/corpus/cel_expression/` (expression bodies extracted from
//! the spec examples + conformance fixtures).
#![no_main]

use libfuzzer_sys::fuzz_target;
use nika_schema::expression::{parse_expression, scan_templates};

fuzz_target!(|data: &[u8]| {
    if let Ok(src) = std::str::from_utf8(data) {
        let _ = parse_expression(src);
        let _ = scan_templates(src);
    }
});
