use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::sync::{Arc, Barrier, Mutex};

use serde_json::json;
use sha2::{Digest as _, Sha256};

use super::*;

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::new(value).expect("valid key")
}

fn digest(byte: u8) -> RequestDigest {
    RequestDigest::from_bytes([byte; 32])
}

fn page_limit(value: usize) -> EventPageLimit {
    EventPageLimit::new(value).expect("valid page limit")
}

fn transition(store: &JobStore, id: &JobId, status: JobStatus) -> JobMutation {
    store
        .transition_with_events(id, status, &[json!({"status": status.to_string()})])
        .expect("legal transition")
}

fn stamp_execution(store: &JobStore, id: &JobId, byte: u8) {
    store
        .stamp_execution_identity(
            id,
            format!("execution-{byte}"),
            format!("trace-{byte}"),
            digest(byte).as_str().to_owned(),
        )
        .expect("stamp execution identity");
}

fn downgrade_state_to_v2(state: &mut serde_json::Value) {
    state["version"] = json!(2);
    for job in state["jobs"].as_array_mut().expect("jobs") {
        let stored = job.as_object_mut().expect("stored job");
        stored.remove("terminal_sequence");
        stored.remove("identity_digest");
        let record = &job["record"];
        let job_id = record["id"].as_str().expect("job id").to_owned();
        let request_digest = record["request_digest"]
            .as_str()
            .expect("request digest")
            .to_owned();
        let mut previous: Option<String> = None;
        for event in job["events"].as_array_mut().expect("events") {
            event["previous_hash"] = previous
                .clone()
                .map_or(serde_json::Value::Null, serde_json::Value::String);
            let preimage = json!({
                "job_id": &job_id,
                "payload": event["payload"].clone(),
                "previous_hash": previous,
                "request_digest": &request_digest,
                "sequence": event["sequence"],
            });
            let canonical = serde_json::to_vec(&preimage).expect("legacy preimage");
            let mut hasher = Sha256::new();
            hasher.update(b"nika.job-event.chain\0v1\0");
            hasher.update(canonical);
            let mut hash = String::with_capacity(64);
            for byte in hasher.finalize() {
                write!(&mut hash, "{byte:02x}").expect("write digest");
            }
            event["hash"] = json!(hash);
            previous = Some(hash);
        }
        job["event_head"] = previous.map_or(serde_json::Value::Null, serde_json::Value::String);
    }
}

#[derive(Default)]
struct MemoryApprovalHistory {
    recorded: Mutex<BTreeSet<RequestDigest>>,
}

impl ApprovalHistory for MemoryApprovalHistory {
    fn verify_recorded(&self, digests: &[RequestDigest]) -> Result<(), ApprovalHistoryError> {
        let recorded = self
            .recorded
            .lock()
            .map_err(|_| ApprovalHistoryError::Unavailable)?;
        if digests.iter().all(|digest| recorded.contains(digest)) {
            Ok(())
        } else {
            Err(ApprovalHistoryError::MissingRecord)
        }
    }

    fn record_once(&self, digests: &[RequestDigest]) -> Result<(), ApprovalHistoryError> {
        let mut recorded = self
            .recorded
            .lock()
            .map_err(|_| ApprovalHistoryError::Unavailable)?;
        if digests.iter().any(|digest| recorded.contains(digest)) {
            return Err(ApprovalHistoryError::AlreadyRecorded);
        }
        recorded.extend(digests.iter().cloned());
        Ok(())
    }
}

#[test]
fn request_digest_accepts_only_canonical_lowercase_hex() {
    let cases = [
        ("lowercase", "ab".repeat(32), true),
        ("uppercase", "AB".repeat(32), false),
        ("mixed case", "aB".repeat(32), false),
        ("too short", "ab".repeat(31), false),
        ("too long", format!("{}0", "ab".repeat(32)), false),
        ("non-hex", format!("{}g", "ab".repeat(31)), false),
    ];

    for (name, value, accepted) in cases {
        assert_eq!(
            RequestDigest::new(value).is_ok(),
            accepted,
            "boundary case: {name}"
        );
    }
}

