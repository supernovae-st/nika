// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The choices a candidate makes where the request leaves one open, as the provider judge lists
//! them beside its whole answer: recorded, shown as assumptions and never held; bounded, on one
//! line; a seat is never asked to list them.

use std::time::Duration;

use nika_compile::surface::Binding;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};

use super::super::{Judge, Verdict, state, whole};
use super::{asked, recorded, shown};
use crate::decide::{ChoiceFuture, ChoiceQuestion, DecisionError, DecisionSeat};
use crate::plan::Plan;
use crate::{AuthoringPolicy, CompileRequest, DiagnosticKind, NoProvider};

const INTENT: &str = "Draft a reply to each complaint in ./in/complaints.json.";
const CANDIDATE: &str = "nika: replies\ntasks:\n  read:\n    invoke: { tool: \"nika:read\", args: { path: \"./in/complaints.json\" } }\n";

/// The provider judge: `faithful` to the whole request, with the properties of `more`.
struct Listing(Value);

impl ProviderInferDyn for Listing {
    async fn infer(&self, _request: InferRequest) -> Result<InferResponse, ProviderError> {
        let mut answer = json!({"choice": "faithful", "reason": "the bytes do what it asks"});
        if let (Some(answer), Some(more)) = (answer.as_object_mut(), self.0.as_object()) {
            answer.extend(more.clone());
        }
        let content = vec![ContentBlock::Text {
            text: answer.to_string(),
        }];
        Ok(InferResponse::new(
            content,
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// A seat, never asked here.
struct Silent;

impl DecisionSeat for Silent {
    fn name(&self) -> &'static str {
        "mock/seat"
    }

    fn choose<'a>(&'a self, _question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async { Err(DecisionError("never asked".to_owned())) })
    }
}

/// The whole verdict of [`INTENT`] over [`CANDIDATE`], asked of the provider judge answering
/// with `more`.
async fn judged(more: Value) -> Verdict {
    let request = CompileRequest::create(INTENT);
    let base = state(INTENT, &request, CANDIDATE);
    let binding = Binding::of(INTENT, &request, &Plan::default(), CANDIDATE);
    let (mut verdict, mut out) = (Verdict::default(), crate::initial());
    let policy = AuthoringPolicy::new("mock/judge", 256, Duration::from_secs(2));
    let provider = Listing(more);
    let judge = &Judge::Provider(&policy, &provider);
    let asked = (&base, "fixture");
    whole(INTENT, asked, judge, &binding, None, &mut verdict, &mut out).await;
    verdict
}

/// A choice the request leaves open is listed beside the whole answer, recorded on the attempt
/// and shown as an assumption: the verdict still carries the request, and nothing is held.
#[tokio::test]
async fn a_choice_the_request_leaves_open_is_shown_never_held() {
    let said = "the replies are written in English";
    let verdict = judged(json!({"assumed": [said]})).await;
    assert!(verdict.settled(), "nothing is held");
    assert_eq!(verdict.assumed, [said]);
    let mut entry = json!({});
    recorded(&verdict, &mut entry);
    assert_eq!(entry["assumed"], json!([said]));
    let mut out = crate::initial();
    shown(&verdict, &mut out);
    let [finding] = out.diagnostics.as_slice() else {
        panic!("one finding: {:#?}", out.diagnostics);
    };
    assert_eq!(finding.kind, DiagnosticKind::Applied);
    assert_eq!(finding.target, "verify_assumption");
    assert!(finding.message.contains(said), "{}", finding.message);
}

/// At most three choices are kept, each on one line and cut at 160 characters. A judge that
/// lists none, or a list that is not one, adds nothing and records nothing.
#[tokio::test]
async fn what_the_judge_lists_is_bounded_and_one_line() {
    let long = "x".repeat(200);
    let verdict = judged(json!({"assumed": ["a\nb", "", long, "c", "d"]})).await;
    let cut = format!("{}…", "x".repeat(160));
    assert_eq!(verdict.assumed, ["a b", cut.as_str(), "c"]);
    for more in [json!({}), json!({"assumed": "English"})] {
        let verdict = judged(more).await;
        assert!(verdict.settled() && verdict.assumed.is_empty());
        let mut entry = json!({});
        recorded(&verdict, &mut entry);
        assert_eq!(entry.get("assumed"), None);
    }
}

/// A seat is never asked to list choices: its answers are choices, never text.
#[test]
fn a_seat_is_never_asked_to_list() {
    let question = ChoiceQuestion::new("verify-request", "judge", json!({}), Vec::new());
    let seated = asked(&Judge::<NoProvider>::Seat(&Silent), question.clone());
    assert_eq!(seated, question);
}
