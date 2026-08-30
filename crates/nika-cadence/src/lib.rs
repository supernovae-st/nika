// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-cadence` — L0 · PUR · zéro I/O · zéro async.
//!
//! The grammar of the arming registry (the `arm:` block of `nika.yaml`,
//! D-2026-08-10-N3), the pure slot calculator (`next_after` ·
//! `prev_before` — the half-open interval `(prev, next]` a beat is due
//! in), the pure planner (`due` · `earliest_next`) the firing edges
//! read, the firing + ledger state machines, and the pure OS-unit renderer
//! (`emit` — W3, « LE PONT ») the `--emit` verb and `serve` read. Filesystem
//! discovery, locks, fsync, and rotation stay at the L4 adapter. Two L4
//! consumers read this registry
//! (`nika arm` today · `nika serve` at ②), so the shared logic lives
//! at L0 — never in a CLI crate (the layering precedent: `nika-check`'s
//! Cargo.toml · "THREE L0 consumers make any higher layer an upward-dep
//! violation").
//!
//! The four locks (D-2026-08-11-N1→N4 · one law at four moments: THE
//! FILE PROPOSES, THE MACHINE DISPOSES):
//!
//! - **N1 · DST** — a slot that does not exist fires at the FIRST VALID
//!   instant (02:00 absent ⇒ 03:00) · a doubled slot fires ONCE, at its
//!   first occurrence. Written policy, never a guess.
//! - **N2 · no resume** — a beat starts from ZERO; every tick is a new
//!   run. The pure fold describes evidence; it never resumes or executes.
//! - **N3 · identity** — the MACHINE's key authorizes; `par:` DECLARES
//!   the human and proves nothing (a merge arms nothing).
//! - **N4 · absence** — removing a line does NOT disarm; that gesture
//!   is `arm --disarm`, an L4 act this crate knows nothing about.
//!
//! The grammar laws (all validated at parse, every refusal named and
//! teaching its fix):
//!
//! - `manqué:` and `plafond:` are REQUIRED, no default — choosing for
//!   the operator is choosing who pays.
//! - The timezone lives INSIDE the cadence expression
//!   (`TZ=Europe/Paris 0 9 * * 1` · scar #1 closed by the FORM) —
//!   a TZ-less cadence is a named refusal. Only `on-webhook` goes
//!   zoneless.
//! - The cron field count is validated BEFORE any field semantics —
//!   a 6-field expression dies on the count (scar #6), which is why
//!   the cron parser is hand-counted here and zero cron library is
//!   linked.
//! - Both cadence FORMS parse (cron and readable `lundi 9h07`) and
//!   display normalizes to the readable one. The weekday origin is
//!   NAMED: `0` and `7` are dimanche/Sunday.
//! - `dom`+`dow` restricted together is a refusal (the Vixie OR trap).
//! - Safe values sit in the DEFAULTS (law ⑥): `où: local` ·
//!   `chevauchement: sauter` · `après_saut: prochain-créneau` — a slow
//!   beat never becomes a tight loop by default.
//! - `actif: false` requires `raison:` + `jusqu_au:` — a suspension
//!   without an expiry is a forgotten one.
//! - Round 1 refuses by name: `signature:` (verification is ②'s),
//!   `budget:` (waits for a measured lack), every unknown key.
//!
//! The three traps, as constraints (§2quinquies):
//!
//! ① This crate does NOT depend on the kernel `Clock` (that trait has
//!   no civil surface). The calculator takes a `jiff::Zoned`; the clock
//!   lives at the L4 edge. Determinism becomes trivial: tests use
//!   literal instants, no clock to drive.
//! ② The calculator NEVER sleeps (a `VirtualClock::sleep` does not
//!   advance time — a sleeping loop would spin forever under the mode
//!   meant to prove it). The caller sleeps, never the calculator.
//! ③ `jiff`'s `TimeZone::get` is FORBIDDEN here (it prefers the host's
//!   `/usr/share/zoneinfo` — a hermeticity hole). Zones resolve from
//!   the EMBEDDED tzdb only: `jiff_tzdb::get` + `TimeZone::tzif`.

#![forbid(unsafe_code)]

pub mod cron;
pub mod due;
pub mod emit;
pub mod error;
pub mod firing;
pub mod ledger;
pub mod next;
pub mod parse;
pub mod phrase;
pub mod registry;
pub mod schedule;
pub mod schedule_plan;
pub mod tick;

pub use cron::{CronSpec, Field};
pub use due::{Due, DueKind, MISSED_SLOTS_CAP, ON_TIME_WINDOW, due, earliest_next};
pub use error::{CadenceError, CadenceErrorKind};
pub use firing::{
    ArmGeneration, Decision, FencingToken, FiringEvent, FiringPolicy, FiringState, SkipReason,
    SlotId, decide, fold, transition,
};
pub use ledger::{
    Claim, DecisionKind, ExecutionLink, HistoryEntry, LastRecord, RecordOutcome, Unsettled,
};
pub use next::{Shift, Slot, next_slots};
pub use parse::{parse_registry, validate};
pub use registry::{AfterSkip, ArmRegistry, Beat, Cadence, Locus, MissPolicy, Overlap};
pub use schedule::{
    ScheduleDefinition, ScheduleDraft, ScheduleFinding, ScheduleFindingKind, ScheduleJitter,
    ScheduleOrigin, ScheduleRevision, ScheduleWhen, ScheduleWhenDraft,
};
pub use schedule_plan::{
    MAX_SCHEDULE_PROJECTION_SLOTS, ScheduleDecisionState, ScheduleDueVerdict, ScheduleLastSlot,
    SchedulePlan, SchedulePlanError, ScheduleProjection, ScheduleSlot, plan_schedule,
};
pub use tick::{ScheduleDecision, TickDecision, tick_decision, v0_unsupported};

#[cfg(test)]
mod tests;
