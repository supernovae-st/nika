// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure planning for canonical durable schedules.
//!
//! [`plan_schedule`] is the one authority used after apply, for status, and
//! before a resident claim. Its wall clock and durable predecessor are explicit
//! inputs. A returned wake instant is only a cache hint: callers must invoke
//! the planner again with the current definition, current wall time, and last
//! durable slot before claiming. This module performs no I/O, sleeping, clock
//! read, overlap execution, or `afterSkip` execution.

use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use nika_error::prelude::{NikaCode, NikaErrorCode, codes};
use serde_json::{Value, json};

use crate::due::{MISSED_SLOTS_CAP, ON_TIME_WINDOW};
use crate::firing::SlotId;
use crate::next::{Shift, Slot, next_slots};
use crate::registry::{AfterSkip, Cadence, MissPolicy, Overlap};
use crate::schedule::{ScheduleDefinition, ScheduleRevision, ScheduleWhen};

/// Maximum number of future slots materialized by one planner call.
pub const MAX_SCHEDULE_PROJECTION_SLOTS: usize = 64;

/// One canonical schedule slot, ready for an API/status response or durable
/// decision record.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScheduleSlot {
    id: SlotId,
    scheduled_for: Timestamp,
    requested_civil: Option<DateTime>,
    shift: Shift,
}

impl ScheduleSlot {
    /// Stable deduplication identity under the existing `nika/arm-slot@1` law.
    #[must_use]
    pub const fn id(&self) -> &SlotId {
        &self.id
    }

    /// Effective instant at which the slot belongs on the timeline.
    #[must_use]
    pub const fn scheduled_for(&self) -> Timestamp {
        self.scheduled_for
    }

    /// Requested civil time for a cadence slot; absent for an absolute `once`.
    #[must_use]
    pub const fn requested_civil(&self) -> Option<DateTime> {
        self.requested_civil
    }

    /// Exact, spring-gap-forward, or autumn-fold-first evidence from the
    /// existing cadence oracle.
    #[must_use]
    pub const fn shift(&self) -> Shift {
        self.shift
    }

    /// Copy the minimal state an adapter persists after its claim or skip is
    /// durable.
    #[must_use]
    pub fn durable_state(&self) -> ScheduleLastSlot {
        ScheduleLastSlot {
            id: self.id.clone(),
            scheduled_for: self.scheduled_for,
        }
    }

    fn once(definition: &ScheduleDefinition, at: Timestamp) -> Self {
        let zoned = at.to_zoned(TimeZone::UTC);
        Self {
            id: SlotId::derive(definition.workflow(), &once_key(at), &zoned),
            scheduled_for: at,
            requested_civil: None,
            shift: Shift::Exact,
        }
    }

    fn cadence(definition: &ScheduleDefinition, expression: &str, slot: &Slot) -> Self {
        Self {
            id: SlotId::derive(definition.workflow(), expression, &slot.at),
            scheduled_for: slot.at.timestamp(),
            requested_civil: Some(slot.civil),
            shift: slot.shift,
        }
    }
}

/// Minimal durable state of the last schedule decision that consumed a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScheduleLastSlot {
    id: SlotId,
    scheduled_for: Timestamp,
}

impl ScheduleLastSlot {
    /// Rebuild persisted slot state after its wire fields have been validated.
    #[must_use]
    pub const fn new(id: SlotId, scheduled_for: Timestamp) -> Self {
        Self { id, scheduled_for }
    }

    /// Stable slot identity.
    #[must_use]
    pub const fn id(&self) -> &SlotId {
        &self.id
    }

    /// Canonical scheduled instant used as the next cadence cursor.
    #[must_use]
    pub const fn scheduled_for(&self) -> Timestamp {
        self.scheduled_for
    }
}

/// Explicit prior decision state supplied to every authoritative plan.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScheduleDecisionState {
    last_slot: Option<ScheduleLastSlot>,
}

impl ScheduleDecisionState {
    /// A schedule with no durable slot decision yet.
    #[must_use]
    pub const fn empty() -> Self {
        Self { last_slot: None }
    }

    /// State restored from the adapter's last durable slot record.
    #[must_use]
    pub const fn new(last_slot: Option<ScheduleLastSlot>) -> Self {
        Self { last_slot }
    }

    /// Convenience for persisting and immediately replaying one planned slot.
    #[must_use]
    pub fn after(slot: &ScheduleSlot) -> Self {
        Self::new(Some(slot.durable_state()))
    }

    /// Last durable slot, when one exists.
    #[must_use]
    pub const fn last_slot(&self) -> Option<&ScheduleLastSlot> {
        self.last_slot.as_ref()
    }
}

impl Default for ScheduleDecisionState {
    fn default() -> Self {
        Self::empty()
    }
}

