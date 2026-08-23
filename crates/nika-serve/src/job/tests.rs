use std::collections::BTreeSet;
use std::sync::{Arc, Barrier, Mutex};

use serde_json::json;

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
    ];
    let legal = [
        (JobStatus::Queued, JobStatus::Running),
        (JobStatus::Queued, JobStatus::Failed),
        (JobStatus::Running, JobStatus::Paused),
        (JobStatus::Running, JobStatus::Succeeded),
        (JobStatus::Running, JobStatus::Failed),
        (JobStatus::Paused, JobStatus::Running),
        (JobStatus::Paused, JobStatus::Failed),
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

#[test]
fn event_bounds_refuse_before_durable_mutation() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-event-bounds"), digest(14))
            .expect("create"),
    );
    let oversized_batch = vec![json!(null); MAX_EVENT_BATCH_LEN + 1];
    assert!(matches!(
        store
            .append_events(record.id(), &oversized_batch)
            .expect_err("batch cap"),
        JobStoreError::EventBatchTooLarge { .. }
    ));
    let oversized_payload = json!("x".repeat(MAX_EVENT_PAYLOAD_BYTES));
    assert!(matches!(
        store
            .append_events(record.id(), &[oversized_payload])
            .expect_err("payload cap"),
        JobStoreError::EventPayloadTooLarge { index: 0, .. }
    ));
    assert!(
        store
            .events_after(record.id(), 0, page_limit(1))
            .expect("empty journal")
            .is_empty()
    );
}

#[test]
fn event_pages_are_bounded_and_resume_without_overlap() {
    assert!(matches!(
        EventPageLimit::new(0).expect_err("zero page"),
        JobStoreError::InvalidEventPageLimit { requested: 0, .. }
    ));
    assert!(EventPageLimit::new(MAX_EVENT_PAGE_LEN + 1).is_err());

    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-pages"), digest(15))
            .expect("create"),
    );
    store
        .append_events(
            record.id(),
            &[json!({"n": 1}), json!({"n": 2}), json!({"n": 3})],
        )
        .expect("append events");

    let first = store
        .events_after(record.id(), 0, page_limit(2))
        .expect("first page");
    let second = store
        .events_after(record.id(), 2, page_limit(2))
        .expect("second page");
    assert_eq!(
        first.iter().map(JobEvent::sequence).collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        second.iter().map(JobEvent::sequence).collect::<Vec<_>>(),
        [3]
    );
}

#[test]
fn oversized_snapshot_refuses_load_and_persist_without_rewrite() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-snapshot-bound"), digest(16))
            .expect("create"),
    );
    let maximum_payload = json!("x".repeat(MAX_EVENT_PAYLOAD_BYTES - 2));
    let maximum_batch = vec![maximum_payload; MAX_EVENT_BATCH_LEN];
    assert!(matches!(
        store
            .append_events(record.id(), &maximum_batch)
            .expect_err("snapshot cap"),
        JobStoreError::SnapshotTooLarge { .. }
    ));
    assert!(
        store
            .events_after(record.id(), 0, page_limit(1))
            .expect("refused append left no events")
            .is_empty()
    );
    drop(store);

    let state_path = root.path().join("jobs/state.json");
    let state = std::fs::OpenOptions::new()
        .write(true)
        .open(&state_path)
        .expect("open state");
    state
        .set_len(MAX_JOB_SNAPSHOT_BYTES as u64 + 1)
        .expect("extend state");
    drop(state);
    let refused = JobStore::open(root.path()).expect_err("oversized state");
    assert!(matches!(
        refused,
        JobStoreError::SnapshotTooLarge {
            bytes,
            maximum: MAX_JOB_SNAPSHOT_BYTES,
        } if bytes == MAX_JOB_SNAPSHOT_BYTES as u64 + 1
    ));
    assert_eq!(
        std::fs::metadata(state_path).expect("state metadata").len(),
        MAX_JOB_SNAPSHOT_BYTES as u64 + 1,
        "refused oversized state must not be rewritten"
    );
}

#[test]
fn approval_event_without_history_authority_fails_closed() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-unanchored-approval"), digest(27))
            .expect("create"),
    );

    assert!(matches!(
        store
            .append_events(
                record.id(),
                &[json!({
                    "digest": digest(28).as_str(),
                    "decision": "allow",
                    "kind": "approval_decided"
                })],
            )
            .expect_err("approval without authority must refuse"),
        JobStoreError::ApprovalHistoryRequired
    ));
    assert!(
        store
            .events_after(record.id(), 0, page_limit(1))
            .expect("refused append leaves no event")
            .is_empty()
    );
}

