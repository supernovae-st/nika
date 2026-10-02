// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exact decimals under the number law: one form per value whatever its spelling, exact sums
//! and divisions, half-away-from-zero rounding only where asked, and the law's grammar.

use super::numbers::{Decimal, Law, exact, law, number_like};

fn d(text: &str) -> Decimal {
    Decimal::from_law(text).expect("a number the law reads")
}

#[test]
fn one_value_has_one_form_whatever_its_spelling() {
    for spelling in ["70", "70.0", "7e1", "700e-1", "7.000E+1", " 70\t"] {
        assert_eq!(d(spelling), d("70"), "{spelling:?}");
    }
    assert_eq!(d("-0"), Decimal::zero());
    assert_eq!(d("0.0e5"), Decimal::zero());
    assert_ne!(d("70"), d("70.000000000000001"));
    assert_eq!(d("70.0").to_string(), "70");
    assert_eq!(d("-1.50").to_string(), "-1.5");
    assert_eq!(d("1.25e3").to_string(), "1250");
    assert_eq!(d("5e-3").to_string(), "0.005");
}

#[test]
fn the_law_reads_only_its_grammar_and_finite_values() {
    for text in [
        "+5", "007", ".5", "1.", "1e", "0x10", "", " ", "1 2", "12\n", "NaN", "Infinity", "1e999",
        "--1",
    ] {
        assert_eq!(law(text), Law::NotANumber, "{text:?}");
    }
    // The exact reading admits no blank around the number; the law admits spaces and tabs.
    assert_eq!(exact(" 12"), Law::NotANumber);
    assert_eq!(law(" 12"), Law::Number(d("12")));
    assert!(matches!(law("1e-999"), Law::Number(_)));
    let wide = format!("0.{}", "1".repeat(1_500));
    assert_eq!(law(&wide), Law::Beyond);
    assert_eq!(law("1e-1001"), Law::Beyond);
}

#[test]
fn texts_that_look_like_numbers_are_told_apart_from_plain_text() {
    for text in ["007", "+5", ".5", "1.", " 12\n", "1E+3", "-inf", "NaN"] {
        assert!(number_like(text), "{text:?}");
    }
    for text in [
        "north",
        "2031-03-01",
        "12:30",
        "1,5",
        "A12",
        "",
        "1e",
        "1.2.3",
    ] {
        assert!(!number_like(text), "{text:?}");
    }
}

#[test]
fn sums_are_exact_where_a_binary_float_is_not() {
    assert_eq!(d("0.1").plus(&d("0.2")), d("0.3"));
    assert_ne!(d("0.30000000000000004"), d("0.3"));
    assert_eq!(d("-2.5").plus(&d("4.25")), d("1.75"));
    assert_eq!(d("1e20").plus(&d("1")), d("100000000000000000001"));
    assert_eq!(d("5").plus(&d("-5")), Decimal::zero());
    assert_eq!(d("-3").plus(&d("1")), d("-2"));
}

#[test]
fn a_division_is_exact_when_it_ends_and_rounds_only_when_asked() {
    assert_eq!(d("1").divided(4, None), Some(d("0.25")));
    assert_eq!(d("10").divided(3, None), None);
    assert_eq!(d("10").divided(3, Some(2)), Some(d("3.33")));
    assert_eq!(d("20").divided(3, Some(2)), Some(d("6.67")));
    assert_eq!(d("1").divided(3, Some(0)), Some(Decimal::zero()));
    assert_eq!(d("5").divided(0, None), None);
    // Half away from zero, on the exact value: 1.005 is 1.01, which a binary float misses.
    assert_eq!(d("1.005").rounded(2), d("1.01"));
    assert_eq!(d("2.5").rounded(0), d("3"));
    assert_eq!(d("-2.5").rounded(0), d("-3"));
    assert_eq!(d("0.004").rounded(2), Decimal::zero());
}

#[test]
fn order_follows_the_value() {
    let mut values = [
        d("10"),
        d("9.5"),
        d("-1"),
        d("0"),
        d("1e1"),
        d("-1.5"),
        d("0.01"),
    ];
    values.sort();
    let shown: Vec<String> = values.iter().map(ToString::to_string).collect();
    assert_eq!(shown, ["-1.5", "-1", "0", "0.01", "9.5", "10", "10"]);
}

#[test]
fn the_runtime_carries_ordinary_numbers_and_not_beyond_its_precision() {
    for text in [
        "0.1",
        "70",
        "-12.75",
        "1e300",
        "18446744073709551615",
        "-9223372036854775808",
        "0.30000000000000004",
    ] {
        assert!(d(text).survives_runtime(), "{text}");
    }
    for text in [
        "12345678901234567890123",
        "1.0000000000000000001",
        "0.1000000000000000000001",
    ] {
        assert!(!d(text).survives_runtime(), "{text}");
    }
}
