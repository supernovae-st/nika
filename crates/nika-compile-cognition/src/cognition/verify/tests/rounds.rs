// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The COLD and WARM rounds under a selected judge (R4 A11, R6): a COLD repair loop with no
//! repair count goes on only while each defect set names a part never named before or narrows
//! the last one, in whatever order the judge names it; a repair that yields the same bytes asks
//! the judge nothing and ends the repairs; a round the judge doubted and left not READY keeps no
//! replayable record and is held, so no replay asks the same judge again on the same bytes, while
//! a round it answered nothing to keeps it; a not-READY round keeps the clauses the judge carried
//! settled.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};

use crate::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionError, DecisionSeat};
use crate::{
    AuthoringPolicy, Cognition, CompileOutcome, CompileRequest, CompileStatus, HotPolicy,
    NoProvider,
};

/// The live COLD request (B16): a read, a computation no law of the core reads, a write.
const READ: &str = "read ./data/input.csv";
const SUM: &str = "sum qty over the rows where status is shipped";
const WRITE: &str = "write the sum to ./out/result.json";
/// The program the transform seat writes for [`SUM`]: the number 0 on a source with no row.
const GENERATED: &str =
    ".records | map(select(.status == \"shipped\") | .qty | tonumber) | add // 0";
/// The selected judge's name.
const JUDGE: &str = "mock/typed-judge";

fn intent() -> String {
    format!("{READ}, {SUM}, {WRITE}")
}

/// The plan the author proposes: a computation the typed stages do not state.
fn plan() -> String {
    json!({
        "steps": [
            {"op": "read", "detail": "./data/input.csv", "evidence": READ},
            {"op": "compute", "detail": SUM, "evidence": SUM, "computation": {"present": false}}
        ],
        "effects": [{"verb": "write", "target": "./out/result.json", "policy": "automatic",
            "evidence": WRITE}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": format!("{READ},"), "role": "operation"},
            {"text": format!("{SUM},"), "role": "operation"},
            {"text": WRITE, "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    })
    .to_string()
}

/// The same computation spelled otherwise: a repair's program, so its candidate's bytes differ.
const RESPELLED: &str =
    "[.records[] | select(.status == \"shipped\") | .qty | tonumber] | add // 0";
/// The same computation spelled a third way.
const RESPELLED_AGAIN: &str =
    ".records | map(select(.status == \"shipped\")) | map(.qty | tonumber) | add // 0";

/// The transform seat's program for [`SUM`], with its own example.
fn program() -> String {
    spelled(GENERATED)
}

/// The transform seat's program `jq` for [`SUM`], with its own example.
fn spelled(jq: &str) -> String {
    let example = json!([
        {"id": "a1", "status": "shipped", "qty": "40"},
        {"id": "a2", "status": "pending", "qty": "15"}
    ]);
    json!({"jq": jq, "columns_read": ["status", "qty"], "example_input": example,
        "expected_output": 40})
    .to_string()
}

/// The author: it answers each authoring call from its replies in order and keeps the role of
/// each; a judge question never reaches it (the selected seat judges).
struct Author {
    replies: Mutex<VecDeque<String>>,
    calls: Mutex<usize>,
}

impl Author {
    fn new(replies: impl IntoIterator<Item = String>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().collect()),
            calls: Mutex::new(0),
        }
    }

    fn calls(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

impl ProviderInferDyn for Author {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        if let ResponseFormat::JsonSchema(schema) = &request.response_format {
            let choice = schema["properties"].get("choice");
            assert!(choice.is_none(), "the author became the judge");
        }
        *self.calls.lock().unwrap() += 1;
        let text = (self.replies.lock().unwrap().pop_front()).expect("an unexpected author call");
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(10, 10),
            StopReason::EndTurn,
        ))
    }
}

/// The selected judge: it answers each question its script names, in order, and keeps every
/// question id; a question the script does not expect panics.
struct Judging {
    script: Mutex<VecDeque<(&'static str, Result<&'static str, &'static str>)>>,
    asked: Mutex<Vec<String>>,
}

impl Judging {
    fn new(script: &[(&'static str, Result<&'static str, &'static str>)]) -> Self {
        Self {
            script: Mutex::new(script.iter().copied().collect()),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn left(&self) -> usize {
        self.script.lock().unwrap().len()
    }
}

impl DecisionSeat for Judging {
    fn name(&self) -> &str {
        JUDGE
    }

    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.id.clone());
            let (id, answer) = (self.script.lock().unwrap().pop_front())
                .unwrap_or_else(|| panic!("an unscripted question: {}", question.id));
            assert_eq!(question.id, id);
            answer
                .map(|key| ChoiceAnswer::new(key, JUDGE))
                .map_err(|why| DecisionError(why.to_owned()))
        })
    }
}

/// The COLD request under `policy`, authored by `author` and judged by `judge`.
async fn cold(policy: AuthoringPolicy, author: &Author, judge: &Judging) -> CompileOutcome {
    cold_carrying(policy, author, judge, Vec::new()).await
}

/// The COLD request under `policy`, carrying the verdicts `declined` that rejected bytes in
/// earlier rounds of the conversation, authored by `author` and judged by `judge`.
async fn cold_carrying(
    policy: AuthoringPolicy,
    author: &Author,
    judge: &Judging,
    declined: Vec<Value>,
) -> CompileOutcome {
    let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed",
        "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
    let request = CompileRequest::create(intent())
        .with_knowledge(observed)
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy)
        .with_declined(declined);
    let cognition = Cognition {
        provider: Some(author),
        seat: Some(judge),
    };
    crate::compile_with_cognition(&request, cognition)
        .await
        .unwrap()
}

/// The verification attempts the decision records, in order.
fn attempts(out: &CompileOutcome) -> Vec<Value> {
    let decision = out.provenance.decision.as_ref().unwrap();
    (decision["semantic_verification"].as_array())
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

/// The authoring policy for COLD with `repairs` repair rounds, or none.
fn policy(repairs: Option<u32>) -> AuthoringPolicy {
    let policy = AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2));
    match repairs {
        Some(repairs) => policy.with_repairs(repairs),
        None => policy.with_unbounded_repairs(),
    }
}