#[test]
fn approval_decision_digest_is_mandatory_and_chain_bound() {
    let root = tempfile::tempdir().expect("root");
    let history = Arc::new(MemoryApprovalHistory::default());
    let store = JobStore::open_with_approval_history(root.path(), history.clone()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-approval-chain"), digest(17))
            .expect("create"),
    );
    assert!(matches!(
        store
            .append_events(record.id(), &[json!({"kind": "approval_decided"})])
            .expect_err("missing approval digest"),
        JobStoreError::InvalidApprovalEvent
    ));

    let approval_digest = digest(18).as_str().to_owned();
    let events = store
        .append_events(
            record.id(),
            &[json!({
                "digest": approval_digest,
                "decision": "allow",
                "kind": "approval_decided"
            })],
        )
        .expect("append approval decision");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].previous_hash(), None);
    assert_eq!(events[0].hash().len(), 64);
    let replay_job = admitted_record(
        store
            .create_or_replay(key("request-approval-replay"), digest(23))
            .expect("create replay target"),
    );
    assert!(matches!(
        store
            .append_events(replay_job.id(), &[events[0].payload().clone()])
            .expect_err("claim digest is globally one-shot"),
        JobStoreError::ApprovalClaimAlreadyRecorded
    ));
    drop(store);

    assert!(matches!(
        JobStore::open(root.path()),
        Err(JobStoreError::ApprovalHistoryRequired)
    ));
    assert!(matches!(
        JobStore::open_with_approval_history(
            root.path(),
            Arc::new(MemoryApprovalHistory::default())
        ),
        Err(JobStoreError::ApprovalHistoryMismatch)
    ));

    let state_path = root.path().join("jobs/state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_path).expect("read state"))
            .expect("decode state");
    state["jobs"][0]["events"][0]["payload"]["digest"] = json!(digest(19).as_str());
    let mut forged = serde_json::to_vec(&state).expect("encode forged state");
    forged.push(b'\n');
    std::fs::write(&state_path, forged).expect("write forged state");

    assert!(matches!(
        JobStore::open_with_approval_history(root.path(), history),
        Err(JobStoreError::Corrupt(_))
    ));
}

#[test]
fn approval_history_refuses_reuse_after_coordinated_suffix_rollback() {
    let root = tempfile::tempdir().expect("root");
    let history = Arc::new(MemoryApprovalHistory::default());
    let store = JobStore::open_with_approval_history(root.path(), history.clone()).expect("store");
    let first = admitted_record(
        store
            .create_or_replay(key("request-approval-first"), digest(24))
            .expect("first job"),
    );
    let second = admitted_record(
        store
            .create_or_replay(key("request-approval-second"), digest(25))
            .expect("second job"),
    );
    let approval = json!({
        "digest": digest(26).as_str(),
        "decision": "allow",
        "kind": "approval_decided"
    });
    store
        .append_events(first.id(), &[json!({"kind": "progress"})])
        .expect("record retained prefix");
    store
        .append_events(first.id(), std::slice::from_ref(&approval))
        .expect("record approval");
    drop(store);

    let state_path = root.path().join("jobs/state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_path).expect("read state"))
            .expect("decode state");
    let retained_head = state["jobs"][0]["events"][0]["hash"].clone();
    state["jobs"][0]["events"]
        .as_array_mut()
        .expect("events array")
        .truncate(1);
    state["jobs"][0]["event_count"] = json!(1);
    state["jobs"][0]["event_head"] = retained_head;
    let mut rolled_back = serde_json::to_vec(&state).expect("encode coordinated rollback");
    rolled_back.push(b'\n');
    std::fs::write(&state_path, rolled_back).expect("write coordinated rollback");

    let reopened =
        JobStore::open_with_approval_history(root.path(), history).expect("coherent state reopens");
    let retained = reopened
        .events_after(first.id(), 0, page_limit(1))
        .expect("rolled-back suffix");
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].payload()["kind"], "progress");
    assert!(matches!(
        reopened
            .append_events(second.id(), &[approval])
            .expect_err("authority refuses reuse after rollback"),
        JobStoreError::ApprovalClaimAlreadyRecorded
    ));
    assert!(
        reopened
            .events_after(second.id(), 0, page_limit(1))
            .expect("refused append leaves no event")
            .is_empty()
    );
}

