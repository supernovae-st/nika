// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Go-duration string parsing for the task-level `timeout:` field.
//!
//! Per spec `03-dag.md` §timeout · « Format · Go-duration / Kubernetes-style
//! string `[0-9]+(\.[0-9]+)?(ns|us|µs|ms|s|m|h)` » with the rules ·
//!
//! - Positive · `> 0`.
//! - Maximum · `24h`. « Tasks needing longer should split into a workflow
//!   chain. »
//! - Compound units · « combine in descending order (`1h30m500ms` ✓ ·
//!   `30m1h` ✗) ».
//! - Unit suffixes (case-sensitive) · `ns` · `us` (or `µs`) · `ms` · `s` ·
//!   `m` · `h`. « No `d`/`w` (use compound · `48h` instead of `2d`) » —
//!   note `48h` itself exceeds the 24h cap, so `24h` is the practical max.

use std::fmt;
use std::time::Duration;

/// Why a Go-duration string failed to parse.
///
/// Pure data — the parser wraps this into `SchemaError::BadTimeout` with
/// the YAML span.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GoDurationError {
    /// The string is empty.
    Empty,
    /// A segment is missing its unit suffix (e.g. `"30"`).
    MissingUnit,
    /// An unknown unit suffix (e.g. `"2d"` · `"5w"`).
    UnknownUnit {
        /// The offending unit text.
        unit: String,
    },
    /// Units are not in strictly descending order (e.g. `"30m1h"`).
    UnitOrder {
        /// The unit that broke the descending order.
        unit: String,
    },
    /// A segment has no digits or a malformed number.
    BadNumber,
    /// The total is zero (`"0s"`) — durations must be `> 0`.
    Zero,
    /// The total exceeds the `24h` maximum.
    TooLarge,
}

impl fmt::Display for GoDurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "duration is empty"),
            Self::MissingUnit => write!(
                f,
                "duration segment is missing a unit suffix (ns·us·µs·ms·s·m·h)"
            ),
            Self::UnknownUnit { unit } => write!(
                f,
                "unknown duration unit `{unit}` (valid: ns·us·µs·ms·s·m·h · no d/w)"
            ),
            Self::UnitOrder { unit } => write!(
                f,
                "duration units must be in descending order (`1h30m` ✓ · `30m1h` ✗) — `{unit}` is out of order"
            ),
            Self::BadNumber => write!(f, "duration segment has a malformed number"),
            Self::Zero => write!(f, "duration must be > 0"),
            Self::TooLarge => write!(f, "duration exceeds the 24h maximum"),
        }
    }
}

/// The unit suffixes, largest first. Index = rank for the descending-order
/// rule. `µs` is the alias of `us` (same rank).
const UNITS: &[(&str, u64)] = &[
    ("h", 3_600_000_000_000),
    ("m", 60_000_000_000),
    ("s", 1_000_000_000),
    ("ms", 1_000_000),
    ("us", 1_000),
    ("µs", 1_000),
    ("ns", 1),
];

/// 24 hours in nanoseconds — the spec maximum.
const MAX_NANOS: u128 = 24 * 3_600_000_000_000;

/// Rank of a unit for the descending-order rule (0 = `h` … 4 = `ns`).
/// `us` and `µs` share a rank.
fn unit_rank(unit: &str) -> Option<u32> {
    match unit {
        "h" => Some(0),
        "m" => Some(1),
        "s" => Some(2),
        "ms" => Some(3),
        "us" | "µs" => Some(4),
        "ns" => Some(5),
        _ => None,
    }
}