#[test]
fn opaque_wire_types_refuse_forged_deserialization() {
    let id = JobId::random();
    let encoded = serde_json::to_string(&id).expect("encode job id");
    assert_eq!(
        serde_json::from_str::<JobId>(&encoded).expect("decode canonical job id"),
        id
    );

    assert!(serde_json::from_str::<JobId>(r#""not-a-uuid\nFORGED""#).is_err());
    assert!(serde_json::from_str::<JobId>(r#""00000000-0000-4000-0000-000000000000""#).is_err());
    assert!(serde_json::from_str::<JobId>(r#""00000000-0000-1000-8000-000000000000""#).is_err());
    assert!(serde_json::from_str::<IdempotencyKey>(r#""bad\nkey""#).is_err());
    assert!(serde_json::from_str::<RequestDigest>(&format!("\"{}\"", "AB".repeat(32))).is_err());
}

#[test]
fn lifecycle_transition_table_is_exhaustive() {
    let statuses = [
        JobStatus::Queued,
        JobStatus::Running,
        JobStatus::Interrupted,
        JobStatus::Paused,
        JobStatus::Succeeded,
        JobStatus::Failed,
        JobStatus::Cancelled,
    ];
    let legal = [
        (JobStatus::Queued, JobStatus::Running),
        (JobStatus::Queued, JobStatus::Failed),
        (JobStatus::Running, JobStatus::Paused),
        (JobStatus::Running, JobStatus::Succeeded),
        (JobStatus::Running, JobStatus::Failed),
        (JobStatus::Paused, JobStatus::Running),
        (JobStatus::Paused, JobStatus::Failed),
        (JobStatus::Queued, JobStatus::Cancelled),
        (JobStatus::Running, JobStatus::Cancelled),
        (JobStatus::Paused, JobStatus::Cancelled),
    ];

    for current in statuses {
        for next in statuses {
            assert_eq!(
                current.allows(next),
                legal.contains(&(current, next)),
                "transition {current} -> {next}"
            );
        }
    }
}

fn admitted_record(admission: Admission) -> JobRecord {
    match admission {
        Admission::Created(record) | Admission::Existing(record) | Admission::Conflict(record) => {
            record
        }
    }
}

#[test]
fn restart_replays_the_same_job() {
    let root = tempfile::tempdir().expect("root");
    let first = JobStore::open(root.path()).expect("store");
    let created = admitted_record(
        first
            .create_or_replay(key("request-1"), digest(1))
            .expect("create"),
    );
    drop(first);

    let restarted = JobStore::open(root.path()).expect("restart");
    let replay = restarted
        .create_or_replay(key("request-1"), digest(1))
        .expect("replay");

    assert_eq!(replay, Admission::Existing(created));
}

#[test]
fn version_two_unsettled_and_terminal_states_migrate_with_new_bindings() {
    let queued_root = tempfile::tempdir().expect("queued root");
    let queued_store = JobStore::open(queued_root.path()).expect("queued store");
    let queued_record = admitted_record(
        queued_store
            .create_or_replay(key("request-v2-queued"), digest(55))
            .expect("create queued"),
    );
    stamp_execution(&queued_store, queued_record.id(), 55);
    drop(queued_store);
    let queued_path = queued_root.path().join("jobs/state.json");
    let mut queued: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queued_path).expect("read queued state"))
            .expect("decode queued state");
    downgrade_state_to_v2(&mut queued);
    let mut encoded = serde_json::to_vec(&queued).expect("encode v2 queued state");
    encoded.push(b'\n');
    std::fs::write(&queued_path, encoded).expect("write v2 queued state");

    let migrated_store = JobStore::open(queued_root.path()).expect("migrate v2 queued state");
    let migrated_record = migrated_store
        .get(queued_record.id())
        .expect("get migrated record")
        .expect("migrated record exists");
    assert_eq!(migrated_record.execution_id(), None);
    assert_eq!(migrated_record.trace_id(), None);
    drop(migrated_store);
    let migrated: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queued_path).expect("read migrated state"))
            .expect("decode migrated state");
    assert_eq!(migrated["version"], 3);
    assert!(migrated["jobs"][0].get("terminal_sequence").is_some());

    let terminal_root = tempfile::tempdir().expect("terminal root");
    let terminal_store = JobStore::open(terminal_root.path()).expect("terminal store");
    let record = admitted_record(
        terminal_store
            .create_or_replay(key("request-v2-terminal"), digest(56))
            .expect("create terminal candidate"),
    );
    stamp_execution(&terminal_store, record.id(), 56);
    transition(&terminal_store, record.id(), JobStatus::Running);
    let receipt = JobReceipt::new(
        record.id().clone(),
        "execution-56",
        "trace-56",
        digest(56).as_str().to_owned(),
        Some("legacy-chain-head".to_owned()),
    )
    .expect("receipt");
    terminal_store
        .settle_with_events(
            record.id(),
            JobStatus::Succeeded,
            &[json!({"kind": "execution.settled", "status": "succeeded"})],
            None,
            Some(receipt.clone()),
        )
        .expect("settle terminal candidate");
    drop(terminal_store);
    let terminal_path = terminal_root.path().join("jobs/state.json");
    let mut terminal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&terminal_path).expect("read terminal state"))
            .expect("decode terminal state");
    downgrade_state_to_v2(&mut terminal);
    let mut encoded = serde_json::to_vec(&terminal).expect("encode v2 terminal state");
    encoded.push(b'\n');
    std::fs::write(&terminal_path, encoded).expect("write v2 terminal state");

    let migrated = JobStore::open(terminal_root.path()).expect("migrate v2 terminal state");
    let migrated_record = migrated
        .get(record.id())
        .expect("read migrated terminal")
        .expect("terminal exists");
    assert_eq!(migrated_record.status(), JobStatus::Succeeded);
    assert_eq!(migrated_record.receipt(), Some(&receipt));
    drop(migrated);
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&terminal_path).expect("read migrated terminal"))
            .expect("decode migrated terminal");
    assert_eq!(persisted["version"], 3);
    assert!(persisted["jobs"][0]["identity_digest"].as_str().is_some());
    assert!(persisted["jobs"][0]["terminal_sequence"].as_u64().is_some());
}