/// The verifier's findings of `out` (each starting « The judge »), in order.
fn judged_findings(out: &CompileOutcome) -> Vec<&str> {
    (out.diagnostics.iter())
        .filter(|d| d.target == "semantic_verification" && d.message.starts_with("The judge"))
        .map(|d| d.message.as_str())
        .collect()
}

/// The finding a defect the repairs did not settle leaves, after `repairs` repairs: the part, then
/// the judge's reason (no task performing the read, the computation's own task failing it).
fn unsettled_defect(part: &str, repairs: usize) -> String {
    let note = if part == SUM {
        "the judge points to the task compute"
    } else {
        "the judge finds no task performing it"
    };
    format!(
        "The judge compared the whole request with the candidate's bytes: it does not carry « {part} ({note}) ». {repairs} repair(s) from that defect did not settle it; nothing is READY. Next: a stronger authoring model, or a restatement of that part."
    )
}

/// What the judge names as failing each part found missing: no task performs the read; the
/// computation's own task does it differently.
const POINTED: [&str; 3] = ["omitted", "task-compute", "task-write_output"];

/// The defect sets the attempts of `out` recorded, each with its attempt number.
fn defect_sets(out: &CompileOutcome) -> Vec<(Value, Value)> {
    (attempts(out).iter())
        .map(|attempt| (attempt["attempt"].clone(), attempt["defects"].clone()))
        .collect()
}

/// One COLD verdict naming, of the request's three parts, those at `missing` (each with the
/// reason [`POINTED`] gives it), its pending computation clause carried.
fn naming(missing: &[usize]) -> Vec<(&'static str, Result<&'static str, &'static str>)> {
    let mut script = vec![
        ("verify-clause-0", Ok("carried")),
        ("verify-request", Ok("unfaithful")),
    ];
    let ids = [
        ("verify-part-0", "verify-point-0"),
        ("verify-part-1", "verify-point-1"),
        ("verify-part-2", "verify-point-2"),
    ];
    for (k, (part, point)) in ids.into_iter().enumerate() {
        if missing.contains(&k) {
            script.extend([(part, Ok("missing")), (point, Ok(POINTED[k]))]);
        } else {
            script.push((part, Ok("carried")));
        }
    }
    script
}