/// Parse a Go-duration string (`"500ms"` · `"1h30m"` · `"2.5s"`) into a
/// [`Duration`].
///
/// Grammar (spec `03-dag.md` §timeout) ·
/// `^([0-9]+(\.[0-9]+)?(ns|us|µs|ms|s|m|h))+$` · units case-sensitive ·
/// strictly descending order · total `> 0` and `≤ 24h`.
///
/// # Errors
///
/// Returns a [`GoDurationError`] describing the first violation.
pub fn parse_go_duration(input: &str) -> Result<Duration, GoDurationError> {
    if input.is_empty() {
        return Err(GoDurationError::Empty);
    }

    let mut total_nanos: u128 = 0;
    let mut last_rank: Option<u32> = None;
    let mut rest = input;

    while !rest.is_empty() {
        let (value, after_number) = take_number(rest)?;
        let (unit, after_unit) = take_unit(after_number)?;

        let rank = unit_rank(unit).ok_or_else(|| GoDurationError::UnknownUnit {
            unit: unit.to_owned(),
        })?;
        if let Some(prev) = last_rank {
            // Strictly descending — a repeat (`1h2h`) is also rejected.
            if rank <= prev {
                return Err(GoDurationError::UnitOrder {
                    unit: unit.to_owned(),
                });
            }
        }
        last_rank = Some(rank);

        let unit_nanos = UNITS
            .iter()
            .find(|(u, _)| *u == unit)
            .map(|(_, n)| *n)
            .ok_or_else(|| GoDurationError::UnknownUnit {
                unit: unit.to_owned(),
            })?;

        // Fractional segments (e.g. `2.5s`) go through f64; integral ones
        // stay exact. The 24h cap keeps every value far below f64's exact
        // integer range (unit_nanos ≤ 3.6e12 < 2^52), so neither the
        // u64→f64 conversion nor the rounding here can drift.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let segment_nanos: u128 = if value.fract() == 0.0 {
            (value as u128).saturating_mul(u128::from(unit_nanos))
        } else {
            (value * unit_nanos as f64).round() as u128
        };
        total_nanos = total_nanos.saturating_add(segment_nanos);
        if total_nanos > MAX_NANOS {
            return Err(GoDurationError::TooLarge);
        }

        rest = after_unit;
    }

    if total_nanos == 0 {
        return Err(GoDurationError::Zero);
    }

    // total_nanos ≤ MAX_NANOS < u64::MAX, so the cast is lossless.
    #[allow(clippy::cast_possible_truncation)]
    Ok(Duration::from_nanos(total_nanos as u64))
}

/// Take the leading `[0-9]+(\.[0-9]+)?` from `rest`.
fn take_number(rest: &str) -> Result<(f64, &str), GoDurationError> {
    let int_len = rest.chars().take_while(char::is_ascii_digit).count();
    if int_len == 0 {
        return Err(GoDurationError::BadNumber);
    }
    let mut len = int_len;
    let after_int = &rest[int_len..];
    if let Some(frac) = after_int.strip_prefix('.') {
        let frac_len = frac.chars().take_while(char::is_ascii_digit).count();
        if frac_len == 0 {
            return Err(GoDurationError::BadNumber);
        }
        len = int_len + 1 + frac_len;
    }
    let value: f64 = rest[..len]
        .parse()
        .map_err(|_| GoDurationError::BadNumber)?;
    Ok((value, &rest[len..]))
}

