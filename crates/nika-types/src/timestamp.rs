// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Wall-clock + duration value types — RFC 3339 + nanosecond-resolution.
//!
//! Locked by Q9 (2026-04-16 swarm-2 L0 architecture decision):
//!
//! * `Timestamp(i64 unix_ns)` — signed 64-bit nanoseconds since 1970-01-01 UTC.
//!   Range: ±292 years around epoch (fine for everything we ship through v0.100).
//!   Wire-format: RFC 3339 Display; i64 ns internally; signed so Debug of a
//!   corrupted value shows the sign instead of a `u64` overflow surprise.
//!
//! * `WallDuration(i64 nanos)` — signed 64-bit nanoseconds.
//!   Signed so subtraction (`end - start`) yields a `WallDuration` directly.
//!   Kept distinct from `std::time::Duration` because Duration is unsigned
//!   + has weird `From<Duration>` conversions that erase our invariants.
//!
//! These replace the ad-hoc `_ms: u64` / `_us: u64` fields sprinkled across
//! `Event`, `InferResponse::ttft`, `ToolExecError::Timeout`, `SpanEvent::duration` —
//! the retrofit lands incrementally; this module ships the reserved types
//! first (Wave 4B-#3, ADR seed). Accessors on the old fields stay for one
//! minor version per ADR-010 deprecation policy.

use core::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Nanoseconds per millisecond.
pub const NS_PER_MS: i64 = 1_000_000;
/// Nanoseconds per microsecond.
pub const NS_PER_US: i64 = 1_000;
/// Nanoseconds per second.
pub const NS_PER_SEC: i64 = 1_000_000_000;

// ─── Timestamp ──────────────────────────────────────────────────────

/// UTC wall-clock instant, signed nanoseconds since Unix epoch.
///
/// Signed because negative instants (pre-1970) stay representable without
/// a panic; a corrupted timestamp displays as `-9223372036…` rather than
/// wrapping to a giant positive number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[non_exhaustive]
pub struct Timestamp {
    /// Signed nanoseconds since `1970-01-01T00:00:00Z`.
    pub unix_ns: i64,
}

impl Timestamp {
    /// The Unix epoch (`1970-01-01T00:00:00Z`).
    pub const EPOCH: Self = Self { unix_ns: 0 };

    /// Construct a timestamp from a raw signed-nanosecond counter.
    #[must_use]
    pub const fn from_unix_ns(unix_ns: i64) -> Self {
        Self { unix_ns }
    }

    /// Construct a timestamp from unsigned milliseconds since epoch.
    ///
    /// Saturates at `i64::MAX / NS_PER_MS` on overflow (the year 294247).
    #[must_use]
    pub const fn from_unix_ms(unix_ms: u64) -> Self {
        // Safe cast: u64 ms fits if < i64::MAX / NS_PER_MS; else saturate.
        let max_ms = (i64::MAX / NS_PER_MS) as u64;
        let capped = if unix_ms > max_ms { max_ms } else { unix_ms };
        #[allow(clippy::cast_possible_wrap)]
        // Safe: `capped` is ≤ max_ms which fits in i64 by construction.
        Self {
            unix_ns: (capped as i64) * NS_PER_MS,
        }
    }

    /// Signed milliseconds since epoch.
    ///
    /// Truncates toward zero for sub-millisecond precision.
    #[must_use]
    pub const fn unix_ms(self) -> i64 {
        self.unix_ns / NS_PER_MS
    }

    /// Signed microseconds since epoch.
    #[must_use]
    pub const fn unix_us(self) -> i64 {
        self.unix_ns / NS_PER_US
    }
}

impl fmt::Display for Timestamp {
    /// RFC 3339 rendering of the instant.
    ///
    /// Format: `YYYY-MM-DDTHH:MM:SS[.fff]Z`. We render to millisecond
    /// precision on wire (3-digit fractional seconds) — nanosecond-level
    /// precision stays available via [`Self::unix_ns`] for programmatic
    /// consumers that need it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We avoid a chrono dep in L0. The math below is purely integer
        // arithmetic against the proleptic Gregorian calendar, mirroring
        // Howard Hinnant's date-algorithms paper.
        let ns = self.unix_ns;
        let whole_ms = ns.div_euclid(NS_PER_MS);
        let sec = whole_ms.div_euclid(1000);
        let frac_ms = whole_ms.rem_euclid(1000);

        let days = sec.div_euclid(86_400);
        let mut secs_of_day = sec.rem_euclid(86_400);
        let hours = secs_of_day / 3600;
        secs_of_day %= 3600;
        let minutes = secs_of_day / 60;
        let seconds = secs_of_day % 60;

        let (year, month, day) = civil_from_days(days);

