use std::fmt;
use std::fs::File;
use std::io;

use nix::fcntl::Flock;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

const DIGEST_HEX_LEN: usize = 64;
const MAX_IDEMPOTENCY_KEY_BYTES: usize = 255;

/// Maximum encoded size of one durable event payload.
pub const MAX_EVENT_PAYLOAD_BYTES: usize = 64 * 1024;
/// Maximum number of event payloads admitted by one append operation.
pub const MAX_EVENT_BATCH_LEN: usize = 64;
/// Maximum encoded size of the complete durable job snapshot.
pub const MAX_JOB_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;
/// Maximum number of events returned by one resume page.
pub const MAX_EVENT_PAGE_LEN: usize = 256;

/// Opaque, non-sequential identifier for one durable job.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct JobId(String);

impl JobId {
    pub(crate) fn random() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub(crate) fn validate(&self) -> Result<(), JobStoreError> {
        let parsed = Uuid::parse_str(&self.0)
            .map_err(|_| JobStoreError::Corrupt("job id is not a UUID".to_owned()))?;
        if parsed.get_version_num() != 4
            || parsed.get_variant() != uuid::Variant::RFC4122
            || parsed.to_string() != self.0
        {
            return Err(JobStoreError::Corrupt(
                "job id is not a canonical random UUID".to_owned(),
            ));
        }
        Ok(())
    }

    /// Parse a canonical opaque job identifier received from a wire adapter.
    ///
    /// Snapshot validation still reports [`JobStoreError::Corrupt`]. Wire
    /// adapters use this constructor so an unknown or malformed id cannot be
    /// distinguished from a missing job.
    ///
    /// # Errors
    /// Returns [`JobStoreError::InvalidJobId`] unless the value is a lowercase
    /// hyphenated RFC 4122 UUID v4.
    pub fn parse(value: impl Into<String>) -> Result<Self, JobStoreError> {
        let id = Self(value.into());
        id.validate().map_err(|_| JobStoreError::InvalidJobId)?;
        Ok(id)
    }

    /// Return the opaque identifier string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for JobId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let id = Self(String::deserialize(deserializer)?);
        id.validate().map_err(serde::de::Error::custom)?;
        Ok(id)
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Bounded caller-supplied key that identifies retries of one request.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Validate and construct an idempotency key.
    ///
    /// Keys contain 1–255 visible ASCII bytes. This keeps the future HTTP
    /// projection bounded without making the key part of a filesystem path.
    ///
    /// # Errors
    /// Returns [`JobStoreError::InvalidIdempotencyKey`] when the key is empty,
    /// oversized, or contains a non-visible ASCII byte.
    pub fn new(value: impl Into<String>) -> Result<Self, JobStoreError> {
        let value = value.into();
        validate_idempotency_key(&value)?;
        Ok(Self(value))
    }

    pub(crate) fn validate(&self) -> Result<(), JobStoreError> {
        validate_idempotency_key(&self.0)
    }

    /// Return the validated key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for IdempotencyKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Stable digest of the admitted request bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct RequestDigest(String);

impl RequestDigest {
    /// Validate a 32-byte digest encoded as 64 hexadecimal characters.
    ///
    /// # Errors
    /// Returns [`JobStoreError::InvalidRequestDigest`] for any other shape.
    pub fn new(value: impl Into<String>) -> Result<Self, JobStoreError> {
        let value = value.into();
        validate_digest(&value)?;
        Ok(Self(value))
    }

    /// Encode raw digest bytes using lowercase hexadecimal.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(encode_hex(bytes))
    }

    pub(crate) fn validate(&self) -> Result<(), JobStoreError> {
        validate_digest(&self.0)
    }

    /// Return the lowercase hexadecimal digest.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RequestDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Proof that the server bootstrap established a new exclusive incarnation.
///
/// The type is public so it can guard the recovery API, but only `nika-serve`
/// can construct a value. The HTTP bootstrap introduced in W06 owns that
/// construction after it proves exclusivity and before it exposes the store.
#[non_exhaustive]
pub struct ServerIncarnation {
    pub(crate) generation: IncarnationGeneration,
    pub(crate) _lease: Flock<File>,
}

impl fmt::Debug for ServerIncarnation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerIncarnation")
            .field("generation", &self.generation.get())
            .field("lease", &"<held>")
            .finish_non_exhaustive()
    }
}

