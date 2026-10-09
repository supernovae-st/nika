// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Verdicts no repair starts from or ends on (R4 A11, R6): a judge that abstains, a
//! whole-request verdict its parts disagree with, a judge whose call fails, and a defect set a
//! COLD repair already answered. A whole request the judge answered and did not carry is never
//! replayed to it: its record is dropped; a judge that answered nothing keeps the record, and
//! its answer round asks the whole request again on the same bytes.
use super::*;

/// Why a doubt the parts could not decide stays undecided in a compile that ran no trial.
const NO_TRIAL: &str = "no trial run of these exact bytes exists in this compile";

/// The route step of a not-READY verdict whose whole request the judge doubted.
const UNREPLAYABLE: &str = "verify: doubted, not replayable";

/// The finding a contested whole request leaves when `reasons` kept it undecided.
fn contested_whole(doubt: &str, reasons: &str) -> String {
    format!(
        "The judge did not accept the request as carried ({doubt}) and located no defect a repair could start from; the same judge asked again decides nothing ({reasons}). Nothing is READY on it. Next: a correction of the request, or another verifier."
    )
}

/// A judge that abstains settles nothing (R4 A11): no defect to repair from, no repair call,
/// the request INCOMPLETE naming what the judge could not settle. Its abstention on the whole
/// request is a doubt that asks each part alone and the extra-operation question, each abstained
/// on too: every part, whether a task does more than asked, and the whole request stay unknown,
/// never contested, and no question reaches the author. An abstention never carries the
/// request, so the same bytes are never replayed to the same judge: the record is dropped.
#[tokio::test]
async fn an_abstaining_judge_keeps_the_request_incomplete() {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![plan(SUM).to_string(), program(&sum)]);
    let judge = Judging::new(&seat, abstain);
    let out = compiled(&judge).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform"]);
    assert_eq!(seat.calls(), 2);
    assert_eq!(
        judged(&out),
        [
            "judge_clause",
            "judge_request",
            "judge_part",
            "judge_part",
            "judge_part",
            "judge_extra"
        ]
    );
    assert_eq!(judge.judged().len(), 6);
    let attempt = &attempts(&out)[0];
    assert_eq!(attempt["doubt"], json!(["none"]), "{attempt:#}");
    assert_eq!(attempt["defects"], json!([]), "{attempt:#}");
    assert_eq!(attempt["contested"], json!([]), "{attempt:#}");
    assert_eq!(attempt["unsettled"], json!([]), "{attempt:#}");
    // An abstention is not consumed: only the answers the verdict used count.
    assert_eq!(
        (&attempt["attempted"], &attempt["consumed"]),
        (&json!(6), &json!(0))
    );
    // The clause and the same part asked alone name one unknown, at its first place; the whole
    // request no answer decided comes last.
    let unknown = json!([
        SUM.0,
        "read ./data/input.csv",
        SUM.1,
        "whether any task does something the request does not ask (the judge made no choice)",
        intent(SUM)
    ]);
    assert_eq!(attempt["unknown"], unknown, "{attempt:#}");
    let told = verifier_said(&out);
    for part in unknown.as_array().unwrap() {
        let named = format!("could not settle `{}`", part.as_str().unwrap());
        assert!(told.iter().any(|m| m.contains(&named)), "{named}: {told:?}");
    }
    for refuted in ["does not carry", "did not accept"] {
        assert!(told.iter().all(|m| !m.contains(refuted)), "{told:?}");
    }
    assert!(out.provenance.plan.is_none(), "{out:#?}");
    assert_eq!(verify_route(&out), ["verify: not ready", UNREPLAYABLE]);
}