#[test]
fn failed_transition_persists_redacted_error_and_drops_paths() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let created = admitted_record(
        store
            .create_or_replay(key("request-error"), digest(40))
            .expect("create"),
    );
    store
        .transition_with_events(
            created.id(),
            JobStatus::Running,
            &[json!({"kind": "execution.started", "status": "running"})],
        )
        .expect("running");
    let failed = store
        .transition_with_events(
            created.id(),
            JobStatus::Failed,
            &[json!({
                "kind": "execution.settled",
                "status": "failed",
                "code": "NIKA-ASSERT-001",
                "message": "task boom: expected true /tmp/secret.json",
                "path": "/tmp/secret.json"
            })],
        )
        .expect("failed");
    let (code, message) = failed.record().error().expect("diagnosis");
    assert_eq!(code, "NIKA-ASSERT-001");
    assert!(message.contains("task boom"));
    assert!(!message.contains("/tmp"), "{message}");
}

#[test]
fn conflicting_key_reuse_does_not_mutate_the_job() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let created = admitted_record(
        store
            .create_or_replay(key("request-2"), digest(2))
            .expect("create"),
    );

    let conflict = store
        .create_or_replay(key("request-2"), digest(3))
        .expect("conflict verdict");

    assert_eq!(conflict, Admission::Conflict(created.clone()));
    assert_eq!(store.get(created.id()).expect("get"), Some(created));
}

