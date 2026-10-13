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
    Attempt, Bounds, CopyReceipt, Digest, EffectCounts, FinalReceipt, FinalState, Held,
    LedgerFacts, Observation, Rehearsal, RehearsalFuture, RehearsalReport, Rehearse,
    RehearsedOutput, RoomEvidence, Spent,
};
use nika_compile_cognition::{
    Cognition, NoProvider, Removal, verify_document, verify_document_presented_with,
    verify_document_with,
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
/// whole-request answer, its doubt's location and its parts only when told to.
struct Judge {
    whole: &'static str,
    doubt: &'static str,
    part: &'static str,
    point: &'static str,
    removed: &'static str,
    /// The probability it reports for its whole-request answer and for its doubt's location.
    weighed: (Option<f64>, Option<f64>),
    /// The probability it reports for each part's answer.
    parts_weighed: Option<f64>,
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
            parts_weighed: None,
            asked: Mutex::new(Vec::new()),
        }
    }

    /// The same judge weighing its whole-request answer at `whole` and its doubt's location at
    /// `doubt`, the rest of each distribution on NONE, as a decision model reports them.
    fn weighing(mut self, whole: f64, doubt: f64) -> Self {
        self.weighed = (Some(whole), Some(doubt));
        self
    }

    /// The same judge weighing each part's answer at `part`, the rest on NONE.
    fn weighing_parts(mut self, part: f64) -> Self {
        self.parts_weighed = Some(part);
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
            _ if id.starts_with("verify-part-") => self.parts_weighed,
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

/// The judge weighs its rejection of the whole request at the holding probability, carries every
/// part, names nothing extra and, asked where its doubt is, names nothing: held as the preview,
/// never READY, typed unresolved.
#[tokio::test]
async fn a_doubt_nothing_located_is_held_never_ready() {
    let judge = Judge::new("unfaithful", "unlocated").weighing(0.9, 0.9);
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
    let first = Judge::new("unfaithful", "unlocated").weighing(0.9, 0.9);
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

/// Adding a pending proposal preserves the same removal gate, before either the author
/// interprets the request or the decision seat qualifies the program.
#[tokio::test]
async fn a_presented_proposal_cannot_bypass_an_unconfirmed_removal() {
    let judge = Judge::new("faithful", "unlocated").reading_removals("kept");
    let request = CompileRequest::create(DELEGATING).with_authoring_policy(AuthoringPolicy::new(
        "mock/judge",
        1024,
        Duration::from_secs(2),
    ));
    let presentation = json!({"kind":"proposal", "selected":null, "pending_choices":[{
        "id":"output-current", "key":"output", "question":"Where should the digest go?",
        "state":"open", "after":[], "free_text":true, "multi_select":false,
        "options":[
            {"key":"md", "label":"Markdown", "recommended":true,
             "values":[{"role":"output_path", "value":DIGEST}]},
            {"key":"html", "label":"HTML", "recommended":false,
             "values":[{"role":"output_path", "value":"./news/digest.html"}]}]}]});
    let claims = [Removal::new(
        "https://techcrunch.com/feed",
        "je voulais les articles d'aujourd'hui",
    )];
    let selections = authored();
    let out = verify_document_presented_with(
        &request,
        &digest(),
        (&selections, &[]),
        &presentation,
        &claims,
        Cognition {
            provider: Some(&Approving),
            seat: Some(&judge),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(judge.ids(), ["verify-removed-0"]);
    let held = findings(&out, "removal");
    assert_eq!(held.len(), 1, "{held:?}");
    assert!(held[0].contains("https://techcrunch.com/feed"), "{held:?}");
    assert_eq!(decision(&out)["removals"][0]["choice"], "kept");
    assert_eq!(decision(&out)["presentation"], presentation);
    assert!(decision(&out).get("request_interpretation").is_none());
    assert!(decision(&out).get("semantic_verification").is_none());
    assert_eq!(
        out.provenance
            .authoring
            .as_ref()
            .map_or(0, |receipt| receipt.calls),
        0
    );
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

/// A part judged « missing » that its judge weighs below the confidence its question needs (0.7,
/// under 0.8 with five options) decides nothing: no task is asked, nothing is declined, and each
/// such part is stated with the proposal, READY on the parts (2026-10-11).
#[tokio::test]
async fn a_weighed_missing_part_below_its_confidence_is_stated_never_held() {
    let judge = Judge::new("faithful", "unlocated")
        .weighing(0.45, 0.2)
        .judging_parts("missing", "task-write_digest")
        .weighing_parts(0.7);
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate.as_deref(), Some(digest().as_str()));
    let ids = judge.ids();
    assert!(ids.iter().any(|id| id == "verify-part-0"), "{ids:?}");
    assert!(
        !ids.iter().any(|id| id.starts_with("verify-point-")),
        "{ids:?}"
    );
    let attempt = &decision(&out)["semantic_verification"][0];
    assert_eq!(attempt["settled_by"], "verify-parts", "{attempt:#}");
    assert_eq!(attempt["rejected"], false, "{attempt:#}");
    let said = findings(&out, "verify_doubt");
    let stated = "missing (reported at 0.70) without locating a defect";
    assert!(said.len() == 1 && said[0].contains(stated), "{said:?}");
    assert_eq!(attempt["stated_doubt"], said[0].as_str(), "{attempt:#}");
    assert!(findings(&out, "verify_held").is_empty(), "{out:#?}");
}

/// At or above that confidence (0.9) a part judged « missing » decides: the task the judge names
/// is a defect for the author, the document kept as the preview with what to repair; a part no
/// task fails is stated with the proposal, never held.
#[tokio::test]
async fn a_confident_missing_part_repairs_when_located_and_is_stated_when_not() {
    let judge = Judge::new("faithful", "unlocated")
        .weighing(0.45, 0.2)
        .judging_parts("missing", "task-write_digest")
        .weighing_parts(0.9);
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_some(), "kept as the preview");
    assert!(judge.ids().iter().any(|id| id == "verify-point-0"));
    let attempt = &decision(&out)["semantic_verification"][0];
    let defects = attempt["defects"].as_array().map_or(0, Vec::len);
    assert!(defects > 0, "{attempt:#}");
    assert_eq!(attempt["rejected"], true, "{attempt:#}");
    let judge = Judge::new("faithful", "unlocated")
        .weighing(0.45, 0.2)
        .judging_parts("missing", "no_task")
        .weighing_parts(0.9);
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(judge.ids().iter().any(|id| id == "verify-point-0"));
    let attempt = &decision(&out)["semantic_verification"][0];
    assert_eq!(attempt["settled_by"], "verify-parts", "{attempt:#}");
    let said = findings(&out, "verify_doubt");
    let stated = "missing (reported at 0.90) without locating a defect";
    assert!(said.len() == 1 && said[0].contains(stated), "{said:?}");
}

/// An answer reported with no distribution is taken at its word, as before: a part judged
/// « missing » that no task fails stays contested and keeps the document from READY, with
/// nothing stated.
#[tokio::test]
async fn an_unweighed_missing_part_no_task_fails_still_holds() {
    let judge = Judge::new("faithful", "unlocated")
        .weighing(0.45, 0.2)
        .judging_parts("missing", "no_task");
    let out = verified(&CompileRequest::create(DELEGATING), Some(&judge)).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let attempt = &decision(&out)["semantic_verification"][0];
    assert_eq!(attempt["settled_by"], Value::Null, "{attempt:#}");
    assert!(findings(&out, "verify_doubt").is_empty(), "{out:#?}");
}

const SALES_PATH: &str = "./in/sales.json";
const TOTAL_PATH: &str = "./out/total.json";
const SALES: &str =
    r#"[{"client":"a","amount":10},{"client":"b","amount":5},{"client":"a","amount":2.5}]"#;
const TOTAL_INTENT: &str = "Read ./in/sales.json, compute the total of the amount column and write the total to ./out/total.json";

/// The document a conversation wrote for [`TOTAL_INTENT`]: it reads the sales, checks its total
/// as the request words it and writes it.
const TOTALS: &str = r#"nika: totals
permits:
  tools: ["nika:read", "nika:jq", "nika:assert", "nika:write"]
  fs:
    read: ["./in/sales.json"]
    write: ["./out/total.json"]
tasks:
  load:
    invoke:
      tool: "nika:read"
      args: { path: "./in/sales.json" }
  sum:
    with: { rows: "${{ tasks.load.output }}" }
    invoke:
      tool: "nika:jq"
      args: { input: "${{ with.rows }}", expression: "fromjson | {total: (map(.amount) | add)} | tojson" }
  verified:
    with: { total: "${{ tasks.sum.output }}" }
    invoke:
      tool: "nika:jq"
      args: { input: "${{ with.total }}", expression: "fromjson | .total != null" }
  admit:
    with: { ok: "${{ tasks.verified.output }}" }
    invoke:
      tool: "nika:assert"
      args: { condition: "${{ with.ok }}", message: "the total breaks « the total of the amount column »" }
  save:
    after: { admit: success }
    with: { total: "${{ tasks.sum.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/total.json", content: "${{ with.total }}" }
"#;

/// A room whose one run copies the sales in whole and writes `output` as the total, counted.
struct Totals {
    output: &'static str,
    runs: Mutex<usize>,
}

impl Rehearse for Totals {
    fn rehearse<'a>(&'a self, candidate: &'a str, _inputs: &'a [String]) -> RehearsalFuture<'a> {
        *self.runs.lock().unwrap() += 1;
        let mut observed = Observation::none();
        observed.bounds = Bounds::new(10_000, 1_048_576, 65_536);
        let sales = Digest::of(SALES.as_bytes());
        let copy = CopyReceipt::new(
            SALES_PATH,
            sales.clone(),
            Some(sales),
            Held::Whole(SALES.into()),
        );
        observed.copies = vec![copy];
        let held = Held::Whole(self.output.into());
        let digest = Digest::of(self.output.as_bytes());
        observed.finals = vec![FinalReceipt::new(
            TOTAL_PATH,
            FinalState::File { digest, held },
        )];
        observed.ledger = LedgerFacts::clean(vec![TOTAL_PATH.into()]);
        observed.spent = Spent::new(SALES.len() as u64, self.output.len() as u64);
        let outputs = vec![RehearsedOutput::new(TOTAL_PATH, self.output)];
        let digest = nika_compile::surface::sha256(candidate);
        let report = RehearsalReport::new(
            Rehearsal::Passed { outputs },
            Attempt::Completed { elapsed_ms: 2 },
            EffectCounts::none(),
            digest,
        )
        .with_admitted_digest("fixture-admission")
        .with_room(RoomEvidence::new(true, true))
        .with_observation(observed);
        Box::pin(async move { report })
    }
    fn bound(&self) -> Duration {
        Duration::from_secs(1)
    }
}

/// A decision seat that carries the whole request and every part, the parts a run is shown for
/// included, keeping the ids it was asked.
struct Carrying(Mutex<Vec<String>>);

impl DecisionSeat for Carrying {
    fn name(&self) -> &'static str {
        "fixture/carrying"
    }

    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        let id = question.id.as_str();
        self.0.lock().unwrap().push(id.to_owned());
        let choice = match id {
            "verify-request" => "faithful",
            "verify-extra" => "only_requested",
            _ if id.contains("part-") => "carried",
            _ => "none",
        };
        let answer = ChoiceAnswer::new(choice, "fixture/carrying");
        Box::pin(async move { Ok(answer) })
    }
}

