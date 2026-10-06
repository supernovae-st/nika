// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Authoring grants bound actual requests, separately from repair preferences. A creation is
//! authored under the shared default strategy (`escalate`): the private plan, then the sketch
//! door; the compiler writes the source and its permits, and its READY waits for a judgment the
//! grant must leave room for.

use super::*;

/// The run model the compiler asks for, answered in the request itself.
fn answered() -> Value {
    json!({"answers": {"model": RUN_MODEL}})
}

/// The account a round's receipt keeps: the grant, what was configured, what was sent.
fn authority(document: &Value) -> Value {
    document["provenance"]["authoring"]["backend"]["authority"].clone()
}

/// The roles of the calls a round journaled, with how each ended (`null` when it answered).
fn roles(document: &Value) -> Vec<(String, Value)> {
    document["provenance"]["authoring"]["context"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|call| {
            let role = call["call"].as_str().unwrap_or_default().to_owned();
            (role, call["result"]["failure_kind"].clone())
        })
        .collect()
}

/// The permits the compiler derives for [`INTENT`]'s stated read and write, as it emits them.
const STATED_PERMITS: &str = "permits:\n  fs:\n    read:\n    - ./a.md\n    write:\n    - ./b.md\n";

/// The lines of one task in an emitted document: its key line and everything nested under it.
fn task_block<'a>(candidate: &'a str, task: &str) -> Vec<&'a str> {
    let key = format!("  {task}:");
    let mut lines = candidate.lines().skip_while(|line| *line != key);
    let head = lines.next().into_iter();
    head.chain(lines.take_while(|line| line.starts_with("    ")))
        .collect()
}

/// A plan whose read cites words the request never wrote: the core asks one evidence repair.
fn loose_plan() -> String {
    let mut plan: Value = serde_json::from_str(&plan_answer(DRAFT, &[])).expect("plan");
    plan["steps"][0]["evidence"] = json!("Read the file ./a.md");
    plan.to_string()
}

/// The judge's closed choices, then the plan the repair call answers.
pub(super) fn judged_repair() -> Vec<Reply> {
    vec![
        Reply::Text(plan_answer(DRAFT, &[])),
        Reply::Text(json!({"choice": "unfaithful"}).to_string()),
        Reply::Text(json!({"choice": "part-1"}).to_string()),
        Reply::Text(plan_answer(DRAFT, &[])),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]
}

