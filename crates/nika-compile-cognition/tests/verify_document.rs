// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A document a Session's conversation wrote is verified as a native door verifies its own
//! candidate before anything is proposed: the laws over the world the host observed, the core's
//! finish, then the whole-request verdict of the judge the caller permits, carrying the verdicts
//! the conversation kept (R6). It is READY only on that verdict; a doubt nothing located is held,
//! a located defect keeps the bytes as the preview with what to repair, and with no judge the
//! whole request stays pending. An answer the judge weighs decides at its calibrated threshold:
//! below it, the parts decide and the doubt is stated. The judge is a scripted double: it
//! decides what each test states.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::Duration;

use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus};
use nika_compile_cognition::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat};
use nika_compile_cognition::rehearse::{
    Attempt, EffectCounts, Rehearsal, RehearsalFuture, RehearsalReport, Rehearse,
};
use nika_compile_cognition::{
    Cognition, NoProvider, Removal, verify_document, verify_document_with,
};
use nika_compile_fidelity::fidelity::resolution::Resolution;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};

const HACKER_NEWS: &str = "https://news.ycombinator.com";
const DIGEST: &str = "./news/digest.md";
/// A request that delegates the sources and leaves the output name to the author.
const DELEGATING: &str = "recupere les news tech recentes, resume les et ecris le resume en markdown dans un dossier du projet. les sources publiques tu les choisis toi meme";
const DELEGATION: &str = "les sources publiques tu les choisis toi meme";
const OUTPUT_WORDS: &str = "ecris le resume en markdown dans un dossier du projet";
/// The words a held candidate whose doubt nothing located opens with.
const UNRESOLVED: &str = "The verifier doubted the request as a whole but located nothing";
/// The words a weighed rejection the parts overrule is stated with.
const OVERRULED: &str = "The verifier doubted that this workflow carries the request as a whole";

/// A digest workflow: one GET, one summary, one write.
fn digest() -> String {
    let host = HACKER_NEWS
        .split_once("://")
        .map_or(HACKER_NEWS, |(_, rest)| rest);
    let mut source = format!(
        "nika: news-digest\nmodel: mock/echo\npermits:\n  tools: [\"nika:fetch\", \"nika:write\"]\n  net:\n    http: [\"{host}\"]\n  fs:\n    write: [\"{DIGEST}\"]\ntasks:\n"
    );
    write!(
        source,
        "  news:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"{HACKER_NEWS}\", method: GET }}\n"
    )
    .unwrap();
    source.push_str("  summarize:\n    with:\n      news: \"${{ tasks.news.output }}\"\n");
    source.push_str("    infer:\n      max_tokens: 1000\n      prompt: \"Résume en Markdown les actualités ci-dessous, sans rien inventer : ${{ with.news }}\"\n");
    write!(
        source,
        "  write_digest:\n    with:\n      digest: \"${{{{ tasks.summarize.output }}}}\"\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"{DIGEST}\", content: \"${{{{ with.digest }}}}\" }}\n"
    )
    .unwrap();
    source
}

/// The source chosen within the delegation and the derived output, on the person's words.
fn authored() -> Vec<Resolution> {
    Resolution::read_all(&[
        json!({"value": HACKER_NEWS, "kind": "delegated", "role": "read_source",
            "excerpt": DELEGATION}),
        json!({"value": DIGEST, "kind": "derived", "role": "output_path",
            "excerpt": OUTPUT_WORDS}),
    ])
    .unwrap()
}

/// A decision seat answering each verifier question by its id (the whole request as `whole`,
/// each part as `part` and the task question it leads to as `point`, nothing extra, where a doubt
/// is as `doubt`, a removal claim as `removed`), keeping every question. It weighs its
/// whole-request answer and its doubt's location only when told to.
struct Judge {
    whole: &'static str,
    doubt: &'static str,
    part: &'static str,
    point: &'static str,
    removed: &'static str,
    /// The probability it reports for its whole-request answer and for its doubt's location.
    weighed: (Option<f64>, Option<f64>),
    asked: Mutex<Vec<ChoiceQuestion>>,
}

