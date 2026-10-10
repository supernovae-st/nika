// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A doubt only a trial run decides, when the room refused the bytes before any attempt (A2),
//! through the document door and the public rehearsed entry. The author's first document builds
//! its jq input as an object holding a value the run builds, which the room's argument law
//! (`rehearse::arguments`, applied here as the room applies it) refuses as a data bound; the
//! judge, a decision seat, rejects the whole request and locates no part. The room's refusal is
//! a construct the document chose, so the door asks the author once for an equivalent document
//! the room runs, with the room's words and every obligation kept; the restated bytes run, and
//! the judge decides over that run. The same refusal again reopens nothing, a refusal the
//! request's own effect causes reopens nothing, and the held candidate keeps its record with the
//! rejection inside it: a round that replays it asks that judge nothing, another judge decides
//! it with no authoring call. A scripted verdict proves the door's exits, never a model's.
use super::observed::{JUDGE, Judging, Kept, ran};
use super::*;
use nika_compile_cognition::decide::{ChoiceFuture, ChoiceQuestion, DecisionSeat};
use nika_compile_cognition::rehearse::{
    Attempt, EffectCounts, Observation, Refusal, Rehearsal, RehearsalFuture, RehearsalReport,
    Rehearse, arguments,
};
use nika_compile_cognition::{Cognition, compile_with_cognition_rehearsed};
use nika_kernel::ai::provider::{ContentBlock, Role, StopReason, TokenUsage};
use nika_schema::{FileId, ParseMode};
use std::sync::Mutex;

/// The author's first document: the open tickets kept by a jq step whose input is an object
/// holding the text the run read.
const WRAPPED: &str = r#"nika: open-tickets
permits:
  tools: ["nika:read", "nika:jq", "nika:write"]
  fs: { read: ["./tickets.json"], write: ["./out/open.json"] }
tasks:
  read_tickets:
    invoke: { tool: "nika:read", args: { path: "./tickets.json" } }
  keep_open:
    with: { text: "${{ tasks.read_tickets.output }}" }
    invoke: { tool: "nika:jq", args: { input: { tickets: "${{ with.text }}" }, expression: '.tickets | fromjson | map(select(.status == "open"))' } }
  write_open:
    with: { open: "${{ tasks.keep_open.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/open.json", content: "${{ with.open }}" } }
"#;

/// The same work restated in a form the room runs: the text read is the jq step's input.
const PLAIN: &str = r#"nika: open-tickets
permits:
  tools: ["nika:read", "nika:jq", "nika:write"]
  fs: { read: ["./tickets.json"], write: ["./out/open.json"] }
tasks:
  read_tickets:
    invoke: { tool: "nika:read", args: { path: "./tickets.json" } }
  keep_open:
    with: { text: "${{ tasks.read_tickets.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.text }}", expression: 'fromjson | map(select(.status == "open"))' } }
  write_open:
    with: { open: "${{ tasks.keep_open.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/open.json", content: "${{ with.open }}" } }
"#;

/// The words the room's argument law gives `candidate`, when it refuses it.
fn room_words(candidate: &str) -> Option<String> {
    let workflow = nika_schema::parse(candidate, FileId::new(0), ParseMode::Strict).unwrap();
    arguments::evaluated(&workflow).err()
}

/// A host whose room refuses, before any attempt, every candidate the argument law refuses (a
/// data bound in the law's words), or every candidate as `effect` when set; any other candidate
/// it runs to completion over the tickets (each text kept whole).
struct Room {
    effect: Option<&'static str>,
    shown: Mutex<Vec<String>>,
}

impl Room {
    fn new(effect: Option<&'static str>) -> Self {
        let shown = Mutex::new(Vec::new());
        Self { effect, shown }
    }
    fn shown(&self) -> Vec<String> {
        self.shown.lock().unwrap().clone()
    }
}

/// The room's refusal of `candidate` before any attempt: `refusal`, in `reason`.
fn refused(candidate: &str, refusal: Refusal, reason: String) -> RehearsalReport {
    let sha = nika_compile::surface::sha256(candidate);
    let outcome = Rehearsal::NotRun { reason };
    RehearsalReport::new(outcome, Attempt::NeverAttempted, EffectCounts::none(), sha)
        .with_observation(Observation::refused(refusal))
}

