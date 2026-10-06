// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A candidate its judge answered and did not accept is never replayed to it (R6), through the
//! real CLI on a scripted loopback seat under the semantic Sketch route: the plan record an
//! answer round replays goes once that round's judge holds the candidate, and the next answer
//! round authors again. The verdict is kept beside the intent: a later round that authors the
//! held bytes again, `--fresh` or not, never asks that judge on them. No provider, network beyond
//! 127.0.0.1, credential store or paid call.

use super::{LoopbackSeat, command, result};
use nika_onboard::compile::intent_sha256;
use serde_json::{Value, json};
use std::path::Path;
use std::process::Output;

const INTENT: &str = "Write the greeting I choose to ./out/result.txt.";
/// The intent's one part, as the judge asks it alone.
const PART: &str = "Write the greeting I choose to ./out/result.txt";
/// The answer every answer round gives.
const ANSWER: &str = "const.greeting=\"hello\"";
/// The compiler's `verify_held` finding on a candidate its judge rejected with no defect located.
const HELD: &str = "The candidate was judged and not accepted, with no defect a repair could start from: it is shown, never offered, and nothing was written. A correction of the request or another verifier can decide it.";

/// One real `nika compile --json` of [`INTENT`] on the vLLM loopback route under the Sketch
/// strategy, with `extra` arguments: its exit, its document and every request the seat received.
fn compile(room: &Path, script: &[String], extra: &[&str]) -> (Output, Value, Vec<Value>) {
    let seat = LoopbackSeat::start(script.to_vec());
    let out = command(room)
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .args(["compile", INTENT, "--authoring-model", "vllm/loopback-seat"])
        .args(["--authoring-strategy", "sketch", "--authoring-timeout", "2"])
        .args(extra)
        .arg("--json")
        .output()
        .expect("CLI");
    let doc = result(&out);
    (out, doc, seat.bodies())
}

/// The sketch, which asks the greeting, and its fills: one task, `task`, writing the greeting to
/// the stated destination.
fn authored(task: &str) -> Vec<String> {
    vec![
        json!({"name": "greeting", "tasks": [{"id": task, "verb": "invoke",
            "tool": "nika:write", "purpose": "save the greeting",
            "writes": ["./out/result.txt"]}],
            "questions": [{"key": "const.greeting", "label": "What greeting?",
                "answer_type": "text", "why": "the human chooses it"}],
            "gaps": [], "notes": "graph"})
        .to_string(),
        json!({"fills": [{"task": task, "field": "args.content",
            "value": "${{ const.greeting }}"}], "notes": "fills"})
        .to_string(),
    ]
}

/// A closed choice the judge was asked, as its request shows it: the state it judged and the
/// option keys offered, in order. `None` for an authoring call.
fn judged(body: &Value) -> Option<(Value, Vec<String>)> {
    let said = (body["messages"].as_array()?.iter())
        .filter(|message| message["role"] == "user")
        .find_map(|message| message["content"].as_str()?.strip_prefix("STATE:\n"))?;
    let (state, options) = said.split_once("\n\nOPTIONS:\n")?;
    let keys = (options.lines())
        .filter_map(|line| line.strip_prefix("- ")?.split_once(": "))
        .map(|(key, _)| key.to_owned())
        .collect();
    Some((serde_json::from_str(state).ok()?, keys))
}

/// The sha256 of the candidate a judge question judged, when the request is one.
fn judged_sha(body: &Value) -> Option<String> {
    let (state, _) = judged(body)?;
    let candidate = state["candidate_nika"].as_str()?;
    Some(nika_event::source_id::sha256_hex(candidate.as_bytes()))
}

/// The applied findings an outcome document carries on `target`.
fn applied<'a>(doc: &'a Value, target: &str) -> Vec<&'a str> {
    (doc["diagnostics"].as_array().into_iter().flatten())
        .filter(|d| d["kind"] == "applied" && d["target"] == target)
        .filter_map(|d| d["message"].as_str())
        .collect()
}