#[test]
fn concurrent_duplicates_create_exactly_one_job() {
    let root = tempfile::tempdir().expect("root");
    let stores = (0..8)
        .map(|_| JobStore::open(root.path()).expect("independent store"))
        .collect::<Vec<_>>();
    let barrier = Arc::new(Barrier::new(8));

    let admissions = std::thread::scope(|scope| {
        stores
            .into_iter()
            .map(|store| {
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    barrier.wait();
                    store
                        .create_or_replay(key("request-concurrent"), digest(4))
                        .expect("admission")
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|worker| worker.join().expect("worker"))
            .collect::<Vec<_>>()
    });
    let created = admissions
        .iter()
        .filter(|admission| matches!(admission, Admission::Created(_)))
        .count();
    let ids: Vec<_> = admissions
        .iter()
        .map(|admission| admission.record().id().clone())
        .collect();

    assert_eq!(created, 1);
    assert!(ids.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(
        JobStore::open(root.path())
            .expect("verification store")
            .load_state()
            .expect("state")
            .jobs
            .len(),
        1,
        "duplicate admission must not leave a second runnable record"
    );
}

#[test]
fn independently_opened_stores_share_one_kernel_lease() {
    let root = tempfile::tempdir().expect("root");
    let first = JobStore::open(root.path()).expect("first store");
    let second = JobStore::open(root.path()).expect("second store");
    let held = first.kernel_lease().expect("first lease");

    assert!(
        second
            .try_kernel_lease()
            .expect("nonblocking second lease")
            .is_none(),
        "a separately opened store must contend on the same kernel lease"
    );
    drop(held);
    assert!(
        second
            .try_kernel_lease()
            .expect("lease after release")
            .is_some()
    );
}

#[test]
fn illegal_transition_refuses_without_mutation() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-transition"), digest(5))
            .expect("create"),
    );

    let error = store
        .transition_with_events(
            record.id(),
            JobStatus::Succeeded,
            &[json!({"status": "succeeded"})],
        )
        .expect_err("queued cannot succeed directly");

    assert!(matches!(error, JobStoreError::IllegalTransition { .. }));
    assert_eq!(
        store
            .get(record.id())
            .expect("get")
            .expect("record")
            .status(),
        JobStatus::Queued
    );
}

#[test]
fn truncated_state_fails_closed() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    store
        .create_or_replay(key("request-torn"), digest(6))
        .expect("create");
    drop(store);

    std::fs::write(
        root.path().join("jobs/state.json"),
        b"{\"version\":1,\"jobs\":[",
    )
    .expect("truncate state");

    assert!(matches!(
        JobStore::open(root.path()),
        Err(JobStoreError::Corrupt(_))
    ));
}

#[test]
fn public_io_error_does_not_disclose_the_durable_root() {
    let root = tempfile::tempdir().expect("root");
    let jobs = root.path().join("jobs");
    std::fs::create_dir(&jobs).expect("jobs");
    std::fs::create_dir(jobs.join("state.json")).expect("state directory");

    let error = JobStore::open(root.path()).expect_err("directory state must refuse");
    let rendered = error.to_string();

    assert!(matches!(
        &error,
        JobStoreError::Io(kind) if *kind == std::io::ErrorKind::InvalidData
    ));
    assert_eq!(rendered, "job store I/O failed: invalid data");
    assert!(!rendered.contains(root.path().to_string_lossy().as_ref()));
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn job_store_debug_is_opaque_to_the_durable_root() {
    let sandbox = tempfile::tempdir().expect("sandbox");
    let sentinel = "SENTINEL-job-store-debug-root";
    let root = sandbox.path().join(sentinel);
    std::fs::create_dir(&root).expect("root");
    let store = JobStore::open(&root).expect("store");

    let rendered = format!("{store:?}");

    assert_eq!(rendered, "JobStore { durable_root: <redacted> }");
    assert!(!rendered.contains(sentinel));
    assert!(!rendered.contains(root.to_string_lossy().as_ref()));
}

#[test]
fn unknown_persisted_fields_fail_closed_without_rewrite() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    drop(store);
    let state_path = root.path().join("jobs/state.json");
    let future = b"{\"version\":1,\"jobs\":[],\"future_authority\":\"must-survive\"}\n";
    std::fs::write(&state_path, future).expect("write future state");

    assert!(matches!(
        JobStore::open(root.path()),
        Err(JobStoreError::Corrupt(_))
    ));
    assert_eq!(
        std::fs::read(state_path).expect("read refused state"),
        future,
        "refusing an unknown field must not erase future authority"
    );
}

