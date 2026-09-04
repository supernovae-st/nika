use std::io;

use jiff::Timestamp;
use nika_cadence::{
    ArmGeneration, ScheduleDecision, ScheduleDefinition, ScheduleFinding, ScheduleLastSlot,
    ScheduleOrigin, ScheduleRevision, ScheduleSlot,
};
use nika_error::prelude::{NikaCode, NikaErrorCode, codes};
use thiserror::Error;

/// Maximum number of API-origin schedules in one resident store.
pub const MAX_API_SCHEDULES: usize = 64;
/// Maximum deterministic JSON bytes for one normalized schedule record.
pub const MAX_ENCODED_SCHEDULE_BYTES: usize = 7 * 1024;
/// Maximum deterministic JSON bytes for the complete resident schedule snapshot.
pub const MAX_SCHEDULE_STORE_BYTES: usize = 1024 * 1024;
/// Maximum origin-local last-slot decisions retained by the resident.
pub(crate) const MAX_DURABLE_SCHEDULE_DECISIONS: usize = 256;

/// Declarative concurrency precondition supplied by the future HTTP adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScheduleApplyPrecondition {
    /// Create only when the id is absent, equivalent to `If-None-Match: *`.
    Create,
    /// Update only from this revision, equivalent to `If-Match`.
    Revision(ScheduleRevision),
}

/// Durable verdict for one declarative schedule apply.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ScheduleApplyOutcome {
    /// A previously absent schedule was durably created.
    Created(ScheduleDefinition),
    /// The normalized specification was already durable.
    Unchanged(ScheduleDefinition),
    /// The expected prior revision was durably replaced.
    Updated(ScheduleDefinition),
    /// The precondition did not authorize a mutation.
    Conflict {
        /// Current durable revision, or `None` when the id is absent.
        current: Option<ScheduleRevision>,
    },
}

/// Durable resident action that consumed one canonical slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScheduleSlotAction {
    Claimed,
    Skipped,
}

/// Durable run identity and generation fencing one claimed schedule slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScheduleClaimEvidence {
    run_id: String,
    execution_id: String,
    trace_id: String,
    generation: ArmGeneration,
}

impl ScheduleClaimEvidence {
    pub(crate) fn new(
        run_id: String,
        execution_id: String,
        trace_id: String,
        generation: ArmGeneration,
    ) -> Self {
        Self {
            run_id,
            execution_id,
            trace_id,
            generation,
        }
    }

    pub(crate) fn run_id(&self) -> &str {
        &self.run_id
    }

    pub(crate) fn execution_id(&self) -> &str {
        &self.execution_id
    }

    pub(crate) fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub(crate) const fn generation(&self) -> &ArmGeneration {
        &self.generation
    }
}

/// Bounded last-slot evidence restored by status and the resident planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScheduleDecisionRecord {
    pub(super) origin: ScheduleOrigin,
    pub(super) schedule_id: String,
    pub(super) revision: ScheduleRevision,
    pub(super) slot: ScheduleLastSlot,
    pub(super) decision: ScheduleDecision,
    pub(super) action: ScheduleSlotAction,
    pub(super) decided_at: Timestamp,
    pub(super) reason: Option<String>,
    pub(super) claim: Option<ScheduleClaimEvidence>,
}

impl ScheduleDecisionRecord {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        origin: ScheduleOrigin,
        schedule_id: String,
        revision: ScheduleRevision,
        slot: &ScheduleSlot,
        decision: ScheduleDecision,
        action: ScheduleSlotAction,
        decided_at: Timestamp,
        reason: Option<String>,
        claim: Option<ScheduleClaimEvidence>,
    ) -> Self {
        Self {
            origin,
            schedule_id,
            revision,
            slot: slot.durable_state(),
            decision,
            action,
            decided_at,
            reason,
            claim,
        }
    }

    pub(crate) const fn origin(&self) -> ScheduleOrigin {
        self.origin
    }

    pub(crate) fn schedule_id(&self) -> &str {
        &self.schedule_id
    }

    pub(crate) const fn revision(&self) -> &ScheduleRevision {
        &self.revision
    }

    pub(crate) const fn slot(&self) -> &ScheduleLastSlot {
        &self.slot
    }

    pub(crate) const fn decision(&self) -> ScheduleDecision {
        self.decision
    }

    pub(crate) const fn action(&self) -> ScheduleSlotAction {
        self.action
    }

    pub(crate) const fn decided_at(&self) -> Timestamp {
        self.decided_at
    }

    pub(crate) fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub(crate) const fn claim(&self) -> Option<&ScheduleClaimEvidence> {
        self.claim.as_ref()
    }
}

/// Typed failures from the durable API schedule store.
#[derive(Debug, Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ScheduleStoreError {
    /// Descriptor-rooted filesystem access failed.
    #[error("schedule store I/O failed: {0}")]
    Io(io::ErrorKind),
    /// The store was last written by an engine speaking a NEWER machine
    /// protocol than this one (ADR-132 · #1352): refused, never reinterpreted.
    #[error("{0}")]
    WrittenByNewerEngine(String),
    /// The canonical schedule model refused the draft.
    #[error("schedule is invalid: {0}")]
    InvalidSchedule(ScheduleFinding),
    /// One normalized schedule exceeded its encoded ceiling.
    #[error("encoded schedule is {bytes} bytes; maximum is {maximum}")]
    ScheduleTooLarge {
        /// Encoded record size.
        bytes: usize,
        /// Maximum admitted size.
        maximum: usize,
    },
    /// A creation would exceed the bounded schedule count.
    #[error("schedule store reached its maximum of {maximum} schedules")]
    ScheduleLimit {
        /// Maximum admitted schedule count.
        maximum: usize,
    },
    /// The bounded last-slot decision table is full.
    #[error("schedule decision store reached its maximum of {maximum} entries")]
    DecisionLimit {
        /// Maximum retained origin-local decisions.
        maximum: usize,
    },
    /// The complete deterministic snapshot exceeded its encoded ceiling.
    #[error("schedule store snapshot is {bytes} bytes; maximum is {maximum}")]
    SnapshotTooLarge {
        /// Encoded or on-disk snapshot size.
        bytes: u64,
        /// Maximum admitted size.
        maximum: usize,
    },
    /// Existing state failed closed during recovery or mutation.
    #[error("schedule store state is corrupt: {0}")]
    Corrupt(String),
    /// A future canonical enum value has no v1 durable representation.
    #[error("schedule store schema does not support this canonical value")]
    UnsupportedCanonicalValue,
    /// The in-process serialization lock was poisoned.
    #[error("schedule store lock is poisoned")]
    LockPoisoned,
}

impl NikaErrorCode for ScheduleStoreError {
    fn nika_code(&self) -> NikaCode {
        codes::NIKA_018
    }
}

impl From<io::Error> for ScheduleStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl From<ScheduleFinding> for ScheduleStoreError {
    fn from(finding: ScheduleFinding) -> Self {
        Self::InvalidSchedule(finding)
    }
}