/// Round 1 asks the greeting and records its plan for the answer round. Round 2 answers it: the
/// record replays to the round's judge, which rejects the request and carries its one part with
/// no extra operation, so the candidate is held and the record goes. Round 3 answers again: with
/// no record to replay, it authors afresh (other bytes here) and its judge reads only those.
#[test]
fn a_held_answer_round_removes_the_record_and_the_next_round_authors_again() {
    let room = tempfile::tempdir().expect("room");
    let record = room
        .path()
        .join(".nika/compile")
        .join(format!("{}.plan.json", intent_sha256(INTENT)));
    let (out, doc, bodies) = compile(room.path(), &authored("save"), &[]);
    assert_eq!(out.status.code(), Some(2), "{doc}");
    assert_eq!(doc["questions"][0]["key"], "const.greeting", "{doc}");
    assert_eq!(bodies.len(), 2, "the sketch and its fills: {doc}");
    let kept: Value =
        serde_json::from_str(&std::fs::read_to_string(&record).expect("the kept record"))
            .expect("a record");
    assert_eq!(
        kept["resume"], false,
        "an open question is no failed judgment"
    );
    // Round 2: replayed (no authoring call), judged and held.
    let doubt = [
        r#"{"choice":"unfaithful"}"#,
        r#"{"choice":"carried"}"#,
        r#"{"choice":"only_requested"}"#,
    ]
    .map(str::to_owned);
    let (out, doc, bodies) = compile(room.path(), &doubt, &["--answer", ANSWER]);
    assert_eq!(out.status.code(), Some(2), "{doc}");
    assert_eq!(doc["status"], "incomplete", "{doc}");
    let asked: Vec<Vec<String>> = (bodies.iter())
        .map(|body| {
            judged(body)
                .expect("a judge question, never an authoring call")
                .1
        })
        .collect();
    assert_eq!(
        asked,
        [
            vec!["faithful", "unfaithful", "none"],
            vec!["carried", "missing", "none"],
            vec!["only_requested", "task-save", "none"],
        ]
    );
    let (part, _) = judged(&bodies[1]).expect("the part asked alone");
    assert_eq!(part["clause"], json!({"text": PART}));
    assert_eq!(applied(&doc, "verify_held"), [HELD], "{doc}");
    assert_eq!(doc["provenance"]["plan"], Value::Null, "{doc}");
    let held = judged_sha(&bodies[0]).expect("the held bytes");
    let candidate = doc["candidate"].as_str().expect("the held preview");
    assert_eq!(
        nika_event::source_id::sha256_hex(candidate.as_bytes()),
        held,
        "the judged bytes, shown as the preview"
    );
    assert_eq!(doc["written"], Value::Null, "{doc}");
    assert_eq!(doc.get("plan_record_error"), None, "{doc}");
    assert!(
        !record.exists(),
        "the record of the held candidate is removed"
    );
    assert!(
        !room.path().join("out").exists(),
        "nothing was written or run"
    );
    // Round 3: nothing left to replay: the answer round authors again, then its judge reads its
    // own candidate, never the held bytes.
    let mut fresh = authored("write_greeting");
    fresh.push(r#"{"choice":"faithful"}"#.to_owned());
    let (out, doc, bodies) = compile(room.path(), &fresh, &["--answer", ANSWER]);
    assert_eq!(out.status.code(), Some(0), "{doc}");
    let shas: Vec<Option<String>> = bodies.iter().map(judged_sha).collect();
    assert_eq!(
        shas.len(),
        3,
        "the sketch, its fills, the whole request: {doc}"
    );
    assert_eq!(shas[..2], [None, None], "authoring calls first: {doc}");
    let judged_fresh = shas[2].clone().expect("the whole request judged");
    assert_ne!(judged_fresh, held, "other bytes");
    let route = doc["provenance"]["decision"]["route"].to_string();
    assert!(!route.contains("replayed plan"), "{route}");
    assert_eq!(doc["status"], "ready", "{doc}");
}

/// The verdicts kept beside [`INTENT`]'s plan record that rejected its candidate bytes.
fn kept_declined(room: &Path) -> Value {
    let path = room
        .join(".nika/compile")
        .join(format!("{}.declined.json", intent_sha256(INTENT)));
    let text = std::fs::read_to_string(path).expect("the kept rejections");
    serde_json::from_str(&text).expect("a record")
}

/// A round that authors the held bytes again never asks their judge (R6 across rounds). Round 2
/// holds the replayed candidate, and the verdict that rejected it is kept beside the intent.
/// Round 3 has no record to replay: it authors afresh, into those very bytes, and the verdict
/// kept from round 2 decides them with no call: held again, its attempt `carried`, the same
/// verdict read back. A `--fresh` round carries it too, and the verdict is kept once.
#[test]
fn a_round_that_authors_the_held_bytes_again_never_asks_their_judge() {
    let room = tempfile::tempdir().expect("room");
    let (out, doc, _) = compile(room.path(), &authored("save"), &[]);
    assert_eq!(out.status.code(), Some(2), "{doc}");
    let doubt = [
        r#"{"choice":"unfaithful"}"#,
        r#"{"choice":"carried"}"#,
        r#"{"choice":"only_requested"}"#,
    ]
    .map(str::to_owned);
    let (out, doc, bodies) = compile(room.path(), &doubt, &["--answer", ANSWER]);
    assert_eq!(out.status.code(), Some(2), "{doc}");
    assert_eq!(bodies.len(), 3, "the request, its part, the extra question");
    let held = judged_sha(&bodies[0]).expect("the held bytes");
    assert_eq!(applied(&doc, "verify_held"), [HELD], "{doc}");
    let judged = doc["provenance"]["decision"]["semantic_verification"][0].clone();
    assert_eq!(judged["candidate_sha256"], held.as_str());
    assert_eq!(
        (&judged["declined"], &judged["rejected"], &judged["settled"]),
        (&json!(true), &json!(true), &json!(false))
    );
    assert_eq!(judged["carried"], false);
    let kept = kept_declined(room.path());
    assert_eq!(
        kept,
        json!({"compile_version": nika_onboard::compile::COMPILE_WIRE_VERSION,
            "engine": env!("CARGO_PKG_VERSION"), "intent_sha256": intent_sha256(INTENT),
            "declined": [judged.clone()]})
    );
    for extra in [
        vec!["--answer", ANSWER],
        vec!["--answer", ANSWER, "--fresh"],
    ] {
        // The sketch and its fills of round 1 again: the same bytes, never asked of the judge.
        let (out, doc, bodies) = compile(room.path(), &authored("save"), &extra);
        assert_eq!(out.status.code(), Some(2), "{extra:?}: {doc}");
        assert_eq!(doc["status"], "incomplete", "{doc}");
        let shas: Vec<Option<String>> = bodies.iter().map(judged_sha).collect();
        assert_eq!(
            shas,
            [None, None],
            "the sketch and its fills, no judge question"
        );
        let candidate = doc["candidate"].as_str().expect("the held preview");
        assert_eq!(
            nika_event::source_id::sha256_hex(candidate.as_bytes()),
            held,
            "the held bytes, authored again"
        );
        assert_eq!(applied(&doc, "verify_held"), [HELD], "{doc}");
        let attempts = doc["provenance"]["decision"]["semantic_verification"]
            .as_array()
            .expect("the attempts");
        assert_eq!(attempts.len(), 1, "{doc}");
        let carried = &attempts[0];
        assert_eq!(carried["carried"], true);
        assert_eq!(carried["same_bytes_as"], Value::Null);
        assert_eq!(carried["questions"], json!([]));
        for count in ["attempted", "returned", "consumed"] {
            assert_eq!(carried[count], 0, "{count}");
        }
        for field in [
            "judge",
            "candidate_sha256",
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
        ] {
            assert_eq!(carried[field], judged[field], "{field}");
        }
        let route = &doc["provenance"]["decision"]["route"];
        let steps = (route.as_array().into_iter().flatten()).filter_map(Value::as_str);
        assert_eq!(
            steps
                .filter(|step| step.starts_with("verify:"))
                .collect::<Vec<_>>(),
            [
                "verify: same bytes, rejected in an earlier round",
                "verify: not ready, candidate held"
            ],
            "{doc}"
        );
        assert_eq!(doc["written"], Value::Null, "{doc}");
        assert_eq!(doc.get("declined_record_error"), None, "{doc}");
        assert_eq!(kept_declined(room.path()), kept, "kept once");
    }
    assert!(
        !room.path().join("out").exists(),
        "nothing was written or run"
    );
}
