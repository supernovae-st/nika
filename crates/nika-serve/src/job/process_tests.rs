// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Process-boundary recovery tests. Children only touch one temporary root;
//! no server, credentials, model, network or workflow executor is started.
//! SIGKILL bypasses destructors. It does not simulate a power failure.

use std::io::{BufRead as _, Write as _};
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::Duration;

use serde_json::json;

use super::*;

const WORKER: &str = "job::process_tests::subprocess_worker";
const MARKER: &str = "NIKA_PROCESS_BOUNDARY_READY";
const ROOT_ENV: &str = "NIKA_JOB_PROCESS_TEST_ROOT";
const MODE_ENV: &str = "NIKA_JOB_PROCESS_TEST_MODE";

fn key() -> IdempotencyKey {
    IdempotencyKey::new("session-command-1").expect("fixture key")
}

fn digest() -> RequestDigest {
    RequestDigest::from_bytes([73; 32])
}

fn record(store: &JobStore) -> JobRecord {
    let admission = store.replay(&key(), &digest()).expect("replay by key");
    let Some(Admission::Existing(record)) = admission else {
        panic!("the acknowledged identity must survive the process: {admission:?}");
    };
    record
}

fn snapshot(root: &Path) -> Vec<u8> {
    std::fs::read(root.join("jobs/state.json")).expect("durable snapshot")
}