/// Authoritative due classification for the current injected wall time.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScheduleDueVerdict {
    /// The slot is inside the existing on-time grace window.
    ScheduledOnTime {
        /// Slot ready to claim.
        slot: ScheduleSlot,
    },
    /// A missed slot remains eligible under the declaration's catch-up law.
    CatchUp {
        /// Slot ready to claim.
        slot: ScheduleSlot,
        /// Outstanding slots in the silence, saturated at the cadence cap.
        missed_slots: u32,
    },
    /// The declaration explicitly skips missed slots.
    SkippedMissed {
        /// Slot the adapter should durably mark consumed by the skip.
        slot: ScheduleSlot,
        /// Outstanding slots in the silence, saturated at the cadence cap.
        missed_slots: u32,
    },
    /// The slot was missed and then failed the separate maximum-lateness door.
    SkippedTooLate {
        /// Slot the adapter should durably mark consumed by the skip.
        slot: ScheduleSlot,
        /// Absolute seconds elapsed since `scheduledFor`.
        lateness_seconds: u64,
        /// Inclusive declaration bound that the lateness exceeded.
        maximum_seconds: u64,
    },
    /// The definition is inactive and its declared pause bound has not
    /// passed; pause evidence remains visible to status.
    PausedInactive {
        /// Operator-provided pause reason.
        reason: String,
        /// ISO date bounding the declared pause.
        pause_until: String,
    },
    /// The one-time slot already has a matching durable decision.
    OnceConsumed {
        /// Identity proving the same one-time slot cannot re-arm.
        slot_id: SlotId,
        /// Scheduled instant carried by the durable predecessor.
        scheduled_for: Timestamp,
    },
    /// No slot is currently actionable.
    NotDue,
}

impl ScheduleDueVerdict {
    /// Slot carried by an actionable claim/skip verdict.
    #[must_use]
    pub const fn slot(&self) -> Option<&ScheduleSlot> {
        match self {
            Self::ScheduledOnTime { slot }
            | Self::CatchUp { slot, .. }
            | Self::SkippedMissed { slot, .. }
            | Self::SkippedTooLate { slot, .. } => Some(slot),
            Self::PausedInactive { .. } | Self::OnceConsumed { .. } | Self::NotDue => None,
        }
    }

    const fn needs_immediate_wake(&self) -> bool {
        self.slot().is_some()
    }
}

/// Bounded future-slot projection. The private allocation prevents callers
/// from manufacturing an unbounded planner result.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScheduleProjection(Box<[ScheduleSlot]>);

impl ScheduleProjection {
    /// Future slots in chronological order.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ScheduleSlot> {
        self.0.iter()
    }

    /// Number of projected slots, always at most
    /// [`MAX_SCHEDULE_PROJECTION_SLOTS`].
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether this schedule has no timed future slot in the projection.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Complete pure planner output for apply/status and the resident wake loop.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SchedulePlan {
    revision: ScheduleRevision,
    due: ScheduleDueVerdict,
    projection: ScheduleProjection,
    earliest_wake_hint: Option<Timestamp>,
    overlap: Overlap,
    after_skip: AfterSkip,
}

impl SchedulePlan {
    /// Revision that was actually planned.
    #[must_use]
    pub const fn revision(&self) -> &ScheduleRevision {
        &self.revision
    }

    /// Current due classification.
    #[must_use]
    pub const fn due(&self) -> &ScheduleDueVerdict {
        &self.due
    }

    /// Bounded future projection.
    #[must_use]
    pub const fn projection(&self) -> &ScheduleProjection {
        &self.projection
    }

    /// Future slots in chronological order.
    #[must_use]
    pub fn next_slots(&self) -> impl ExactSizeIterator<Item = &ScheduleSlot> {
        self.projection.iter()
    }

    /// Earliest instant worth waking. This is a hint, never claim authority.
    #[must_use]
    pub const fn earliest_wake_hint(&self) -> Option<Timestamp> {
        self.earliest_wake_hint
    }

    /// Declared overlap input for the execution adapter; not executed here.
    #[must_use]
    pub const fn overlap(&self) -> Overlap {
        self.overlap
    }

    /// Declared post-overlap-skip input for the adapter; not executed here.
    #[must_use]
    pub const fn after_skip(&self) -> AfterSkip {
        self.after_skip
    }
}