#[test]
fn deleted_state_after_empty_initialization_fails_closed_on_restart() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    drop(store);
    let jobs = root.path().join("jobs");

    assert!(jobs.join("initialized.json").is_file());
    assert!(jobs.join("state.json").is_file());
    std::fs::remove_file(jobs.join("state.json")).expect("delete state");

    assert!(matches!(
        JobStore::open(root.path()),
        Err(JobStoreError::Corrupt(_))
    ));
}

#[test]
fn renamed_away_state_fails_closed_on_restart() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    drop(store);
    let jobs = root.path().join("jobs");
    std::fs::rename(jobs.join("state.json"), jobs.join("state.lost")).expect("rename state away");

    assert!(matches!(
        JobStore::open(root.path()),
        Err(JobStoreError::Corrupt(_))
    ));
}

#[test]
fn paused_status_survives_restart() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-paused"), digest(7))
            .expect("create"),
    );
    transition(&store, record.id(), JobStatus::Running);
    transition(&store, record.id(), JobStatus::Paused);
    drop(store);

    let restarted = JobStore::open(root.path()).expect("restart");
    assert_eq!(
        restarted
            .get(record.id())
            .expect("get")
            .expect("record")
            .status(),
        JobStatus::Paused
    );
}

#[cfg(unix)]
#[test]
fn symlink_roots_and_visible_path_replacement_cannot_redirect_state() {
    use std::os::unix::fs::symlink;

    let sandbox = tempfile::tempdir().expect("sandbox");
    let project = sandbox.path().join("project");
    let outside = sandbox.path().join("outside");
    std::fs::create_dir(&project).expect("project");
    std::fs::create_dir(&outside).expect("outside");
    let planted = sandbox.path().join("planted");
    symlink(&outside, &planted).expect("plant root");
    assert!(JobStore::open(&planted).is_err());

    let child_planted = sandbox.path().join("child-planted");
    std::fs::create_dir(&child_planted).expect("child-planted root");
    symlink(&outside, child_planted.join("jobs")).expect("plant jobs child");
    assert!(JobStore::open(&child_planted).is_err());

    let store = JobStore::open(&project).expect("store");
    let held = sandbox.path().join("held-project");
    std::fs::rename(&project, &held).expect("replace visible root");
    symlink(&outside, &project).expect("redirect visible root");
    store
        .create_or_replay(key("request-held"), digest(8))
        .expect("create through held descriptor");

    assert!(held.join("jobs/state.json").is_file());
    assert!(!outside.join("jobs/state.json").exists());
}

