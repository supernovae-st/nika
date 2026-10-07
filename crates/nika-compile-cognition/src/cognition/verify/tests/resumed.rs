// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A verdict on the same bytes whose localization did not finish is resumed, never repeated as
//! final and never asked again (R6): a call that got no answer stopped it, or it waited for a
//! whole trial run this call now has. Every answer its judge gave those bytes is read back with
//! no call; only what it never got is asked. A verdict that finished is repeated with no call.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};

use super::super::{knowledge, native_verdict};
use super::attempts::{INTENT, Judged, MODEL, PART, attempts, declined, ready};
use super::{OMITTED, route};
use crate::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus};

/// The route of a verdict whose localization is resumed.
const RESUMED: &str = "verify: same bytes, localization resumed";
/// The route of a rejection carried from an earlier round.
const CARRIED: &str = "verify: same bytes, rejected in an earlier round";

/// A judge answering each question from its script in order (`None`: the call fails), keeping
/// the options of each question it was asked; a question beyond its script panics.
struct Script {
    replies: Mutex<VecDeque<Option<&'static str>>>,
    asked: Mutex<Vec<Vec<String>>>,
}

impl Script {
    fn new(replies: impl IntoIterator<Item = Option<&'static str>>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().collect()),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// How many questions reached the judge.
    fn calls(&self) -> usize {
        self.asked.lock().unwrap().len()
    }

    /// The scripted replies no question asked for.
    fn left(&self) -> usize {
        self.replies.lock().unwrap().len()
    }
}

impl ProviderInferDyn for Script {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let ResponseFormat::JsonSchema(schema) = &request.response_format else {
            panic!("a judge question is a closed choice");
        };
        let keys: Vec<String> =
            serde_json::from_value(schema["properties"]["choice"]["enum"].clone()).unwrap();
        self.asked.lock().unwrap().push(keys.clone());
        let reply = (self.replies.lock().unwrap().pop_front())
            .unwrap_or_else(|| panic!("an unscripted question: {keys:?}"));
        let Some(key) = reply else {
            return Err(ProviderError::Other {
                reason: "the judge is unreachable".to_owned(),
            });
        };
        let text = json!({"choice": key}).to_string();
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// The provider judge seated as [`MODEL`].
fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2))
}

/// The native verdict of `out` under `request` as attempt `attempt`, by `judge`, over the run
/// `observation` when one is given.
async fn verdict(
    judge: &Script,
    request: &CompileRequest,
    out: CompileOutcome,
    attempt: usize,
    observation: Option<&Value>,
) -> Judged {
    let reading = crate::lexicon::read(INTENT);
    let seats = (judge, None);
    native_verdict(
        INTENT,
        &reading,
        &policy(),
        seats,
        request,
        out,
        attempt,
        observation,
    )
    .await
}

/// The ids of an attempt's questions, each with whether its answer was read back.
fn asked(attempt: &Value) -> Vec<(String, bool)> {
    (attempt["questions"].as_array().into_iter().flatten())
        .map(|question| {
            let id = question["question"].as_str().unwrap_or_default().to_owned();
            (id, question["read_back"] == true)
        })
        .collect()
}