/// Typed planner refusals. A refusal yields no effective schedule hint.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum SchedulePlanError {
    /// No deterministic slot-offset law exists for `jitter: hash` yet.
    #[error("active timed schedules with hash jitter are unsupported until an offset law exists")]
    UnsupportedHashJitter,
    /// No preemption law exists for `overlap: replace` yet.
    #[error(
        "active timed schedules with overlap=replace are unsupported until a preemption law exists"
    )]
    UnsupportedOverlapReplace,
    /// No queueing law exists for `overlap: queue` yet.
    #[error(
        "active timed schedules with overlap=queue are unsupported until a queueing law exists"
    )]
    UnsupportedOverlapQueue,
    /// No completion-trigger law exists for `afterSkip: on_completion` yet.
    #[error(
        "active timed schedules with afterSkip=on_completion are unsupported until a completion-trigger law exists"
    )]
    UnsupportedAfterSkipOnCompletion,
    /// No enforcement law reads the documented (m,k) `tolerance` yet.
    #[error(
        "active timed schedules with tolerance are unsupported until the (m,k)-firm law exists"
    )]
    UnsupportedTolerance,
    /// A validated canonical cadence failed to re-enter the shared parser.
    #[error("canonical cadence failed to re-parse: {0}")]
    InvalidCanonicalCadence(String),
}

impl NikaErrorCode for SchedulePlanError {
    fn nika_code(&self) -> NikaCode {
        codes::NIKA_017
    }
}

/// Recompute one canonical schedule from current facts.
///
/// `projection_limit` is clamped to [`MAX_SCHEDULE_PROJECTION_SLOTS`]. A
/// cached [`SchedulePlan::earliest_wake_hint`] must never be used to claim:
/// invoke this function again with fresh `now` and restored `state` first.
///
/// # Errors
/// Active timed hash jitter refuses until a deterministic offset law is
/// ratified, `overlap: replace` / `overlap: queue` refuse until preemption
/// and queueing laws exist, `afterSkip: on_completion` refuses until a
/// completion-trigger law exists, and `tolerance` refuses until the
/// (m,k)-firm law reads it. A canonical cadence that no longer parses
/// also fails closed.
pub fn plan_schedule(
    definition: &ScheduleDefinition,
    now: &Zoned,
    state: &ScheduleDecisionState,
    projection_limit: usize,
) -> Result<SchedulePlan, SchedulePlanError> {
    if !definition.is_active() && !pause_expired(definition, now) {
        return Ok(paused_plan(definition, now));
    }
    if !matches!(definition.when(), ScheduleWhen::Webhook) {
        if definition.jitter().is_some() {
            return Err(SchedulePlanError::UnsupportedHashJitter);
        }
        if definition.overlap() == Overlap::Remplacer {
            return Err(SchedulePlanError::UnsupportedOverlapReplace);
        }
        if definition.overlap() == Overlap::File {
            return Err(SchedulePlanError::UnsupportedOverlapQueue);
        }
        if definition.after_skip() == AfterSkip::ACompletion {
            return Err(SchedulePlanError::UnsupportedAfterSkipOnCompletion);
        }
        if definition.tolerance().is_some() {
            return Err(SchedulePlanError::UnsupportedTolerance);
        }
    }
    let limit = projection_limit.min(MAX_SCHEDULE_PROJECTION_SLOTS);
    match definition.when() {
        ScheduleWhen::Once { at } => Ok(plan_once(definition, *at, now, state, limit)),
        ScheduleWhen::Cadence { expression } => {
            let cadence = parse_canonical_cadence(expression)?;
            Ok(plan_cadence(
                definition, expression, &cadence, now, state, limit,
            ))
        }
        ScheduleWhen::Webhook => Ok(complete_plan(
            definition,
            ScheduleDueVerdict::NotDue,
            Vec::new(),
            None,
            now,
        )),
    }
}

/// The declared pause bound, judged the arm-fire way: a `pauseUntil`
/// strictly before the decision instant's own civil date means the
/// suspension is over and the definition plans as active again.
fn pause_expired(definition: &ScheduleDefinition, now: &Zoned) -> bool {
    definition
        .pause_until()
        .is_some_and(|until| crate::tick::date_expired(until, now))
}

fn paused_plan(definition: &ScheduleDefinition, now: &Zoned) -> SchedulePlan {
    complete_plan(
        definition,
        ScheduleDueVerdict::PausedInactive {
            reason: definition.pause_reason().unwrap_or_default().to_owned(),
            pause_until: definition.pause_until().unwrap_or_default().to_owned(),
        },
        Vec::new(),
        None,
        now,
    )
}