#[test]
fn event_sequences_are_monotone_and_resumable() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-events"), digest(9))
            .expect("create"),
    );

    let first = store
        .append_events(
            record.id(),
            &[json!({"kind": "queued"}), json!({"kind": "started"})],
        )
        .expect("first events");
    let second = store
        .append_events(record.id(), &[json!({"kind": "paused"})])
        .expect("second events");

    assert_eq!(
        first
            .iter()
            .chain(&second)
            .map(JobEvent::sequence)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(
        store
            .events_after(record.id(), 1, page_limit(MAX_EVENT_PAGE_LEN))
            .expect("events after")
            .iter()
            .map(JobEvent::sequence)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert!(matches!(
        store
            .events_after(record.id(), 4, page_limit(MAX_EVENT_PAGE_LEN))
            .expect_err("cursor beyond latest sequence"),
        JobStoreError::CursorBeyondLatest {
            after: 4,
            latest: 3,
            ..
        }
    ));
}

#[test]
fn interrupted_running_job_is_settled_before_replay() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-interrupted"), digest(10))
            .expect("create"),
    );
    transition(&store, record.id(), JobStatus::Running);
    let snapshot_digest = digest(48).as_str().to_owned();
    store
        .stamp_execution_identity(
            record.id(),
            "execution-interrupted".to_owned(),
            "trace-interrupted".to_owned(),
            snapshot_digest.clone(),
        )
        .expect("stamp interrupted identity");
    drop(store);

    let restarted = JobStore::open(root.path()).expect("restart");
    assert_eq!(
        restarted
            .get(record.id())
            .expect("get unresolved job")
            .expect("unresolved job exists")
            .status(),
        JobStatus::Running
    );
    assert!(matches!(
        restarted
            .transition_with_events(
                record.id(),
                JobStatus::Interrupted,
                &[json!({"status": "interrupted"})],
            )
            .expect_err("public transition cannot claim restart authority"),
        JobStoreError::IllegalTransition {
            from: JobStatus::Running,
            to: JobStatus::Interrupted,
        }
    ));
    let incarnation = restarted
        .claim_server_incarnation()
        .expect("claim restart generation");
    assert_eq!(
        restarted
            .settle_interrupted_jobs(&incarnation)
            .expect("settle interrupted jobs"),
        1
    );
    let replay = restarted
        .create_or_replay(key("request-interrupted"), digest(10))
        .expect("replay");

    let replayed = admitted_record(replay);
    assert_eq!(replayed.id(), record.id());
    assert_eq!(replayed.status(), JobStatus::Interrupted);
    assert_eq!(replayed.execution_id(), Some("execution-interrupted"));
    assert_eq!(replayed.trace_id(), Some("trace-interrupted"));
    let receipt = replayed.receipt().expect("interrupted receipt");
    assert_eq!(receipt.job_id(), record.id());
    assert_eq!(receipt.execution_id(), "execution-interrupted");
    assert_eq!(receipt.trace_id(), "trace-interrupted");
    assert_eq!(receipt.snapshot_digest(), snapshot_digest);
    assert_eq!(receipt.chain_head(), None);
    assert_eq!(restarted.load_state().expect("state").jobs.len(), 1);

    drop(restarted);
    let reopened = JobStore::open(root.path()).expect("second restart");
    let settled = reopened
        .get(record.id())
        .expect("get settled job")
        .expect("settled job exists");
    assert_eq!(settled.status(), JobStatus::Interrupted);
    assert_eq!(
        reopened
            .settle_interrupted_jobs(&incarnation)
            .expect("settlement is idempotent"),
        0
    );
    assert!(matches!(
        reopened
            .transition_with_events(
                record.id(),
                JobStatus::Running,
                &[json!({"status": "running"})],
            )
            .expect_err("interrupted job cannot silently resume"),
        JobStoreError::IllegalTransition {
            from: JobStatus::Interrupted,
            to: JobStatus::Running,
        }
    ));
}

#[test]
fn identityless_running_job_restarts_as_recoverable_queued_without_receipt() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay_captured(
                key("request-identityless-running"),
                digest(57),
                usize::MAX,
                "root.nika.yaml".to_owned(),
                "captured-world",
            )
            .expect("create captured job"),
    );
    transition(&store, record.id(), JobStatus::Running);
    let incarnation = store.claim_server_incarnation().expect("incarnation");

    assert_eq!(
        store
            .settle_interrupted_jobs(&incarnation)
            .expect("recover identityless running job"),
        0,
        "requeue is not a terminal interruption"
    );
    let recovered = store
        .get(record.id())
        .expect("get recovered job")
        .expect("recovered job exists");
    assert_eq!(recovered.status(), JobStatus::Queued);
    assert_eq!(recovered.execution_id(), None);
    assert_eq!(recovered.trace_id(), None);
    assert_eq!(recovered.receipt(), None);
    assert_eq!(
        store.queued_jobs().expect("restart schedule"),
        vec![(record.id().clone(), "root.nika.yaml".to_owned())]
    );
    let events = store
        .events_after(record.id(), 0, page_limit(MAX_EVENT_PAGE_LEN))
        .expect("recovery events");
    assert_eq!(
        events.last().expect("requeue event").payload()["kind"],
        "execution.requeued"
    );
}