/// Under no repair count, a defect set already repaired from is no progress in whatever order
/// the judge names it (R4 A11): the first attempt names the read then the computation (parts
/// asked in the request's order), the second, over a repair whose program is spelled otherwise
/// (other bytes), the computation then the read (its clause asked before the whole request). The
/// second set is the first one, so the repairs end there with no second repair, the request
/// INCOMPLETE; and the judge that doubted it is never asked again on these bytes: the record is
/// not replayable and the candidate is held.
#[tokio::test]
async fn a_defect_set_met_again_in_another_order_ends_the_repairs() {
    let author = Author::new([plan(), program(), plan(), spelled(RESPELLED)]);
    let judge = Judging::new(&[
        ("verify-clause-0", Ok("carried")),
        ("verify-request", Ok("unfaithful")),
        ("verify-part-0", Ok("missing")),
        ("verify-point-0", Ok("omitted")),
        ("verify-part-1", Ok("missing")),
        ("verify-point-1", Ok("task-compute")),
        ("verify-part-2", Ok("carried")),
        ("verify-clause-0", Ok("missing")),
        ("verify-clause-0-point", Ok("task-compute")),
        ("verify-request", Ok("unfaithful")),
        ("verify-part-0", Ok("missing")),
        ("verify-point-0", Ok("omitted")),
        ("verify-part-1", Ok("carried")),
        ("verify-part-2", Ok("carried")),
    ]);
    let out = cold(policy(None), &author, &judge).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(judge.left(), 0, "{:?}", judge.asked.lock().unwrap());
    assert_eq!(
        author.calls(),
        4,
        "the opening and one repair, each with its program"
    );
    let named = [
        (json!(0), json!([READ, SUM])),
        (json!(1), json!([SUM, READ])),
    ];
    assert_eq!(defect_sets(&out), named);
    let shas: Vec<Value> = (attempts(&out).iter())
        .map(|attempt| attempt["candidate_sha256"].clone())
        .collect();
    assert_ne!(shas[0], shas[1], "the repair's bytes differ");
    let steps = [
        "verify: repair 1",
        "verify: no progress",
        "verify: not ready",
        "verify: doubted, not replayable",
    ];
    assert_eq!(verify_route(&out), steps);
    assert_eq!(out.provenance.plan, None);
    // Beside the core's own pending findings, the verifier names each defect with its note.
    let defects = [unsettled_defect(SUM, 1), unsettled_defect(READ, 1)];
    assert_eq!(judged_findings(&out), defects);
}

/// A repair that yields the very bytes the judge already declined asks it nothing (R6): the
/// attempt repeats the earlier verdict with no call, names the attempt it repeats, and the
/// repairs end there as no progress; the judge never answers the same bytes twice.
#[tokio::test]
async fn a_repair_yielding_the_same_bytes_asks_nothing_and_ends_the_repairs() {
    let author = Author::new([plan(), program(), plan(), program()]);
    let judge = Judging::new(&naming(&[0, 1]));
    let out = cold(policy(None), &author, &judge).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!((author.calls(), judge.left()), (4, 0));
    assert_eq!(judge.asked.lock().unwrap().len(), 7, "one verdict asked");
    let recorded = attempts(&out);
    let repeated = &recorded[1];
    let fields = [
        "attempt",
        "attempted",
        "questions",
        "same_bytes_as",
        "defects",
        "declined",
        "rejected",
    ]
    .map(|key| repeated[key].clone());
    let expected = [
        json!(1),
        json!(0),
        json!([]),
        json!(0),
        json!([READ, SUM]),
        json!(true),
        json!(true),
    ];
    assert_eq!(fields, expected);
    assert_eq!(
        repeated["candidate_sha256"],
        recorded[0]["candidate_sha256"]
    );
    let steps = [
        "verify: repair 1",
        "verify: same bytes, earlier verdict stands",
        "verify: no progress",
        "verify: not ready",
        "verify: doubted, not replayable",
    ];
    assert_eq!(verify_route(&out), steps);
    let defects = [unsettled_defect(READ, 1), unsettled_defect(SUM, 1)];
    assert_eq!(judged_findings(&out), defects);
}

