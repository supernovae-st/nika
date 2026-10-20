// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `due` — the pure planner both firing edges read (`fire` at W2,
//! `serve` at W5): GIVEN the registry, the wall clock handed in (an L4
//! act — this crate never reads one, trap ①), and the firing state as
//! a CALLBACK, answer « what is due now? » and « what fires next? ».
//!
//! The firing state is the one thing this crate must never own (N2 —
//! no resume: the crate computes slots, never carries run state), so
//! `last_fired` arrives as a function over the beat index and the pure
//! code never opens a store.
//!
//! The law:
//!
//! - only ACTIVE, LOCAL beats are ever due — a suspended beat is told
//!   and bounded (`actif: false` carries `raison:` + `jusqu_au:`) and
//!   wakes after its `jusqu_au` in its cadence's zone, counting its
//!   silence from the wake (the slots it slept through are not missed),
//!   and a cloud beat's calendar stays the operator's;
//! - a beat is due when a slot should have fired in `(last_fired,
//!   now]` — strictly after the last fire (an already-fired slot never
//!   re-fires), at or before now (a slot AT now has just passed). The
//!   window is read over the FIRE set (`next_slots`), never the
//!   existing-civil set: the gap's advanced fire is a fire like any
//!   other, and a planner that skipped it would drop a missed slot
//!   into a hole;
//! - the KIND: `OnTime` when the due slot is the ONLY one of the
//!   silence and within [`ON_TIME_WINDOW`] of now — `Missed { slots }`
//!   otherwise, counting the whole silence, saturated at
//!   [`MISSED_SLOTS_CAP`];
//! - NEVER fired (no state) is the on-time window only: N2 — a beat
//!   starts from ZERO, and a planner without state invents no backlog.

use jiff::Zoned;

use crate::error::CadenceError;
use crate::next::{Slot, next_slots};
use crate::registry::{ArmRegistry, Beat, Cadence, Locus};
use crate::tick::{still_suspended, wake_instant};

/// The on-time grace: a slot fired within five minutes of its instant
/// is ON TIME, not missed. A `SignedDuration`, not a `Span`: the window
/// is absolute time (five minutes is five minutes, whatever the
/// calendar says) — and `Span`'s builders are not `const` in jiff 0.2.
pub const ON_TIME_WINDOW: jiff::SignedDuration = jiff::SignedDuration::from_mins(5);

/// The missed-slot count saturates here: a silence longer than ten
/// thousand slots is an OUTAGE, and the exact figure teaches nothing
/// the cap does not (the `tolérance:` (m,k)-firm law's own read:
/// « beyond, it's an outage, not a skip »).
pub const MISSED_SLOTS_CAP: usize = 10_000;

/// What « due » means for one beat.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueKind {
    /// The slot is the silence's only one and within
    /// [`ON_TIME_WINDOW`] of now — fire it now.
    OnTime,
    /// The silence outgrew the window — the `manqué:` policy governs
    /// what happens; `slots` counts it (the due slot included),
    /// saturated at [`MISSED_SLOTS_CAP`].
    Missed {
        /// How many slots the silence covers, capped.
        slots: u32,
    },
}

/// One beat due: its index in the registry, the beat, the slot that
/// fell due, and the kind.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct Due<'a> {
    /// The beat's position in the registry (the firing state keys on it).
    pub index: usize,
    /// The beat itself.
    pub beat: &'a Beat,
    /// The most recent slot of the silence — the one to fire.
    pub slot: Slot,
    /// On time or missed (and how many).
    pub kind: DueKind,
    /// The first slot of the silence for a missed beat (`None` on time): the
    /// silence runs from it to [`Self::slot`].
    pub first: Option<Slot>,
}

