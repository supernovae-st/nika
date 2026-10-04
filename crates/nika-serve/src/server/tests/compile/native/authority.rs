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

/// An explicit repair preference of one: the shared law needs 32 requests for it (the plan's
/// verification and its repair, then the sketch door), granted here in full and stated.
pub(super) fn one_repair(seat: &Seat) -> NativeAuthoring {
    NativeAuthoring::new(SEAT, seat.providers())
        .with_max_calls(32)
        .with_repairs(1)
}

#[tokio::test(flavor = "multi_thread")]
async fn absent_authority_never_buys_a_repair_request() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![
        Reply::Text(loose_plan()),
        Reply::Text(plan_answer(DRAFT, &[])),
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
async fn absent_authority_never_hides_a_transport_retry() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Busy, Reply::Text(plan_answer(DRAFT, &[]))]);
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
    for (limits, expected_calls, ready, configured) in [
        // The operator's grant: the plan, its judgment and the locate question, the repair, the
        // repaired plan's judgment — one real repair.
        (json!({}), 5, true, 32),
        (json!({"max_calls": 1, "repairs": 0}), 1, false, 14),
    ] {
        let world = TestWorld::new();
        let seat = Seat::start(judged_repair());
        let (server, _) = start_native(&world, compile_limits(), one_repair(&seat)).await;
        for refused in [
            json!({"max_calls": 33}),
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
        assert_eq!(account["configured"]["worst_case"], configured);
        let granted = if ready { 32 } else { 1 };
        assert!(
            account["max_calls"] == granted,
            "authority max_calls must match the granted bound"
        );
        server.stop().await.expect("clean stop");
    }
}

/// The default grant (one request) authors the private plan: the compiler assembles the stated
/// read and write and derives their permits, and the candidate stays a preview — its judgment
/// is the next request, refused before a byte leaves, never READY and never kept.
#[tokio::test(flavor = "multi_thread")]
async fn a_default_round_sends_one_plan_and_keeps_its_candidate_a_preview() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![
        Reply::Text(plan_answer(DRAFT, &[])),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]);
    let operator = NativeAuthoring::new(SEAT, seat.providers());
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&answered()))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(document["compile_version"], 2);
    assert_eq!(document["status"], "incomplete", "{document:#}");
    assert_eq!(seat.calls(), 1, "one physical request, nothing more");
    assert_eq!(document["provenance"]["strategy"], "cold");
    let preview = document["candidate"].as_str().expect("the preview");
    assert!(preview.contains(STATED_PERMITS), "{preview}");
    assert!(
        preview.contains(&format!("\nmodel: {RUN_MODEL}\n")),
        "{preview}"
    );
    assert_eq!(
        roles(&document),
        [
            ("plan".to_owned(), Value::Null),
            ("judge_request".to_owned(), json!("admission_refused")),
        ]
    );
    let account = authority(&document);
    assert_eq!(account["source"], "default: one request");
    assert_eq!(account["max_calls"], 1);
    assert_eq!(account["http_requests"]["sent"], 1);
    assert_eq!(account["invocations"], json!({"sent": 1, "refused": 1}));
    assert!(
        document["diagnostics"]
            .to_string()
            .contains("nothing is READY"),
        "{document:#}"
    );
    // A private plan's round is never kept: a native round is (the Cold replay is not served).
    assert!(response.header("nika-compile-replay").is_none());
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
/// compiler's document gating the write on the human. The same request under the default grant
/// sends the plan alone: the sketch is refused before it leaves, and nothing is READY.
#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_grant_reaches_a_judged_sketch_and_the_default_one_never_sends_a_second() {
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
    let default = NativeAuthoring::new(SEAT, seat.providers());
    let (server, _) = start_native(&world, compile_limits(), default).await;
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

/// The seat's policy and its account resolve one strategy, `escalate`: the configured worst
/// case follows the repair preference (the default preference of three asks 66, an explicit
/// zero 14, an explicit one 32) and is never a grant — the grant stays one request unless the
/// operator names more, and the sends are counted apart.
/// How a case seats its operator over the controlled seat.
type Operator = fn(&Seat) -> NativeAuthoring;

#[tokio::test(flavor = "multi_thread")]
async fn the_policy_and_its_account_resolve_one_strategy() {
    let cases: [(Operator, u64, u64); 3] = [
        (|seat| NativeAuthoring::new(SEAT, seat.providers()), 66, 1),
        (
            |seat| NativeAuthoring::new(SEAT, seat.providers()).with_repairs(0),
            14,
            1,
        ),
        (one_repair, 32, 32),
    ];
    for (operator, worst_case, max_calls) in cases {
        let world = TestWorld::new();
        let seat = Seat::start(vec![Reply::Text(plan_answer(DRAFT, &[]))]);
        let (server, _) = start_native(&world, compile_limits(), operator(&seat)).await;
        let response = server.request(&compile_request(&fresh(&json!({})))).await;
        assert_eq!(response.status, 200, "{}", response.body);
        let account = authority(&response.json());
        assert!(
            account["configured"]["strategy"] == "escalate",
            "authority strategy must be escalate"
        );
        assert!(
            account["configured"]["worst_case"] == worst_case,
            "configured worst case must follow the repair preference"
        );
        assert!(
            account["max_calls"] == max_calls,
            "authority max_calls must match the configured grant"
        );
        assert!(
            account["http_requests"]["sent"] == 1,
            "the authority receipt must count one sent request"
        );
        assert_eq!(seat.calls(), 1);
        server.stop().await.expect("clean stop");
    }
}