/// Under no repair count, progress is monotone (R4 A11): a defect set that narrows the last one
/// (the read and the computation, then the computation alone) is progress and repaired from; a
/// set then naming only a part already named, without narrowing the last set (the read again),
/// ends the repairs. Each repair spells the program otherwise, so every attempt reads new bytes.
#[tokio::test]
async fn a_narrowed_defect_set_is_repaired_from_and_a_part_named_again_ends_the_repairs() {
    let author = Author::new([
        plan(),
        program(),
        plan(),
        spelled(RESPELLED),
        plan(),
        spelled(RESPELLED_AGAIN),
    ]);
    let mut script = naming(&[0, 1]);
    script.extend(naming(&[1]));
    script.extend(naming(&[0]));
    let judge = Judging::new(&script);
    let out = cold(policy(None), &author, &judge).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!((author.calls(), judge.left()), (6, 0), "{out:#?}");
    let named = [
        (json!(0), json!([READ, SUM])),
        (json!(1), json!([SUM])),
        (json!(2), json!([READ])),
    ];
    assert_eq!(defect_sets(&out), named);
    let steps = [
        "verify: repair 1",
        "verify: repair 2",
        "verify: no progress",
        "verify: not ready",
        "verify: doubted, not replayable",
    ];
    assert_eq!(verify_route(&out), steps);
    assert_eq!(judged_findings(&out), [unsettled_defect(READ, 2)]);
}

/// What a candidate whose located defects no repair settled is held with.
const HELD_DEFECTS: &str = "The candidate was judged and not accepted: the parts named above stay missing. It is shown, never offered, and nothing was written; this verifier is not asked again on these bytes, in this compile or in a later round that carries this verdict. A correction of the request, another authoring model or another verifier can decide it.";

/// A COLD round whose bytes the selected judge rejected in an earlier round of the conversation
/// (the host carries that verdict) asks it nothing on them (R6): the same plan and program author
/// the same bytes, and the attempt repeats the earlier verdict with no call, recorded as carried.
/// Its located defects are what a repair starts from (a repair is an authoring call, never a
/// judge call on those bytes): the repaired program's other bytes are judged, and carried by the
/// judge, READY. Without a repair the policy grants, the round ends not READY, held with the
/// defects named, and keeps no record a later round could replay to the same judge.
#[tokio::test]
async fn a_rejection_carried_from_an_earlier_round_seeds_a_repair() {
    let author = Author::new([plan(), program()]);
    let judge = Judging::new(&naming(&[0, 1]));
    let first = cold(policy(Some(0)), &author, &judge).await;
    assert_eq!(first.status, CompileStatus::Incomplete, "{first:#?}");
    let earlier = attempts(&first)[0].clone();
    let flags = ["rejected", "settled", "carried"].map(|key| earlier[key].clone());
    assert_eq!(flags, [json!(true), json!(false), json!(false)]);
    // The repair's program is spelled otherwise: other bytes, judged and carried.
    let author = Author::new([plan(), program(), plan(), spelled(RESPELLED)]);
    let judge = Judging::new(&[
        ("verify-clause-0", Ok("carried")),
        ("verify-request", Ok("faithful")),
    ]);
    let out = cold_carrying(policy(Some(1)), &author, &judge, vec![earlier.clone()]).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        author.calls(),
        4,
        "the plan and its program, one repair and its program"
    );
    assert_eq!(judge.left(), 0, "{:?}", judge.asked.lock().unwrap());
    let recorded = attempts(&out);
    assert_eq!(recorded.len(), 2, "{recorded:#?}");
    let fields = [
        "attempt",
        "attempted",
        "questions",
        "carried",
        "same_bytes_as",
        "defects",
        "notes",
        "judge",
        "candidate_sha256",
        "rejected",
    ]
    .map(|key| recorded[0][key].clone());
    let expected = [
        json!(0),
        json!(0),
        json!([]),
        json!(true),
        Value::Null,
        json!([READ, SUM]),
        earlier["notes"].clone(),
        json!({"seat": JUDGE, "kind": "decision_seat"}),
        earlier["candidate_sha256"].clone(),
        json!(true),
    ];
    assert_eq!(fields, expected);
    assert_ne!(recorded[1]["candidate_sha256"], earlier["candidate_sha256"]);
    assert_eq!(recorded[1]["settled_by"], "verify-request");
    let steps = [
        "verify: same bytes, rejected in an earlier round",
        "verify: repair 1",
        "verify: judged (decision_seat)",
    ];
    assert_eq!(verify_route(&out), steps);
    // No repair granted: the carried verdict holds the same bytes, with no call.
    let author = Author::new([plan(), program()]);
    let silent = Judging::new(&[]);
    let out = cold_carrying(policy(Some(0)), &author, &silent, vec![earlier.clone()]).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(author.calls(), 2, "the plan and its program, no repair");
    assert!(silent.asked.lock().unwrap().is_empty(), "no call");
    let sha = crate::cognition::knowledge::sha256(out.candidate.as_deref().unwrap());
    assert_eq!(earlier["candidate_sha256"], json!(sha), "the same bytes");
    let steps = [
        "verify: same bytes, rejected in an earlier round",
        "verify: not ready",
        "verify: doubted, not replayable",
    ];
    assert_eq!(verify_route(&out), steps);
    assert_eq!(out.provenance.plan, None);
    let defects = [unsettled_defect(READ, 0), unsettled_defect(SUM, 0)];
    assert_eq!(judged_findings(&out), defects);
    assert_eq!(findings(&out).1, [HELD_DEFECTS]);
}

