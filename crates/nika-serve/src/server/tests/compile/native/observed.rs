// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The two capabilities a remote compile shares with `nika compile`: the caller's observation
//! of the files its request states (`observed_world`, the document its own engine printed with
//! `nika compile --observe-only`) rides the round as the CLI's observer's does — to the seat,
//! the grounding law and the judge — and binds its kept round; and the operator's decision model
//! (`--decision-model`) judges every candidate in place of the author. A document the observer
//! never prints, or one about a path the request does not state, is refused before any call.

use super::*;

/// What the caller's engine observed of [`INTENT`]'s source: a header, never a row.
fn observation() -> Value {
    json!({"observed": [{"path": "./a.md", "state": "observed", "complete": false,
        "kind": "csv", "columns": ["region", "sales_total"], "delimiter": ","}]})
}

/// The text of every request the seat received.
fn sent(seat: &Seat) -> String {
    seat.bodies()
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn capabilities(health: &Value) -> Vec<String> {
    health["supportedCapabilities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

async fn health(server: &TestServer) -> Value {
    server
        .request("GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .json()
}

/// The observation reaches the seat's requests and the plan the round keeps; its replay must
/// repeat it, and one that differs (the caller's files changed) is another input.
#[tokio::test(flavor = "multi_thread")]
async fn an_admitted_observation_rides_the_round_and_binds_its_kept_round() {
    let world = TestWorld::new();
    let seat = Seat::start(question_round());
    let authoring = NativeAuthoring::new(SEAT, seat.providers()).with_max_calls(4);
    let (server, _) = start_native(&world, compile_limits(), authoring).await;
    assert!(capabilities(&health(&server).await).contains(&"compileObservedWorld".to_owned()));

    let observed = json!({"observed_world": observation()});
    let first = server.request(&compile_request(&fresh(&observed))).await;
    assert_eq!(first.status, 200, "{}", first.body);
    assert!(sent(&seat).contains("sales_total"), "{}", sent(&seat));
    let token = token_of(&first);
    // The kept round's zero-call replay repeats the observation; another one is another input.
    let again = server
        .request(&compile_request(&replay(&token, &observed)))
        .await;
    assert_eq!(again.status, 200, "{}", again.body);
    let changed = json!({"observed_world": {"observed": [{"path": "./a.md", "state": "absent"}]}});
    let moved = server
        .request(&compile_request(&replay(&token, &changed)))
        .await;
    assert_eq!(moved.status, 409, "{}", moved.body);
    assert!(
        moved.body.contains("compile_replay_input_changed"),
        "{}",
        moved.body
    );
    let calls = seat.calls();

    // Without it, nothing of the caller's files reaches the seat.
    let bare = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(bare.status, 200, "{}", bare.body);
    let unobserved = seat.bodies()[calls..]
        .iter()
        .map(Value::to_string)
        .collect::<String>();
    assert!(!unobserved.contains("sales_total"), "{unobserved}");
    server.stop().await.expect("clean stop");
}

/// Refused whole and before any call: a path the request does not state, a key the observer
/// never writes, a repeated key, an oversized document, and an observation beside a constant.
#[tokio::test(flavor = "multi_thread")]
async fn an_observation_beyond_the_observers_shape_is_refused_before_any_call() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(plan_answer(DRAFT, &[]))]);
    let authoring = NativeAuthoring::new(SEAT, seat.providers()).with_repairs(0);
    let (server, _) = start_native(&world, compile_limits(), authoring).await;
    let wide: Vec<String> = (0..20_000).map(|n| format!("column_{n}")).collect();
    for (world, code) in [
        (
            json!({"observed": [{"path": "./secrets.json", "state": "absent"}]}),
            "compile_observation_refused",
        ),
        (
            json!({"observed": [{"path": "./a.md", "state": "observed", "rows": [["N", 1]]}]}),
            "compile_observation_refused",
        ),
        (
            json!({"observed": [{"path": "./a.md", "state": "observed", "columns": wide}]}),
            "compile_observation_refused",
        ),
        (json!("./a.md"), "compile_observation_refused"),
    ] {
        let body = fresh(&json!({"observed_world": world}));
        let refused = server.request(&compile_request(&body)).await;
        assert_eq!(refused.status, 422, "{}", refused.body);
        assert!(refused.body.contains(code), "{}", refused.body);
    }
    let repeated = fresh(&json!({})).replacen(
        "\"intent\"",
        r#""observed_world":{"observed":[{"path":"./a.md","state":"absent","state":"observed"}]},"intent""#,
        1,
    );
    let refused = server.request(&compile_request(&repeated)).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert!(
        refused.body.contains("malformed_compile_request"),
        "{}",
        refused.body
    );
    let constant = json!({"compile_version": 2, "mode": "edit", "cognition": "explicitProvider",
        "source": "nika: x\ntasks: {}\n", "change": {"set_constant": {"name": "n", "value": 1}},
        "observed_world": observation()});
    let refused = server
        .request(&compile_request(&constant.to_string()))
        .await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert!(
        refused.body.contains("malformed_compile_request"),
        "{}",
        refused.body
    );
    assert_eq!(seat.calls(), 0, "nothing was sent");
    server.stop().await.expect("clean stop");
}

/// The operator's decision model judges the candidate, never the author: the judgment is a
/// closed choice sent under the decision model's name, recorded as the decision seat's, and the
/// health names the capability (never the model).
#[tokio::test(flavor = "multi_thread")]
async fn a_seated_decision_model_judges_in_place_of_the_author() {
    let world = TestWorld::new();
    // The decision model first qualifies the recalled references (an answer naming none leaves
    // them shown, unqualified), then the author plans, then the decision model judges.
    let seat = Seat::start(vec![
        Reply::Text("{}".to_owned()),
        Reply::Text(plan_answer(DRAFT, &[])),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]);
    let authoring = NativeAuthoring::new(SEAT, seat.providers())
        .with_repairs(0)
        .with_decision_model("vllm/s06-judge");
    let (server, _) = start_native(&world, compile_limits(), authoring).await;
    let advertised = health(&server).await;
    assert!(capabilities(&advertised).contains(&"compileDecisionSeat".to_owned()));
    assert!(
        !advertised.to_string().contains("s06-judge"),
        "{advertised}"
    );

    let response = server
        .request(&compile_request(&fresh(&super::authority::answered())))
        .await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    let bodies = seat.bodies();
    assert_eq!(bodies.len(), 3, "{document:#}");
    assert_eq!(
        bodies[0]["model"], "s06-judge",
        "the qualification: {}",
        bodies[0]
    );
    assert_eq!(bodies[1]["model"], "s06-seat");
    assert_eq!(bodies[2]["model"], "s06-judge", "{}", bodies[2]);
    let attempts = &document["provenance"]["decision"]["semantic_verification"];
    assert_eq!(
        attempts[0]["judge"],
        json!({"seat": "vllm/s06-judge", "kind": "decision_seat"}),
        "{document:#}"
    );
    assert_eq!(document["status"], "ready", "{document:#}");
    let backend = &document["provenance"]["authoring"]["backend"];
    assert_eq!(backend["decision_model"], "vllm/s06-judge", "{backend:#}");
    assert!(
        backend["authority"]["decision_seat"]
            .as_str()
            .is_some_and(|note| note.starts_with("outside this authority")),
        "{backend:#}"
    );
    server.stop().await.expect("clean stop");

    // Without one, the author judges itself and the health says nothing of a decision seat.
    let plain = Seat::start(vec![Reply::Text(plan_answer(DRAFT, &[]))]);
    let authoring = NativeAuthoring::new(SEAT, plain.providers()).with_repairs(0);
    let (server, _) = start_native(&world, compile_limits(), authoring).await;
    assert!(!capabilities(&health(&server).await).contains(&"compileDecisionSeat".to_owned()));
    server.stop().await.expect("clean stop");
}

/// A decision model that cannot be seated refuses the server before it binds: never the author
/// silently judging in its place.
#[tokio::test(flavor = "multi_thread")]
async fn an_unseatable_decision_model_refuses_the_server() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![]);
    for model in ["claude-code/default", "nobody"] {
        let authoring = NativeAuthoring::new(SEAT, seat.providers()).with_decision_model(model);
        let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
        let resident = ResidentConfig::new(&world.state).with_limits(compile_limits());
        let authority = ResidentAuthority::open(resident, backend)
            .await
            .expect("authority");
        let config = ServerConfig::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            &world.workflows,
            &world.token,
        )
        .with_native_authoring(authoring);
        let refused = BoundServer::attach(config, &authority).await;
        assert!(refused.is_err(), "{model}");
    }
}

