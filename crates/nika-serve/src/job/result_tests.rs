// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;

use serde_json::json;

use super::*;

type StateEdit = (&'static str, fn(&mut serde_json::Value));

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::new(value).expect("valid key")
}

fn digest(byte: u8) -> RequestDigest {
    RequestDigest::from_bytes([byte; 32])
}

fn admitted_record(admission: Admission) -> JobRecord {
    match admission {
        Admission::Created(record) | Admission::Existing(record) | Admission::Conflict(record) => {
            record
        }
    }
}

fn transition(store: &JobStore, id: &JobId, status: JobStatus) -> JobMutation {
    store
        .transition_with_events(id, status, &[json!({"status": status.to_string()})])
        .expect("legal transition")
}

fn mismatched_receipts(
    created: &JobId,
    other: &JobId,
    snapshot_digest: &str,
    other_snapshot_digest: &str,
) -> [(&'static str, JobReceipt); 4] {
    [
        (
            "job",
            JobReceipt::new(
                other.clone(),
                "execution-42",
                "trace-42",
                snapshot_digest,
                None,
            )
            .expect("valid mismatching job receipt"),
        ),
        (
            "execution",
            JobReceipt::new(
                created.clone(),
                "other-execution",
                "trace-42",
                snapshot_digest,
                None,
            )
            .expect("valid mismatching execution receipt"),
        ),
        (
            "trace",
            JobReceipt::new(
                created.clone(),
                "execution-42",
                "other-trace",
                snapshot_digest,
                None,
            )
            .expect("valid mismatching trace receipt"),
        ),
        (
            "snapshot",
            JobReceipt::new(
                created.clone(),
                "execution-42",
                "trace-42",
                other_snapshot_digest,
                None,
            )
            .expect("valid mismatching snapshot receipt"),
        ),
    ]
}

#[test]
fn terminal_settlement_requires_complete_result_binding_before_mutation() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let created = admitted_record(
        store
            .create_or_replay(key("request-legacy-result"), digest(41))
            .expect("create"),
    );
    transition(&store, created.id(), JobStatus::Running);
    store
        .stamp_identity(
            created.id(),
            "legacy-execution".to_owned(),
            "legacy-trace".to_owned(),
        )
        .expect("legacy identity");
    assert!(matches!(
        store.transition_with_events(
            created.id(),
            JobStatus::Succeeded,
            &[json!({"kind": "execution.settled", "status": "succeeded"})]
        ),
        Err(JobStoreError::InvalidReceipt)
    ));
    let running = store
        .get(created.id())
        .expect("get running")
        .expect("running exists");
    assert_eq!(running.status(), JobStatus::Running);
}

#[test]
fn execution_and_receipt_identity_mismatches_are_refused_without_mutation() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let created = admitted_record(
        store
            .create_or_replay(key("request-receipt-identity"), digest(42))
            .expect("create"),
    );
    let other = admitted_record(
        store
            .create_or_replay(key("request-receipt-other"), digest(43))
            .expect("create other"),
    );
    transition(&store, created.id(), JobStatus::Running);
    let snapshot_digest = digest(44).as_str().to_owned();
    for (execution_id, trace_id) in [("", "trace-42"), ("execution-42", "")] {
        assert!(matches!(
            store.stamp_execution_identity(
                created.id(),
                execution_id.to_owned(),
                trace_id.to_owned(),
                snapshot_digest.clone(),
            ),
            Err(JobStoreError::InvalidReceipt)
        ));
    }
    store
        .stamp_execution_identity(
            created.id(),
            "execution-42".to_owned(),
            "trace-42".to_owned(),
            snapshot_digest.clone(),
        )
        .expect("stamp identity");
    let other_snapshot_digest = digest(45).as_str().to_owned();

    for (name, receipt) in mismatched_receipts(
        created.id(),
        other.id(),
        &snapshot_digest,
        &other_snapshot_digest,
    ) {
        assert!(
            matches!(
                store.settle_with_events(
                    created.id(),
                    JobStatus::Succeeded,
                    &[json!({"kind": "execution.settled", "status": "succeeded"})],
                    Some(BTreeMap::from([("answer".to_owned(), json!(42))])),
                    Some(receipt),
                ),
                Err(JobStoreError::ReceiptIdentityMismatch)
            ),
            "mismatching {name} identity must refuse"
        );
    }

    for (execution_id, trace_id, digest) in [
        ("other-execution", "trace-42", snapshot_digest.as_str()),
        ("execution-42", "other-trace", snapshot_digest.as_str()),
        ("execution-42", "trace-42", other_snapshot_digest.as_str()),
    ] {
        assert!(matches!(
            store.stamp_execution_identity(
                created.id(),
                execution_id.to_owned(),
                trace_id.to_owned(),
                digest.to_owned(),
            ),
            Err(JobStoreError::ReceiptIdentityMismatch)
        ));
    }

    let unchanged = store
        .get(created.id())
        .expect("get unchanged job")
        .expect("job exists");
    assert_eq!(unchanged.status(), JobStatus::Running);
    assert_eq!(unchanged.execution_id(), Some("execution-42"));
    assert_eq!(unchanged.trace_id(), Some("trace-42"));
    assert_eq!(unchanged.outputs(), None);
    assert_eq!(unchanged.receipt(), None);
}

