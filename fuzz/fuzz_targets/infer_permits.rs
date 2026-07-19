//! Fuzz target 4 · the `--infer-permits` synthesis surface.
//!
//! Invariant under fuzz · for any input that PARSES, `nika_schema::infer_permits`
//! (the mechanism behind `nika check --infer-permits`, which derives the
//! tightest boundary a workflow needs) NEVER panics, aborts, or hangs. This is
//! the second surface the thesis leans on — the audit flagged it uncovered
//! alongside the checker. Both strict and lenient parses feed the inference.
//!
//! Corpus · `fuzz/corpus/infer_permits/` (seeded from conformance permits fixtures).
#![no_main]

use libfuzzer_sys::fuzz_target;
use nika_schema::{infer_permits, parse, FileId, ParseMode};

fuzz_target!(|data: &[u8]| {
    if let Ok(src) = std::str::from_utf8(data) {
        for mode in [ParseMode::Strict, ParseMode::Lenient] {
            if let Ok(wf) = parse(src, FileId::new(0), mode) {
                let _ = infer_permits(&wf);
            }
        }
    }
});
