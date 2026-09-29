// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A clause may state a canonical equivalent in a partly composed spelling.
use nika_compile::surface::observed::stated_spellings;

#[test]
fn observed_statement_binds_the_actual_composite_bytes() {
    let stated = "Ch\u{1a1}\u{300}";
    let observed = vec!["Ch\u{1edd}".to_owned(), "Cho\u{31b}\u{300}".to_owned()];
    let clause = format!("sum qty where status is {stated}");
    assert_eq!(
        stated_spellings(&clause, &observed, &["status".to_owned(), "qty".to_owned()]),
        observed
            .iter()
            .map(|s| (stated.to_owned(), s.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn observed_statement_keeps_multiword_values_and_canonical_mark_order() {
    let stated = "Ch\u{1a1}\u{300} giao";
    assert_eq!(
        stated_spellings(
            &format!("status is '{stated}'"),
            &["Ch\u{1edd} giao".to_owned()],
            &[]
        ),
        vec![(stated.to_owned(), "Ch\u{1edd} giao".to_owned())]
    );
    let stated = "a\u{301}\u{323}";
    assert_eq!(
        stated_spellings(
            &format!("status is {stated}"),
            &["\u{1ea1}\u{301}".to_owned()],
            &[]
        ),
        vec![(stated.to_owned(), "\u{1ea1}\u{301}".to_owned())]
    );
}

#[test]
fn observed_statement_preserves_boundaries_columns_and_exact_bytes() {
    let stated = "Ch\u{1a1}\u{300}";
    let observed = vec!["Ch\u{1edd}".to_owned()];
    for clause in [
        format!("x{stated}"),
        format!("{stated}x"),
        format!("{stated}\u{301}"),
        "status is Ch\u{1edd}".to_owned(),
        "status is ch\u{1edd}".to_owned(),
        "status is Cho".to_owned(),
    ] {
        assert!(
            stated_spellings(&clause, &observed, &[]).is_empty(),
            "{clause}"
        );
    }
    assert!(stated_spellings(&format!("sum {stated}"), &observed, &[stated.to_owned()]).is_empty());
    assert!(stated_spellings("status is ffi", &["\u{fb03}".to_owned()], &[]).is_empty());
    assert!(stated_spellings("status is anything", &[String::new()], &[]).is_empty());
}
