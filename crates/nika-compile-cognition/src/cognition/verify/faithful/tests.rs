// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The whole request against the candidate (R4 A11, R5 R6 and A1), asked of a scripted judge:
//! the authoring provider through the journaled call, or a selected seat. « faithful » carries
//! the request in one question; no admitted answer judges nothing. A doubt asks each part alone
//! as evidence: a part judged missing is a defect only with the reason its task question then
//! gives (the task that fails it, or an operation of its own no task performs, never offered for
//! a prohibition), a part no task fails is contested, a part left without a choice stays unknown,
//! and a call that fails or is refused stops the localization ([`declined`]). When no part is
//! missing the extra-operation question follows; a doubt nothing decides stays contested when
//! the judge rejected the bytes, unknown when it only abstained, unless the judge reads a whole
//! run of these exact bytes ([`observed`]). Every question id, option list and verdict list is
//! asserted exactly.

use std::collections::VecDeque;
use std::iter::repeat_n;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nika_compile::surface::{Binding, Disposition, Judgment};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};

use super::super::{CLAUSE, Declined, Judge, Verdict, WHOLE, parts, state};
use super::{
    EXTRA, OBSERVED, OBSERVED_PART, PART, POINT, POINT_OMITTED, Pointed, RESTRICTING, RUN,
    restricts, task_ids, whole,
};
use crate::authority::{Envelope, Seat as Authority};
use crate::cognition::knowledge::sha256;
use crate::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionError, DecisionSeat};
use crate::plan::Plan;
use crate::{AuthoringPolicy, CompileOutcome, CompileRequest};
use nika_compile_seats::judge::{CREATED, REVISED, REVISED_APPENDED};

use Kind::{Extra, Locate, Observed, ObservedPart, Part, Point, Request};
use Reply::{Choose, Fail, Prose};

/// The parts of one step put to a decision seat together.
mod batched;
/// The questions a localization that stops, and a doubt held by its cause, kept beside this file
/// to bound its size.
mod declined;
/// What the judged bytes hold of the lent catalogue, told to every question judging them.
mod held;
/// What the engine's facts settle of the extra-operation question, and where a doubt nothing
/// else located is asked to be.
mod located;
/// The questions over a run of these bytes.
mod observed;
/// Which part a task question may call an operation no task performs.
mod omittable;
/// A clause a revision's change replaces, over a run of the revised bytes.
mod superseded;

/// The model the authoring provider is seated as when it judges.
const MODEL: &str = "mock/judge";
/// The selected seat's name.
const SEAT: &str = "mock/typed-judge";

/// Three parts: a read, a negation that restricts (« only », « not »), a write.
const ORDERS: &str = "Read ./data/orders.json, keep only the orders that are not cancelled, write them to ./out/open.json.";
const ORDER_PARTS: [&str; 3] = [
    "Read ./data/orders.json",
    "keep only the orders that are not cancelled",
    "write them to ./out/open.json",
];
/// Four parts; the third restricts by negation alone (the field case: a conversion the request
/// rules out, a prohibition).
const CALENDAR: &str = "Read ./data/calendar.json, list the meetings of the week, times are local and need no time-zone conversion, write the list to ./out/week.md.";
const ZONE: &str = "times are local and need no time-zone conversion";
/// Three parts; the last forbids an effect.
const REPORT: &str =
    "Read ./data/report.md, summarize it in three bullets, never send it to the owner.";
const NEVER: &str = "never send it to the owner";
/// The same prohibition, stated with a negative contraction.
const CONTRACTED: &str =
    "Read ./data/report.md, summarize it in three bullets, don't send it to the owner.";
/// One part, which restricts nothing.
const GREETING: &str = "Write the text hello to ./out/result.txt.";
/// Two parts; the second corrects the first.
const CORRECTED: &str = "Write the report to ./out/a.md, actually write it to ./out/b.md instead.";

/// No judgment: what a verdict that does not carry the request holds.
const NO_JUDGMENT: [Judgment; 0] = [];

/// Why a rejected doubt with no run of these bytes stays contested.
const UNOBSERVED: &str = "no trial run of these exact bytes exists in this compile";
/// The extra-operation question left without a choice.
const EXTRA_UNSETTLED: &str =
    "whether any task does something the request does not ask (the judge made no choice)";
/// The extra-operation question whose call got no answer.
const EXTRA_UNANSWERED: &str =
    "whether any task does something the request does not ask (the call got no answer)";
/// The extra-operation question a candidate naming no task is never asked.
const NO_TASK_NAMED: &str =
    "whether any task does something the request does not ask (the candidate names no task)";
/// The extra-operation question a candidate that does not parse is never asked.
const UNPARSED: &str =
    "whether any task does something the request does not ask (the candidate does not parse)";
/// What the localizing question asked of a doubt nothing located says it asks.
const LOCATE: &str = "Locate your doubt.";
/// The defect an extra operation leaves; its note names the task.
const EXTRA_DEFECT: &str = "only what the request asks";
/// The note of a part no task performs.
const OMITTED: &str = "the judge finds no task performing it";
/// The options of every task question over [`CANDIDATE`], in its tasks' order, for a part that
/// may ask an operation of its own.
const POINTER: [&str; 6] = [
    "task-load",
    "task-keep",
    "task-save",
    "omitted",
    "no_task",
    "none",
];
/// The options of a task question over [`CANDIDATE`] for a prohibition: it asks no operation of
/// its own, so no `omitted`.
const PROHIBITED: [&str; 5] = ["task-load", "task-keep", "task-save", "no_task", "none"];

/// The candidate every question reads: three tasks a task question may name.
const CANDIDATE: &str = r#"nika: open-orders
permits:
  fs: { read: ["./data/orders.json"], write: ["./out/open.json"] }
  tools: ["nika:read", "nika:jq", "nika:write"]
tasks:
  load:
    invoke:
      tool: "nika:read"
      args: { path: "./data/orders.json" }
  keep:
    with: { rows: "${{ tasks.load.output }}" }
    invoke:
      tool: "nika:jq"
      args: { input: "${{ with.rows }}", expression: "map(select(.status != \"cancelled\"))" }
  save:
    with: { rows: "${{ tasks.keep.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/open.json", content: "${{ with.rows }}" }
"#;

/// One scripted answer of the provider judge.
#[derive(Clone, Copy, Debug)]
enum Reply {
    /// An answer naming this key.
    Choose(&'static str),
    /// No answer: the call fails.
    Fail,
    /// An answer that is no JSON choice.
    Prose,
}

/// The question a whole-request judgment asks, as the options of its schema name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Request,
    Part,
    Point,
    Extra,
    /// One part asked again over a run of these bytes.
    ObservedPart,
    /// The whole request asked over a run of these bytes.
    Observed,
    /// Where a doubt nothing located is, asked with no run of these bytes.
    Locate,
}

impl Kind {
    /// The question offering `keys`: the whole request over a run offers `consistent` (beside
    /// `unexercised`); a part over a run offers `unexercised` and never `consistent`; where a doubt
    /// is, `unlocated`.
    fn of(keys: &[String]) -> Self {
        let offers = |key: &str| keys.iter().any(|offered| offered == key);
        if offers("consistent") {
            Observed
        } else if offers("unexercised") {
            ObservedPart
        } else if offers("faithful") {
            Request
        } else if offers("carried") {
            Part
        } else if offers("no_task") {
            Point
        } else if offers("only_requested") {
            Extra
        } else if offers("unlocated") {
            Locate
        } else {
            panic!("no whole-request question offers {keys:?}")
        }
    }
}

