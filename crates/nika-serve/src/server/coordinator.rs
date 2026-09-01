use std::fmt;
use std::time::{Duration, Instant};

use nika_execution::AdmittedExecution;
use serde_json::json;
use sha2::{Digest as _, Sha256};

use crate::{
    Admission, IdempotencyKey, JobId, JobOrigin, JobReceipt, JobRecord, JobStatus, RequestDigest,
};

use super::{ExecutionTask, ServerError, ServerLimits, StoreHandle};

const OBSERVATION_POLL: Duration = Duration::from_millis(5);
const SCHEDULE_KEY_DOMAIN: &[u8] = b"nika/resident-schedule-idempotency@1\0";

/// One resident admission, queue, concurrency, and observation authority.
///
/// This coordinator does not claim exactly-once execution. A crash after the
/// ARM claim or backend effect but before its terminal receipt remains durable
/// `interrupted`/unsettled ambiguity for an operator to observe.
#[derive(Clone)]
pub struct ResidentExecutionCoordinator {
    store: StoreHandle,
    jobs: tokio::sync::mpsc::Sender<ExecutionTask>,
    limits: ServerLimits,
}

impl fmt::Debug for ResidentExecutionCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResidentExecutionCoordinator")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl ResidentExecutionCoordinator {
    pub(super) fn new(
        store: StoreHandle,
        jobs: tokio::sync::mpsc::Sender<ExecutionTask>,
        limits: ServerLimits,
    ) -> Self {
        Self {
            store,
            jobs,
            limits,
        }
    }

    pub(super) async fn admit_manual(
        &self,
        key: IdempotencyKey,
        digest: RequestDigest,
        workflow: String,
        world: String,
    ) -> Result<Admission, ServerError> {
        let permit = self
            .jobs
            .clone()
            .try_reserve_owned()
            .map_err(|_| ServerError::ExecutionQueueFull)?;
        let admission = self
            .store
            .create_or_replay(key, digest, self.limits.max_jobs(), workflow, world)
            .await?;
        match &admission {
            Admission::Created(record) => {
                permit.send(ExecutionTask::new(
                    record.id().clone(),
                    self.limits.default_max_cost_usd(),
                ));
            }
            Admission::Existing(record) if record.status() == JobStatus::Queued => {
                permit.send(ExecutionTask::new(
                    record.id().clone(),
                    self.limits.default_max_cost_usd(),
                ));
            }
            Admission::Existing(_) | Admission::Conflict(_) => drop(permit),
        }
        Ok(admission)
    }

    /// Prepare a normal durable run before ARM appends its fenced claim.
    ///
    /// The exact encoded snapshot is persisted and the run is marked running
    /// before returning. Dropping a newly prepared value before `execute`
    /// settles it failed without invoking the backend or retaining queue space.
    ///
    /// # Errors
    /// Returns a typed queue, store, identity, or snapshot refusal.
    pub fn prepare_scheduled(
        &self,
        admitted: AdmittedExecution,
        origin: JobOrigin,
    ) -> Result<PreparedScheduledRun, ServerError> {
        self.prepare_scheduled_with_max_cost(admitted, origin, None)
    }

    /// Prepare a scheduled run carrying its required per-fire spend ceiling.
    ///
    /// The declared ceiling is validated, then folded with the server-level
    /// default: when both exist the LOWER wins — a schedule restricts the
    /// server's ceiling, never widens it (#1349).
    ///
    /// # Errors
    /// Returns the same typed refusals as [`Self::prepare_scheduled`].
    pub fn prepare_scheduled_with_max_cost(
        &self,
        admitted: AdmittedExecution,
        origin: JobOrigin,
        max_cost_usd: Option<f64>,
    ) -> Result<PreparedScheduledRun, ServerError> {
        if max_cost_usd.is_some_and(|cost| !cost.is_finite() || cost <= 0.0) {
            return Err(ServerError::ScheduledAdmission);
        }
        let max_cost_usd = self.limits.effective_max_cost_usd(max_cost_usd);
        let key = scheduled_key(&origin)?;
        let world = admitted
            .snapshot()
            .encode()
            .map_err(|_| ServerError::ScheduledAdmission)?;
        let digest = RequestDigest::from_bytes(Sha256::digest(world.as_bytes()).into());
        let workflow = admitted.snapshot().root().to_owned();
        let execution_id = admitted.execution_id().to_string();
        let trace_id = admitted.trace_id().to_string();
        let snapshot_digest = admitted.snapshot().digest().to_owned();
        let event_origin = origin.clone();
        let permit = self
            .jobs
            .clone()
            .try_reserve_owned()
            .map_err(|_| ServerError::ExecutionQueueFull)?;
        let admission = self.store.prepare_scheduled_blocking(
            key,
            digest,
            self.limits.max_jobs(),
            workflow,
            world,
            origin,
            execution_id,
            trace_id,
            snapshot_digest,
            json!({"kind": "execution.prepared", "origin": event_origin, "status": "running"}),
        )?;
        match admission {
            Admission::Created(record) => Ok(PreparedScheduledRun::created(
                self.clone(),
                record,
                admitted,
                permit,
                max_cost_usd,
            )),
            Admission::Existing(record) => {
                drop(permit);
                Ok(PreparedScheduledRun::existing(self.clone(), record))
            }
            Admission::Conflict(_) => {
                drop(permit);
                Err(ServerError::ScheduledIdempotencyConflict)
            }
        }
    }

    fn observe_terminal(&self, id: &JobId) -> Result<JobRecord, ServerError> {
        let queue_waves = self
            .limits
            .queue_capacity()
            .div_ceil(self.limits.max_concurrent_jobs())
            .saturating_add(1);
        let queue_waves = u32::try_from(queue_waves).unwrap_or(u32::MAX);
        let observation_window = self
            .limits
            .execution_timeout()
            .saturating_mul(queue_waves)
            .checked_add(self.limits.shutdown_grace())
            .ok_or(ServerError::ScheduledObservationTimeout)?;
        let deadline = Instant::now()
            .checked_add(observation_window)
            .ok_or(ServerError::ScheduledObservationTimeout)?;
        loop {
            let record = self
                .store
                .get_blocking(id.clone())?
                .ok_or_else(|| crate::JobStoreError::JobNotFound(id.clone()))?;
            if !matches!(record.status(), JobStatus::Queued | JobStatus::Running) {
                return Ok(record);
            }
            if Instant::now() >= deadline {
                return Err(ServerError::ScheduledObservationTimeout);
            }
            std::thread::sleep(OBSERVATION_POLL);
        }
    }

    fn abort_prepared(&self, record: &JobRecord) {
        if record.status() != JobStatus::Running {
            return;
        }
        let Ok(receipt) = JobReceipt::with_origin(
            record.id().clone(),
            record.execution_id().unwrap_or_default(),
            record.trace_id().unwrap_or_default(),
            record.snapshot_digest().unwrap_or_default(),
            None,
            record.origin().clone(),
        ) else {
            return;
        };
        let _result = self.store.settle_with_result_blocking(
            record.id().clone(),
            JobStatus::Failed,
            json!({
                "code": "scheduled_claim_aborted",
                "kind": "execution.aborted_before_claim",
                "message": "prepared schedule admission lost its ARM claim",
                "status": "failed"
            }),
            None,
            Some(receipt),
        );
    }
}