/// The class a judge alone let through: a conversation's document ran clean and its judge carried
/// it, but its trial wrote a total the request refuses. The core's own judgment of that very run
/// withdraws it, naming the obligation; the right total stays READY, judged once.
#[tokio::test]
async fn a_carried_document_whose_trial_writes_a_wrong_total_is_never_ready() {
    for (output, kept) in [(r#"{"total":17.5}"#, true), (r#"{"total":16}"#, false)] {
        let judge = Carrying(Mutex::new(Vec::new()));
        let room = Totals {
            output,
            runs: Mutex::new(0),
        };
        let cognition = Cognition::<NoProvider> {
            provider: None,
            seat: Some(&judge as &dyn DecisionSeat),
        };
        let request = CompileRequest::create(TOTAL_INTENT);
        let out = verify_document(&request, TOTALS, (&[], &[]), cognition, Some(&room))
            .await
            .unwrap();
        let asked = judge.0.lock().unwrap().clone();
        assert_eq!(
            asked.first().map(String::as_str),
            Some("verify-request"),
            "{output}"
        );
        assert_eq!(*room.runs.lock().unwrap(), 1, "{output}");
        assert_eq!(
            out.status == CompileStatus::Ready,
            kept,
            "{output}: {out:#?}"
        );
        let failed: Vec<Value> = findings(&out, "rehearsal")
            .iter()
            .filter_map(|said| serde_json::from_str::<Value>(said).ok())
            .filter(|said| said["kind"] == "behaviour_failed")
            .collect();
        assert_eq!(failed.len(), usize::from(!kept), "{output}: {out:#?}");
    }
}

const RANK_INTENT: &str =
    "Read ./rows.json, keep the top 2 rows by points and write them to ./top.json";

/// The ranked rows of [`RANK_INTENT`], written with no check of the order or of the size.
const RANKED: &str = r#"nika: top
permits:
  tools: ["nika:read", "nika:jq", "nika:write"]
  fs:
    read: ["./rows.json"]
    write: ["./top.json"]
tasks:
  load:
    invoke: { tool: "nika:read", args: { path: "./rows.json" } }
  rank:
    with: { rows: "${{ tasks.load.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.rows }}", expression: "fromjson | sort_by(.points) | reverse | .[:2] | tojson" } }
  save:
    with: { top: "${{ tasks.rank.output }}" }
    invoke: { tool: "nika:write", args: { path: "./top.json", content: "${{ with.top }}" } }