#[test]
fn opening_another_handle_does_not_interrupt_a_live_owner() {
    let root = tempfile::tempdir().expect("root");
    let owner = JobStore::open(root.path()).expect("owner store");
    let incarnation = owner
        .claim_server_incarnation()
        .expect("claim live server lease");
    assert_eq!(
        owner
            .settle_interrupted_jobs(&incarnation)
            .expect("initial settlement"),
        0
    );
    let record = admitted_record(
        owner
            .create_or_replay(key("request-live-owner"), digest(11))
            .expect("create"),
    );
    transition(&owner, record.id(), JobStatus::Running);
    stamp_execution(&owner, record.id(), 11);

    let second = JobStore::open(root.path()).expect("observer store");
    assert!(matches!(
        second
            .claim_server_incarnation()
            .expect_err("second live server must not claim the root"),
        JobStoreError::ServerLeaseHeld
    ));
    let observed = second
        .get(record.id())
        .expect("get running job")
        .expect("running job exists");

    assert_eq!(observed.status(), JobStatus::Running);

    drop(incarnation);
    let successor = second
        .claim_server_incarnation()
        .expect("successor claims released lease");
    assert_eq!(
        second
            .settle_interrupted_jobs(&successor)
            .expect("successor settles prior owner"),
        1
    );
}

#[test]
fn consumed_incarnation_cannot_interrupt_a_new_live_owner() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let first = store
        .claim_server_incarnation()
        .expect("claim first generation");
    assert_eq!(
        store
            .settle_interrupted_jobs(&first)
            .expect("consume first settlement"),
        0
    );
    let live = admitted_record(
        store
            .create_or_replay(key("request-after-settlement"), digest(12))
            .expect("create live job"),
    );
    transition(&store, live.id(), JobStatus::Running);
    stamp_execution(&store, live.id(), 12);

    assert_eq!(
        store
            .settle_interrupted_jobs(&first)
            .expect("repeated settlement is inert"),
        0
    );
    assert_eq!(
        store
            .get(live.id())
            .expect("get live job")
            .expect("live job exists")
            .status(),
        JobStatus::Running
    );

    drop(store);
    let reopened = JobStore::open(root.path()).expect("reopen store");
    assert_eq!(
        reopened
            .settle_interrupted_jobs(&first)
            .expect("persisted settlement stays inert"),
        0
    );
    assert_eq!(
        reopened
            .get(live.id())
            .expect("get owner after reopen")
            .expect("owner survives")
            .status(),
        JobStatus::Running
    );

    drop(first);
    let second = reopened
        .claim_server_incarnation()
        .expect("claim next generation");
    assert_eq!(
        reopened
            .settle_interrupted_jobs(&second)
            .expect("new generation settles prior owner"),
        1
    );
}