/// What one request sent the judge: the question's kind, the instructions (the system text) and
/// the state it read.
struct Sent {
    kind: Kind,
    told: String,
    state: Value,
}

/// The authoring provider as the judge: it answers each question from its script in order,
/// after checking that the question's options are those of the scripted kind, and keeps what
/// each request sent. A question the script does not expect panics: nothing is asked beyond it.
struct Scripted {
    script: Mutex<VecDeque<(Kind, Reply)>>,
    sent: Mutex<Vec<Sent>>,
}

impl Scripted {
    fn new(script: impl IntoIterator<Item = (Kind, Reply)>) -> Self {
        Self {
            script: Mutex::new(script.into_iter().collect()),
            sent: Mutex::new(Vec::new()),
        }
    }

    /// The scripted answers no question asked for.
    fn left(&self) -> usize {
        self.script.lock().unwrap().len()
    }

    /// The kinds of the questions asked, in order.
    fn kinds(&self) -> Vec<Kind> {
        (self.sent.lock().unwrap().iter())
            .map(|sent| sent.kind)
            .collect()
    }
}

/// The text of the message at `at` of `request`.
fn said(request: &InferRequest, at: usize) -> String {
    match request.messages.get(at).and_then(|m| m.content.first()) {
        Some(ContentBlock::Text { text }) => text.clone(),
        _ => String::new(),
    }
}

impl ProviderInferDyn for Scripted {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let ResponseFormat::JsonSchema(schema) = &request.response_format else {
            panic!("a judge question is a closed choice");
        };
        let keys: Vec<String> =
            serde_json::from_value(schema["properties"]["choice"]["enum"].clone()).unwrap();
        let kind = Kind::of(&keys);
        let (scripted, reply) = (self.script.lock().unwrap().pop_front())
            .unwrap_or_else(|| panic!("an unscripted {kind:?} question: {keys:?}"));
        assert_eq!(kind, scripted, "{keys:?}");
        let user = said(&request, 1);
        let state: Value = (user.strip_prefix("STATE:\n"))
            .and_then(|rest| rest.split_once("\n\nOPTIONS:\n"))
            .map(|(state, _)| serde_json::from_str(state).unwrap())
            .unwrap();
        let told = said(&request, 0);
        self.sent.lock().unwrap().push(Sent { kind, told, state });
        let text = match reply {
            Choose(key) => json!({"choice": key}).to_string(),
            Prose => "The workflow looks right to me.".to_owned(),
            Fail => {
                return Err(ProviderError::Other {
                    reason: "the judge is unreachable".to_owned(),
                });
            }
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// One scripted answer of the selected seat: the question id it expects, then its choice or
/// its failure.
type SeatReply = (&'static str, Result<&'static str, &'static str>);

/// A selected seat: it answers the questions its script names, in order, and keeps every
/// question; a question the script does not expect panics.
struct Seated {
    script: Mutex<VecDeque<SeatReply>>,
    asked: Mutex<Vec<ChoiceQuestion>>,
}

impl Seated {
    fn new(script: impl IntoIterator<Item = SeatReply>) -> Self {
        Self {
            script: Mutex::new(script.into_iter().collect()),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn asked(&self) -> Vec<ChoiceQuestion> {
        self.asked.lock().unwrap().clone()
    }

    /// The scripted answers no question asked for.
    fn left(&self) -> usize {
        self.script.lock().unwrap().len()
    }
}

impl DecisionSeat for Seated {
    fn name(&self) -> &str {
        SEAT
    }

    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.clone());
            let (id, answer) = (self.script.lock().unwrap().pop_front())
                .unwrap_or_else(|| panic!("an unscripted question: {}", question.id));
            assert_eq!(question.id, id);
            answer
                .map(|key| ChoiceAnswer::new(key, SEAT))
                .map_err(|why| DecisionError(why.to_owned()))
        })
    }
}

/// One whole-request judgment and what it left.
struct Judged {
    verdict: Verdict,
    out: CompileOutcome,
    binding: Binding,
}

/// The whole verdict of `intent` under `request` over `candidate`, asked of `judge`
/// (`observation`: a run of these bytes the caller observed).
async fn judged<P: ProviderInferDyn>(
    intent: &str,
    request: &CompileRequest,
    candidate: &str,
    judge: &Judge<'_, P>,
    observation: Option<&Value>,
) -> Judged {
    let base = state(intent, request, candidate);
    let binding = Binding::of(intent, request, &Plan::default(), candidate);
    let mut verdict = Verdict::default();
    let mut out = crate::initial();
    let asked = (&base, "fixture");
    whole(
        intent,
        asked,
        judge,
        &binding,
        observation,
        &mut verdict,
        &mut out,
    )
    .await;
    Judged {
        verdict,
        out,
        binding,
    }
}

/// The whole verdict of the created `intent` over [`CANDIDATE`], asked of the provider judge.
async fn provided(intent: &str, judge: &Scripted, observation: Option<&Value>) -> Judged {
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let request = CompileRequest::create(intent);
    let provider = Judge::Provider(&policy, judge);
    judged(intent, &request, CANDIDATE, &provider, observation).await
}

/// The ids of the questions asked, in order.
fn ids(verdict: &Verdict) -> Vec<&str> {
    (verdict.records.iter())
        .filter_map(|record| record["question"].as_str())
        .collect()
}

/// The roles the questions were asked under, in order.
fn roles(verdict: &Verdict) -> Vec<&str> {
    (verdict.records.iter())
        .filter_map(|record| record["role"].as_str())
        .collect()
}

/// The record of the question `id`.
fn record<'v>(verdict: &'v Verdict, id: &str) -> &'v Value {
    (verdict.records.iter())
        .find(|record| record["question"] == id)
        .unwrap_or_else(|| panic!("no question {id}: {:?}", verdict.records))
}

/// The calls attempted, the answers returned and the answers consumed.
fn counts(verdict: &Verdict) -> (u32, u32, u32) {
    (verdict.attempted, verdict.returned, verdict.consumed)
}

/// The whole request carried by the answer to `question` of `seat`, under `binding`.
fn carried(intent: &str, question: &str, seat: &str, binding: &Binding) -> Judgment {
    let whole = (0, intent.len());
    Judgment::new(
        intent,
        whole,
        Disposition::Carried,
        seat,
        question,
        binding.clone(),
    )
}

/// A verdict's lists, as the decision record writes them.
fn lists(verdict: &Verdict) -> Value {
    let notes: Vec<Value> = (verdict.notes.iter())
        .map(|(defect, note)| json!({"defect": defect, "note": note}))
        .collect();
    json!({
        "defects": verdict.defects,
        "notes": notes,
        "unknown": verdict.unknown,
        "contested": verdict.contested,
        "doubt": verdict.doubt,
        "unsettled": verdict.unsettled,
    })
}

