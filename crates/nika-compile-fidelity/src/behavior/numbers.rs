// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exact decimal numbers under the one number law: a finite JSON number, or a text the law's
//! grammar accepts (the reader's `NUMBER_TEXT`), kept as its exact decimal value. Nothing is
//! rounded through a binary float: a float parse only tests the law's finiteness condition, and
//! whether the runtime's number type carries a source value unchanged.

use std::cmp::Ordering;
use std::fmt;

/// The widest exponent, in either direction, a number keeps for an exact comparison.
const MAX_EXPONENT: i64 = 1_000;
/// The most significant digits a number keeps for an exact comparison.
const MAX_DIGITS: usize = 1_000;
/// The extra digits a division computes: the expansion of a quotient that ends always ends
/// within them for a divisor below 2^64, whose powers of 2 and 5 are both below 64.
const DIVISION_DIGITS: i64 = 64;

/// An exact decimal number, normalized so that equal values have one form: `70`, `70.0` and
/// `7e1` are the same number, and `-0` is zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decimal {
    negative: bool,
    /// Base-10 digits, most significant first, without a leading or trailing zero; empty for
    /// zero.
    digits: Vec<u8>,
    /// The power of ten the last digit stands for.
    exponent: i64,
}

/// What the number law reads in a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Law {
    /// A number the law reads.
    Number(Decimal),
    /// No number the law reads.
    NotANumber,
    /// A number the law reads, beyond the precision this component compares exactly.
    Beyond,
}

/// The parts of a number in the law's grammar, blanks excluded:
/// `-?(0|[1-9][0-9]*)([.][0-9]+)?([eE][+-]?[0-9]+)?`.
struct Parts<'a> {
    negative: bool,
    whole: &'a str,
    fraction: &'a str,
    exponent_negative: bool,
    exponent: &'a str,
}

/// The run of ASCII digits of `text` from byte `from`, and the byte after it.
fn digits_from(text: &str, from: usize) -> (&str, usize) {
    let bytes = text.as_bytes();
    let mut end = from;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    (text.get(from..end).unwrap_or_default(), end)
}

/// The parts of `text` when the whole of it is one number of the law's grammar.
fn parts(text: &str) -> Option<Parts<'_>> {
    let bytes = text.as_bytes();
    let negative = bytes.first() == Some(&b'-');
    let (whole, mut at) = digits_from(text, usize::from(negative));
    if whole.is_empty() || (whole.len() > 1 && whole.starts_with('0')) {
        return None;
    }
    let mut fraction = "";
    if bytes.get(at) == Some(&b'.') {
        let (digits, end) = digits_from(text, at + 1);
        if digits.is_empty() {
            return None;
        }
        fraction = digits;
        at = end;
    }
    let mut exponent_negative = false;
    let mut exponent = "";
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        match bytes.get(at) {
            Some(b'-') => {
                exponent_negative = true;
                at += 1;
            }
            Some(b'+') => at += 1,
            _ => {}
        }
        let (digits, end) = digits_from(text, at);
        if digits.is_empty() {
            return None;
        }
        exponent = digits;
        at = end;
    }
    (at == bytes.len()).then_some(Parts {
        negative,
        whole,
        fraction,
        exponent_negative,
        exponent,
    })
}

/// What the law reads in `text` once any space or tab around it is removed.
pub(crate) fn law(text: &str) -> Law {
    exact(text.trim_matches(&[' ', '\t'][..]))
}

/// What the law reads in `text` exactly as it stands: the grammar with no blank, then a value
/// a float holds as finite (`1e999` is no number).
pub(crate) fn exact(text: &str) -> Law {
    let Some(parts) = parts(text) else {
        return Law::NotANumber;
    };
    if !text.parse::<f64>().is_ok_and(f64::is_finite) {
        return Law::NotANumber;
    }
    Decimal::from_parts(&parts).map_or(Law::Beyond, Law::Number)
}

/// Whether `text`, a value the law does not read as a number, still looks like one to some
/// number reader: a sign, digits with at most one point (`007`, `+5`, `.5`, `1.`), an
/// exponent, blanks around, or a word for an infinity or a missing number. Whether such a text
/// is a number is the runtime's reading, never a fact this component states.
pub(crate) fn number_like(text: &str) -> bool {
    let trimmed = text.trim();
    let unsigned = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    if matches!(
        unsigned.to_ascii_lowercase().as_str(),
        "nan" | "inf" | "infinity"
    ) {
        return true;
    }
    let (mantissa, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, None), |(mantissa, exponent)| {
            (mantissa, Some(exponent))
        });
    let (mut digits, mut points) = (0_usize, 0_usize);
    for byte in mantissa.bytes() {
        match byte {
            b'0'..=b'9' => digits += 1,
            b'.' => points += 1,
            _ => return false,
        }
    }
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && e.bytes().all(|b| b.is_ascii_digit())
    });
    digits > 0 && points <= 1 && exponent_ok
}

