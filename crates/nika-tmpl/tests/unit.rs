// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unit battery for the island lexer — lives OUT of `src/` on purpose:
//! these tests exercise deliberately unbalanced-brace strings (`}}` runs ·
//! lone `{`), the exact inputs that defeat textual `src/` scanners
//! (check-unwrap's test-stripper counts braces without string-awareness).
//! Everything under test is `pub` — nothing here needs module access.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use nika_tmpl::{ScanError, find_island_close, scan_islands, single_island};

fn bodies(s: &str) -> Vec<&str> {
    scan_islands(s)
        .unwrap()
        .into_iter()
        .map(|i| i.body.trim())
        .collect()
}

#[test]
fn scans_single_and_prose_and_multiple() {
    assert_eq!(bodies("${{ inputs.x }}"), vec!["inputs.x"]);
    assert_eq!(bodies("before ${{ a }} after"), vec!["a"]);
    assert_eq!(bodies("${{ a }} mid ${{ b }}"), vec!["a", "b"]);
    assert!(bodies("no islands here").is_empty());
}

#[test]
fn quote_aware_close_is_not_fooled_by_braces_in_literals() {
    assert_eq!(
        bodies(r#"${{ inputs.x == "}}" }}"#),
        vec![r#"inputs.x == "}}""#]
    );
    assert_eq!(bodies("${{ inputs.x == '}}' }}"), vec!["inputs.x == '}}'"]);
    // escaped quote inside a literal does not close the literal early
    assert_eq!(bodies(r#"${{ "\"}}" }}"#), vec![r#""\"}}""#]);
}

#[test]
fn escaped_opener_is_literal_not_island() {
    assert!(bodies(r"\${{ not an island }}").is_empty());
    // real island after an escaped one
    assert_eq!(bodies(r"\${{ lit }} then ${{ real }}"), vec!["real"]);
}

#[test]
fn unterminated_is_an_error_with_offset() {
    assert_eq!(
        scan_islands("prefix ${{ dangling"),
        Err(ScanError::Unterminated { offset: 7 })
    );
}

#[test]
fn spans_are_exact_byte_offsets_including_unicode() {
    let s = "café ${{ x }}!";
    let islands = scan_islands(s).unwrap();
    assert_eq!(islands.len(), 1);
    let isl = islands[0];
    // "café " is 6 bytes (é = 2) — the `$` starts at byte 6
    assert_eq!(isl.start, 6);
    assert_eq!(&s[isl.start..isl.end], "${{ x }}");
    assert_eq!(isl.body.trim(), "x");
}

#[test]
fn scan_error_offset_is_total() {
    assert_eq!(scan_islands("ab ${{ x").unwrap_err().offset(), 3);
}

#[test]
fn scan_error_display_names_the_byte() {
    // Pins the hand-written Display body (an empty rendering — the
    // `Ok(Default::default())` mutant — must fail here).
    let msg = ScanError::Unterminated { offset: 7 }.to_string();
    assert!(msg.contains("unterminated"), "{msg}");
    assert!(msg.contains("byte 7"), "{msg}");
}

#[test]
fn exact_spans_pin_the_scan_advances() {
    // Precise (start, body, end) for adjacency + escape + lone-brace cases —
    // pins the loop advances (`i += 1/3`, `i = end`) and the `}}` `==` check:
    // any wrong advance drops/misplaces an island; the `==`→`!=` flip closes
    // on a lone `}`. Well-formedness alone doesn't catch a DROPPED island.
    let two = scan_islands("${{a}}${{b}}").unwrap();
    assert_eq!(two.len(), 2, "adjacent islands both found");
    assert_eq!((two[0].start, two[0].body, two[0].end), (0, "a", 6));
    assert_eq!((two[1].start, two[1].body, two[1].end), (6, "b", 12));

    // escaped opener THEN a real island — pins the `i += 3` escaped-skip
    // advance (a wrong skip drops or misreads the real second island).
    let esc = scan_islands(r"\${{a}}${{b}}").unwrap();
    assert_eq!(esc.len(), 1, "only the second (real) island");
    assert_eq!((esc[0].start, esc[0].body, esc[0].end), (7, "b", 13));

    // the escaped skip at offset ≥ 2 with a real island ADJACENT — an
    // arithmetic corruption of the skip (`+= 3` → `*= 3` overshoots from
    // byte 3 to byte 9) jumps PAST the real opener and drops the island.
    let adj = scan_islands(r"xy\${{${{z}}").unwrap();
    assert_eq!(adj.len(), 1, "the real island right after the escape");
    assert_eq!((adj[0].start, adj[0].body, adj[0].end), (6, "z", 12));

    // a lone `}` inside the body does NOT close (kills `==`→`!=`): the close
    // is the real `}}`, so the body spans through the single brace.
    let lone = scan_islands("${{ a } b }}").unwrap();
    assert_eq!(lone.len(), 1);
    assert_eq!(lone[0].body, " a } b ");

    // a stray `}}` in prose before an island is not a close; the not-an-opener
    // advance (`i += 1`) still finds the later island at its exact offset.
    let prose = scan_islands("x}} ${{ y }}").unwrap();
    assert_eq!(prose.len(), 1);
    assert_eq!((prose[0].start, prose[0].body.trim()), (4, "y"));
}

#[test]
fn escaped_quote_in_body_literal_pins_the_two_byte_skip() {
    // Inside a `"…"` body literal, `\"` is an escaped quote (the string does
    // NOT end there), so the FIRST `}}` (inside the literal) is body text and
    // the island closes at the OUTER `}}`. Pins the `i += 2` quote-escape skip.
    let isl = scan_islands(r#"${{ "a\"}}b" }}"#).unwrap();
    assert_eq!(isl.len(), 1);
    assert_eq!(isl[0].body.trim(), r#""a\"}}b""#);

    // The skip's DIRECTION pins too: sliding BACKWARD from the `\` (a
    // `+= 2` → `-= 2` corruption) re-reads the opening quote, closes the
    // literal early, and the `}}` inside it terminates the island at
    // byte 8 instead of 16 — a different body, not a hang.
    let dir = scan_islands(r#"${{ "x\q}}rest" }}"#).unwrap();
    assert_eq!(dir.len(), 1);
    assert_eq!(dir[0].body.trim(), r#""x\q}}rest""#);

    // The quote-CLOSE comparison pins its polarity (`==` → `!=` closes the
    // literal on any ordinary byte): a run of `}` inside a single-quoted
    // literal stays body text up to the real closing quote.
    let pol = scan_islands("${{ '}}}a' }}").unwrap();
    assert_eq!(pol.len(), 1);
    assert_eq!(pol[0].body.trim(), "'}}}a'");
}

#[test]
fn find_close_matches_the_scanner() {
    assert_eq!(find_island_close(" inputs.x }}", 0), Some(10));
    assert_eq!(find_island_close(" '}}' }}", 0), Some(6));
    assert_eq!(find_island_close(" no close", 0), None);
}

#[test]
fn single_island_is_type_preserving_only_when_whole() {
    assert_eq!(single_island("${{ ref }}"), Some("ref"));
    assert_eq!(single_island("  ${{ ref }}  "), Some("ref"));
    assert_eq!(single_island("prefix ${{ ref }}"), None);
    assert_eq!(single_island("${{ a }}${{ b }}"), None);
    assert_eq!(single_island("plain text"), None);
    // conservative: quoted `}}` inside makes it non-single (historical)
    assert_eq!(single_island("${{ x == '}}' }}"), None);
}