"#;

/// The same rows checked and asserted with the request's words before they are written.
const CHECKED: &str = r#"  ranked:
    with: { top: "${{ tasks.rank.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.top }}", expression: "fromjson | length <= 2" } }
  admit:
    with: { ok: "${{ tasks.ranked.output }}" }
    invoke: { tool: "nika:assert", args: { condition: "${{ with.ok }}", message: "the rows are not « keep the top 2 rows by points »" } }
"#;

/// A conversation's document whose stated property no check carries is held before any judge or
/// run, the property named and the bytes kept as the preview; with its check, it is judged as it
/// was.
#[tokio::test]
async fn a_document_whose_stated_property_no_check_carries_is_held_before_any_judge() {
    let checked = checked();
    for (source, held) in [(RANKED, true), (checked.as_str(), false)] {
        let judge = Judge::new("faithful", "unlocated");
        let room = Room(Mutex::new(0));
        let cognition = Cognition::<NoProvider> {
            provider: None,
            seat: Some(&judge as &dyn DecisionSeat),
        };
        let request = CompileRequest::create(RANK_INTENT);
        let out = verify_document(&request, source, (&[], &[]), cognition, Some(&room))
            .await
            .unwrap();
        let named = (findings(&out, "semantic_verification").iter())
            .any(|said| said.contains("keep the top 2 rows by points"));
        assert_eq!(named, held, "{out:#?}");
        if held {
            assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
            assert!(out.candidate.is_some(), "kept as the preview");
            assert!(judge.ids().is_empty(), "no judge asked: {:?}", judge.ids());
            assert_eq!(*room.0.lock().unwrap(), 0, "no run");
        } else {
            let asked = judge.ids();
            assert_eq!(asked.first().map(String::as_str), Some("verify-request"));
        }
    }
}

