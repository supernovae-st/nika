// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_event::settlement::{RunCause, RunSettlement, RunState, Spend, TaskTally};
use tokio::sync::{Barrier, Semaphore};
use tokio::task::JoinSet;

use super::*;

fn cancellation_limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(30),
        Duration::from_secs(1),
        4,
        16,
        64,
        32,
    )
}

/// Acknowledges cancellation but holds the actual result behind a barrier.
struct HeldSettlementBackend {
    settlement: RunSettlement,
    cancelled: Semaphore,
    release: Semaphore,
}

impl HeldSettlementBackend {
    fn new(state: RunState, cause: RunCause) -> Self {
        let mut tasks = TaskTally::new();
        tasks.total = 3;
        tasks.ok = 1;
        tasks.cancelled = u32::from(state == RunState::Cancelled);
        Self {
            settlement: RunSettlement::new(state, cause)
                .with_elapsed_ms(73)
                .with_tasks(tasks)
                .with_spend(Spend::new(Some(0.0042), 1, 1)),
            cancelled: Semaphore::new(0),
            release: Semaphore::new(0),
        }
    }
}

impl ExecutionBackend for HeldSettlementBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async move {
            self.release.acquire().await.expect("release gate").forget();
            let disposition = match self.settlement.state {
                RunState::Succeeded => ExecutionDisposition::Succeeded,
                RunState::Cancelled => ExecutionDisposition::Cancelled,
                RunState::Paused => ExecutionDisposition::Paused,
                _ => ExecutionDisposition::Failed,
            };
            ExecutionOutcome::from(disposition)
                .with_settlement(self.settlement.clone())
                .with_outputs(BTreeMap::from([("answer".to_owned(), json!(42))]))
                .with_chain_head("runtime-chain-head")
        })
    }

    fn execute_with_cancel<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        _max_cost_usd: Option<f64>,
        cancel: nika_types::cancel::CancelCtx,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async move {
            super::super::cancel::cancelled(cancel).await;
            self.cancelled.add_permits(1);
            self.execute(context).await
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_cancel_preserves_the_returned_runtime_result() {
    for (state, cause) in [
        (RunState::Succeeded, RunCause::Normal),
        (RunState::Cancelled, RunCause::Operator),
        (RunState::Failed, RunCause::TaskFailed),
    ] {
        tokio::time::timeout(Duration::from_secs(10), cancel_held_result(state, cause))
            .await
            .expect("cancel race completes");
    }
}

async fn cancel_held_result(state: RunState, cause: RunCause) {
    let world = TestWorld::new();
    let backend = Arc::new(HeldSettlementBackend::new(state, cause));
    let expected = serde_json::to_value(&backend.settlement).expect("settlement");
    let server = world.start(backend.clone(), cancellation_limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "cancel-race",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "running")
        .await
        .expect("running");

    let barrier = Arc::new(Barrier::new(13));
    let mut racers = JoinSet::new();
    for _ in 0..12 {
        let request = cancel_request(&id);
        let address = server.address;
        let barrier = Arc::clone(&barrier);
        racers.spawn(async move {
            barrier.wait().await;
            wire_request(address, &request).await
        });
    }
    barrier.wait().await;
    while let Some(response) = racers.join_next().await {
        let response = response.expect("cancel request");
        assert_eq!(response.status, 202, "{response:#?}");
        let body = response.json();
        assert_eq!(body["id"], id);
        assert_eq!(body["status"], "running", "action is not settlement");
        assert!(body.get("settlement").is_none());
        assert!(body.get("receipt").is_none());
    }
    backend
        .cancelled
        .acquire()
        .await
        .expect("cancel observed")
        .forget();
    backend.release.add_permits(1);
    wait_for_status(&server, &id, state.as_str())
        .await
        .expect("runtime settles");

    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert_eq!(job["status"], state.as_str());
    assert_eq!(job["settlement"], expected);
    assert_eq!(job["outputs"]["answer"], 42);
    assert_eq!(job["receipt"]["chain_head"], "runtime-chain-head");
    assert_eq!(job["receipt"]["execution_id"], job["execution_id"]);
    assert_eq!(job["receipt"]["trace_id"], job["trace_id"]);
    let replay = server.request(&cancel_request(&id)).await;
    assert_eq!(replay.status, 200);
    assert_eq!(replay.json(), job);

    let streamed = server.request(&events_request(&id, None)).await;
    let events = parse_sse_data(&streamed.body);
    let settled = events
        .iter()
        .filter(|event| event.get("settlement").is_some())
        .collect::<Vec<_>>();
    assert_eq!(settled.len(), 1, "one execution owner settles: {events:?}");
    assert_eq!(settled[0]["settlement"], expected);
    assert_eq!(settled[0]["outputs"], job["outputs"]);
    assert_eq!(settled[0]["receipt"], job["receipt"]);
    assert_eq!(
        settled[0]["kind"],
        if state == RunState::Cancelled {
            "execution.cancelled"
        } else {
            "execution.settled"
        }
    );
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn cancellation_grace_without_a_runtime_result_is_interrupted() {
    let world = TestWorld::new();
    // The execution deadline exceeds cancellation grace, distinguishing the
    // cancellation timeout from the general run timeout.
    let server = world
        .start(Arc::new(TestBackend::hangs()), cancellation_limits())
        .await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "cancel-grace",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "running")
        .await
        .expect("running");
    let response = server.request(&cancel_request(&id)).await;
    assert_eq!(response.status, 202);
    assert_eq!(response.json()["status"], "running");
    let streamed = tokio::time::timeout(
        super::super::CANCEL_GRACE + Duration::from_secs(3),
        server.request(&events_request(&id, None)),
    )
    .await
    .expect("grace closes the observation");
    let events = parse_sse_data(&streamed.body);
    assert!(
        events
            .iter()
            .any(|event| event["kind"] == "execution.interrupted")
    );
    assert!(!events.iter().any(|event| event["status"] == "cancelled"));
    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert_eq!(job["status"], "interrupted");
    assert!(
        job.get("settlement").is_none(),
        "no invented runtime result"
    );
    assert!(job.get("outputs").is_none());
    assert_eq!(job["receipt"]["execution_id"], job["execution_id"]);
    assert!(
        job["receipt"].get("chain_head").is_none(),
        "no runtime trace closure"
    );
    server.stop().await.expect("clean stop");
}