impl Rehearse for Room {
    fn bound(&self) -> Duration {
        Duration::from_secs(10)
    }
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }
    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        _inputs: &'a [String],
        _targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move {
            self.shown.lock().unwrap().push(candidate.to_owned());
            match (self.effect, room_words(candidate)) {
                (Some(effect), _) => refused(candidate, Refusal::Effect, effect.to_owned()),
                (None, Some(words)) => refused(candidate, Refusal::DataBounds, words),
                (None, None) => ran(candidate, Kept::Whole),
            }
        })
    }
}

/// A scripted document author: the documents it answers, in order, and the last message of each
/// call it received. A call past the script fails as a provider would.
struct Author {
    documents: Vec<String>,
    told: Mutex<Vec<String>>,
}

impl Author {
    fn new(documents: &[&str]) -> Self {
        let told = Mutex::new(Vec::new());
        let documents = documents.iter().map(|d| (*d).to_owned()).collect();
        Self { documents, told }
    }
    fn told(&self) -> Vec<String> {
        self.told.lock().unwrap().clone()
    }
}

impl ProviderInferDyn for Author {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let last = (request.messages.iter().rev())
            .find(|message| matches!(message.role, Role::User))
            .map(|message| {
                (message.content.iter())
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>()
            })
            .unwrap_or_default();
        let at = {
            let mut told = self.told.lock().unwrap();
            told.push(last);
            told.len() - 1
        };
        let Some(document) = self.documents.get(at) else {
            let reason = "the scripted author has no further document".to_owned();
            return Err(ProviderError::Other { reason });
        };
        let answer = json!({"candidate": document, "candidate_lines": [], "operations": [],
            "questions": [], "gaps": [], "notes": "scripted"});
        let text = answer.to_string();
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(100, 50),
            StopReason::EndTurn,
        ))
    }
}

/// The doubt no part locates on one document: the whole request unfaithful, its two parts
/// carried (the read and the write are the request's own, so the engine's facts leave no task
/// to ask about).
const DOUBT: [(&str, &str); 3] = [
    ("verify-request", "unfaithful"),
    ("verify-part-0", "carried"),
    ("verify-part-1", "carried"),
];

/// [`DOUBT`] on a document no run decides: asked where the doubt is, nowhere.
fn doubted() -> Vec<(&'static str, &'static str)> {
    let mut script = DOUBT.to_vec();
    script.push(("verify-doubt", "unlocated"));
    script
}

/// [`TICKETS`] through the document door, judged by `judge` and rehearsed in `room`.
async fn restated(room: &Room, judge: &dyn DecisionSeat, author: &Author) -> CompileOutcome {
    let cognition = Cognition {
        provider: Some(author),
        seat: Some(judge),
    };
    let policy = policy(NativeMode::Escalate).with_repairs(4);
    let request = CompileRequest::create(TICKETS).with_authoring_policy(policy);
    compile_with_cognition_rehearsed(&request, cognition, Some(room as &dyn Rehearse))
        .await
        .unwrap()
}

/// The held outcome's record: kept, its rejection of `candidate` by [`JUDGE`] inside it.
fn assert_record_carries_its_rejection(out: &CompileOutcome, candidate: &str) {
    let record = out.provenance.plan.as_ref().expect("the record is kept");
    let declined = record["declined"].as_array().expect("its rejections");
    let sha = nika_compile::surface::sha256(candidate);
    assert!(
        (declined.iter()).any(|attempt| attempt["candidate_sha256"] == sha.as_str()
            && attempt["judge"]["seat"] == JUDGE
            && attempt["rejected"] == true),
        "{record:#}"
    );
}