fn plan_once(
    definition: &ScheduleDefinition,
    at: Timestamp,
    now: &Zoned,
    state: &ScheduleDecisionState,
    limit: usize,
) -> SchedulePlan {
    let slot = ScheduleSlot::once(definition, at);
    // Consumed iff the instant is at or before the watermark: a path edit
    // keeps it consumed, a later re-dating is a new instant (#1354).
    if applicable_last(definition, state).is_some_and(|last| last.scheduled_for >= at) {
        return complete_plan(
            definition,
            ScheduleDueVerdict::OnceConsumed {
                slot_id: slot.id,
                scheduled_for: at,
            },
            Vec::new(),
            None,
            now,
        );
    }
    if now.timestamp() < at {
        let projection = (limit > 0).then(|| slot.clone()).into_iter().collect();
        return complete_plan(
            definition,
            ScheduleDueVerdict::NotDue,
            projection,
            Some(at),
            now,
        );
    }
    let verdict = classify_due(definition, slot, 1, now);
    complete_plan(definition, verdict, Vec::new(), None, now)
}

fn plan_cadence(
    definition: &ScheduleDefinition,
    expression: &str,
    cadence: &Cadence,
    now: &Zoned,
    state: &ScheduleDecisionState,
    limit: usize,
) -> SchedulePlan {
    let last = applicable_last(definition, state);
    let window = cadence_window(definition, expression, cadence, now, last);
    let verdict = window.map_or(ScheduleDueVerdict::NotDue, |window| {
        let slot = match definition.missed() {
            MissPolicy::Rattraper => window.first,
            MissPolicy::RattraperUneFois | MissPolicy::Sauter => window.latest,
        };
        classify_due(definition, slot, window.count, now)
    });
    let basis = projection_basis(now, last);
    let first_future = cadence.next_after(&basis).map(|slot| slot.at.timestamp());
    let projection = next_slots(cadence, &basis, limit)
        .map(|slot| ScheduleSlot::cadence(definition, expression, &slot))
        .collect();
    complete_plan(definition, verdict, projection, first_future, now)
}

struct DueWindow {
    first: ScheduleSlot,
    latest: ScheduleSlot,
    count: u32,
}

fn cadence_window(
    definition: &ScheduleDefinition,
    expression: &str,
    cadence: &Cadence,
    now: &Zoned,
    last: Option<&ScheduleLastSlot>,
) -> Option<DueWindow> {
    let Some(last) = last else {
        let slot = recent_slot(cadence, now)?;
        let slot = ScheduleSlot::cadence(definition, expression, &slot);
        return within_on_time(now, slot.scheduled_for).then(|| DueWindow {
            first: slot.clone(),
            latest: slot,
            count: 1,
        });
    };
    let cursor = last.scheduled_for.to_zoned(TimeZone::UTC);
    let mut due = next_slots(cadence, &cursor, MISSED_SLOTS_CAP);
    let first = due.next().filter(|slot| slot.at <= *now)?;
    let mut latest = first.clone();
    let mut count = 1u32;
    for slot in due.take_while(|slot| slot.at <= *now) {
        latest = slot;
        count = count.saturating_add(1);
    }
    if usize::try_from(count).ok() == Some(MISSED_SLOTS_CAP)
        && let Some(actual_latest) = recent_slot(cadence, now)
        && actual_latest.at > cursor
    {
        latest = actual_latest;
    }
    Some(DueWindow {
        first: ScheduleSlot::cadence(definition, expression, &first),
        latest: ScheduleSlot::cadence(definition, expression, &latest),
        count,
    })
}

fn recent_slot(cadence: &Cadence, now: &Zoned) -> Option<Slot> {
    let previous = cadence.prev_before(now)?;
    cadence
        .next_after(&previous.at)
        .filter(|slot| slot.at <= *now)
        .or(Some(previous))
}

fn classify_due(
    definition: &ScheduleDefinition,
    slot: ScheduleSlot,
    missed_slots: u32,
    now: &Zoned,
) -> ScheduleDueVerdict {
    let lateness_seconds = lateness(now.timestamp(), slot.scheduled_for);
    if missed_slots == 1 && within_on_time(now, slot.scheduled_for) {
        return ScheduleDueVerdict::ScheduledOnTime { slot };
    }
    if let Some(maximum_seconds) = definition.max_lateness_seconds()
        && lateness_seconds > maximum_seconds
    {
        return ScheduleDueVerdict::SkippedTooLate {
            slot,
            lateness_seconds,
            maximum_seconds,
        };
    }
    match definition.missed() {
        MissPolicy::Rattraper | MissPolicy::RattraperUneFois => {
            ScheduleDueVerdict::CatchUp { slot, missed_slots }
        }
        MissPolicy::Sauter => ScheduleDueVerdict::SkippedMissed { slot, missed_slots },
    }
}

/// The persisted last slot is the schedule's WATERMARK: the store scopes
/// it by origin and schedule id, so a declaration edit — the workflow path,
/// the cadence text — must not reset it (a consumed `once` stays consumed;
/// catch-up never re-answers slots the old declaration already answered).
/// A `SlotId` is local to one slot and never the continuity key; only a
/// schedule kind that has no slots (a webhook) carries no watermark.
fn applicable_last<'a>(
    definition: &ScheduleDefinition,
    state: &'a ScheduleDecisionState,
) -> Option<&'a ScheduleLastSlot> {
    let last = state.last_slot.as_ref()?;
    (!matches!(definition.when(), ScheduleWhen::Webhook)).then_some(last)
}