/// [`RANKED`] with [`CHECKED`]: its permits name the assert it calls.
fn checked() -> String {
    let permits = RANKED.replace(
        r#""nika:jq", "nika:write""#,
        r#""nika:jq", "nika:assert", "nika:write""#,
    );
    format!("{permits}{CHECKED}")
}

/// A request whose effect no run rehearses (a POST to the person's own service): one record per
/// invoice, even when the same invoice arrives twice.
const INVOICES: &str = "Create one record in our CRM for each invoice found in ./inbox/: POST {invoice_id, customer, amount} as JSON to http://127.0.0.1:58495/crm/invoices. The mail server sometimes delivers the same invoice twice at the same moment; an invoice must still create exactly one record.";

/// A correct candidate for [`INVOICES`], verbatim from a Session whose judge held it with no run of
/// its bytes: each invoice read, duplicates collapsed by id, the collapse checked, then one POST
/// per invoice.
const ONE_RECORD_PER_INVOICE: &str = r#"nika: inbox-invoices-to-crm
const:
  crm_endpoint: "http://127.0.0.1:58495/crm/invoices"
permits:
  tools: ["nika:glob", "nika:read", "nika:jq", "nika:assert", "nika:fetch"]
  fs: { read: ["./inbox/**"] }
  net: { http: ["127.0.0.1"] }
