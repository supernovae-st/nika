use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{Read as _, Write as _};
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use nika_fs::OwnedDir;
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

mod admission;
mod binding;
mod migration;
mod stored_job;

use binding::{
    attach_interrupted_receipt, ensure_receipt_matches, has_complete_execution_identity,
    hash_execution_identity, migrate_legacy_nonterminal_record, validate_identity_binding,
    validate_snapshot_digest, validate_terminal_record,
};
use migration::decode_state;

use super::model::{EventHash, IncarnationGeneration};
use super::{
    Admission, ApprovalHistoryError, EventPageLimit, IdempotencyKey, JobEvent, JobId, JobMutation,
    JobReceipt, JobRecord, JobStatus, JobStoreError, MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES,
    MAX_EVENT_BATCH_LEN, MAX_EVENT_PAYLOAD_BYTES, MAX_JOB_SNAPSHOT_BYTES, RequestDigest,
    ServerIncarnation,
};

const JOBS_DIR: &str = "jobs";
const INITIALIZED_FILE: &str = "initialized.json";
const INITIALIZED_BODY: &str = "{\"schema\":\"nika/job-store-init@1\"}\n";
const LOCK_FILE: &str = "store.lock";
const SERVER_LOCK_FILE: &str = "server.lock";
const STATE_FILE: &str = "state.json";
const LEGACY_STATE_VERSION: u32 = 2;
const STATE_VERSION: u32 = 3;
const EVENT_HASH_DOMAIN: &[u8] = b"nika.job-event.chain\0v1\0";
const IDENTITY_HASH_DOMAIN: &[u8] = b"nika.job-identity.binding\0v1\0";

/// Monotonic authority for one-shot approval-decision history.
///
/// An implementation belongs to a durability domain that an actor able to
/// rewrite the job snapshot cannot coherently roll back. A successful
/// [`record_once`](Self::record_once) durably records the complete batch before
/// returning. If any supplied digest was recorded earlier, the implementation
/// returns [`ApprovalHistoryError::AlreadyRecorded`] and records none of them.
///
/// The history may contain records absent from the job snapshot after a failed
/// snapshot write or a rollback. [`verify_recorded`](Self::verify_recorded)
/// therefore proves containment, not equality.
pub trait ApprovalHistory: Send + Sync + 'static {
    /// Verify that every supplied digest is already in authoritative history.
    ///
    /// # Errors
    /// Returns [`ApprovalHistoryError::MissingRecord`] when any digest is
    /// absent, or [`ApprovalHistoryError::Unavailable`] when no durable verdict
    /// can be made.
    fn verify_recorded(&self, digests: &[RequestDigest]) -> Result<(), ApprovalHistoryError>;

    /// Atomically record a batch only if every digest is globally unused.
    ///
    /// # Errors
    /// Returns [`ApprovalHistoryError::AlreadyRecorded`] without mutation when
    /// any digest was already recorded, or [`ApprovalHistoryError::Unavailable`]
    /// when the durable decision cannot be completed.
    fn record_once(&self, digests: &[RequestDigest]) -> Result<(), ApprovalHistoryError>;
}

/// Descriptor-rooted durable job state.
///
/// Every operation takes both an in-process mutex and a kernel advisory lock,
/// then reloads and validates the snapshot before reading or mutating it. A
/// visible path replacement after [`JobStore::open`] cannot redirect I/O.
pub struct JobStore {
    dir: OwnedDir,
    approval_history: Option<Arc<dyn ApprovalHistory>>,
    local: Mutex<()>,
    fail_fast_lease: bool,
    #[cfg(test)]
    fail_next_persist: AtomicBool,
}

impl fmt::Debug for JobStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JobStore { durable_root: <redacted> }")
    }
}

impl JobStore {
    /// Open a durable job store beneath an existing root directory.
    ///
    /// The `jobs` child is created descriptor-relatively. Existing state is
    /// validated before the store is returned, so truncated or forged state
    /// fails closed at startup.
    ///
    /// # Errors
    /// Returns an error when the root cannot be held safely or existing state
    /// is unreadable, truncated, violates a persisted invariant, or already
    /// contains approvals that require an external history authority.
    pub fn open(root: &Path) -> Result<Self, JobStoreError> {
        Self::open_inner(root, None, false)
    }

    /// Open a store whose kernel lease fails fast as [`JobStoreError::Busy`].
    ///
    /// HTTP's dedicated blocking owner uses this so lock contention cannot pin
    /// a request or shutdown behind an unbounded flock wait. Ordinary callers
    /// keep the blocking [`Self::open`] contract.
    ///
    /// # Errors
    /// Returns the same typed failures as [`Self::open`].
    pub(crate) fn open_fail_fast(root: &Path) -> Result<Self, JobStoreError> {
        Self::open_inner(root, None, true)
    }

    /// Open a durable job store with an external monotonic approval history.
    ///
    /// Approval events fail closed under [`Self::open`]. This constructor is
    /// required to append or reopen them. The authority must live outside the
    /// job snapshot's rollback domain and atomically enforce the
    /// [`ApprovalHistory`] contract.
    ///
    /// # Errors
    /// Returns an error when the root cannot be held safely, existing state is
    /// invalid, or its approval digests are absent from authoritative history.
    pub fn open_with_approval_history(
        root: &Path,
        approval_history: Arc<dyn ApprovalHistory>,
    ) -> Result<Self, JobStoreError> {
        Self::open_inner(root, Some(approval_history), false)
    }

