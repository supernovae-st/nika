use std::fmt;
use std::io;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

const DIGEST_HEX_LEN: usize = 64;
const MAX_IDEMPOTENCY_KEY_BYTES: usize = 255;

/// Opaque, non-sequential identifier for one durable job.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(String);

impl JobId {
    pub(crate) fn random() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub(crate) fn validate(&self) -> Result<(), JobStoreError> {
        let parsed = Uuid::parse_str(&self.0)
            .map_err(|_| JobStoreError::Corrupt("job id is not a UUID".to_owned()))?;
        if parsed.get_version_num() != 4 || parsed.to_string() != self.0 {
            return Err(JobStoreError::Corrupt(
                "job id is not a canonical random UUID".to_owned(),
            ));
        }
        Ok(())
    }

    /// Return the opaque identifier string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Bounded caller-supplied key that identifies retries of one request.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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

/// Stable digest of the admitted request bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(DIGEST_HEX_LEN);
        for byte in bytes {
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self(encoded)
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

/// Proof that the server bootstrap established a new exclusive incarnation.
///
/// The type is public so it can guard the recovery API, but only `nika-serve`
/// can construct a value. The HTTP bootstrap introduced in W06 owns that
/// construction after it proves exclusivity and before it exposes the store.
#[derive(Debug)]
#[non_exhaustive]
pub struct ServerIncarnation {
    pub(crate) _private: (),
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
}

/// One sequenced payload in a job's durable event stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct JobEvent {
    pub(crate) sequence: u64,
    pub(crate) payload: Value,
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

/// Typed failures from the durable job store.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum JobStoreError {
    /// Descriptor-rooted filesystem operation failed.
    #[error("job store I/O failed: {0}")]
    Io(#[from] io::Error),
    /// An idempotency key violated its bounded wire contract.
    #[error("idempotency key must contain 1 to 255 visible ASCII bytes")]
    InvalidIdempotencyKey,
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