tasks:
  list_invoices:
    invoke:
      tool: "nika:glob"
      args: { pattern: "./inbox/*" }
  read_invoices:
    with: { paths: "${{ tasks.list_invoices.output }}" }
    for_each: { items: "${{ with.paths }}", fail_fast: true }
    invoke:
      tool: "nika:read"
      args: { path: "${{ item }}" }
  invoice_records:
    with: { texts: "${{ tasks.read_invoices.output }}" }
    invoke:
      tool: "nika:jq"
      args:
        input: { texts: "${{ with.texts }}" }
        expression: '[.texts[] | fromjson | {invoice_id: .invoice_id, customer: .customer, amount: .amount}] | unique_by(.invoice_id)'
  one_record_per_invoice:
    with: { records: "${{ tasks.invoice_records.output }}" }
    invoke:
      tool: "nika:jq"
      args:
        input: { records: "${{ with.records }}" }
        expression: '([.records[].invoice_id] | length) == ([.records[].invoice_id] | unique | length)'
  check_one_per_invoice:
    with: { ok: "${{ tasks.one_record_per_invoice.output }}" }
    invoke:
      tool: "nika:assert"
      args:
        condition: "${{ with.ok }}"
        message: "« an invoice must still create exactly one record »"
  create_records:
    after: { check_one_per_invoice: success }
    with: { records: "${{ tasks.invoice_records.output }}" }
    for_each: { items: "${{ with.records }}" }
    invoke:
      tool: "nika:fetch"
      args:
        url: "${{ const.crm_endpoint }}"
        method: POST
        headers: { content-type: "application/json" }
        body: "${{ item }}"
outputs:
  records: ${{ tasks.invoice_records.output }}
  responses: ${{ tasks.create_records.output }}
"#;

/// A broken candidate for [`INVOICES`] from the same Session: its check always fails, so it posts
/// nothing.
const ALWAYS_FAILS: &str = r#"nika: inbox-invoices-to-crm
model: deepseek/deepseek-flash
const:
  crm_endpoint: "http://127.0.0.1:58495/crm/invoices"
permits:
  tools: ["nika:glob", "nika:read", "nika:jq", "nika:assert", "nika:fetch"]
  fs: { read: ["./inbox/**"] }
  net: { http: ["127.0.0.1"] }
tasks:
  list_files:
    invoke: { tool: "nika:glob", args: { pattern: "./inbox/*" } }
  read_files:
    with: { paths: "${{ tasks.list_files.output }}" }
    for_each: { items: "${{ with.paths }}", fail_fast: true }
    invoke: { tool: "nika:read", args: { path: "${{ item }}" } }
  records:
    with: { files: "${{ tasks.read_files.output }}" }
    invoke:
      tool: "nika:jq"
      args:
        input: { files: "${{ with.files }}" }
        expression: '[.files[] | fromjson | {invoice_id: .invoice_id, customer: .customer, amount: .amount}] | unique_by(.invoice_id)'
  one_record_per_invoice:
    with: { records: "${{ tasks.records.output }}" }
    invoke:
      tool: "nika:jq"
      args:
        input: { records: "${{ with.records }}" }
        expression: '([.records[].invoice_id] | length) == ([.records[].invoice_id] | unique | length)'
  checked:
    with: { files: "${{ tasks.read_files.output }}" }
    invoke:
      tool: "nika:assert"
      args:
        condition: false
        message: "${{ with.files }}"
  create_records:
    after: { checked: success }
    with: { records: "${{ tasks.records.output }}" }
    for_each: { items: "${{ with.records }}" }
    invoke:
      tool: "nika:fetch"
      args:
        url: "${{ const.crm_endpoint }}"
        method: POST
        headers: { content-type: "application/json" }
        body: "${{ item }}"
outputs:
  records: ${{ tasks.records.output }}
  posted: ${{ tasks.create_records.output }}
"#;

/// The document `candidate` verified for the created [`INVOICES`] under `judge`, with no selection
/// and no room: no run of its bytes exists.
async fn invoices(candidate: &str, judge: &Judge) -> CompileOutcome {
    let cognition = Cognition::<NoProvider> {
        provider: None,
        seat: Some(judge as &dyn DecisionSeat),
    };
    let request = CompileRequest::create(INVOICES);
    verify_document(&request, candidate, (&[], &[]), cognition, None)
        .await
        .unwrap()
}

