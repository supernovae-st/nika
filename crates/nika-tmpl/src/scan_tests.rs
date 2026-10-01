// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Scanner behavior exercised by the library test target.

use super::{ScanError, find_island_close, scan_islands, single_island};

fn bodies(s: &str) -> Vec<&str> {
    scan_islands(s)
        .expect("scan")
        .into_iter()
        .map(|isl| isl.body.trim())
        .collect()
}

fn spans(s: &str) -> Vec<(usize, &str, usize, usize)> {
    scan_islands(s)
        .expect("scan")
        .into_iter()
        .map(|isl| (isl.start, isl.body, isl.body_start, isl.end))
        .collect()
}

#[test]
fn scans_single_and_prose_and_multiple() {
    assert_eq!(bodies("${{ inputs.x }}"), vec!["inputs.x"]);
    assert_eq!(bodies("before ${{ a }} after"), vec!["a"]);
    assert_eq!(bodies("${{ a }} mid ${{ b }}"), vec!["a", "b"]);
    assert!(bodies("no islands here").is_empty());
    assert!(bodies("").is_empty());
}

#[test]
fn every_span_is_exact_on_the_canonical_shapes() {
    assert_eq!(spans("${{ a }}"), vec![(0, " a ", 3, 8)]);
    assert_eq!(spans("x ${{ a }} y"), vec![(2, " a ", 5, 10)]);
    assert_eq!(spans("${{a}}${{b}}"), vec![(0, "a", 3, 6), (6, "b", 9, 12)]);
    assert_eq!(spans("${{}}"), vec![(0, "", 3, 5)]);
}

#[test]
fn spans_are_byte_offsets_never_char_offsets() {
    let s = "café ${{ x }}!";
    assert_eq!(spans(s), vec![(6, " x ", 9, 14)]);
    assert_eq!(&s[6..14], "${{ x }}");
}

#[test]
fn quote_aware_close_is_not_fooled_by_braces_in_literals() {
    assert_eq!(
        bodies(r#"${{ inputs.x == "}}" }}"#),
        vec![r#"inputs.x == "}}""#]
    );
    assert_eq!(bodies("${{ inputs.x == '}}' }}"), vec!["inputs.x == '}}'"]);
    assert_eq!(bodies(r#"${{ "\"}}" }}"#), vec![r#""\"}}""#]);
}

#[test]
fn escaped_opener_is_literal_not_island() {
    assert!(bodies(r"\${{ not an island }}").is_empty());
    assert_eq!(bodies(r"\${{ lit }} then ${{ real }}"), vec!["real"]);
}

#[test]
fn unterminated_is_an_error_at_the_opener_offset() {
    assert_eq!(
        scan_islands("prefix ${{ dangling"),
        Err(ScanError::Unterminated { offset: 7 })
    );
    assert_eq!(
        scan_islands("${{ ok }} then ${{ dangling"),
        Err(ScanError::Unterminated { offset: 15 })
    );
    assert_eq!(
        scan_islands("${{"),
        Err(ScanError::Unterminated { offset: 0 })
    );
}

#[test]
fn openers_that_are_not_openers_and_closers_without_openers() {
    assert!(bodies("}} stray closers }}").is_empty());
    assert!(bodies("${").is_empty());
    assert!(bodies("$").is_empty());
    assert!(bodies("{{ mustache }}").is_empty());
}

#[test]
fn scan_error_offset_is_total() {
    assert_eq!(scan_islands("ab ${{ x").unwrap_err().offset(), 3);
}

#[test]
fn scan_error_display_names_the_byte() {
    let msg = ScanError::Unterminated { offset: 7 }.to_string();
    assert!(msg.contains("unterminated"), "{msg}");
    assert!(msg.contains("byte 7"), "{msg}");
}

#[test]
fn exact_spans_pin_the_scan_advances() {
    assert_eq!(spans("${{a}}${{b}}"), vec![(0, "a", 3, 6), (6, "b", 9, 12)]);

    assert_eq!(spans(r"\${{a}}${{b}}"), vec![(7, "b", 10, 13)]);

    assert_eq!(spans(r"x\${{${{z}}"), vec![(5, "z", 8, 11)]);
    assert_eq!(spans(r"xy\${{${{z}}"), vec![(6, "z", 9, 12)]);

    assert_eq!(spans("${{ a } b }}"), vec![(0, " a } b ", 3, 12)]);

    assert_eq!(spans("x}} ${{ y }}"), vec![(4, " y ", 7, 12)]);
}

#[test]
fn escaped_quote_in_body_literal_pins_the_two_byte_skip() {
    assert_eq!(bodies(r#"${{ "a\"}}b" }}"#), vec![r#""a\"}}b""#]);

    assert_eq!(bodies(r#"${{ "x\q}}rest" }}"#), vec![r#""x\q}}rest""#]);

    assert_eq!(bodies("${{ '}}}a' }}"), vec!["'}}}a'"]);
}

#[test]
fn find_close_is_the_offset_of_the_closing_brace_pair() {
    assert_eq!(find_island_close(" inputs.x }}", 0), Some(10));
    assert_eq!(find_island_close(" '}}' }}", 0), Some(6));
    assert_eq!(find_island_close(" no close", 0), None);
    assert_eq!(find_island_close("", 0), None);

    let s = "a }} b }}";
    assert_eq!(find_island_close(s, 0), Some(2));
    assert_eq!(find_island_close(s, 4), Some(7));

    assert_eq!(find_island_close(" a } b }}", 0), Some(7));
    assert_eq!(find_island_close(" a }", 0), None);
    assert_eq!(find_island_close("}", 0), None);
}

#[test]
fn find_close_stays_inside_string_literals_until_they_end() {
    assert_eq!(find_island_close(" '}} ", 0), None);
    assert_eq!(find_island_close(r#" "}}" }}"#, 0), Some(6));
    assert_eq!(find_island_close(r#" "a\"}}b" }}"#, 0), Some(10));
    assert_eq!(find_island_close(r#" "it's" }}"#, 0), Some(8));
}

#[test]
fn single_island_is_type_preserving_only_when_the_whole_string_is_one() {
    assert_eq!(single_island("${{ ref }}"), Some("ref"));
    assert_eq!(single_island("${{ref}}"), Some("ref"));
    assert_eq!(single_island("  ${{ ref }}  "), Some("ref"));
    assert_eq!(single_island("${{ inputs.topic }}"), Some("inputs.topic"));
    assert_eq!(single_island("${{}}"), Some(""));

    assert_eq!(single_island("prefix ${{ ref }}"), None);
    assert_eq!(single_island("${{ ref }} suffix"), None);
    assert_eq!(single_island("${{ a }}${{ b }}"), None);
    assert_eq!(single_island("plain text"), None);
    assert_eq!(single_island(""), None);
    assert_eq!(single_island("${{ unterminated"), None);
}

#[test]
fn single_island_refuses_each_half_of_the_textual_reject_on_its_own() {
    assert_eq!(single_island("${{ ${{ x }}"), None);
    assert_eq!(single_island("${{ x == '}}' }}"), None);
    assert_eq!(single_island("${{ '}}' ${{ x }}"), None);
}

#[test]
fn single_island_defers_to_the_scanner_on_an_unterminated_quote() {
    assert_eq!(single_island("${{'{{{{}}"), None);
    assert!(scan_islands("${{'{{{{}}").is_err());
    assert_eq!(
        single_island("${{ tasks.a.output }}"),
        Some("tasks.a.output")
    );
}
