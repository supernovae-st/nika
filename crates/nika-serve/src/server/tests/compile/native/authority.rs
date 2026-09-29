// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Authoring grants bound actual requests, separately from repair preferences.

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn absent_authority_never_buys_a_repair_request() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![
        Reply::Text(native_answer("nika: broken\ntasks: {}\n")),
        Reply::Text(native_answer(&candidate(RUN_MODEL, false))),
    ]);
    let operator = NativeAuthoring::new(SEAT, seat.providers());
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(seat.calls(), 1, "default repairs grant no extra request");
    assert_ne!(response.json()["status"], "ready");
    let document = response.json();
    let receipt = &document["provenance"]["authoring"];
    assert_eq!(receipt["backend"]["usage_complete"], true);
    assert!(
        receipt["context"]
            .as_array()
            .expect("context")
            .iter()
            .any(|entry| { entry["result"]["failure_kind"] == "admission_refused" }),
        "{document:#}"
    );
    assert!(document.to_string().contains("max_calls"));
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn unidentified_responses_remain_visible_beside_named_responses() {
    let world = TestWorld::new();
    let mut named: Value =
        serde_json::from_str(&completion(&native_answer("nika: broken\ntasks: {}\n")))
            .expect("completion");
    named["model"] = json!("observed-first-response");
    let seat = Seat::start(vec![
        Reply::Status(200, named.to_string()),
        Reply::Text(native_answer(&candidate(RUN_MODEL, false))),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]);
    let operator = NativeAuthoring::new(SEAT, seat.providers())
        // 3 + repairs (nv1b): the candidate, its repair and the judge's two whole-request
        // questions.
        .with_max_calls(4)
        .with_repairs(1);
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    let document = response.json();
    assert_eq!(document["status"], "ready", "{document:#}");
    assert_eq!(
        seat.calls(),
        3,
        "the named answer, its repair, then the judgment"
    );
    let backend = &document["provenance"]["authoring"]["backend"];
    assert_eq!(
        backend["observed_models"],
        json!(["observed-first-response"])
    );
    // The repaired answer and the judge's (native step 1) name no model.
    assert_eq!(backend["unreported_models"], 2);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn absent_authority_never_hides_a_transport_retry() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![
        Reply::Busy,
        Reply::Text(native_answer(&candidate(RUN_MODEL, false))),
    ]);
    let operator = NativeAuthoring::new(SEAT, seat.providers()).with_repairs(0);
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(seat.calls(), 1, "a 503 never grants another wire request");
    assert_ne!(response.json()["status"], "ready");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_authority_repairs_and_a_caller_can_narrow_but_never_widen_it() {
    for (limits, expected_calls, ready) in [
        // The operator's grant: the broken answer, its repair, then the judgment (native step 1).
        (json!({}), 3, true),
        (json!({"max_calls": 1, "repairs": 0}), 1, false),
    ] {
        let world = TestWorld::new();
        let seat = Seat::start(vec![
            Reply::Text(native_answer("nika: broken\ntasks: {}\n")),
            Reply::Text(native_answer(&candidate(RUN_MODEL, false))),
            Reply::Text(JUDGE_APPROVES.to_owned()),
        ]);
        let operator = NativeAuthoring::new(SEAT, seat.providers())
            // 3 + repairs (nv1b): the candidate, its repair and the judge's two whole-request
            // questions.
            .with_max_calls(4)
            .with_repairs(1);
        let (server, _) = start_native(&world, compile_limits(), operator).await;
        for refused in [
            json!({"max_calls": 5}),
            json!({"max_calls": 0}),
            json!({"max_calls": 1}),
        ] {
            let response = server
                .request(&compile_request(&fresh(&json!({"limits": refused}))))
                .await;
            assert_eq!(response.status, 422, "{}", response.body);
            assert_eq!(seat.calls(), 0, "bad authority never contacts a provider");
            if refused == json!({"max_calls": 1}) {
                assert!(
                    response.body.contains("limits.repairs"),
                    "{}",
                    response.body
                );
                assert!(!response.body.contains("exceeds its bound"));
            }
        }
        let response = server
            .request(&compile_request(&fresh(&json!({"limits": limits}))))
            .await;
        assert_eq!(response.status, 200, "{}", response.body);
        let document = response.json();
        assert_eq!(document["status"] == "ready", ready, "{document:#}");
        assert_eq!(seat.calls(), expected_calls);
        let backend = &document["provenance"]["authoring"]["backend"];
        assert_eq!(backend["requested_model"], SEAT);
        assert_eq!(
            backend["authority"]["http_requests"]["sent"],
            expected_calls
        );
        assert_eq!(backend["authority"]["invocations"]["sent"], expected_calls);
        server.stop().await.expect("clean stop");
    }
}