/// Persisted monotone identity for one exclusive server startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct IncarnationGeneration(u64);

impl IncarnationGeneration {
    pub(crate) const INITIAL: Self = Self(0);

    pub(crate) fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }

    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

/// Validated hard limit for one durable event resume page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventPageLimit(usize);

impl EventPageLimit {
    /// Construct a non-zero page limit within the server hard cap.
    ///
    /// # Errors
    /// Returns [`JobStoreError::InvalidEventPageLimit`] for zero or a value
    /// greater than [`MAX_EVENT_PAGE_LEN`].
    pub fn new(value: usize) -> Result<Self, JobStoreError> {
        if value == 0 || value > MAX_EVENT_PAGE_LEN {
            return Err(JobStoreError::InvalidEventPageLimit {
                requested: value,
                maximum: MAX_EVENT_PAGE_LEN,
            });
        }
        Ok(Self(value))
    }

    /// Return the validated page length.
    #[must_use]
    pub fn get(self) -> usize {
        self.0
    }
}

/// Durable execution lifecycle exposed by the job state plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum JobStatus {
    /// Admitted but not yet executing.
    Queued,
    /// Execution owns the job in the active server incarnation.
    Running,
    /// Execution ownership was lost and effect settlement is unknown.
    Interrupted,
    /// Execution paused with resumable state.
    Paused,
    /// Execution completed successfully.
    Succeeded,
    /// Execution settled unsuccessfully.
    Failed,
}

impl JobStatus {
    pub(crate) fn allows(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Queued | Self::Paused, Self::Running | Self::Failed)
                | (Self::Running, Self::Paused | Self::Succeeded | Self::Failed)
        )
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Interrupted => "interrupted",
            Self::Paused => "paused",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        })
    }
}

/// Durable identity and status for one request admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct JobRecord {
    pub(crate) id: JobId,
    pub(crate) idempotency_key: IdempotencyKey,
    pub(crate) request_digest: RequestDigest,
    pub(crate) status: JobStatus,
    /// Contained `.nika.yaml` name captured at admission. Empty on stores
    /// written before this field existed; those queued rows cannot be
    /// rescheduled after a crash.
    #[serde(default)]
    pub(crate) workflow: String,
    /// Engine execution identity minted when the captured world is readmitted.
    /// Empty until the worker readmits the POST-time snapshot.
    #[serde(default)]
    pub(crate) execution_id: String,
    /// Trace identity derived from [`Self::execution_id`]. Empty until readmit.
    #[serde(default)]
    pub(crate) trace_id: String,
}

impl JobRecord {
    /// Return the opaque job identifier.
    #[must_use]
    pub fn id(&self) -> &JobId {
        &self.id
    }

    /// Return the caller-supplied retry key.
    #[must_use]
    pub fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    /// Return the digest bound to the retry key.
    #[must_use]
    pub fn request_digest(&self) -> &RequestDigest {
        &self.request_digest
    }

    /// Return the durable lifecycle status.
    #[must_use]
    pub fn status(&self) -> JobStatus {
        self.status
    }

    /// Return the contained workflow name captured at admission.
    #[must_use]
    pub fn workflow(&self) -> &str {
        &self.workflow
    }

    /// Return the engine execution identity after snapshot readmission.
    #[must_use]
    pub fn execution_id(&self) -> Option<&str> {
        (!self.execution_id.is_empty()).then_some(self.execution_id.as_str())
    }

    /// Return the engine trace identity after snapshot readmission.
    #[must_use]
    pub fn trace_id(&self) -> Option<&str> {
        (!self.trace_id.is_empty()).then_some(self.trace_id.as_str())
    }
}

/// One sequenced payload in a job's durable event stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct JobEvent {
    pub(crate) sequence: u64,
    pub(crate) payload: Value,
    pub(crate) previous_hash: Option<EventHash>,
    pub(crate) hash: EventHash,
}

