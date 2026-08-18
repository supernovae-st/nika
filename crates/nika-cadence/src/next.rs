// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The next-slot calculator — pure, clock-free, zero I/O, and every
//! displacement DECLARED at the type level.
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
//! occurrence. Since the slot-merge correction: the displacement rides
//! the TYPE ([`Slot::shift`]) — for an engine whose thesis is « refusal
//! is declared », a slot that moved (or two that merged) is declared,
//! never lost in silence (before, `0,30 2,3 * * *` on gap day fired
//! twice instead of four times and no signature could say why).

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

/// What the calendar did to a slot — the N1 law, declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Shift {
    /// The civil slot exists exactly once — nothing happened.
    Exact,
    /// The civil slot did not exist (a spring gap) — the beat fires at
    /// the FIRST VALID instant instead (`02:30 → 03:00`, N1 · AVANCER).
    AdvancedFirstValid,
    /// The civil slot exists twice (an autumn fold) — the beat fires at
    /// its FIRST occurrence only (N1 · PREMIÈRE OCCURRENCE).
    FoldedFirst,
}

/// One computed slot: the instant, the civil slot that asked for it,
/// and the calendar's verdict. The merge visibility lives here — a
/// `check` display can now say « le 29 mars ce beat tire 2 fois au
/// lieu de 4 » from the types alone.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Slot {
    /// The instant the beat fires.
    pub at: jiff::Zoned,
    /// The civil slot (in the beat's zone) that produced it.
    pub civil: DateTime,
    /// What the calendar did to this slot (N1, declared).
    pub shift: Shift,
}

/// Resolve a civil slot to an instant, applying N1 — and SAY which arm
/// applied. The tzdb itself classifies (no minute-stepping):
///
/// - unambiguous → [`Shift::Exact`];
/// - in a fold → `earlier()` maps back to the same civil time: that IS
///   the first occurrence — [`Shift::FoldedFirst`];
/// - in a gap → the civil time exists nowhere, so `later()` is the first
///   valid instant (02:30 in a one-hour gap ⇒ 03:00, never jiff's
///   03:30 roll-forward) — [`Shift::AdvancedFirstValid`].
///
/// `None` only when the tzdb itself cannot disambiguate (never in
/// practice — a fold always has two sides and a gap is bounded).
fn resolve(civil: DateTime, tz: &TimeZone) -> Option<Slot> {
    let amb = tz.to_ambiguous_zoned(civil);
    if !amb.is_ambiguous() {
        return Some(Slot {
            at: amb.compatible().ok()?,
            civil,
            shift: Shift::Exact,
        });
    }
    // Ambiguous: a fold or a gap. In a FOLD, the earlier occurrence maps
    // back to the same civil time — it IS the first occurrence (N1). In
    // a GAP the civil time exists nowhere; jiff's `later()` rolls forward
    // by the gap LENGTH (02:30 ⇒ 03:30), which is NOT the law. N1 says
    // AVANCER to the FIRST VALID instant (02:30 ⇒ 03:00): step the civil
    // clock forward until it exists. Bounded at 26 h of minutes — the
    // widest gap the tzdb remembers is Apia 2011 (a whole DAY skipped,
    // 24 h), and the loop still lands (probed: 2011-12-30 → 12-31).
    let first = amb.earlier().ok()?;
    if first.datetime() == civil {
        return Some(Slot {
            at: first,
            civil,
            shift: Shift::FoldedFirst,
        });
    }
    let mut cursor = civil;
    for _ in 0..=(26 * 60) {
        cursor = cursor.checked_add(1.minute()).ok()?;
        let probe = tz.to_ambiguous_zoned(cursor);
        if !probe.is_ambiguous() {
            return Some(Slot {
                at: probe.compatible().ok()?,
                civil,
                shift: Shift::AdvancedFirstValid,
            });
        }
    }
    None
}

impl Cadence {
    /// The next slot strictly after `from` — `None` for a webhook (no
    /// calendar) or an unreachable schedule. The computation is pure:
    /// `from` carries the instant, the BEAT's zone carries the days.
    #[must_use]
    pub fn next_after(&self, from: &jiff::Zoned) -> Option<Slot> {
        match self {
            Self::Webhook => None,
            Self::Cron { tz, spec } => {
                let zone = bundled_tz(tz)?;
                spec.next_after(&zone, from)
            }
        }
    }

    /// The last slot at or before `from` — the mirror of
    /// [`next_after`](Self::next_after): strictly-after there,
    /// at-or-before here, so the two bound the half-open interval
    /// `(prev, next]` a beat is due in. `None` for a webhook (no
    /// calendar) or when no slot exists within the 366-day walk.
    ///
    /// The N1 law in mirror: a slot that never EXISTED (a spring gap) is
    /// never returned — it fired at the first valid instant AFTER, and
    /// `next_after` carries that one. A slot that exists twice (an
    /// autumn fold) fired at its FIRST occurrence — the mirror hands
    /// that one back, never the second.
    #[must_use]
    pub fn prev_before(&self, from: &jiff::Zoned) -> Option<Slot> {
        match self {
            Self::Webhook => None,
            Self::Cron { tz, spec } => {
                let zone = bundled_tz(tz)?;
                spec.prev_before(&zone, from)
            }
        }
    }
}