/// Under no repair count, a set naming a part never named before is progress (the read, then
/// the computation); the read named again after it is not (A, B, A ends).
#[tokio::test]
async fn a_new_part_is_progress_and_a_part_named_again_after_another_is_not() {
    let author = Author::new([
        plan(),
        program(),
        plan(),
        spelled(RESPELLED),
        plan(),
        spelled(RESPELLED_AGAIN),
    ]);
    let mut script = naming(&[0]);
    script.extend(naming(&[1]));
    script.extend(naming(&[0]));
    let judge = Judging::new(&script);
    let out = cold(policy(None), &author, &judge).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!((author.calls(), judge.left()), (6, 0), "{out:#?}");
    let named = [
        (json!(0), json!([READ])),
        (json!(1), json!([SUM])),
        (json!(2), json!([READ])),
    ];
    assert_eq!(defect_sets(&out), named);
    let steps = [
        "verify: repair 1",
        "verify: repair 2",
        "verify: no progress",
        "verify: not ready",
        "verify: doubted, not replayable",
    ];
    assert_eq!(verify_route(&out), steps);
}

/// The core's finding on a clause still pending on `candidate`, as `why` the core names it.
fn pending(clause: &str, why: &str, candidate: &str) -> String {
    let sha = crate::cognition::knowledge::sha256(candidate);
    format!(
        "The request states `{clause}` and {why}: no law reads from candidate {} that it carries it, and no admitted judgment made in this compile settles it. Nothing is READY on a pending clause: it stays INCOMPLETE until an admitted judgment of these bytes against the whole request carries it.",
        &sha[..12]
    )
}

/// Why the core names the computation clause pending: no law reads what its task carries.
const UNREAD: &str = "a task carries words no law reads";
/// Why the core names the whole request pending: no element of the plan names it whole.
const UNNAMED: &str = "no element of the plan names it";

/// The semantic findings of `out` and its `verify_held` finding, each in order.
fn findings(out: &CompileOutcome) -> (Vec<&str>, Vec<&str>) {
    let of = |target: &str| -> Vec<&str> {
        (out.diagnostics.iter())
            .filter(|d| d.target == target)
            .map(|d| d.message.as_str())
            .collect()
    };
    (of("semantic_verification"), of("verify_held"))
}

/// What the verifier names of a clause it could not settle.
fn unknown(clause: &str) -> String {
    format!(
        "The judge could not settle `{clause}` against the candidate (it abstained, answered outside its options, or its call failed); nothing is READY on it. Next: a judge that answers, or a restatement the deterministic reader reads."
    )
}

/// Why a verification left the rest of its questions unasked.
const STOPPED: &str = "The verification stopped at a judge call that got no answer (refused by the call bound, or failed: the receipt says which); nothing after it was asked of that judge. Next: another round, or a larger call bound.";
/// What a candidate judged and rejected with no defect located is held with.
const HELD: &str = "The candidate was judged and not accepted, with no defect a repair could start from: it is shown, never offered, and nothing was written. A correction of the request or another verifier can decide it.";