/// The lists a verdict is expected to hold: each defect with the note the judge gave it, what
/// stayed unknown, what is contested, the whole request's doubt and why nothing settled it.
fn found(
    defects: &[(&str, &str)],
    unknown: &[&str],
    contested: &[&str],
    doubt: &[&str],
    unsettled: &[&str],
) -> Value {
    let notes: Vec<Value> = (defects.iter())
        .map(|(defect, note)| json!({"defect": defect, "note": note}))
        .collect();
    let defects: Vec<&str> = defects.iter().map(|(defect, _)| *defect).collect();
    json!({"defects": defects, "notes": notes, "unknown": unknown, "contested": contested,
        "doubt": doubt, "unsettled": unsettled})
}

/// The note of a part the judge points at `task` for.
fn points(task: &str) -> String {
    format!("the judge points to the task {task}")
}

/// A run of [`CANDIDATE`] as the sketch door observes it: the input it read and the output it
/// wrote, each read whole or only in part.
fn observed(input_whole: bool, output_whole: bool) -> Value {
    json!({
        "candidate_sha256": sha256(CANDIDATE),
        "inputs": [{"path": "./data/orders.json",
            "text": r#"[{"id":1,"status":"open"},{"id":2,"status":"cancelled"}]"#,
            "read_whole": input_whole}],
        "outputs": [{"path": "./out/open.json", "text": r#"[{"id":1,"status":"open"}]"#,
            "written": true, "read_whole": output_whole}],
    })
}

/// A doubt (`doubt`) no part locates: each of `count` parts carried, then `last` when given.
/// Over [`CANDIDATE`] and [`ORDERS`] the engine's facts then settle every task (its read is the
/// request's source, its write the output a carried part states), so no extra question is asked.
fn undisputed(
    doubt: &'static str,
    count: usize,
    last: Option<(Kind, Reply)>,
) -> Vec<(Kind, Reply)> {
    let mut script = vec![(Request, Choose(doubt))];
    script.extend(repeat_n((Part, Choose("carried")), count));
    script.extend(last);
    script
}

/// [`CANDIDATE`] writing `./out/elsewhere.json`, a path the request never names: its `save` is
/// the one task the engine's facts leave open.
fn elsewhere() -> String {
    CANDIDATE.replace("./out/open.json", "./out/elsewhere.json")
}

/// An unfaithful request of `count` parts whose part `at` is judged missing, its task question
/// answered `pointed`; every other part carried.
fn pointing(count: usize, at: usize, pointed: Reply) -> Vec<(Kind, Reply)> {
    let mut script = vec![(Request, Choose("unfaithful"))];
    for k in 0..count {
        if k == at {
            script.extend([(Part, Choose("missing")), (Point, pointed)]);
        } else {
            script.push((Part, Choose("carried")));
        }
    }
    script
}

/// « faithful » carries the whole request in one question when no run proves whole outputs (here
/// a run read only in part): one Carried judgment of the whole span under the candidate's
/// binding, named by the question that settled it. Nothing else is asked, nothing is doubted,
/// and the verdict names the request it asked. Over a whole run, see [`observed`].
#[tokio::test]
async fn a_faithful_request_is_carried_by_its_one_question() {
    let judge = Scripted::new([(Request, Choose("faithful"))]);
    let Judged {
        verdict, binding, ..
    } = provided(ORDERS, &judge, Some(&observed(true, false))).await;
    let judgment = carried(ORDERS, "verify-request", MODEL, &binding);
    assert_eq!(verdict.judgments, [judgment]);
    assert_eq!(ids(&verdict), ["verify-request"]);
    assert_eq!(roles(&verdict), ["judge_request"]);
    let asked = &verdict.records[0];
    assert_eq!(asked["options"], json!(["faithful", "unfaithful", "none"]));
    assert_eq!(asked["choice"], "faithful");
    assert_eq!(lists(&verdict), found(&[], &[], &[], &[], &[]));
    assert_eq!(counts(&verdict), (1, 1, 1));
    assert_eq!(verdict.settled_by, Some("verify-request"));
    assert!(verdict.whole_asked);
    assert_eq!(verdict.request.as_deref(), Some(ORDERS));
    assert_eq!(verdict.declined, Declined::No);
    assert!(!verdict.stopped);
    assert!(verdict.settled() && !verdict.doubted());
    assert_eq!(judge.left(), 0);
}

/// A whole-request call that returns no admitted choice judges nothing: a failed call, an
/// answer that is no JSON choice and a choice no option offers each leave the request unknown,
/// with the one record naming why, and nothing further is asked of that judge. Nothing is
/// declined: no answer of the judge stands against these bytes. Only the call that got no answer
/// stops the verdict.
#[tokio::test]
async fn a_whole_request_without_an_admitted_choice_is_unknown_and_asks_nothing_more() {
    let cases = [
        (
            Fail,
            0,
            "the judge call got no answer; the receipt says why",
        ),
        (
            Prose,
            1,
            "the seat answer is not JSON: expected value at line 1 column 1",
        ),
        (
            Choose("invented"),
            1,
            "the seat chose `invented`, which was not offered",
        ),
    ];
    for (reply, returned, error) in cases {
        let judge = Scripted::new([(Request, reply)]);
        let Judged { verdict, out, .. } = provided(ORDERS, &judge, None).await;
        assert_eq!(ids(&verdict), ["verify-request"], "{reply:?}");
        assert_eq!(verdict.records[0]["error"], error, "{reply:?}");
        assert_eq!(verdict.records[0].get("choice"), None, "{reply:?}");
        let unknown = found(&[], &[ORDERS], &[], &[], &[]);
        assert_eq!(lists(&verdict), unknown, "{reply:?}");
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert_eq!(counts(&verdict), (1, returned, 0), "{reply:?}");
        assert_eq!(verdict.settled_by, None);
        assert_eq!(verdict.declined, Declined::No, "{reply:?}");
        assert_eq!(verdict.stopped, returned == 0, "{reply:?}");
        assert!(!verdict.settled() && !verdict.doubted(), "{reply:?}");
        // Only the failed call leaves the provider's finding; an answer that came back is
        // judged on its record alone.
        let failed = (out.diagnostics.iter())
            .filter(|d| d.target == "authoring_provider")
            .count();
        assert_eq!(failed, usize::from(matches!(reply, Fail)), "{reply:?}");
        assert_eq!(judge.left(), 0);
    }
}