/// A correct candidate the judge rejects as a whole at 0.84 and locates nowhere (its doubt points
/// at a part below even odds), with no run of its bytes: under the bar a rejection located nowhere
/// needs, its parts carry the request. READY with the doubt stated, the attempt recording the
/// probability, the bar and the outcome. Held before, identical bytes went READY or held as the
/// judge's lean crossed the holding probability.
#[tokio::test]
async fn a_correct_candidate_doubted_under_the_bar_with_no_run_is_proposed_with_its_doubt() {
    let judge = Judge::new("unfaithful", "part-3").weighing(0.84, 0.25);
    let out = invoices(ONE_RECORD_PER_INVOICE, &judge).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(judge.ids().last().map(String::as_str), Some("verify-doubt"));
    let attempt = &decision(&out)["semantic_verification"][0];
    assert_eq!(attempt["settled_by"], "verify-parts", "{attempt:#}");
    let unlocated = json!({"probability": 0.84, "bar": 0.9, "outcome": "stated"});
    assert_eq!(attempt["unlocated_rejection"], unlocated, "{attempt:#}");
    let said = findings(&out, "verify_doubt");
    let reported = format!("{OVERRULED} (reported at 0.84)");
    assert!(
        said.len() == 1 && said[0].starts_with(&reported),
        "{said:?}"
    );
    assert!(findings(&out, "verify_held").is_empty(), "{out:#?}");
}

/// A broken candidate of the same request, rejected at 0.99 and located nowhere, with no run: at
/// its bar the rejection still holds, never proposed, the decision recorded.
#[tokio::test]
async fn a_broken_candidate_doubted_at_its_bar_with_no_run_stays_held() {
    let judge = Judge::new("unfaithful", "part-3").weighing(0.99, 0.34);
    let out = invoices(ALWAYS_FAILS, &judge).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let held = findings(&out, "verify_held");
    assert!(
        held.len() == 1 && held[0].starts_with(UNRESOLVED),
        "{held:?}"
    );
    let attempt = &decision(&out)["semantic_verification"][0];
    let unlocated = json!({"probability": 0.99, "bar": 0.9, "outcome": "held"});
    assert_eq!(attempt["unlocated_rejection"], unlocated, "{attempt:#}");
}

/// A request that asks the person before its effect: post the report, but only once they said
/// yes.
const GATED_SEND: &str =
    "Lis ./report.md et envoie-le à http://127.0.0.1:8768/notify, mais demande-moi avant d'envoyer";

/// What the deterministic door emits for [`GATED_SEND`] (its contract fixture): the send runs only
/// on the person's typed yes, asked before it, with no default answering for them.
const CONFIRMED_SEND: &str = r"const:
  send_endpoint: http://127.0.0.1:8768/notify
  source_path: ./report.md
nika: compiled-workflow
outputs:
  send_review: ${{ tasks.send_review.output }}
  send_status: ${{ tasks.send.status }}
permits:
  fs:
    read:
    - ./report.md
  net:
    http:
    - 127.0.0.1
  tools:
  - nika:notify
  - nika:prompt
  - nika:read
tasks:
  read_source:
    invoke:
      args:
        path: ${{ const.source_path }}
      tool: nika:read
  send:
    invoke:
      args:
        channel: webhook
        message: ${{ with.content }}
        target: ${{ const.send_endpoint }}
      tool: nika:notify
    when: ${{ with.approved == true }}
    with:
      approved: ${{ tasks.send_review.output }}
      content: ${{ tasks.read_source.output }}
  send_review:
    after:
      read_source: success
    invoke:
      args:
        message: 'Approve posting this exact content to ${{ const.send_endpoint }}? Content: ${{ with.content }}'
      tool: nika:prompt
    with:
      content: ${{ tasks.read_source.output }}
";

/// [`CONFIRMED_SEND`] without its confirmation: the send runs unasked. The language alone finds
/// nothing wrong with it; only the request says a confirmation was asked for.
const UNCONFIRMED_SEND: &str = r"const:
  send_endpoint: http://127.0.0.1:8768/notify
  source_path: ./report.md
nika: compiled-workflow
outputs:
  send_status: ${{ tasks.send.status }}
permits:
  fs:
    read:
    - ./report.md
  net:
    http:
    - 127.0.0.1
  tools:
  - nika:notify
  - nika:read