    fn open_inner(
        root: &Path,
        approval_history: Option<Arc<dyn ApprovalHistory>>,
        fail_fast_lease: bool,
    ) -> Result<Self, JobStoreError> {
        let store = Self {
            dir: OwnedDir::create(root, &[JOBS_DIR])?,
            approval_history,
            local: Mutex::new(()),
            fail_fast_lease,
            #[cfg(test)]
            fail_next_persist: AtomicBool::new(false),
        };
        {
            let _local = store.local_guard()?;
            let _lease = store.kernel_lease()?;
            store.initialize_or_load()?;
        }
        Ok(store)
    }

    /// ADR-132 · the RESIDENT becomes the store's writer once it holds the
    /// server lease (the freeze audit moved the stamp after the lease: a
    /// second start that loses the lease never rewrites the live
    /// resident's writer, and a newer protocol's stamp never lands beside
    /// an older resident still serving). The newer-writer refusal fires
    /// at every load (`validate`), open included.
    ///
    /// # Errors
    /// Returns an error when locking, loading or writing fails.
    pub(crate) fn stamp_writer_as_resident(&self) -> Result<(), JobStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        self.stamp_writer()
    }

    /// The stamp itself (a store an older engine wrote is re-stamped; an
    /// unstamped one is stamped). Held under the caller's lease.
    fn stamp_writer(&self) -> Result<(), JobStoreError> {
        let mut state = self.load_state()?;
        let mine = crate::writer::WriterStamp::this_engine();
        if state.writer.as_ref() != Some(&mine) {
            state.writer = Some(mine);
            self.persist(&state)?;
        }
        Ok(())
    }

    /// Create a job or replay the record already bound to this key.
    ///
    /// Reusing a key with another digest yields [`Admission::Conflict`] and
    /// does not mutate durable state.
    ///
    /// # Errors
    /// Returns an error when locking, loading, validation, or durable writing
    /// fails. A conflict is a successful [`Admission`] verdict, not an error.
    pub fn create_or_replay(
        &self,
        key: IdempotencyKey,
        digest: RequestDigest,
    ) -> Result<Admission, JobStoreError> {
        self.create_or_replay_bounded(key, digest, usize::MAX)
    }

    /// Create or replay while refusing a new record above `max_jobs`.
    ///
    /// Existing identical and conflicting bindings remain observable at the
    /// ceiling. Only creation of another durable identity is refused.
    ///
    /// # Errors
    /// Returns [`JobStoreError::CapacityExceeded`] when a new key would make
    /// the durable record count exceed the configured ceiling.
    pub fn create_or_replay_bounded(
        &self,
        key: IdempotencyKey,
        digest: RequestDigest,
        max_jobs: usize,
    ) -> Result<Admission, JobStoreError> {
        self.create_or_replay_named(key, digest, max_jobs, String::new())
    }

    /// Create or replay while recording the contained workflow name.
    ///
    /// The name is the restart schedule: queued jobs with a non-empty
    /// workflow are re-enqueued when the next listener incarnation starts.
    ///
    /// # Errors
    /// Returns the same typed failures as [`Self::create_or_replay_bounded`].
    pub fn create_or_replay_named(
        &self,
        key: IdempotencyKey,
        digest: RequestDigest,
        max_jobs: usize,
        workflow: String,
    ) -> Result<Admission, JobStoreError> {
        self.create_or_replay_inner(key, digest, max_jobs, workflow, None)
    }

    /// Create or replay while persisting the POST-time execution world.
    ///
    /// The sidecar is written under the same exclusive lease as the durable
    /// row so a queued job can be readmitted after restart without recapturing
    /// live files.
    ///
    /// # Errors
    /// Returns the same typed failures as [`Self::create_or_replay_named`].
    pub fn create_or_replay_captured(
        &self,
        key: IdempotencyKey,
        digest: RequestDigest,
        max_jobs: usize,
        workflow: String,
        world: &str,
    ) -> Result<Admission, JobStoreError> {
        self.create_or_replay_inner(key, digest, max_jobs, workflow, Some(world))
    }

    fn create_or_replay_inner(
        &self,
        key: IdempotencyKey,
        digest: RequestDigest,
        max_jobs: usize,
        workflow: String,
        world: Option<&str>,
    ) -> Result<Admission, JobStoreError> {
        key.validate()?;
        digest.validate()?;
        if let Some(world) = world
            && world.len() > MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES
        {
            return Err(JobStoreError::SnapshotTooLarge {
                bytes: world.len() as u64,
                maximum: MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES,
            });
        }
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;

        if let Some(existing) = state
            .jobs
            .iter()
            .find(|job| job.record.idempotency_key == key)
        {
            return Ok(if existing.record.request_digest == digest {
                Admission::Existing(existing.record.clone())
            } else {
                Admission::Conflict(existing.record.clone())
            });
        }

        if state.jobs.len() >= max_jobs {
            return Err(JobStoreError::CapacityExceeded);
        }

        let record = JobRecord {
            id: unique_job_id(&state),
            idempotency_key: key,
            request_digest: digest,
            status: JobStatus::Queued,
            origin: crate::JobOrigin::Manual,
            workflow,
            execution_id: String::new(),
            trace_id: String::new(),
            snapshot_digest: String::new(),
            error_code: String::new(),
            error_message: String::new(),
            outputs: None,
            receipt: None,
            settlement: None,
            paused_outputs: None,
            paused_receipt: None,
        };
        if let Some(world) = world {
            self.dir.write_atomic(&world_file(&record.id), world)?;
        }
        state.jobs.push(StoredJob {
            record: record.clone(),
            events: Vec::new(),
            event_count: 0,
            event_head: None,
            identity_digest: None,
            terminal_sequence: None,
        });
        self.persist(&state)?;
        Ok(Admission::Created(record))
    }

    /// Load the POST-time execution world for one job.
    ///
    /// # Errors
    /// Returns [`JobStoreError::Corrupt`] when the sidecar is absent or not
    /// UTF-8, or an I/O failure from the held directory.
    pub fn load_world(&self, id: &JobId) -> Result<String, JobStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut file = self
            .dir
            .open_relative(Path::new(&world_file(id)))
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    JobStoreError::Corrupt("execution world is missing".to_owned())
                } else {
                    JobStoreError::Io(error.kind())
                }
            })?;
        let mut body = Vec::new();
        std::io::Read::by_ref(&mut file)
            .take(MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES as u64 + 1)
            .read_to_end(&mut body)?;
        if body.len() > MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES {
            return Err(JobStoreError::SnapshotTooLarge {
                bytes: body.len() as u64,
                maximum: MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES,
            });
        }
        String::from_utf8(body)
            .map_err(|_| JobStoreError::Corrupt("execution world is not valid UTF-8".to_owned()))
    }

    /// Persist engine identities minted by snapshot readmission.
    ///
    /// The first stamp wins so a replay cannot rewrite a settled identity.
    ///
    /// # Errors
    /// Returns [`JobStoreError::InvalidReceipt`] for an empty identity,
    /// [`JobStoreError::JobNotFound`] for an unknown job, or a storage failure.
    pub fn stamp_identity(
        &self,
        id: &JobId,
        execution_id: String,
        trace_id: String,
    ) -> Result<JobRecord, JobStoreError> {
        self.stamp_identity_inner(id, execution_id, trace_id, None)
    }

    /// Persist engine identities and the immutable snapshot digest minted by
    /// snapshot readmission.
    ///
    /// The first stamp wins so a replay cannot rewrite a settled identity.
    /// A legacy matching identity may acquire its previously absent digest.
    ///
    /// # Errors
    /// Returns [`JobStoreError::JobNotFound`],
    /// [`JobStoreError::InvalidReceipt`],
    /// [`JobStoreError::ReceiptIdentityMismatch`], or a storage failure.
    pub fn stamp_execution_identity(
        &self,
        id: &JobId,
        execution_id: String,
        trace_id: String,
        snapshot_digest: String,
    ) -> Result<JobRecord, JobStoreError> {
        if validate_snapshot_digest(&snapshot_digest).is_err() {
            return Err(JobStoreError::InvalidReceipt);
        }
        self.stamp_identity_inner(id, execution_id, trace_id, Some(snapshot_digest))
    }

    fn stamp_identity_inner(
        &self,
        id: &JobId,
        execution_id: String,
        trace_id: String,
        snapshot_digest: Option<String>,
    ) -> Result<JobRecord, JobStoreError> {
        if execution_id.is_empty() || trace_id.is_empty() {
            return Err(JobStoreError::InvalidReceipt);
        }
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        let job = state
            .jobs
            .iter_mut()
            .find(|job| job.record.id == *id)
            .ok_or_else(|| JobStoreError::JobNotFound(id.clone()))?;
        if job.record.execution_id.is_empty() {
            job.record.execution_id = execution_id;
            job.record.trace_id = trace_id;
            if let Some(snapshot_digest) = snapshot_digest {
                job.record.snapshot_digest = snapshot_digest;
            }
            job.identity_digest = Some(hash_execution_identity(&job.record)?);
            let record = job.record.clone();
            self.persist(&state)?;
            return Ok(record);
        }
        if job.record.execution_id != execution_id || job.record.trace_id != trace_id {
            return Err(JobStoreError::ReceiptIdentityMismatch);
        }
        if job.record.snapshot_digest.is_empty()
            && let Some(snapshot_digest) = snapshot_digest
        {
            job.record.snapshot_digest = snapshot_digest;
            job.identity_digest = Some(hash_execution_identity(&job.record)?);
            let record = job.record.clone();
            self.persist(&state)?;
            return Ok(record);
        }
        if let Some(snapshot_digest) = snapshot_digest
            && job.record.snapshot_digest != snapshot_digest
        {
            return Err(JobStoreError::ReceiptIdentityMismatch);
        }
        Ok(job.record.clone())
    }

    /// Read a job record by opaque id.
    ///
    /// # Errors
    /// Returns an error when the store cannot be locked or validated.
    pub fn get(&self, id: &JobId) -> Result<Option<JobRecord>, JobStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let state = self.load_state()?;
        Ok(state
            .jobs
            .iter()
            .find(|job| job.record.id == *id)
            .map(|job| job.record.clone()))
    }

    /// Return queued jobs that still name a workflow to reschedule.
    ///
    /// # Errors
    /// Returns an error when the store cannot be locked or validated.
    pub fn queued_jobs(&self) -> Result<Vec<(JobId, String)>, JobStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let state = self.load_state()?;
        Ok(state
            .jobs
            .iter()
            .filter(|job| job.record.status == JobStatus::Queued && !job.record.workflow.is_empty())
            .map(|job| (job.record.id.clone(), job.record.workflow.clone()))
            .collect())
    }

    /// Apply one legal lifecycle transition and its events in one replacement.
    ///
    /// # Errors
    /// Returns [`JobStoreError::JobNotFound`] for an unknown id,
    /// [`JobStoreError::IllegalTransition`] for a forbidden edge, or a storage
    /// error. Forbidden edges do not mutate the snapshot.
    pub fn transition_with_events(
        &self,
        id: &JobId,
        next: JobStatus,
        payloads: &[Value],
    ) -> Result<JobMutation, JobStoreError> {
        self.transition_inner(id, next, payloads, None, None, None)
    }

    /// Atomically settle a job with declared outputs, receipt, and events.
    ///
    /// `outputs` is optional so an older execution adapter can honestly leave
    /// the field absent instead of fabricating a result. A present empty map
    /// means the workflow declared or resolved no outputs.
    ///
    /// # Errors
    /// Returns the transition/storage failures from
    /// [`Self::transition_with_events`], or a typed receipt failure when the
    /// supplied binding does not match the job's stamped identity.
    pub fn settle_with_events(
        &self,
        id: &JobId,
        next: JobStatus,
        payloads: &[Value],
        outputs: Option<BTreeMap<String, Value>>,
        receipt: Option<JobReceipt>,
    ) -> Result<JobMutation, JobStoreError> {
        if !next.is_settled() {
            return Err(JobStoreError::InvalidReceipt);
        }
        self.transition_inner(id, next, payloads, outputs, receipt, None)
    }

    fn transition_inner(
        &self,
        id: &JobId,
        next: JobStatus,
        payloads: &[Value],
        outputs: Option<BTreeMap<String, Value>>,
        receipt: Option<JobReceipt>,
        expected: Option<JobStatus>,
    ) -> Result<JobMutation, JobStoreError> {
        let batch = ValidatedEventBatch::for_transition(payloads)?;
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        ensure_approval_claims_unused(&state, &batch)?;
        let job = state
            .jobs
            .iter_mut()
            .find(|job| job.record.id == *id)
            .ok_or_else(|| JobStoreError::JobNotFound(id.clone()))?;
        let current = job.record.status;
        if expected.is_some_and(|status| current != status) || !current.allows(next) {
            return Err(JobStoreError::IllegalTransition {
                from: current,
                to: next,
            });
        }
        if next.is_settled()
            && !job.record.execution_id.is_empty()
            && (!has_complete_execution_identity(&job.record) || receipt.is_none())
        {
            return Err(JobStoreError::InvalidReceipt);
        }
        if let Some(receipt) = &receipt {
            receipt.validate()?;
            ensure_receipt_matches(&job.record, receipt)?;
        }
        job.record.status = next;
        copy_error_from_payloads(&mut job.record, payloads);
        job.record.outputs = outputs;
        job.record.receipt = receipt;
        job.terminal_sequence = if next.is_settled() {
            Some(job.final_sequence_after(&batch)?)
        } else {
            None
        };
        let events = job.append_payloads(&batch)?;
        let record = job.record.clone();
        self.persist_event_mutation(&state, &batch)?;
        Ok(JobMutation { record, events })
    }

    /// Append payloads with strictly increasing per-job sequence numbers.
    ///
    /// # Errors
    /// Returns an error for an unknown job, exhausted sequence, invalid
    /// snapshot, or failed durable write.
    pub fn append_events(
        &self,
        id: &JobId,
        payloads: &[Value],
    ) -> Result<Vec<JobEvent>, JobStoreError> {
        let batch = ValidatedEventBatch::new(payloads)?;
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        ensure_approval_claims_unused(&state, &batch)?;
        let job = state
            .jobs
            .iter_mut()
            .find(|job| job.record.id == *id)
            .ok_or_else(|| JobStoreError::JobNotFound(id.clone()))?;
        let appended = job.append_payloads(&batch)?;
        if !appended.is_empty() {
            self.persist_event_mutation(&state, &batch)?;
        }
        Ok(appended)
    }

    /// Return events whose sequence is greater than the supplied cursor.
    ///
    /// # Errors
    /// Returns an error for an unknown job, a cursor beyond the latest durable
    /// sequence, or a store that cannot be locked and validated.
    pub fn events_after(
        &self,
        id: &JobId,
        after: u64,
        limit: EventPageLimit,
    ) -> Result<Vec<JobEvent>, JobStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let state = self.load_state()?;
        let job = state
            .jobs
            .iter()
            .find(|job| job.record.id == *id)
            .ok_or_else(|| JobStoreError::JobNotFound(id.clone()))?;
        let latest = job.events.last().map_or(0, |event| event.sequence);
        if after > latest {
            return Err(JobStoreError::CursorBeyondLatest {
                job: id.clone(),
                after,
                latest,
            });
        }
        Ok(job
            .events
            .iter()
            .filter(|event| event.sequence > after)
            .take(limit.get())
            .cloned()
            .collect())
    }

    pub(crate) fn event_page(
        &self,
        id: &JobId,
        after: u64,
        limit: EventPageLimit,
    ) -> Result<(Vec<JobEvent>, JobRecord, Option<u64>), JobStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let state = self.load_state()?;
        let job = state
            .jobs
            .iter()
            .find(|job| job.record.id == *id)
            .ok_or_else(|| JobStoreError::JobNotFound(id.clone()))?;
        let latest = job.events.last().map_or(0, |event| event.sequence);
        if after > latest {
            return Err(JobStoreError::CursorBeyondLatest {
                job: id.clone(),
                after,
                latest,
            });
        }
        let events = job
            .events
            .iter()
            .filter(|event| event.sequence > after)
            .take(limit.get())
            .cloned()
            .collect();
        Ok((events, job.record.clone(), job.terminal_sequence))
    }

    fn local_guard(&self) -> Result<MutexGuard<'_, ()>, JobStoreError> {
        self.local.lock().map_err(|_| JobStoreError::LockPoisoned)
    }

    pub(super) fn kernel_lease(&self) -> Result<Flock<std::fs::File>, JobStoreError> {
        let file = self.dir.open_lock(LOCK_FILE)?;
        let mode = if self.fail_fast_lease {
            FlockArg::LockExclusiveNonblock
        } else {
            FlockArg::LockExclusive
        };
        Flock::lock(file, mode).map_err(|(_file, errno)| {
            if self.fail_fast_lease
                && (errno == nix::errno::Errno::EAGAIN || errno == nix::errno::Errno::EWOULDBLOCK)
            {
                JobStoreError::Busy
            } else {
                JobStoreError::from(std::io::Error::from_raw_os_error(errno as i32))
            }
        })
    }

    #[cfg(test)]
    pub(super) fn try_kernel_lease(&self) -> Result<Option<Flock<std::fs::File>>, JobStoreError> {
        let file = self.dir.open_lock(LOCK_FILE)?;
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(lease) => Ok(Some(lease)),
            Err((_file, nix::errno::Errno::EAGAIN)) => Ok(None),
            Err((_file, errno)) => Err(JobStoreError::from(std::io::Error::from_raw_os_error(
                errno as i32,
            ))),
        }
    }

    fn initialize_or_load(&self) -> Result<(), JobStoreError> {
        let marker = self.dir.read_optional(INITIALIZED_FILE)?;
        let state = self.read_state_optional()?;
        match (marker.as_deref(), state.as_deref()) {
            (None, None) => {
                self.persist(&PersistedState::new())?;
                self.dir.write_atomic(INITIALIZED_FILE, INITIALIZED_BODY)?;
                Ok(())
            }
            (Some(marker), Some(state)) => {
                validate_initialization_marker(marker)?;
                let decoded = decode_state(state)?;
                self.verify_approval_history(&decoded.state)?;
                if decoded.migrated {
                    self.persist(&decoded.state)?;
                }
                Ok(())
            }
            (Some(marker), None) => {
                validate_initialization_marker(marker)?;
                Err(missing_state_error())
            }
            (None, Some(_)) => Err(JobStoreError::Corrupt(
                "initialization marker is missing".to_owned(),
            )),
        }
    }

    /// Claim a server-wide lease and durably bind a new incarnation generation.
    ///
    /// The returned capability owns `server.lock` until drop. This is
    /// crate-private because only W06's bootstrap may claim it before binding.
    ///
    /// # Errors
    /// Returns an error when another live server owns the lease, or when
    /// locking, loading, generation allocation, or durable writing fails.
    pub(crate) fn claim_server_incarnation(&self) -> Result<ServerIncarnation, JobStoreError> {
        let server_lease = self.server_lease()?;
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        let generation = state.incarnation.claim()?;
        self.persist(&state)?;
        Ok(ServerIncarnation {
            generation,
            _lease: server_lease,
        })
    }

    /// Settle ownerless running jobs once for a leased server incarnation.
    ///
    /// The capability argument cannot be constructed outside `nika-serve`.
    /// W06's server bootstrap owns it after proving exclusivity and must call
    /// this method before exposing the store.
    ///
    /// # Errors
    /// Returns an error when locking, loading, validation, or durable writing
    /// fails.
    pub fn settle_interrupted_jobs(
        &self,
        incarnation: &ServerIncarnation,
    ) -> Result<usize, JobStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        if state.incarnation.current != incarnation.generation {
            return Err(JobStoreError::StaleServerIncarnation);
        }
        if state.incarnation.settled == Some(incarnation.generation) {
            return Ok(0);
        }
        let prior_generation = state.incarnation.settled.map(IncarnationGeneration::get);
        let current_generation = incarnation.generation.get();
        let mut settled = 0;
        for job in &mut state.jobs {
            if job.record.status == JobStatus::Running {
                if has_complete_execution_identity(&job.record) {
                    let payload = serde_json::json!({
                        "incarnation_generation": current_generation,
                        "kind": "interrupted",
                        "previous_incarnation_generation": prior_generation,
                        "status": "interrupted",
                    });
                    let batch =
                        ValidatedEventBatch::for_transition(std::slice::from_ref(&payload))?;
                    job.record.status = JobStatus::Interrupted;
                    attach_interrupted_receipt(&mut job.record)?;
                    job.terminal_sequence = Some(job.final_sequence_after(&batch)?);
                    job.append_payloads(&batch)?;
                    settled += 1;
                } else {
                    if job.record.workflow.is_empty() {
                        return Err(JobStoreError::Corrupt(
                            "running job lacks both durable execution identity and restart schedule"
                                .to_owned(),
                        ));
                    }
                    let payload = serde_json::json!({
                        "incarnation_generation": current_generation,
                        "kind": "execution.requeued",
                        "previous_incarnation_generation": prior_generation,
                        "status": "queued",
                    });
                    let batch =
                        ValidatedEventBatch::for_transition(std::slice::from_ref(&payload))?;
                    job.record.status = JobStatus::Queued;
                    job.record.outputs = None;
                    job.record.receipt = None;
                    job.terminal_sequence = None;
                    job.append_payloads(&batch)?;
                }
            }
        }
        state.incarnation.settled = Some(incarnation.generation);
        self.persist(&state)?;
        Ok(settled)
    }

    /// Mark one running job interrupted under the live server incarnation.
    ///
    /// This is the crate-internal live-timeout edge. Public
    /// [`Self::transition_with_events`] still refuses `running -> interrupted`.
    ///
    /// # Errors
    /// Returns a stale-incarnation, missing-job, illegal-status, or storage
    /// failure. Forbidden edges do not mutate the snapshot.
    pub(crate) fn interrupt_running(
        &self,
        id: &JobId,
        incarnation: &ServerIncarnation,
        payload: &Value,
    ) -> Result<JobRecord, JobStoreError> {
        let batch = ValidatedEventBatch::for_transition(std::slice::from_ref(payload))?;
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        if state.incarnation.current != incarnation.generation {
            return Err(JobStoreError::StaleServerIncarnation);
        }
        ensure_approval_claims_unused(&state, &batch)?;
        let job = state
            .jobs
            .iter_mut()
            .find(|job| job.record.id == *id)
            .ok_or_else(|| JobStoreError::JobNotFound(id.clone()))?;
        if job.record.status != JobStatus::Running {
            return Err(JobStoreError::IllegalTransition {
                from: job.record.status,
                to: JobStatus::Interrupted,
            });
        }
        job.record.status = JobStatus::Interrupted;
        attach_interrupted_receipt(&mut job.record)?;
        job.terminal_sequence = Some(job.final_sequence_after(&batch)?);
        job.append_payloads(&batch)?;
        let record = job.record.clone();
        self.persist_event_mutation(&state, &batch)?;
        Ok(record)
    }

    fn server_lease(&self) -> Result<Flock<std::fs::File>, JobStoreError> {
        let file = self.dir.open_lock(SERVER_LOCK_FILE)?;
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(lease) => Ok(lease),
            Err((_file, nix::errno::Errno::EAGAIN)) => Err(JobStoreError::ServerLeaseHeld),
            Err((_file, errno)) => Err(JobStoreError::from(std::io::Error::from_raw_os_error(
                errno as i32,
            ))),
        }
    }

    pub(super) fn load_state(&self) -> Result<PersistedState, JobStoreError> {
        let marker = self
            .dir
            .read_optional(INITIALIZED_FILE)?
            .ok_or_else(|| JobStoreError::Corrupt("initialization marker is missing".to_owned()))?;
        validate_initialization_marker(&marker)?;
        let text = self
            .read_state_optional()?
            .ok_or_else(missing_state_error)?;
        let mut state = decode_state(&text)?.state;
        self.verify_approval_history(&state)?;
        for job in &mut state.jobs {
            job.project_settlement()?;
        }
        Ok(state)
    }

    fn persist(&self, state: &PersistedState) -> Result<(), JobStoreError> {
        let body = prepare_snapshot(state)?;
        self.write_snapshot(&body)
    }

    fn persist_event_mutation(
        &self,
        state: &PersistedState,
        batch: &ValidatedEventBatch<'_>,
    ) -> Result<(), JobStoreError> {
        let body = prepare_snapshot(state)?;
        let claims = batch.approval_digests()?;
        if !claims.is_empty() {
            let history = self
                .approval_history
                .as_ref()
                .ok_or(JobStoreError::ApprovalHistoryRequired)?;
            history.record_once(&claims)?;
        }
        self.write_snapshot(&body)
    }

    fn write_snapshot(&self, body: &EncodedSnapshot) -> Result<(), JobStoreError> {
        #[cfg(test)]
        if self.fail_next_persist.swap(false, Ordering::AcqRel) {
            return Err(std::io::Error::other("injected durable write failure").into());
        }
        self.dir.write_atomic(STATE_FILE, body.as_str())?;
        Ok(())
    }

    fn verify_approval_history(&self, state: &PersistedState) -> Result<(), JobStoreError> {
        let claims = state.approval_claims()?;
        if claims.is_empty() {
            return Ok(());
        }
        let history = self
            .approval_history
            .as_ref()
            .ok_or(JobStoreError::ApprovalHistoryRequired)?;
        history.verify_recorded(&claims)?;
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn fail_next_persist(&self) {
        self.fail_next_persist.store(true, Ordering::Release);
    }

    fn read_state_optional(&self) -> Result<Option<String>, JobStoreError> {
        let mut file = match self.dir.open_relative(Path::new(STATE_FILE)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let bytes = file.metadata()?.len();
        if bytes > MAX_JOB_SNAPSHOT_BYTES as u64 {
            return Err(snapshot_too_large(bytes));
        }
        let mut body = Vec::new();
        std::io::Read::by_ref(&mut file)
            .take(MAX_JOB_SNAPSHOT_BYTES as u64 + 1)
            .read_to_end(&mut body)?;
        if body.len() > MAX_JOB_SNAPSHOT_BYTES {
            return Err(snapshot_too_large(body.len() as u64));
        }
        String::from_utf8(body)
            .map(Some)
            .map_err(|_| JobStoreError::Corrupt("state is not valid UTF-8".to_owned()))
    }
}

// Compile-time pin for the crate-private W06 bootstrap seam. Keeping the
// authority non-public is more important than making it externally callable
// before the listener module lands.
#[cfg(not(test))]
const _: fn(&JobStore) -> Result<ServerIncarnation, JobStoreError> =
    JobStore::claim_server_incarnation;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedState {
    version: u32,
    incarnation: IncarnationLedger,
    pub(super) jobs: Vec<StoredJob>,
    /// The engine that last wrote this store (ADR-132 · #1352) — absent on
    /// a store written before the stamp existed.
    #[serde(default)]
    writer: Option<crate::writer::WriterStamp>,
}

impl PersistedState {
    fn new() -> Self {
        Self {
            version: STATE_VERSION,
            incarnation: IncarnationLedger::new(),
            jobs: Vec::new(),
            writer: Some(crate::writer::WriterStamp::this_engine()),
        }
    }

    fn validate(&self) -> Result<(), JobStoreError> {
        if self.version != STATE_VERSION {
            return Err(JobStoreError::Corrupt(
                "state version is unsupported".to_owned(),
            ));
        }
        // ADR-132 · #1352 · a store last written by a NEWER protocol is not
        // ours to reinterpret: fail closed, name both engines.
        if let Some(reason) = self
            .writer
            .as_ref()
            .and_then(crate::writer::WriterStamp::newer_than_this_engine)
        {
            return Err(JobStoreError::WrittenByNewerEngine(reason));
        }
        self.incarnation.validate()?;
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut approval_claims = BTreeSet::new();
        for job in &self.jobs {
            job.record.id.validate()?;
            job.record.idempotency_key.validate()?;
            job.record.request_digest.validate()?;
            validate_terminal_record(&job.record)?;
            validate_identity_binding(job)?;
            if !ids.insert(job.record.id.clone()) {
                return Err(JobStoreError::Corrupt("duplicate job id".to_owned()));
            }
            if !keys.insert(job.record.idempotency_key.clone()) {
                return Err(JobStoreError::Corrupt(
                    "duplicate idempotency key".to_owned(),
                ));
            }
            validate_events(job)?;
            for event in &job.events {
                if let Some(digest) = approval_digest(&event.payload)
                    && !approval_claims.insert(digest)
                {
                    return Err(JobStoreError::Corrupt(
                        "approval claim digest is duplicated".to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn approval_claims(&self) -> Result<Vec<RequestDigest>, JobStoreError> {
        self.jobs
            .iter()
            .flat_map(|job| &job.events)
            .filter_map(|event| approval_digest(&event.payload))
            .map(RequestDigest::new)
            .collect()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IncarnationLedger {
    current: IncarnationGeneration,
    settled: Option<IncarnationGeneration>,
}

impl IncarnationLedger {
    const fn new() -> Self {
        Self {
            current: IncarnationGeneration::INITIAL,
            settled: None,
        }
    }

    fn claim(&mut self) -> Result<IncarnationGeneration, JobStoreError> {
        let generation = self
            .current
            .next()
            .ok_or(JobStoreError::IncarnationGenerationExhausted)?;
        self.current = generation;
        Ok(generation)
    }

    fn validate(&self) -> Result<(), JobStoreError> {
        if self
            .settled
            .is_some_and(|settled| settled.get() == 0 || settled > self.current)
        {
            return Err(JobStoreError::Corrupt(
                "incarnation settlement generation is invalid".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredJob {
    record: JobRecord,
    events: Vec<JobEvent>,
    event_count: u64,
    event_head: Option<EventHash>,
    identity_digest: Option<EventHash>,
    terminal_sequence: Option<u64>,
}

fn world_file(id: &JobId) -> String {
    format!("{}.world", id.as_str())
}

fn unique_job_id(state: &PersistedState) -> JobId {
    loop {
        let candidate = JobId::random();
        if state.jobs.iter().all(|job| job.record.id != candidate) {
            return candidate;
        }
    }
}

fn validate_events(job: &StoredJob) -> Result<(), JobStoreError> {
    if usize::try_from(job.event_count).ok() != Some(job.events.len())
        || job.event_head.as_ref() != job.events.last().map(|event| &event.hash)
    {
        return Err(JobStoreError::Corrupt(
            "event chain head or count does not match its journal".to_owned(),
        ));
    }
    if job.record.status.is_settled() {
        let terminal = job.terminal_sequence.ok_or_else(|| {
            JobStoreError::Corrupt("terminal job is missing its event sequence".to_owned())
        })?;
        if terminal == 0 || terminal > job.event_count {
            return Err(JobStoreError::Corrupt(
                "terminal event sequence is outside the journal".to_owned(),
            ));
        }
    } else if job.terminal_sequence.is_some() {
        return Err(JobStoreError::Corrupt(
            "unsettled job carries a terminal event sequence".to_owned(),
        ));
    }
    let mut previous: Option<&EventHash> = None;
    for (index, event) in job.events.iter().enumerate() {
        let expected = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| JobStoreError::Corrupt("event sequence exceeds u64".to_owned()))?;
        if event.sequence != expected {
            return Err(JobStoreError::Corrupt(
                "event sequence is not contiguous".to_owned(),
            ));
        }
        event.hash.validate()?;
        stored_job::validate_pause_payload(&event.payload, &job.record.id)?;
        if event.previous_hash.as_ref() != previous {
            return Err(JobStoreError::Corrupt(
                "event chain predecessor does not match".to_owned(),
            ));
        }
        validate_approval_event(&event.payload)
            .map_err(|_| JobStoreError::Corrupt("approval event digest is invalid".to_owned()))?;
        let expected_hash = hash_event(
            &job.record,
            job.terminal_sequence,
            event.sequence,
            event.previous_hash.as_ref(),
            &event.payload,
        )?;
        if event.hash != expected_hash {
            return Err(JobStoreError::Corrupt(
                "event chain hash does not match its canonical preimage".to_owned(),
            ));
        }
        previous = Some(&event.hash);
    }
    Ok(())
}

fn hash_event(
    record: &JobRecord,
    terminal_sequence: Option<u64>,
    sequence: u64,
    previous_hash: Option<&EventHash>,
    payload: &Value,
) -> Result<EventHash, JobStoreError> {
    let preimage = if terminal_sequence == Some(sequence) {
        serde_json::json!({
            "job_id": record.id.as_str(),
            "payload": payload,
            "previous_hash": previous_hash.map(EventHash::as_str),
            "request_digest": record.request_digest.as_str(),
            "sequence": sequence,
            "terminal_binding": {
                "execution_id": &record.execution_id,
                "outputs": &record.outputs,
                "receipt": &record.receipt,
                "snapshot_digest": &record.snapshot_digest,
                "status": record.status,
                "trace_id": &record.trace_id,
            },
        })
    } else {
        serde_json::json!({
            "job_id": record.id.as_str(),
            "payload": payload,
            "previous_hash": previous_hash.map(EventHash::as_str),
            "request_digest": record.request_digest.as_str(),
            "sequence": sequence,
        })
    };
    let canonical = serde_json::to_vec(&preimage)
        .map_err(|_| JobStoreError::Corrupt("event preimage cannot be encoded".to_owned()))?;
    let mut hasher = Sha256::new();
    hasher.update(EVENT_HASH_DOMAIN);
    hasher.update(canonical);
    Ok(EventHash::from_bytes(hasher.finalize().into()))
}

fn validate_initialization_marker(marker: &str) -> Result<(), JobStoreError> {
    if marker != INITIALIZED_BODY {
        return Err(JobStoreError::Corrupt(
            "initialization marker is invalid".to_owned(),
        ));
    }
    Ok(())
}

fn missing_state_error() -> JobStoreError {
    JobStoreError::Corrupt("state file is missing after initialization".to_owned())
}

struct ValidatedEventBatch<'a> {
    payloads: &'a [Value],
    transition: bool,
}

impl<'a> ValidatedEventBatch<'a> {
    fn new(payloads: &'a [Value]) -> Result<Self, JobStoreError> {
        if payloads.len() > MAX_EVENT_BATCH_LEN {
            return Err(JobStoreError::EventBatchTooLarge {
                count: payloads.len(),
                maximum: MAX_EVENT_BATCH_LEN,
            });
        }
        for (index, payload) in payloads.iter().enumerate() {
            let bytes = encoded_payload_len(payload)?;
            // A paused result lives in its immutable event, whereas a final
            // result lives in the record. Both obey the whole-store bound;
            // moving the result must not shrink outputs to an ordinary event.
            let maximum = if payload["kind"] == "execution.settled"
                && payload["status"] == "paused"
                && payload.get("outputs").is_some()
            {
                MAX_JOB_SNAPSHOT_BYTES
            } else {
                MAX_EVENT_PAYLOAD_BYTES
            };
            if bytes > maximum {
                return Err(JobStoreError::EventPayloadTooLarge {
                    index,
                    bytes,
                    maximum,
                });
            }
            validate_approval_event(payload)?;
        }
        Ok(Self {
            payloads,
            transition: false,
        })
    }

    fn for_transition(payloads: &'a [Value]) -> Result<Self, JobStoreError> {
        if payloads.is_empty() {
            return Err(JobStoreError::TransitionEventRequired);
        }
        let mut batch = Self::new(payloads)?;
        batch.transition = true;
        Ok(batch)
    }

    fn payloads(&self) -> &'a [Value] {
        self.payloads
    }

    fn len(&self) -> usize {
        self.payloads.len()
    }

    fn approval_digests(&self) -> Result<Vec<RequestDigest>, JobStoreError> {
        self.payloads
            .iter()
            .filter_map(approval_digest)
            .map(RequestDigest::new)
            .collect()
    }
}

fn validate_approval_event(payload: &Value) -> Result<(), JobStoreError> {
    if payload.get("kind").and_then(Value::as_str) != Some("approval_decided") {
        return Ok(());
    }
    let valid = approval_digest(payload).is_some_and(|digest| RequestDigest::new(digest).is_ok());
    if !valid {
        return Err(JobStoreError::InvalidApprovalEvent);
    }
    Ok(())
}

fn approval_digest(payload: &Value) -> Option<&str> {
    if payload.get("kind").and_then(Value::as_str) != Some("approval_decided") {
        return None;
    }
    payload.get("digest").and_then(Value::as_str)
}

fn ensure_approval_claims_unused(
    state: &PersistedState,
    batch: &ValidatedEventBatch<'_>,
) -> Result<(), JobStoreError> {
    let mut claims = state
        .jobs
        .iter()
        .flat_map(|job| &job.events)
        .filter_map(|event| approval_digest(&event.payload))
        .collect::<BTreeSet<_>>();
    for digest in batch.payloads().iter().filter_map(approval_digest) {
        if !claims.insert(digest) {
            return Err(JobStoreError::ApprovalClaimAlreadyRecorded);
        }
    }
    Ok(())
}

#[derive(Default)]
struct EncodedByteCounter(usize);

impl std::io::Write for EncodedByteCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("encoded event length overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn encoded_payload_len(payload: &Value) -> Result<usize, JobStoreError> {
    let mut counter = EncodedByteCounter::default();
    serde_json::to_writer(&mut counter, payload)
        .map_err(|_| JobStoreError::Corrupt("event payload cannot be encoded".to_owned()))?;
    counter.flush()?;
    Ok(counter.0)
}

struct EncodedSnapshot(String);

fn prepare_snapshot(state: &PersistedState) -> Result<EncodedSnapshot, JobStoreError> {
    state.validate()?;
    EncodedSnapshot::new(state)
}

impl EncodedSnapshot {
    fn new(state: &PersistedState) -> Result<Self, JobStoreError> {
        let mut body = serde_json::to_string(state)
            .map_err(|_| JobStoreError::Corrupt("state cannot be encoded".to_owned()))?;
        body.push('\n');
        if body.len() > MAX_JOB_SNAPSHOT_BYTES {
            return Err(snapshot_too_large(body.len() as u64));
        }
        Ok(Self(body))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

fn snapshot_too_large(bytes: u64) -> JobStoreError {
    JobStoreError::SnapshotTooLarge {
        bytes,
        maximum: MAX_JOB_SNAPSHOT_BYTES,
    }
}

fn copy_error_from_payloads(record: &mut JobRecord, payloads: &[Value]) {
    if !record.error_code.is_empty() {
        return;
    }
    for payload in payloads {
        let Some(code) = payload
            .get("code")
            .and_then(Value::as_str)
            .filter(|code| !code.is_empty())
        else {
            continue;
        };
        let message = payload.get("message").and_then(Value::as_str).unwrap_or("");
        record.error_code = bound_token(code, 64);
        record.error_message = bound_message(message);
        return;
    }
}

fn bound_token(raw: &str, max: usize) -> String {
    raw.chars()
        .filter(char::is_ascii_graphic)
        .take(max)
        .collect()
}

fn bound_message(raw: &str) -> String {
    let mut out = String::new();
    for token in raw.split_whitespace() {
        if token.starts_with('/') || token.contains(":\\") {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(token);
        if out.len() >= 240 {
            out.truncate(240);
            break;
        }
    }
    out
}
