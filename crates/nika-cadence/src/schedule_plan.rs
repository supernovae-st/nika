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
    /// The definition is inactive; pause evidence remains visible to status.
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
/// ratified, and `overlap: replace` / `overlap: queue` refuse until
/// preemption and queueing laws exist. A canonical cadence that no longer
/// parses also fails closed.
pub fn plan_schedule(
    definition: &ScheduleDefinition,
    now: &Zoned,
    state: &ScheduleDecisionState,
    projection_limit: usize,
) -> Result<SchedulePlan, SchedulePlanError> {
    if !definition.is_active() {
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
    if applicable_last(definition, state).is_some_and(|last| last.id == slot.id) {
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

fn applicable_last<'a>(
    definition: &ScheduleDefinition,
    state: &'a ScheduleDecisionState,
) -> Option<&'a ScheduleLastSlot> {
    let last = state.last_slot.as_ref()?;
    let expected = slot_id_at(definition, last.scheduled_for)?;
    (expected == last.id).then_some(last)
}

fn slot_id_at(definition: &ScheduleDefinition, at: Timestamp) -> Option<SlotId> {
    let key = match definition.when() {
        ScheduleWhen::Once { .. } => once_key(at),
        ScheduleWhen::Cadence { expression } => expression.clone(),
        ScheduleWhen::Webhook => return None,
    };
    Some(SlotId::derive(
        definition.workflow(),
        &key,
        &at.to_zoned(TimeZone::UTC),
    ))
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
        Ok(cadence @ Cadence::Cron { .. }) => Ok(cadence),
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

#[cfg(test)]
#[path = "schedule_plan/tests.rs"]
mod tests;
