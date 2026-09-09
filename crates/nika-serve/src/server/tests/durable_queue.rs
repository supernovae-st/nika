// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn post_admission_path_mutation_cannot_change_queued_world_across_restart() {
    let world = TestWorld::new();
    let hanging = Arc::new(GatedBackend::new());
    let first = world.start(hanging.clone(), bounded_queue_limits()).await;
    let body = r#"{"workflow":"root.nika.yaml"}"#;
    let mut ids = Vec::new();
    for index in 0..4 {
        let response = first
            .request(&post_request(body, &format!("qr-{index}"), &auth_header()))
            .await;
        assert_eq!(response.status, 202, "{}", response.body);
        ids.push(response.json()["id"].as_str().expect("id").to_owned());
    }
    for _ in 0..200 {
        if hanging.calls() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(hanging.calls(), 2);
    assert!(matches!(
        first.stop().await,
        Err(ServerError::ShutdownTimeout)
    ));
    std::fs::write(world.workflows.join("root.nika.yaml"), "not-a-workflow")
        .expect("mutate server-local bytes after snapshot admission");
    let replacement = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let second = world.start(replacement.clone(), limits()).await;
    wait_for_calls(replacement.as_ref(), 2)
        .await
        .expect("queued pair");
    let mut succeeded = 0;
    let mut interrupted = 0;
    for id in &ids {
        let payload = second
            .request(&get_request(&format!("/v1/jobs/{id}/status")))
            .await
            .json();
        match payload["status"].as_str() {
            Some("succeeded") => succeeded += 1,
            Some("interrupted") => interrupted += 1,
            Some(other) => assert_eq!(other, "succeeded"),
            None => assert!(!payload["status"].is_null()),
        }
    }
    assert_eq!((interrupted, succeeded), (2, 2));
    second.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn first_run_creates_a_missing_state_root() {
    let world = TestWorld::new();
    std::fs::remove_dir_all(&world.state).expect("remove");
    let server = world
        .start(
            Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded)),
            limits(),
        )
        .await;
    assert!(world.state.is_dir());
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn queue_full_does_not_leave_a_durable_row() {
    let world = TestWorld::new();
    let backend = Arc::new(GatedBackend::new());
    let server = world.start(backend.clone(), bounded_queue_limits()).await;
    let body = r#"{"workflow":"root.nika.yaml"}"#;
    for index in 0..4 {
        let response = server
            .request(&post_request(body, &format!("nl-{index}"), &auth_header()))
            .await;
        assert_eq!(response.status, 202, "{}", response.body);
    }
    let excess = server
        .request(&post_request(body, "nl-x", &auth_header()))
        .await;
    assert_eq!(excess.status, 503);
    assert!(excess.json().get("id").is_none());
    backend.release(4);
    let _stopped = server.stop().await;
}