/// A COLD candidate the judge doubts with no defect located (its computation clause carried,
/// every part carried, no task doing more) is never repaired from and never READY; its record is
/// dropped and the candidate held, so no replay asks the same judge again on these bytes. The
/// clause the judge carried stays settled: the core names only the whole request pending, then
/// the verifier the disagreement. A judge that answered nothing (its first call failed) is asked
/// nothing more and keeps the record: a later round asks it; the core names both clauses
/// pending, the verifier each one unknown and why it stopped.
#[tokio::test]
async fn a_doubted_cold_round_keeps_no_record_and_an_unanswered_one_keeps_it() {
    let doubted: &[(&str, Result<&str, &str>)] = &[
        ("verify-clause-0", Ok("carried")),
        ("verify-request", Ok("unfaithful")),
        ("verify-part-0", Ok("carried")),
        ("verify-part-1", Ok("carried")),
        ("verify-part-2", Ok("carried")),
        ("verify-extra", Ok("only_requested")),
    ];
    let unanswered: &[(&str, Result<&str, &str>)] =
        &[("verify-clause-0", Err("the seat is unavailable"))];
    let unobserved = "no trial run of these exact bytes exists in this compile";
    let contested = format!(
        "The judge did not accept the request as carried (unfaithful) and located no defect a repair could start from; the same judge asked again decides nothing ({unobserved}). Nothing is READY on it. Next: a correction of the request, or another verifier."
    );
    for (script, replayable) in [(doubted, false), (unanswered, true)] {
        let author = Author::new([plan(), program()]);
        let judge = Judging::new(script);
        let out = cold(policy(Some(1)), &author, &judge).await;
        assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
        assert_eq!(
            (author.calls(), judge.left()),
            (2, 0),
            "no repair: {out:#?}"
        );
        let attempt = &attempts(&out)[0];
        assert_eq!(attempt["defects"], json!([]), "{attempt:#}");
        let candidate = out.candidate.as_deref().expect("the candidate stays shown");
        let mut steps = vec!["verify: not ready"];
        let (told, kept) = findings(&out);
        if replayable {
            let record = out
                .provenance
                .plan
                .as_ref()
                .expect("an unjudged round is kept");
            assert_eq!(record["strategy"], "cold");
            assert_eq!(attempt["doubt"], json!([]), "{attempt:#}");
            assert_eq!(attempt["unknown"], json!([SUM, intent()]), "{attempt:#}");
            let flags = (
                &attempt["stopped"],
                &attempt["declined"],
                &attempt["whole_asked"],
            );
            assert_eq!(flags, (&json!(true), &json!(false), &json!(false)));
            let expected = [
                pending(SUM, UNREAD, candidate),
                pending(&intent(), UNNAMED, candidate),
                unknown(SUM),
                unknown(&intent()),
                STOPPED.to_owned(),
            ];
            assert_eq!(told, expected);
            assert!(kept.is_empty(), "{kept:?}");
        } else {
            steps.push("verify: doubted, not replayable");
            assert_eq!(out.provenance.plan, None);
            assert_eq!(attempt["doubt"], json!(["unfaithful"]), "{attempt:#}");
            assert_eq!(attempt["contested"], json!([intent()]), "{attempt:#}");
            let flags = (
                &attempt["stopped"],
                &attempt["declined"],
                &attempt["rejected"],
            );
            assert_eq!(flags, (&json!(false), &json!(true), &json!(true)));
            // The clause the judge carried is no longer named pending.
            let expected = [pending(&intent(), UNNAMED, candidate), contested.clone()];
            assert_eq!(told, expected);
            assert_eq!(kept, [HELD]);
        }
        assert_eq!(verify_route(&out), steps);
    }
}

/// The WARM request: the seat settles its one reading (a lookup), then judges the candidate;
/// the field the identifier is read in is answered.
const WARM: &str = "Trova la voce B-8 in ./voci.json e scrivila in ./out/voce.json.";
const WARM_FIELD: &str = "const.voce_id_field";

/// A WARM seat: it settles the reading with a lookup and answers each verifier question by its
/// id: the clauses and parts carried, the whole request `request`, no task doing more.
struct Warm {
    request: Result<&'static str, &'static str>,
    asked: Mutex<Vec<String>>,
}

impl DecisionSeat for Warm {
    fn name(&self) -> &str {
        JUDGE
    }

    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.id.clone());
            let id = question.id.as_str();
            let answer = if id == "verify-request" {
                self.request
            } else if id == "verify-extra" {
                Ok("only_requested")
            } else if id.starts_with("verify-") {
                Ok("carried")
            } else {
                Ok("lookup")
            };
            answer
                .map(|key| ChoiceAnswer::new(key, JUDGE))
                .map_err(|why| DecisionError(why.to_owned()))
        })
    }
}

