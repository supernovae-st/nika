//! Fuzz target 3 · the capability CHECKER.
//!
//! Invariant under fuzz · for any input that PARSES, `nika_check::check`
//! NEVER panics, aborts, or hangs — it returns a `CheckReport` (findings or
//! clean). The checker is the component whose soundness the whole
//! capability-boundary argument rests on, so its robustness on arbitrary
//! workflows is a first-class fuzz invariant (the parser itself is covered by
//! `parse_workflow`). Both strict and lenient parses are checked.
//!
//! Corpus · `fuzz/corpus/cap_check/` (seeded from conformance permits fixtures).
#![no_main]

use libfuzzer_sys::fuzz_target;
use nika_check::check;
use nika_schema::{FileId, ParseMode, parse};

fuzz_target!(|data: &[u8]| {
    if let Ok(src) = std::str::from_utf8(data) {
        for mode in [ParseMode::Strict, ParseMode::Lenient] {
            if let Ok(wf) = parse(src, FileId::new(0), mode) {
                let _ = check(&wf);
            }
        }
    }
});