impl Decimal {
    /// The number the law reads in `text` (spaces or tabs around it allowed), when it reads
    /// one this component compares exactly.
    #[must_use]
    pub fn from_law(text: &str) -> Option<Self> {
        let Law::Number(number) = law(text) else {
            return None;
        };
        Some(number)
    }

    /// Zero.
    #[must_use]
    pub fn zero() -> Self {
        Self {
            negative: false,
            digits: Vec::new(),
            exponent: 0,
        }
    }

    /// A count of records.
    #[must_use]
    pub fn from_count(count: u64) -> Self {
        let digits = count.to_string().bytes().map(|b| b - b'0').collect();
        Self::normalized(false, digits, 0)
    }

    /// Whether this is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    fn from_parts(parts: &Parts<'_>) -> Option<Self> {
        let stated: i64 = if parts.exponent.is_empty() {
            0
        } else {
            parts.exponent.parse().ok()?
        };
        let stated = if parts.exponent_negative {
            -stated
        } else {
            stated
        };
        let fraction = i64::try_from(parts.fraction.len()).ok()?;
        let digits = parts
            .whole
            .bytes()
            .chain(parts.fraction.bytes())
            .map(|b| b - b'0')
            .collect();
        let number = Self::normalized(parts.negative, digits, stated.checked_sub(fraction)?);
        number.within_bounds().then_some(number)
    }

    /// One form per value: no leading or trailing zero, a zero never negative.
    fn normalized(negative: bool, mut digits: Vec<u8>, mut exponent: i64) -> Self {
        let leading = digits.iter().take_while(|d| **d == 0).count();
        digits.drain(..leading);
        while digits.last() == Some(&0) {
            digits.pop();
            exponent = exponent.saturating_add(1);
        }
        if digits.is_empty() {
            return Self::zero();
        }
        Self {
            negative,
            digits,
            exponent,
        }
    }

    fn width(&self) -> i64 {
        i64::try_from(self.digits.len()).unwrap_or(i64::MAX)
    }

    /// The power of ten just above the most significant digit.
    fn top(&self) -> i64 {
        self.exponent.saturating_add(self.width())
    }

    fn within_bounds(&self) -> bool {
        self.digits.len() <= MAX_DIGITS
            && self.exponent.abs() <= MAX_EXPONENT
            && self.top().abs() <= MAX_EXPONENT
    }

    fn signum(&self) -> Ordering {
        if self.digits.is_empty() {
            Ordering::Equal
        } else if self.negative {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }

    /// The digits of this number's magnitude scaled to `exponent` (at most its own).
    fn aligned(&self, exponent: i64) -> Vec<u8> {
        let shift = usize::try_from(self.exponent.saturating_sub(exponent)).unwrap_or(0);
        let mut digits = self.digits.clone();
        digits.resize(digits.len() + shift, 0);
        digits
    }

    /// The exact sum of two numbers.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        if self.is_zero() {
            return other.clone();
        }
        if other.is_zero() {
            return self.clone();
        }
        let exponent = self.exponent.min(other.exponent);
        let (left, right) = (self.aligned(exponent), other.aligned(exponent));
        if self.negative == other.negative {
            return Self::normalized(self.negative, add_digits(&left, &right), exponent);
        }
        match compare_digits(&left, &right) {
            Ordering::Equal => Self::zero(),
            Ordering::Greater => {
                Self::normalized(self.negative, sub_digits(&left, &right), exponent)
            }
            Ordering::Less => Self::normalized(other.negative, sub_digits(&right, &left), exponent),
        }
    }

    /// `floor(|self| × 10^shift / divisor)` as digits, and whether no remainder was dropped.
    fn scaled_quotient(&self, shift: i64, divisor: u64) -> (Vec<u8>, bool) {
        let scale = self.exponent.saturating_add(shift);
        let mut dividend = self.digits.clone();
        if scale > 0 {
            let zeros = usize::try_from(scale).unwrap_or(0);
            dividend.resize(dividend.len() + zeros, 0);
        }
        let (mut quotient, remainder) = long_division(&dividend, divisor);
        let mut exact = remainder == 0;
        if scale < 0 {
            let dropped = usize::try_from(scale.unsigned_abs())
                .unwrap_or(usize::MAX)
                .min(quotient.len());
            let kept = quotient.len() - dropped;
            exact &= quotient
                .get(kept..)
                .unwrap_or_default()
                .iter()
                .all(|d| *d == 0);
            quotient.truncate(kept);
        }
        if quotient.is_empty() {
            quotient.push(0);
        }
        (quotient, exact)
    }

    /// `self / divisor`: rounded half away from zero to `places` decimals when the request
    /// rounds, else exact when its decimal expansion ends; `None` for an expansion that never
    /// ends (the request states no precision for it) or a zero divisor.
    #[must_use]
    pub fn divided(&self, divisor: u64, places: Option<u32>) -> Option<Self> {
        if divisor == 0 {
            return None;
        }
        if self.is_zero() {
            return Some(Self::zero());
        }
        if let Some(places) = places {
            // The first digit past the stated places decides, half away from zero.
            let (digits, _) = self.scaled_quotient(i64::from(places) + 1, divisor);
            let (last, kept) = digits.split_last()?;
            let kept = if *last >= 5 {
                add_digits(kept, &[1])
            } else {
                kept.to_vec()
            };
            return Some(Self::normalized(self.negative, kept, -i64::from(places)));
        }
        let shift = DIVISION_DIGITS.saturating_sub(self.exponent.min(0));
        let (digits, exact) = self.scaled_quotient(shift, divisor);
        exact.then(|| Self::normalized(self.negative, digits, -shift))
    }

    /// This number rounded half away from zero to `places` decimals.
    #[must_use]
    pub fn rounded(&self, places: u32) -> Self {
        self.divided(1, Some(places))
            .unwrap_or_else(|| self.clone())
    }

    /// Whether the runtime's number type carries this value unchanged: an integer within 64
    /// bits, or a value whose shortest binary-float form reads back as the same decimal. A
    /// source value it does not carry cannot be judged by exact equality once a run rewrites it.
    pub(crate) fn survives_runtime(&self) -> bool {
        if self.exponent >= 0 && self.top() <= 20 {
            let text = self.to_string();
            let fits = if self.negative {
                text.parse::<i64>().is_ok()
            } else {
                text.parse::<u64>().is_ok()
            };
            if fits {
                return true;
            }
        }
        let Ok(float) = self.to_string().parse::<f64>() else {
            return false;
        };
        float.is_finite() && Self::from_law(&format!("{float:e}")).as_ref() == Some(self)
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        let by_sign = self.signum().cmp(&other.signum());
        if by_sign != Ordering::Equal || self.is_zero() {
            return by_sign;
        }
        let magnitude = self.top().cmp(&other.top()).then_with(|| {
            // Aligned at their most significant digit, a missing digit reads as zero.
            let len = self.digits.len().max(other.digits.len());
            (0..len)
                .map(|at| {
                    let left = self.digits.get(at).copied().unwrap_or(0);
                    left.cmp(&other.digits.get(at).copied().unwrap_or(0))
                })
                .find(|order| *order != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        });
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Decimal {
    /// The plain decimal form: `70`, `-0.5`, `1250`, never an exponent.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_zero() {
            return f.write_str("0");
        }
        let digits: String = self.digits.iter().map(|d| char::from(b'0' + d)).collect();
        let sign = if self.negative { "-" } else { "" };
        if self.exponent >= 0 {
            let zeros = "0".repeat(usize::try_from(self.exponent).unwrap_or(0));
            return write!(f, "{sign}{digits}{zeros}");
        }
        let point = usize::try_from(self.exponent.unsigned_abs()).unwrap_or(usize::MAX);
        if point >= digits.len() {
            let zeros = "0".repeat(point - digits.len());
            write!(f, "{sign}0.{zeros}{digits}")
        } else {
            let (whole, fraction) = digits.split_at(digits.len() - point);
            write!(f, "{sign}{whole}.{fraction}")
        }
    }
}

