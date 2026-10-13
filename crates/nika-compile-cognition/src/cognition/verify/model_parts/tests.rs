// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The model-written parts of a request over scripted judges: the provider judge's statement
//! (its words checked against the request, each part found on the candidate's paths), a weighing
//! seat's sink answers averaged over both option orders, the holds they leave, and the record a
//! repeat reads back.

use std::sync::Mutex;
use std::time::Duration;

use nika_compile::surface::Binding;
use nika_compile::{ModelFound, ModelPart, PartBasis};
use nika_compile_fidelity::model_paths::ModelSink;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};

use super::super::{Declined, Judge, Verdict, WHOLE, record, state, whole};
use super::{MODEL_STEPS, document, read};
use crate::decide::system_one::Order;
use crate::decide::{
    BatchFuture, ChoiceAnswer, ChoiceBatch, ChoiceFuture, ChoiceQuestion, DecisionError,
    DecisionSeat, each_alone,
};
use crate::plan::Plan;
use crate::{AuthoringPolicy, CompileRequest, NoProvider};

/// A request whose result a model must write: a summary of each article.
const INTENT: &str = "Read ./notes.md and summarize each article into ./out/summary.md.";
/// The sentence of [`INTENT`] that names the summary's file.
const SENTENCE: &str = "Read ./notes.md and summarize each article into ./out/summary.md";
/// The words a provider judge quotes for the part.
const WORDS: &str = "summarize each article";
/// The sentence a provider judge gives as the reason for its choice.
const REASON: &str = "the bytes write the notes as they are";
/// How the outcome says the law did not run.
const UNRUN: &str = "model-step check not run: ";
const READ: &str = "  read:\n    invoke: { tool: \"nika:read\", args: { path: \"./notes.md\" } }\n";
/// The candidate copies the notes into the summary's file: no model writes it.
const COPY: &str = "  write:\n    with: { text: \"${{ tasks.read.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/summary.md\", content: \"${{ with.text }}\" } }\n";
/// A model step summarizing the notes.
const SUMMARIZE: &str = "  summarize:\n    with: { text: \"${{ tasks.read.output }}\" }\n    infer: { prompt: \"Summarize ${{ with.text }}\" }\n";
/// The write of what the model step wrote.
const WRITE_SUMMARY: &str = "  write:\n    with: { summary: \"${{ tasks.summarize.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/summary.md\", content: \"${{ with.summary }}\" } }\n";

/// The candidate: the read, then `tasks`.
fn candidate(tasks: &str) -> String {
    format!("nika: summary\ntasks:\n{READ}{tasks}")
}

/// The provider judge's statement of one part, its sink labelled `sink`.
fn one(sink: &str) -> Value {
    json!([{"words": WORDS, "result": "the summary file", "sink": sink}])
}

fn write() -> ModelSink {
    ModelSink::Task("write".to_owned())
}

/// The authoring provider as the judge: it answers the whole request `faithful`, with the parts
/// it states (none: it ignores the property), and keeps each request's schema.
struct Stating {
    parts: Option<Value>,
    schemas: Mutex<Vec<Value>>,
}

impl Stating {
    fn new(parts: Option<Value>) -> Self {
        Self {
            parts,
            schemas: Mutex::new(Vec::new()),
        }
    }
}