/// One explicit repair and a request ceiling; actual dispatches enforce that ceiling.
pub(super) fn one_repair(seat: &Seat) -> NativeAuthoring {
    NativeAuthoring::new(SEAT, seat.providers())
        .with_max_calls(32)
        .with_repairs(1)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_one_request_limit_never_buys_a_repair_request() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![
        Reply::Text(loose_plan()),
        Reply::Text(plan_answer(DRAFT, &[])),
    ]);
    let operator = NativeAuthoring::new(SEAT, seat.providers()).with_max_calls(1);
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        seat.calls(),
        1,
        "the explicit request limit allows no extra request"
    );
    assert_ne!(response.json()["status"], "ready");
    let document = response.json();
    let receipt = &document["provenance"]["authoring"];
    assert_eq!(receipt["backend"]["usage_complete"], true);
    assert_eq!(
        roles(&document),
        [
            ("plan".to_owned(), Value::Null),
            ("repair".to_owned(), json!("admission_refused")),
        ],
        "the evidence repair is refused before a byte leaves: {document:#}"
    );
    assert!(document.to_string().contains("max_calls"));
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn unidentified_responses_remain_visible_beside_named_responses() {
    let world = TestWorld::new();
    let mut named: Value =
        serde_json::from_str(&completion(&plan_answer(DRAFT, &[]))).expect("completion");
    named["model"] = json!("observed-first-response");
    let seat = Seat::start(vec![
        Reply::Status(200, named.to_string()),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]);
    // The plan and its judgment; repairs stay the default preference.
    let operator = NativeAuthoring::new(SEAT, seat.providers()).with_max_calls(4);
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&answered()))).await;
    let document = response.json();
    assert_eq!(document["status"], "ready", "{document:#}");
    assert_eq!(seat.calls(), 2, "the named plan, then its judgment");
    let backend = &document["provenance"]["authoring"]["backend"];
    assert_eq!(
        backend["observed_models"],
        json!(["observed-first-response"])
    );
    // The judge's answer (native step 1) names no model.
    assert_eq!(backend["unreported_models"], 1);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn continuous_preparation_never_hides_a_transport_retry() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Busy, Reply::Text(plan_answer(DRAFT, &[]))]);
    let operator = NativeAuthoring::new(SEAT, seat.providers()).with_repairs(0);
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    let bodies = seat.bodies();
    assert_eq!(bodies.len(), 2, "the transient retry reached the seat");
    assert_eq!(bodies[0], bodies[1], "the retry sends the same request");
    assert_eq!(document["provenance"]["authoring"]["calls"], 1);
    assert_eq!(roles(&document), [("plan".to_owned(), Value::Null)]);
    let account = authority(&document);
    assert_eq!(account["max_calls"], Value::Null);
    assert_eq!(account["invocations"], json!({"sent": 1, "refused": 0}));
    assert_eq!(
        account["http_requests"],
        json!({"sent": 2, "refused": 0, "unknown": null}),
        "a physical retry remains visible beside its logical invocation"
    );
    assert_eq!(document["status"], "incomplete", "{document:#}");
    assert!(document["candidate"].is_null(), "{document:#}");
    assert_eq!(document["questions"][0]["key"], "model");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_one_request_limit_refuses_a_transport_retry() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Busy, Reply::Text(plan_answer(DRAFT, &[]))]);
    let operator = NativeAuthoring::new(SEAT, seat.providers())
        .with_repairs(0)
        .with_max_calls(1);
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(
        seat.calls(),
        1,
        "no retry exceeds the explicit request limit"
    );
    let account = authority(&document);
    assert_eq!(account["max_calls"], 1);
    assert_eq!(account["invocations"], json!({"sent": 1, "refused": 0}));
    assert_eq!(
        account["http_requests"],
        json!({"sent": 1, "refused": 1, "unknown": null})
    );
    assert_ne!(document["status"], "ready", "{document:#}");
    assert!(document["candidate"].is_null(), "{document:#}");
    assert!(document.to_string().contains("max_calls"), "{document:#}");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_authority_repairs_and_a_caller_can_narrow_but_never_widen_it() {
    for (limits, expected_calls, ready) in [
        // The operator's grant: the plan, its judgment and the locate question, the repair, the
        // repaired plan's judgment — one real repair.
        (json!({}), 5, true),
        (json!({"max_calls": 1, "repairs": 0}), 1, false),
    ] {
        let world = TestWorld::new();
        let seat = Seat::start(judged_repair());
        let (server, _) = start_native(&world, compile_limits(), one_repair(&seat)).await;
        for refused in [
            json!({"max_calls": 33}),
            json!({"max_calls": 0}),
            json!({"repairs": 2}),
        ] {
            let response = server
                .request(&compile_request(&fresh(&json!({"limits": refused}))))
                .await;
            assert_eq!(response.status, 422, "{}", response.body);
            assert_eq!(seat.calls(), 0, "bad authority never contacts a provider");
        }
        let mut fields = answered();
        fields["limits"] = limits;
        let response = server.request(&compile_request(&fresh(&fields))).await;
        assert_eq!(response.status, 200, "{}", response.body);
        let document = response.json();
        assert_eq!(document["status"] == "ready", ready, "{document:#}");
        assert_eq!(seat.calls(), expected_calls);
        let backend = &document["provenance"]["authoring"]["backend"];
        assert_eq!(backend["requested_model"], SEAT);
        let account = &backend["authority"];
        assert_eq!(account["http_requests"]["sent"], expected_calls);
        assert_eq!(account["invocations"]["sent"], expected_calls);
        // What the configuration could ask for is not what was granted, nor what was sent.
        assert_eq!(account["configured"]["worst_case"], Value::Null);
        let granted = if ready { 32 } else { 1 };
        assert!(
            account["max_calls"] == granted,
            "authority max_calls must match the granted bound"
        );
        server.stop().await.expect("clean stop");
    }
}

