// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a judged round answers on the wire (R4 A11, R6). A candidate is proposed only READY,
//! once an admitted judgment carried the whole request. Bytes the judge declined are held: the
//! document still carries them as its candidate, INCOMPLETE, with the `verify_held` finding, and
//! keeps no round to replay them; the judge is never asked of them again, a repair that returns
//! them included. A candidate no admitted judgment was made of is withdrawn with the
//! `verify_resume` finding, its round kept, and that round's zero-call replay asks no judge.
//! Every document validates against the contract the server publishes, which names each marker
//! it carries.

use std::collections::BTreeSet;

use super::authority::{
    GATED, answered, findings, gated_round_judged, held_findings, judged_then, roles, verify_steps,
};
use super::openapi::{ANSWER, REQUEST, exchange, schema_at, served};
use super::*;

/// The compiler's own targets a judged round's findings may carry.
const MARKERS: [&str; 3] = ["semantic_verification", "verify_held", "verify_resume"];

/// The published contract names every marker `document` carries: the outcome's `diagnostics`
/// describe each, and its `candidate` says what it is under each status a marker leaves.
fn assert_published(published: &Value, document: &Value) {
    let outcome = &published["components"]["schemas"]["CompileOutcome"]["properties"];
    let diagnostics = outcome["diagnostics"]["description"]
        .as_str()
        .expect("the diagnostics are described");
    let carried: BTreeSet<String> = (findings(document).into_iter())
        .map(|(_, target, _)| target)
        .filter(|target| MARKERS.contains(&target.as_str()))
        .collect();
    for target in &carried {
        assert!(
            diagnostics.contains(&format!("`{target}`")),
            "`{target}` is published: {diagnostics}"
        );
    }
    let candidate = outcome["candidate"]["description"]
        .as_str()
        .expect("the candidate is described");
    for word in ["`ready`", "`incomplete`", "`verify_held`"] {
        assert!(candidate.contains(word), "{word}: {candidate}");
    }
    // The generation-2 outcome reads both fields as generation 1 states them.
    let native = &published["components"]["schemas"]["CompileOutcomeV2"]["properties"];
    for field in ["candidate", "diagnostics"] {
        assert_eq!(
            native[field]["$ref"],
            format!("#/components/schemas/CompileOutcome/properties/{field}")
        );
    }
}