/// Active · local beats whose previous slot falls in `(last_fired,
/// now]`, in registry order. `last_fired(index)` comes from the firing
/// state — the pure code never reads it. (FCI-014: an iterator, not a
/// Vec — the walk itself is eager so a broken cadence refuses the whole
/// plan instead of surfacing mid-iteration.)
///
/// # Errors
/// A [`CadenceError`] when a beat's cadence expression does not parse —
/// a registry that bypassed `validate` refuses the whole plan rather
/// than skipping one beat in silence.
pub fn due<'a>(
    reg: &'a ArmRegistry,
    now: &Zoned,
    last_fired: &dyn Fn(usize) -> Option<Zoned>,
) -> Result<impl Iterator<Item = Due<'a>>, CadenceError> {
    let mut out = Vec::new();
    for (index, beat) in reg.beats().enumerate() {
        if still_suspended(beat, now) || beat.locus() != Locus::Local {
            continue;
        }
        let cadence = Cadence::parse(&beat.cadence)?;
        match since(last_fired(index), beat) {
            None => {
                // N2: no state, no invented backlog — the on-time
                // window alone decides.
                let Some(slot) = cadence.prev_before(now) else {
                    continue;
                };
                if within_window(now, &slot) {
                    out.push(Due {
                        index,
                        beat,
                        slot,
                        kind: DueKind::OnTime,
                        first: None,
                    });
                }
            }
            Some(fired) => {
                let Some((first, slot, slots)) = silence(&cadence, &fired, now) else {
                    continue;
                };
                let on_time = slots == 1 && within_window(now, &slot);
                let kind = if on_time {
                    DueKind::OnTime
                } else {
                    DueKind::Missed { slots }
                };
                out.push(Due {
                    index,
                    beat,
                    slot,
                    kind,
                    first: (!on_time).then_some(first),
                });
            }
        }
    }
    Ok(out.into_iter())
}

/// The earliest next slot among active · local beats — `None` means
/// nothing armed (or only webhooks, which carry no calendar). The
/// firing edge sleeps until then, never longer (trap ②: the caller
/// sleeps, never the calculator).
///
/// # Errors
/// As [`due`]: a cadence that will not parse refuses the whole answer.
pub fn earliest_next(
    reg: &ArmRegistry,
    now: &Zoned,
) -> Result<Option<(usize, Slot)>, CadenceError> {
    let mut best: Option<(usize, Slot)> = None;
    for (index, beat) in reg.beats().enumerate() {
        if beat.locus() != Locus::Local {
            continue;
        }
        // A suspension still asleep offers its first slot after the wake:
        // the edge never sleeps through it.
        let from = if still_suspended(beat, now) {
            let Some(before_wake) = wake_instant(beat)
                .and_then(|wake| wake.checked_sub(jiff::SignedDuration::from_nanos(1)).ok())
            else {
                continue;
            };
            before_wake
        } else {
            now.clone()
        };
        let cadence = Cadence::parse(&beat.cadence)?;
        let Some(slot) = cadence.next_after(&from) else {
            continue;
        };
        if best.as_ref().is_none_or(|(_, held)| slot.at < held.at) {
            best = Some((index, slot));
        }
    }
    Ok(best)
}

/// Where a beat's silence starts: its last fire, or for a woken suspension
/// the wake when that is later, so the slots a suspension slept through are
/// never counted as missed. A beat that never fired has no silence, woken or
/// not (N2: the on-time window alone decides). The silence is read strictly
/// after its start, so a woken beat's starts just before the wake: a slot AT
/// the wake instant is owed.
fn since(last_fired: Option<Zoned>, beat: &Beat) -> Option<Zoned> {
    let woke = wake_instant(beat)
        .and_then(|wake| wake.checked_sub(jiff::SignedDuration::from_nanos(1)).ok());
    match (last_fired, woke) {
        (Some(fired), Some(woke)) if woke.timestamp() > fired.timestamp() => Some(woke),
        (fired, _) => fired,
    }
}

/// The silence between the last fire and now, over the FIRE set: the
/// most recent slot owed (the one to fire) and how many the silence
/// covers, saturated at [`MISSED_SLOTS_CAP`]. `None` — not due — when
/// no slot falls in `(fired, now]` (already fired · a state ahead of
/// the clock).
fn silence(cadence: &Cadence, fired: &Zoned, now: &Zoned) -> Option<(Slot, Slot, u32)> {
    let mut slots = next_slots(cadence, fired, MISSED_SLOTS_CAP).take_while(|s| s.at <= *now);
    let first = slots.next()?;
    let mut last = first.clone();
    let mut count = 1u32;
    for slot in slots {
        last = slot;
        count += 1;
    }
    Some((first, last, count))
}

/// Within the on-time grace: the slot's instant is at most
/// [`ON_TIME_WINDOW`] older than `now` (a slot AT now has age zero —
/// it has just passed). Read on epoch seconds: both instants are
/// absolute, whatever zone they ride.
fn within_window(now: &Zoned, slot: &Slot) -> bool {
    let age = now.timestamp().as_second() - slot.at.timestamp().as_second();
    (0..=ON_TIME_WINDOW.as_secs()).contains(&age)
}