/// The verification attempts the decision records, in order.
fn attempts(out: &CompileOutcome) -> Vec<Value> {
    let decision = out.provenance.decision.as_ref().unwrap();
    decision["semantic_verification"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The verification steps of the route the decision records, in order.
fn verify_route(out: &CompileOutcome) -> Vec<String> {
    let decision = out.provenance.decision.as_ref().unwrap();
    (decision["route"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .filter(|step| step.starts_with("verify:"))
        .map(str::to_owned)
        .collect()
}

/// The verdict of a judge that doubts the whole request and then approves each part asked alone,
/// naming no task that does more than the request asks: a disagreement it locates no defect of.
fn doubting(asked: &Asked<'_>) -> String {
    if asked.offers("faithful") {
        return "unfaithful".to_owned();
    }
    approve(asked)
}

/// A COLD candidate whose whole-request verdict does not carry the request while every part,
/// asked alone, is carried and no task does more than it asks (R6): the same judge agreeing with
/// itself part by part decides nothing, and no trial run of these bytes exists. The request is
/// contested: never READY, never repaired from though a repair round is granted, the finding
/// naming the disagreement and its next action, never a defect, and its record dropped.
#[tokio::test]
async fn a_doubted_cold_candidate_whose_every_part_is_carried_is_contested_never_repaired() {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![plan(SUM).to_string(), program(&sum)]);
    let judge = Judging::new(&seat, doubting);
    let out = compiled_as(&judge, SUM, 1).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform"], "{out:#?}");
    assert_eq!(seat.calls(), 2);
    // The clause no law reads, the whole request, its three parts alone, the extra question.
    assert_eq!(
        judged(&out),
        [
            "judge_clause",
            "judge_request",
            "judge_part",
            "judge_part",
            "judge_part",
            "judge_extra"
        ]
    );
    // Each question over the whole request, a part of it alone or its tasks is told what a
    // request to author this workflow asks of its bytes, as the whole verdict is.
    let created = "This candidate is the workflow the request asks Nika to author.";
    let systems = judge.systems();
    assert_eq!(systems.len(), 6, "{systems:#?}");
    for (at, system) in systems.iter().enumerate().skip(1) {
        assert!(system.contains(created), "{at}: {system}");
    }
    let verified = attempts(&out);
    assert_eq!(verified.len(), 1, "{verified:#?}");
    let attempt = &verified[0];
    assert_eq!(attempt["doubt"], json!(["unfaithful"]), "{attempt:#}");
    assert_eq!(attempt["defects"], json!([]), "{attempt:#}");
    assert_eq!(attempt["unknown"], json!([]), "{attempt:#}");
    assert_eq!(attempt["contested"], json!([intent(SUM)]), "{attempt:#}");
    assert_eq!(attempt["unsettled"], json!([NO_TRIAL]), "{attempt:#}");
    assert_eq!(attempt["settled_by"], Value::Null, "{attempt:#}");
    assert_eq!(verify_route(&out), ["verify: not ready", UNREPLAYABLE]);
    // The candidate stays the preview; its record keeps the rejection, so no replay asks the
    // same judge again.
    assert!(out.candidate.is_some(), "{out:#?}");
    assert!(record_as_declined(&out), "{out:#?}");
    // Beside the core's own pending findings, the verifier names the disagreement, never a
    // defect nor an abstention.
    let told = verifier_said(&out);
    let contested: Vec<&String> = told.iter().filter(|m| m.starts_with("The judge")).collect();
    assert_eq!(
        contested,
        [&contested_whole("unfaithful", NO_TRIAL)],
        "{told:?}"
    );
    for refuted in ["does not carry", "could not settle"] {
        assert!(told.iter().all(|m| !m.contains(refuted)), "{told:?}");
    }
}

/// The COLD request of [`SUM`] under `repairs` repair rounds, its judge's calls failing before
/// any answer (the judge is the authoring provider): the outcome and the seat.
async fn unanswered(repairs: u32) -> (CompileOutcome, Scripted) {
    let sum = format!("{GENERATED} // 0");
    let failing = Scripted::failing(vec![plan(SUM).to_string(), program(&sum)], 2);
    let out = compiled_as(&failing, SUM, repairs).await;
    (out, failing)
}

/// The answer round of a COLD `record` for [`SUM`], judged by the approving double over a seat
/// that must not author: the outcome and the seat.
async fn answer_round(record: Value) -> (CompileOutcome, Scripted) {
    let seat = Scripted::new(vec![plan(SUM).to_string()]);
    let judge = Judging::new(&seat, approve);
    let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
    let policy = AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2));
    let request = CompileRequest::create(intent(SUM))
        .with_knowledge(observed)
        .with_plan(record)
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy);
    let out = compile_with_provider(&request, &judge).await.unwrap();
    (out, seat)
}

/// A COLD candidate's record decides whether an answer round may replay it (R6): one whose whole
/// request the judge doubted is dropped at the not-READY exit, so no answer round replays those
/// bytes to that judge; one whose judge answered nothing (its first call failed, and nothing was
/// asked after it) is kept, nothing doubted, and its answer round replays the same bytes with no
/// authoring call, judged again: the clause no law reads first, then the whole request.
#[tokio::test]
async fn a_doubted_cold_whole_is_never_replayed_and_a_failed_judge_keeps_its_record() {
    let sum = format!("{GENERATED} // 0");
    // The judge doubts and locates nothing: the record keeps that rejection, the route says why.
    let seat = Scripted::new(vec![plan(SUM).to_string(), program(&sum)]);
    let doubted = compiled_as(&Judging::new(&seat, doubting), SUM, 0).await;
    assert_eq!(doubted.status, CompileStatus::Incomplete, "{doubted:#?}");
    assert!(
        doubted.candidate.is_some(),
        "the preview stays: {doubted:#?}"
    );
    assert!(record_as_declined(&doubted), "{doubted:#?}");
    assert_eq!(verify_route(&doubted), ["verify: not ready", UNREPLAYABLE]);
    // The judge's first call fails: it stops the verdict, so the whole request is never asked;
    // the clause and the whole request stay unknown, and the record is kept.
    let (failed, failing) = unanswered(0).await;
    assert_eq!(failed.status, CompileStatus::Incomplete, "{failed:#?}");
    assert_eq!(failing.calls(), 3, "{failed:#?}");
    assert_eq!(judged(&failed), ["judge_clause"]);
    let attempt = &attempts(&failed)[0];
    assert_eq!(
        attempt["unknown"],
        json!([SUM.0, intent(SUM)]),
        "{attempt:#}"
    );
    assert_eq!(attempt["doubt"], json!([]), "{attempt:#}");
    let calls = (&attempt["attempted"], &attempt["returned"]);
    assert_eq!(calls, (&json!(1), &json!(0)), "{attempt:#}");
    let flags = (&attempt["stopped"], &attempt["declined"]);
    assert_eq!(flags, (&json!(true), &json!(false)), "{attempt:#}");
    assert_eq!(verify_route(&failed), ["verify: not ready"]);
    let record = failed.provenance.plan.clone().expect("the record is kept");
    assert_eq!(record["strategy"], "cold", "{record:#}");
    // Its answer round replays those very bytes, no plan, transform or repair call, and asks the
    // round's judge again, the clause no law reads first.
    let (answered, seat) = answer_round(record).await;
    assert_eq!(seat.calls(), 0, "{answered:#?}");
    assert_eq!(answered.candidate, failed.candidate, "the same bytes");
    assert_eq!(authored(&answered), Vec::<String>::new(), "{answered:#?}");
    let questions = attempts(&answered)[0]["questions"].clone();
    let asked: Vec<(&Value, &Value, &Value)> = (questions.as_array().into_iter().flatten())
        .map(|record| (&record["question"], &record["role"], &record["choice"]))
        .collect();
    let (clause, whole) = (json!("verify-clause-0"), json!("verify-request"));
    let (clause_role, whole_role) = (json!("judge_clause"), json!("judge_request"));
    let (carried, faithful) = (json!("carried"), json!("faithful"));
    let want = [
        (&clause, &clause_role, &carried),
        (&whole, &whole_role, &faithful),
    ];
    assert_eq!(asked, want, "{questions:#}");
    assert_eq!(answered.status, CompileStatus::Ready, "{answered:#?}");
}

/// The former RED witness P-COLD-WHOLE, now green (C3): a COLD record kept because its judge
/// answered nothing replays in its answer round with its whole request pending again: the
/// round asks the remainder the core names (`judge_clause`), then the whole request, and only
/// the whole request carried makes it READY, settled by that question.
#[tokio::test]
async fn a_kept_cold_record_whose_whole_request_was_never_judged_is_not_ready_unjudged() {
    let (failed, _) = unanswered(0).await;
    let record = failed.provenance.plan.clone().expect("the record is kept");
    let (answered, _) = answer_round(record).await;
    assert_eq!(
        judged(&answered),
        ["judge_clause", "judge_request"],
        "{answered:#?}"
    );
    let attempt = &attempts(&answered)[0];
    assert_eq!(
        attempt["settled_by"],
        json!("verify-request"),
        "{attempt:#}"
    );
    assert_eq!(answered.status, CompileStatus::Ready, "{answered:#?}");
}

/// Four sums of the shipped quantities the laws admit as proposed, each its own bytes: what a
/// seat answers at the opening and at each repair here.
const SUMS: [&str; 4] = [
    ".records | map(select(.status == \"shipped\") | .qty | tonumber) | add // 0",
    "reduce (.records[] | select(.status == \"shipped\") | .qty | tonumber) as $q (0; . + $q)",
    "[.records[] | select(.status == \"shipped\") | .qty | tonumber] | add // 0",
    "(.records | map(select(.status == \"shipped\") | .qty | tonumber) | add) // 0",
];

/// The part the cycling judge finds missing at each verification attempt (R4 A11): the read,
/// then the write, then the read again (A, B, A); every later attempt is faithful.
const CYCLE: [&str; 3] = ["read ./data/input.csv", SUM.1, "read ./data/input.csv"];

/// The task a judge here names for the part it finds missing: the one writing the result, the
/// one computing the sum, or the one reading the source.
fn culprit(part: &str) -> &'static str {
    if part == SUM.1 {
        "write_output"
    } else if part == SUM.0 {
        "compute"
    } else {
        "read_source"
    }
}

