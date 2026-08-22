use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use nika_fs::OwnedDir;
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    Admission, IdempotencyKey, JobEvent, JobId, JobRecord, JobStatus, JobStoreError, RequestDigest,
    ServerIncarnation,
};

const JOBS_DIR: &str = "jobs";
const INITIALIZED_FILE: &str = "initialized.json";
const INITIALIZED_BODY: &str = "{\"schema\":\"nika/job-store-init@1\"}\n";
const LOCK_FILE: &str = "store.lock";
const STATE_FILE: &str = "state.json";
const STATE_VERSION: u32 = 1;

/// Descriptor-rooted durable job state.
///
/// Every operation takes both an in-process mutex and a kernel advisory lock,
/// then reloads and validates the snapshot before reading or mutating it. A
/// visible path replacement after [`JobStore::open`] cannot redirect I/O.
#[derive(Debug)]
pub struct JobStore {
    dir: OwnedDir,
    local: Mutex<()>,
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
    /// is unreadable, truncated, or violates a persisted invariant.
    pub fn open(root: &Path) -> Result<Self, JobStoreError> {
        let store = Self {
            dir: OwnedDir::create(root, &[JOBS_DIR])?,
            local: Mutex::new(()),
        };
        {
            let _local = store.local_guard()?;
            let _lease = store.kernel_lease()?;
            store.initialize_or_load()?;
        }
        Ok(store)
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
        key.validate()?;
        digest.validate()?;
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

        let record = JobRecord {
            id: unique_job_id(&state),
            idempotency_key: key,
            request_digest: digest,
            status: JobStatus::Queued,
        };
        state.jobs.push(StoredJob {
            record: record.clone(),
            events: Vec::new(),
        });
        self.persist(&state)?;
        Ok(Admission::Created(record))
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

    /// Apply one legal lifecycle transition and durably return the new record.
    ///
    /// # Errors
    /// Returns [`JobStoreError::JobNotFound`] for an unknown id,
    /// [`JobStoreError::IllegalTransition`] for a forbidden edge, or a storage
    /// error. Forbidden edges do not mutate the snapshot.
    pub fn transition(&self, id: &JobId, next: JobStatus) -> Result<JobRecord, JobStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        let job = state
            .jobs
            .iter_mut()
            .find(|job| job.record.id == *id)
            .ok_or_else(|| JobStoreError::JobNotFound(id.clone()))?;
        let current = job.record.status;
        if !current.allows(next) {
            return Err(JobStoreError::IllegalTransition {
                from: current,
                to: next,
            });
        }
        job.record.status = next;
        let record = job.record.clone();
        self.persist(&state)?;
        Ok(record)
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
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        let job = state
            .jobs
            .iter_mut()
            .find(|job| job.record.id == *id)
            .ok_or_else(|| JobStoreError::JobNotFound(id.clone()))?;
        let mut next = job
            .events
            .last()
            .map_or(Ok(1), |event| event.sequence.checked_add(1).ok_or(()))
            .map_err(|()| JobStoreError::SequenceExhausted(id.clone()))?;
        let mut appended = Vec::with_capacity(payloads.len());
        for payload in payloads {
            let event = JobEvent {
                sequence: next,
                payload: payload.clone(),
            };
            next = next
                .checked_add(1)
                .ok_or_else(|| JobStoreError::SequenceExhausted(id.clone()))?;
            job.events.push(event.clone());
            appended.push(event);
        }
        if !appended.is_empty() {
            self.persist(&state)?;
        }
        Ok(appended)
    }