fn once_key(at: Timestamp) -> String {
    format!("once:{at}")
}

fn projection_basis(now: &Zoned, last: Option<&ScheduleLastSlot>) -> Zoned {
    match last {
        Some(last) if last.scheduled_for > now.timestamp() => {
            last.scheduled_for.to_zoned(TimeZone::UTC)
        }
        Some(_) | None => now.clone(),
    }
}

fn within_on_time(now: &Zoned, scheduled_for: Timestamp) -> bool {
    lateness(now.timestamp(), scheduled_for)
        <= u64::try_from(ON_TIME_WINDOW.as_secs()).unwrap_or(u64::MAX)
}

fn lateness(now: Timestamp, scheduled_for: Timestamp) -> u64 {
    u64::try_from(now.as_second().saturating_sub(scheduled_for.as_second())).unwrap_or(u64::MAX)
}

fn parse_canonical_cadence(expression: &str) -> Result<Cadence, SchedulePlanError> {
    match Cadence::parse(expression) {
        Ok(cadence @ (Cadence::Cron { .. } | Cadence::Every { .. })) => Ok(cadence),
        Ok(Cadence::Webhook) => Err(SchedulePlanError::InvalidCanonicalCadence(
            "timed definition parsed as webhook".to_owned(),
        )),
        Err(error) => Err(SchedulePlanError::InvalidCanonicalCadence(
            error.to_string(),
        )),
    }
}

fn complete_plan(
    definition: &ScheduleDefinition,
    due: ScheduleDueVerdict,
    projection: Vec<ScheduleSlot>,
    next_future: Option<Timestamp>,
    now: &Zoned,
) -> SchedulePlan {
    let earliest_wake_hint = if due.needs_immediate_wake() {
        Some(now.timestamp())
    } else {
        next_future
    };
    SchedulePlan {
        revision: definition.revision(),
        due,
        projection: ScheduleProjection(projection.into_boxed_slice()),
        earliest_wake_hint,
        overlap: definition.overlap(),
        after_skip: definition.after_skip(),
    }
}

/// The existing API/status shape of an already judged due value; the planner keeps authority.
#[must_use]
pub fn due_json(due: &ScheduleDueVerdict) -> Value {
    match due {
        ScheduleDueVerdict::ScheduledOnTime { slot } => {
            json!({"kind": "scheduled", "slot": slot_json(slot)})
        }
        ScheduleDueVerdict::CatchUp { slot, missed_slots } => json!({
            "kind": "catch_up", "slot": slot_json(slot), "missedSlots": missed_slots
        }),
        ScheduleDueVerdict::SkippedMissed { slot, missed_slots } => json!({
            "kind": "skipped_missed", "slot": slot_json(slot), "missedSlots": missed_slots
        }),
        ScheduleDueVerdict::SkippedTooLate {
            slot,
            lateness_seconds,
            maximum_seconds,
        } => json!({
            "kind": "skipped_too_late", "slot": slot_json(slot),
            "latenessSeconds": lateness_seconds, "maximumSeconds": maximum_seconds
        }),
        ScheduleDueVerdict::PausedInactive {
            reason,
            pause_until,
        } => json!({
            "kind": "paused", "reason": reason, "pauseUntil": pause_until
        }),
        ScheduleDueVerdict::OnceConsumed {
            slot_id,
            scheduled_for,
        } => json!({
            "kind": "once_consumed", "slotId": slot_id.as_str(),
            "scheduledFor": scheduled_for.to_string()
        }),
        ScheduleDueVerdict::NotDue => json!({"kind": "not_due"}),
    }
}

/// The existing API/status shape of a canonical slot, preserving its civil/shift evidence.
#[must_use]
pub fn slot_json(slot: &ScheduleSlot) -> Value {
    json!({
        "slotId": slot.id().as_str(),
        "scheduledFor": slot.scheduled_for().to_string(),
        "requestedCivil": slot.requested_civil().map(|civil| civil.to_string()),
        "shift": shift_word(slot.shift()),
    })
}

const fn shift_word(value: Shift) -> &'static str {
    match value {
        Shift::Exact => "exact",
        Shift::AdvancedFirstValid => "advanced_first_valid",
        Shift::FoldedFirst => "folded_first",
    }
}