/// A repair whose plan the compiler assembles into the very bytes the judge declined: the judge
/// is not asked of them again. The earlier verdict stands as a new attempt with no call, the
/// repairs end there although no count bounds them, and the bytes are held.
#[tokio::test(flavor = "multi_thread")]
async fn a_repair_that_returns_the_declined_bytes_asks_the_judge_nothing_and_holds_them() {
    let world = TestWorld::new();
    // The repair answers the plan the judge declined; the approval scripted after it is never
    // asked.
    let seat = Seat::start(judged_then(DRAFT));
    let operator = NativeAuthoring::new(SEAT, seat.providers());
    let (server, _backend) = start_native(&world, compile_limits(), operator).await;
    let (_, published) = served(&server).await;
    let (request, answer) = (
        schema_at(&published, REQUEST),
        schema_at(&published, ANSWER),
    );
    let response = exchange(&server, &request, &answer, &fresh(&answered())).await;
    let document = response.json();
    assert_eq!(document["status"], "incomplete", "{document:#}");
    assert_eq!(seat.calls(), 6, "no call after the repair");
    // No request bound was configured: the receipt says so, and the contract admits it.
    let account = &document["provenance"]["authoring"]["backend"]["authority"];
    assert_eq!(account["max_calls"], Value::Null);
    assert_eq!(account["source"], "default: no request bound");
    let journaled: Vec<String> = roles(&document).into_iter().map(|(role, _)| role).collect();
    assert_eq!(
        journaled,
        [
            "plan",
            "judge_request",
            "judge_part",
            "judge_part",
            "judge_point",
            "repair"
        ]
    );
    assert_eq!(
        verify_steps(&document),
        [
            "verify: repair 1",
            "verify: same bytes, earlier verdict stands",
            "verify: no progress",
            "verify: not ready",
            "verify: doubted, not replayable"
        ]
    );
    // The held bytes are still the document's candidate: the ones judged, then repeated.
    let candidate = document["candidate"].as_str().expect("the held candidate");
    let sha = sha256_hex(candidate.as_bytes());
    let attempts = document["provenance"]["decision"]["semantic_verification"]
        .as_array()
        .expect("the attempts");
    assert_eq!(attempts.len(), 2, "{attempts:#?}");
    let (first, again) = (&attempts[0], &attempts[1]);
    assert_eq!(first["candidate_sha256"], sha.as_str());
    assert_eq!(first["same_bytes_as"], Value::Null);
    assert_eq!(first["questions"].as_array().map(Vec::len), Some(4));
    // The repeated attempt: the same bytes and judge, the earlier verdict read back, no call.
    assert_eq!(again["attempt"], 1);
    assert_eq!(again["candidate_sha256"], sha.as_str());
    assert_eq!(again["same_bytes_as"], 0);
    assert_eq!(again["questions"], json!([]));
    for count in ["attempted", "returned", "consumed"] {
        assert_eq!(again[count], 0, "{count}");
    }
    assert_eq!(
        again["usage"],
        json!({"calls": 0, "input_tokens": 0, "output_tokens": 0, "complete": true})
    );
    for field in [
        "judge",
        "defects",
        "notes",
        "doubt",
        "unknown",
        "contested",
        "unsettled",
        "declined",
        "rejected",
        "settled",
        "stopped",
        "whole_asked",
        "request",
        "settled_by",
        "reference",
    ] {
        assert_eq!(again[field], first[field], "{field}");
    }
    assert_eq!(
        (&again["rejected"], &again["settled"]),
        (&json!(true), &json!(false))
    );
    // A repeat within this compile, never a verdict carried from an earlier round: the server
    // hands no declined verdict from one request to the next.
    assert_eq!(
        (&first["carried"], &again["carried"]),
        (&json!(false), &json!(false))
    );
    // The core's pending whole request, the judge's defect after its one repair, the marker.
    assert_eq!(
        findings(&document),
        held_findings(candidate, 1),
        "{document:#}"
    );
    assert_eq!(document["questions"], json!([]));
    assert_eq!(document["check_preview"]["scope"], "sourceOnly");
    assert_eq!(document["provenance"]["plan"], Value::Null);
    assert!(
        response.header("nika-compile-replay").is_none(),
        "nothing kept"
    );
    assert_published(&published, &document);
    server.stop().await.expect("clean stop");
}