/// Default preparation continues through judgment; it neither saves nor runs the result.
#[tokio::test(flavor = "multi_thread")]
async fn a_default_round_reaches_its_judgment_without_a_request_grant() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![
        Reply::Text(plan_answer(DRAFT, &[])),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]);
    let operator = NativeAuthoring::new(SEAT, seat.providers());
    let (server, backend) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&answered()))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(document["status"], "ready", "{document:#}");
    assert_eq!(seat.calls(), 2);
    let candidate = document["candidate"].as_str().expect("candidate");
    assert!(candidate.contains(STATED_PERMITS), "{candidate}");
    assert_eq!(
        roles(&document),
        [
            ("plan".to_owned(), Value::Null),
            ("judge_request".to_owned(), Value::Null),
        ]
    );
    let account = authority(&document);
    assert_eq!(account["max_calls"], Value::Null);
    assert_eq!(account["configured"]["repairs"], Value::Null);
    assert_eq!(account["configured"]["worst_case"], Value::Null);
    assert_eq!(account["http_requests"]["sent"], 2);
    assert_eq!(account["invocations"], json!({"sent": 2, "refused": 0}));
    assert_eq!(backend.calls(), 0, "preparation never runs the candidate");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_default_round_repairs_a_judged_defect_without_an_implicit_count() {
    let world = TestWorld::new();
    let seat = Seat::start(judged_repair());
    let (server, _) = start_native(
        &world,
        compile_limits(),
        NativeAuthoring::new(SEAT, seat.providers()),
    )
    .await;
    let response = server.request(&compile_request(&fresh(&answered()))).await;
    let document = response.json();
    assert_eq!(document["status"], "ready", "{document:#}");
    assert_eq!(seat.calls(), 5);
    assert_eq!(authority(&document)["max_calls"], Value::Null);
    assert_eq!(authority(&document)["configured"]["repairs"], Value::Null);
    server.stop().await.expect("clean stop");
}

/// A plan whose candidate needs a value the request never names asks it in one request: no
/// judgment is due, no second request is sent, and the round is not kept.
#[tokio::test(flavor = "multi_thread")]
async fn a_plan_question_round_sends_one_request_and_keeps_no_token() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(plan_answer(DRAFT, &[]))]);
    let operator = NativeAuthoring::new(SEAT, seat.providers());
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(document["status"], "incomplete", "{document:#}");
    assert!(document["candidate"].is_null(), "{document:#}");
    let keys: Vec<&Value> = (document["questions"].as_array().expect("questions").iter())
        .map(|question| &question["key"])
        .collect();
    assert_eq!(keys, [&json!("model")]);
    assert_eq!(seat.calls(), 1);
    assert_eq!(roles(&document), [("plan".to_owned(), Value::Null)]);
    assert_eq!(document["provenance"]["strategy"], "cold");
    // Its answer round is authored again: the Cold replay is a missing Serve raccord.
    assert!(response.header("nika-compile-replay").is_none());
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_caller_can_select_large_limits_where_the_operator_left_preparation_open() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(plan_answer(DRAFT, &[]))]);
    let operator = NativeAuthoring::new(SEAT, seat.providers());
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let limits = json!({"limits": {
        "max_calls": 1000, "repairs": 100, "deadline_ms": 86_400_000,
    }});
    let response = server.request(&compile_request(&fresh(&limits))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(authority(&document)["max_calls"], 1000);
    assert_eq!(authority(&document)["configured"]["repairs"], 100);
    assert_eq!(
        authority(&document)["configured"]["worst_case"],
        Value::Null
    );
    assert_eq!(
        seat.calls(),
        1,
        "the question still pauses for its missing value"
    );
    server.stop().await.expect("clean stop");
}

/// A request that gates its write on the human: the plan cannot carry the gate, so the sketch
/// door takes it.
const GATED: &str =
    "Read ./a.md and do something clever with it, then ask me before you write ./b.md";

fn gated_round() -> Vec<Reply> {
    let mut plan: Value = serde_json::from_str(&plan_answer(DRAFT, &[])).expect("plan");
    plan["effects"][0]["policy"] = json!("human_first");
    plan["effects"][0]["evidence"] = json!("ask me before you write ./b.md");
    let sketch = json!({"name": "clever-rewrite", "tasks": [
        {"id": "read_source", "verb": "invoke", "tool": "nika:read", "purpose": "read",
         "reads": ["./a.md"]},
        {"id": "transform", "verb": "infer", "purpose": "rewrite cleverly",
         "with": [{"name": "text", "from": "read_source"}]},
        {"id": "approve", "verb": "invoke", "tool": "nika:prompt", "purpose": "ask first"},
        {"id": "write_result", "verb": "invoke", "tool": "nika:write", "purpose": "write",
         "writes": ["./b.md"], "with": [{"name": "text", "from": "transform"}],
         "gated_by": "approve"},
    ], "questions": [], "gaps": [], "notes": "read, transform, ask, write"});
    let fills = json!({"fills": [
        {"task": "transform", "field": "prompt",
         "value": "Rewrite this text in a clever way, inventing nothing: ${{ with.text }}"},
        {"task": "approve", "field": "args.message", "value": "Write the rewrite to ./b.md?"},
    ], "notes": "two holes"});
    vec![
        Reply::Text(plan.to_string()),
        Reply::Text(sketch.to_string()),
        Reply::Text(fills.to_string()),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]
}