/// Prepared half of the two-phase scheduled admission transaction.
pub struct PreparedScheduledRun {
    coordinator: ResidentExecutionCoordinator,
    record: JobRecord,
    admitted: Option<AdmittedExecution>,
    permit: Option<tokio::sync::mpsc::OwnedPermit<ExecutionTask>>,
    abort_on_drop: bool,
    max_cost_usd: Option<f64>,
}

impl fmt::Debug for PreparedScheduledRun {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedScheduledRun")
            .field("run_id", self.record.id())
            .field("status", &self.record.status())
            .finish_non_exhaustive()
    }
}

impl PreparedScheduledRun {
    fn created(
        coordinator: ResidentExecutionCoordinator,
        record: JobRecord,
        admitted: AdmittedExecution,
        permit: tokio::sync::mpsc::OwnedPermit<ExecutionTask>,
        max_cost_usd: Option<f64>,
    ) -> Self {
        Self {
            coordinator,
            record,
            admitted: Some(admitted),
            permit: Some(permit),
            abort_on_drop: true,
            max_cost_usd,
        }
    }

    fn existing(coordinator: ResidentExecutionCoordinator, record: JobRecord) -> Self {
        Self {
            coordinator,
            record,
            admitted: None,
            permit: None,
            abort_on_drop: false,
            max_cost_usd: None,
        }
    }

    /// Normal durable run id allocated before the ARM claim.
    #[must_use]
    pub fn run_id(&self) -> &JobId {
        self.record.id()
    }

    /// Execution identity bound to the normal run.
    #[must_use]
    pub fn execution_id(&self) -> Option<&str> {
        self.record.execution_id()
    }

    /// Direct trace identity bound to the normal run.
    #[must_use]
    pub fn trace_id(&self) -> Option<&str> {
        self.record.trace_id()
    }

    /// Enqueue through the shared lane and block only on durable observation.
    ///
    /// # Errors
    /// Returns if the durable store disappears or the bounded execution and
    /// shutdown window elapses without a visible terminal/paused record.
    pub fn execute(mut self) -> Result<JobRecord, ServerError> {
        if let (Some(permit), Some(admitted)) = (self.permit.take(), self.admitted.take()) {
            self.abort_on_drop = false;
            permit.send(ExecutionTask::scheduled(
                self.record.id().clone(),
                admitted,
                self.record.origin().clone(),
                self.max_cost_usd,
            ));
        }
        self.coordinator.observe_terminal(self.record.id())
    }

    #[cfg(test)]
    pub(super) fn abandon_for_restart_test(mut self) {
        self.abort_on_drop = false;
    }
}

impl Drop for PreparedScheduledRun {
    fn drop(&mut self) {
        if self.abort_on_drop {
            self.coordinator.abort_prepared(&self.record);
        }
    }
}

fn scheduled_key(origin: &JobOrigin) -> Result<IdempotencyKey, ServerError> {
    let (namespace, schedule_id, slot_id) = origin
        .schedule_key_parts()
        .ok_or(ServerError::ScheduledAdmission)?;
    let mut hasher = Sha256::new();
    hasher.update(SCHEDULE_KEY_DOMAIN);
    hasher.update(namespace.as_bytes());
    hasher.update([0]);
    hasher.update(schedule_id.as_bytes());
    hasher.update([0]);
    hasher.update(slot_id.as_bytes());
    IdempotencyKey::new(format!("schedule:{namespace}:{:x}", hasher.finalize()))
        .map_err(ServerError::JobStore)
}