/// The sketch door's round whose judge answers a choice it was not offered: no admitted
/// judgment of the candidate exists, nothing was declined. The candidate is withdrawn and its
/// round kept; that round's replay calls no one, so it judges nothing either: the same bytes
/// come back held for a judge, the whole request pending.
#[tokio::test(flavor = "multi_thread")]
async fn a_candidate_no_admitted_judgment_was_made_of_is_withdrawn_and_its_round_kept() {
    let world = TestWorld::new();
    let seat = Seat::start(gated_round_judged(r#"{"choice":"maybe"}"#));
    let operator = NativeAuthoring::new(SEAT, seat.providers());
    let (server, _backend) = start_native(&world, compile_limits(), operator).await;
    let (_, published) = served(&server).await;
    let (request, answer) = (
        schema_at(&published, REQUEST),
        schema_at(&published, ANSWER),
    );
    let mut fields = answered();
    fields["intent"] = json!(GATED);
    let response = exchange(&server, &request, &answer, &fresh(&fields)).await;
    let document = response.json();
    assert_eq!(document["status"], "incomplete", "{document:#}");
    assert_eq!(document["candidate"], Value::Null, "{document:#}");
    assert_eq!(document["check_preview"], Value::Null);
    assert_eq!(document["questions"], json!([]));
    assert_eq!(seat.calls(), 4);
    let journaled: Vec<String> = roles(&document).into_iter().map(|(role, _)| role).collect();
    assert_eq!(journaled, ["plan", "sketch", "fill", "judge_request"]);
    // The judge's answer is recorded and admitted nothing: an unknown, never a doubt.
    let attempts = document["provenance"]["decision"]["semantic_verification"]
        .as_array()
        .expect("the attempts");
    assert_eq!(attempts.len(), 1, "{attempts:#?}");
    let attempt = &attempts[0];
    assert_eq!(
        attempt["questions"],
        json!([{
            "question": "verify-request",
            "options": ["faithful", "unfaithful", "none"],
            "error": "the seat chose `maybe`, which was not offered",
            "role": "judge_request",
        }])
    );
    for (field, value) in [
        ("unknown", json!([GATED])),
        ("doubt", json!([])),
        ("defects", json!([])),
        ("contested", json!([])),
        ("declined", json!(false)),
        ("rejected", json!(false)),
        ("settled", json!(false)),
        ("stopped", json!(false)),
        ("whole_asked", json!(true)),
        ("request", json!(GATED)),
        ("settled_by", Value::Null),
        ("same_bytes_as", Value::Null),
        ("carried", json!(false)),
        ("attempted", json!(1)),
        ("returned", json!(1)),
        ("consumed", json!(0)),
    ] {
        assert_eq!(attempt[field], value, "{field}: {attempt:#}");
    }
    let finding = |kind: &str, target: &str, message: &str| {
        (kind.to_owned(), target.to_owned(), message.to_owned())
    };
    // The sketch door's own journal (its graph and its fills, each accepted by the evidence
    // laws), the request no admitted judgment settled, then the marker.
    assert_eq!(
        findings(&document),
        [
            finding(
                "applied",
                "authoring_native",
                "The authoring conversation recorded 2 round(s), including 2 candidate or sketch judgment(s): round 0: accepted; round 1: accepted."
            ),
            finding(
                "unknown",
                "semantic_verification",
                &format!(
                    "The judge could not settle `{GATED}` against the candidate (it abstained, answered outside its options, or its call failed); nothing is READY on it. Next: a judge that answers, or a restatement the deterministic reader reads."
                )
            ),
            finding(
                "applied",
                "verify_resume",
                "The candidate was not judged, so it is not offered; its bytes are kept: a round that replays this record under a judge asks it on the same candidate, with no new authoring call (a replay with no judge judges nothing)."
            ),
        ],
        "{document:#}"
    );
    assert_eq!(
        verify_steps(&document),
        ["verify: not ready", "verify: unjudged, record kept"]
    );
    assert_published(&published, &document);
    // The kept round: its replay is zero calls, so no judge is asked. The same bytes come back,
    // never proposed: the whole request still pending on them.
    let token = token_of(&response);
    let replayed = exchange(&server, &request, &answer, &replay(&token, &fields)).await;
    let replayed = replayed.json();
    assert_eq!(replayed["compile_version"], 1, "{replayed:#}");
    assert_eq!(replayed["status"], "incomplete", "{replayed:#}");
    let bytes = replayed["candidate"].as_str().expect("the kept bytes");
    assert_eq!(attempt["candidate_sha256"], sha256_hex(bytes.as_bytes()));
    let open: Vec<&Value> = (replayed["provenance"]["decision"]["pending"]["open"].as_array())
        .into_iter()
        .flatten()
        .map(|clause| &clause["clause"])
        .collect();
    assert_eq!(open, [&json!(GATED)], "{replayed:#}");
    assert_eq!(seat.calls(), 4, "the replay called no one");
    server.stop().await.expect("clean stop");
}