/// A WARM candidate the seat doubts with no defect located is never READY and keeps no
/// replayable record; one whose whole-request call failed keeps it.
#[tokio::test]
async fn a_doubted_warm_round_keeps_no_record_and_an_unanswered_one_keeps_it() {
    for (request, replayable) in [(Ok("unfaithful"), false), (Err("unavailable"), true)] {
        let seat = Warm {
            request,
            asked: Mutex::new(Vec::new()),
        };
        let cognition = Cognition::<NoProvider> {
            provider: None,
            seat: Some(&seat),
        };
        let request = CompileRequest::create(WARM).answer(WARM_FIELD, "\"id\"");
        let out = crate::compile_with_cognition(&request, cognition)
            .await
            .unwrap();
        assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
        let asked = seat.asked.lock().unwrap().clone();
        assert_eq!(asked[..2], ["clause-0", "verify-request"], "{asked:?}");
        let mut steps = vec!["verify: not ready"];
        if replayable {
            assert_eq!(asked.len(), 2, "{asked:?}");
            let record = out
                .provenance
                .plan
                .as_ref()
                .expect("an unjudged round is kept");
            assert_eq!(record["strategy"], "warm");
        } else {
            let localized = ["verify-part-0", "verify-extra"];
            assert_eq!(asked[2..], localized, "{asked:?}");
            steps.push("verify: doubted, not replayable");
            assert_eq!(out.provenance.plan, None);
            assert_eq!(findings(&out).1, [HELD]);
        }
        assert_eq!(verify_route(&out), steps);
    }
}

/// The COLD request with a schedule before it, judged by a seat answering the whole request
/// `request` (its clause and parts carried, no task doing more).
async fn scheduled(request: Result<&'static str, &'static str>) -> CompileOutcome {
    let scheduled = format!("Every weekday at 8, {}", intent());
    let mut proposal: Value = serde_json::from_str(&plan()).unwrap();
    let trigger = json!({"text": "Every weekday at 8,", "role": "trigger"});
    proposal["regions"]
        .as_array_mut()
        .unwrap()
        .insert(0, trigger);
    let author = Author::new([proposal.to_string(), program()]);
    let judge = Warm {
        request,
        asked: Mutex::new(Vec::new()),
    };
    let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed",
        "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
    let compiled = CompileRequest::create(scheduled)
        .with_knowledge(observed)
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy(Some(0)));
    let cognition = Cognition {
        provider: Some(&author),
        seat: Some(&judge),
    };
    crate::compile_with_cognition(&compiled, cognition)
        .await
        .unwrap()
}

/// The non-mandatory binding values a schedule asks beside the candidate (R4 S1).
const BINDINGS: [&str; 4] = [
    "trigger.timezone",
    "trigger.missed",
    "trigger.overlap",
    "trigger.ceiling",
];

/// A COLD round asks its schedule's binding values beside the candidate. One the judge doubts
/// keeps no record and asks the human nothing more: its questions and requested boundary go with
/// the record, so nobody answers about bytes no round replays, and the held candidate stays
/// shown. A round the judge carries keeps them, READY; one whose judge answered nothing keeps
/// them with its record, for the later round that asks the judge again.
#[tokio::test]
async fn a_doubted_cold_round_asks_no_question_and_an_unanswered_one_keeps_them() {
    let cases = [
        (
            Ok("faithful"),
            CompileStatus::Ready,
            "verify: judged (decision_seat)",
        ),
        (
            Err("unavailable"),
            CompileStatus::Incomplete,
            "verify: not ready",
        ),
        (
            Ok("unfaithful"),
            CompileStatus::Incomplete,
            "verify: doubted, not replayable",
        ),
    ];
    for (verdict, status, last) in cases {
        let out = scheduled(verdict).await;
        assert_eq!(out.status, status, "{out:#?}");
        assert!(
            out.candidate.is_some(),
            "the candidate stays shown: {out:#?}"
        );
        let kept = !matches!(verdict, Ok("unfaithful"));
        let asked: Vec<(&str, bool)> = (out.questions.iter())
            .map(|q| (q.key.as_str(), q.mandatory))
            .collect();
        let expected: Vec<(&str, bool)> = if kept {
            BINDINGS.iter().map(|key| (*key, false)).collect()
        } else {
            Vec::new()
        };
        assert_eq!(asked, expected, "{verdict:?}");
        let flags = (
            out.requested_boundary.is_some(),
            out.provenance.plan.is_some(),
        );
        assert_eq!(flags, (kept, kept), "{verdict:?}");
        assert_eq!(verify_route(&out).last().map(String::as_str), Some(last));
        let held: &[&str] = if kept { &[] } else { &[HELD] };
        assert_eq!(findings(&out).1, held, "{verdict:?}");
    }
}