impl JobEvent {
    /// Return the per-job sequence number, starting at one.
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Return the event payload exactly as admitted.
    #[must_use]
    pub fn payload(&self) -> &Value {
        &self.payload
    }

    /// Return the prior event hash, or `None` for the first event.
    #[must_use]
    pub fn previous_hash(&self) -> Option<&str> {
        self.previous_hash.as_ref().map(EventHash::as_str)
    }

    /// Return this event's lowercase SHA-256 chain hash.
    #[must_use]
    pub fn hash(&self) -> &str {
        self.hash.as_str()
    }
}

/// Lowercase SHA-256 identity for one domain-separated durable event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct EventHash(String);

impl EventHash {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(encode_hex(bytes))
    }

    pub(crate) fn validate(&self) -> Result<(), JobStoreError> {
        validate_digest(&self.0)
            .map_err(|_| JobStoreError::Corrupt("event hash is invalid".to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for EventHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hash = Self(String::deserialize(deserializer)?);
        hash.validate().map_err(serde::de::Error::custom)?;
        Ok(hash)
    }
}

/// One atomically persisted lifecycle transition and its event batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobMutation {
    pub(crate) record: JobRecord,
    pub(crate) events: Vec<JobEvent>,
}

impl JobMutation {
    /// Return the durable record after the transition.
    #[must_use]
    pub fn record(&self) -> &JobRecord {
        &self.record
    }

    /// Return the events committed in the same snapshot replacement.
    #[must_use]
    pub fn events(&self) -> &[JobEvent] {
        &self.events
    }
}

/// Result of binding an idempotency key to a request digest.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Admission {
    /// A new durable job was created.
    Created(JobRecord),
    /// The same key and digest replayed the existing job.
    Existing(JobRecord),
    /// The key was already bound to different request bytes.
    Conflict(JobRecord),
}

impl Admission {
    /// Return the created or pre-existing durable record.
    #[must_use]
    pub fn record(&self) -> &JobRecord {
        match self {
            Self::Created(record) | Self::Existing(record) | Self::Conflict(record) => record,
        }
    }
}

/// Typed failures from a monotonic approval-history authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ApprovalHistoryError {
    /// At least one digest was already present in the monotonic history.
    #[error("approval digest was already recorded")]
    AlreadyRecorded,
    /// The job snapshot names a digest absent from the monotonic history.
    #[error("approval digest is absent from the authoritative history")]
    MissingRecord,
    /// The authoritative history could not make a durable decision.
    #[error("approval history is unavailable")]
    Unavailable,
}