/// The verdict of a judge whose defect sets cycle through [`CYCLE`]: the whole request unfaithful
/// while a cycle step is left, the part of the step missing and the task it names, every other
/// question approved.
fn cycling(asked: &Asked<'_>) -> String {
    let missing = CYCLE.get(asked.requests.wrapping_sub(1)).copied();
    if asked.offers("faithful") {
        let verdict = if missing.is_some() {
            "unfaithful"
        } else {
            "faithful"
        };
        return verdict.to_owned();
    }
    match (missing, asked.clause) {
        (Some(part), Some(clause)) if part == clause && asked.part() => "missing".to_owned(),
        (Some(part), Some(clause)) if part == clause && asked.pointer() => {
            format!("task-{}", culprit(part))
        }
        _ => approve(asked),
    }
}

/// Under no repair count, a defect set already repaired from is no progress whenever it was seen
/// (R4 A11): the judge names the read (A), then the write (B), then the read again; the third
/// set repeats the first, so the repairs end there, never a third repair from it, and the
/// request stays INCOMPLETE naming the part, the judge's reason and the two repairs made. Each
/// attempt records its own defect set and the task its judge named; the last verdict doubted the
/// whole request, so its record is dropped.
#[tokio::test]
async fn a_defect_set_already_repaired_from_ends_the_repairs_whenever_it_was_seen() {
    // A plan and its program for the opening and each repair, a fourth pair past the stop: each
    // program another admitted sum, so each repair writes bytes no judge has read (the same bytes
    // again would repeat the earlier verdict with no call).
    let answers = SUMS.iter().map(|sum| [plan(SUM).to_string(), program(sum)]);
    let seat = Scripted::new(answers.flatten().collect());
    let judge = Judging::new(&seat, cycling);
    let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
    let policy = AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2))
        .with_unbounded_repairs();
    let request = CompileRequest::create(intent(SUM))
        .with_knowledge(observed)
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy);
    let out = compile_with_provider(&request, &judge).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(
        authored(&out),
        [
            "plan",
            "transform",
            "repair",
            "transform",
            "repair",
            "transform"
        ],
        "{out:#?}"
    );
    assert_eq!(seat.calls(), 6);
    let sets: Vec<(Value, Value, Value)> = (attempts(&out).iter())
        .map(|a| {
            (
                a["attempt"].clone(),
                a["defects"].clone(),
                a["notes"].clone(),
            )
        })
        .collect();
    let want: Vec<(Value, Value, Value)> = (CYCLE.iter().enumerate())
        .map(|(at, part)| {
            let note = format!("the judge points to the task {}", culprit(part));
            let notes = json!([{"defect": part, "note": note}]);
            (json!(at), json!([part]), notes)
        })
        .collect();
    assert_eq!(sets, want);
    assert_eq!(
        verify_route(&out),
        [
            "verify: repair 1",
            "verify: repair 2",
            "verify: no progress",
            "verify: not ready",
            UNREPLAYABLE
        ]
    );
    assert!(record_as_declined(&out), "{out:#?}");
    let told = verifier_said(&out);
    let named = format!(
        "it does not carry « {} (the judge points to the task read_source) ». 2 repair(s) from that defect did not settle it",
        CYCLE[0]
    );
    assert!(told.iter().any(|m| m.contains(&named)), "{told:?}");
}

