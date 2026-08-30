use super::*;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyPersistedStateV2 {
    version: u32,
    incarnation: IncarnationLedger,
    jobs: Vec<LegacyStoredJobV2>,
}

impl LegacyPersistedStateV2 {
    fn migrate(self) -> Result<PersistedState, JobStoreError> {
        if self.version != LEGACY_STATE_VERSION {
            return Err(JobStoreError::Corrupt(
                "legacy state version is unsupported".to_owned(),
            ));
        }
        self.incarnation.validate()?;
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for job in &self.jobs {
            job.record.id.validate()?;
            job.record.idempotency_key.validate()?;
            job.record.request_digest.validate()?;
            validate_terminal_record(&job.record)?;
            if !ids.insert(job.record.id.clone()) {
                return Err(JobStoreError::Corrupt("duplicate job id".to_owned()));
            }
            if !keys.insert(job.record.idempotency_key.clone()) {
                return Err(JobStoreError::Corrupt(
                    "duplicate idempotency key".to_owned(),
                ));
            }
            validate_legacy_events(job)?;
        }
        let state = PersistedState {
            version: STATE_VERSION,
            incarnation: self.incarnation,
            jobs: self
                .jobs
                .into_iter()
                .map(migrate_job)
                .collect::<Result<Vec<_>, _>>()?,
        };
        state.validate()?;
        Ok(state)
    }
}

fn migrate_job(job: LegacyStoredJobV2) -> Result<StoredJob, JobStoreError> {
    if !job.record.status.is_settled() {
        return Ok(StoredJob {
            record: migrate_legacy_nonterminal_record(job.record),
            events: job.events,
            event_count: job.event_count,
            event_head: job.event_head,
            identity_digest: None,
            terminal_sequence: None,
        });
    }

    validate_legacy_terminal_event(&job)?;
    let identity_digest = (!job.record.execution_id.is_empty())
        .then(|| hash_execution_identity(&job.record))
        .transpose()?;
    let mut migrated = StoredJob {
        record: job.record,
        events: job.events,
        event_count: job.event_count,
        event_head: job.event_head,
        identity_digest,
        terminal_sequence: Some(job.event_count),
    };
    rehash_events(&mut migrated)?;
    Ok(migrated)
}

/// V2 did not bind terminal result fields. Migration is an explicit
/// trust-on-first-open boundary: first validate its old chain and terminal
/// shape, then mint the v3 result binding and rewrite atomically. This keeps
/// real history readable without pretending v2 carried proof it never had.
fn rehash_events(job: &mut StoredJob) -> Result<(), JobStoreError> {
    let mut previous = None;
    for event in &mut job.events {
        event.previous_hash.clone_from(&previous);
        event.hash = hash_event(
            &job.record,
            job.terminal_sequence,
            event.sequence,
            event.previous_hash.as_ref(),
            &event.payload,
        )?;
        previous = Some(event.hash.clone());
    }
    job.event_head = previous;
    Ok(())
}

fn validate_legacy_terminal_event(job: &LegacyStoredJobV2) -> Result<(), JobStoreError> {
    let event = job.events.last().ok_or_else(|| {
        JobStoreError::Corrupt("legacy terminal job has no terminal event".to_owned())
    })?;
    let kind = event.payload.get("kind").and_then(Value::as_str);
    let status = event.payload.get("status").and_then(Value::as_str);
    let valid = match job.record.status {
        JobStatus::Succeeded => kind == Some("execution.settled") && status == Some("succeeded"),
        JobStatus::Failed => {
            matches!(kind, Some("execution.settled" | "execution.refused"))
                && status == Some("failed")
        }
        JobStatus::Interrupted => {
            matches!(kind, Some("execution.interrupted" | "interrupted"))
                && status == Some("interrupted")
        }
        JobStatus::Queued | JobStatus::Running | JobStatus::Paused => false,
    };
    if !valid {
        return Err(JobStoreError::Corrupt(
            "legacy terminal event does not match its durable status".to_owned(),
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyStoredJobV2 {
    record: JobRecord,
    events: Vec<JobEvent>,
    event_count: u64,
    event_head: Option<EventHash>,
}

fn validate_legacy_events(job: &LegacyStoredJobV2) -> Result<(), JobStoreError> {
    if usize::try_from(job.event_count).ok() != Some(job.events.len())
        || job.event_head.as_ref() != job.events.last().map(|event| &event.hash)
    {
        return Err(JobStoreError::Corrupt(
            "legacy event chain head or count does not match its journal".to_owned(),
        ));
    }
    let mut previous: Option<&EventHash> = None;
    for (index, event) in job.events.iter().enumerate() {
        let expected = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| JobStoreError::Corrupt("event sequence exceeds u64".to_owned()))?;
        if event.sequence != expected || event.previous_hash.as_ref() != previous {
            return Err(JobStoreError::Corrupt(
                "legacy event chain sequence or predecessor does not match".to_owned(),
            ));
        }
        event.hash.validate()?;
        validate_approval_event(&event.payload)
            .map_err(|_| JobStoreError::Corrupt("approval event digest is invalid".to_owned()))?;
        let expected_hash = hash_event(
            &job.record,
            None,
            event.sequence,
            event.previous_hash.as_ref(),
            &event.payload,
        )?;
        if event.hash != expected_hash {
            return Err(JobStoreError::Corrupt(
                "legacy event chain hash does not match its canonical preimage".to_owned(),
            ));
        }
        previous = Some(&event.hash);
    }
    Ok(())
}

pub(super) struct DecodedState {
    pub(super) state: PersistedState,
    pub(super) migrated: bool,
}

#[derive(Deserialize)]
struct StateVersionProbe {
    version: u32,
}

pub(super) fn decode_state(text: &str) -> Result<DecodedState, JobStoreError> {
    let probe: StateVersionProbe = serde_json::from_str(text)
        .map_err(|_| JobStoreError::Corrupt("state is not valid JSON".to_owned()))?;
    match probe.version {
        STATE_VERSION => {
            let state: PersistedState = serde_json::from_str(text)
                .map_err(|_| JobStoreError::Corrupt("state is not valid JSON".to_owned()))?;
            state.validate()?;
            Ok(DecodedState {
                state,
                migrated: false,
            })
        }
        LEGACY_STATE_VERSION => {
            let legacy: LegacyPersistedStateV2 = serde_json::from_str(text)
                .map_err(|_| JobStoreError::Corrupt("legacy state is not valid JSON".to_owned()))?;
            Ok(DecodedState {
                state: legacy.migrate()?,
                migrated: true,
            })
        }
        _ => Err(JobStoreError::Corrupt(
            "state version is unsupported".to_owned(),
        )),
    }
}