#[test]
fn event_chain_rejects_modification_interior_deletion_permutation_and_graft() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let first = admitted_record(
        store
            .create_or_replay(key("request-chain-a"), digest(20))
            .expect("first job"),
    );
    let second = admitted_record(
        store
            .create_or_replay(key("request-chain-b"), digest(21))
            .expect("second job"),
    );
    for (record, label) in [(&first, "a"), (&second, "b")] {
        store
            .append_events(
                record.id(),
                &[
                    json!({"kind": "progress", "value": format!("{label}-1")}),
                    json!({"kind": "progress", "value": format!("{label}-2")}),
                    json!({"kind": "progress", "value": format!("{label}-3")}),
                ],
            )
            .expect("append chain");
    }
    drop(store);

    let state_path = root.path().join("jobs/state.json");
    let pristine = std::fs::read(&state_path).expect("read pristine state");
    assert_tamper_refused(root.path(), &pristine, |state| {
        state["jobs"][0]["events"][1]["payload"]["value"] = json!("forged");
    });
    assert_tamper_refused(root.path(), &pristine, |state| {
        let retained_head = {
            let events = state["jobs"][0]["events"]
                .as_array_mut()
                .expect("events array");
            events.remove(1);
            events.last().expect("retained tail")["hash"].clone()
        };
        state["jobs"][0]["event_count"] = json!(2);
        state["jobs"][0]["event_head"] = retained_head;
    });
    assert_tamper_refused(root.path(), &pristine, |state| {
        state["jobs"][0]["events"]
            .as_array_mut()
            .expect("events array")
            .swap(0, 1);
    });
    assert_tamper_refused(root.path(), &pristine, |state| {
        let graft = state["jobs"][1]["events"][1].clone();
        state["jobs"][0]["events"][1] = graft;
    });
}

fn assert_tamper_refused(
    root: &std::path::Path,
    pristine: &[u8],
    mutate: impl FnOnce(&mut serde_json::Value),
) {
    let state_path = root.join("jobs/state.json");
    let mut state: serde_json::Value = serde_json::from_slice(pristine).expect("decode state");
    mutate(&mut state);
    let mut tampered = serde_json::to_vec(&state).expect("encode tampered state");
    tampered.push(b'\n');
    std::fs::write(&state_path, tampered).expect("write tampered state");
    assert!(matches!(
        JobStore::open(root),
        Err(JobStoreError::Corrupt(_))
    ));
    std::fs::write(state_path, pristine).expect("restore pristine state");
}

#[test]
fn wire_job_id_parse_accepts_only_canonical_uuid_v4() {
    let id = JobId::random();
    assert_eq!(
        JobId::parse(id.as_str()).expect("canonical").as_str(),
        id.as_str()
    );
    assert!(matches!(
        JobId::parse("not-a-job-id"),
        Err(JobStoreError::InvalidJobId)
    ));
    assert!(JobId::parse("00000000-0000-4000-8000-000000000000").is_ok());
    assert!(matches!(
        JobId::parse("00000000-0000-1000-8000-000000000000"),
        Err(JobStoreError::InvalidJobId)
    ));
}

#[test]
fn bounded_create_refuses_a_new_identity_and_keeps_replay() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let first = admitted_record(
        store
            .create_or_replay_bounded(key("capacity-first"), digest(30), 1)
            .expect("create"),
    );
    let replay = store
        .create_or_replay_bounded(key("capacity-first"), digest(30), 1)
        .expect("replay");
    assert_eq!(replay, Admission::Existing(first));
    assert!(matches!(
        store.create_or_replay_bounded(key("capacity-second"), digest(31), 1),
        Err(JobStoreError::CapacityExceeded)
    ));
}

#[test]
fn live_interrupt_requires_running_and_the_current_incarnation() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let created = admitted_record(
        store
            .create_or_replay(key("request-live-interrupt"), digest(32))
            .expect("create"),
    );
    let running = transition(&store, created.id(), JobStatus::Running);
    let incarnation = store.claim_server_incarnation().expect("incarnation");
    let payload = json!({"kind": "execution.interrupted", "status": "interrupted"});
    let interrupted = store
        .interrupt_running(running.record().id(), &incarnation, &payload)
        .expect("interrupt");
    assert_eq!(interrupted.status(), JobStatus::Interrupted);
    assert!(matches!(
        store.interrupt_running(interrupted.id(), &incarnation, &payload),
        Err(JobStoreError::IllegalTransition { .. })
    ));
}
