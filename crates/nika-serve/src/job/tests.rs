use std::sync::{Arc, Barrier};

use serde_json::json;

use super::*;

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::new(value).expect("valid key")
}

fn digest(byte: u8) -> RequestDigest {
    RequestDigest::from_bytes([byte; 32])
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
        .transition(record.id(), JobStatus::Succeeded)
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
    store
        .transition(record.id(), JobStatus::Running)
        .expect("running");
    store
        .transition(record.id(), JobStatus::Paused)
        .expect("paused");
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
            .events_after(record.id(), 1)
            .expect("events after")
            .iter()
            .map(JobEvent::sequence)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert!(matches!(
        store
            .events_after(record.id(), 4)
            .expect_err("cursor beyond latest sequence"),
        JobStoreError::CursorBeyondLatest {
            after: 4,
            latest: 3,
            ..
        }
    ));
}

#[test]
fn interrupted_running_job_replays_instead_of_creating_a_second_runnable_job() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-interrupted"), digest(10))
            .expect("create"),
    );
    let running = store
        .transition(record.id(), JobStatus::Running)
        .expect("running");
    drop(store);

    let restarted = JobStore::open(root.path()).expect("restart");
    let replay = restarted
        .create_or_replay(key("request-interrupted"), digest(10))
        .expect("replay");

    assert_eq!(replay, Admission::Existing(running));
    assert_eq!(restarted.load_state().expect("state").jobs.len(), 1);
}
