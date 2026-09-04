use std::fmt;
use std::io::Read as _;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use nika_cadence::{
    AfterSkip, MissPolicy, Overlap, ScheduleDecision, ScheduleDecisionState, ScheduleDefinition,
    ScheduleDraft, ScheduleJitter, ScheduleLastSlot, ScheduleOrigin, ScheduleRevision,
    ScheduleSlot, ScheduleWhen, ScheduleWhenDraft, SlotId,
};
use nika_fs::OwnedDir;
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::{
    MAX_API_SCHEDULES, MAX_ENCODED_SCHEDULE_BYTES, MAX_SCHEDULE_STORE_BYTES, ScheduleApplyOutcome,
    ScheduleApplyPrecondition, ScheduleClaimEvidence, ScheduleDecisionRecord, ScheduleSlotAction,
    ScheduleStoreError, model::MAX_DURABLE_SCHEDULE_DECISIONS,
};

const SCHEDULES_DIR: &str = "schedules";
const INITIALIZED_FILE: &str = "initialized.json";
const INITIALIZED_BODY: &str = "{\"schema\":\"nika/api-schedule-store-init@1\"}\n";
const LOCK_FILE: &str = "store.lock";
const STATE_FILE: &str = "state.json";
const STATE_VERSION: u32 = 1;

/// Descriptor-rooted durable authority for API-origin resident schedules.
///
/// Every operation serializes in-process callers, acquires a kernel lock, and
/// reloads the bounded snapshot. Acknowledged mutations have passed atomic
/// replacement, file synchronization, and containing-directory synchronization.
pub struct ScheduleStore {
    dir: OwnedDir,
    local: Mutex<()>,
}

impl fmt::Debug for ScheduleStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ScheduleStore { durable_root: <redacted> }")
    }
}

impl ScheduleStore {
    /// Open or initialize the contained `schedules` authority below `root`.
    ///
    /// Existing state is decoded and completely validated before the store is
    /// returned. Symlinked components, unknown fields, unsupported versions,
    /// truncated snapshots, and broken revisions refuse recovery.
    ///
    /// # Errors
    /// Returns a typed I/O, size, or corruption refusal.
    pub fn open(root: &Path) -> Result<Self, ScheduleStoreError> {
        let store = Self {
            dir: OwnedDir::create(root, &[SCHEDULES_DIR])?,
            local: Mutex::new(()),
        };
        {
            let _local = store.local_guard()?;
            let _lease = store.kernel_lease()?;
            store.initialize_or_load()?;
            store.stamp_writer()?;
        }
        Ok(store)
    }

    /// ADR-132 · this engine becomes the store's writer at open.
    fn stamp_writer(&self) -> Result<(), ScheduleStoreError> {
        let mut state = self.load_state()?;
        let mine = crate::writer::WriterStamp::this_engine();
        if state.writer.as_ref() != Some(&mine) {
            state.writer = Some(mine);
            self.persist(&state)?;
        }
        Ok(())
    }