#[tokio::test]
async fn a_refused_trial_reopens_the_author_and_the_restated_document_runs_and_is_judged() {
    let words = room_words(WRAPPED).expect("the room's law refuses the wrapped input");
    assert!(words.starts_with("task keep_open args.input"), "{words}");
    assert_eq!(
        room_words(PLAIN),
        None,
        "the restated document passes the law"
    );
    let room = Room::new(None);
    let mut script = doubted();
    script.extend(DOUBT);
    script.push(("verify-observed", "consistent"));
    let judge = Judging::new(&script);
    let author = Author::new(&[WRAPPED, PLAIN]);
    let out = restated(&room, &judge, &author).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate.as_deref(), Some(PLAIN));
    assert_eq!(judge.left(), 0, "{out:#?}");
    let ids: Vec<&str> = script.iter().map(|(id, _)| *id).collect();
    assert_eq!(judge.ids(), ids, "no question asked twice of one document");
    // The author was asked once more, with the room's words and every obligation kept.
    let told = author.told();
    assert_eq!(told.len(), 2, "{told:#?}");
    assert!(told[1].contains(&words), "{}", told[1]);
    assert!(
        told[1].contains("keep every requested operation"),
        "{}",
        told[1]
    );
    assert_eq!(room.shown(), [WRAPPED, PLAIN].map(str::to_owned));
    let attempts = verification(&out);
    assert_eq!(attempts.len(), 2, "{attempts:#?}");
    let first = &attempts[0];
    assert_eq!(first["doubt"], json!(["unfaithful"]), "{first:#}");
    assert_eq!(first["contested"], json!([TICKETS]), "{first:#}");
    assert_eq!(first["unsettled"], json!([NO_TRIAL]), "{first:#}");
    let unobserved = json!({"refusal": Refusal::DataBounds.word(), "reason": words});
    assert_eq!(first["unobserved"], unobserved, "{first:#}");
    let second = &attempts[1];
    assert_eq!(second["settled_by"], "verify-observed", "{second:#}");
    let sha = nika_compile::surface::sha256(PLAIN);
    assert_eq!(second["candidate_sha256"], sha.as_str(), "{second:#}");
    assert!(
        route(&out).contains("verify: trial refused, restated"),
        "{out:#?}"
    );
}

#[tokio::test]
async fn the_same_refusal_again_reopens_nothing_and_holds_the_document_with_its_record() {
    let again = "# the same construct, other bytes\n".to_owned() + WRAPPED;
    let again = again.as_str();
    let room = Room::new(None);
    let mut script = doubted();
    script.extend(doubted());
    let judge = Judging::new(&script);
    let author = Author::new(&[WRAPPED, again]);
    let out = restated(&room, &judge, &author).await;
    assert_eq!(author.told().len(), 2, "no third authoring call");
    assert_eq!(judge.left(), 0, "{out:#?}");
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(
        out.candidate.as_deref(),
        Some(again),
        "the last document stays shown"
    );
    assert!(out.questions.is_empty(), "{out:#?}");
    assert_record_carries_its_rejection(&out, again);
    let words = room_words(again).unwrap();
    let held = findings(&out, "verify_held");
    assert_eq!(held.len(), 1, "{held:?}");
    let opened = held[0].starts_with(HELD) || held[0].starts_with(UNRESOLVED);
    assert!(opened && held[0].contains(&words), "{held:?}");
}

#[tokio::test]
async fn a_refusal_the_requested_effect_causes_reopens_nothing() {
    let reason = "the candidate needs an effect a rehearsal denies";
    let room = Room::new(Some(reason));
    let judge = Judging::new(&doubted());
    let author = Author::new(&[PLAIN]);
    let out = restated(&room, &judge, &author).await;
    assert_eq!(author.told().len(), 1, "no restatement asked: {out:#?}");
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let attempt = &verification(&out)[0];
    let unobserved = json!({"refusal": Refusal::Effect.word(), "reason": reason});
    assert_eq!(attempt["unobserved"], unobserved, "{attempt:#}");
    assert_record_carries_its_rejection(&out, PLAIN);
    assert!(!route(&out).contains("restated"), "{out:#?}");
}

/// A judge of another name: it answers every whole-request question `faithful`.
struct Another {
    asked: Mutex<Vec<String>>,
}

impl DecisionSeat for Another {
    fn name(&self) -> &'static str {
        "mock/another-judge"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.id.clone());
            let answer = nika_compile_cognition::decide::ChoiceAnswer::new("faithful", "another");
            Ok(answer)
        })
    }
}