#[cfg(test)]
#[path = "schedule_plan/tests.rs"]
mod tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod json_oracle_tests {
    //! The schedule JSON byte oracle: every expected string below was captured
    //! by running the immutable pre-move Serve projections
    //! (`nika-serve` `schedule_http::{when_json, due_json, slot_json}`) on these
    //! same values, never by the projections under test. It covers the three
    //! current `ScheduleWhen`, seven `ScheduleDueVerdict` and three `Shift`
    //! variants, nulls and the exact timestamp forms.

    use super::*;
    use crate::schedule::when_json;

    fn slot(
        id: SlotId,
        scheduled_for: Timestamp,
        requested_civil: Option<DateTime>,
        shift: Shift,
    ) -> ScheduleSlot {
        ScheduleSlot {
            id,
            scheduled_for,
            requested_civil,
            shift,
        }
    }

    const SLOT_A: &str = "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0";
    const SLOT_B: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn at(text: &str) -> Timestamp {
        text.parse().expect("timestamp")
    }

    fn id(text: &str) -> SlotId {
        SlotId::from_wire(text).expect("slot id")
    }

    fn slots() -> Vec<(&'static str, ScheduleSlot)> {
        let civil: DateTime = "2026-03-29T02:30:00".parse().expect("civil");
        vec![
            (
                "exact",
                slot(
                    id(SLOT_A),
                    at("2099-09-01T07:00:00Z"),
                    Some(civil),
                    Shift::Exact,
                ),
            ),
            (
                "advanced",
                slot(
                    id(SLOT_A),
                    at("2026-03-29T01:00:00Z"),
                    Some(civil),
                    Shift::AdvancedFirstValid,
                ),
            ),
            (
                "folded",
                slot(
                    id(SLOT_B),
                    at("2026-10-25T00:30:00.123456789Z"),
                    Some("2026-10-25T02:30:00.5".parse().expect("civil")),
                    Shift::FoldedFirst,
                ),
            ),
            (
                "absolute",
                slot(id(SLOT_B), at("1970-01-01T00:00:00Z"), None, Shift::Exact),
            ),
        ]
    }

    fn whens() -> Vec<(&'static str, ScheduleWhen)> {
        vec![
            (
                "once",
                ScheduleWhen::Once {
                    at: at("2099-09-01T07:00:00Z"),
                },
            ),
            (
                "once_nanos",
                ScheduleWhen::Once {
                    at: at("2026-03-29T01:30:00.000000001Z"),
                },
            ),
            (
                "cadence",
                ScheduleWhen::Cadence {
                    expression: "0 9 * * *".to_owned(),
                },
            ),
            (
                "cadence_escaped",
                ScheduleWhen::Cadence {
                    expression: "*/5 * * * * « é » \"q\" \\".to_owned(),
                },
            ),
            ("webhook", ScheduleWhen::Webhook),
        ]
    }

    fn dues() -> Vec<(String, ScheduleDueVerdict)> {
        let mut out = Vec::new();
        for (s, slot) in slots() {
            out.push((
                format!("scheduled/{s}"),
                ScheduleDueVerdict::ScheduledOnTime { slot: slot.clone() },
            ));
            out.push((
                format!("catch_up/{s}"),
                ScheduleDueVerdict::CatchUp {
                    slot: slot.clone(),
                    missed_slots: 3,
                },
            ));
            out.push((
                format!("skipped_missed/{s}"),
                ScheduleDueVerdict::SkippedMissed {
                    slot: slot.clone(),
                    missed_slots: u32::MAX,
                },
            ));
            out.push((
                format!("skipped_too_late/{s}"),
                ScheduleDueVerdict::SkippedTooLate {
                    slot,
                    lateness_seconds: u64::MAX,
                    maximum_seconds: 0,
                },
            ));
        }
        out.push((
            "paused".to_owned(),
            ScheduleDueVerdict::PausedInactive {
                reason: "maintenance « é » \"q\" \\ \n".to_owned(),
                pause_until: "2099-01-01T00:00:00Z".to_owned(),
            },
        ));
        out.push((
            "paused_empty".to_owned(),
            ScheduleDueVerdict::PausedInactive {
                reason: String::new(),
                pause_until: String::new(),
            },
        ));
        out.push((
            "once_consumed".to_owned(),
            ScheduleDueVerdict::OnceConsumed {
                slot_id: id(SLOT_A),
                scheduled_for: at("2099-09-01T07:00:00.25Z"),
            },
        ));
        out.push(("not_due".to_owned(), ScheduleDueVerdict::NotDue));
        out
    }