/// A rejection whose localization stopped at a call with no answer, carried into a later round,
/// is resumed there (R6): its whole-request answer is read back with no call, never asked again,
/// and only the part question it never got an answer to is asked, with the task question it
/// leads to. The resumed verdict, which finished, is what a later round carries: repeated with no
/// call. The latest verdict of those bytes is the one carried.
#[tokio::test]
async fn a_stopped_rejection_resumes_its_localization_in_a_later_round() {
    // Round 1: the whole request is rejected, then the part's call gets no answer.
    let judge = Script::new([Some("unfaithful"), None]);
    let request = CompileRequest::create(INTENT);
    let (first, stopped) = declined(verdict(&judge, &request, ready(), 0, None).await);
    assert!(stopped.stopped && stopped.rejected());
    assert_eq!(stopped.unknown, [PART, INTENT]);
    let earlier = attempts(&first)[0].clone();
    assert_eq!(earlier["stopped"], true);
    // Round 2: the host carries it; the judge is asked the part and its task question only.
    let judge = Script::new([Some("missing"), Some("omitted")]);
    let request = CompileRequest::create(INTENT).with_declined(vec![earlier.clone()]);
    let (second, resumed) = declined(verdict(&judge, &request, ready(), 0, None).await);
    assert_eq!((judge.calls(), judge.left()), (2, 0));
    assert!(!judge.asked.lock().unwrap()[0].contains(&"faithful".to_owned()));
    assert!(!resumed.stopped && !resumed.carried && resumed.same_bytes_as.is_none());
    assert_eq!(resumed.defects, [PART]);
    assert_eq!(resumed.notes, [(PART.to_owned(), OMITTED.to_owned())]);
    assert_eq!(resumed.doubt, ["unfaithful"]);
    assert_eq!(resumed.read_back, 1);
    let later = attempts(&second)[0].clone();
    let questions = [
        ("verify-request".to_owned(), true),
        ("verify-part-0".to_owned(), false),
        ("verify-point-0".to_owned(), false),
    ];
    assert_eq!(asked(&later), questions);
    assert_eq!(later["read_back"], 1);
    assert_eq!(later["attempted"], 2);
    assert_eq!(route(&second).last().map(String::as_str), Some(RESUMED));
    // Round 3: both kept; the latest, finished verdict is repeated with no call.
    let silent = Script::new([]);
    let request = CompileRequest::create(INTENT).with_declined(vec![earlier, later.clone()]);
    let (third, repeated) = declined(verdict(&silent, &request, ready(), 0, None).await);
    assert_eq!(silent.calls(), 0, "no call");
    assert!(repeated.carried && !repeated.stopped);
    assert_eq!(repeated.defects, [PART]);
    assert_eq!(route(&third).last().map(String::as_str), Some(CARRIED));
}

/// A verdict that waited for a whole trial run of its bytes is resumed when this call has one
/// (R6): every answer it got over the bytes is read back with no call, and the question over the
/// run, the discriminating observation it waited for, is the only one asked. Consistent outputs
/// carry the request: READY on the run, never on a second vote over the same bytes. With no run,
/// or a partial one, it is repeated with no call.
#[tokio::test]
async fn a_verdict_that_waited_for_a_run_is_resumed_over_a_whole_run() {
    // The whole request rejected, its one part carried, no task doing more: nothing located,
    // and no run of these bytes, so the doubt stays.
    let judge = Script::new([Some("unfaithful"), Some("carried"), Some("only_requested")]);
    let request = CompileRequest::create(INTENT);
    let (first, waited) = declined(verdict(&judge, &request, ready(), 0, None).await);
    assert_eq!(waited.contested, [INTENT]);
    assert_eq!(
        waited.unsettled,
        ["no trial run of these exact bytes exists in this compile"]
    );
    let sha = knowledge::sha256(first.candidate.as_deref().unwrap());
    let run = |read_whole: bool| {
        json!({"candidate_sha256": sha, "inputs": [], "outputs": [
            {"path": "./out/result.txt", "text": "hello", "written": true,
                "read_whole": read_whole}]})
    };
    // A partial run decides nothing: the verdict is repeated with no call.
    let silent = Script::new([]);
    let partial = run(false);
    let (again, repeated) =
        declined(verdict(&silent, &request, first.clone(), 1, Some(&partial)).await);
    assert_eq!(silent.calls(), 0, "no call");
    assert_eq!(repeated.same_bytes_as, Some(0));
    assert_eq!(
        route(&again).last().map(String::as_str),
        Some("verify: same bytes, earlier verdict stands")
    );
    // A whole run: only the question over it is asked.
    let judge = Script::new([Some("consistent")]);
    let whole = run(true);
    let Ok(out) = verdict(&judge, &request, first, 1, Some(&whole)).await else {
        panic!("consistent outputs over the whole run carry the request");
    };
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(judge.calls(), 1);
    assert!(judge.asked.lock().unwrap()[0].contains(&"consistent".to_owned()));
    let resumed = attempts(&out)[1].clone();
    let questions = [
        ("verify-request".to_owned(), true),
        ("verify-part-0".to_owned(), true),
        ("verify-extra".to_owned(), true),
        ("verify-observed".to_owned(), false),
    ];
    assert_eq!(asked(&resumed), questions);
    assert_eq!(resumed["settled_by"], "verify-observed");
    assert_eq!(resumed["read_back"], 3);
    let steps = route(&out);
    assert!(steps.iter().any(|step| step == RESUMED), "{steps:?}");
}
