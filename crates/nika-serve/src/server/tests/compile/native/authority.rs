// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Authoring grants bound actual requests, separately from repair preferences. A creation is
//! authored under the shared default strategy (`escalate`): the document door asks for the
//! whole document first, and its READY waits for a judgment the grant must leave room for.

use super::*;

/// The account a round's receipt keeps: the grant, what was configured, what was sent.
fn authority(document: &Value) -> Value {
    document["provenance"]["authoring"]["backend"]["authority"].clone()
}

/// The roles of the calls a round journaled, with how each ended (`null` when it answered).
pub(super) fn roles(document: &Value) -> Vec<(String, Value)> {
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

/// The permits the document states for [`INTENT`]'s read and write.
const STATED_PERMITS: &str = "  fs:\n    read: [\"./a.md\"]\n    write: [\"./b.md\"]\n";

/// The lines of one task in an emitted document: its key line and everything nested under it.
fn task_block<'a>(candidate: &'a str, task: &str) -> Vec<&'a str> {
    let key = format!("  {task}:");
    let mut lines = candidate.lines().skip_while(|line| *line != key);
    let head = lines.next().into_iter();
    head.chain(lines.take_while(|line| line.starts_with("    ")))
        .collect()
}

/// A document that writes a destination the request never names: the laws ask one repair.
fn loose_document() -> String {
    native_answer(&candidate(RUN_MODEL, false).replace("./b.md", "./elsewhere.md"))
}

/// The task of the document that writes `./b.md`: the one the judge points to.
const WRITE_TASK: &str = "write_result";

/// The draft detail of the plan the repair answers in [`judged_repair`]: the same read, draft
/// and write, the draft detailed otherwise, so the repaired candidate is other bytes than the
/// ones the judge declined (those are never asked of it again).
const REPAIRED_DRAFT: &str = "do something clever with it, in plain words";

/// The judge's closed choices, then the document the repair call answers.
pub(super) fn judged_repair() -> Vec<Reply> {
    judged_then(REPAIRED_DRAFT)
}

/// The document, the judge's closed choices over it, the repair's document with the prompt
/// stating `repaired`, then an approving judge.
pub(super) fn judged_then(repaired: &str) -> Vec<Reply> {
    vec![
        Reply::Text(document_answer(DRAFT)),
        Reply::Text(json!({"choice": "unfaithful"}).to_string()),
        // Each part of the request asked alone: the read carried, the write missing. A part
        // judged missing is a defect only with its reason: the judge then points to the task
        // that does it differently, and the repair starts from that part and that reason.
        Reply::Text(json!({"choice": "carried"}).to_string()),
        Reply::Text(json!({"choice": "missing"}).to_string()),
        Reply::Text(json!({"choice": format!("task-{WRITE_TASK}")}).to_string()),
        Reply::Text(document_answer(repaired)),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]
}

/// The defect [`judged_repair`] locates, with the reason its pointer gave, as a repair and a
/// finding read it.
pub(super) const NOTED_DEFECT: &str =
    "then write ./b.md (the judge points to the task write_result)";

/// The judge's located defect that `repairs` repairs did not settle.
pub(super) fn unrepaired_finding(repairs: usize) -> String {
    format!(
        "The judge compared the whole request with the candidate's bytes: it does not carry « {NOTED_DEFECT} ». {repairs} repair(s) from that defect did not settle it; nothing is READY. Next: a stronger authoring model, or a restatement of that part."
    )
}

/// Every finding of a document, in order, as its kind, target and message.
pub(super) fn findings(document: &Value) -> Vec<(String, String, String)> {
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    (document["diagnostics"].as_array().into_iter().flatten())
        .map(|d| (text(&d["kind"]), text(&d["target"]), text(&d["message"])))
        .collect()
}

/// The `verify_held` finding of a candidate the judge declined with a located defect: that
/// verifier is not asked again on these bytes, in this compile or in a later round that carries
/// the verdict (the server passes no declined verdict from one request to the next).
pub(super) const HELD_DEFECTS: &str = "The candidate was judged and not accepted: the parts named above stay missing. It is shown, never offered, and nothing was written; this verifier is not asked again on these bytes, in this compile or in a later round that carries this verdict. A correction of the request, another authoring model or another verifier can decide it.";