/// « unfaithful » asks each part alone. A part judged missing asks at once why: the task that
/// fails it, or an operation no task performs. Either answer makes it a defect in the request's
/// own words, the reason kept beside it as its note; the other parts are still asked, and a
/// located defect asks neither the extra question nor the run of these bytes, though one was
/// observed (a plain part's defect is never asked again over the run). The task question offers
/// every task of the candidate in its order.
#[tokio::test]
async fn an_unfaithful_request_whose_plain_part_is_missing_names_that_part_and_asks_no_extra() {
    assert_eq!(parts(ORDERS), ORDER_PARTS);
    let named = points("save");
    for (pointed, note) in [("task-save", named.as_str()), ("omitted", OMITTED)] {
        let judge = Scripted::new([
            (Request, Choose("unfaithful")),
            (Part, Choose("carried")),
            (Part, Choose("carried")),
            (Part, Choose("missing")),
            (Point, Choose(pointed)),
        ]);
        let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observed(true, true))).await;
        let asked = [
            "verify-request",
            "verify-part-0",
            "verify-part-1",
            "verify-part-2",
            "verify-point-2",
        ];
        assert_eq!(ids(&verdict), asked);
        let roles_asked = [
            "judge_request",
            "judge_part",
            "judge_part",
            "judge_part",
            "judge_point",
        ];
        assert_eq!(roles(&verdict), roles_asked);
        let point = record(&verdict, "verify-point-2");
        assert_eq!(point["options"], json!(POINTER));
        assert_eq!(point["choice"], pointed);
        let clause = json!({"text": ORDER_PARTS[2], "restricts": false});
        assert_eq!(
            (&point["clause"], point.get("observation")),
            (&clause, None)
        );
        let located = found(&[(ORDER_PARTS[2], note)], &[], &[], &["unfaithful"], &[]);
        assert_eq!(lists(&verdict), located, "{pointed}");
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert_eq!(counts(&verdict), (5, 5, 5));
        assert_eq!(verdict.declined, Declined::Rejected);
        assert!(verdict.doubted() && !verdict.settled());
        // The task question reads the part alone, never the run, and is told what it asks.
        let sent = judge.sent.lock().unwrap();
        let alone = json!({"text": ORDER_PARTS[2]});
        assert_eq!(sent[4].state["clause"], alone);
        assert_eq!(sent[4].state.get("observation"), None);
        assert!(sent[4].told.contains(POINT), "{}", sent[4].told);
        assert!(sent[4].told.contains(POINT_OMITTED), "{}", sent[4].told);
        assert!(!sent[4].told.contains(RESTRICTING), "{}", sent[4].told);
        drop(sent);
        assert_eq!(judge.left(), 0);
    }
}

/// A whole-request NONE declines these bytes as « unfaithful » does: recorded as the doubt, never
/// consumed as a verdict, and localized part by part; here the read is the part the candidate
/// misses, the judge pointing to the task that fails it, which rejects them.
#[tokio::test]
async fn a_whole_request_none_is_a_doubt_localized_as_unfaithful_is() {
    let judge = Scripted::new([
        (Request, Choose("none")),
        (Part, Choose("missing")),
        (Point, Choose("task-load")),
        (Part, Choose("carried")),
        (Part, Choose("carried")),
    ]);
    let Judged { verdict, .. } = provided(ORDERS, &judge, None).await;
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-point-0",
        "verify-part-1",
        "verify-part-2",
    ];
    assert_eq!(ids(&verdict), asked);
    let note = points("load");
    let located = found(&[(ORDER_PARTS[0], note.as_str())], &[], &[], &["none"], &[]);
    assert_eq!(lists(&verdict), located);
    assert_eq!(counts(&verdict), (5, 5, 4));
    assert_eq!(verdict.declined, Declined::Rejected);
    assert!(verdict.doubted());
    assert_eq!(judge.left(), 0);
}

/// A restriction judged missing is a defect only with the task the judge names as doing what
/// it forbids or ignoring it. The task question follows the part at once, is told the clause
/// restricts, offers each task of the candidate in order and `no_task`, and its record names the
/// clause. The field's time-zone clause and a prohibition of an effect ask no operation of their
/// own: never offered `omitted`, nor told what it means. Each pointed at a task is a defect
/// naming that task; no extra question follows.
#[tokio::test]
async fn a_restriction_judged_missing_is_a_defect_with_the_task_the_judge_names() {
    assert_eq!(task_ids(CANDIDATE), ["load", "keep", "save"]);
    let cases = [
        (CALENDAR, ZONE, 4, "task-keep", "keep"),
        (REPORT, NEVER, 3, "task-save", "save"),
    ];
    for (intent, clause, count, pointer, task) in cases {
        let split = parts(intent);
        assert_eq!((split.len(), split[2].as_str()), (count, clause));
        let judge = Scripted::new(pointing(count, 2, Choose(pointer)));
        let Judged { verdict, .. } = provided(intent, &judge, None).await;
        let mut asked: Vec<String> = (0..count).map(|k| format!("verify-part-{k}")).collect();
        asked.insert(3, "verify-point-2".to_owned());
        asked.insert(0, "verify-request".to_owned());
        assert_eq!(ids(&verdict), asked);
        let point = record(&verdict, "verify-point-2");
        assert_eq!(point["role"], "judge_point");
        assert_eq!(point["options"], json!(PROHIBITED));
        assert_eq!(point["choice"], pointer);
        assert_eq!(point["clause"], json!({"text": clause, "restricts": true}));
        let note = points(task);
        let located = found(&[(clause, note.as_str())], &[], &[], &["unfaithful"], &[]);
        assert_eq!(lists(&verdict), located, "{intent}");
        assert_eq!(verdict.consumed, u32::try_from(count + 2).unwrap());
        let sent = judge.sent.lock().unwrap();
        assert_eq!(sent[4].kind, Point);
        assert!(sent[4].told.contains(POINT) && sent[4].told.contains(RESTRICTING));
        assert!(!sent[4].told.contains(POINT_OMITTED), "{}", sent[4].told);
        drop(sent);
        assert_eq!(judge.left(), 0);
    }
}

/// A prohibition judged missing is never a defect « no task performs it » (the field failure: a
/// conversion the request rules out, which no repair can add): a judge choosing `omitted` anyway
/// is refused on admission, so the part stays unknown and the localization goes on. A
/// conditional (« if a row has no email, skip it ») restricts but may ask an operation of its own
/// (the filter no task performs): it is offered `omitted`, told what it means, and that answer
/// names it.
#[tokio::test]
async fn a_prohibition_is_never_offered_omitted_and_a_conditional_is() {
    let mut script = pointing(4, 2, Choose("omitted"));
    script.push((Extra, Choose("only_requested")));
    let judge = Scripted::new(script);
    let Judged { verdict, .. } = provided(CALENDAR, &judge, None).await;
    let point = record(&verdict, "verify-point-2");
    assert_eq!(point["options"], json!(PROHIBITED));
    let refused = "the seat chose `omitted`, which was not offered";
    assert_eq!(point["error"], refused);
    let undecided = found(&[], &[ZONE], &[CALENDAR], &["unfaithful"], &[UNOBSERVED]);
    assert_eq!(lists(&verdict), undecided);
    assert_eq!(counts(&verdict), (7, 7, 6));
    assert_eq!(judge.left(), 0);
    let conditional = "if a row has no email, skip it";
    assert!(restricts(conditional));
    let judge = Scripted::new([(Point, Choose("omitted"))]);
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let provider = Judge::Provider(&policy, &judge);
    let base = state(conditional, &CompileRequest::create(conditional), CANDIDATE);
    let tasks = task_ids(CANDIDATE);
    let mut verdict = Verdict::default();
    let mut out = crate::initial();
    let asked = (&base, "fixture");
    let pointed = super::point(
        "verify-point-0",
        conditional,
        &tasks,
        asked,
        &provider,
        &mut verdict,
        &mut out,
    );
    assert!(matches!(pointed.await, Some(Pointed::Defect(note)) if note == OMITTED));
    let asked = &verdict.records[0];
    assert_eq!(asked["options"], json!(POINTER));
    assert_eq!(
        asked["clause"],
        json!({"text": conditional, "restricts": true})
    );
    assert_eq!(counts(&verdict), (1, 1, 1));
    let sent = judge.sent.lock().unwrap();
    assert!(sent[0].told.contains(POINT_OMITTED), "{}", sent[0].told);
    assert!(sent[0].told.contains(RESTRICTING), "{}", sent[0].told);
}