        write!(
            f,
            "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{frac_ms:03}Z"
        )
    }
}

/// Convert a signed day count since Unix epoch to (year, month, day).
///
/// Howard Hinnant's algorithm. `days` can be negative (pre-1970).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn civil_from_days(days: i64) -> (i64, u8, u8) {
    // Shift epoch 0000-03-01 (leap-year cycle boundary).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // 0..=146096
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // 0..=399
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // 0..=365
    let mp = (5 * doy + 2) / 153; // 0..=11
    let d = doy - (153 * mp + 2) / 5 + 1; // 1..=31
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // 1..=12
    let y = y + if m <= 2 { 1 } else { 0 };
    // Safe: m is 1..=12 and d is 1..=31 by construction.
    (y, m as u8, d as u8)
}

// ─── WallDuration ───────────────────────────────────────────────────

/// Signed nanosecond duration.
///
/// Distinct from `std::time::Duration` because we need:
///   * Signed arithmetic (a - b can be negative without panicking),
///   * `Copy` semantics at a fixed i64 size (std Duration is 96 bits),
///   * Serde-roundtrip stability (std Duration's serde shape isn't stable
///     under feature flags across the ecosystem).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[non_exhaustive]
pub struct WallDuration {
    /// Signed nanoseconds.
    pub nanos: i64,
}

impl WallDuration {
    /// Zero duration.
    pub const ZERO: Self = Self { nanos: 0 };

    /// From signed nanoseconds.
    #[must_use]
    pub const fn from_nanos(nanos: i64) -> Self {
        Self { nanos }
    }

    /// From unsigned microseconds. Saturates at `i64::MAX`.
    #[must_use]
    pub const fn from_micros(micros: u64) -> Self {
        let max = (i64::MAX / NS_PER_US) as u64;
        let capped = if micros > max { max } else { micros };
        #[allow(clippy::cast_possible_wrap)]
        Self {
            nanos: (capped as i64) * NS_PER_US,
        }
    }

    /// From unsigned milliseconds. Saturates at `i64::MAX`.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        let max = (i64::MAX / NS_PER_MS) as u64;
        let capped = if millis > max { max } else { millis };
        #[allow(clippy::cast_possible_wrap)]
        Self {
            nanos: (capped as i64) * NS_PER_MS,
        }
    }

    /// From unsigned whole seconds. Saturates at `i64::MAX`.
    #[must_use]
    pub const fn from_secs(secs: u64) -> Self {
        let max = (i64::MAX / NS_PER_SEC) as u64;
        let capped = if secs > max { max } else { secs };
        #[allow(clippy::cast_possible_wrap)]
        Self {
            nanos: (capped as i64) * NS_PER_SEC,
        }
    }

    /// Total nanoseconds (signed).
    #[must_use]
    pub const fn as_nanos(self) -> i64 {
        self.nanos
    }

    /// Total microseconds (signed, truncated).
    #[must_use]
    pub const fn as_micros(self) -> i64 {
        self.nanos / NS_PER_US
    }

    /// Total milliseconds (signed, truncated).
    #[must_use]
    pub const fn as_millis(self) -> i64 {
        self.nanos / NS_PER_MS
    }

    /// Total whole seconds (signed, truncated).
    #[must_use]
    pub const fn as_secs(self) -> i64 {
        self.nanos / NS_PER_SEC
    }
}