impl CronSpec {
    /// Day-by-day over a 3000-day horizon, hours and minutes nested inside
    /// matching days. The day walk happens in the BEAT's zone — a Monday
    /// slot is Monday in Paris, whatever zone `from` rides.
    ///
    /// The horizon was 1500 days, justified by "a 29 February is never
    /// further than 4 years". That is false (Gate 11, 2026-08-13): a century
    /// year is leap only when divisible by 400, so 2100 is not one, and from
    /// 2096-03-01 the next 29 February is 2104-02-29 — about 2920 days out.
    /// A `0 9 29 2 *` beat returned None there and died in silence. 3000 days
    /// clears the widest real gap the Gregorian rules can produce (8 years)
    /// with room to spare; the walk still ends rather than spinning, which is
    /// what the bound is for.
    pub(crate) fn next_after(&self, tz: &TimeZone, from: &jiff::Zoned) -> Option<Slot> {
        let from_local = from.with_time_zone(tz.clone());
        let mut day = from_local.date();
        for _ in 0..3000 {
            if self.covers(day) {
                for hour in self.hours().iter() {
                    for minute in self.minutes().iter() {
                        // Both conversions are unreachable by TYPE (a
                        // `Field<0, 23>` and a `Field<0, 59>` cannot exceed
                        // i8), and they used to answer `?` while the line
                        // below answered `continue`. Same failure shape, same
                        // expression, two different answers, which teaches
                        // the next reader the wrong one. A per-candidate
                        // failure skips the candidate; it never kills the
                        // beat. Named by a Rust review, 2026-08-13.
                        let (Ok(h), Ok(m)) = (i8::try_from(hour), i8::try_from(minute)) else {
                            continue;
                        };
                        let civil = day.to_datetime(jiff::civil::time(h, m, 0, 0));
                        // One irresolvable civil time must not silence the
                        // whole beat. This was `resolve(civil, tz)?`, which
                        // propagated the None straight out of next_after: a
                        // single unresolvable candidate killed every later
                        // slot, forever. Skip the candidate, keep the walk.
                        //
                        // No tzdb entry reaches this arm today — resolve only
                        // gives up after 26 h of minute-stepping, wider than
                        // any gap the tzdb remembers (Apia 2011, a whole day).
                        // So this is GUARDED, not tested: a non-attempt is not
                        // a proof, and claiming a test here would be the lie.
                        let Some(candidate) = resolve(civil, tz) else {
                            continue;
                        };
                        if &candidate.at > from {
                            return Some(candidate);
                        }
                    }
                }
            }
            day = day.tomorrow().ok()?;
        }
        None
    }

    /// Day-by-day BACKWARDS over a 366-day window — the recent past a
    /// planner asks about (« quel créneau vient de passer ? »), never an
    /// archaeology: a 29 February three years back answers `None`, and
    /// the walk still ends rather than spinning, which is what the bound
    /// is for. Hours and minutes descend inside matching days, in the
    /// BEAT's zone, so the first candidate at or before `from` IS the
    /// answer.
    pub(crate) fn prev_before(&self, tz: &TimeZone, from: &jiff::Zoned) -> Option<Slot> {
        let from_local = from.with_time_zone(tz.clone());
        let mut day = from_local.date();
        for _ in 0..366 {
            if self.covers(day) {
                for hour in self.hours().iter().rev() {
                    for minute in self.minutes().iter().rev() {
                        // Same per-candidate discipline as the forward
                        // walk: a failed conversion skips the candidate,
                        // it never kills the beat.
                        let (Ok(h), Ok(m)) = (i8::try_from(hour), i8::try_from(minute)) else {
                            continue;
                        };
                        let civil = day.to_datetime(jiff::civil::time(h, m, 0, 0));
                        let Some(candidate) = resolve(civil, tz) else {
                            continue;
                        };
                        // N1 in mirror: the ADVANCED slot never existed —
                        // it fired at the first valid instant AFTER, which
                        // `next_after` already carries. The mirror skips
                        // it rather than hand back a slot that was never
                        // a fire of its own.
                        if candidate.shift == Shift::AdvancedFirstValid {
                            continue;
                        }
                        if &candidate.at <= from {
                            return Some(candidate);
                        }
                    }
                }
            }
            day = day.yesterday().ok()?;
        }
        None
    }
}

/// The next `count` slots after `from` — "les 4 prochaines dates" the
/// display shows. An infinite-safe fold over [`Cadence::next_after`]
/// (each step is strictly later than the last), truncated at `count`;
/// a webhook or an unreachable schedule simply ends the walk early.
/// Each item is a [`Slot`] — a displaced or merged slot SAYS so
/// (`shift`), so the display can teach the DST law per date.
pub fn next_slots(
    cadence: &Cadence,
    from: &jiff::Zoned,
    count: usize,
) -> impl Iterator<Item = Slot> {
    let first = cadence.next_after(from);
    std::iter::successors(first, move |prev| cadence.next_after(&prev.at)).take(count)
}