#[tokio::test]
async fn a_replayed_held_record_asks_its_judge_nothing_and_another_judge_decides_it() {
    let room = Room::new(Some("the candidate needs an effect a rehearsal denies"));
    let held = restated(&room, &Judging::new(&doubted()), &Author::new(&[PLAIN])).await;
    let record = held.provenance.plan.clone().expect("the record is kept");
    let policy = policy(NativeMode::Escalate).with_repairs(4);
    let replay = CompileRequest::create(TICKETS)
        .with_plan(record)
        .with_authoring_policy(policy);
    // The same judge, no host memory: the record's own rejection is repeated with no call.
    let judge = Judging::new(&[]);
    let author = Author::new(&[]);
    let cognition = Cognition {
        provider: Some(&author),
        seat: Some(&judge as &dyn DecisionSeat),
    };
    let rehearsed = Some(&room as &dyn Rehearse);
    let out = compile_with_cognition_rehearsed(&replay, cognition, rehearsed)
        .await
        .unwrap();
    assert!(judge.asked().is_empty(), "{out:#?}");
    assert!(author.told().is_empty(), "{out:#?}");
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(out.candidate.as_deref(), Some(PLAIN));
    // The verdict is the record's own rejection, repeated with no call (R6).
    let attempt = verification(&out).last().cloned().unwrap_or_default();
    assert_eq!(attempt["carried"], true, "{attempt:#}");
    assert_eq!(attempt["attempted"], 0, "{attempt:#}");
    assert_eq!(attempt["judge"]["seat"], JUDGE, "{attempt:#}");
    // Another judge decides the same bytes, with no authoring call.
    let another = Another {
        asked: Mutex::new(Vec::new()),
    };
    let cognition = Cognition {
        provider: Some(&author),
        seat: Some(&another as &dyn DecisionSeat),
    };
    let out = compile_with_cognition_rehearsed(&replay, cognition, rehearsed)
        .await
        .unwrap();
    assert!(author.told().is_empty(), "{out:#?}");
    assert_eq!(another.asked.lock().unwrap()[..1], ["verify-request"]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate.as_deref(), Some(PLAIN));
}

#[tokio::test]
async fn a_record_held_without_a_room_names_the_refusal_of_the_round_that_replays_it() {
    let policy = policy(NativeMode::Escalate).with_repairs(4);
    let (judge, author) = (Judging::new(&doubted()), Author::new(&[PLAIN]));
    let cognition = Cognition {
        provider: Some(&author),
        seat: Some(&judge as &dyn DecisionSeat),
    };
    let request = CompileRequest::create(TICKETS).with_authoring_policy(policy.clone());
    let held = compile_with_cognition_rehearsed(&request, cognition, None)
        .await
        .unwrap();
    assert_eq!(
        verification(&held)[0]["unobserved"],
        Value::Null,
        "no room asked"
    );
    let record = held.provenance.plan.clone().expect("the record is kept");
    // Replayed where a room refuses these bytes: the round names that refusal, with no call.
    let reason = "the candidate needs an effect a rehearsal denies";
    let room = Room::new(Some(reason));
    let replay = CompileRequest::create(TICKETS)
        .with_plan(record)
        .with_authoring_policy(policy);
    let judge = Judging::new(&[]);
    let cognition = Cognition {
        provider: Some(&author),
        seat: Some(&judge as &dyn DecisionSeat),
    };
    let rehearsed = Some(&room as &dyn Rehearse);
    let out = compile_with_cognition_rehearsed(&replay, cognition, rehearsed)
        .await
        .unwrap();
    assert!(judge.asked().is_empty(), "{out:#?}");
    let attempt = verification(&out).last().cloned().unwrap_or_default();
    let unobserved = json!({"refusal": Refusal::Effect.word(), "reason": reason});
    assert_eq!(attempt["unobserved"], unobserved, "{attempt:#}");
    let told = findings(&out, "verify_held");
    assert!(told.iter().any(|held| held.contains(reason)), "{told:?}");
}