/// A computation the request states (« sum qty over the rows where status is shipped ») asks an
/// operation of its own: judged missing, its task question offers `omitted` (no task performs
/// it), so a candidate that never computes it is a defect a repair starts from, never a part
/// left contested.
#[tokio::test]
async fn a_computation_part_judged_missing_may_be_an_operation_no_task_performs() {
    let intent = "Read ./data/orders.json, sum qty over the rows where status is shipped, write the sum to ./out/open.json.";
    let sum = "sum qty over the rows where status is shipped";
    assert_eq!(parts(intent)[1], sum);
    let judge = Scripted::new([(Point, Choose("omitted"))]);
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let provider = Judge::Provider(&policy, &judge);
    let base = state(intent, &CompileRequest::create(intent), CANDIDATE);
    let tasks = task_ids(CANDIDATE);
    let (mut verdict, mut out) = (Verdict::default(), crate::initial());
    let asked = (&base, "fixture");
    let pointed = super::point(
        "verify-point-1",
        sum,
        &tasks,
        asked,
        &provider,
        &mut verdict,
        &mut out,
    );
    let pointed = pointed.await;
    assert_eq!(verdict.records[0]["options"], json!(POINTER));
    assert!(matches!(pointed, Some(Pointed::Defect(note)) if note == OMITTED));
    let judge = Scripted::new(pointing(3, 1, Choose("omitted")));
    let Judged { verdict, .. } = provided(intent, &judge, None).await;
    let located = found(&[(sum, OMITTED)], &[], &[], &["unfaithful"], &[]);
    assert_eq!(lists(&verdict), located);
    assert_eq!(judge.left(), 0);
}

/// A part judged missing that the judge then finds no task failing is contested, never a defect
/// a repair could start from; a task question left without a choice (NONE) keeps it unknown and
/// the localization goes on. Either way the extra question follows, and with no run of these
/// bytes the request itself stays contested: the judge rejected them.
#[tokio::test]
async fn a_restriction_no_task_violates_is_contested_and_an_unpointed_one_unknown() {
    let cases = [
        (
            Choose("no_task"),
            json!([]),
            json!([ZONE, CALENDAR]),
            (7, 7, 7),
        ),
        (Choose("none"), json!([ZONE]), json!([CALENDAR]), (7, 7, 6)),
    ];
    for (pointer, unknown, contested, calls) in cases {
        let mut script = pointing(4, 2, pointer);
        script.push((Extra, Choose("only_requested")));
        let judge = Scripted::new(script);
        let Judged { verdict, .. } = provided(CALENDAR, &judge, None).await;
        let asked = [
            "verify-request",
            "verify-part-0",
            "verify-part-1",
            "verify-part-2",
            "verify-point-2",
            "verify-part-3",
            "verify-extra",
        ];
        assert_eq!(ids(&verdict), asked, "{pointer:?}");
        let point = record(&verdict, "verify-point-2");
        assert_eq!(point["options"], json!(PROHIBITED), "{pointer:?}");
        let disputed = json!({"defects": [], "notes": [], "unknown": unknown,
            "contested": contested, "doubt": ["unfaithful"], "unsettled": [UNOBSERVED]});
        assert_eq!(lists(&verdict), disputed, "{pointer:?}");
        assert_eq!(counts(&verdict), calls, "{pointer:?}");
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert_eq!(verdict.declined, Declined::Rejected);
        assert!(!verdict.settled() && !verdict.stopped);
        assert_eq!(judge.left(), 0);
    }
}

/// The options a part is asked under: carried, missing, `superseded` only for a part a later one
/// can supersede (never the last), `no_operation` only for a part that restricts nothing of a
/// request of several parts, then none.
fn part_options(k: usize, count: usize, restricting: bool) -> Value {
    let mut options = vec!["carried", "missing"];
    if k + 1 < count {
        options.push("superseded");
    }
    if count > 1 && !restricting {
        options.push("no_operation");
    }
    options.push("none");
    json!(options)
}

/// `no_operation` is offered only to a part that restricts nothing, of a request of several
/// parts: never to a restriction (the time-zone clause, the negation « only … not cancelled »,
/// a prohibition stated « don't »), never to the one part of a request of one; `superseded` only
/// to a part a later one follows, never to the last. A plain part asking no operation is settled,
/// never a defect; each part's record says whether its clause restricts, and only a restriction
/// is told so.
#[tokio::test]
async fn no_operation_is_offered_only_to_a_plain_part_of_a_request_of_several() {
    let cases: [(&str, &[bool]); 4] = [
        (CALENDAR, &[false, false, true, false]),
        (ORDERS, &[false, true, false]),
        (CONTRACTED, &[false, false, true]),
        (GREETING, &[false]),
    ];
    let last = json!(["carried", "missing", "no_operation", "none"]);
    assert_eq!(part_options(3, 4, false), last);
    assert_eq!(
        part_options(0, 1, false),
        json!(["carried", "missing", "none"])
    );
    for (intent, restricting) in cases {
        let split = parts(intent);
        assert_eq!(split.len(), restricting.len(), "{intent}");
        let several = split.len() > 1;
        let mut script = vec![(Request, Choose("unfaithful"))];
        for &restricts in restricting {
            let plain = several && !restricts;
            let answer = if plain { "no_operation" } else { "carried" };
            script.push((Part, Choose(answer)));
        }
        // A task the facts leave open (a path these requests never name, or the write of a part
        // asking nothing) is asked about; nothing located, the doubt is asked where it is.
        script.push((Extra, Choose("only_requested")));
        script.push((Locate, Choose("unlocated")));
        let judge = Scripted::new(script);
        let Judged { verdict, .. } = provided(intent, &judge, None).await;
        let sent = judge.sent.lock().unwrap();
        for (k, (&restricts, part)) in restricting.iter().zip(&split).enumerate() {
            let asked = record(&verdict, &format!("verify-part-{k}"));
            let options = part_options(k, split.len(), restricts);
            assert_eq!(asked["options"], options, "{intent}: {part}");
            let clause = json!({"text": part, "restricts": restricts});
            assert_eq!(asked["clause"], clause, "{intent}");
            let told = &sent[k + 1].told;
            assert_eq!(told.contains(RESTRICTING), restricts, "{intent}: {part}");
        }
        drop(sent);
        let disputed = found(&[], &[], &[intent], &["unfaithful"], &[UNOBSERVED]);
        assert_eq!(lists(&verdict), disputed, "{intent}");
        assert_eq!(judge.left(), 0);
    }
}

/// A part answered without a choice that the call still returned (NONE, an answer that is no
/// JSON choice, a choice no option offers) stays unknown: never a defect, never carried, and no
/// task question is asked. The localization goes on: with no part missing the extra question
/// is still asked, and with no run of these bytes the rejected request stays contested beside
/// them.
#[tokio::test]
async fn a_part_left_without_a_choice_is_unknown_never_a_defect() {
    let judge = Scripted::new([
        (Request, Choose("unfaithful")),
        (Part, Choose("none")),
        (Part, Prose),
        (Part, Choose("invented")),
        (Extra, Choose("only_requested")),
    ]);
    let Judged { verdict, .. } = provided(ORDERS, &judge, None).await;
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-part-2",
        "verify-extra",
    ];
    assert_eq!(ids(&verdict), asked);
    let errors = [
        None,
        Some("the seat answer is not JSON: expected value at line 1 column 1"),
        Some("the seat chose `invented`, which was not offered"),
    ];
    for (k, error) in errors.into_iter().enumerate() {
        let part = record(&verdict, &format!("verify-part-{k}"));
        assert_eq!(part["error"].as_str(), error, "{k}");
        let restricts = k == 1;
        let clause = json!({"text": ORDER_PARTS[k], "restricts": restricts});
        assert_eq!(part["clause"], clause);
    }
    let unsettled = found(&[], &ORDER_PARTS, &[ORDERS], &["unfaithful"], &[UNOBSERVED]);
    assert_eq!(lists(&verdict), unsettled);
    assert_eq!(verdict.judgments, NO_JUDGMENT);
    assert_eq!(counts(&verdict), (5, 5, 2));
    assert_eq!(judge.left(), 0);
}