impl Judge {
    fn new(whole: &'static str, doubt: &'static str) -> Self {
        Self {
            whole,
            doubt,
            part: "carried",
            point: "none",
            removed: "removed",
            weighed: (None, None),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// The same judge weighing its whole-request answer at `whole` and its doubt's location at
    /// `doubt`, the rest of each distribution on NONE, as a decision model reports them.
    fn weighing(mut self, whole: f64, doubt: f64) -> Self {
        self.weighed = (Some(whole), Some(doubt));
        self
    }

    /// The same judge answering each part `part`, and the task question it leads to `point`.
    fn judging_parts(mut self, part: &'static str, point: &'static str) -> Self {
        (self.part, self.point) = (part, point);
        self
    }

    /// The same judge answering each removal claim as `removed`.
    fn reading_removals(mut self, removed: &'static str) -> Self {
        self.removed = removed;
        self
    }

    fn ids(&self) -> Vec<String> {
        let asked = self.asked.lock().unwrap();
        asked.iter().map(|question| question.id.clone()).collect()
    }
}

impl DecisionSeat for Judge {
    fn name(&self) -> &'static str {
        "fixture/judge"
    }

    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        self.asked.lock().unwrap().push(question.clone());
        let id = question.id.as_str();
        let choice = match id {
            "verify-request" => self.whole,
            "verify-extra" => "only_requested",
            "verify-doubt" => self.doubt,
            _ if id.starts_with("verify-part-") => self.part,
            _ if id.starts_with("verify-point-") => self.point,
            _ if id.starts_with("verify-removed-") => self.removed,
            _ => "none",
        };
        let mut answer = ChoiceAnswer::new(choice, "fixture/judge");
        let weighed = match id {
            "verify-request" => self.weighed.0,
            "verify-doubt" => self.weighed.1,
            _ => None,
        };
        if let Some(p) = weighed {
            let rest = ("none".to_owned(), 1.0 - p);
            answer.probabilities.extend([(choice.to_owned(), p), rest]);
        }
        Box::pin(async move { Ok(answer) })
    }
}

/// The digest verified for `request` under `judge` (no judge at all when `None`).
async fn verified(request: &CompileRequest, judge: Option<&Judge>) -> CompileOutcome {
    let cognition = Cognition::<NoProvider> {
        provider: None,
        seat: judge.map(|judge| judge as &dyn DecisionSeat),
    };
    let selections = authored();
    verify_document(request, &digest(), (&selections, &[]), cognition, None)
        .await
        .unwrap()
}

/// The decision record of an outcome.
fn decision(out: &CompileOutcome) -> &Value {
    out.provenance.decision.as_ref().expect("a decision record")
}

/// The route steps an outcome records, in order.
fn steps(out: &CompileOutcome) -> Vec<String> {
    (decision(out)["route"].as_array().into_iter().flatten())
        .filter_map(|step| step.as_str().map(str::to_owned))
        .collect()
}

/// The messages of an outcome's findings on `target`.
fn findings(out: &CompileOutcome, target: &str) -> Vec<String> {
    (out.diagnostics.iter())
        .filter(|d| d.target == target)
        .map(|d| d.message.clone())
        .collect()
}

#[tokio::test]
async fn a_document_its_judge_carries_is_ready_on_that_verdict() {
    let judge = Judge::new("faithful", "unlocated");
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate.as_deref(), Some(digest().as_str()));
    assert_eq!(judge.ids(), ["verify-request"]);
    let attempt = &decision(&out)["semantic_verification"][0];
    assert_eq!(attempt["settled_by"], "verify-request", "{attempt:#}");
    let route = steps(&out);
    assert_eq!(
        route.first().map(String::as_str),
        Some("conversation: document")
    );
    assert_eq!(
        route.last().map(String::as_str),
        Some("verify: judged (decision_seat)")
    );
}

/// The judge rejects the whole request, carries every part, names nothing extra and, asked where
/// its doubt is, names nothing: held as the preview, never READY, typed unresolved.
#[tokio::test]
async fn a_doubt_nothing_located_is_held_never_ready() {
    let judge = Judge::new("unfaithful", "unlocated");
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_some(), "shown, never offered");
    let attempt = &decision(&out)["semantic_verification"][0];
    assert_eq!(attempt["unresolved"], true, "{attempt:#}");
    assert_eq!(attempt["settled_by"], Value::Null, "{attempt:#}");
    let held = findings(&out, "verify_held");
    assert!(
        held.len() == 1 && held[0].starts_with(UNRESOLVED),
        "{held:?}"
    );
    let ids = judge.ids();
    assert_eq!(ids.first().map(String::as_str), Some("verify-request"));
    assert_eq!(ids.last().map(String::as_str), Some("verify-doubt"));
}

