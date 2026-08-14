// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Property + fuzz-safety tests for the template-island lexer (Gate 6).
//!
//! Since the checker and the runtime now BOTH consume `scan_islands`, parity is
//! by construction — so these tests pin the scanner's own invariants: it is
//! TOTAL (never panics on arbitrary input · the fuzz-safety class that bit the
//! sibling go-duration lexer) and every span it returns is well-formed
//! (char-boundary-safe · `${{`/`}}`-delimited · ordered · non-overlapping).

use nika_tmpl::{ScanError, scan_islands, single_island};
use proptest::prelude::*;

/// Every span a successful scan returns is structurally valid against `s`.
fn assert_spans_wellformed(s: &str) {
    let mut prev_end = 0usize;
    match scan_islands(s) {
        Ok(islands) => {
            for isl in islands {
                // ordered + non-overlapping
                assert!(isl.start >= prev_end, "islands must not overlap in {s:?}");
                // char-boundary safe (no mid-UTF8 slice can ever panic downstream)
                assert!(s.is_char_boundary(isl.start), "start off-boundary {s:?}");
                assert!(
                    s.is_char_boundary(isl.body_start),
                    "body_start off-boundary {s:?}"
                );
                assert!(s.is_char_boundary(isl.end), "end off-boundary {s:?}");
                assert!(isl.end <= s.len(), "end past eof {s:?}");
                // delimiters are exactly `${{ … }}`
                assert_eq!(isl.body_start, isl.start + 3, "body_start == start+3");
                assert_eq!(&s[isl.start..isl.body_start], "${{", "opener {s:?}");
                assert_eq!(&s[isl.end - 2..isl.end], "}}", "closer {s:?}");
                // body is exactly the slice between the delimiters
                assert_eq!(
                    isl.body,
                    &s[isl.body_start..isl.end - 2],
                    "body slice {s:?}"
                );
                prev_end = isl.end;
            }
        }
        Err(ScanError::Unterminated { offset }) => {
            // the reported offset is a real, on-boundary opener
            assert!(s.is_char_boundary(offset), "err offset off-boundary {s:?}");
            assert_eq!(
                &s[offset..offset + 3],
                "${{",
                "err offset not an opener {s:?}"
            );
        }
        // `ScanError` is `#[non_exhaustive]` — a future variant needs no new
        // structural assertion here.
        Err(_) => {}
    }
}

proptest! {
    // Found counterexamples PERSIST (#511): the default SourceParallel
    // walks up for a lib.rs/main.rs this integration target does not
    // have, so every CI-found seed was lost to the log. WithSource
    // writes `tests/proptest-regressions/` beside this file — check the
    // file in when a failure lands and the input pins forever.
    #![proptest_config(ProptestConfig {
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"),
        )),
        .. ProptestConfig::default()
    })]

    // Arbitrary UTF-8 — the scanner must never panic and always return
    // well-formed spans or a clean Unterminated error.
    #[test]
    fn scan_is_total_and_wellformed(s in ".*") {
        assert_spans_wellformed(&s);
    }

    // Hostile alphabet biased to the delimiter/quote/escape bytes — the exact
    // structure the general `.*` strategy rarely produces densely.
    #[test]
    fn scan_wellformed_on_hostile_alphabet(s in prop::collection::vec(
        prop::sample::select(vec!['{', '}', '$', '\'', '"', '\\', 'a', ' ', '世']), 0..24)
        .prop_map(|v| v.into_iter().collect::<String>()))
    {
        assert_spans_wellformed(&s);
    }

    // single_island ⟺ scan agreement (spn-nika): if `single_island` says the
    // whole string is one island, `scan_islands` MUST find exactly one spanning
    // the trimmed body. (The converse need not hold — single_island is
    // conservative on quoted `}}`, a documented false-negative.)
    #[test]
    fn single_island_agrees_with_scan(s in prop::collection::vec(
        prop::sample::select(vec!['{', '}', '$', '\'', '"', '\\', 'a', ' ']), 0..20)
        .prop_map(|v| v.into_iter().collect::<String>()))
    {
        if let Some(body) = single_island(&s) {
            match scan_islands(&s) {
                Ok(islands) => {
                    prop_assert_eq!(islands.len(), 1, "single_island Some but scan found {} · {:?}", islands.len(), s);
                    prop_assert_eq!(islands[0].body.trim(), body);
                }
                Err(_) => prop_assert!(false, "single_island Some but scan Err · {:?}", s),
            }
        }
    }
}

/// The #511 counterexample, pinned forever: an unterminated quote
/// swallowing the closer — the textual strip said `Some("'{{{{")`
/// while the quote-aware scan said `Unterminated`. One vocabulary,
/// one verdict: both refuse now (the legitimate forms stay).
#[test]
fn issue_511_unterminated_quote_refuses_in_both_vocabularies() {
    let s = "${{'{{{{}}";
    assert_eq!(
        single_island(s),
        None,
        "the fast path defers to the scanner"
    );
    assert!(scan_islands(s).is_err(), "the scanner still refuses");
    // The everyday forms are untouched.
    assert_eq!(single_island("${{ inputs.x }}"), Some("inputs.x"));
    assert_eq!(
        single_island("  ${{ tasks.a.output }}  "),
        Some("tasks.a.output")
    );
    assert_eq!(single_island("${{ a }} tail"), None);
}

/// Exhaustive: every string of length ≤ 5 over the hostile ASCII alphabet.
/// Deterministic complement to the randomized proptests (≈37k inputs).
#[test]
fn exhaustive_short_strings_never_panic_and_stay_wellformed() {
    const ALPHA: &[u8] = b"}{'\"\\$a ";
    fn rec(buf: &mut Vec<u8>, len: usize) {
        if buf.len() == len {
            if let Ok(s) = std::str::from_utf8(buf) {
                assert_spans_wellformed(s);
            }
            return;
        }
        for &b in ALPHA {
            buf.push(b);
            rec(buf, len);
            buf.pop();
        }
    }
    for len in 0..=5 {
        rec(&mut Vec::new(), len);
    }
}