/// The findings of a document held with [`judged_repair`]'s defect unsettled after `repairs`
/// repairs: the door's journal of its rounds (one per repair after the first, each document
/// accepted by the evidence laws), the judge's defect, then the held marker.
pub(super) fn held_findings(repairs: usize) -> Vec<(String, String, String)> {
    let finding =
        |kind: &str, target: &str, message: String| (kind.to_owned(), target.to_owned(), message);
    let rounds = repairs + 1;
    let accepted: Vec<String> = (0..rounds)
        .map(|round| format!("round {round}: accepted"))
        .collect();
    vec![
        finding(
            "applied",
            "authoring_native",
            format!(
                "The authoring conversation recorded {rounds} round(s), including {rounds} candidate or sketch judgment(s): {}.",
                accepted.join("; ")
            ),
        ),
        finding(
            "unknown",
            "semantic_verification",
            unrepaired_finding(repairs),
        ),
        finding("applied", "verify_held", HELD_DEFECTS.to_owned()),
    ]
}

/// The verification steps of a document's decision route, in order.
pub(super) fn verify_steps(document: &Value) -> Vec<String> {
    let route = document["provenance"]["decision"]["route"].as_array();
    (route.into_iter().flatten())
        .filter_map(Value::as_str)
        .filter(|step| step.starts_with("verify:"))
        .map(str::to_owned)
        .collect()
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
        Reply::Text(loose_document()),
        Reply::Text(document_answer(DRAFT)),
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
            ("document".to_owned(), Value::Null),
            ("document-repair".to_owned(), json!("admission_refused")),
        ],
        "the repair is refused before a byte leaves: {document:#}"
    );
    assert!(document.to_string().contains("max_calls"));
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn unidentified_responses_remain_visible_beside_named_responses() {
    let world = TestWorld::new();
    let mut named: Value =
        serde_json::from_str(&completion(&document_answer(DRAFT))).expect("completion");
    named["model"] = json!("observed-first-response");
    let seat = Seat::start(vec![
        Reply::Status(200, named.to_string()),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]);
    // The document and its judgment; repairs stay the default preference.
    let operator = NativeAuthoring::new(SEAT, seat.providers()).with_max_calls(4);
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    let document = response.json();
    assert_eq!(document["status"], "ready", "{document:#}");
    assert_eq!(seat.calls(), 2, "the named document, then its judgment");
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
    let seat = Seat::start(vec![Reply::Busy, Reply::Text(open_document(""))]);
    let operator = NativeAuthoring::new(SEAT, seat.providers()).with_repairs(0);
    let (server, _) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    let bodies = seat.bodies();
    assert_eq!(bodies.len(), 2, "the transient retry reached the seat");
    assert_eq!(bodies[0], bodies[1], "the retry sends the same request");
    assert_eq!(document["provenance"]["authoring"]["calls"], 1);
    assert_eq!(roles(&document), [("document".to_owned(), Value::Null)]);
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
    assert_eq!(document["questions"][0]["key"], OPEN_KEY);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_one_request_limit_refuses_a_transport_retry() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Busy, Reply::Text(document_answer(DRAFT))]);
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
        // The operator's grant: the document, its judgment, the request's two parts asked
        // alone, the task the missing one points to, the repair, the repaired document's
        // judgment — one real repair.
        (json!({}), 7, true),
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
        let fields = json!({"limits": limits});
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
        Reply::Text(document_answer(DRAFT)),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]);
    let operator = NativeAuthoring::new(SEAT, seat.providers());
    let (server, backend) = start_native(&world, compile_limits(), operator).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(document["status"], "ready", "{document:#}");
    assert_eq!(seat.calls(), 2);
    let candidate = document["candidate"].as_str().expect("candidate");
    assert!(candidate.contains(STATED_PERMITS), "{candidate}");
    assert_eq!(
        roles(&document),
        [
            ("document".to_owned(), Value::Null),
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
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    let document = response.json();
    assert_eq!(document["status"], "ready", "{document:#}");
    assert_eq!(seat.calls(), 7);
    assert_eq!(authority(&document)["max_calls"], Value::Null);
    assert_eq!(authority(&document)["configured"]["repairs"], Value::Null);
    let journaled: Vec<String> = roles(&document).into_iter().map(|(role, _)| role).collect();
    assert_eq!(
        journaled,
        [
            "document",
            "judge_request",
            "judge_part",
            "judge_part",
            "judge_point",
            "document-repair",
            "judge_request"
        ]
    );
    let attempts = document["provenance"]["decision"]["semantic_verification"]
        .as_array()
        .expect("the attempts");
    assert_eq!(attempts.len(), 2, "{attempts:#?}");
    assert_located(&attempts[0]);
    // The repair starts from the part and the judge's reason, never from the doubt alone.
    let told = user_texts(&seat.bodies()[5]);
    assert_eq!(told.len(), 2, "the request, then what the verifier found");
    assert!(
        told[1].starts_with(REPAIR_OPENING) && told[1].contains(LOCATED),
        "{}",
        told[1]
    );
    // The repaired document's judgment carries the request whole, over other bytes than the
    // ones the judge declined: the bytes now proposed.
    let proposed = document["candidate"].as_str().expect("the candidate");
    assert_carried(&attempts[1], proposed);
    assert_ne!(
        attempts[0]["candidate_sha256"],
        attempts[1]["candidate_sha256"]
    );
    assert_eq!(
        verify_steps(&document),
        ["verify: repair 1", "verify: judged (authoring_provider)"]
    );
    server.stop().await.expect("clean stop");
}

/// The first attempt of [`judged_repair`]: the whole verdict doubts, and each part of the
/// request is asked alone as evidence: the read carried, the write missing. Neither part
/// restricts, and a request of two parts offers each `no_operation`. The missing write is a
/// defect with its reason, the task the judge names among the candidate's; a located defect
/// asks no extra-operation question.
fn assert_located(first: &Value) {
    assert_eq!(first["doubt"], json!(["unfaithful"]), "{first:#}");
    assert_eq!(first["defects"], json!(["then write ./b.md"]));
    assert_eq!(
        first["notes"],
        json!([{"defect": "then write ./b.md", "note": "the judge points to the task write_result"}])
    );
    for settled in ["unknown", "contested", "unsettled"] {
        assert_eq!(first[settled], json!([]), "{settled}");
    }
    assert_eq!(first["settled_by"], Value::Null);
    // The record states how the judge declined these bytes (rejected, not an abstention), that
    // every call it sent was answered, and the whole request it asked: its own verdict, four
    // questions sent, answered and consumed.
    for (field, value) in [
        ("declined", json!(true)),
        ("rejected", json!(true)),
        ("stopped", json!(false)),
        ("whole_asked", json!(true)),
        ("request", json!(INTENT)),
        ("same_bytes_as", Value::Null),
        ("attempted", json!(4)),
        ("returned", json!(4)),
        ("consumed", json!(4)),
    ] {
        assert_eq!(first[field], value, "{field}: {first:#}");
    }
    assert_eq!(first["usage"]["calls"], 4, "{first:#}");
    let asked = first["questions"].as_array().expect("questions");
    assert_eq!(asked.len(), 4, "{first:#}");
    assert_eq!(asked[0]["question"], "verify-request");
    assert_eq!(asked[0]["choice"], "unfaithful");
    // Only an earlier part can be superseded by a later one: the last part is never offered it.
    let parts = [
        (
            "Read ./a.md and do something clever with it",
            "carried",
            json!(["carried", "missing", "superseded", "no_operation", "none"]),
        ),
        (
            "then write ./b.md",
            "missing",
            json!(["carried", "missing", "no_operation", "none"]),
        ),
    ];
    for (k, (part, choice, offered)) in parts.into_iter().enumerate() {
        let question = &asked[k + 1];
        assert_eq!(question["question"], format!("verify-part-{k}"));
        assert_eq!(question["role"], "judge_part");
        assert_eq!(question["choice"], choice);
        assert_eq!(question["options"], offered, "{question:#}");
        assert_eq!(
            question["clause"],
            json!({"text": part, "restricts": false})
        );
    }
    assert_pointed(&asked[3]);
}

/// An attempt whose one question carried the whole request over `proposed`, the READY bytes.
fn assert_carried(second: &Value, proposed: &str) {
    assert_eq!(second["questions"].as_array().map(Vec::len), Some(1));
    assert_eq!(second["questions"][0]["question"], "verify-request");
    assert_eq!(second["questions"][0]["choice"], "faithful");
    assert_eq!(second["settled_by"], "verify-request");
    for (field, value) in [
        ("defects", json!([])),
        ("doubt", json!([])),
        ("declined", json!(false)),
        ("rejected", json!(false)),
        ("stopped", json!(false)),
        ("whole_asked", json!(true)),
        ("request", json!(INTENT)),
        ("same_bytes_as", Value::Null),
    ] {
        assert_eq!(second[field], value, "{field}: {second:#}");
    }
    assert_eq!(second["candidate_sha256"], sha256_hex(proposed.as_bytes()));
}

/// What a document repair is told before the findings it starts from.
const REPAIR_OPENING: &str =
    "COMPILER DIAGNOSTICS on your candidate. Return the complete corrected JSON answer";

/// The judge's located defect as the repair reads it: the part and the judge's reason.
const LOCATED: &str = "\n1. [semantic_verification] the judge compared the whole request with the candidate's bytes: it does not carry « then write ./b.md » · the judge's reason: the judge points to the task write_result\n";

/// The texts of a request's user turns, in order.
fn user_texts(body: &Value) -> Vec<String> {
    (body["messages"].as_array().into_iter().flatten())
        .filter(|message| message["role"] == "user")
        .filter_map(|message| message["content"].as_str().map(str::to_owned))
        .collect()
}

/// The question after the write judged missing: why, among every task of the candidate in
/// document order, an operation no task performs, or no task failing it; answered with the
/// task that writes `./b.md`.
fn assert_pointed(point: &Value) {
    assert_eq!(point["question"], "verify-point-1", "{point:#}");
    assert_eq!(point["role"], "judge_point");
    assert_eq!(point["choice"], format!("task-{WRITE_TASK}"));
    assert_eq!(
        point["options"],
        json!([
            "task-read_source",
            "task-transform",
            "task-write_result",
            "omitted",
            "no_task",
            "none"
        ])
    );
    assert_eq!(
        point["clause"],
        json!({"text": "then write ./b.md", "restricts": false})
    );
}

/// A document that leaves a value the request never names asks it in one request: no
/// judgment is due, no second request is sent, and the round is kept for its answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_document_question_round_sends_one_request_and_keeps_its_token() {
    let world = TestWorld::new();
    let seat = Seat::start(question_round());
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
    assert_eq!(keys, [&json!(OPEN_KEY)]);
    assert_eq!(seat.calls(), 1);
    assert_eq!(roles(&document), [("document".to_owned(), Value::Null)]);
    assert_eq!(document["provenance"]["strategy"], "native");
    // Its answer round replays the kept round.
    assert!(response.header("nika-compile-replay").is_some());
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_caller_can_select_large_limits_where_the_operator_left_preparation_open() {
    let world = TestWorld::new();
    let seat = Seat::start(question_round());
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

/// A request that gates its write on the human: the document carries the gate.
pub(super) const GATED: &str =
    "Read ./a.md and do something clever with it, then ask me before you write ./b.md";

fn gated_round() -> Vec<Reply> {
    gated_round_judged(JUDGE_APPROVES)
}

/// The document for [`GATED`]: the human's confirmation gates the write.
const GATED_DOCUMENT: &str = "nika: clever-rewrite\nmodel: mistral/mistral-small-latest\npermits:\n  tools: [\"nika:read\", \"nika:prompt\", \"nika:write\"]\n  fs:\n    read: [\"./a.md\"]\n    write: [\"./b.md\"]\ntasks:\n  read_source:\n    invoke:\n      tool: nika:read\n      args: { path: ./a.md }\n  transform:\n    with: { text: \"${{ tasks.read_source.output }}\" }\n    infer:\n      max_tokens: 600\n      prompt: \"Rewrite this text in a clever way, inventing nothing: ${{ with.text }}\"\n  approve:\n    invoke:\n      tool: nika:prompt\n      args: { mode: confirm, message: \"Write the rewrite to ./b.md?\" }\n  write_result:\n    after: { approve: success }\n    with:\n      approved: ${{ tasks.approve.output }}\n      content: ${{ tasks.transform.output }}\n    when: ${{ with.approved == true }}\n    invoke:\n      tool: nika:write\n      args: { path: ./b.md, content: \"${{ with.content }}\" }\n";

/// The document door's answer for [`GATED`], then the judge's `answer`.
pub(super) fn gated_round_judged(answer: &str) -> Vec<Reply> {
    vec![
        Reply::Text(native_answer(GATED_DOCUMENT)),
        Reply::Text(answer.to_owned()),
    ]
}

/// An explicit grant of two: the document and its judgment — READY, the document gating the
/// write on the human. An explicit limit of one sends the document alone: its judgment is
/// refused before it leaves, and nothing is READY.
#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_grant_reaches_a_judged_document_and_a_limit_of_one_stops_the_second() {
    let request = || fresh(&json!({"intent": GATED}));
    let world = TestWorld::new();
    let seat = Seat::start(gated_round());
    let granted = NativeAuthoring::new(SEAT, seat.providers()).with_max_calls(2);
    let (server, _) = start_native(&world, compile_limits(), granted).await;
    let response = server.request(&compile_request(&request())).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(document["status"], "ready", "{document:#}");
    let journaled: Vec<String> = roles(&document).into_iter().map(|(role, _)| role).collect();
    assert_eq!(journaled, ["document", "judge_request"]);
    assert_eq!(seat.calls(), 2);
    let account = authority(&document);
    assert_eq!(account["http_requests"]["sent"], 2);
    assert_eq!(account["invocations"]["sent"], 2);
    assert_eq!(account["max_calls"], 2);
    // The document: the stated paths, their permits and the human gate.
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
    // The first request asks the seat for the whole document.
    let schema = seat.bodies()[0]["response_format"].to_string();
    assert!(schema.contains("candidate_lines"), "{schema}");
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
            ("document".to_owned(), Value::Null),
            ("judge_request".to_owned(), json!("admission_refused")),
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
        let seat = Seat::start(question_round());
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