/// An explicit grant of four: the plan, the sketch, its fills and the judgment — READY, the
/// compiler's document gating the write on the human. An explicit limit of one sends the plan
/// alone: the sketch is refused before it leaves, and nothing is READY.
#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_grant_reaches_a_judged_sketch_and_a_limit_of_one_stops_the_second() {
    let request = || {
        let mut fields = answered();
        fields["intent"] = json!(GATED);
        fresh(&fields)
    };
    let world = TestWorld::new();
    let seat = Seat::start(gated_round());
    let granted = NativeAuthoring::new(SEAT, seat.providers()).with_max_calls(4);
    let (server, _) = start_native(&world, compile_limits(), granted).await;
    let response = server.request(&compile_request(&request())).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(document["status"], "ready", "{document:#}");
    let journaled: Vec<String> = roles(&document).into_iter().map(|(role, _)| role).collect();
    assert_eq!(journaled, ["plan", "sketch", "fill", "judge_request"]);
    assert_eq!(seat.calls(), 4);
    let account = authority(&document);
    assert_eq!(account["http_requests"]["sent"], 4);
    assert_eq!(account["invocations"]["sent"], 4);
    assert_eq!(account["max_calls"], 4);
    // The compiler's document: the stated paths, their permits and the human gate.
    let emitted = document["candidate"].as_str().expect("candidate");
    assert!(emitted.contains(STATED_PERMITS), "{emitted}");
    assert!(
        task_block(emitted, "approve").contains(&"      tool: nika:prompt"),
        "{emitted}"
    );
    let write = task_block(emitted, "write_result");
    for gate in [
        "    when: ${{ with.approved == true }}",
        "      approved: ${{ tasks.approve.output }}",
    ] {
        assert!(
            write.contains(&gate),
            "the write waits for the human: {emitted}"
        );
    }
    for body in seat.bodies() {
        let schema = body["response_format"].to_string();
        assert!(
            !schema.contains("candidate_lines") && !schema.contains("\"candidate\""),
            "no source is ever asked of the seat: {schema}"
        );
    }
    assert!(response.header("nika-compile-replay").is_some(), "kept");
    server.stop().await.expect("clean stop");

    let world = TestWorld::new();
    let seat = Seat::start(gated_round());
    let limited = NativeAuthoring::new(SEAT, seat.providers()).with_max_calls(1);
    let (server, _) = start_native(&world, compile_limits(), limited).await;
    let response = server.request(&compile_request(&request())).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_ne!(document["status"], "ready", "{document:#}");
    assert!(document["candidate"].is_null(), "{document:#}");
    assert_eq!(seat.calls(), 1, "no hidden second request");
    assert_eq!(
        roles(&document),
        [
            ("plan".to_owned(), Value::Null),
            ("sketch".to_owned(), json!("admission_refused")),
        ]
    );
    let account = authority(&document);
    assert_eq!(account["http_requests"]["sent"], 1);
    assert_eq!(account["invocations"], json!({"sent": 1, "refused": 1}));
    server.stop().await.expect("clean stop");
}

/// Requested work and actual dispatch counts stay distinct; COLD has no finite estimate.
type Operator = fn(&Seat) -> NativeAuthoring;

#[tokio::test(flavor = "multi_thread")]
async fn the_policy_and_its_account_resolve_one_strategy() {
    let cases: [(Operator, Option<u32>, Option<u32>); 3] = [
        (
            |seat| NativeAuthoring::new(SEAT, seat.providers()),
            None,
            None,
        ),
        (
            |seat| NativeAuthoring::new(SEAT, seat.providers()).with_repairs(0),
            Some(0),
            None,
        ),
        (one_repair, Some(1), Some(32)),
    ];
    for (operator, repairs, max_calls) in cases {
        let world = TestWorld::new();
        let seat = Seat::start(vec![Reply::Text(plan_answer(DRAFT, &[]))]);
        let (server, _) = start_native(&world, compile_limits(), operator(&seat)).await;
        let response = server.request(&compile_request(&fresh(&json!({})))).await;
        assert_eq!(response.status, 200, "{}", response.body);
        let account = authority(&response.json());
        assert_eq!(account["configured"]["strategy"], "escalate");
        assert_eq!(account["configured"]["repairs"], json!(repairs));
        assert_eq!(account["configured"]["worst_case"], Value::Null);
        assert_eq!(account["max_calls"], json!(max_calls));
        assert_eq!(account["http_requests"]["sent"], 1);
        assert_eq!(seat.calls(), 1);
        server.stop().await.expect("clean stop");
    }
}
