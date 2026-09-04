// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_event::settlement::{RunCause, RunSettlement, RunState, Spend};

use super::*;

struct PausingBackend;

impl ExecutionBackend for PausingBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async {
            ExecutionOutcome::from(ExecutionDisposition::Paused)
                .with_settlement(pause_settlement())
                .with_outputs(BTreeMap::from([("draft".to_owned(), json!("ready"))]))
                .with_chain_head("pause-chain-head")
        })
    }
}

fn pause_settlement() -> RunSettlement {
    RunSettlement::new(RunState::Paused, RunCause::HumanGate)
        .with_elapsed_ms(93)
        .with_spend(Spend::new(Some(0.03), 1, 0))
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_closes_observation_with_durable_result_and_evidence() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(PausingBackend), limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "pause-result",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "paused")
        .await
        .expect("paused");
    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert_eq!(job["status"], "paused");
    assert_eq!(
        job["settlement"],
        serde_json::to_value(pause_settlement()).expect("settlement")
    );
    assert_eq!(job["outputs"]["draft"], "ready");
    assert_eq!(job["receipt"]["chain_head"], "pause-chain-head");
    assert_eq!(job["receipt"]["execution_id"], job["execution_id"]);
    assert_eq!(job["receipt"]["trace_id"], job["trace_id"]);

    let streamed = tokio::time::timeout(
        Duration::from_secs(2),
        server.request(&events_request(&id, None)),
    )
    .await
    .expect("paused stream closes without heartbeating forever");
    let events = parse_sse_data(&streamed.body);
    let pause = events.last().expect("pause boundary");
    assert_eq!(pause["status"], "paused");
    assert_eq!(pause["settlement"], job["settlement"]);
    assert_eq!(pause["outputs"], job["outputs"]);
    assert_eq!(pause["receipt"], job["receipt"]);
    let cursor = pause["sequence"].as_u64().expect("cursor").to_string();
    let resumed = tokio::time::timeout(
        Duration::from_secs(2),
        server.request(&events_request(&id, Some(&cursor))),
    )
    .await
    .expect("replay at paused cursor also closes");
    assert!(parse_sse_data(&resumed.body).is_empty());
    let cancel = server.request(&cancel_request(&id)).await;
    assert_eq!(cancel.status, 200);
    assert_eq!(
        cancel.json(),
        job,
        "a completed pause is not an active cancellation"
    );
    server.stop().await.expect("stop before restart");

    let server = world.start(Arc::new(PausingBackend), limits()).await;
    let restored = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert_eq!(restored, job, "paused evidence survives resident restart");
    server.stop().await.expect("clean stop");
}