impl ProviderInferDyn for Stating {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let ResponseFormat::JsonSchema(schema) = &request.response_format else {
            panic!("a judge question is a closed choice");
        };
        self.schemas.lock().unwrap().push(schema.clone());
        let mut answer = json!({"choice": "faithful", "reason": REASON});
        if let Some(parts) = &self.parts {
            answer["model_parts"] = parts.clone();
        }
        let text = answer.to_string();
        let content = vec![ContentBlock::Text { text }];
        Ok(InferResponse::new(
            content,
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// A selected seat: `faithful` to the whole request, taken at its word, and each sink question's
/// distribution over `computed`, `model` and `none` from its script, by question id. It keeps
/// every question and counts its batches.
struct Weighing {
    weighs: bool,
    script: Vec<(&'static str, [f64; 3])>,
    asked: Mutex<Vec<ChoiceQuestion>>,
    batches: Mutex<usize>,
}

impl Weighing {
    fn new(weighs: bool, script: Vec<(&'static str, [f64; 3])>) -> Self {
        Self {
            weighs,
            script,
            asked: Mutex::new(Vec::new()),
            batches: Mutex::new(0),
        }
    }

    /// The ids of the questions it was asked, sorted.
    fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = (self.asked.lock().unwrap().iter())
            .map(|question| question.id.clone())
            .collect();
        ids.sort();
        ids
    }
}

impl DecisionSeat for Weighing {
    fn name(&self) -> &'static str {
        "mock/weighing"
    }

    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.clone());
            if question.id == "verify-request" {
                return Ok(ChoiceAnswer::new("faithful", "mock/weighing"));
            }
            let (_, [computed, model, none]) = (self.script.iter())
                .find(|(id, _)| *id == question.id)
                .ok_or_else(|| DecisionError(format!("unscripted {}", question.id)))?;
            let choice = if model >= computed {
                "model"
            } else {
                "computed"
            };
            let mut answer = ChoiceAnswer::new(choice, "mock/weighing");
            answer.probabilities.extend([
                ("computed".to_owned(), *computed),
                ("model".to_owned(), *model),
                ("none".to_owned(), *none),
            ]);
            Ok(answer)
        })
    }

    fn choose_each<'a>(&'a self, batch: &'a ChoiceBatch) -> BatchFuture<'a> {
        *self.batches.lock().unwrap() += 1;
        each_alone(batch, |question| self.choose(question))
    }

    fn weighs(&self) -> bool {
        self.weighs
    }
}

/// The whole verdict of [`INTENT`] over the candidate of `tasks`, asked of `judge`.
async fn judged<P: ProviderInferDyn>(tasks: &str, judge: &Judge<'_, P>) -> Verdict {
    let candidate = candidate(tasks);
    let request = CompileRequest::create(INTENT);
    let base = state(INTENT, &request, &candidate);
    let binding = Binding::of(INTENT, &request, &Plan::default(), &candidate);
    let (mut verdict, mut out) = (Verdict::default(), crate::initial());
    let asked = (&base, "fixture");
    whole(INTENT, asked, judge, &binding, None, &mut verdict, &mut out).await;
    verdict
}

/// The verdict the provider judge `judge` gives the candidate of `tasks`.
async fn provided(judge: &Stating, tasks: &str) -> Verdict {
    let policy = AuthoringPolicy::new("mock/judge", 256, Duration::from_secs(2));
    judged(tasks, &Judge::Provider(&policy, judge)).await
}

/// The verdict the seat `seat` gives the candidate of `tasks`.
async fn seated(seat: &Weighing, tasks: &str) -> Verdict {
    judged(tasks, &Judge::<NoProvider>::Seat(seat)).await
}

/// The parts the law found, when it ran.
fn parts(verdict: &Verdict) -> Vec<ModelPart> {
    verdict.model_parts.clone().expect("the law ran")
}

/// The request asks for a summary and the candidate copies the notes: the part the judge quotes
/// is held, a defect in the person's words, and the bytes are declined.
#[tokio::test]
async fn a_copy_where_a_summary_is_asked_is_held_in_the_persons_words() {
    let verdict = provided(&Stating::new(Some(one("task:write"))), COPY).await;
    let parts = parts(&verdict);
    assert_eq!(parts.len(), 1);
    assert_eq!(
        (&parts[0].sink, &parts[0].found),
        (&Some(write()), &ModelFound::Missing)
    );
    assert_eq!(verdict.defects, [WORDS]);
    let note = "a model must write it, and no model step writes what `./out/summary.md` (task `write`) receives";
    assert_eq!(verdict.notes, [(WORDS.to_owned(), note.to_owned())]);
    assert_eq!(verdict.declined, Declined::Rejected);
    assert!(!verdict.settled());
    let whole = (verdict.records.iter()).find(|r| r["question"] == "verify-request");
    assert_eq!(
        whole.unwrap()["also"]["reason"],
        REASON,
        "the reason is kept"
    );
    assert_eq!(verdict.model_parts_unrun, None);
}

