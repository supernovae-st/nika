use std::io;

use nika_cadence::{ScheduleDefinition, ScheduleFinding, ScheduleRevision};
use thiserror::Error;

/// Maximum number of API-origin schedules in one resident store.
pub const MAX_API_SCHEDULES: usize = 64;
/// Maximum deterministic JSON bytes for one normalized schedule record.
pub const MAX_ENCODED_SCHEDULE_BYTES: usize = 7 * 1024;
/// Maximum deterministic JSON bytes for the complete resident schedule snapshot.
pub const MAX_SCHEDULE_STORE_BYTES: usize = 1024 * 1024;

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

/// Typed failures from the durable API schedule store.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ScheduleStoreError {
    /// Descriptor-rooted filesystem access failed.
    #[error("schedule store I/O failed: {0}")]
    Io(io::ErrorKind),
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