    fn cases() -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (w, when) in whens() {
            let v = when_json(&when);
            out.push((format!("when/{w}/compact"), v.to_string()));
            out.push((format!("when/{w}/pretty"), format!("{v:#}")));
        }
        for (s, slot) in slots() {
            out.push((format!("slot/{s}"), slot_json(&slot).to_string()));
        }
        for (d, due) in dues() {
            out.push((format!("due/{d}"), due_json(&due).to_string()));
        }
        out
    }

    /// `(case, JSON)` captured from the pre-move Serve projections; never recomputed here.
    const EXPECTED: &[(&str, &str)] = &[
        (
            "when/once/compact",
            "{\"at\":\"2099-09-01T07:00:00Z\",\"kind\":\"once\"}",
        ),
        (
            "when/once/pretty",
            "{\n  \"at\": \"2099-09-01T07:00:00Z\",\n  \"kind\": \"once\"\n}",
        ),
        (
            "when/once_nanos/compact",
            "{\"at\":\"2026-03-29T01:30:00.000000001Z\",\"kind\":\"once\"}",
        ),
        (
            "when/once_nanos/pretty",
            "{\n  \"at\": \"2026-03-29T01:30:00.000000001Z\",\n  \"kind\": \"once\"\n}",
        ),
        (
            "when/cadence/compact",
            "{\"expression\":\"0 9 * * *\",\"kind\":\"cadence\"}",
        ),
        (
            "when/cadence/pretty",
            "{\n  \"expression\": \"0 9 * * *\",\n  \"kind\": \"cadence\"\n}",
        ),
        (
            "when/cadence_escaped/compact",
            "{\"expression\":\"*/5 * * * * « é » \\\"q\\\" \\\\\",\"kind\":\"cadence\"}",
        ),
        (
            "when/cadence_escaped/pretty",
            "{\n  \"expression\": \"*/5 * * * * « é » \\\"q\\\" \\\\\",\n  \"kind\": \"cadence\"\n}",
        ),
        ("when/webhook/compact", "{\"kind\":\"webhook\"}"),
        ("when/webhook/pretty", "{\n  \"kind\": \"webhook\"\n}"),
        (
            "slot/exact",
            "{\"requestedCivil\":\"2026-03-29T02:30:00\",\"scheduledFor\":\"2099-09-01T07:00:00Z\",\"shift\":\"exact\",\"slotId\":\"0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0\"}",
        ),
        (
            "slot/advanced",
            "{\"requestedCivil\":\"2026-03-29T02:30:00\",\"scheduledFor\":\"2026-03-29T01:00:00Z\",\"shift\":\"advanced_first_valid\",\"slotId\":\"0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0\"}",
        ),
        (
            "slot/folded",
            "{\"requestedCivil\":\"2026-10-25T02:30:00.5\",\"scheduledFor\":\"2026-10-25T00:30:00.123456789Z\",\"shift\":\"folded_first\",\"slotId\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}",
        ),
        (
            "slot/absolute",
            "{\"requestedCivil\":null,\"scheduledFor\":\"1970-01-01T00:00:00Z\",\"shift\":\"exact\",\"slotId\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}",
        ),
        (
            "due/scheduled/exact",
            "{\"kind\":\"scheduled\",\"slot\":{\"requestedCivil\":\"2026-03-29T02:30:00\",\"scheduledFor\":\"2099-09-01T07:00:00Z\",\"shift\":\"exact\",\"slotId\":\"0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0\"}}",
        ),
        (
            "due/catch_up/exact",
            "{\"kind\":\"catch_up\",\"missedSlots\":3,\"slot\":{\"requestedCivil\":\"2026-03-29T02:30:00\",\"scheduledFor\":\"2099-09-01T07:00:00Z\",\"shift\":\"exact\",\"slotId\":\"0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0\"}}",
        ),
        (
            "due/skipped_missed/exact",
            "{\"kind\":\"skipped_missed\",\"missedSlots\":4294967295,\"slot\":{\"requestedCivil\":\"2026-03-29T02:30:00\",\"scheduledFor\":\"2099-09-01T07:00:00Z\",\"shift\":\"exact\",\"slotId\":\"0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0\"}}",
        ),
        (
            "due/skipped_too_late/exact",
            "{\"kind\":\"skipped_too_late\",\"latenessSeconds\":18446744073709551615,\"maximumSeconds\":0,\"slot\":{\"requestedCivil\":\"2026-03-29T02:30:00\",\"scheduledFor\":\"2099-09-01T07:00:00Z\",\"shift\":\"exact\",\"slotId\":\"0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0\"}}",
        ),
        (
            "due/scheduled/advanced",
            "{\"kind\":\"scheduled\",\"slot\":{\"requestedCivil\":\"2026-03-29T02:30:00\",\"scheduledFor\":\"2026-03-29T01:00:00Z\",\"shift\":\"advanced_first_valid\",\"slotId\":\"0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0\"}}",
        ),
        (
            "due/catch_up/advanced",
            "{\"kind\":\"catch_up\",\"missedSlots\":3,\"slot\":{\"requestedCivil\":\"2026-03-29T02:30:00\",\"scheduledFor\":\"2026-03-29T01:00:00Z\",\"shift\":\"advanced_first_valid\",\"slotId\":\"0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0\"}}",
        ),
        (
            "due/skipped_missed/advanced",
            "{\"kind\":\"skipped_missed\",\"missedSlots\":4294967295,\"slot\":{\"requestedCivil\":\"2026-03-29T02:30:00\",\"scheduledFor\":\"2026-03-29T01:00:00Z\",\"shift\":\"advanced_first_valid\",\"slotId\":\"0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0\"}}",
        ),
        (
            "due/skipped_too_late/advanced",
            "{\"kind\":\"skipped_too_late\",\"latenessSeconds\":18446744073709551615,\"maximumSeconds\":0,\"slot\":{\"requestedCivil\":\"2026-03-29T02:30:00\",\"scheduledFor\":\"2026-03-29T01:00:00Z\",\"shift\":\"advanced_first_valid\",\"slotId\":\"0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0\"}}",
        ),
        (
            "due/scheduled/folded",
            "{\"kind\":\"scheduled\",\"slot\":{\"requestedCivil\":\"2026-10-25T02:30:00.5\",\"scheduledFor\":\"2026-10-25T00:30:00.123456789Z\",\"shift\":\"folded_first\",\"slotId\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}}",
        ),
        (
            "due/catch_up/folded",
            "{\"kind\":\"catch_up\",\"missedSlots\":3,\"slot\":{\"requestedCivil\":\"2026-10-25T02:30:00.5\",\"scheduledFor\":\"2026-10-25T00:30:00.123456789Z\",\"shift\":\"folded_first\",\"slotId\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}}",
        ),
        (
            "due/skipped_missed/folded",
            "{\"kind\":\"skipped_missed\",\"missedSlots\":4294967295,\"slot\":{\"requestedCivil\":\"2026-10-25T02:30:00.5\",\"scheduledFor\":\"2026-10-25T00:30:00.123456789Z\",\"shift\":\"folded_first\",\"slotId\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}}",
        ),
        (
            "due/skipped_too_late/folded",
            "{\"kind\":\"skipped_too_late\",\"latenessSeconds\":18446744073709551615,\"maximumSeconds\":0,\"slot\":{\"requestedCivil\":\"2026-10-25T02:30:00.5\",\"scheduledFor\":\"2026-10-25T00:30:00.123456789Z\",\"shift\":\"folded_first\",\"slotId\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}}",
        ),
        (
            "due/scheduled/absolute",
            "{\"kind\":\"scheduled\",\"slot\":{\"requestedCivil\":null,\"scheduledFor\":\"1970-01-01T00:00:00Z\",\"shift\":\"exact\",\"slotId\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}}",
        ),
        (
            "due/catch_up/absolute",
            "{\"kind\":\"catch_up\",\"missedSlots\":3,\"slot\":{\"requestedCivil\":null,\"scheduledFor\":\"1970-01-01T00:00:00Z\",\"shift\":\"exact\",\"slotId\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}}",
        ),
        (
            "due/skipped_missed/absolute",
            "{\"kind\":\"skipped_missed\",\"missedSlots\":4294967295,\"slot\":{\"requestedCivil\":null,\"scheduledFor\":\"1970-01-01T00:00:00Z\",\"shift\":\"exact\",\"slotId\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}}",
        ),
        (
            "due/skipped_too_late/absolute",
            "{\"kind\":\"skipped_too_late\",\"latenessSeconds\":18446744073709551615,\"maximumSeconds\":0,\"slot\":{\"requestedCivil\":null,\"scheduledFor\":\"1970-01-01T00:00:00Z\",\"shift\":\"exact\",\"slotId\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}}",
        ),
        (
            "due/paused",
            "{\"kind\":\"paused\",\"pauseUntil\":\"2099-01-01T00:00:00Z\",\"reason\":\"maintenance « é » \\\"q\\\" \\\\ \\n\"}",
        ),
        (
            "due/paused_empty",
            "{\"kind\":\"paused\",\"pauseUntil\":\"\",\"reason\":\"\"}",
        ),
        (
            "due/once_consumed",
            "{\"kind\":\"once_consumed\",\"scheduledFor\":\"2099-09-01T07:00:00.25Z\",\"slotId\":\"0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0\"}",
        ),
        ("due/not_due", "{\"kind\":\"not_due\"}"),
    ];

    #[test]
    fn schedule_projections_keep_the_pre_move_bytes() {
        let want: Vec<(String, String)> = EXPECTED
            .iter()
            .map(|(n, t)| ((*n).to_owned(), (*t).to_owned()))
            .collect();
        assert_eq!(cases(), want);
    }
}
