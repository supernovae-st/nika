// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The verifier's own tests: the grounding, the clause and whole-request judgments, a call that
//! gets no answer stopping the verdict, what a doubted verdict leaves replayable and holds, and
//! the observation a native verdict may show its judge. The attempts on the same bytes and their
//! records ([`attempts`]), the rejections carried from an earlier round ([`carried`]), the
//! localizations resumed on the same bytes ([`resumed`]) and the COLD and WARM rounds
//! ([`rounds`]) are kept beside this file to bound its size; the parts a doubted request is asked
//! in are proven with their owner, `nika_compile_clauses::parts` (ADR-145).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::{Declined, grounding, parts};
use crate::authority::{Envelope, Seat};
use serde_json::{Value, json};

/// The attempts of one compile on the same bytes, their records, usage and forensic summary.
mod attempts;
/// The rejections a host carries from an earlier round of the conversation.
mod carried;
/// The localizations a verdict on the same bytes left unfinished, resumed.
mod resumed;
/// The COLD and WARM rounds under a doubting judge.
mod rounds;

/// The note of a part or clause no task performs.
const OMITTED: &str = "the judge finds no task performing it";
/// What a candidate judged and rejected with no defect located is held with (`verify_held`).
const HELD: &str = "The candidate was judged and not accepted, with no defect a repair could start from: it is shown, never offered, and nothing was written. A correction of the request or another verifier can decide it.";
/// What a candidate whose located defects the repairs did not settle is held with: this verifier
/// is not asked again on these bytes, in this compile or in a later round carrying the verdict.
const HELD_DEFECTS: &str = "The candidate was judged and not accepted: the parts named above stay missing. It is shown, never offered, and nothing was written; this verifier is not asked again on these bytes, in this compile or in a later round that carries this verdict. A correction of the request, another authoring model or another verifier can decide it.";
/// What a candidate the verifier only abstained on is held with: an abstention is not carried
/// to a later round, so a new round that authors again can decide it.
const HELD_ABSTAINED: &str = "The verifier read the candidate and abstained: it neither accepted nor rejected it, and located no defect. It is shown, never offered, and nothing was written; it is not asked again on these bytes in this compile. A correction of the request, another verifier, or a new round that authors again can decide it.";
/// What a verification stopped at a call that got no answer adds to the findings.
const STOPPED: &str = "The verification stopped at a judge call that got no answer (refused by the call bound, or failed: the receipt says which); nothing after it was asked of that judge. Next: another round, or a larger call bound.";

/// Approves the whole request unless it `doubt`s it, and every clause or part asked alone
/// except the `missing` ones, counted in asking order across clause and part questions; asked
/// which task fails one it found missing, it answers `pointer` (`omitted` when unset); asked
/// whether a task does more than the request, or over an observed run, it approves. It keeps
/// the instructions of every question. A whole verdict that conflicts with a clause must never
/// erase the clause's concrete defect.
#[derive(Default)]
struct Approving {
    missing: Vec<usize>,
    doubt: bool,
    pointer: Option<&'static str>,
    clauses: AtomicUsize,
    told: Mutex<Vec<String>>,
}