/// A part call that fails or is refused (no answer came back) stops the localization: that part
/// and every later one stay unknown, then the request, never contested; no extra question and no
/// run is asked, nothing more of that judge. A defect located before the stop stays a defect, and
/// then the request is not named unknown. An authority that refuses the call stops it the same
/// way, before any byte leaves.
#[tokio::test]
async fn a_failed_or_refused_part_call_stops_the_localization() {
    let judge = Scripted::new([
        (Request, Choose("unfaithful")),
        (Part, Choose("carried")),
        (Part, Fail),
    ]);
    let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observed(true, true))).await;
    assert_eq!(
        ids(&verdict),
        ["verify-request", "verify-part-0", "verify-part-1"]
    );
    let failed = record(&verdict, "verify-part-1");
    let error = "the judge call got no answer; the receipt says why";
    assert_eq!(failed["error"], error);
    let clause = json!({"text": ORDER_PARTS[1], "restricts": true});
    assert_eq!(failed["clause"], clause);
    let stopped = [ORDER_PARTS[1], ORDER_PARTS[2], ORDERS];
    let unknown = found(&[], &stopped, &[], &["unfaithful"], &[]);
    assert_eq!(lists(&verdict), unknown);
    assert_eq!(counts(&verdict), (3, 2, 2));
    assert_eq!(verdict.judgments, NO_JUDGMENT);
    assert!(verdict.stopped && verdict.doubted() && !verdict.settled());
    assert_eq!(judge.left(), 0);
    // A defect located before the stop.
    let judge = Scripted::new([
        (Request, Choose("unfaithful")),
        (Part, Choose("missing")),
        (Point, Choose("task-load")),
        (Part, Fail),
    ]);
    let Judged { verdict, .. } = provided(ORDERS, &judge, None).await;
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-point-0",
        "verify-part-1",
    ];
    assert_eq!(ids(&verdict), asked);
    let note = points("load");
    let located = found(
        &[(ORDER_PARTS[0], note.as_str())],
        &ORDER_PARTS[1..],
        &[],
        &["unfaithful"],
        &[],
    );
    assert_eq!(lists(&verdict), located);
    assert_eq!(counts(&verdict), (4, 3, 3));
    assert!(verdict.stopped);
    assert_eq!(judge.left(), 0);
    // A refused call, after a whole-request NONE.
    let inner = Scripted::new([(Request, Choose("none")), (Part, Choose("none"))]);
    let authority = Arc::new(Envelope::new(2, "authorize more judge calls"));
    let seat = Authority::new(inner, authority.clone());
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let provider = Judge::Provider(&policy, &seat);
    let request = CompileRequest::create(ORDERS);
    let Judged { verdict, out, .. } = judged(ORDERS, &request, CANDIDATE, &provider, None).await;
    assert_eq!(authority.account(), json!({"sent": 2, "refused": 1}));
    assert_eq!(
        ids(&verdict),
        ["verify-request", "verify-part-0", "verify-part-1"]
    );
    let refused = [ORDER_PARTS[0], ORDER_PARTS[1], ORDER_PARTS[2], ORDERS];
    assert_eq!(lists(&verdict), found(&[], &refused, &[], &["none"], &[]));
    assert_eq!(counts(&verdict), (3, 2, 0));
    assert_eq!(verdict.declined, Declined::Abstained);
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let last = &receipt.context.last().unwrap()["result"]["failure_kind"];
    assert_eq!(last, "admission_refused");
    assert_eq!(seat.inner().left(), 0);
}

/// A whole-request NONE whose every part and extra question are answered NONE again judged
/// nothing anywhere: the request stays unknown beside its parts, never contested, and no reason
/// is recorded for a disagreement that never was. Over a whole run of these bytes each part left
/// open is asked again, and answered NONE there changes nothing: the whole request is never asked
/// over a run while a part stays open. The same abstention whose parts are all carried and whose
/// extra question names no task still only abstained: unknown, never contested.
#[tokio::test]
async fn a_whole_none_left_without_any_choice_stays_unknown_never_contested() {
    let observation = observed(true, true);
    for over_the_run in [false, true] {
        let mut script = vec![(Request, Choose("none"))];
        script.extend(repeat_n((Part, Choose("none")), 3));
        let mut asked = vec![
            "verify-request",
            "verify-part-0",
            "verify-part-1",
            "verify-part-2",
        ];
        if over_the_run {
            script.extend(repeat_n((ObservedPart, Choose("none")), 3));
            asked.extend([
                "verify-observed-part-0",
                "verify-observed-part-1",
                "verify-observed-part-2",
            ]);
        }
        script.push((Extra, Choose("none")));
        asked.push("verify-extra");
        let judge = Scripted::new(script);
        let shown = over_the_run.then_some(&observation);
        let Judged { verdict, .. } = provided(ORDERS, &judge, shown).await;
        assert_eq!(ids(&verdict), asked);
        let mut unknown = ORDER_PARTS.to_vec();
        unknown.extend([EXTRA_UNSETTLED, ORDERS]);
        let abstained = found(&[], &unknown, &[], &["none"], &[]);
        assert_eq!(lists(&verdict), abstained, "{over_the_run}");
        let calls = u32::try_from(asked.len()).unwrap();
        assert_eq!(counts(&verdict), (calls, calls, 0));
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        // The judge answered (NONE): its abstention stands against these bytes.
        assert_eq!(verdict.declined, Declined::Abstained);
        assert!(verdict.doubted() && !verdict.settled());
        assert_eq!(judge.left(), 0);
    }
    let mut script = vec![(Request, Choose("none"))];
    script.extend(repeat_n((Part, Choose("none")), 3));
    script.push((Extra, Choose("only_requested")));
    let judge = Scripted::new(script);
    let Judged { verdict, .. } = provided(ORDERS, &judge, None).await;
    let mut unknown = ORDER_PARTS.to_vec();
    unknown.push(ORDERS);
    assert_eq!(lists(&verdict), found(&[], &unknown, &[], &["none"], &[]));
    assert_eq!(counts(&verdict), (5, 5, 1));
    assert!(!verdict.rejected() && verdict.doubted());
    assert_eq!(judge.left(), 0);
}

