// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `Cost` newtype — nano-USD precision for LLM billing.
//!
//! 1 nano-USD = 1e-9 USD. An i128 gives ~170 quintillion USD range
//! with nano-USD precision, sufficient for any realistic billing scenario.
//!
//! ## Why not f64?
//! - f64 cannot represent 0.1 exactly.
//! - 10M inference calls at $0.0001 = drift by cents.
//! - Stripe uses i64 micro-units; `OpenAI` uses Decimal server-side.
//! - Migration from f64 after 1M runs in `SQLite` = pain.

use core::fmt;
use core::ops::{Add, Sub};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Cost in nano-USD (1e-9 USD). Signed to support credits/refunds.
///
/// # Display
///
/// Formats as `"$X.XXXXXX"` (6 decimal places, micro-USD resolution).
///
/// ```
/// use nika_types::cost::Cost;
/// let c = Cost::from_micro_usd(1_500_000); // $1.50
/// assert_eq!(c.to_string(), "$1.500000");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct Cost {
    /// Value in nano-USD.
    pub nano_usd: i128,
}

impl Cost {
    /// Create a cost from nano-USD.
    #[must_use]
    pub const fn new(nano_usd: i128) -> Self {
        Self { nano_usd }
    }

    /// Zero cost.
    #[must_use]
    pub const fn zero() -> Self {
        Self { nano_usd: 0 }
    }

    /// Create from micro-USD (1e-6 USD). Convenience for typical API pricing.
    #[must_use]
    pub fn from_micro_usd(micro: i64) -> Self {
        Self {
            nano_usd: i128::from(micro) * 1_000,
        }
    }

    /// Create from milli-USD (1e-3 USD).
    #[must_use]
    pub fn from_milli_usd(milli: i64) -> Self {
        Self {
            nano_usd: i128::from(milli) * 1_000_000,
        }
    }

    /// Whether this cost is zero.
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.nano_usd == 0
    }

    /// Multiply by a scalar (e.g., token count). Saturating.
    ///
    /// Kept saturating because the scale factor is always non-negative
    /// (token count) — overflow here means we're pricing more tokens than
    /// the universe can hold, so clamping to `i128::MAX` is safe.
    #[must_use]
    pub fn saturating_mul(self, factor: u64) -> Self {
        Self {
            nano_usd: self.nano_usd.saturating_mul(i128::from(factor)),
        }
    }

    /// Checked addition. Returns `None` on overflow.
    ///
    /// Prefer this over `+` in billing aggregation where overflow must be
    /// observable (ledger reconciliation, invoice generation).
    #[must_use]
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.nano_usd
            .checked_add(rhs.nano_usd)
            .map(|nano_usd| Self { nano_usd })
    }

    /// Checked subtraction. Returns `None` on underflow.
    ///
    /// Prefer this over `-` in billing aggregation where underflow must be
    /// observable (credit balance, refund reconciliation).
    #[must_use]
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.nano_usd
            .checked_sub(rhs.nano_usd)
            .map(|nano_usd| Self { nano_usd })
    }

    /// Lossy conversion to `f64` USD — for display and the JSON `cost_usd`
    /// event field the CLI folds into its spend meter, where a float is
    /// expected. Never for billing math.
    ///
    /// Precision: f64 rounds at ~15 significant decimal digits, so the
    /// round-trip `Cost → f64 → Cost` is lossy below ~15 USD for deeply
    /// sub-cent values. Always prefer reading `Cost` directly for billing.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn to_usd_f64(self) -> f64 {
        (self.nano_usd as f64) / 1e9
    }
}

/// `+` follows stdlib `i128` semantics (panic in debug, wrap in release).
///
/// For billing aggregation where overflow must be observable, use
/// [`Cost::checked_add`] instead.
impl Add for Cost {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            nano_usd: self.nano_usd + rhs.nano_usd,
        }
    }
}

/// `-` follows stdlib `i128` semantics (panic in debug, wrap in release).
///
/// For billing aggregation where underflow must be observable, use
/// [`Cost::checked_sub`] instead.
impl Sub for Cost {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            nano_usd: self.nano_usd - rhs.nano_usd,
        }
    }
}

impl fmt::Display for Cost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.nano_usd < 0 { "-" } else { "" };
        let abs = self.nano_usd.unsigned_abs();
        let dollars = abs / 1_000_000_000;
        let frac = (abs % 1_000_000_000) / 1_000; // micro-USD resolution
        write!(f, "{sign}${dollars}.{frac:06}")
    }
}