/// A model step whose text no result reads is decorative: held, naming the step.
#[tokio::test]
async fn a_model_step_no_result_reads_is_held_as_decorative() {
    let tasks = format!("{SUMMARIZE}{COPY}");
    let verdict = provided(&Stating::new(Some(one("task:write"))), &tasks).await;
    let summarize = "summarize".to_owned();
    assert_eq!(
        parts(&verdict)[0].found,
        ModelFound::Decorative { task: summarize }
    );
    let note =
        "a model must write it, and the model step `summarize` writes text that no result reads";
    assert_eq!(verdict.notes, [(WORDS.to_owned(), note.to_owned())]);
    assert!(!verdict.settled());
}

/// The model step's text feeds the write: the part is met, nothing is held, the verdict settles.
#[tokio::test]
async fn a_model_step_feeding_the_write_meets_the_part() {
    let tasks = format!("{SUMMARIZE}{WRITE_SUMMARY}");
    let verdict = provided(&Stating::new(Some(one("task:write"))), &tasks).await;
    let parts = parts(&verdict);
    let basis = PartBasis::Words {
        words: WORDS.to_owned(),
        result: "the summary file".to_owned(),
    };
    let met = ModelFound::Met {
        task: "summarize".to_owned(),
    };
    assert_eq!(parts, [ModelPart::new(basis, Some(write()), met)]);
    assert!(verdict.defects.is_empty() && verdict.settled());
    assert_eq!(verdict.declined, Declined::No);
}

/// An empty statement is judged: no part needed, nothing held. Words the request does not hold
/// are an invented need and carry nothing.
#[tokio::test]
async fn an_empty_or_invented_statement_states_no_part() {
    for statement in [
        json!([]),
        json!([{"words": "translate it into German", "result": "r", "sink": "task:write"}]),
    ] {
        let verdict = provided(&Stating::new(Some(statement.clone())), COPY).await;
        assert_eq!(verdict.model_parts, Some(Vec::new()), "{statement}");
        assert!(verdict.settled(), "{statement}");
    }
}

/// The judge is offered the candidate's own sinks and `none`; a judge that ignores the property
/// states nothing: the law does not run, never « passed », and nothing is held.
#[tokio::test]
async fn a_judge_that_states_nothing_leaves_the_law_unrun() {
    let judge = Stating::new(None);
    let verdict = provided(&judge, COPY).await;
    assert_eq!(verdict.model_parts, None);
    let why = format!("{UNRUN}the judge stated no model parts");
    assert_eq!(verdict.model_parts_unrun, Some(why));
    assert!(verdict.settled());
    let schemas = judge.schemas.lock().unwrap();
    assert_eq!(schemas.len(), 1, "one call: the whole request");
    let required = json!(["choice", "model_parts", "reason"]);
    assert_eq!(schemas[0]["required"], required);
    let sinks = &schemas[0]["properties"]["model_parts"]["items"]["properties"]["sink"]["enum"];
    assert_eq!(sinks, &json!(["task:write", "none"]));
}

/// A sink the judge was not offered makes its statement unusable: the law does not run, and the
/// answer's record keeps why.
#[tokio::test]
async fn an_unoffered_sink_leaves_the_law_unrun_and_says_why() {
    let verdict = provided(&Stating::new(Some(one("task:send"))), COPY).await;
    assert_eq!(verdict.model_parts, None);
    let whole = (verdict.records.iter()).find(|r| r["question"] == "verify-request");
    let why = "the sink `task:send` was not offered";
    assert_eq!(whole.unwrap()["model_parts_refused"], why);
    let said = format!("{UNRUN}the judge's model parts were malformed ({why})");
    assert_eq!(verdict.model_parts_unrun, Some(said));
    assert!(verdict.settled());
}