#[test]
fn persisted_receipt_identity_mismatches_are_refused_on_reopen() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let created = admitted_record(
        store
            .create_or_replay(key("request-persisted-receipt"), digest(42))
            .expect("create"),
    );
    let other = admitted_record(
        store
            .create_or_replay(key("request-persisted-receipt-other"), digest(43))
            .expect("create other"),
    );
    transition(&store, created.id(), JobStatus::Running);
    let snapshot_digest = digest(44).as_str().to_owned();
    let other_snapshot_digest = digest(45).as_str().to_owned();
    store
        .stamp_execution_identity(
            created.id(),
            "execution-42".to_owned(),
            "trace-42".to_owned(),
            snapshot_digest.clone(),
        )
        .expect("stamp identity");

    let receipt = JobReceipt::new(
        created.id().clone(),
        "execution-42",
        "trace-42",
        snapshot_digest,
        None,
    )
    .expect("matching receipt");
    store
        .settle_with_events(
            created.id(),
            JobStatus::Succeeded,
            &[json!({"kind": "execution.settled", "status": "succeeded"})],
            None,
            Some(receipt),
        )
        .expect("matching settlement");
    drop(store);

    let state_path = root.path().join("jobs/state.json");
    let pristine = std::fs::read(&state_path).expect("read pristine state");
    for (field, forged) in [
        ("job_id", json!(other.id().as_str())),
        ("execution_id", json!("forged-execution")),
        ("trace_id", json!("forged-trace")),
        ("snapshot_digest", json!(other_snapshot_digest)),
    ] {
        let mut state: serde_json::Value =
            serde_json::from_slice(&pristine).expect("decode pristine state");
        state["jobs"][0]["record"]["receipt"][field] = forged;
        let mut tampered = serde_json::to_vec(&state).expect("encode tampered state");
        tampered.push(b'\n');
        std::fs::write(&state_path, tampered).expect("write tampered state");
        assert!(
            matches!(JobStore::open(root.path()), Err(JobStoreError::Corrupt(_))),
            "persisted receipt field {field} must be bound to its record"
        );
        std::fs::write(&state_path, &pristine).expect("restore pristine state");
    }
}

#[test]
fn terminal_chain_binds_coherent_identity_outputs_and_receipt_edits() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let created = admitted_record(
        store
            .create_or_replay(key("request-terminal-binding"), digest(52))
            .expect("create"),
    );
    let snapshot_digest = digest(53).as_str().to_owned();
    store
        .stamp_execution_identity(
            created.id(),
            "execution-52".to_owned(),
            "trace-52".to_owned(),
            snapshot_digest.clone(),
        )
        .expect("stamp identity");
    transition(&store, created.id(), JobStatus::Running);
    let receipt = JobReceipt::new(
        created.id().clone(),
        "execution-52",
        "trace-52",
        snapshot_digest,
        Some("chain-head-52".to_owned()),
    )
    .expect("receipt");
    store
        .settle_with_events(
            created.id(),
            JobStatus::Succeeded,
            &[json!({"kind": "execution.settled", "status": "succeeded"})],
            Some(BTreeMap::from([("answer".to_owned(), json!(52))])),
            Some(receipt),
        )
        .expect("settle");
    drop(store);

    let state_path = root.path().join("jobs/state.json");
    let pristine = std::fs::read(&state_path).expect("read pristine state");
    let edits: [StateEdit; 5] = [
        ("execution", |state| {
            state["jobs"][0]["record"]["execution_id"] = json!("forged-execution");
            state["jobs"][0]["record"]["receipt"]["execution_id"] = json!("forged-execution");
        }),
        ("trace", |state| {
            state["jobs"][0]["record"]["trace_id"] = json!("forged-trace");
            state["jobs"][0]["record"]["receipt"]["trace_id"] = json!("forged-trace");
        }),
        ("snapshot", |state| {
            let forged = digest(54).as_str().to_owned();
            state["jobs"][0]["record"]["snapshot_digest"] = json!(forged);
            state["jobs"][0]["record"]["receipt"]["snapshot_digest"] =
                state["jobs"][0]["record"]["snapshot_digest"].clone();
        }),
        ("outputs", |state| {
            state["jobs"][0]["record"]["outputs"]["answer"] = json!(999);
        }),
        ("receipt", |state| {
            state["jobs"][0]["record"]["receipt"]["chain_head"] = json!("forged-head");
        }),
    ];
    for (name, edit) in edits {
        let mut state: serde_json::Value =
            serde_json::from_slice(&pristine).expect("decode pristine state");
        edit(&mut state);
        let mut tampered = serde_json::to_vec(&state).expect("encode tampered state");
        tampered.push(b'\n');
        std::fs::write(&state_path, tampered).expect("write tampered state");
        assert!(
            matches!(JobStore::open(root.path()), Err(JobStoreError::Corrupt(_))),
            "coherent {name} edit must break terminal proof"
        );
        std::fs::write(&state_path, &pristine).expect("restore pristine state");
    }
}