/// Asked where its doubt is, the judge names a task the engine's facts leave open: a located
/// defect, the document kept as the preview with what to repair, never READY.
#[tokio::test]
async fn a_located_defect_keeps_the_document_as_the_preview() {
    let judge = Judge::new("unfaithful", "task-news");
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_some(), "kept as the preview");
    let attempt = &decision(&out)["semantic_verification"][0];
    assert_eq!(
        attempt["defects"],
        json!(["only what the request asks"]),
        "{attempt:#}"
    );
    assert!(
        !findings(&out, "semantic_verification").is_empty(),
        "{out:#?}"
    );
}

/// The verdict the conversation kept is carried into the next verification of the same bytes:
/// the judge is asked nothing more, and the bytes stay held (R6).
#[tokio::test]
async fn a_verdict_the_conversation_kept_is_never_asked_again() {
    let first = Judge::new("unfaithful", "unlocated");
    let held = verified(&CompileRequest::create(DELEGATING), Some(&first)).await;
    let kept = decision(&held)["semantic_verification"]
        .as_array()
        .cloned()
        .unwrap();
    let again = Judge::new("faithful", "unlocated");
    let request = CompileRequest::create(DELEGATING).with_declined(kept);
    let out = verified(&request, Some(&again)).await;
    assert!(again.ids().is_empty(), "asked nothing: {:?}", again.ids());
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(decision(&out)["semantic_verification"][0]["carried"], true);
}

/// With no judge permitted, nothing is READY: the whole request stays pending on these bytes.
#[tokio::test]
async fn with_no_judge_the_whole_request_stays_pending() {
    let out = verified(&CompileRequest::create(DELEGATING), None).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_some(), "{out:#?}");
    let open = &decision(&out)["pending"]["open"];
    assert_eq!(open[0]["clause"], DELEGATING, "{open:#}");
    let said = findings(&out, "semantic_verification");
    assert!(
        said.iter().any(|s| s.starts_with("No judge was permitted")),
        "{said:?}"
    );
}

/// A room counting the runs it is asked for, which runs nothing.
struct Room(Mutex<usize>);

impl Rehearse for Room {
    fn rehearse<'a>(&'a self, candidate: &'a str, _inputs: &'a [String]) -> RehearsalFuture<'a> {
        *self.0.lock().unwrap() += 1;
        let outcome = Rehearsal::NotRun {
            reason: "fixture".to_owned(),
        };
        let report = RehearsalReport::new(
            outcome,
            Attempt::NeverAttempted,
            EffectCounts::none(),
            candidate,
        );
        Box::pin(async move { report })
    }
    fn bound(&self) -> Duration {
        Duration::from_secs(1)
    }
}