/// Typed failures from the durable job store.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum JobStoreError {
    /// Descriptor-rooted filesystem operation failed.
    ///
    /// Only the error kind crosses this public boundary. Path-bearing source
    /// context is discarded so rendering or chaining the typed error cannot
    /// disclose the operator's durable root.
    #[error("job store I/O failed: {0}")]
    Io(io::ErrorKind),
    /// An idempotency key violated its bounded wire contract.
    #[error("idempotency key must contain 1 to 255 visible ASCII bytes")]
    InvalidIdempotencyKey,
    /// A wire job id was not a canonical random UUID.
    #[error("job id must be a canonical random UUID")]
    InvalidJobId,
    /// A request digest was not canonical lowercase hexadecimal.
    #[error("request digest must contain exactly 64 lowercase hexadecimal characters")]
    InvalidRequestDigest,
    /// Durable state was unreadable or violated an invariant.
    #[error("job store state is corrupt: {0}")]
    Corrupt(String),
    /// No durable job has the requested opaque id.
    #[error("job {0} does not exist")]
    JobNotFound(JobId),
    /// The requested lifecycle edge is not legal.
    #[error("job transition {from} -> {to} is not allowed")]
    IllegalTransition {
        /// Current durable status.
        from: JobStatus,
        /// Requested next status.
        to: JobStatus,
    },
    /// The event sequence cannot advance without wrapping.
    #[error("job {0} exhausted its event sequence")]
    SequenceExhausted(JobId),
    /// One append request exceeded the bounded event batch contract.
    #[error("event batch contains {count} payloads; maximum is {maximum}")]
    EventBatchTooLarge {
        /// Number of payloads supplied by the caller.
        count: usize,
        /// Maximum payload count admitted in one append.
        maximum: usize,
    },
    /// A lifecycle transition omitted its mandatory durable event.
    #[error("a lifecycle transition requires at least one event")]
    TransitionEventRequired,
    /// An approval event omitted its canonical claim digest.
    #[error("approval_decided event requires a canonical digest")]
    InvalidApprovalEvent,
    /// A durable approval claim digest was already recorded.
    #[error("approval_decided digest was already consumed")]
    ApprovalClaimAlreadyRecorded,
    /// An approval event was attempted without a monotonic history authority.
    #[error("approval_decided event requires an approval history authority")]
    ApprovalHistoryRequired,
    /// The monotonic approval history could not make a durable decision.
    #[error("approval history is unavailable")]
    ApprovalHistoryUnavailable,
    /// The job snapshot names an approval absent from its authoritative history.
    #[error("job approval history does not match its authority")]
    ApprovalHistoryMismatch,
    /// One encoded event payload exceeded the durable payload ceiling.
    #[error("event payload {index} is {bytes} bytes; maximum is {maximum}")]
    EventPayloadTooLarge {
        /// Zero-based index of the refused payload in the append batch.
        index: usize,
        /// Encoded payload size.
        bytes: usize,
        /// Maximum encoded payload size.
        maximum: usize,
    },
    /// The complete encoded state exceeded its bounded snapshot contract.
    #[error("job store snapshot is {bytes} bytes; maximum is {maximum}")]
    SnapshotTooLarge {
        /// Encoded or on-disk snapshot size.
        bytes: u64,
        /// Maximum snapshot size.
        maximum: usize,
    },
    /// A requested event page was zero or exceeded the hard cap.
    #[error("event page limit {requested} is invalid; maximum is {maximum}")]
    InvalidEventPageLimit {
        /// Caller-supplied page limit.
        requested: usize,
        /// Maximum admitted page length.
        maximum: usize,
    },
    /// The persisted incarnation generation cannot advance without wrapping.
    #[error("server incarnation generation is exhausted")]
    IncarnationGenerationExhausted,
    /// A superseded incarnation attempted to settle current job ownership.
    #[error("server incarnation is stale")]
    StaleServerIncarnation,
    /// Another live server owns the durable root.
    #[error("server incarnation lease is already held")]
    ServerLeaseHeld,
    /// A resume cursor names an event that has not been persisted.
    #[error("job {job} event cursor {after} is beyond latest sequence {latest}")]
    CursorBeyondLatest {
        /// Job whose event journal was queried.
        job: JobId,
        /// Caller-supplied exclusive resume cursor.
        after: u64,
        /// Latest sequence currently persisted, or zero for an empty journal.
        latest: u64,
    },
    /// A panic occurred while another thread held the in-process lock.
    #[error("job store local lock is poisoned")]
    LockPoisoned,
    /// A bounded transport cannot admit another durable job.
    #[error("job store capacity is exhausted")]
    CapacityExceeded,
    /// Another writer currently owns the kernel lease.
    #[error("job store is busy")]
    Busy,
}

impl From<io::Error> for JobStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl From<ApprovalHistoryError> for JobStoreError {
    fn from(error: ApprovalHistoryError) -> Self {
        match error {
            ApprovalHistoryError::AlreadyRecorded => Self::ApprovalClaimAlreadyRecorded,
            ApprovalHistoryError::MissingRecord => Self::ApprovalHistoryMismatch,
            ApprovalHistoryError::Unavailable => Self::ApprovalHistoryUnavailable,
        }
    }
}

fn validate_idempotency_key(value: &str) -> Result<(), JobStoreError> {
    if value.is_empty()
        || value.len() > MAX_IDEMPOTENCY_KEY_BYTES
        || !value.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(JobStoreError::InvalidIdempotencyKey);
    }
    Ok(())
}

fn validate_digest(value: &str) -> Result<(), JobStoreError> {
    if value.len() != DIGEST_HEX_LEN
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(JobStoreError::InvalidRequestDigest);
    }
    Ok(())
}

fn encode_hex(bytes: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(DIGEST_HEX_LEN);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}