/// A doubt no part locates and no task explains is a disagreement (R6): with no run of these
/// bytes it stays contested, its reason recorded, with no judgment: never READY. A candidate
/// that names no task, or does not parse, is not asked the extra question, which stays unknown
/// with that reason.
#[tokio::test]
async fn a_disagreement_without_an_observation_is_contested_never_ready() {
    let judge = Scripted::new(undisputed(
        "unfaithful",
        3,
        Some((Locate, Choose("unlocated"))),
    ));
    let Judged { verdict, .. } = provided(ORDERS, &judge, None).await;
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-part-2",
        "verify-doubt",
    ];
    assert_eq!(ids(&verdict), asked);
    assert_eq!(record(&verdict, "verify-doubt")["choice"], "unlocated");
    let disputed = found(&[], &[], &[ORDERS], &["unfaithful"], &[UNOBSERVED]);
    assert_eq!(lists(&verdict), disputed);
    assert_eq!(verdict.judgments, NO_JUDGMENT);
    assert!(!verdict.settled() && verdict.doubted() && verdict.unresolved());
    assert_eq!(counts(&verdict), (5, 5, 5));
    assert_eq!(judge.left(), 0);
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let request = CompileRequest::create(ORDERS);
    for (candidate, why) in [("tasks: [", UNPARSED), ("nika: empty\n", NO_TASK_NAMED)] {
        assert_eq!(task_ids(candidate), Vec::<String>::new());
        let mut script = vec![(Request, Choose("unfaithful"))];
        script.extend(repeat_n((Part, Choose("carried")), 3));
        let judge = Scripted::new(script);
        let provider = Judge::Provider(&policy, &judge);
        let Judged { verdict, .. } = judged(ORDERS, &request, candidate, &provider, None).await;
        assert_eq!(ids(&verdict), asked[..4], "{candidate}");
        let disputed = found(&[], &[why], &[ORDERS], &["unfaithful"], &[UNOBSERVED]);
        assert_eq!(lists(&verdict), disputed, "{candidate}");
        assert_eq!(judge.left(), 0);
    }
}

/// What every question of `kind` says it asks.
fn asks(kind: Kind) -> &'static [&'static str] {
    match kind {
        Request => &[WHOLE],
        Part => &[CLAUSE, PART],
        Point => &[POINT],
        Extra => &[EXTRA],
        ObservedPart => &[RUN, OBSERVED_PART],
        Observed => &[RUN, OBSERVED],
        Locate => &[LOCATE],
    }
}

/// Every question of a whole-request judgment carries the framing of what is judged: over a
/// created workflow, what a request to author it asks of its bytes; over a revision, which
/// request is asked and which is history; never both. Each also says what it asks: the whole
/// request, a part asked alone (told it restricts only when it does), the task a part judged
/// missing names (told the same), the part left open asked again over the run, the extra
/// operation, the whole request over the run and the task a part named in the run names.
#[tokio::test]
async fn every_question_is_framed_as_a_creation_or_a_revision() {
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let created = CompileRequest::create(CALENDAR);
    let change = "Also list the meetings of the week.";
    let revised = CompileRequest::edit(CANDIDATE, change)
        .with_original_intent("Read ./data/calendar.json, write the list to ./out/week.md.");
    let observation = observed(true, true);
    for (request, framing, never) in [(&created, CREATED, REVISED), (&revised, REVISED, CREATED)] {
        let mut script = pointing(4, 2, Choose("no_task"));
        script.extend([
            (ObservedPart, Choose("carried")),
            (Extra, Choose("only_requested")),
            (Observed, Choose("part-1")),
            (Point, Choose("task-keep")),
        ]);
        let judge = Scripted::new(script);
        let provider = Judge::Provider(&policy, &judge);
        let Judged { verdict, .. } =
            judged(CALENDAR, request, CANDIDATE, &provider, Some(&observation)).await;
        assert_eq!(verdict.records.len(), 10);
        let kinds = [
            Request,
            Part,
            Part,
            Part,
            Point,
            Part,
            ObservedPart,
            Extra,
            Observed,
            Point,
        ];
        assert_eq!(judge.kinds(), kinds);
        let sent = judge.sent.lock().unwrap();
        for (k, sent) in sent.iter().enumerate() {
            let told = &sent.told;
            assert!(told.contains(framing), "{k}: {told}");
            assert!(!told.contains(never), "{k}: {told}");
            assert!(!told.contains(REVISED_APPENDED), "{k}: {told}");
            let asked = asks(sent.kind);
            assert!(asked.iter().all(|text| told.contains(text)), "{k}: {told}");
            let clause = &sent.state["clause"]["text"];
            let restricting = matches!(sent.kind, Part | Point | ObservedPart) && clause == ZONE;
            assert_eq!(told.contains(RESTRICTING), restricting, "{k}: {told}");
            assert_eq!(
                sent.state.get("revision").is_some(),
                framing == REVISED,
                "{k}"
            );
        }
        drop(sent);
        let note = "in the trial run, the judge points to the task keep";
        let split = parts(CALENDAR);
        let decided = found(&[(split[1].as_str(), note)], &[], &[], &["unfaithful"], &[]);
        assert_eq!(lists(&verdict), decided);
        assert_eq!(judge.left(), 0);
    }
}

/// A revision judged on the earlier request followed by the change as the human stated it
/// (`<original>\nChange: <words>`) is told so in every question: the change takes precedence and
/// a clause of the earlier request it replaces is superseded; its state marks the revision as
/// appended. The change is one part, its label kept with it, and, the last part, it is never
/// offered `superseded`.
#[tokio::test]
async fn an_appended_change_is_framed_as_the_earlier_request_followed_by_the_change() {
    let original = "Read ./data/calendar.json, write the list to ./out/week.md.";
    let change = "Also list the meetings of the week.";
    let request = CompileRequest::edit(CANDIDATE, change).with_original_intent(original);
    let intent = nika_compile::revise_intent(&request).unwrap();
    assert_eq!(intent, format!("{original}\nChange: {change}"));
    let split = [
        "Read ./data/calendar.json",
        "write the list to ./out/week.md",
        "Change: Also list the meetings of the week",
    ];
    assert_eq!(parts(&intent), split);
    // The candidate's read and write are paths this request never names: the extra question is
    // asked, then, nothing located and no run, where the doubt is.
    let mut script = undisputed("unfaithful", 3, Some((Extra, Choose("only_requested"))));
    script.push((Locate, Choose("unlocated")));
    let judge = Scripted::new(script);
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let provider = Judge::Provider(&policy, &judge);
    let Judged { verdict, .. } = judged(&intent, &request, CANDIDATE, &provider, None).await;
    let sent = judge.sent.lock().unwrap();
    assert_eq!(sent.len(), 6);
    let revision = json!({"change": change, "base_request": original, "appended": true});
    for (k, sent) in sent.iter().enumerate() {
        let told = &sent.told;
        assert!(told.contains(REVISED_APPENDED), "{k}: {told}");
        assert!(
            !told.contains(REVISED) && !told.contains(CREATED),
            "{k}: {told}"
        );
        assert_eq!(sent.state["revision"], revision, "{k}");
        assert_eq!(sent.state["request"], intent.as_str(), "{k}");
        assert_eq!(sent.state["original_request"], Value::Null, "{k}");
    }
    drop(sent);
    for (k, part) in split.iter().enumerate() {
        let asked = record(&verdict, &format!("verify-part-{k}"));
        assert_eq!(asked["options"], part_options(k, 3, false), "{part}");
    }
    let disputed = found(&[], &[], &[intent.as_str()], &["unfaithful"], &[UNOBSERVED]);
    assert_eq!(lists(&verdict), disputed);
    assert_eq!(judge.left(), 0);
}