    /// Declaratively create or update one normalized API schedule.
    ///
    /// Equality is judged by [`ScheduleRevision`], after the canonical model
    /// validates and normalizes the draft. An identical retry is `Unchanged`
    /// before precondition comparison, making a lost response safe. Any
    /// different existing value requires its exact current revision.
    ///
    /// # Errors
    /// Returns before mutation for an invalid or oversized schedule, at the
    /// count ceiling, or when durable state cannot be read or synchronized.
    pub fn apply(
        &self,
        draft: ScheduleDraft,
        precondition: ScheduleApplyPrecondition,
    ) -> Result<ScheduleApplyOutcome, ScheduleStoreError> {
        let definition = draft.validate()?;
        let candidate = PersistedSchedule::from_definition(&definition)?;
        candidate.validate_encoded_size()?;

        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        match state
            .schedules
            .binary_search_by(|stored| stored.id.as_str().cmp(definition.id()))
        {
            Ok(index) => {
                let current_revision = state.schedules[index].revision()?;
                if current_revision == definition.revision() {
                    return Ok(ScheduleApplyOutcome::Unchanged(definition));
                }
                match precondition {
                    ScheduleApplyPrecondition::Revision(expected)
                        if expected == current_revision =>
                    {
                        state.schedules[index] = candidate;
                        self.persist(&state)?;
                        Ok(ScheduleApplyOutcome::Updated(definition))
                    }
                    ScheduleApplyPrecondition::Create | ScheduleApplyPrecondition::Revision(_) => {
                        Ok(ScheduleApplyOutcome::Conflict {
                            current: Some(current_revision),
                        })
                    }
                }
            }
            Err(index) => match precondition {
                ScheduleApplyPrecondition::Create => {
                    if state.schedules.len() >= MAX_API_SCHEDULES {
                        return Err(ScheduleStoreError::ScheduleLimit {
                            maximum: MAX_API_SCHEDULES,
                        });
                    }
                    state.schedules.insert(index, candidate);
                    self.persist(&state)?;
                    Ok(ScheduleApplyOutcome::Created(definition))
                }
                ScheduleApplyPrecondition::Revision(_) => {
                    Ok(ScheduleApplyOutcome::Conflict { current: None })
                }
            },
        }
    }