#[test]
fn running_identity_binding_refuses_forgery_before_restart_receipt() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let created = admitted_record(
        store
            .create_or_replay(key("request-running-binding"), digest(60))
            .expect("create"),
    );
    store
        .stamp_execution_identity(
            created.id(),
            "execution-60".to_owned(),
            "trace-60".to_owned(),
            digest(61).as_str().to_owned(),
        )
        .expect("stamp identity");
    transition(&store, created.id(), JobStatus::Running);
    drop(store);

    let state_path = root.path().join("jobs/state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_path).expect("read state"))
            .expect("decode state");
    state["jobs"][0]["record"]["execution_id"] = json!("forged-execution");
    let mut tampered = serde_json::to_vec(&state).expect("encode tampered state");
    tampered.push(b'\n');
    std::fs::write(&state_path, tampered).expect("write tampered state");

    assert!(matches!(
        JobStore::open(root.path()),
        Err(JobStoreError::Corrupt(_))
    ));
}

#[test]
fn result_data_is_terminal_only_and_preserves_failed_identity() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let created = admitted_record(
        store
            .create_or_replay(key("request-terminal-only"), digest(46))
            .expect("create"),
    );
    let snapshot_digest = digest(47).as_str().to_owned();
    store
        .stamp_execution_identity(
            created.id(),
            "execution-46".to_owned(),
            "trace-46".to_owned(),
            snapshot_digest.clone(),
        )
        .expect("stamp identity");
    let receipt = JobReceipt::new(
        created.id().clone(),
        "execution-46",
        "trace-46",
        snapshot_digest,
        Some("chain-head-46".to_owned()),
    )
    .expect("receipt");
    let outputs = BTreeMap::from([("answer".to_owned(), json!(46))]);

    assert!(matches!(
        store.settle_with_events(
            created.id(),
            JobStatus::Running,
            &[json!({"kind": "execution.started", "status": "running"})],
            Some(outputs.clone()),
            Some(receipt.clone()),
        ),
        Err(JobStoreError::InvalidReceipt)
    ));
    let queued = store
        .get(created.id())
        .expect("get queued")
        .expect("queued exists");
    assert_eq!(queued.status(), JobStatus::Queued);
    assert_eq!(queued.outputs(), None);
    assert_eq!(queued.receipt(), None);

    transition(&store, created.id(), JobStatus::Running);
    let failed = store
        .settle_with_events(
            created.id(),
            JobStatus::Failed,
            &[json!({
                "kind": "execution.settled",
                "status": "failed",
                "code": "NIKA-TEST-001",
                "message": "expected failure"
            })],
            Some(outputs.clone()),
            Some(receipt.clone()),
        )
        .expect("failed settlement");
    assert_eq!(failed.record().execution_id(), Some("execution-46"));
    assert_eq!(failed.record().trace_id(), Some("trace-46"));
    assert_eq!(failed.record().outputs(), Some(&outputs));
    assert_eq!(failed.record().receipt(), Some(&receipt));

    drop(store);
    let reopened = JobStore::open(root.path()).expect("reopen failed settlement");
    let persisted = reopened
        .get(created.id())
        .expect("get failed")
        .expect("failed exists");
    assert_eq!(persisted.execution_id(), Some("execution-46"));
    assert_eq!(persisted.trace_id(), Some("trace-46"));
    assert_eq!(persisted.outputs(), Some(&outputs));
    assert_eq!(persisted.receipt(), Some(&receipt));
    drop(reopened);

    let state_path = root.path().join("jobs/state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_path).expect("read terminal state"))
            .expect("decode terminal state");
    state["jobs"][0]["record"]["status"] = json!("running");
    let mut forged = serde_json::to_vec(&state).expect("encode unsettled result state");
    forged.push(b'\n');
    std::fs::write(&state_path, forged).expect("write unsettled result state");
    assert!(matches!(
        JobStore::open(root.path()),
        Err(JobStoreError::Corrupt(_))
    ));
}

/// ADR-130 · ONE mapping, the words equal by construction: every run
/// state's wire word is the job status it projects to, and back; the
/// job's own words (`queued` · `running` · `interrupted`) are never a run
/// state.
#[test]
fn the_job_status_words_are_the_settlements() {
    use nika_event::settlement::RunState;
    for state in [
        RunState::Succeeded,
        RunState::Failed,
        RunState::Paused,
        RunState::Cancelled,
    ] {
        let status = crate::JobStatus::from(state);
        let word = serde_json::to_value(state).expect("a state serializes");
        assert_eq!(
            serde_json::Value::String(status.to_string()),
            word,
            "{status}"
        );
        assert_eq!(status.run_state(), Some(state), "{status}");
    }
    for own in [
        crate::JobStatus::Queued,
        crate::JobStatus::Running,
        crate::JobStatus::Interrupted,
    ] {
        assert_eq!(
            own.run_state(),
            None,
            "{own} is the job's own word, never a run state"
        );
    }
}
