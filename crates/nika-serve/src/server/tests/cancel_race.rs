// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinSet;

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_cancel_is_idempotent_and_wins_the_running_settlement_race() {
    for round in 0..32 {
        let outcome =
            tokio::time::timeout(Duration::from_secs(10), run_concurrent_cancel_round(round)).await;
        assert!(outcome.is_ok(), "concurrent cancel round {round} timed out");
    }
}

async fn run_concurrent_cancel_round(round: usize) {
    let world = TestWorld::new();
    let backend = Arc::new(GatedBackend::new());
    let server = world.start(backend.clone(), limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            &format!("cancel-race-{round}"),
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "running")
        .await
        .expect("running");

    let request = cancel_request(&id);
    let barrier = Arc::new(tokio::sync::Barrier::new(13));
    let mut racers = JoinSet::new();
    for _ in 0..12 {
        let request = request.clone();
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
        assert!(matches!(response.status, 200 | 202), "{response:#?}");
        assert_eq!(response.json()["id"], id);
    }
    backend.release(1);
    wait_for_status(&server, &id, "cancelled")
        .await
        .expect("cancelled");

    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert_eq!(job["status"], "cancelled");
    assert_eq!(job["receipt"]["job_id"], id);
    assert_eq!(job["receipt"]["execution_id"], job["execution_id"]);
    assert_eq!(job["receipt"]["trace_id"], job["trace_id"]);
    let replay = server.request(&cancel_request(&id)).await;
    assert_eq!(replay.status, 200, "{}", replay.body);
    assert_eq!(replay.json(), job);

    let streamed = server.request(&events_request(&id, None)).await;
    let events = parse_sse_data(&streamed.body);
    let terminal = events
        .iter()
        .find(|event| event["status"] == "cancelled")
        .expect("cancelled terminal event");
    assert_eq!(terminal["receipt"], job["receipt"]);
    server.stop().await.expect("clean stop");
}