/// What the caller's engine read of [`INTENT`]'s source, beside [`observation`]: its text.
fn trial_inputs() -> Value {
    json!({"files": [{"path": "./a.md", "text": "region,sales_total\nN,1\n"}]})
}

/// On a server that tries candidates, a round with `trial_inputs` tries its final candidate in
/// the observed room over a scratch project of exactly those files, and journals the trial in
/// the outcome; trial inputs no observation read, or sent alone, are refused before any call.
#[tokio::test(flavor = "multi_thread")]
async fn trial_inputs_are_tried_in_the_shared_room_and_refused_outside_their_law() {
    use nika_onboard::compile::room::JqHelper;
    let world = TestWorld::new();
    let seat = Seat::start(vec![
        Reply::Text(plan_answer(DRAFT, &[])),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]);
    let authoring = NativeAuthoring::new(SEAT, seat.providers())
        .with_repairs(0)
        .with_trials(JqHelper::new("/nonexistent/jq-helper"));
    let (server, _) = start_native(&world, compile_limits(), authoring).await;
    assert!(capabilities(&health(&server).await).contains(&"compileTrialInputs".to_owned()));
    let mut fields = super::authority::answered();
    fields["observed_world"] = observation();
    fields["trial_inputs"] = trial_inputs();
    let response = server.request(&compile_request(&fresh(&fields))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    let rehearsal = &document["provenance"]["decision"]["rehearsal"];
    assert_eq!(
        rehearsal["scope"], "this compile invocation",
        "{document:#}"
    );
    assert!(
        rehearsal["reports"]
            .as_array()
            .is_some_and(|r| !r.is_empty()),
        "{document:#}"
    );
    assert!(
        !document.to_string().contains("region,sales_total"),
        "the trial inputs are never echoed: {document:#}"
    );
    let calls = seat.calls();
    for (fields, code) in [
        (
            json!({"trial_inputs": trial_inputs()}),
            "malformed_compile_request",
        ),
        (
            json!({"observed_world": observation(),
                "trial_inputs": {"files": [{"path": "./b.md", "text": "x"}]}}),
            "compile_trial_inputs_refused",
        ),
    ] {
        let refused = server.request(&compile_request(&fresh(&fields))).await;
        assert_eq!(refused.status, 422, "{}", refused.body);
        assert!(refused.body.contains(code), "{}", refused.body);
    }
    assert_eq!(seat.calls(), calls, "nothing more was sent");
    server.stop().await.expect("clean stop");

    // A server that tries nothing refuses them, and lists no trial capability.
    let plain = Seat::start(vec![Reply::Text(plan_answer(DRAFT, &[]))]);
    let authoring = NativeAuthoring::new(SEAT, plain.providers()).with_repairs(0);
    let (server, _) = start_native(&world, compile_limits(), authoring).await;
    assert!(!capabilities(&health(&server).await).contains(&"compileTrialInputs".to_owned()));
    let fields = json!({"observed_world": observation(), "trial_inputs": trial_inputs()});
    let refused = server.request(&compile_request(&fresh(&fields))).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert!(
        refused.body.contains("compile_trial_inputs_refused"),
        "{}",
        refused.body
    );
    assert_eq!(plain.calls(), 0);
    server.stop().await.expect("clean stop");
}