    /// Read one API-origin definition from a freshly locked durable snapshot.
    ///
    /// # Errors
    /// Returns a typed I/O or corruption failure.
    pub(crate) fn get(&self, id: &str) -> Result<Option<ScheduleDefinition>, ScheduleStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let state = self.load_state()?;
        state
            .schedules
            .binary_search_by(|stored| stored.id.as_str().cmp(id))
            .ok()
            .map(|index| state.schedules[index].definition())
            .transpose()
    }

    /// Read all API-origin definitions from one bounded durable snapshot.
    ///
    /// # Errors
    /// Returns a typed I/O or corruption failure.
    pub(crate) fn all(&self) -> Result<Box<[ScheduleDefinition]>, ScheduleStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let state = self.load_state()?;
        state
            .schedules
            .iter()
            .map(PersistedSchedule::definition)
            .collect::<Result<Vec<_>, _>>()
            .map(Vec::into_boxed_slice)
    }

    /// Restore the last consumed slot for an origin-local schedule id.
    ///
    /// # Errors
    /// Returns a typed I/O or corruption failure.
    pub(crate) fn decision_state(
        &self,
        origin: ScheduleOrigin,
        id: &str,
    ) -> Result<ScheduleDecisionState, ScheduleStoreError> {
        self.last_decision(origin, id).map(|record| {
            ScheduleDecisionState::new(record.map(|decision| decision.slot().clone()))
        })
    }

    /// Read the bounded last durable action for status projection.
    ///
    /// # Errors
    /// Returns a typed I/O or corruption failure.
    pub(crate) fn last_decision(
        &self,
        origin: ScheduleOrigin,
        id: &str,
    ) -> Result<Option<ScheduleDecisionRecord>, ScheduleStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let state = self.load_state()?;
        let key = decision_key(origin, id);
        state
            .decisions
            .binary_search_by(|stored| stored.key.as_str().cmp(&key))
            .ok()
            .map(|index| state.decisions[index].record())
            .transpose()
    }

    /// Atomically consume one slot as a claim or skip.
    ///
    /// The current API definition must still carry `revision`. Project
    /// definitions are reloaded and compared by the caller immediately before
    /// this transaction. `false` means another caller already consumed the
    /// same slot or an API mutation made the candidate stale.
    ///
    /// # Errors
    /// Returns a typed I/O, corruption, or size failure before acknowledgement.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn consume_slot(
        &self,
        origin: ScheduleOrigin,
        definition: &ScheduleDefinition,
        slot: &ScheduleSlot,
        decision: ScheduleDecision,
        action: ScheduleSlotAction,
        decided_at: jiff::Timestamp,
        reason: Option<String>,
    ) -> Result<bool, ScheduleStoreError> {
        self.consume_slot_with_claim(
            origin, definition, slot, decision, action, decided_at, reason, None,
        )
    }

    /// Atomically persist a claimed slot with its run and generation fence.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn consume_claimed_slot(
        &self,
        origin: ScheduleOrigin,
        definition: &ScheduleDefinition,
        slot: &ScheduleSlot,
        decision: ScheduleDecision,
        decided_at: jiff::Timestamp,
        claim: ScheduleClaimEvidence,
    ) -> Result<bool, ScheduleStoreError> {
        self.consume_slot_with_claim(
            origin,
            definition,
            slot,
            decision,
            ScheduleSlotAction::Claimed,
            decided_at,
            None,
            Some(claim),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn consume_slot_with_claim(
        &self,
        origin: ScheduleOrigin,
        definition: &ScheduleDefinition,
        slot: &ScheduleSlot,
        decision: ScheduleDecision,
        action: ScheduleSlotAction,
        decided_at: jiff::Timestamp,
        reason: Option<String>,
        claim: Option<ScheduleClaimEvidence>,
    ) -> Result<bool, ScheduleStoreError> {
        let _local = self.local_guard()?;
        let _lease = self.kernel_lease()?;
        let mut state = self.load_state()?;
        if origin == ScheduleOrigin::Api {
            let Ok(index) = state
                .schedules
                .binary_search_by(|stored| stored.id.as_str().cmp(definition.id()))
            else {
                return Ok(false);
            };
            if state.schedules[index].revision()? != definition.revision() {
                return Ok(false);
            }
        }
        let record = ScheduleDecisionRecord::new(
            origin,
            definition.id().to_owned(),
            definition.revision(),
            slot,
            decision,
            action,
            decided_at,
            reason,
            claim,
        );
        let persisted = PersistedDecision::from_record(&record);
        let key = persisted.key.clone();
        match state
            .decisions
            .binary_search_by(|stored| stored.key.as_str().cmp(&key))
        {
            Ok(index) if state.decisions[index].slot_id == slot.id().as_str() => Ok(false),
            Ok(index) if state.decisions[index].scheduled_timestamp()? >= slot.scheduled_for() => {
                Ok(false)
            }
            Ok(index) => {
                state.decisions[index] = persisted;
                self.persist(&state)?;
                Ok(true)
            }
            Err(index) => {
                if state.decisions.len() >= MAX_DURABLE_SCHEDULE_DECISIONS {
                    return Err(ScheduleStoreError::DecisionLimit {
                        maximum: MAX_DURABLE_SCHEDULE_DECISIONS,
                    });
                }
                state.decisions.insert(index, persisted);
                self.persist(&state)?;
                Ok(true)
            }
        }
    }

    fn local_guard(&self) -> Result<MutexGuard<'_, ()>, ScheduleStoreError> {
        self.local
            .lock()
            .map_err(|_| ScheduleStoreError::LockPoisoned)
    }

    fn kernel_lease(&self) -> Result<Flock<std::fs::File>, ScheduleStoreError> {
        let file = self.dir.open_lock(LOCK_FILE)?;
        Flock::lock(file, FlockArg::LockExclusive).map_err(|(_file, errno)| {
            ScheduleStoreError::from(std::io::Error::from_raw_os_error(errno as i32))
        })
    }

    fn initialize_or_load(&self) -> Result<(), ScheduleStoreError> {
        let marker = self.dir.read_optional(INITIALIZED_FILE)?;
        let state = self.read_state_optional()?;
        match (marker.as_deref(), state.as_deref()) {
            (None, None) => {
                self.persist(&PersistedState::new())?;
                self.dir.write_atomic(INITIALIZED_FILE, INITIALIZED_BODY)?;
                Ok(())
            }
            (Some(marker), Some(state)) => {
                validate_marker(marker)?;
                decode_state(state).map(|_| ())
            }
            (Some(marker), None) => {
                validate_marker(marker)?;
                Err(ScheduleStoreError::Corrupt(
                    "state is missing after initialization".to_owned(),
                ))
            }
            (None, Some(_)) => Err(ScheduleStoreError::Corrupt(
                "initialization marker is missing".to_owned(),
            )),
        }
    }

    fn load_state(&self) -> Result<PersistedState, ScheduleStoreError> {
        let marker = self.dir.read_optional(INITIALIZED_FILE)?.ok_or_else(|| {
            ScheduleStoreError::Corrupt("initialization marker is missing".to_owned())
        })?;
        validate_marker(&marker)?;
        let text = self.read_state_optional()?.ok_or_else(|| {
            ScheduleStoreError::Corrupt("state is missing after initialization".to_owned())
        })?;
        decode_state(&text)
    }

    fn persist(&self, state: &PersistedState) -> Result<(), ScheduleStoreError> {
        let body = prepare_snapshot(state)?;
        self.dir.write_atomic(STATE_FILE, &body)?;
        Ok(())
    }

    fn read_state_optional(&self) -> Result<Option<String>, ScheduleStoreError> {
        let mut file = match self.dir.open_relative(Path::new(STATE_FILE)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let bytes = file.metadata()?.len();
        if bytes > MAX_SCHEDULE_STORE_BYTES as u64 {
            return Err(snapshot_too_large(bytes));
        }
        let mut body = Vec::new();
        file.by_ref()
            .take(MAX_SCHEDULE_STORE_BYTES as u64 + 1)
            .read_to_end(&mut body)?;
        if body.len() > MAX_SCHEDULE_STORE_BYTES {
            return Err(snapshot_too_large(body.len() as u64));
        }
        String::from_utf8(body)
            .map(Some)
            .map_err(|_| ScheduleStoreError::Corrupt("state is not valid UTF-8".to_owned()))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedState {
    version: u32,
    schedules: Vec<PersistedSchedule>,
    #[serde(default)]
    decisions: Vec<PersistedDecision>,
    /// The engine that last wrote this store (ADR-132 · #1352).
    #[serde(default)]
    writer: Option<crate::writer::WriterStamp>,
}

impl PersistedState {
    fn new() -> Self {
        Self {
            version: STATE_VERSION,
            schedules: Vec::new(),
            decisions: Vec::new(),
            writer: Some(crate::writer::WriterStamp::this_engine()),
        }
    }

    fn validate(&self) -> Result<(), ScheduleStoreError> {
        if self.version != STATE_VERSION {
            return Err(ScheduleStoreError::Corrupt(
                "state version is unsupported".to_owned(),
            ));
        }
        // ADR-132 · #1352 · a newer writer's state is not ours to reinterpret.
        if let Some(reason) = self
            .writer
            .as_ref()
            .and_then(crate::writer::WriterStamp::newer_than_this_engine)
        {
            return Err(ScheduleStoreError::WrittenByNewerEngine(reason));
        }
        if self.schedules.len() > MAX_API_SCHEDULES {
            return Err(ScheduleStoreError::Corrupt(
                "schedule count exceeds its bound".to_owned(),
            ));
        }
        let mut previous: Option<&str> = None;
        for stored in &self.schedules {
            if previous.is_some_and(|id| id >= stored.id.as_str()) {
                return Err(ScheduleStoreError::Corrupt(
                    "schedule ids are duplicated or not sorted".to_owned(),
                ));
            }
            stored.validate()?;
            previous = Some(&stored.id);
        }
        let mut previous_decision: Option<&str> = None;
        if self.decisions.len() > MAX_DURABLE_SCHEDULE_DECISIONS {
            return Err(ScheduleStoreError::Corrupt(
                "schedule decision count exceeds its bound".to_owned(),
            ));
        }
        for stored in &self.decisions {
            if previous_decision.is_some_and(|key| key >= stored.key.as_str()) {
                return Err(ScheduleStoreError::Corrupt(
                    "schedule decisions are duplicated or not sorted".to_owned(),
                ));
            }
            stored.validate()?;
            previous_decision = Some(&stored.key);
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedSchedule {
    id: String,
    origin: String,
    workflow: String,
    when: PersistedWhen,
    max_cost_usd: f64,
    missed: String,
    max_lateness_seconds: Option<u64>,
    overlap: String,
    after_skip: String,
    jitter: Option<String>,
    tolerance: Option<String>,
    active: bool,
    pause_reason: Option<String>,
    pause_until: Option<String>,
    revision: String,
    integrity: String,
    // Reserved explicitly for a later history schema. V1 writes and accepts
    // only an empty array, so no unreviewed event or secret shape can persist.
    history: Vec<Value>,
}

impl PersistedSchedule {
    fn from_definition(definition: &ScheduleDefinition) -> Result<Self, ScheduleStoreError> {
        let when = match definition.when() {
            ScheduleWhen::Once { at } => PersistedWhen::Once { at: at.to_string() },
            ScheduleWhen::Cadence { expression } => PersistedWhen::Cadence {
                expression: expression.clone(),
            },
            ScheduleWhen::Webhook => PersistedWhen::Webhook,
            _ => return Err(ScheduleStoreError::UnsupportedCanonicalValue),
        };
        let missed = match definition.missed() {
            MissPolicy::Rattraper => "catch-up",
            MissPolicy::RattraperUneFois => "catch-up-once",
            MissPolicy::Sauter => "skip",
            _ => return Err(ScheduleStoreError::UnsupportedCanonicalValue),
        };
        let overlap = match definition.overlap() {
            Overlap::Sauter => "skip",
            Overlap::File => "queue",
            Overlap::Remplacer => "replace",
            _ => return Err(ScheduleStoreError::UnsupportedCanonicalValue),
        };
        let after_skip = match definition.after_skip() {
            AfterSkip::ProchainCreneau => "next-slot",
            AfterSkip::ACompletion => "on-completion",
            _ => return Err(ScheduleStoreError::UnsupportedCanonicalValue),
        };
        let jitter = match definition.jitter() {
            None => None,
            Some(ScheduleJitter::Hash) => Some("hash".to_owned()),
            Some(_) => return Err(ScheduleStoreError::UnsupportedCanonicalValue),
        };
        let mut stored = Self {
            id: definition.id().to_owned(),
            origin: "api".to_owned(),
            workflow: definition.workflow().to_owned(),
            when,
            max_cost_usd: definition.max_cost_usd(),
            missed: missed.to_owned(),
            max_lateness_seconds: definition.max_lateness_seconds(),
            overlap: overlap.to_owned(),
            after_skip: after_skip.to_owned(),
            jitter,
            tolerance: definition.tolerance().map(str::to_owned),
            active: definition.is_active(),
            pause_reason: definition.pause_reason().map(str::to_owned),
            pause_until: definition.pause_until().map(str::to_owned),
            revision: definition.revision().as_str().to_owned(),
            integrity: String::new(),
            history: Vec::new(),
        };
        stored.integrity = stored.compute_integrity()?;
        Ok(stored)
    }

    fn validate(&self) -> Result<(), ScheduleStoreError> {
        self.validate_encoded_size()?;
        if self.origin != "api" {
            return Err(ScheduleStoreError::Corrupt(
                "schedule origin is not api".to_owned(),
            ));
        }
        if !self.history.is_empty() {
            return Err(ScheduleStoreError::Corrupt(
                "schedule history is unsupported by schema v1".to_owned(),
            ));
        }
        self.revision()?;
        if self.compute_integrity()? != self.integrity {
            return Err(ScheduleStoreError::Corrupt(
                "schedule integrity does not match its deterministic record".to_owned(),
            ));
        }
        let definition = self.definition()?;
        if definition.revision() != self.revision()? {
            return Err(ScheduleStoreError::Corrupt(
                "schedule revision does not match its canonical definition".to_owned(),
            ));
        }
        Ok(())
    }

    fn definition(&self) -> Result<ScheduleDefinition, ScheduleStoreError> {
        let when = match &self.when {
            PersistedWhen::Once { at } => ScheduleWhenDraft::Once { at: at.clone() },
            PersistedWhen::Cadence { expression } => ScheduleWhenDraft::Cadence {
                expression: expression.clone(),
            },
            PersistedWhen::Webhook => ScheduleWhenDraft::Webhook,
        };
        let missed = match self.missed.as_str() {
            "catch-up" => MissPolicy::Rattraper,
            "catch-up-once" => MissPolicy::RattraperUneFois,
            "skip" => MissPolicy::Sauter,
            _ => return Err(corrupt_value("missed")),
        };
        let overlap = match self.overlap.as_str() {
            "skip" => Overlap::Sauter,
            "queue" => Overlap::File,
            "replace" => Overlap::Remplacer,
            _ => return Err(corrupt_value("overlap")),
        };
        let after_skip = match self.after_skip.as_str() {
            "next-slot" => AfterSkip::ProchainCreneau,
            "on-completion" => AfterSkip::ACompletion,
            _ => return Err(corrupt_value("after_skip")),
        };
        let jitter = match self.jitter.as_deref() {
            None => None,
            Some("hash") => Some(ScheduleJitter::Hash),
            Some(_) => return Err(corrupt_value("jitter")),
        };
        let mut draft = ScheduleDraft::new(
            self.id.clone(),
            self.workflow.clone(),
            when,
            self.max_cost_usd,
            missed,
        );
        draft.max_lateness_seconds = self.max_lateness_seconds;
        draft.overlap = Some(overlap);
        draft.after_skip = Some(after_skip);
        draft.jitter = jitter;
        draft.tolerance.clone_from(&self.tolerance);
        draft.active = Some(self.active);
        draft.pause_reason.clone_from(&self.pause_reason);
        draft.pause_until.clone_from(&self.pause_until);
        draft.validate().map_err(|finding| {
            ScheduleStoreError::Corrupt(format!(
                "stored schedule failed canonical validation: {finding}"
            ))
        })
    }

    fn revision(&self) -> Result<ScheduleRevision, ScheduleStoreError> {
        ScheduleRevision::from_wire(&self.revision).ok_or_else(|| corrupt_value("revision"))
    }

    fn compute_integrity(&self) -> Result<String, ScheduleStoreError> {
        let projection = IntegrityProjection {
            id: &self.id,
            origin: &self.origin,
            workflow: &self.workflow,
            when: &self.when,
            max_cost_usd: self.max_cost_usd,
            missed: &self.missed,
            max_lateness_seconds: self.max_lateness_seconds,
            overlap: &self.overlap,
            after_skip: &self.after_skip,
            jitter: self.jitter.as_deref(),
            tolerance: self.tolerance.as_deref(),
            active: self.active,
            pause_reason: self.pause_reason.as_deref(),
            pause_until: self.pause_until.as_deref(),
            revision: &self.revision,
            history: &self.history,
        };
        let bytes = serde_json::to_vec(&projection).map_err(|_| {
            ScheduleStoreError::Corrupt("schedule integrity cannot be encoded".to_owned())
        })?;
        let digest = Sha256::digest(bytes);
        Ok(format!("sha256:{digest:x}"))
    }

    fn validate_encoded_size(&self) -> Result<(), ScheduleStoreError> {
        let bytes = serde_json::to_vec(self)
            .map_err(|_| ScheduleStoreError::Corrupt("schedule cannot be encoded".to_owned()))?
            .len();
        if bytes > MAX_ENCODED_SCHEDULE_BYTES {
            return Err(ScheduleStoreError::ScheduleTooLarge {
                bytes,
                maximum: MAX_ENCODED_SCHEDULE_BYTES,
            });
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct IntegrityProjection<'a> {
    id: &'a str,
    origin: &'a str,
    workflow: &'a str,
    when: &'a PersistedWhen,
    max_cost_usd: f64,
    missed: &'a str,
    max_lateness_seconds: Option<u64>,
    overlap: &'a str,
    after_skip: &'a str,
    jitter: Option<&'a str>,
    tolerance: Option<&'a str>,
    active: bool,
    pause_reason: Option<&'a str>,
    pause_until: Option<&'a str>,
    revision: &'a str,
    history: &'a [Value],
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum PersistedWhen {
    Once { at: String },
    Cadence { expression: String },
    Webhook,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedDecision {
    key: String,
    origin: String,
    schedule_id: String,
    revision: String,
    slot_id: String,
    scheduled_for: String,
    decision: String,
    action: String,
    decided_at: String,
    reason: Option<String>,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    execution_id: Option<String>,
    #[serde(default)]
    trace_id: Option<String>,
    #[serde(default)]
    arm_generation: Option<String>,
}

impl PersistedDecision {
    fn from_record(record: &ScheduleDecisionRecord) -> Self {
        Self {
            key: decision_key(record.origin(), record.schedule_id()),
            origin: origin_word(record.origin()).to_owned(),
            schedule_id: record.schedule_id().to_owned(),
            revision: record.revision().as_str().to_owned(),
            slot_id: record.slot().id().as_str().to_owned(),
            scheduled_for: record.slot().scheduled_for().to_string(),
            decision: decision_word(record.decision()).to_owned(),
            action: action_word(record.action()).to_owned(),
            decided_at: record.decided_at().to_string(),
            reason: record.reason().map(str::to_owned),
            run_id: record.claim().map(|claim| claim.run_id().to_owned()),
            execution_id: record.claim().map(|claim| claim.execution_id().to_owned()),
            trace_id: record.claim().map(|claim| claim.trace_id().to_owned()),
            arm_generation: record
                .claim()
                .map(|claim| claim.generation().as_str().to_owned()),
        }
    }

    fn validate(&self) -> Result<(), ScheduleStoreError> {
        let record = self.record()?;
        if self.key != decision_key(record.origin(), record.schedule_id()) {
            return Err(ScheduleStoreError::Corrupt(
                "schedule decision key does not match its identity".to_owned(),
            ));
        }
        Ok(())
    }

    fn record(&self) -> Result<ScheduleDecisionRecord, ScheduleStoreError> {
        let origin = match self.origin.as_str() {
            "project" => ScheduleOrigin::Project,
            "api" => ScheduleOrigin::Api,
            _ => return Err(corrupt_value("decision origin")),
        };
        let revision = ScheduleRevision::from_wire(&self.revision)
            .ok_or_else(|| corrupt_value("decision revision"))?;
        let slot_id =
            SlotId::from_wire(&self.slot_id).ok_or_else(|| corrupt_value("decision slot id"))?;
        let scheduled_for = self
            .scheduled_for
            .parse()
            .map_err(|_| corrupt_value("decision scheduled_for"))?;
        let decision = match self.decision.as_str() {
            "scheduled" => ScheduleDecision::Scheduled,
            "catch_up" => ScheduleDecision::CatchUp,
            _ => return Err(corrupt_value("decision kind")),
        };
        let action = match self.action.as_str() {
            "claimed" => ScheduleSlotAction::Claimed,
            "skipped" => ScheduleSlotAction::Skipped,
            _ => return Err(corrupt_value("decision action")),
        };
        let decided_at = self
            .decided_at
            .parse()
            .map_err(|_| corrupt_value("decision decided_at"))?;
        let claim = match (
            self.run_id.as_deref(),
            self.execution_id.as_deref(),
            self.trace_id.as_deref(),
            self.arm_generation.as_deref(),
        ) {
            (None, None, None, None) => None,
            (Some(run_id), Some(execution_id), Some(trace_id), Some(generation))
                if crate::JobId::parse(run_id).is_ok()
                    && !execution_id.is_empty()
                    && execution_id.len() <= 128
                    && !trace_id.is_empty()
                    && trace_id.len() <= 128 =>
            {
                let generation = nika_cadence::ArmGeneration::from_wire(generation)
                    .ok_or_else(|| corrupt_value("decision arm generation"))?;
                Some(ScheduleClaimEvidence::new(
                    run_id.to_owned(),
                    execution_id.to_owned(),
                    trace_id.to_owned(),
                    generation,
                ))
            }
            _ => return Err(corrupt_value("decision claim evidence")),
        };
        if self.schedule_id.is_empty()
            || self.schedule_id.len() > nika_cadence::schedule::MAX_SCHEDULE_ID_BYTES
            || self
                .reason
                .as_ref()
                .is_some_and(|reason| reason.len() > 1_024)
        {
            return Err(corrupt_value("decision metadata"));
        }
        Ok(ScheduleDecisionRecord {
            origin,
            schedule_id: self.schedule_id.clone(),
            revision,
            slot: ScheduleLastSlot::new(slot_id, scheduled_for),
            decision,
            action,
            decided_at,
            reason: self.reason.clone(),
            claim,
        })
    }

    fn scheduled_timestamp(&self) -> Result<jiff::Timestamp, ScheduleStoreError> {
        self.scheduled_for
            .parse()
            .map_err(|_| corrupt_value("decision scheduled_for"))
    }
}

fn decision_key(origin: ScheduleOrigin, id: &str) -> String {
    format!("{}:{id}", origin_word(origin))
}

const fn origin_word(origin: ScheduleOrigin) -> &'static str {
    match origin {
        ScheduleOrigin::Project => "project",
        ScheduleOrigin::Api => "api",
        _ => "unknown",
    }
}

const fn decision_word(decision: ScheduleDecision) -> &'static str {
    match decision {
        ScheduleDecision::Scheduled => "scheduled",
        ScheduleDecision::CatchUp => "catch_up",
        _ => "unknown",
    }
}

const fn action_word(action: ScheduleSlotAction) -> &'static str {
    match action {
        ScheduleSlotAction::Claimed => "claimed",
        ScheduleSlotAction::Skipped => "skipped",
    }
}

fn validate_marker(marker: &str) -> Result<(), ScheduleStoreError> {
    if marker == INITIALIZED_BODY {
        Ok(())
    } else {
        Err(ScheduleStoreError::Corrupt(
            "initialization marker is invalid".to_owned(),
        ))
    }
}

fn decode_state(text: &str) -> Result<PersistedState, ScheduleStoreError> {
    let state: PersistedState = serde_json::from_str(text)
        .map_err(|_| ScheduleStoreError::Corrupt("state is not valid schema JSON".to_owned()))?;
    state.validate()?;
    Ok(state)
}

fn prepare_snapshot(state: &PersistedState) -> Result<String, ScheduleStoreError> {
    state.validate()?;
    let mut body = serde_json::to_string(state)
        .map_err(|_| ScheduleStoreError::Corrupt("state cannot be encoded".to_owned()))?;
    body.push('\n');
    if body.len() > MAX_SCHEDULE_STORE_BYTES {
        return Err(snapshot_too_large(body.len() as u64));
    }
    Ok(body)
}

fn snapshot_too_large(bytes: u64) -> ScheduleStoreError {
    ScheduleStoreError::SnapshotTooLarge {
        bytes,
        maximum: MAX_SCHEDULE_STORE_BYTES,
    }
}

fn corrupt_value(field: &str) -> ScheduleStoreError {
    ScheduleStoreError::Corrupt(format!("schedule {field} is invalid"))
}