/// A weighing seat is asked each sink in both option orders with the whole request, in one
/// batch: a sink whose averaged p(model) reaches 0.6 that no model step writes is held, in the
/// sentence that names its file.
#[tokio::test]
async fn a_weighing_seat_holds_a_sink_a_model_must_write() {
    let seat = Weighing::new(
        true,
        vec![
            ("verify-sink-0", [0.2, 0.75, 0.05]),
            ("verify-sink-0-r", [0.3, 0.65, 0.05]),
        ],
    );
    let verdict = seated(&seat, COPY).await;
    assert_eq!(*seat.batches.lock().unwrap(), 1, "one request");
    let ids = ["verify-request", "verify-sink-0", "verify-sink-0-r"];
    assert_eq!(seat.ids(), ids);
    let asked = seat.asked.lock().unwrap().clone();
    let sink = (asked.iter()).find(|q| q.id == "verify-sink-0").unwrap();
    let twin = (asked.iter()).find(|q| q.id == "verify-sink-0-r").unwrap();
    assert_eq!((sink.order, twin.order), (Order::Question, Order::Reversed));
    let shown = json!({"task": "write", "writes": "./out/summary.md"});
    assert_eq!(
        (&sink.state["sink"], &sink.state["cited"]),
        (&shown, &json!(SENTENCE))
    );
    let parts = parts(&verdict);
    let p = 0.7;
    assert!(
        matches!(&parts[0].basis, PartBasis::Sink { cited: Some(c), p: q } if c == SENTENCE && (q - p).abs() < 1e-9)
    );
    assert_eq!(parts[0].found, ModelFound::Missing);
    assert_eq!(verdict.defects, [SENTENCE]);
    let note = "`./out/summary.md` (task `write`) receives content a model must write (p 0.70 over both option orders), and no model step writes it";
    assert_eq!(verdict.notes, [(SENTENCE.to_owned(), note.to_owned())]);
    assert_eq!(verdict.declined, Declined::Rejected);
}

/// A lean toward the first option decides nothing once both orders are averaged: the sink stays
/// open, no part is stated and nothing is held.
#[tokio::test]
async fn averaging_both_orders_cancels_a_first_option_lean() {
    let seat = Weighing::new(
        true,
        vec![
            ("verify-sink-0", [0.62, 0.33, 0.05]),
            ("verify-sink-0-r", [0.30, 0.62, 0.08]),
        ],
    );
    let verdict = seated(&seat, COPY).await;
    assert_eq!(verdict.model_parts, None, "an open sink states nothing");
    let open = "`./out/summary.md` (task `write`)";
    let why = format!("{UNRUN}both option orders left {open} undecided");
    assert_eq!(verdict.model_parts_unrun, Some(why));
    assert!(verdict.defects.is_empty() && verdict.settled());
}

/// A sink whose content a program computes has no part: judged, none needed.
#[tokio::test]
async fn a_computed_sink_states_no_part() {
    let seat = Weighing::new(
        true,
        vec![
            ("verify-sink-0", [0.8, 0.15, 0.05]),
            ("verify-sink-0-r", [0.7, 0.25, 0.05]),
        ],
    );
    let verdict = seated(&seat, COPY).await;
    assert_eq!(verdict.model_parts, Some(Vec::new()));
    assert!(verdict.settled());
}

/// A model step on the sink's path meets the part a weighing seat states.
#[tokio::test]
async fn a_weighed_part_is_met_by_the_model_step_on_its_path() {
    let seat = Weighing::new(
        true,
        vec![
            ("verify-sink-0", [0.1, 0.85, 0.05]),
            ("verify-sink-0-r", [0.2, 0.75, 0.05]),
        ],
    );
    let verdict = seated(&seat, &format!("{SUMMARIZE}{WRITE_SUMMARY}")).await;
    let met = ModelFound::Met {
        task: "summarize".to_owned(),
    };
    assert_eq!(parts(&verdict)[0].found, met);
    assert!(verdict.defects.is_empty() && verdict.settled());
}

/// A seat that does not weigh its answers is asked the whole request alone, as before: the law
/// does not run with it, and the verdict says so.
#[tokio::test]
async fn a_seat_that_does_not_weigh_is_asked_the_whole_request_alone() {
    let seat = Weighing::new(false, Vec::new());
    let verdict = seated(&seat, COPY).await;
    assert_eq!(seat.ids(), ["verify-request"]);
    assert_eq!(*seat.batches.lock().unwrap(), 0);
    assert_eq!(verdict.model_parts, None);
    let why = format!("{UNRUN}the judge seat does not weigh its answers");
    assert_eq!(verdict.model_parts_unrun, Some(why));
}

