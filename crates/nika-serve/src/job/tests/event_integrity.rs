// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

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
fn snapshot_limit_is_sixteen_mib_and_never_truncates_or_rewrites() {
    assert_eq!(MAX_JOB_SNAPSHOT_BYTES, 16 * 1024 * 1024);
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let record = admitted_record(
        store
            .create_or_replay(key("request-snapshot-bound"), digest(16))
            .expect("create"),
    );
    let maximum_payload = json!("x".repeat(MAX_EVENT_PAYLOAD_BYTES - 2));
    let maximum_batch = vec![maximum_payload; MAX_EVENT_BATCH_LEN];
    for _ in 0..3 {
        store
            .append_events(record.id(), &maximum_batch)
            .expect("three maximum batches remain below 16 MiB");
    }
    let state_path = root.path().join("jobs/state.json");
    let intact = std::fs::read(&state_path).expect("read intact snapshot");
    assert!(
        intact.len() > 4 * 1024 * 1024,
        "the former 4 MiB ceiling must be crossed explicitly"
    );
    assert!(intact.len() <= MAX_JOB_SNAPSHOT_BYTES);
    let events = store
        .events_after(record.id(), 0, page_limit(MAX_EVENT_PAGE_LEN))
        .expect("read intact events");
    assert_eq!(events.len(), MAX_EVENT_BATCH_LEN * 3);
    assert!(
        events
            .iter()
            .all(|event| event.payload() == &maximum_batch[0])
    );
    assert!(matches!(
        store
            .append_events(record.id(), &maximum_batch)
            .expect_err("fourth maximum batch crosses 16 MiB"),
        JobStoreError::SnapshotTooLarge { .. }
    ));
    assert_eq!(
        std::fs::read(&state_path).expect("read refused snapshot"),
        intact,
        "an oversized replacement must leave the durable snapshot byte-exact"
    );
    drop(store);

    let reopened = JobStore::open(root.path()).expect("reopen intact state above 4 MiB");
    let events = reopened
        .events_after(record.id(), 0, page_limit(MAX_EVENT_PAGE_LEN))
        .expect("read untruncated events");
    assert_eq!(events.len(), MAX_EVENT_BATCH_LEN * 3);
    assert!(
        events
            .iter()
            .all(|event| event.payload() == &maximum_batch[0]),
        "every maximum-size payload must survive without truncation"
    );
    drop(reopened);

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
    stamp_execution(&store, created.id(), 32);
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

#[test]
fn named_admission_is_the_restart_schedule() {
    let root = tempfile::tempdir().expect("root");
    let store = JobStore::open(root.path()).expect("store");
    let created = admitted_record(
        store
            .create_or_replay_named(
                key("request-named"),
                digest(33),
                usize::MAX,
                "root.nika.yaml".to_owned(),
            )
            .expect("create"),
    );
    assert_eq!(created.workflow(), "root.nika.yaml");
    let queued = store.queued_jobs().expect("queued");
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].0, created.id().clone());
    assert_eq!(queued[0].1, "root.nika.yaml");
    let anonymous = admitted_record(
        store
            .create_or_replay(key("request-anonymous"), digest(34))
            .expect("create"),
    );
    assert!(anonymous.workflow().is_empty());
    let queued = store.queued_jobs().expect("queued");
    assert_eq!(queued.len(), 1, "empty workflow is not rescheduled");
}