impl nika_kernel::ai::provider::ProviderInferDyn for Approving {
    async fn infer(
        &self,
        request: nika_kernel::ai::provider::InferRequest,
    ) -> Result<nika_kernel::ai::provider::InferResponse, nika_kernel::ai::provider::ProviderError>
    {
        use nika_kernel::ai::provider::{
            ContentBlock, InferResponse, ProviderError, ResponseFormat, StopReason, TokenUsage,
        };
        let ResponseFormat::JsonSchema(schema) = &request.response_format else {
            return Err(ProviderError::Other {
                reason: "not a verifier question".to_owned(),
            });
        };
        let system = request.messages.first().and_then(|m| m.content.first());
        if let Some(ContentBlock::Text { text }) = system {
            self.told.lock().unwrap().push(text.clone());
        }
        let keys = schema["properties"]["choice"]["enum"].to_string();
        let key = if keys.contains("\"faithful\"") {
            if self.doubt { "unfaithful" } else { "faithful" }
        } else if keys.contains("\"no_task\"") {
            self.pointer.unwrap_or("omitted")
        } else if keys.contains("\"only_requested\"") {
            "only_requested"
        } else if keys.contains("\"consistent\"") {
            "consistent"
        } else {
            let at = (self.clauses).fetch_add(1, Ordering::SeqCst);
            if self.missing.contains(&at) {
                "missing"
            } else {
                "carried"
            }
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: json!({"choice": key}).to_string(),
            }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

async fn ten_clauses<P: nika_kernel::ai::provider::ProviderInferDyn>(
    provider: &P,
) -> (Vec<String>, super::Verdict, crate::CompileOutcome) {
    let clauses: Vec<String> = (0..10).map(|k| format!("clause number {k}")).collect();
    let intent = clauses.join(", ");
    let mut open: Vec<Value> = Vec::new();
    let mut at = 0;
    for clause in &clauses {
        let span = json!([[at, at + clause.len()]]);
        open.push(json!({"clause": clause, "witness": "label", "spans": span}));
        at += clause.len() + 2;
    }
    open.push(json!({"clause": intent, "witness": null, "spans": [[0, intent.len()]]}));
    let mut settled = crate::initial();
    settled.candidate = Some("nika: all-clauses\n".to_owned());
    settled.provenance.decision = Some(json!({"pending": {"open": open}}));
    let policy = crate::AuthoringPolicy::new("mock/judge", 256, std::time::Duration::from_secs(2));
    let judge = super::Judge::Provider(&policy, provider);
    let request = crate::CompileRequest::create(intent.as_str());
    let plan = crate::plan::Plan::default();
    let mut out = crate::initial();
    let verdict = super::verdict_on(&intent, &request, &plan, &settled, &judge, &mut out).await;
    (clauses, verdict, out)
}

/// The questions of `verdict` asked under `role`.
fn asked_as(verdict: &super::Verdict, role: &str) -> usize {
    (verdict.records.iter())
        .filter(|record| record["role"] == role)
        .count()
}

/// The ids of the questions `records` holds, in order.
fn ids(records: &[Value]) -> Vec<String> {
    (records.iter())
        .filter_map(|r| r["question"].as_str().map(str::to_owned))
        .collect()
}

/// Every clause the core left pending is judged; a missing ninth clause asks which task fails
/// it, and named as an operation no task performs it is the defect a repair starts from, its
/// note beside it, while the whole request and every other clause are carried.
#[tokio::test]
async fn every_clause_is_judged_and_a_missing_ninth_clause_blocks() {
    for missing in [vec![], vec![8]] {
        let provider = Approving {
            missing: missing.clone(),
            ..Approving::default()
        };
        let (clauses, verdict, mut out) = ten_clauses(&provider).await;
        let asked = (
            asked_as(&verdict, "judge_clause"),
            asked_as(&verdict, "judge_point"),
            asked_as(&verdict, "judge_request"),
        );
        assert_eq!(asked, (10, missing.len(), 1), "{:?}", verdict.records);
        assert!(verdict.unknown.is_empty() && verdict.contested.is_empty());
        let defects: Vec<String> = missing
            .iter()
            .map(|&index| clauses[index].clone())
            .collect();
        assert_eq!(verdict.defects, defects);
        let notes: Vec<(String, String)> = (defects.iter())
            .map(|defect| (defect.clone(), OMITTED.to_owned()))
            .collect();
        assert_eq!(verdict.notes, notes);
        // A clause judged missing rejects these bytes, though the whole request was carried.
        let declined = if missing.is_empty() {
            Declined::No
        } else {
            Declined::Rejected
        };
        assert_eq!(verdict.declined, declined);
        assert_eq!(verdict.doubted(), !missing.is_empty());
        let whole = clauses.join(", ");
        assert_eq!(verdict.request.as_deref(), Some(whole.as_str()));
        assert_eq!(verdict.settled_by, Some("verify-request"));
        let mut judged: Vec<String> = (0..10)
            .filter(|k| !missing.contains(k))
            .map(|k| format!("verify-clause-{k}"))
            .collect();
        judged.push("verify-request".to_owned());
        let settled: Vec<&str> = (verdict.judgments.iter())
            .map(|judgment| judgment.question.as_str())
            .collect();
        assert_eq!(settled, judged);
        super::blocked(&mut out, &verdict, 0);
        let told: Vec<&str> = (out.diagnostics.iter())
            .filter(|d| d.target == "semantic_verification")
            .map(|d| d.message.as_str())
            .collect();
        if let Some(&index) = missing.first() {
            let point = (verdict.records.iter())
                .find(|r| r["role"] == "judge_point")
                .unwrap();
            assert_eq!(point["question"], format!("verify-clause-{index}-point"));
            assert_eq!(point["options"], json!(["omitted", "no_task", "none"]));
            let clause = json!({"text": clauses[index], "restricts": false});
            assert_eq!(point["clause"], clause);
            let message = format!(
                "The judge compared the whole request with the candidate's bytes: it does not carry « {} ({OMITTED}) ». 0 repair(s) from that defect did not settle it; nothing is READY. Next: a stronger authoring model, or a restatement of that part.",
                clauses[index]
            );
            assert_eq!(told, [message.as_str()]);
        } else {
            assert!(out.diagnostics.is_empty(), "{out:#?}");
        }
    }
}

/// The explicit judge authority bounds the calls (R4 A11): its ninth call is refused before any
/// byte leaves, and a call that gets no answer stops the verdict there. The clause it refused,
/// every clause after it and the whole request are unknown, never asked, and nothing more is
/// sent to that judge; a refusal declines nothing.
#[tokio::test]
async fn every_clause_still_obeys_the_explicit_judge_authority() {
    let authority = Arc::new(Envelope::new(8, "test bound"));
    let provider = Seat::new(Approving::default(), authority.clone());
    let (clauses, verdict, out) = ten_clauses(&provider).await;
    assert_eq!(authority.account(), json!({"sent": 8, "refused": 1}));
    let asked: Vec<String> = (0..9).map(|k| format!("verify-clause-{k}")).collect();
    assert_eq!(ids(&verdict.records), asked);
    let mut unknown = clauses[8..].to_vec();
    unknown.push(clauses.join(", "));
    assert_eq!(verdict.unknown, unknown);
    assert!(verdict.defects.is_empty() && verdict.contested.is_empty());
    let counts = (verdict.attempted, verdict.returned, verdict.consumed);
    assert_eq!(counts, (9, 8, 8));
    assert!(verdict.stopped);
    assert_eq!(verdict.declined, Declined::No);
    assert!(!verdict.whole_asked && verdict.request.is_none());
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(receipt.context.len(), 9, "the refusal is journaled");
    let refused = &receipt.context.last().unwrap()["result"]["failure_kind"];
    assert_eq!(refused, "admission_refused");
    // The usage counts the eight requests sent, never the refusal no byte left for.
    let usage = json!({"calls": 8, "input_tokens": 8, "output_tokens": 8, "complete": true});
    assert_eq!(verdict.usage, usage);
    // Each unsettled clause is named, then why nothing after the refusal was asked.
    let mut blocked = crate::initial();
    super::blocked(&mut blocked, &verdict, 0);
    let told: Vec<&str> = (blocked.diagnostics.iter())
        .filter(|d| d.target == "semantic_verification")
        .map(|d| d.message.as_str())
        .collect();
    let mut expected: Vec<String> = (verdict.unknown.iter())
        .map(|unknown| format!("The judge could not settle `{unknown}` against the candidate (it abstained, answered outside its options, or its call failed); nothing is READY on it. Next: a judge that answers, or a restatement the deterministic reader reads."))
        .collect();
    expected.push(STOPPED.to_owned());
    assert_eq!(told, expected);
}

/// A clause asked on its own and the same part of the doubted whole request, both judged
/// missing, name one defect at its first place, however far apart they were found: the ninth
/// and tenth clauses, then the ninth part again, leave the ninth and the tenth once each, each
/// read with its note. Each missing one asked which task fails it under its own id.
#[tokio::test]
async fn a_clause_and_the_same_part_found_missing_name_one_defect() {
    let provider = Approving {
        missing: vec![8, 9, 18],
        doubt: true,
        ..Approving::default()
    };
    let (clauses, verdict, _) = ten_clauses(&provider).await;
    let roles = (
        asked_as(&verdict, "judge_clause"),
        asked_as(&verdict, "judge_request"),
        asked_as(&verdict, "judge_part"),
        asked_as(&verdict, "judge_point"),
    );
    assert_eq!(roles, (10, 1, 10, 3), "{:?}", verdict.records);
    let pointed: Vec<String> = (verdict.records.iter())
        .filter(|r| r["role"] == "judge_point")
        .filter_map(|r| r["question"].as_str().map(str::to_owned))
        .collect();
    let pointers = [
        "verify-clause-8-point",
        "verify-clause-9-point",
        "verify-point-8",
    ];
    assert_eq!(pointed, pointers);
    assert_eq!(verdict.defects, [clauses[8].clone(), clauses[9].clone()]);
    let noted = [
        format!("{} ({OMITTED})", clauses[8]),
        format!("{} ({OMITTED})", clauses[9]),
    ];
    assert_eq!(verdict.noted_defects(), noted);
    assert_eq!(verdict.doubt, ["unfaithful"]);
    assert!(verdict.unknown.is_empty() && verdict.contested.is_empty());
    // The eight clauses carried are judged; the doubted whole request is not.
    let judged: Vec<&str> = (verdict.judgments.iter())
        .map(|judgment| judgment.question.as_str())
        .collect();
    let carried: Vec<String> = (0..8).map(|k| format!("verify-clause-{k}")).collect();
    assert_eq!(judged, carried);
}

/// An unfaithful whole-request verdict asks every part alone, each question bound to its part's
/// own words: the seventeenth or the twentieth part judged missing asks which task fails it at
/// once, then is the defect, its path whole, with no extra question (R4 A11, E36).
#[tokio::test]
async fn every_part_is_asked_alone_and_the_missing_one_is_the_defect() {
    let clauses: Vec<String> = (0..20)
        .map(|k| format!("write clause {k} to ./out/part-{k}.json"))
        .collect();
    let intent = clauses.join("; ");
    assert_eq!(parts(&intent), clauses);
    let request = crate::CompileRequest::create(intent.as_str());
    let plan = crate::plan::Plan::default();
    let candidate = "nika: localization\n";
    let binding = super::Binding::of(&intent, &request, &plan, candidate);
    let base = super::state(&intent, &request, candidate);
    let policy = crate::AuthoringPolicy::new("mock/judge", 256, std::time::Duration::from_secs(2));
    for index in [16, 19] {
        let provider = Approving {
            doubt: true,
            missing: vec![index],
            ..Approving::default()
        };
        let judge = super::Judge::Provider(&policy, &provider);
        let mut verdict = super::Verdict::default();
        let mut out = crate::initial();
        let asked = (&base, "fixture");
        super::whole(
            &intent,
            asked,
            &judge,
            &binding,
            None,
            &mut verdict,
            &mut out,
        )
        .await;
        assert_eq!(verdict.defects, [clauses[index].clone()]);
        let note = (clauses[index].clone(), OMITTED.to_owned());
        assert_eq!(verdict.notes, [note]);
        assert_eq!(verdict.doubt, ["unfaithful"]);
        assert!(verdict.unknown.is_empty() && verdict.contested.is_empty());
        assert!(verdict.judgments.is_empty());
        let asked = ids(&verdict.records);
        assert_eq!(asked.len(), 22, "{asked:?}");
        assert_eq!(asked[0], "verify-request");
        assert_eq!(asked[index + 2], format!("verify-point-{index}"));
        let point = &verdict.records[index + 2];
        assert_eq!(point["options"], json!(["omitted", "no_task", "none"]));
        let clause = json!({"text": clauses[index], "restricts": false});
        assert_eq!(point["clause"], clause);
        let asked_alone = (verdict.records.iter()).filter(|r| r["role"] == "judge_part");
        for (k, record) in asked_alone.enumerate() {
            assert_eq!(record["question"], format!("verify-part-{k}"));
            let clause = json!({"text": clauses[k], "restricts": false});
            assert_eq!(record["clause"], clause);
            let missing = if k == index { "missing" } else { "carried" };
            assert_eq!(record["choice"], missing, "{k}");
        }
    }
}

/// The candidate and the state a clause question reads: a greeting written by one task.
const GREETING: &str = r#"nika: greeting
permits:
  tools: ["nika:write"]
  fs:
    write: ["./out/result.txt"]
tasks:
  save:
    invoke:
      tool: "nika:write"
      args:
        path: "./out/result.txt"
        content: hello
        overwrite: true
        create_dirs: true
"#;

/// One pending clause judged by `provider` at its statements `spans` of `intent`, over
/// [`GREETING`].
async fn clause_judged(
    intent: &str,
    open: &super::Open,
    provider: &Approving,
) -> (super::Verdict, crate::CompileOutcome) {
    let request = crate::CompileRequest::create(intent);
    let plan = crate::plan::Plan::default();
    let binding = super::Binding::of(intent, &request, &plan, GREETING);
    let base = super::state(intent, &request, GREETING);
    let policy = crate::AuthoringPolicy::new("mock/judge", 256, std::time::Duration::from_secs(2));
    let judge = super::Judge::Provider(&policy, provider);
    let mut verdict = super::Verdict::default();
    let mut out = crate::initial();
    let asked = (0, &base, &binding, "fixture");
    super::judge_clause(open, asked, &judge, &mut verdict, &mut out).await;
    (verdict, out)
}

/// A clause judged missing asks at once which task fails it (R4 A11), under the clause's own
/// id: each task of the candidate in order, then `no_task`, its record naming the clause. A
/// prohibition (« never overwrite it ») asks no operation of its own, so it is never offered
/// `omitted`, and a judge choosing it anyway is refused on admission: the clause stays unknown,
/// never a defect. A task makes the clause a defect with that note; no task failing it leaves it
/// contested; no choice leaves it unknown. A restricting clause is told so in both questions and
/// is never offered `no_operation`.
#[tokio::test]
async fn a_clause_judged_missing_asks_which_task_fails_it() {
    let intent = "Write the text hello to ./out/result.txt, never overwrite it.";
    let clause = "never overwrite it";
    let at = intent.find(clause).unwrap();
    let open = super::Open {
        clause: clause.to_owned(),
        unclaimed: true,
        spans: vec![(at, at + clause.len())],
    };
    let pointed = "the judge points to the task save";
    let unoffered = "the seat chose `omitted`, which was not offered";
    let cases = [
        (
            "task-save",
            json!([clause]),
            json!([[clause, pointed]]),
            json!([]),
            json!([]),
            2,
        ),
        (
            "omitted",
            json!([]),
            json!([]),
            json!([clause]),
            json!([]),
            1,
        ),
        (
            "no_task",
            json!([]),
            json!([]),
            json!([]),
            json!([clause]),
            2,
        ),
        ("none", json!([]), json!([]), json!([clause]), json!([]), 1),
    ];
    for (pointer, defects, notes, unknown, contested, consumed) in cases {
        let provider = Approving {
            missing: vec![0],
            pointer: Some(pointer),
            ..Approving::default()
        };
        let (verdict, _) = clause_judged(intent, &open, &provider).await;
        let asked = ids(&verdict.records);
        assert_eq!(asked, ["verify-clause-0", "verify-clause-0-point"]);
        let first = &verdict.records[0];
        assert_eq!(first["options"], json!(["carried", "missing", "none"]));
        let point = &verdict.records[1];
        assert_eq!(point["role"], "judge_point");
        assert_eq!(point["options"], json!(["task-save", "no_task", "none"]));
        assert_eq!(point["clause"], json!({"text": clause, "restricts": true}));
        let refused = (pointer == "omitted").then_some(unoffered);
        assert_eq!(point["error"].as_str(), refused, "{pointer}");
        let found = json!({"defects": verdict.defects, "notes": verdict.notes,
            "unknown": verdict.unknown, "contested": verdict.contested});
        let expected = json!({"defects": defects, "notes": notes, "unknown": unknown,
            "contested": contested});
        assert_eq!(found, expected, "{pointer}");
        assert!(verdict.judgments.is_empty());
        assert_eq!(verdict.consumed, consumed, "{pointer}");
        // The missing answer rejects these bytes whatever the task question then names.
        assert_eq!(verdict.declined, Declined::Rejected, "{pointer}");
        assert!(!verdict.stopped, "{pointer}");
        let told = provider.told.lock().unwrap();
        assert!(
            told.iter()
                .all(|text| text.contains(super::faithful::RESTRICTING))
        );
        assert!(told[1].contains("The clause below was judged missing."));
        assert!(!told[1].contains("omitted:"), "{}", told[1]);
    }
}

/// A conditional clause (« if a row has no email, skip it ») restricts but is no prohibition: it
/// may ask an operation of its own (a filter no task performs), so its task question offers
/// `omitted`, says what it means, and that answer makes the clause a defect.
#[tokio::test]
async fn a_conditional_clause_judged_missing_may_be_an_operation_no_task_performs() {
    let intent =
        "Read ./data/contacts.csv; if a row has no email, skip it; write ./out/result.txt.";
    let clause = "if a row has no email, skip it";
    let at = intent.find(clause).unwrap();
    let open = super::Open {
        clause: clause.to_owned(),
        unclaimed: false,
        spans: vec![(at, at + clause.len())],
    };
    let provider = Approving {
        missing: vec![0],
        ..Approving::default()
    };
    let (verdict, _) = clause_judged(intent, &open, &provider).await;
    let point = &verdict.records[1];
    assert_eq!(point["question"], "verify-clause-0-point");
    let offered = json!(["task-save", "omitted", "no_task", "none"]);
    assert_eq!(point["options"], offered);
    assert_eq!(point["clause"], json!({"text": clause, "restricts": true}));
    assert_eq!(verdict.defects, [clause]);
    assert_eq!(verdict.notes, [(clause.to_owned(), OMITTED.to_owned())]);
    assert_eq!(verdict.consumed, 2);
    let told = provider.told.lock().unwrap();
    let omitted = "omitted: the clause asks an operation of its own (a read, a filter, a computation, a condition, a write) that no task performs.";
    assert!(told[1].contains(omitted), "{}", told[1]);
}

/// A clause stated twice is judged at each statement, and a statement judged missing asks its
/// task question under that statement's id. A plain clause no element claims may ask nothing of
/// the workflow; one an element claims may not.
#[tokio::test]
async fn a_later_statement_of_a_clause_asks_its_task_under_its_own_id() {
    let intent = "Greet the team. Greet the team.";
    let open = |unclaimed: bool| super::Open {
        clause: "Greet the team".to_owned(),
        unclaimed,
        spans: vec![(0, 14), (16, 30)],
    };
    let provider = Approving {
        missing: vec![1],
        pointer: Some("task-save"),
        ..Approving::default()
    };
    let (verdict, _) = clause_judged(intent, &open(true), &provider).await;
    let asked = ids(&verdict.records);
    let expected = [
        "verify-clause-0",
        "verify-clause-0.1",
        "verify-clause-0.1-point",
    ];
    assert_eq!(asked, expected);
    let offered = json!(["carried", "missing", "no_operation", "none"]);
    assert_eq!(verdict.records[0]["options"], offered);
    assert_eq!(verdict.defects, ["Greet the team"]);
    let judged: Vec<(&str, (usize, usize))> = (verdict.judgments.iter())
        .map(|judgment| (judgment.question.as_str(), judgment.span))
        .collect();
    assert_eq!(judged, [("verify-clause-0", (0, 14))]);
    let plain = (provider.told.lock().unwrap().iter())
        .all(|text| !text.contains(super::faithful::RESTRICTING));
    assert!(plain);
    let (verdict, _) = clause_judged(intent, &open(false), &Approving::default()).await;
    let withheld = json!(["carried", "missing", "none"]);
    assert_eq!(verdict.records[0]["options"], withheld);
    assert_eq!(verdict.judgments.len(), 2);
}

/// A run that wrote the greeting, observed of the bytes whose digest is `candidate_sha256`.
fn greeted(candidate_sha256: &str) -> Value {
    json!({
        "candidate_sha256": candidate_sha256,
        "inputs": [],
        "outputs": [{"path": "./out/result.txt", "text": "hello", "written": true,
            "read_whole": true}],
    })
}

/// What a native verdict of the greeting left when it was not READY.
fn not_ready(
    judged: Result<crate::CompileOutcome, Box<(crate::CompileOutcome, super::Verdict)>>,
) -> super::Verdict {
    let Err(judged) = judged else {
        panic!("a doubt no run of these bytes decides is never READY");
    };
    judged.1
}

/// The native verdict of the greeting by a judge that doubts the whole request and approves
/// everything else, shown `observation`.
async fn greeting_judged(
    observation: &Value,
) -> Result<crate::CompileOutcome, Box<(crate::CompileOutcome, super::Verdict)>> {
    let intent = "Write the text hello to ./out/result.txt.";
    let request = crate::CompileRequest::create(intent);
    let reading = crate::lexicon::read(intent);
    let policy = crate::AuthoringPolicy::new("mock/judge", 256, std::time::Duration::from_secs(2));
    let mut ready = crate::initial();
    nika_compile::surface::finish(GREETING.to_owned(), &mut ready);
    assert_eq!(ready.status, crate::CompileStatus::Ready, "{ready:#?}");
    let provider = Approving {
        doubt: true,
        ..Approving::default()
    };
    let seats = (&provider, None);
    super::native_verdict(
        intent,
        &reading,
        &policy,
        seats,
        &request,
        ready,
        0,
        Some(observation),
    )
    .await
}

/// An observation binds only to the bytes it ran (R6): a native verdict shows its judge the run
/// of these exact bytes, and never another candidate's. A doubt nothing locates, over a run of
/// other bytes, stays contested with no question over that run; over a run of these bytes whose
/// one output it never wrote, no question is asked over the run either (it proves no whole
/// output); over a run of these bytes read whole and written, the judge's consistent answer
/// carries the request.
#[tokio::test]
async fn a_native_verdict_shows_its_judge_only_a_run_of_the_same_bytes() {
    let intent = "Write the text hello to ./out/result.txt.";
    let mut ready = crate::initial();
    nika_compile::surface::finish(GREETING.to_owned(), &mut ready);
    let sha = super::knowledge::sha256(&ready.candidate.unwrap());
    let elsewhere = greeted(&super::knowledge::sha256("nika: another-workflow\n"));
    let verdict = not_ready(greeting_judged(&elsewhere).await);
    let asked = ["verify-request", "verify-part-0", "verify-extra"];
    assert_eq!(ids(&verdict.records), asked);
    assert_eq!(verdict.contested, [intent]);
    let unobserved = "no trial run of these exact bytes exists in this compile";
    assert_eq!(verdict.unsettled, [unobserved]);
    assert!(verdict.defects.is_empty() && verdict.unknown.is_empty());
    assert!(verdict.doubted());
    assert_eq!(verdict.candidate_sha256.as_deref(), Some(sha.as_str()));
    let mut unwritten = greeted(&sha);
    unwritten["outputs"][0]["written"] = json!(false);
    let verdict = not_ready(greeting_judged(&unwritten).await);
    assert_eq!(ids(&verdict.records), asked);
    let partial = "the trial run wrote nothing it was read for, or was read only in part: it proves no whole output";
    assert_eq!(verdict.unsettled, [partial]);
    assert_eq!(verdict.contested, [intent]);
    assert!(verdict.defects.is_empty() && verdict.unknown.is_empty());
    let observed = greeted(&sha);
    let Ok(out) = greeting_judged(&observed).await else {
        panic!("a consistent run of these bytes, read whole, carries the request");
    };
    assert_eq!(out.status, crate::CompileStatus::Ready, "{out:#?}");
    let decision = out.provenance.decision.as_ref().unwrap();
    let verified = &decision["semantic_verification"][0];
    let questions = verified["questions"].as_array().unwrap();
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-extra",
        "verify-observed",
    ];
    assert_eq!(ids(questions), asked);
    let receipt = &questions[3]["observation"];
    let digest = super::knowledge::sha256(&observed.to_string());
    assert_eq!(
        (&receipt["candidate_sha256"], &receipt["sha256"]),
        (&json!(sha), &json!(digest))
    );
    for list in ["defects", "unknown", "contested", "unsettled", "notes"] {
        assert_eq!(verified[list], json!([]), "{list}");
    }
    assert_eq!(verified["doubt"], json!(["unfaithful"]));
    assert_eq!(verified["settled_by"], "verify-observed");
    assert_eq!(verified["candidate_sha256"], json!(sha));
    let route = decision["route"].as_array().unwrap();
    assert_eq!(route.last().unwrap(), "verify: judged (authoring_provider)");
}

/// The route steps a decision records, in order.
fn route(out: &crate::CompileOutcome) -> Vec<String> {
    (out.provenance.decision.as_ref())
        .and_then(|decision| decision["route"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|step| step.as_str().map(str::to_owned))
        .collect()
}

/// The `verify_held` findings an outcome carries, with their kind.
fn held_findings(out: &crate::CompileOutcome) -> Vec<(crate::DiagnosticKind, &str)> {
    (out.diagnostics.iter())
        .filter(|d| d.target == "verify_held")
        .map(|d| (d.kind, d.message.as_str()))
        .collect()
}

/// A verdict whose judge answered and did not accept the bytes is never replayed to it (R6):
/// the replayable record is dropped, the route says why and the `verify_held` finding says what
/// can decide it, worded by how the judge declined (defects located, a rejection with none, or
/// an abstention alone); the round's questions and requested boundary go with the record, so no
/// human answers about bytes no round replays. A verdict the judge answered nothing to keeps the
/// record, its questions and its boundary (a later round asks it), and so does one a later
/// question settled.
#[test]
fn a_doubted_verdict_drops_its_replayable_record_and_an_unanswered_one_keeps_it() {
    let record = json!({"strategy": "cold", "intent_sha256": "x"});
    let rejected = super::Verdict {
        doubt: vec!["unfaithful".to_owned()],
        contested: vec!["the request".to_owned()],
        declined: Declined::Rejected,
        ..super::Verdict::default()
    };
    let abstained = super::Verdict {
        doubt: vec!["none".to_owned()],
        unknown: vec!["the request".to_owned()],
        declined: Declined::Abstained,
        ..super::Verdict::default()
    };
    let located = super::Verdict {
        doubt: vec!["unfaithful".to_owned()],
        defects: vec!["a part".to_owned()],
        declined: Declined::Rejected,
        ..super::Verdict::default()
    };
    let unanswered = super::Verdict {
        unknown: vec!["the request".to_owned()],
        stopped: true,
        ..super::Verdict::default()
    };
    let decided = super::Verdict {
        doubt: vec!["unfaithful".to_owned()],
        declined: Declined::Rejected,
        settled_by: Some("verify-observed"),
        ..super::Verdict::default()
    };
    let dropped = ["verify: doubted, not replayable".to_owned()];
    let applied = crate::DiagnosticKind::Applied;
    let cases = [
        (rejected, None, &dropped[..], vec![(applied, HELD)]),
        (located, None, &dropped[..], vec![(applied, HELD_DEFECTS)]),
        (
            abstained,
            None,
            &dropped[..],
            vec![(applied, HELD_ABSTAINED)],
        ),
        (unanswered, Some(record.clone()), &[][..], vec![]),
        (decided, Some(record.clone()), &[][..], vec![]),
    ];
    for (verdict, kept, routed, held) in cases {
        let mut out = crate::initial();
        nika_compile::surface::finish(GREETING.to_owned(), &mut out);
        let boundary = serde_json::to_value(&out.requested_boundary).unwrap();
        assert!(boundary.is_object(), "{out:#?}");
        let text = crate::QuestionType::Text;
        crate::question(&mut out, "const.greeting", "the greeting", text);
        let asked = out.questions.clone();
        assert_eq!(asked.len(), 1);
        out.provenance.plan = Some(record.clone());
        super::unreplayed(&mut out, &verdict);
        let replayable = kept.is_some();
        assert_eq!(out.provenance.plan, kept);
        assert_eq!(route(&out), routed);
        assert_eq!(held_findings(&out), held);
        let (questions, bounded) = if replayable {
            (asked, boundary)
        } else {
            (Vec::new(), Value::Null)
        };
        assert_eq!(out.questions, questions, "{routed:?}");
        let left = serde_json::to_value(&out.requested_boundary).unwrap();
        assert_eq!(left, bounded, "{routed:?}");
        assert_eq!(out.candidate.as_deref(), Some(GREETING), "{routed:?}");
    }
}

/// A doubt on the clause path is a doubt (R6): the whole request carried, a pending clause then
/// judged missing whose task question names no task failing it leaves that clause contested;
/// the judge answered these bytes and did not accept them, so the record that would ask it
/// again is dropped and the outcome is held, worded as a rejection with no defect located.
#[tokio::test]
async fn a_clause_contested_after_a_faithful_whole_request_is_a_doubt() {
    let provider = Approving {
        missing: vec![3],
        pointer: Some("no_task"),
        ..Approving::default()
    };
    let (clauses, verdict, _) = ten_clauses(&provider).await;
    assert_eq!(verdict.settled_by, Some("verify-request"));
    assert_eq!(verdict.contested, [clauses[3].clone()]);
    assert!(verdict.defects.is_empty() && verdict.unknown.is_empty());
    assert_eq!(verdict.declined, Declined::Rejected);
    assert!(verdict.rejected() && verdict.doubted() && !verdict.settled());
    let mut out = crate::initial();
    out.provenance.plan = Some(json!({"strategy": "cold"}));
    super::unreplayed(&mut out, &verdict);
    assert_eq!(out.provenance.plan, None);
    assert_eq!(route(&out), ["verify: doubted, not replayable"]);
    let applied = crate::DiagnosticKind::Applied;
    assert_eq!(held_findings(&out), [(applied, HELD)]);
}

/// The reference selects its contracts by the checker's capability inference over the
/// parsed workflow, whatever task form reaches a tool (R4 A11, E36): an invoke inside a
/// fan-out and the tools an agent may call are read, a tool the agent is denied is not, an
/// MCP tool is named as uncovered, and each contract is its whole section (the write
/// contract past two thousand characters, up to its last error code, and nothing of the
/// next section). The record's digest and size are those of the text sent.
#[test]
fn the_reference_holds_the_whole_contract_of_every_tool_the_workflow_reaches() {
    let candidate = r#"nika: grounded
const:
  paths: ["./in/a.txt", "./in/b.txt"]
permits:
  fs: { read: ["./in/**"], write: ["./out/a.md"] }
  tools: ["nika:read", "nika:write", "nika:jq", "mcp:crm/lookup"]
tasks:
  pages:
    for_each: { items: "${{ const.paths }}", max_parallel: 2 }
    invoke:
      tool: "nika:read"
      args: { path: "${{ item }}" }
  helper:
    agent:
      prompt: "look the customers up"
      tools: ["nika:jq", "mcp:crm/lookup", "!nika:fetch"]
  save:
    with: { text: "${{ tasks.pages.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/a.md", content: "${{ with.text }}" }
"#;
    let grounded = grounding(Some(candidate));
    let (text, record) = (&grounded.text, &grounded.record);
    assert_eq!(record["candidate"], json!("parsed"), "{record:#}");
    let tools = json!(["mcp:crm/lookup", "nika:jq", "nika:read", "nika:write"]);
    assert_eq!(record["tools"], tools, "{record:#}");
    assert_eq!(record["uncovered"], json!(["mcp:crm/lookup"]), "{record:#}");
    for heading in ["### `nika:jq`", "### `nika:read`", "### `nika:write`"] {
        assert!(text.contains(heading), "{heading}: {text}");
    }
    assert!(!text.contains("### `nika:fetch`"), "{text}");
    assert!(!text.contains("### `nika:edit`"), "{text}");
    assert!(
        text.contains("`-002` (`overwrite: false` and the path exists)"),
        "{text}"
    );
    assert!(
        text.contains("No contract is embedded for: mcp:crm/lookup."),
        "{text}"
    );
    assert_eq!(record["sha256"], json!(super::knowledge::sha256(text)));
    assert_eq!(record["bytes"], json!(text.len()));
    let write = record["references"]
        .as_array()
        .and_then(|pieces| pieces.iter().find(|p| p["id"] == json!("nika:write")))
        .and_then(|piece| piece["bytes"].as_u64());
    assert!(write.is_some_and(|bytes| bytes > 2_000), "{record:#}");
}

/// A candidate that does not parse selects no contract and says so; with no candidate the
/// reference keeps the conventions and the language only.
#[test]
fn an_unparsed_or_absent_candidate_selects_no_contract() {
    let unparsed = grounding(Some("tasks: ["));
    assert_eq!(unparsed.record["candidate"], json!("unparsed"));
    assert_eq!(unparsed.record["tools"], json!([]));
    assert!(
        unparsed.text.contains("does not parse"),
        "{}",
        unparsed.text
    );
    assert!(!unparsed.text.contains("### `nika:"), "{}", unparsed.text);
    let none = grounding(None);
    assert_eq!(none.record["candidate"], json!("none"));
    assert!(none.text.contains("# The language in one page"));
    assert!(none.text.contains("# Output conventions"));
    assert!(!none.text.contains("does not parse"));
}
