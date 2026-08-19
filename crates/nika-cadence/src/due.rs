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
//!   and bounded (`actif: false` carries `raison:` + `jusqu_au:`), and
//!   a cloud beat's calendar stays the operator's;
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
        if !beat.is_active() || beat.locus() != Locus::Local {
            continue;
        }
        let cadence = Cadence::parse(&beat.cadence)?;
        match last_fired(index) {
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
                    });
                }
            }
            Some(fired) => {
                let Some((slot, slots)) = silence(&cadence, &fired, now) else {
                    continue;
                };
                let kind = if slots == 1 && within_window(now, &slot) {
                    DueKind::OnTime
                } else {
                    DueKind::Missed { slots }
                };
                out.push(Due {
                    index,
                    beat,
                    slot,
                    kind,
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
        if !beat.is_active() || beat.locus() != Locus::Local {
            continue;
        }
        let cadence = Cadence::parse(&beat.cadence)?;
        let Some(slot) = cadence.next_after(now) else {
            continue;
        };
        if best.as_ref().is_none_or(|(_, held)| slot.at < held.at) {
            best = Some((index, slot));
        }
    }
    Ok(best)
}

/// The silence between the last fire and now, over the FIRE set: the
/// most recent slot owed (the one to fire) and how many the silence
/// covers, saturated at [`MISSED_SLOTS_CAP`]. `None` — not due — when
/// no slot falls in `(fired, now]` (already fired · a state ahead of
/// the clock).
fn silence(cadence: &Cadence, fired: &Zoned, now: &Zoned) -> Option<(Slot, u32)> {
    let mut slots = next_slots(cadence, fired, MISSED_SLOTS_CAP).take_while(|s| s.at <= *now);
    let mut last = slots.next()?;
    let mut count = 1u32;
    for slot in slots {
        last = slot;
        count += 1;
    }
    Some((last, count))
}

/// Within the on-time grace: the slot's instant is at most
/// [`ON_TIME_WINDOW`] older than `now` (a slot AT now has age zero —
/// it has just passed). Read on epoch seconds: both instants are
/// absolute, whatever zone they ride.
fn within_window(now: &Zoned, slot: &Slot) -> bool {
    let age = now.timestamp().as_second() - slot.at.timestamp().as_second();
    (0..=ON_TIME_WINDOW.as_secs()).contains(&age)
}