/// The verdict of a judge that names one defect set in two orders: at the first attempt the
/// clause no law reads (asked before the whole request) and then the read among the parts, from
/// the second on the read and then that clause, both among the parts; each with the task it
/// names, every other question approved.
fn reordering(asked: &Asked<'_>) -> String {
    if asked.offers("faithful") {
        return "unfaithful".to_owned();
    }
    if asked.pointer() {
        return format!("task-{}", culprit(asked.clause.unwrap_or_default()));
    }
    let missing = if asked.part() {
        asked.clause == Some(CYCLE[0]) || (asked.clause == Some(SUM.0) && asked.requests > 1)
    } else {
        asked.clause == Some(SUM.0) && asked.requests == 0
    };
    if missing {
        "missing".to_owned()
    } else {
        approve(asked)
    }
}

/// Under no repair count, the defect set a COLD repair already answered is no progress in any
/// order (R4 A11): the first attempt names the computation then the read, the second the read
/// then the computation; compared as a set, the second repeats the first, so the repairs end
/// after one, each attempt recording its own order.
#[tokio::test]
async fn a_defect_set_already_repaired_from_in_another_order_ends_the_repairs() {
    // Each repair writes other bytes (another admitted sum), which the judge reads afresh.
    let answers = SUMS[..3]
        .iter()
        .map(|sum| [plan(SUM).to_string(), program(sum)]);
    let seat = Scripted::new(answers.flatten().collect());
    let judge = Judging::new(&seat, reordering);
    let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
    let policy = AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2))
        .with_unbounded_repairs();
    let request = CompileRequest::create(intent(SUM))
        .with_knowledge(observed)
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy);
    let out = compile_with_provider(&request, &judge).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let calls = ["plan", "transform", "repair", "transform"];
    assert_eq!(authored(&out), calls, "{out:#?}");
    assert_eq!(seat.calls(), 4);
    let sets: Vec<Value> = (attempts(&out).iter())
        .map(|attempt| attempt["defects"].clone())
        .collect();
    assert_eq!(sets, [json!([SUM.0, CYCLE[0]]), json!([CYCLE[0], SUM.0])]);
    let steps = [
        "verify: repair 1",
        "verify: no progress",
        "verify: not ready",
        UNREPLAYABLE,
    ];
    assert_eq!(verify_route(&out), steps);
    let told = verifier_said(&out);
    for (part, task) in [(CYCLE[0], "read_source"), (SUM.0, "compute")] {
        let named = format!(
            "it does not carry « {part} (the judge points to the task {task}) ». 1 repair(s) from that defect did not settle it"
        );
        assert!(told.iter().any(|m| m.contains(&named)), "{named}: {told:?}");
    }
}