/// The guard kills and reaps on success, panic and handshake timeout. A
/// standard child is used so Drop can reap synchronously without an async
/// runtime; ownership never escapes this test fixture.
struct Worker {
    child: Child,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn start_worker(root: &Path, mode: &str) -> Worker {
    // Test-owned process: synchronous Drop must kill and reap on panic.
    #[allow(clippy::disallowed_types)]
    let child = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", WORKER, "--ignored", "--nocapture"])
        .env_clear()
        .env(ROOT_ENV, root)
        .env(MODE_ENV, mode)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn isolated worker");
    let mut worker = Worker {
        child,
        reader: None,
    };
    let output = worker.child.stdout.take().expect("piped stdout");
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    // Test-owned pipe reader, joined by the guard even without a Tokio runtime.
    #[allow(clippy::disallowed_methods)]
    let reader = std::thread::spawn(move || {
        let result = (|| -> std::io::Result<bool> {
            for line in std::io::BufReader::new(output).lines().take(32) {
                if line? == MARKER {
                    return Ok(true);
                }
            }
            Ok(false)
        })();
        let _ = send.send(result);
    });
    worker.reader = Some(reader);
    assert!(
        receive
            .recv_timeout(Duration::from_secs(15))
            .expect("bounded worker handshake")
            .expect("read handshake"),
        "worker exited without a durable boundary"
    );
    worker
}

fn kill_worker(mut worker: Worker) {
    assert!(
        worker.child.try_wait().expect("worker status").is_none(),
        "worker must still be alive before the crash"
    );
    worker.child.kill().expect("kill this worker only");
    let status = worker.child.wait().expect("reap worker");
    assert_eq!(status.signal(), Some(9), "the crash must be SIGKILL");
}

fn settle(store: &JobStore, record: &JobRecord) {
    let receipt = receipt(record);
    store
        .settle_with_events(
            record.id(),
            JobStatus::Succeeded,
            &[json!({
                "kind": "execution.settled", "status": "succeeded",
                "settlement": {"status": "succeeded", "cause": "normal"},
            })],
            Some(std::collections::BTreeMap::from([(
                "answer".into(),
                json!("persisted-result"),
            )])),
            Some(receipt),
        )
        .expect("commit terminal event and receipt");
}

fn receipt(record: &JobRecord) -> JobReceipt {
    JobReceipt::new(
        record.id().clone(),
        "execution-fixture",
        "trace-fixture",
        digest().as_str(),
        None,
    )
    .expect("fixture receipt binding")
}

#[test]
#[ignore = "child entry; launched with an isolated root by the process tests"]
fn subprocess_worker() {
    // The parent clears the environment and supplies only these fixture inputs.
    // Neither value is a credential or a production configuration lookup.
    #[allow(clippy::disallowed_methods)]
    let root = std::path::PathBuf::from(std::env::var_os(ROOT_ENV).expect("worker root"));
    #[allow(clippy::disallowed_methods)]
    let mode = std::env::var(MODE_ENV).expect("worker mode");
    let store = JobStore::open(&root).expect("open worker store");
    let _incarnation = store
        .claim_server_incarnation()
        .expect("worker owns resident lease");
    let admission = store
        .create_or_replay(key(), digest())
        .expect("durable admission");
    let Admission::Created(created) = admission else {
        panic!("worker fixture starts with one new command");
    };
    if mode != "admitted" {
        store
            .start_queued_execution(
                created.id(),
                "execution-fixture".into(),
                "trace-fixture".into(),
                digest().as_str().to_owned(),
                &[json!({"kind": "execution.started"})],
            )
            .expect("durable execution claim");
    }
    if matches!(mode.as_str(), "effect" | "settled" | "failed-settlement") {
        // Independent observable effect: recovery never reads this file.
        let mut effect = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(root.join("recipient-effect"))
            .expect("one external effect");
        effect.write_all(b"one effect\n").expect("write effect");
        effect.sync_all().expect("sync effect file");
    }
    match mode.as_str() {
        "admitted" | "claimed" | "effect" => {}
        "settled" => settle(&store, &created),
        "failed-settlement" => {
            store.fail_next_persist();
            let failure = store.settle_with_events(
                created.id(),
                JobStatus::Failed,
                &[json!({"kind": "execution.settled", "status": "failed"})],
                None,
                Some(receipt(&created)),
            );
            assert!(matches!(failure, Err(JobStoreError::Io(_))), "{failure:?}");
        }
        _ => panic!("unknown worker fixture mode"),
    }
    writeln!(std::io::stdout(), "{MARKER}").expect("announce committed boundary");
    std::io::stdout().flush().expect("flush handshake");
    loop {
        std::thread::park();
    }
}

#[test]
fn admission_survives_sigkill_before_the_client_receives_its_job_id() {
    let root = tempfile::tempdir().expect("root");
    let child = start_worker(root.path(), "admitted");
    // The parent deliberately has no job id or response from the worker.
    kill_worker(child);
    let reopened = JobStore::open_fail_fast(root.path()).expect("restart");
    let first = record(&reopened);
    assert_eq!(first.status(), JobStatus::Queued);
    let before = snapshot(root.path());
    assert_eq!(
        reopened
            .create_or_replay(key(), digest())
            .expect("duplicate"),
        Admission::Existing(first)
    );
    assert_eq!(
        snapshot(root.path()),
        before,
        "replay is not another admission"
    );
    assert!(!root.path().join("recipient-effect").exists());
}

#[test]
fn a_live_process_keeps_its_lease_and_sigkill_releases_it() {
    let root = tempfile::tempdir().expect("root");
    let child = start_worker(root.path(), "claimed");
    let observer = JobStore::open_fail_fast(root.path()).expect("read-only observation");
    let before = snapshot(root.path());
    assert!(matches!(
        observer.claim_server_incarnation(),
        Err(JobStoreError::ServerLeaseHeld)
    ));
    assert_eq!(record(&observer).status(), JobStatus::Running);
    assert_eq!(
        snapshot(root.path()),
        before,
        "losing process cannot rewrite ownership"
    );
    kill_worker(child);
    let successor = observer
        .claim_server_incarnation()
        .expect("kernel released dead writer");
    assert_eq!(
        observer
            .settle_interrupted_jobs(&successor)
            .expect("recover"),
        1
    );
    assert_eq!(record(&observer).status(), JobStatus::Interrupted);
}

#[test]
fn lost_effect_response_is_interrupted_and_never_blindly_reexecuted() {
    for mode in ["claimed", "effect", "failed-settlement"] {
        let root = tempfile::tempdir().expect("root");
        let child = start_worker(root.path(), mode);
        kill_worker(child);
        let reopened = JobStore::open_fail_fast(root.path()).expect("restart");
        let successor = reopened.claim_server_incarnation().expect("new owner");
        assert_eq!(
            reopened
                .settle_interrupted_jobs(&successor)
                .expect("interrupt"),
            1
        );
        let interrupted = record(&reopened);
        assert_eq!(interrupted.status(), JobStatus::Interrupted, "{mode}");
        assert!(
            interrupted.receipt().is_some(),
            "interruption receipt, not success"
        );
        assert!(interrupted.outputs().is_none(), "no invented answer");
        let before = snapshot(root.path());
        assert!(matches!(
            reopened.start_queued_execution(
                interrupted.id(),
                "retry".into(),
                "retry-trace".into(),
                digest().as_str().into(),
                &[json!({"kind": "execution.started"})]
            ),
            Err(JobStoreError::IllegalTransition { .. })
        ));
        assert_eq!(
            snapshot(root.path()),
            before,
            "refused retry leaves the store intact"
        );
        assert_eq!(
            reopened
                .settle_interrupted_jobs(&successor)
                .expect("repeat recovery"),
            0
        );
        assert_eq!(
            root.path().join("recipient-effect").exists(),
            mode != "claimed"
        );
        if mode != "claimed" {
            assert_eq!(
                std::fs::read(root.path().join("recipient-effect")).expect("effect"),
                b"one effect\n"
            );
        }
    }
}

#[test]
fn a_committed_receipt_survives_loss_of_the_transport_response() {
    let root = tempfile::tempdir().expect("root");
    let child = start_worker(root.path(), "settled");
    kill_worker(child);
    let reopened = JobStore::open_fail_fast(root.path()).expect("restart");
    let successor = reopened.claim_server_incarnation().expect("new owner");
    assert_eq!(
        reopened
            .settle_interrupted_jobs(&successor)
            .expect("recover"),
        0
    );
    let saved = record(&reopened);
    assert_eq!(saved.status(), JobStatus::Succeeded);
    assert_eq!(
        saved.outputs().expect("durable output")["answer"],
        "persisted-result"
    );
    assert_eq!(
        saved.receipt().expect("receipt").execution_id(),
        "execution-fixture"
    );
    let before = snapshot(root.path());
    assert!(matches!(
        reopened
            .create_or_replay(key(), RequestDigest::from_bytes([74; 32]))
            .expect("conflicting request"),
        Admission::Conflict(_)
    ));
    assert_eq!(
        snapshot(root.path()),
        before,
        "same key cannot accept different content"
    );
}

#[test]
fn a_corrupt_store_after_process_loss_is_not_an_empty_session() {
    let root = tempfile::tempdir().expect("root");
    let child = start_worker(root.path(), "admitted");
    kill_worker(child);
    let before = snapshot(root.path());
    for bad in [b"{\"version\":".as_slice(), b"{}".as_slice()] {
        std::fs::write(root.path().join("jobs/state.json"), bad).expect("inject corruption");
        assert!(matches!(
            JobStore::open_fail_fast(root.path()),
            Err(JobStoreError::Corrupt(_))
        ));
        assert_eq!(
            snapshot(root.path()),
            bad,
            "opening does not silently erase corruption"
        );
    }
    std::fs::write(root.path().join("jobs/state.json"), before).expect("restore fixture bytes");
    assert_eq!(
        record(&JobStore::open_fail_fast(root.path()).expect("valid store")).status(),
        JobStatus::Queued
    );
}