/// Take the leading unit suffix (longest match first · `ms` before `m`).
fn take_unit(rest: &str) -> Result<(&str, &str), GoDurationError> {
    if rest.is_empty() {
        return Err(GoDurationError::MissingUnit);
    }
    // Longest-match: 2-char units (`ms` `us` `ns` `µs`) before 1-char.
    for candidate in ["µs", "ms", "us", "ns", "h", "m", "s"] {
        if let Some(after) = rest.strip_prefix(candidate) {
            return Ok((candidate, after));
        }
    }
    // Anything else up to the next digit is an unknown unit (e.g. `d` · `w`).
    // Measure the prefix in BYTES (`len_utf8`), not chars — a multibyte char
    // in the garbage (`→` = 3 bytes) would otherwise make the slice cut
    // mid-char and panic (fuzz regression · nightly 2026-07-03).
    let unit_len: usize = rest
        .chars()
        .take_while(|c| !c.is_ascii_digit())
        .map(char::len_utf8)
        .sum();
    // `rest` is non-empty (checked above); if it somehow starts with a digit,
    // still surface one whole char in the error, sliced on a boundary.
    let end = unit_len.max(rest.chars().next().map_or(0, char::len_utf8));
    Err(GoDurationError::UnknownUnit {
        unit: rest[..end].to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_simple_units() {
        assert_eq!(parse_go_duration("500ms"), Ok(Duration::from_millis(500)));
        assert_eq!(parse_go_duration("30s"), Ok(Duration::from_secs(30)));
        assert_eq!(parse_go_duration("5m"), Ok(Duration::from_secs(300)));
        assert_eq!(parse_go_duration("24h"), Ok(Duration::from_secs(86_400)));
        assert_eq!(parse_go_duration("100ns"), Ok(Duration::from_nanos(100)));
        assert_eq!(parse_go_duration("7us"), Ok(Duration::from_micros(7)));
        assert_eq!(parse_go_duration("7µs"), Ok(Duration::from_micros(7)));
    }

    #[test]
    fn valid_compound_descending() {
        assert_eq!(parse_go_duration("1h30m"), Ok(Duration::from_secs(5_400)));
        assert_eq!(
            parse_go_duration("1h30m500ms"),
            Ok(Duration::from_millis(5_400_500))
        );
    }

    #[test]
    fn valid_fractional() {
        assert_eq!(parse_go_duration("2.5s"), Ok(Duration::from_millis(2_500)));
        assert_eq!(parse_go_duration("0.5h"), Ok(Duration::from_secs(1_800)));
    }

    #[test]
    fn invalid_no_unit() {
        assert_eq!(parse_go_duration("30"), Err(GoDurationError::MissingUnit));
    }

    #[test]
    fn invalid_unknown_unit() {
        assert!(matches!(
            parse_go_duration("2d"),
            Err(GoDurationError::UnknownUnit { unit }) if unit == "d"
        ));
        assert!(matches!(
            parse_go_duration("5w"),
            Err(GoDurationError::UnknownUnit { unit }) if unit == "w"
        ));
    }

    #[test]
    fn invalid_unit_order() {
        assert!(matches!(
            parse_go_duration("30m1h"),
            Err(GoDurationError::UnitOrder { unit }) if unit == "h"
        ));
        // Repeated unit is not descending either.
        assert!(matches!(
            parse_go_duration("1h2h"),
            Err(GoDurationError::UnitOrder { unit }) if unit == "h"
        ));
    }

    #[test]
    fn invalid_zero() {
        assert_eq!(parse_go_duration("0s"), Err(GoDurationError::Zero));
        assert_eq!(parse_go_duration("0h0m"), Err(GoDurationError::Zero));
    }

    #[test]
    fn invalid_too_large() {
        assert_eq!(parse_go_duration("25h"), Err(GoDurationError::TooLarge));
        assert_eq!(parse_go_duration("24h1ns"), Err(GoDurationError::TooLarge));
    }

    #[test]
    fn invalid_empty_and_garbage() {
        assert_eq!(parse_go_duration(""), Err(GoDurationError::Empty));
        assert_eq!(parse_go_duration("abc"), Err(GoDurationError::BadNumber));
        assert_eq!(parse_go_duration("1.s"), Err(GoDurationError::BadNumber));
        // Case-sensitive units: `30S` is not a unit.
        assert!(matches!(
            parse_go_duration("30S"),
            Err(GoDurationError::UnknownUnit { .. })
        ));
    }

    #[test]
    fn unicode_micro_alias_boundary_is_char_safe() {
        // `µ` is multi-byte — ensure slicing never panics mid-char.
        assert_eq!(parse_go_duration("1µs2ns"), Ok(Duration::from_nanos(1_002)));
    }

    #[test]
    fn unknown_unit_multibyte_garbage_is_char_safe() {
        // Fuzz regression (nightly 2026-07-03 · crash-21af10d1): the unknown-unit
        // error path measured the garbage prefix in CHARS but sliced in BYTES —
        // a multibyte char (`→` = 3 bytes) made `rest[..n]` cut mid-char and
        // panic. The sibling test above covers the KNOWN-unit µ path; this one
        // covers the error path the fuzzer actually hit.
        assert!(matches!(
            parse_go_duration("1→"),
            Err(GoDurationError::UnknownUnit { .. })
        ));
        assert!(matches!(
            parse_go_duration("1 →5"),
            Err(GoDurationError::UnknownUnit { .. })
        ));
        // the full crash-shaped value: huge fractional number + unicode garbage
        let crash = "15555555555555.5555555555555555555555555  - # %    `      \u{2192}5";
        assert!(matches!(
            parse_go_duration(crash),
            Err(GoDurationError::UnknownUnit { .. })
        ));
        // the reported unit prefix is char-boundary-clean (whole arrow, not a
        // truncated byte prefix of it)
        match parse_go_duration("1→") {
            Err(GoDurationError::UnknownUnit { unit }) => assert_eq!(unit, "→"),
            other => panic!("expected UnknownUnit, got {other:?}"),
        }
        // the WHOLE garbage prefix is reported (up to the next digit), not just
        // its first char — kills the take_while-condition-flip mutant.
        match parse_go_duration("1xy2s") {
            Err(GoDurationError::UnknownUnit { unit }) => assert_eq!(unit, "xy"),
            other => panic!("expected UnknownUnit, got {other:?}"),
        }
        match parse_go_duration("1 →5") {
            Err(GoDurationError::UnknownUnit { unit }) => assert_eq!(unit, " →"),
            other => panic!("expected UnknownUnit, got {other:?}"),
        }
    }

    #[test]
    fn error_display_renders_the_violation() {
        // Display carries the actionable detail (kills the fmt→Ok(()) mutant).
        let unknown = GoDurationError::UnknownUnit { unit: "d".into() };
        let msg = unknown.to_string();
        assert!(msg.contains('d'), "message names the offending unit: {msg}");
        assert!(
            !GoDurationError::Empty.to_string().is_empty(),
            "every variant renders a non-empty message"
        );
        assert_ne!(
            GoDurationError::Zero.to_string(),
            GoDurationError::TooLarge.to_string(),
            "distinct violations render distinct messages"
        );
    }
}