/// The verdict of a judge that carries the whole request at every attempt, finds the clause no
/// law reads (the sum) missing at its first attempt only, naming the task computing it, and
/// carries everything after.
fn relenting(asked: &Asked<'_>) -> String {
    let first = asked.requests == 0 && asked.clause == Some(SUM.0);
    if first && asked.spanned && !asked.pointer() {
        return "missing".to_owned();
    }
    if first && asked.pointer() {
        return "task-compute".to_owned();
    }
    approve(asked)
}

/// RED witness (P-CLAUSE-DECLINED-REJUDGED): bytes whose pending clause the judge declined are
/// never asked of it again (R6), even when that verdict carried the whole request: a COLD
/// repair that writes the same bytes repeats the verdict with no call, so nothing outvotes it
/// and nothing is READY. `judged_before` (verify.rs) matches only an earlier attempt whose
/// `settled_by` is null, and a faithful whole request sets it beside the declined clause: the
/// same judge is asked the clause again on the same digest, carries it, and the bytes it
/// declined are READY.
#[tokio::test]
async fn a_clause_declined_beside_a_faithful_request_is_never_outvoted_on_the_same_bytes() {
    let sum = format!("{GENERATED} // 0");
    let answers = (0..2).flat_map(|_| [plan(SUM).to_string(), program(&sum)]);
    let seat = Scripted::new(answers.collect());
    let judge = Judging::new(&seat, relenting);
    let out = compiled_as(&judge, SUM, 1).await;
    let verified = attempts(&out);
    assert_eq!(verified.len(), 2, "{verified:#?}");
    let digests = (
        &verified[0]["candidate_sha256"],
        &verified[1]["candidate_sha256"],
    );
    assert_eq!(digests.0, digests.1, "the repair wrote the same bytes");
    assert_ne!(out.status, CompileStatus::Ready, "{verified:#?}");
    assert_eq!(verified[1]["same_bytes_as"], json!(0), "{verified:#?}");
    let once = "the clause, its task, the whole request: once";
    assert_eq!(judge.judged().len(), 3, "{once}");
}
