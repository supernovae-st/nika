// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What a held candidate shows, and what a prohibition's task question may conclude (R4 A11,
//! R6), end to end through the sketch door. The machine document every transport prints
//! (`outcome_document`: `nika compile --json`, Serve `POST /v1/compile`) shows a held candidate
//! as it is, with nothing masked: the bytes the judge declined, INCOMPLETE, their Check preview
//! and the `verify_held` finding beside the verifier's own; a client decides what it displays. A
//! pure prohibition (« need no time-zone conversion ») asks no operation of its own: its task
//! question never offers or describes an operation no task performs, so a prohibition the judge
//! finds missing is a defect only through a task the judge names.
use super::observed::{JUDGE, Judging};
use super::*;
use nika_compile_cognition::{Cognition, compile_with_cognition, decide::DecisionSeat};

/// What the task question says of an operation no task performs, when it offers it.
const OMITTED_TOLD: &str = "omitted: the clause asks an operation of its own (a read, a filter, a computation, a condition, a write) that no task performs.";

/// The finding of a part the judge left without an admitted choice.
fn unsettled(part: &str) -> String {
    format!(
        "The judge could not settle `{part}` against the candidate (it abstained, answered outside its options, or its call failed); nothing is READY on it. Next: a judge that answers, or a restatement the deterministic reader reads."
    )
}

/// `intent` through the sketch door under `repairs` repair rounds, its sketch and fill answered
/// by the author (`sketch`, `fill`), judged by the decision seat `judge`: the outcome and the
/// author's calls.
async fn judged_by(
    intent: &str,
    (sketch, fill): (String, String),
    judge: &Judging,
    repairs: u32,
) -> (CompileOutcome, u32) {
    let author = Rotating::new(vec![sketch, fill]);
    let cognition = Cognition {
        provider: Some(&author),
        seat: Some(judge as &dyn DecisionSeat),
    };
    let policy = allowing(NativeMode::Sketch, repairs);
    let request = CompileRequest::create(intent).with_authoring_policy(policy);
    let out = compile_with_cognition(&request, cognition).await.unwrap();
    (out, author.calls.load(Ordering::SeqCst))
}

/// The task question the judge was asked, by id.
fn pointer(judge: &Judging, id: &str) -> nika_compile_cognition::decide::ChoiceQuestion {
    let asked = judge.asked().into_iter().find(|question| question.id == id);
    assert!(asked.is_some(), "no question {id}: {:?}", judge.ids());
    asked.unwrap()
}

/// A judge that answers `omitted` to the task question of the field case's prohibition chose an
/// option it was never offered: no choice is admitted, the prohibition stays unknown, never a
/// defect and never repaired from though a round is left, and the doubted request is held. The
/// question offered every task and no task failing it, and its words never describe an
/// operation the prohibition would lack.
#[tokio::test]
async fn a_prohibition_judged_missing_is_never_a_defect_without_a_task_named() {
    let judge = Judging::new(&[
        ("verify-request", "unfaithful"),
        ("verify-part-0", "carried"),
        ("verify-part-1", "missing"),
        ("verify-point-1", "omitted"),
        ("verify-part-2", "carried"),
        ("verify-extra", "only_requested"),
    ]);
    let author = (events(), filled("shape_events", AS_THEY_ARE));
    let (out, authored) = judged_by(FIELD, author, &judge, 2).await;
    assert_held(&out);
    assert_eq!(authored, 2, "no repair: {out:#?}");
    assert_eq!(judge.left(), 0, "{out:#?}");
    let asked = pointer(&judge, "verify-point-1");
    let tasks = ["task-read_events", "task-shape_events", "task-write_events"];
    let mut offered: Vec<String> = tasks.map(str::to_owned).to_vec();
    offered.extend(["no_task", "none"].map(str::to_owned));
    assert_eq!(asked.keys(), offered);
    assert!(!asked.instructions.contains(OMITTED_TOLD), "{asked:#?}");
    assert_eq!(asked.state["clause"], json!({"text": LOCAL_TIMES}));
    let attempt = &verification(&out)[0];
    assert_eq!(
        attempt["judge"],
        json!({"seat": JUDGE, "kind": "decision_seat"})
    );
    assert_eq!(attempt["defects"], json!([]), "{attempt:#}");
    assert_eq!(attempt["notes"], json!([]), "{attempt:#}");
    assert_eq!(attempt["unknown"], json!([LOCAL_TIMES]), "{attempt:#}");
    assert_eq!(attempt["contested"], json!([FIELD]), "{attempt:#}");
    assert_eq!(attempt["unsettled"], json!([NO_TRIAL]), "{attempt:#}");
    let record = question(&out, "verify-point-1");
    let refused = (&record["choice"], &record["error"]);
    let error = json!("seat chose `omitted`, outside the offered options");
    assert_eq!(refused, (&Value::Null, &error), "{record:#}");
    // Six questions asked, all answered; the unoffered one is not consumed.
    let counts = (
        &attempt["attempted"],
        &attempt["returned"],
        &attempt["consumed"],
    );
    assert_eq!(counts, (&json!(6), &json!(6), &json!(5)), "{attempt:#}");
    let told = findings(&out, "semantic_verification");
    let want = [unsettled(LOCAL_TIMES), contested_whole(NO_TRIAL)];
    assert_eq!(told, want, "{told:?}");
}