/// A run serves only a verdict: with no judge permitted, the room the host lends runs nothing.
#[tokio::test]
async fn with_no_judge_the_room_runs_nothing() {
    let room = Room(Mutex::new(0));
    let selections = authored();
    let request = CompileRequest::create(DELEGATING);
    let cognition = Cognition::<NoProvider>::default();
    let out = verify_document(
        &request,
        &digest(),
        (&selections, &[]),
        cognition,
        Some(&room),
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(*room.0.lock().unwrap(), 0, "the room ran nothing");
}

/// The laws read the world the host observed: a derived output the project already holds is
/// refused before any judge reads the bytes; observed elsewhere, the judge reads the world.
#[tokio::test]
async fn the_laws_and_the_judge_read_the_observed_world() {
    let held = json!({"observed": [{"path": DIGEST, "state": "observed"}]});
    let judge = Judge::new("faithful", "unlocated");
    let request = CompileRequest::create(DELEGATING).with_knowledge(held);
    let out = verified(&request, Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(judge.ids().is_empty(), "no judge reads refused bytes");
    let refused = findings(&out, "conversation");
    assert!(
        refused.iter().any(|r| r.contains("already exists")),
        "{refused:?}"
    );
    let elsewhere = json!({"observed": [{"path": "./news/other.md", "state": "observed"}]});
    let judge = Judge::new("faithful", "unlocated");
    let request = CompileRequest::create(DELEGATING).with_knowledge(elsewhere.clone());
    let out = verified(&request, Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let asked = judge.asked.lock().unwrap();
    assert_eq!(asked[0].state["observed"], elsewhere);
}

/// A document the laws refuse is never shown to a judge: its refusals are what to repair.
#[tokio::test]
async fn a_document_the_laws_refuse_asks_no_judge() {
    let judge = Judge::new("faithful", "unlocated");
    let cognition = Cognition::<NoProvider> {
        provider: None,
        seat: Some(&judge as &dyn DecisionSeat),
    };
    let request = CompileRequest::create(DELEGATING);
    let out = verify_document(&request, &digest(), (&[], &[]), cognition, None)
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(judge.ids().is_empty());
    let refused = findings(&out, "conversation");
    assert!(
        refused.iter().any(|r| r.starts_with("INVENTED LITERAL")),
        "{refused:?}"
    );
}

/// An authoring provider that approves the whole request when asked.
struct Approving;

impl ProviderInferDyn for Approving {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let offers = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema.to_string().contains("faithful"),
            _ => false,
        };
        let text = if offers {
            json!({"choice": "faithful"}).to_string()
        } else {
            json!({"choice": "none"}).to_string()
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// With no decision seat, the bounded authoring provider judges under the request's policy.
#[tokio::test]
async fn the_authoring_provider_judges_when_no_seat_is_permitted() {
    let policy = AuthoringPolicy::new("mock/judge", 1024, Duration::from_secs(2));
    let request = CompileRequest::create(DELEGATING).with_authoring_policy(policy);
    let cognition = Cognition {
        provider: Some(&Approving),
        seat: None,
    };
    let selections = authored();
    let out = verify_document(&request, &digest(), (&selections, &[]), cognition, None)
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let route = steps(&out);
    assert_eq!(
        route.last().map(String::as_str),
        Some("verify: judged (authoring_provider)")
    );
}

/// The digest verified with one removal claim: the person's words cited as removing a feed a
/// proposal they saw bound.
async fn removing(judge: &Judge, words: &str) -> CompileOutcome {
    let cognition = Cognition::<NoProvider> {
        provider: None,
        seat: Some(judge as &dyn DecisionSeat),
    };
    let selections = authored();
    let claims = [Removal::new("https://techcrunch.com/feed", words)];
    let request = CompileRequest::create(DELEGATING);
    let document = digest();
    let verified = verify_document_with(
        &request,
        &document,
        (&selections, &[]),
        &claims,
        cognition,
        None,
    );
    Box::pin(verified).await.unwrap()
}

/// A revision drops a feed on words the judge does not read as removing it: held before the
/// whole request is asked, never READY, the claim named; its answer recorded.
#[tokio::test]
async fn a_removal_the_judge_does_not_confirm_holds_the_document() {
    let judge = Judge::new("faithful", "unlocated").reading_removals("kept");
    let out = removing(&judge, "en fait je voulais les articles d'aujourd'hui").await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(judge.ids(), ["verify-removed-0"]);
    let held = findings(&out, "removal");
    assert!(
        held.len() == 1 && held[0].contains("https://techcrunch.com/feed"),
        "{held:?}"
    );
    assert_eq!(decision(&out)["removals"][0]["choice"], "kept");
}

/// Words the judge reads as removing the feed let the verdict run: READY on the whole request.
#[tokio::test]
async fn a_removal_the_judge_confirms_lets_the_verdict_run() {
    let judge = Judge::new("faithful", "unlocated");
    let out = removing(&judge, "enleve techcrunch").await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(judge.ids(), ["verify-removed-0", "verify-request"]);
    assert_eq!(decision(&out)["removals"][0]["choice"], "removed");
}

/// The shape of the held cells: the judge rejects the whole request, weighing it at 0.65 (below
/// the holding probability), carries every part and names nothing extra. That answer decides
/// nothing: READY on the parts, the doubt stated in words with the proposal and recorded, never
/// held, and no question asked where the doubt is.
#[tokio::test]
async fn a_weighed_rejection_below_the_holding_probability_is_stated_with_the_proposal() {
    let judge = Judge::new("unfaithful", "unlocated").weighing(0.65, 0.2);
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate.as_deref(), Some(digest().as_str()));
    let attempt = &decision(&out)["semantic_verification"][0];
    assert_eq!(attempt["settled_by"], "verify-parts", "{attempt:#}");
    assert_eq!(attempt["rejected"], false, "{attempt:#}");
    let said = findings(&out, "verify_doubt");
    let reported = format!("{OVERRULED} (reported at 0.65)");
    assert!(
        said.len() == 1 && said[0].starts_with(&reported),
        "{said:?}"
    );
    assert_eq!(attempt["stated_doubt"], said[0].as_str(), "{attempt:#}");
    assert!(findings(&out, "verify_held").is_empty(), "{out:#?}");
    let ids = judge.ids();
    assert!(!ids.iter().any(|id| id == "verify-doubt"), "{ids:?}");
}

/// A weighed « faithful » admits nothing by itself: the parts and the extra question decide. Every
/// part carried carries the request, nothing stated; a part judged missing whose task the judge
/// names keeps the document as the preview with what to repair.
#[tokio::test]
async fn a_weighed_faithful_answer_admits_nothing_by_itself() {
    let judge = Judge::new("faithful", "unlocated").weighing(0.45, 0.2);
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let ids = judge.ids();
    assert!(ids.iter().any(|id| id == "verify-part-0"), "{ids:?}");
    assert_eq!(ids.last().map(String::as_str), Some("verify-extra"));
    let attempt = &decision(&out)["semantic_verification"][0];
    assert_eq!(attempt["settled_by"], "verify-parts", "{attempt:#}");
    assert!(findings(&out, "verify_doubt").is_empty(), "{out:#?}");
    let judge = Judge::new("faithful", "unlocated")
        .weighing(0.45, 0.2)
        .judging_parts("missing", "task-write_digest");
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_some(), "kept as the preview");
    let attempt = &decision(&out)["semantic_verification"][0];
    let defects = attempt["defects"].as_array().map_or(0, Vec::len);
    assert!(defects > 0, "{attempt:#}");
}

/// A rejection weighed at the holding probability or more decides as before: located nowhere,
/// the bytes are held, the judge asked where its doubt is.
#[tokio::test]
async fn a_confident_weighed_rejection_located_nowhere_still_holds() {
    let judge = Judge::new("unfaithful", "unlocated").weighing(0.9, 0.3);
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let held = findings(&out, "verify_held");
    assert!(
        held.len() == 1 && held[0].starts_with(UNRESOLVED),
        "{held:?}"
    );
    assert_eq!(judge.ids().last().map(String::as_str), Some("verify-doubt"));
    assert!(findings(&out, "verify_doubt").is_empty(), "{out:#?}");
}

/// A doubt's location the judge weighs below even odds names nothing: the open task it points
/// to is no defect, and the confident rejection stays held as one located nowhere.
#[tokio::test]
async fn a_doubt_located_below_even_odds_names_nothing() {
    let judge = Judge::new("unfaithful", "task-news").weighing(0.9, 0.3);
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let attempt = &decision(&out)["semantic_verification"][0];
    assert_eq!(attempt["defects"], json!([]), "{attempt:#}");
    let held = findings(&out, "verify_held");
    assert!(
        held.len() == 1 && held[0].starts_with(UNRESOLVED),
        "{held:?}"
    );
}

/// A part the judge judged missing was sent to the author once, for a repair: judged missing
/// again on other bytes of the same request, with a rejection that decides nothing, it is stated
/// with the proposal, never held for the person.
#[tokio::test]
async fn a_part_a_repair_attempt_met_is_stated_when_judged_missing_again() {
    let judge = || {
        Judge::new("unfaithful", "unlocated")
            .weighing(0.65, 0.2)
            .judging_parts("missing", "task-write_digest")
    };
    let first = judge();
    let out = verified(&CompileRequest::create(DELEGATING), Some(&first)).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let mut earlier = decision(&out)["semantic_verification"][0].clone();
    assert!(earlier["defects"].as_array().is_some_and(|d| !d.is_empty()));
    earlier["candidate_sha256"] = json!("the bytes before the repair");
    let again = judge();
    let request = CompileRequest::create(DELEGATING).with_declined(vec![earlier]);
    let out = verified(&request, Some(&again)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let said = findings(&out, "verify_doubt");
    assert!(
        said.len() == 1 && said[0].contains("missing again after a repair attempt"),
        "{said:?}"
    );
}
