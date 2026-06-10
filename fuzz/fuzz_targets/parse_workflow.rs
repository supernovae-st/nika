//! Fuzz target 1 · the canonical workflow parser.
//!
//! Invariant under fuzz · `nika_schema::parse` NEVER panics, aborts, or
//! hangs on arbitrary input — it returns `Ok(RawWorkflow)` or a typed
//! `SchemaError`. Both strict and lenient modes are exercised on every
//! input (they share the lexing/shape path · diverge on unknown fields).
//!
//! Corpus · `fuzz/corpus/parse_workflow/` (spec examples + conformance
//! fixtures · seeded from nika-spec).
#![no_main]

use libfuzzer_sys::fuzz_target;
use nika_schema::{parse, FileId, ParseMode};

fuzz_target!(|data: &[u8]| {
    if let Ok(src) = std::str::from_utf8(data) {
        let _ = parse(src, FileId::new(0), ParseMode::Strict);
        let _ = parse(src, FileId::new(0), ParseMode::Lenient);
    }
});
