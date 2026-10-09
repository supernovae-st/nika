// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The two readings of text the behavioural judge compares by, as properties over their public
//! surface (ADR-003 Gate 6 · ADR-149). The number law reads a number's text into one exact form
//! per value, never through a binary float: the reading of an integer's text agrees with the
//! integer's own order, sum, sign and exact quotient, its plain form reads back as the same
//! number, and the spellings of one value name one number. The date-time shape is lexical and
//! total: a shape it returns masks exactly the digits of a prefix of the text and keeps the rest
//! as the offset, every date-time spelled in a form it admits is read with that form and offset,
//! and no other separator and no bare date has a shape.

use nika_compile_behavior::behavior::Decimal;
use nika_compile_behavior::instant_shape;
use proptest::prelude::*;

/// The number the law reads in an integer's text: always one.
fn integer(value: i128) -> Option<Decimal> {
    Decimal::from_law(&value.to_string())
}

/// `text` with every ASCII digit written `9`, the classifier's form.
fn masked(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_digit() { '9' } else { c })
        .collect()
}

/// The offsets the classifier admits: none, `Z`, `±HH`, `±HHMM` and `±HH:MM`.
fn offset() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("Z".to_owned()),
        "[+-][0-9]{2}",
        "[+-][0-9]{4}",
        "[+-][0-9]{2}:[0-9]{2}",
    ]
}

/// A date-time up to its offset, in a form the classifier admits: a date, `T` or a space, the
/// hour and minute, optional seconds, an optional fraction.
fn stamp() -> impl Strategy<Value = String> {
    (
        "[0-9]{4}-[0-9]{2}-[0-9]{2}",
        prop_oneof![Just('T'), Just(' ')],
        "[0-9]{2}:[0-9]{2}",
        proptest::option::of("[0-9]{2}"),
        proptest::option::of("[0-9]{1,4}"),
    )
        .prop_map(|(date, separator, time, seconds, fraction)| {
            let seconds = seconds.map(|s| format!(":{s}")).unwrap_or_default();
            let fraction = fraction.map(|f| format!(".{f}")).unwrap_or_default();
            format!("{date}{separator}{time}{seconds}{fraction}")
        })
}

proptest! {
    /// An integer's text reads as that integer: the same order, the same exact sum, the same
    /// opposite, and a number plus its opposite is zero.
    #[test]
    fn an_integers_text_reads_as_the_integer(a in any::<i64>(), b in any::<i64>()) {
        let (left, right) = (integer(a.into()), integer(b.into()));
        prop_assert!(left.is_some() && right.is_some());
        let (Some(left), Some(right)) = (left, right) else { return Ok(()) };
        prop_assert_eq!(left.cmp(&right), a.cmp(&b));
        prop_assert_eq!(Some(left.plus(&right)), integer(i128::from(a) + i128::from(b)));
        prop_assert_eq!(Some(left.negated()), integer(-i128::from(a)));
        prop_assert!(left.plus(&left.negated()).is_zero());
        prop_assert_eq!(Decimal::from_count(a.unsigned_abs()), left.negated().max(left));
    }

    /// The plain form a number displays reads back as the same number, whatever its exponent.
    #[test]
    fn the_plain_form_reads_back_as_the_same_number(m in any::<i64>(), e in -30_i32..30) {
        let number = Decimal::from_law(&format!("{m}e{e}"));
        prop_assert!(number.is_some());
        let Some(number) = number else { return Ok(()) };
        prop_assert!(!number.to_string().contains(['e', 'E']));
        prop_assert_eq!(Decimal::from_law(&number.to_string()), Some(number));
    }

    /// Trailing zeros, an exponent of zero and a shifted exponent spell the same number.
    #[test]
    fn the_spellings_of_one_value_name_one_number(a in any::<i64>(), zeros in 1_usize..6) {
        let one = integer(a.into());
        prop_assert!(one.is_some());
        let zeros = "0".repeat(zeros);
        prop_assert_eq!(&Decimal::from_law(&format!("{a}.{zeros}")), &one);
        prop_assert_eq!(&Decimal::from_law(&format!("{a}e0")), &one);
        prop_assert_eq!(&Decimal::from_law(&format!("{a}.{zeros}E+0")), &one);
        if a != 0 {
            prop_assert_eq!(&Decimal::from_law(&format!("{a}0e-1")), &one);
        }
    }

    /// A division that ends is exact, and rounding an integer leaves it as it is.
    #[test]
    fn an_exact_quotient_and_a_rounded_integer_are_the_integer(
        q in any::<i32>(),
        divisor in 1_u64..10_000,
        places in 0_u32..6,
    ) {
        let quotient = integer(q.into());
        let product = integer(i128::from(q) * i128::from(divisor));
        prop_assert!(quotient.is_some() && product.is_some());
        let (Some(quotient), Some(product)) = (quotient, product) else { return Ok(()) };
        prop_assert_eq!(product.divided(divisor, None), Some(quotient.clone()));
        prop_assert_eq!(quotient.rounded(places), quotient.clone());
        prop_assert_eq!(quotient.divided(1, None), Some(quotient));
    }

    /// Whatever the text, a shape masks exactly the digits of a prefix of it, and its offset is
    /// the rest of the text.
    #[test]
    fn a_shape_masks_a_prefix_and_keeps_the_rest_as_its_offset(
        text in prop_oneof![".*", "[0-9T :+.Z-]{10,32}"],
    ) {
        if let Some((form, offset)) = instant_shape(&text) {
            prop_assert_eq!(form.len() + offset.len(), text.len());
            prop_assert!(text.ends_with(&offset));
            prop_assert_eq!(masked(&text[..form.len()]), form);
        }
    }

    /// A date-time spelled in an admitted form is read with that form and that offset.
    #[test]
    fn an_admitted_date_time_is_read_with_its_form_and_offset(
        stamp in stamp(),
        offset in offset(),
    ) {
        let text = format!("{stamp}{offset}");
        prop_assert_eq!(instant_shape(&text), Some((masked(&stamp), offset)));
    }

    /// Another separator between the date and the time, or a bare date, has no shape.
    #[test]
    fn another_separator_or_a_bare_date_has_no_shape(
        stamp in stamp(),
        separator in "[^T ]",
    ) {
        let other = format!("{}{separator}{}", &stamp[..10], &stamp[11..]);
        prop_assert_eq!(instant_shape(&other), None);
        prop_assert_eq!(instant_shape(&stamp[..10]), None);
    }
}