/// The sum of two magnitudes given as digits, most significant first.
fn add_digits(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(left.len().max(right.len()) + 1);
    let (mut left_at, mut right_at) = (left.len(), right.len());
    let mut carry = 0_u8;
    while left_at > 0 || right_at > 0 || carry > 0 {
        let mut sum = carry;
        if left_at > 0 {
            left_at -= 1;
            sum += left.get(left_at).copied().unwrap_or(0);
        }
        if right_at > 0 {
            right_at -= 1;
            sum += right.get(right_at).copied().unwrap_or(0);
        }
        out.push(sum % 10);
        carry = sum / 10;
    }
    out.reverse();
    out
}

/// `larger - smaller` for two magnitudes given as digits, most significant first.
fn sub_digits(larger: &[u8], smaller: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(larger.len());
    let mut smaller_at = smaller.len();
    let mut borrow = 0_u8;
    for digit in larger.iter().rev() {
        let mut take = borrow;
        if smaller_at > 0 {
            smaller_at -= 1;
            take += smaller.get(smaller_at).copied().unwrap_or(0);
        }
        if *digit >= take {
            out.push(digit - take);
            borrow = 0;
        } else {
            out.push(digit + 10 - take);
            borrow = 1;
        }
    }
    out.reverse();
    out
}

/// The order of two magnitudes given as digits without a leading zero.
fn compare_digits(left: &[u8], right: &[u8]) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

/// The quotient digits (one per dividend digit, leading zeros kept) and the remainder of a
/// magnitude divided by `divisor`.
fn long_division(dividend: &[u8], divisor: u64) -> (Vec<u8>, u64) {
    let divisor = u128::from(divisor);
    let mut quotient = Vec::with_capacity(dividend.len());
    let mut remainder = 0_u128;
    for digit in dividend {
        let current = remainder * 10 + u128::from(*digit);
        // `remainder < divisor`, so `current / divisor` is one digit.
        quotient.push(u8::try_from(current / divisor).unwrap_or(9));
        remainder = current % divisor;
    }
    (quotient, u64::try_from(remainder).unwrap_or(u64::MAX))
}