/// The negation « keep only the orders that are not cancelled » is a restriction but no
/// prohibition (a keep asks an operation): told it restricts, judged missing it asks the task
/// that ignores it, `omitted` offered, and named it is the defect. The judge reads each clause
/// alone in its state, its text only; each record names the clause it judged and whether it
/// restricts, and the questions about the whole request name none.
#[tokio::test]
async fn a_negation_is_a_restriction_and_every_part_record_names_its_clause() {
    let judge = Scripted::new(pointing(3, 1, Choose("task-keep")));
    let Judged { verdict, .. } = provided(ORDERS, &judge, None).await;
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-point-1",
        "verify-part-2",
    ];
    assert_eq!(ids(&verdict), asked);
    let roles_asked = [
        "judge_request",
        "judge_part",
        "judge_part",
        "judge_point",
        "judge_part",
    ];
    assert_eq!(roles(&verdict), roles_asked);
    for (k, restricts) in [false, true, false].into_iter().enumerate() {
        let part = record(&verdict, &format!("verify-part-{k}"));
        let clause = json!({"text": ORDER_PARTS[k], "restricts": restricts});
        assert_eq!(part["clause"], clause);
    }
    let point = record(&verdict, "verify-point-1");
    assert_eq!(
        point["clause"],
        json!({"text": ORDER_PARTS[1], "restricts": true})
    );
    assert_eq!(point["options"], json!(POINTER));
    assert_eq!(record(&verdict, "verify-request").get("clause"), None);
    let sent = judge.sent.lock().unwrap();
    let alone = json!({"text": ORDER_PARTS[1]});
    assert_eq!(
        (&sent[2].state["clause"], &sent[3].state["clause"]),
        (&alone, &alone)
    );
    assert!(sent[2].told.contains(RESTRICTING), "{}", sent[2].told);
    assert!(sent[3].told.contains(RESTRICTING), "{}", sent[3].told);
    assert!(sent[3].told.contains(POINT_OMITTED), "{}", sent[3].told);
    assert_eq!(sent[0].state.get("clause"), None);
    drop(sent);
    let note = points("keep");
    let located = found(
        &[(ORDER_PARTS[1], note.as_str())],
        &[],
        &[],
        &["unfaithful"],
        &[],
    );
    assert_eq!(lists(&verdict), located);
    assert_eq!(judge.left(), 0);
}

/// A selected seat is asked the same questions by id and reads the same state; its judgment
/// carries its own name. A choice no option offers is refused on admission (neither
/// `no_operation` nor `superseded` for the one part of a request of one): unknown, never a
/// defect, never a judgment. A seat that fails on the whole request judged nothing; one that
/// fails on a part stops the localization there, the request unknown after its parts.
#[tokio::test]
async fn a_selected_seat_answers_the_same_questions_under_its_own_name() {
    let observation = observed(true, true);
    // Every part carried, the engine's facts settle every task: no extra question is asked.
    let seat = Seated::new([
        ("verify-request", Ok("none")),
        ("verify-part-0", Ok("carried")),
        ("verify-part-1", Ok("carried")),
        ("verify-part-2", Ok("carried")),
        ("verify-observed", Ok("consistent")),
    ]);
    let request = CompileRequest::create(ORDERS);
    let judge = Judge::<Scripted>::Seat(&seat);
    let Judged {
        verdict, binding, ..
    } = judged(ORDERS, &request, CANDIDATE, &judge, Some(&observation)).await;
    let judgment = carried(ORDERS, "verify-observed", SEAT, &binding);
    assert_eq!(verdict.judgments, [judgment]);
    assert_eq!(lists(&verdict), found(&[], &[], &[], &["none"], &[]));
    assert_eq!(verdict.settled_by, Some("verify-observed"));
    assert!(verdict.settled() && !verdict.doubted());
    let asked = seat.asked();
    assert_eq!(asked[1].state["clause"], json!({"text": ORDER_PARTS[0]}));
    assert_eq!(asked[4].state["observation"], observation);
    let run = [
        "consistent",
        "unexercised",
        "part-0",
        "part-1",
        "part-2",
        "none",
    ];
    assert_eq!(asked[4].keys(), run);
    let seat = Seated::new([
        ("verify-request", Ok("unfaithful")),
        ("verify-part-0", Ok("no_operation")),
        ("verify-extra", Ok("only_requested")),
    ]);
    let request = CompileRequest::create(GREETING);
    let judge = Judge::<Scripted>::Seat(&seat);
    let Judged { verdict, .. } = judged(GREETING, &request, CANDIDATE, &judge, None).await;
    let part = record(&verdict, "verify-part-0");
    assert_eq!(part["options"], json!(["carried", "missing", "none"]));
    let refused = "seat chose `no_operation`, outside the offered options";
    assert_eq!(part["error"], refused);
    let one = parts(GREETING);
    let unsettled = found(
        &[],
        &[one[0].as_str()],
        &[GREETING],
        &["unfaithful"],
        &[UNOBSERVED],
    );
    assert_eq!(lists(&verdict), unsettled);
    assert_eq!(verdict.judgments, NO_JUDGMENT);
    let seat = Seated::new([("verify-request", Err("the seat is unavailable"))]);
    let request = CompileRequest::create(ORDERS);
    let judge = Judge::<Scripted>::Seat(&seat);
    let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, None).await;
    assert_eq!(ids(&verdict), ["verify-request"]);
    assert_eq!(verdict.records[0]["error"], "the seat is unavailable");
    assert_eq!(verdict.unknown, [ORDERS]);
    assert_eq!((verdict.attempted, verdict.returned), (1, 0));
    assert!(verdict.stopped);
    // The parts are put to the seat together (A1): the first gets no answer and stops the
    // verdict there; the two answers it never read were sent, so they are recorded and counted.
    let seat = Seated::new([
        ("verify-request", Ok("unfaithful")),
        ("verify-part-0", Err("the seat is unavailable")),
        ("verify-part-1", Ok("carried")),
        ("verify-part-2", Ok("carried")),
    ]);
    let judge = Judge::<Scripted>::Seat(&seat);
    let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, None).await;
    let mut stopped = ORDER_PARTS.to_vec();
    stopped.push(ORDERS);
    assert_eq!(
        lists(&verdict),
        found(&[], &stopped, &[], &["unfaithful"], &[])
    );
    assert_eq!(counts(&verdict), (4, 3, 1));
    let unread: Vec<&str> = (verdict.records.iter())
        .filter(|record| record["role"] == "unread")
        .filter_map(|record| record["question"].as_str())
        .collect();
    assert_eq!(unread, ["verify-part-1", "verify-part-2"]);
    assert!(verdict.stopped);
    assert_eq!(seat.left(), 0);
}

/// A clause restricts as the reader reads a restriction, with an English negative contraction
/// read as its « not »: « don't » and « shouldn't » restrict as « do not » does, which the
/// reader alone would not see; a plain operation never restricts.
#[test]
fn a_negative_contraction_restricts_as_its_not_does() {
    let reader = nika_compile_reader::structure::restricts;
    for clause in [
        "don't send it to the owner",
        "don’t send it to the owner",
        "shouldn't write it",
    ] {
        assert!(restricts(clause), "{clause}");
        assert!(!reader(clause), "the reader alone: {clause}");
    }
    assert!(restricts(NEVER) && reader(NEVER));
    for clause in ["summarize it in three bullets", "write them to ./out/b.csv"] {
        assert!(!restricts(clause), "{clause}");
    }
}