/// The control: a selection (« keep only the open tickets ») may ask an operation of its own. Its
/// task question offers `omitted` after every task and describes it; that answer is a defect
/// with its reason, and the candidate the door cannot reopen past its round is withdrawn naming
/// it.
#[tokio::test]
async fn a_selection_judged_missing_may_lack_an_operation_of_its_own() {
    let judge = Judging::new(&[
        ("verify-request", "unfaithful"),
        ("verify-part-0", "carried"),
        ("verify-part-1", "missing"),
        ("verify-point-1", "omitted"),
    ]);
    let author = (sketch(), fills("fromjson"));
    let (out, authored) = judged_by(TICKETS, author, &judge, 1).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(authored, 2, "{out:#?}");
    assert_eq!(judge.left(), 0, "{out:#?}");
    let asked = pointer(&judge, "verify-point-1");
    let tasks = ["task-keep_open", "task-read_tickets", "task-write_open"];
    let mut offered: Vec<String> = tasks.map(str::to_owned).to_vec();
    offered.extend(["omitted", "no_task", "none"].map(str::to_owned));
    assert_eq!(asked.keys(), offered);
    assert!(asked.instructions.contains(OMITTED_TOLD), "{asked:#?}");
    let attempt = &verification(&out)[0];
    let omitted = "the judge finds no task performing it";
    assert_eq!(attempt["defects"], json!([OPEN_ONLY]), "{attempt:#}");
    let noted = json!([{"defect": OPEN_ONLY, "note": omitted}]);
    assert_eq!(attempt["notes"], noted, "{attempt:#}");
    let told = findings(&out, "semantic_verification");
    assert_eq!(told, [not_carried(OPEN_ONLY, omitted, 0)], "{told:?}");
}

/// The machine document of a held candidate (`outcome_document`, the one projection the CLI and
/// Serve print) masks nothing and offers nothing: status incomplete, the very bytes the judge
/// declined and their Check preview, no question, no boundary, no replay record, and the
/// verifier's findings followed by the `verify_held` finding, as typed; the decision keeps the
/// verdict bound to those bytes, with how it declined them.
#[tokio::test]
async fn the_wire_document_of_a_held_candidate_shows_it_unmasked_and_never_offered() {
    let mut replies = ranked(HIGHEST_FIRST);
    for choice in [
        "unfaithful",
        "carried",
        "carried",
        "carried",
        "only_requested",
    ] {
        replies.push(json!({"choice": choice}).to_string());
    }
    let provider = Rotating::new(replies);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(NativeMode::Sketch));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_held(&out);
    let document = nika_compile::outcome_document(&out);
    let candidate = out.candidate.clone().unwrap();
    assert_eq!(document["status"], json!("incomplete"), "{document:#}");
    assert_eq!(document["candidate"], json!(candidate), "{document:#}");
    assert_eq!(document["check_preview"]["scope"], json!("sourceOnly"));
    let report = serde_json::to_value(&out.check_preview.as_ref().unwrap().report).unwrap();
    assert_eq!(document["check_preview"]["report"], report, "{document:#}");
    assert_eq!(document["questions"], json!([]), "{document:#}");
    assert_eq!(document["requested_boundary"], Value::Null, "{document:#}");
    assert!(document["provenance"].get("plan").is_none(), "{document:#}");
    let typed: Vec<(&str, &str)> = (out.diagnostics.iter())
        .filter(|d| d.target == "semantic_verification" || d.target == "verify_held")
        .map(|d| (d.target.as_str(), d.message.as_str()))
        .collect();
    let contested = contested_whole(NO_TRIAL);
    let want = [
        ("semantic_verification", contested.as_str()),
        ("verify_held", HELD),
    ];
    assert_eq!(typed, want, "{out:#?}");
    let shown: Vec<Value> = (document["diagnostics"].as_array().into_iter().flatten())
        .filter(|d| d["target"] == "semantic_verification" || d["target"] == "verify_held")
        .cloned()
        .collect();
    let printed = json!([
        {"kind": "unknown", "target": "semantic_verification", "message": contested},
        {"kind": "applied", "target": "verify_held", "message": HELD},
    ]);
    assert_eq!(json!(shown), printed, "{document:#}");
    let attempt = &document["provenance"]["decision"]["semantic_verification"][0];
    let sha = nika_compile::surface::sha256(&candidate);
    assert_eq!(attempt["candidate_sha256"], json!(sha), "{attempt:#}");
    let declined = (
        &attempt["declined"],
        &attempt["rejected"],
        &attempt["stopped"],
    );
    assert_eq!(declined, (&json!(true), &json!(true), &json!(false)));
    assert_eq!(attempt["request"], json!(INTENT), "{attempt:#}");
    assert_eq!(attempt["same_bytes_as"], Value::Null, "{attempt:#}");
    let route = document["provenance"]["decision"]["route"].to_string();
    assert!(
        route.contains("verify: not ready, candidate held"),
        "{route}"
    );
}