#[test]
fn interrupted_status_and_event_persist_together_or_not_at_all() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-interrupted-atomic"), digest(22))
            .expect("create"),
    );
    transition(&store, record.id(), JobStatus::Running);
    stamp_execution(&store, record.id(), 22);
    let before = store
        .events_after(record.id(), 0, page_limit(MAX_EVENT_PAGE_LEN))
        .expect("events before settlement");
    let incarnation = store.claim_server_incarnation().expect("claim incarnation");

    store.fail_next_persist();
    assert!(matches!(
        store
            .settle_interrupted_jobs(&incarnation)
            .expect_err("injected write failure"),
        JobStoreError::Io(std::io::ErrorKind::Other)
    ));
    drop(incarnation);
    drop(store);

    let reopened = JobStore::open(root.path()).expect("reopen after refusal");
    assert_eq!(
        reopened
            .get(record.id())
            .expect("get unchanged status")
            .expect("job exists")
            .status(),
        JobStatus::Running
    );
    assert_eq!(
        reopened
            .events_after(record.id(), 0, page_limit(MAX_EVENT_PAGE_LEN))
            .expect("get unchanged events"),
        before
    );

    let successor = reopened
        .claim_server_incarnation()
        .expect("claim successor");
    assert_eq!(
        reopened
            .settle_interrupted_jobs(&successor)
            .expect("settle atomically"),
        1
    );
    drop(successor);
    drop(reopened);

    let settled = JobStore::open(root.path()).expect("reopen settled state");
    assert_eq!(
        settled
            .get(record.id())
            .expect("get settled status")
            .expect("job exists")
            .status(),
        JobStatus::Interrupted
    );
    let events = settled
        .events_after(record.id(), 0, page_limit(MAX_EVENT_PAGE_LEN))
        .expect("get settlement events");
    assert_eq!(events.len(), before.len() + 1);
    let interruption = events.last().expect("interruption event");
    assert_eq!(interruption.payload()["kind"], "interrupted");
    assert_eq!(interruption.payload()["status"], "interrupted");
    assert!(interruption.payload()["incarnation_generation"].is_u64());
}

#[test]
fn transition_and_events_commit_as_one_mutation() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-atomic-transition"), digest(13))
            .expect("create"),
    );

    assert!(matches!(
        store
            .transition_with_events(record.id(), JobStatus::Running, &[])
            .expect_err("eventless transition refuses"),
        JobStoreError::TransitionEventRequired
    ));
    let oversized = json!("x".repeat(MAX_EVENT_PAYLOAD_BYTES));
    assert!(matches!(
        store
            .transition_with_events(record.id(), JobStatus::Running, &[oversized])
            .expect_err("oversized event refuses transition"),
        JobStoreError::EventPayloadTooLarge { .. }
    ));
    assert_eq!(
        store
            .get(record.id())
            .expect("get unchanged job")
            .expect("job exists")
            .status(),
        JobStatus::Queued
    );
    assert!(
        store
            .events_after(record.id(), 0, page_limit(1))
            .expect("unchanged events")
            .is_empty()
    );

    store.fail_next_persist();
    assert!(matches!(
        store
            .transition_with_events(
                record.id(),
                JobStatus::Running,
                &[json!({"status": "running"})],
            )
            .expect_err("injected transition write failure"),
        JobStoreError::Io(std::io::ErrorKind::Other)
    ));
    drop(store);
    let store = JobStore::open(root.path()).expect("reopen after transition refusal");
    assert_eq!(
        store
            .get(record.id())
            .expect("get refused transition")
            .expect("job exists")
            .status(),
        JobStatus::Queued
    );
    assert!(
        store
            .events_after(record.id(), 0, page_limit(1))
            .expect("refused transition left no events")
            .is_empty()
    );

    let mutation = store
        .transition_with_events(
            record.id(),
            JobStatus::Running,
            &[json!({"status": "running"})],
        )
        .expect("atomic mutation");
    assert_eq!(mutation.record().status(), JobStatus::Running);
    assert_eq!(mutation.events().len(), 1);
    drop(store);

    let reopened = JobStore::open(root.path()).expect("reopen");
    assert_eq!(
        reopened
            .get(record.id())
            .expect("get committed job")
            .expect("job exists")
            .status(),
        JobStatus::Running
    );
    let committed = reopened
        .events_after(record.id(), 0, page_limit(1))
        .expect("committed event");
    assert_eq!(committed.as_slice(), mutation.events());
}

#[cfg(test)]
mod event_integrity;