impl fmt::Display for WallDuration {
    /// Human-readable rendering: `"{n}ms"` when ≥ 1ms, `"{n}µs"` when
    /// ≥ 1µs, otherwise `"{n}ns"`. Debug-friendly; machine consumers
    /// should serialise the raw nanos.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let abs = self.nanos.unsigned_abs();
        if abs == 0 {
            return f.write_str("0ns");
        }
        let sign = if self.nanos.is_negative() { "-" } else { "" };
        if abs >= NS_PER_MS as u64 {
            // Derive BOTH the whole and the fractional part from `abs` so the sign
            // is carried once by `sign`. The earlier `rem_euclid` of the SIGNED
            // nanos paired a euclidean (always-positive) remainder with an abs-based
            // whole part, which mis-rendered negative sub-ms remainders
            // (e.g. -1_999_999ns showed "-1.000001ms" instead of "-1.999999ms").
            let whole = abs / NS_PER_MS as u64;
            let frac = abs % NS_PER_MS as u64;
            if frac == 0 {
                write!(f, "{sign}{whole}ms")
            } else {
                write!(f, "{sign}{whole}.{frac:06}ms")
            }
        } else if abs >= NS_PER_US as u64 {
            let us = self.nanos / NS_PER_US;
            write!(f, "{us}us")
        } else {
            write!(f, "{}ns", self.nanos)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_epoch_is_zero_ns() {
        assert_eq!(Timestamp::EPOCH.unix_ns, 0);
    }

    #[test]
    fn timestamp_from_unix_ms_round_trip_via_unix_ms() {
        let t = Timestamp::from_unix_ms(1_700_000_000_000);
        assert_eq!(t.unix_ms(), 1_700_000_000_000);
    }

    #[test]
    fn timestamp_display_epoch() {
        assert_eq!(Timestamp::EPOCH.to_string(), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn timestamp_display_2026_04_17_noon() {
        // 20,560 days from 1970-01-01 to 2026-04-17 (verified by algorithm).
        let ts = Timestamp::from_unix_ms(20_560 * 86_400_000 + 12 * 3_600_000);
        assert_eq!(ts.to_string(), "2026-04-17T12:00:00.000Z");
    }

    #[test]
    fn timestamp_display_shows_fractional_ms() {
        let ts = Timestamp::from_unix_ms(1_234);
        assert_eq!(ts.to_string(), "1970-01-01T00:00:01.234Z");
    }

    #[test]
    fn timestamp_unix_us_truncates_sub_micro() {
        let ts = Timestamp::from_unix_ns(1234); // 1.234µs
        assert_eq!(ts.unix_us(), 1);
    }

    #[test]
    fn wall_duration_zero_display() {
        assert_eq!(WallDuration::ZERO.to_string(), "0ns");
    }

    #[test]
    fn wall_duration_from_millis_as_millis_roundtrip() {
        let d = WallDuration::from_millis(2_500);
        assert_eq!(d.as_millis(), 2_500);
        assert_eq!(d.as_nanos(), 2_500 * NS_PER_MS);
    }

    #[test]
    fn wall_duration_display_ms_bucket() {
        let d = WallDuration::from_millis(42);
        assert_eq!(d.to_string(), "42ms");
    }

    #[test]
    fn wall_duration_display_us_bucket() {
        let d = WallDuration::from_micros(750);
        assert_eq!(d.to_string(), "750us");
    }

    #[test]
    fn wall_duration_display_negative_sub_ms_remainder() {
        // Regression: a negative duration with a non-zero sub-ms remainder must
        // render the true magnitude. -1_999_999ns is -1.999999ms, NOT "-1.000001ms"
        // (the bug when the fractional part came from rem_euclid of the signed nanos).
        assert_eq!(
            WallDuration::from_nanos(-1_999_999).to_string(),
            "-1.999999ms"
        );
        // Positive counterpart is unchanged.
        assert_eq!(
            WallDuration::from_nanos(1_999_999).to_string(),
            "1.999999ms"
        );
        // Exact negative milliseconds carry the sign with no fractional part.
        assert_eq!(WallDuration::from_nanos(-2_000_000).to_string(), "-2ms");
        // Smallest negative fractional remainder (1ns over 1ms).
        assert_eq!(
            WallDuration::from_nanos(-1_000_001).to_string(),
            "-1.000001ms"
        );
    }

    #[test]
    fn wall_duration_display_ns_bucket() {
        let d = WallDuration::from_nanos(42);
        assert_eq!(d.to_string(), "42ns");
    }

    #[test]
    fn wall_duration_from_secs() {
        let d = WallDuration::from_secs(3);
        assert_eq!(d.as_nanos(), 3 * NS_PER_SEC);
        assert_eq!(d.as_millis(), 3000);
        assert_eq!(d.as_secs(), 3);
    }

    #[test]
    fn wall_duration_saturation_preserves_i64_range() {
        // from_secs(u64::MAX) must not overflow.
        let d = WallDuration::from_secs(u64::MAX);
        assert!(d.as_nanos() > 0);
        assert!(d.as_secs() > 0);
    }

    #[test]
    fn timestamp_ordering_is_chronological() {
        let a = Timestamp::from_unix_ns(100);
        let b = Timestamp::from_unix_ns(200);
        assert!(a < b);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn timestamp_serde_transparent_roundtrip() {
        let t = Timestamp::from_unix_ns(1_700_000_000_123_456_789);
        let json = serde_json::to_string(&t).expect("ser");
        // transparent: emits as the inner i64 directly.
        assert_eq!(json, "1700000000123456789");
        let back: Timestamp = serde_json::from_str(&json).expect("de");
        assert_eq!(back, t);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn wall_duration_serde_transparent_roundtrip() {
        let d = WallDuration::from_nanos(42_424_242);
        let json = serde_json::to_string(&d).expect("ser");
        assert_eq!(json, "42424242");
        let back: WallDuration = serde_json::from_str(&json).expect("de");
        assert_eq!(back, d);
    }
}