tasks:
  read_source:
    invoke:
      args:
        path: ${{ const.source_path }}
      tool: nika:read
  send:
    invoke:
      args:
        channel: webhook
        message: ${{ with.content }}
        target: ${{ const.send_endpoint }}
      tool: nika:notify
    with:
      content: ${{ tasks.read_source.output }}
";

/// [`CONFIRMED_SEND`] whose confirmation no longer gates the send: asked first, then sent
/// whatever the answer.
const IGNORED_CONFIRMATION: &str = r"const:
  send_endpoint: http://127.0.0.1:8768/notify
  source_path: ./report.md
nika: compiled-workflow
outputs:
  send_review: ${{ tasks.send_review.output }}
  send_status: ${{ tasks.send.status }}
permits:
  fs:
    read:
    - ./report.md
  net:
    http:
    - 127.0.0.1
  tools:
  - nika:notify
  - nika:prompt
  - nika:read
tasks:
  read_source:
    invoke:
      args:
        path: ${{ const.source_path }}
      tool: nika:read
  send:
    invoke:
      args:
        channel: webhook
        message: ${{ with.content }}
        target: ${{ const.send_endpoint }}
      tool: nika:notify
    after:
      send_review: success
    with:
      content: ${{ tasks.read_source.output }}
  send_review:
    after:
      read_source: success
    invoke:
      args:
        message: 'Approve posting this exact content to ${{ const.send_endpoint }}? Content: ${{ with.content }}'
      tool: nika:prompt
    with:
      content: ${{ tasks.read_source.output }}
";

/// The document `candidate` verified for the created [`GATED_SEND`] under `cognition`, with no
/// selection and no room: nothing is sent.
async fn gated_send<P: ProviderInferDyn>(
    candidate: &str,
    cognition: Cognition<'_, P>,
) -> CompileOutcome {
    let policy = AuthoringPolicy::new("mock/judge", 1024, Duration::from_secs(2));
    let request = CompileRequest::create(GATED_SEND).with_authoring_policy(policy);
    verify_document(&request, candidate, (&[], &[]), cognition, None)
        .await
        .unwrap()
}

/// A confirmation the request asks for before its effect, implemented as the deterministic door
/// emits it: no law refuses it, and it is READY on the judge's verdict.
#[tokio::test]
async fn a_requested_confirmation_before_the_effect_is_ready() {
    let judge = Judge::new("faithful", "unlocated");
    let cognition = Cognition::<NoProvider> {
        provider: None,
        seat: Some(&judge as &dyn DecisionSeat),
    };
    let out = gated_send(CONFIRMED_SEND, cognition).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate.as_deref(), Some(CONFIRMED_SEND));
    assert_eq!(judge.ids(), ["verify-request"]);
    let attempt = &decision(&out)["semantic_verification"][0];
    assert_eq!(attempt["settled_by"], "verify-request", "{attempt:#}");
}

/// The same program without that confirmation, or with one that no longer gates the send, never
/// passes, whichever judge the caller permits: the approval law refuses it before any judge, any
/// reading of the request or any run, and nothing is proposed.
#[tokio::test]
async fn the_same_program_without_its_confirmation_never_passes() {
    for (candidate, head) in [
        (UNCONFIRMED_SEND, "MISSING APPROVAL"),
        (IGNORED_CONFIRMATION, "APPROVAL ORDER"),
    ] {
        let judge = Judge::new("faithful", "unlocated");
        let alone = Cognition::<NoProvider> {
            provider: None,
            seat: Some(&judge as &dyn DecisionSeat),
        };
        let paired = Cognition {
            provider: Some(&Approving),
            seat: Some(&judge as &dyn DecisionSeat),
        };
        for out in [
            gated_send(candidate, alone).await,
            gated_send(candidate, paired).await,
        ] {
            assert_eq!(out.status, CompileStatus::Incomplete, "{head}: {out:#?}");
            let refused = findings(&out, "conversation");
            assert!(
                refused.iter().any(|said| said.starts_with(head)),
                "{head}: {refused:?}"
            );
            let decision = out.provenance.decision.as_ref();
            let read = decision.is_some_and(|d| d.get("request_interpretation").is_some());
            assert!(!read, "{head}: {decision:?}");
            let calls = (out.provenance.authoring.as_ref()).map_or(0, |receipt| receipt.calls);
            assert_eq!(calls, 0, "{head}");
        }
        assert!(judge.ids().is_empty(), "{head}: {:?}", judge.ids());
    }
}
