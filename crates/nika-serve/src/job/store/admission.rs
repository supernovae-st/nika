use serde_json::Value;

use crate::JobOrigin;

use super::{
    Admission, EventHash, IdempotencyKey, JobId, JobMutation, JobReceipt, JobRecord, JobStatus,
    JobStore, JobStoreError, MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES, RequestDigest, StoredJob,
    ValidatedEventBatch, ensure_receipt_matches, hash_execution_identity, unique_job_id,
    validate_snapshot_digest, world_file,
};

impl JobStore {
    /// Cancel an unclaimed job without taking ownership from a running engine.
    /// Identity, receipt and cancellation become durable under the same lease
    /// that excludes `start_execution`; a stale queued read grants no authority.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn cancel_queued(
        &self,
        id: &JobId,
        execution_id: String,
        trace_id: String,
        snapshot_digest: String,
        event: &Value,
        receipt: JobReceipt,
    ) -> Result<JobMutation, JobStoreError> {
        if execution_id.is_empty()
            || trace_id.is_empty()
            || validate_snapshot_digest(&snapshot_digest).is_err()
        {
            return Err(JobStoreError::InvalidReceipt);
        }
        receipt.validate()?;
        let batch = ValidatedEventBatch::for_transition(std::slice::from_ref(event))?;
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        super::ensure_approval_claims_unused(&state, &batch)?;
        let job = state
            .jobs
            .iter_mut()
            .find(|job| job.record.id == *id)
            .ok_or_else(|| JobStoreError::JobNotFound(id.clone()))?;
        if job.record.status != JobStatus::Queued {
            return Err(JobStoreError::IllegalTransition {
                from: job.record.status,
                to: JobStatus::Cancelled,
            });
        }
        job.record.execution_id = execution_id;
        job.record.trace_id = trace_id;
        job.record.snapshot_digest = snapshot_digest;
        ensure_receipt_matches(&job.record, &receipt)?;
        job.record.status = JobStatus::Cancelled;
        job.record.outputs = None;
        job.record.receipt = Some(receipt);
        job.identity_digest = Some(hash_execution_identity(&job.record)?);
        job.terminal_sequence = Some(job.final_sequence_after(&batch)?);
        let events = job.append_payloads(&batch)?;
        let record = job.record.clone();
        self.persist_event_mutation(&state, &batch)?;
        Ok(JobMutation { record, events })
    }

    /// The admission already bound to this key, without creating one
    /// (ADR-132 · the freeze audit): a lost-response retry finds its job
    /// BEFORE the resident touches the registry or the body again, so a
    /// workflow that changed, vanished or went red since the first request
    /// can neither re-run nor lose the job. `None` when the key is unbound.
    ///
    /// # Errors
    /// Returns an error when locking, loading or validation fails.
    pub fn replay(
        &self,
        key: &IdempotencyKey,
        digest: &RequestDigest,
    ) -> Result<Option<Admission>, JobStoreError> {
        key.validate()?;
        digest.validate()?;
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let state = self.load_state()?;
        Ok(state
            .jobs
            .iter()
            .find(|job| job.record.idempotency_key == *key)
            .map(|existing| {
                if existing.record.request_digest == *digest {
                    Admission::Existing(existing.record.clone())
                } else {
                    Admission::Conflict(existing.record.clone())
                }
            }))
    }

    /// Atomically create one scheduled run already claimed as `running`.
    ///
    /// The caller has reserved the shared queue before entering this method.
    /// Persisting `running` before ARM's claim makes a crash visible as
    /// interrupted ambiguity instead of silently replaying an ownerless effect.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_scheduled_captured(
        &self,
        key: IdempotencyKey,
        digest: RequestDigest,
        max_jobs: usize,
        workflow: String,
        world: &str,
        origin: JobOrigin,
        execution_id: String,
        trace_id: String,
        snapshot_digest: String,
        event: &Value,
    ) -> Result<Admission, JobStoreError> {
        key.validate()?;
        digest.validate()?;
        origin.validate()?;
        if !matches!(origin, JobOrigin::Schedule { .. })
            || execution_id.is_empty()
            || trace_id.is_empty()
            || RequestDigest::new(snapshot_digest.clone()).is_err()
        {
            return Err(JobStoreError::InvalidReceipt);
        }
        if world.len() > MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES {
            return Err(JobStoreError::SnapshotTooLarge {
                bytes: world.len() as u64,
                maximum: MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES,
            });
        }
        let batch = ValidatedEventBatch::for_transition(std::slice::from_ref(event))?;
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
            status: JobStatus::Running,
            origin,
            workflow,
            execution_id,
            trace_id,
            snapshot_digest,
            error_code: String::new(),
            error_message: String::new(),
            outputs: None,
            receipt: None,
            settlement: None,
            paused_outputs: None,
            paused_receipt: None,
        };
        let identity_digest = Some(hash_execution_identity(&record)?);
        let mut stored = StoredJob {
            record: record.clone(),
            events: Vec::new(),
            event_count: 0,
            event_head: None::<EventHash>,
            identity_digest,
            terminal_sequence: None,
        };
        stored.append_payloads(&batch)?;
        self.dir.write_atomic(&world_file(&record.id), world)?;
        state.jobs.push(stored);
        self.persist(&state)?;
        Ok(Admission::Created(record))
    }
}