impl Default for Cost {
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_cost() {
        let c = Cost::zero();
        assert!(c.is_zero());
        assert_eq!(c.nano_usd, 0);
        // A non-zero cost must report is_zero() == false (kills `is_zero -> true`).
        assert!(!Cost::new(1).is_zero());
        assert!(!Cost::from_micro_usd(-1).is_zero());
    }

    #[test]
    fn from_micro_usd() {
        let c = Cost::from_micro_usd(1_500_000); // $1.50
        assert_eq!(c.nano_usd, 1_500_000_000);
        assert_eq!(c.to_string(), "$1.500000");
    }

    #[test]
    fn from_milli_usd() {
        let c = Cost::from_milli_usd(2500); // $2.50
        assert_eq!(c.nano_usd, 2_500_000_000);
        assert_eq!(c.to_string(), "$2.500000");
    }

    #[test]
    fn display_zero() {
        assert_eq!(Cost::zero().to_string(), "$0.000000");
    }

    #[test]
    fn display_negative() {
        let c = Cost::new(-500_000_000); // -$0.50
        assert_eq!(c.to_string(), "-$0.500000");
    }

    #[test]
    fn display_sub_penny() {
        let c = Cost::from_micro_usd(50); // $0.000050
        assert_eq!(c.to_string(), "$0.000050");
    }

    #[test]
    fn add_costs() {
        let a = Cost::from_micro_usd(100);
        let b = Cost::from_micro_usd(200);
        assert_eq!((a + b).nano_usd, 300_000);
    }

    #[test]
    fn sub_costs() {
        let a = Cost::from_micro_usd(300);
        let b = Cost::from_micro_usd(100);
        assert_eq!((a - b).nano_usd, 200_000);
    }

    #[test]
    fn saturating_mul() {
        let per_token = Cost::new(50); // 50 nano-USD per token
        let total = per_token.saturating_mul(1000);
        assert_eq!(total.nano_usd, 50_000);
    }

    #[test]
    fn saturating_mul_overflow() {
        let big = Cost::new(i128::MAX / 2);
        let result = big.saturating_mul(u64::MAX);
        assert_eq!(result.nano_usd, i128::MAX); // saturated, not panicked
    }

    #[test]
    fn checked_add_overflow_returns_none() {
        let a = Cost::new(i128::MAX);
        let b = Cost::new(1);
        assert_eq!(a.checked_add(b), None);
    }

    #[test]
    fn checked_add_in_range_returns_some() {
        let a = Cost::from_micro_usd(100);
        let b = Cost::from_micro_usd(200);
        assert_eq!(a.checked_add(b), Some(Cost::from_micro_usd(300)));
    }

    #[test]
    fn checked_sub_underflow_returns_none() {
        let a = Cost::new(i128::MIN);
        let b = Cost::new(1);
        assert_eq!(a.checked_sub(b), None);
    }

    #[test]
    fn checked_sub_in_range_returns_some() {
        let a = Cost::from_micro_usd(300);
        let b = Cost::from_micro_usd(100);
        assert_eq!(a.checked_sub(b), Some(Cost::from_micro_usd(200)));
    }

    #[test]
    fn to_usd_f64_roundtrips_near_cent_scale() {
        let c = Cost::from_micro_usd(1_500_000); // $1.50
        let f = c.to_usd_f64();
        assert!((f - 1.5).abs() < 1e-9);
    }

    #[test]
    fn to_usd_f64_handles_negative() {
        let c = Cost::new(-500_000_000); // -$0.50
        assert!((c.to_usd_f64() + 0.5).abs() < 1e-9);
    }

    #[test]
    fn to_usd_f64_zero() {
        assert!(Cost::zero().to_usd_f64().abs() < f64::EPSILON);
    }

    #[test]
    fn ordering() {
        let small = Cost::from_micro_usd(100);
        let large = Cost::from_micro_usd(200);
        assert!(small < large);
    }

    #[test]
    fn serde_roundtrip() {
        let c = Cost::from_micro_usd(42_000);
        let json = serde_json::to_string(&c).expect("serialize");
        let back: Cost = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(c, back);
    }

    #[test]
    fn default_is_zero() {
        assert!(Cost::default().is_zero());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn cost_is_send_sync() {
        _assert_send_sync::<Cost>();
    }
}
