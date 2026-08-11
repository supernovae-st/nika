// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The next-slot calculator — pure, clock-free, zero I/O.
//!
//! Trap ① · the kernel `Clock` trait has no civil surface (an `Instant`
//! and a `SystemTime`, no zone, no day) — so this calculator takes a
//! `jiff::Zoned` and never sees a clock; the clock lives at the L4 edge.
//! Trap ② · it NEVER sleeps (a `VirtualClock::sleep` does not advance
//! time — a sleeping loop would spin forever under the mode meant to
//! prove it). The caller sleeps, never the calculator.
//! Trap ③ · zones resolve from the EMBEDDED tzdb only
//! (`jiff_tzdb::get` + `TimeZone::tzif`) — `TimeZone::get` is forbidden
//! here: it prefers the host's `/usr/share/zoneinfo`, and the same
//! workflow would fire at an hour set by the machine's tzdata version.
//!
//! N1 (D-2026-08-11-N1) · the DST law: a slot inside a spring gap fires
//! at the FIRST VALID instant (02:00 absent ⇒ 03:00 — a beat never
//! dies silently) · a slot in an autumn fold fires ONCE, at its first
//! occurrence.

use jiff::ToSpan;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;

use crate::cron::CronSpec;
use crate::registry::Cadence;

/// A zone from the EMBEDDED tzdb — the same hermeticity law as
/// `nika:date` (`bundled_tz` there): frozen at build, re-vendored each
/// release, zero system read.
pub(crate) fn bundled_tz(name: &str) -> Option<TimeZone> {
    let (canonical, tzif) = jiff_tzdb::get(name)?;
    TimeZone::tzif(canonical, tzif).ok()
}

/// Resolve a civil slot to an instant, applying N1:
///
/// - unambiguous → itself;
/// - in a gap → jiff's compatible disambiguation rolls FORWARD, which
///   we detect (`zoned.datetime() != civil`) and correct by stepping
///   the civil time minute by minute until it lands on a real instant —
///   the FIRST VALID one (02:30 in a one-hour gap ⇒ 03:00, not 03:30);
/// - in a fold → the compatible disambiguation already picks the FIRST
///   occurrence, which is exactly the law.
///
/// Bounded: no DST gap spans a day (the loop cap is 26 h of minutes,
/// never reached in practice — the widest known gaps are hours).
fn resolve(mut civil: DateTime, tz: &TimeZone) -> Option<jiff::Zoned> {
    for _ in 0..=(26 * 60) {
        let zoned = civil.to_zoned(tz.clone()).ok()?;
        if zoned.datetime() == civil {
            return Some(zoned);
        }
        civil = civil.checked_add(1.minute()).ok()?;
    }
    None
}

impl Cadence {
    /// The next slot strictly after `from` — `None` for a webhook (no
    /// calendar) or an unreachable schedule. The computation is pure:
    /// `from` carries the instant, the BEAT's zone carries the days.
    #[must_use]
    pub fn next_after(&self, from: &jiff::Zoned) -> Option<jiff::Zoned> {
        match self {
            Self::Webhook => None,
            Self::Cron { tz, spec } => {
                let zone = bundled_tz(tz)?;
                spec.next_after(&zone, from)
            }
        }
    }
}

impl CronSpec {
    /// Day-by-day over a 1500-day horizon (a 29 February is never
    /// further than 4 years), hours and minutes nested inside matching
    /// days. The day walk happens in the BEAT's zone — a Monday slot is
    /// Monday in Paris, whatever zone `from` rides.
    pub(crate) fn next_after(&self, tz: &TimeZone, from: &jiff::Zoned) -> Option<jiff::Zoned> {
        let from_local = from.with_time_zone(tz.clone());
        let mut day = from_local.date();
        for _ in 0..1500 {
            if self.covers(day) {
                for &hour in self.hours() {
                    for &minute in self.minutes() {
                        let civil = day.to_datetime(jiff::civil::time(
                            i8::try_from(hour).ok()?,
                            i8::try_from(minute).ok()?,
                            0,
                            0,
                        ));
                        let candidate = resolve(civil, tz)?;
                        if &candidate > from {
                            return Some(candidate);
                        }
                    }
                }
            }
            day = day.tomorrow().ok()?;
        }
        None
    }
}