/// Every part an attempt records is read back whole by a repeat, with no call: each basis, each
/// sink form and each finding.
#[test]
fn an_attempt_records_its_parts_and_a_repeat_reads_them_back() {
    let words = PartBasis::Words {
        words: WORDS.to_owned(),
        result: "the summary file".to_owned(),
    };
    let weighed = |cited: Option<&str>| PartBasis::Sink {
        cited: cited.map(str::to_owned),
        p: 0.83,
    };
    let task = || "summarize".to_owned();
    let parts = vec![
        ModelPart::new(words, Some(write()), ModelFound::Missing),
        ModelPart::new(
            weighed(Some(SENTENCE)),
            None,
            ModelFound::Met { task: task() },
        ),
        ModelPart::new(
            weighed(None),
            Some(ModelSink::Output("summary".to_owned())),
            ModelFound::Decorative { task: task() },
        ),
        ModelPart::new(
            weighed(None),
            Some(write()),
            ModelFound::Undecided {
                task: task(),
                child: "./summarize.nika".to_owned(),
            },
        ),
    ];
    assert_eq!(read(&document(&parts)), Some(parts.clone()));
    assert_eq!(
        read(&Value::Null),
        None,
        "an attempt before the law carries none"
    );
    let seat = Weighing::new(true, Vec::new());
    let judge = Judge::<NoProvider>::Seat(&seat);
    let verdict = Verdict {
        model_parts: Some(parts.clone()),
        ..Verdict::default()
    };
    let mut out = crate::initial();
    record(&mut out, &judge, &verdict, 0);
    assert_eq!(
        out.model_parts,
        Some(parts.clone()),
        "the outcome carries them"
    );
    let decision = out.provenance.decision.clone().unwrap_or_default();
    let attempt = &decision["semantic_verification"][0];
    assert_eq!(attempt["model_parts"], document(&parts));
    assert_eq!(Verdict::recorded(attempt, 0).model_parts, Some(parts));
    assert_eq!(
        out.model_parts_unrun, None,
        "a law that ran says nothing more"
    );
    let why = format!("{UNRUN}the judge seat does not weigh its answers");
    let unweighed = Verdict {
        model_parts_unrun: Some(why.clone()),
        ..Verdict::default()
    };
    record(&mut out, &judge, &unweighed, 1);
    record(&mut out, &judge, &unweighed, 2);
    assert_eq!(out.model_parts, None, "the latest attempt's parts");
    assert_eq!(out.model_parts_unrun, Some(why.clone()), "never silent");
    let decision = out.provenance.decision.clone().unwrap_or_default();
    let attempt = &decision["semantic_verification"][2];
    assert_eq!(attempt["model_parts_unrun"], why.as_str());
    assert_eq!(Verdict::recorded(attempt, 2).model_parts_unrun, Some(why));
}

/// The whole question `seat` was asked.
fn whole_question(seat: &Weighing) -> ChoiceQuestion {
    let asked = seat.asked.lock().unwrap();
    let found = asked.iter().find(|q| q.id == "verify-request");
    found.cloned().expect("the whole question")
}

/// The whole question carries the walk's fact, said once in its instructions: for each result
/// the bytes write or return, the model steps on its data path (engine facts, never claims).
#[tokio::test]
async fn the_whole_question_carries_the_model_steps_on_each_result() {
    let seat = Weighing::new(false, Vec::new());
    let returned = "outputs:\n  summary: \"${{ tasks.summarize.output }}\"\n";
    seated(&seat, &format!("{SUMMARIZE}{WRITE_SUMMARY}{returned}")).await;
    let request = whole_question(&seat);
    let fact = json!([
        {"sink": {"task": "write"}, "model_steps": ["summarize"]},
        {"sink": {"output": "summary"}, "model_steps": ["summarize"]},
    ]);
    assert_eq!(request.state["model_steps"], fact);
    let (said, instructions) = (format!("{WHOLE} {MODEL_STEPS}"), &request.instructions);
    assert!(instructions.contains(&said), "{instructions}");
}

/// Bytes the walk cannot read carry no fact, and the question says none.
#[tokio::test]
async fn bytes_the_walk_cannot_read_carry_no_model_steps() {
    let seat = Weighing::new(false, Vec::new());
    seated(&seat, "  broken: [\n").await;
    let request = whole_question(&seat);
    let (state, instructions) = (&request.state, &request.instructions);
    assert_eq!(state.get("model_steps"), None, "{state:#}");
    assert!(!instructions.contains(MODEL_STEPS), "{instructions}");
}
