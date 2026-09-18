// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A mirror can fail while the execution succeeds. Its loss must survive
//! settlement, SSE replay and reopening the durable job store.

use super::*;

struct RefusedSeal;

impl JournalSeal for RefusedSeal {
    fn seal(
        &self,
        trace: &mut TraceFileSink,
        _workflow_hash: Option<&str>,
        _teardown: Option<&SealTeardown>,
    ) -> bool {
        // The runtime has already completed. Exercise a real writer refusal
        // at finalization without touching key custody or a machine secret.
        let record =
            json!({"kind": "run_sealed", "payload": "x".repeat(nika_dap::chain::MAX_LINE_BYTES)});
        assert!(trace.write_record(&record).is_err());
        false
    }
}

async fn assert_mirror_loss(open_failure: bool, reason: &str) {
    let world = TestWorld::new();
    if open_failure {
        std::fs::create_dir_all(world.workflows.join(".nika")).expect("metadata dir");
        std::fs::write(journal_dir(&world), "not a directory").expect("blocked journal dir");
    }
    let backend = Arc::new(
        ResidentExecutionBackend::new(&world.workflows).with_journal_seal(Arc::new(RefusedSeal)),
    );
    let server = world.start(backend, long_execution_limits()).await;
    let id = run_by_name(&server, "root.nika", "mirror-loss").await;
    wait_for_settled(&server, &id, "succeeded")
        .await
        .expect("execution succeeded");
    let response = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await;
    let job = response.json();
    let evidence = json!({"status": "mirror_lost", "reason": reason});
    assert_eq!(job["status"], "succeeded");
    assert_eq!(job["settlement"]["tasks"]["ok"], 1);
    assert_eq!(job["evidence"], evidence, "{job}");
    assert!(job["receipt"].get("chain_head").is_none());
    assert!(
        !response
            .body
            .contains(world.root.path().to_string_lossy().as_ref())
    );
    let stream = server.request(&events_request(&id, None)).await;
    let events = parse_sse_data(&stream.body);
    assert_eq!(events.last().expect("terminal")["evidence"], evidence);
    server.stop().await.expect("clean stop");
    let jobs = crate::JobStore::open(&world.state).expect("reopen");
    let record = jobs
        .get(&crate::JobId::parse(&id).expect("job id"))
        .expect("read")
        .expect("job");
    assert_eq!(record.status(), crate::JobStatus::Succeeded);
    assert_eq!(
        serde_json::to_value(record.evidence()).expect("evidence"),
        evidence
    );
    assert_eq!(
        crate::inspect_resident(&world.state)
            .expect("resident")
            .mirror_losses,
        Some(1)
    );
    let raw = std::fs::read_to_string(world.state.join("jobs/state.json")).expect("durable state");
    let stored: Value = serde_json::from_str(&raw).expect("state");
    let terminal = stored["jobs"][0]["events"]
        .as_array()
        .expect("events")
        .last()
        .expect("terminal");
    assert_eq!(terminal["payload"]["evidence"], evidence);
}

#[tokio::test(flavor = "multi_thread")]
async fn mirror_open_failure_is_durable_without_changing_execution_success() {
    assert_mirror_loss(true, "write_failed").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn mirror_refusal_after_execution_is_durable_without_claiming_a_chain_head() {
    assert_mirror_loss(false, "record_refused").await;
}