    /// Return events whose sequence is greater than the supplied cursor.
    ///
    /// # Errors
    /// Returns an error for an unknown job, a cursor beyond the latest durable
    /// sequence, or a store that cannot be locked and validated.
    pub fn events_after(&self, id: &JobId, after: u64) -> Result<Vec<JobEvent>, JobStoreError> {
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
            .cloned()
            .collect())
    }

    fn local_guard(&self) -> Result<MutexGuard<'_, ()>, JobStoreError> {
        self.local.lock().map_err(|_| JobStoreError::LockPoisoned)
    }

    pub(super) fn kernel_lease(&self) -> Result<Flock<std::fs::File>, JobStoreError> {
        let file = self.dir.open_lock(LOCK_FILE)?;
        Flock::lock(file, FlockArg::LockExclusive).map_err(|(_file, errno)| {
            JobStoreError::from(std::io::Error::from_raw_os_error(errno as i32))
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
        let state = self.dir.read_optional(STATE_FILE)?;
        match (marker.as_deref(), state.as_deref()) {
            (None, None) => {
                self.persist(&PersistedState::new())?;
                self.dir.write_atomic(INITIALIZED_FILE, INITIALIZED_BODY)?;
                Ok(())
            }
            (Some(marker), Some(state)) => {
                validate_initialization_marker(marker)?;
                decode_state(state).map(|_| ())
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

    /// Settle ownerless running jobs for a new exclusive server incarnation.
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
        _incarnation: &ServerIncarnation,
    ) -> Result<usize, JobStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        let mut settled = 0;
        for job in &mut state.jobs {
            if job.record.status == JobStatus::Running {
                job.record.status = JobStatus::Interrupted;
                settled += 1;
            }
        }
        if settled != 0 {
            self.persist(&state)?;
        }
        Ok(settled)
    }

    pub(super) fn load_state(&self) -> Result<PersistedState, JobStoreError> {
        let marker = self
            .dir
            .read_optional(INITIALIZED_FILE)?
            .ok_or_else(|| JobStoreError::Corrupt("initialization marker is missing".to_owned()))?;
        validate_initialization_marker(&marker)?;
        let text = self
            .dir
            .read_optional(STATE_FILE)?
            .ok_or_else(missing_state_error)?;
        decode_state(&text)
    }

    fn persist(&self, state: &PersistedState) -> Result<(), JobStoreError> {
        state.validate()?;
        let mut body = serde_json::to_string(state)
            .map_err(|_| JobStoreError::Corrupt("state cannot be encoded".to_owned()))?;
        body.push('\n');
        self.dir.write_atomic(STATE_FILE, &body)?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedState {
    version: u32,
    pub(super) jobs: Vec<StoredJob>,
}

impl PersistedState {
    fn new() -> Self {
        Self {
            version: STATE_VERSION,
            jobs: Vec::new(),
        }
    }

    fn validate(&self) -> Result<(), JobStoreError> {
        if self.version != STATE_VERSION {
            return Err(JobStoreError::Corrupt(
                "state version is unsupported".to_owned(),
            ));
        }
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for job in &self.jobs {
            job.record.id.validate()?;
            job.record.idempotency_key.validate()?;
            job.record.request_digest.validate()?;
            if !ids.insert(job.record.id.clone()) {
                return Err(JobStoreError::Corrupt("duplicate job id".to_owned()));
            }
            if !keys.insert(job.record.idempotency_key.clone()) {
                return Err(JobStoreError::Corrupt(
                    "duplicate idempotency key".to_owned(),
                ));
            }
            validate_events(&job.events)?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredJob {
    record: JobRecord,
    events: Vec<JobEvent>,
}

fn unique_job_id(state: &PersistedState) -> JobId {
    loop {
        let candidate = JobId::random();
        if state.jobs.iter().all(|job| job.record.id != candidate) {
            return candidate;
        }
    }
}

fn validate_events(events: &[JobEvent]) -> Result<(), JobStoreError> {
    for (index, event) in events.iter().enumerate() {
        let expected = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| JobStoreError::Corrupt("event sequence exceeds u64".to_owned()))?;
        if event.sequence != expected {
            return Err(JobStoreError::Corrupt(
                "event sequence is not contiguous".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_initialization_marker(marker: &str) -> Result<(), JobStoreError> {
    if marker != INITIALIZED_BODY {
        return Err(JobStoreError::Corrupt(
            "initialization marker is invalid".to_owned(),
        ));
    }
    Ok(())
}

fn decode_state(text: &str) -> Result<PersistedState, JobStoreError> {
    let state: PersistedState = serde_json::from_str(text)
        .map_err(|_| JobStoreError::Corrupt("state is not valid JSON".to_owned()))?;
    state.validate()?;
    Ok(state)
}

fn missing_state_error() -> JobStoreError {
    JobStoreError::Corrupt("state file is missing after initialization".to_owned())
}
