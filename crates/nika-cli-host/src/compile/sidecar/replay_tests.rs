// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A failed judge's real compiler record survives the host's disk roundtrip. Scripted
//! transports exercise the native paths only; this is no model-capability claim.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use nika_onboard::compile::{
    AuthoringPolicy, Cognition, CompileRequest, CompileStatus, NativeMode, NoProvider,
    compile_with_cognition,
    decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionError, DecisionSeat},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

const INTENT: &str = "Write the text hello to ./out/result.txt.";
struct Author(Mutex<VecDeque<String>>, AtomicUsize);
impl ProviderInferDyn for Author {
    async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
        self.1.fetch_add(1, Ordering::SeqCst);
        let text = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected author call");
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}
struct Judge(bool, AtomicUsize);
impl DecisionSeat for Judge {
    fn name(&self) -> &'static str {
        "mock/typed-judge"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.1.fetch_add(1, Ordering::SeqCst);
            assert_eq!(question.id, "verify-request");
            if self.0 {
                Ok(ChoiceAnswer::new("faithful", self.name()))
            } else {
                Err(DecisionError("scripted transport unavailable".into()))
            }
        })
    }
}

#[tokio::test]
async fn an_unjudged_semantic_sidecar_roundtrip_rejudges_the_same_source_without_authoring() {
    let author = Author(
        Mutex::new(VecDeque::from([
            json!({"name":"greeting","tasks":[{"id":"save","verb":"invoke","tool":"nika:write",
            "purpose":"save the greeting","writes":["./out/result.txt"]}],
            "questions":[],"gaps":[],"notes":"graph"})
            .to_string(),
            json!({"fills":[{"task":"save","field":"args.content","value":"hello"}],
            "notes":"fills"})
            .to_string(),
        ])),
        AtomicUsize::new(0),
    );
    let down = Judge(false, AtomicUsize::new(0));
    let policy = AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
        .with_repairs(0);
    let out = compile_with_cognition(
        &CompileRequest::create(INTENT).with_authoring_policy(policy),
        Cognition {
            provider: Some(&author),
            seat: Some(&down),
        },
    )
    .await
    .unwrap();
    assert!(super::resumable(&out), "{out:#?}");
    assert_eq!(author.1.load(Ordering::SeqCst), 2);
    assert_eq!(down.1.load(Ordering::SeqCst), 1);
    let original = out
        .provenance
        .plan
        .as_ref()
        .expect("native semantic record");
    assert_eq!(original["semantic_record"], 1);
    assert!(original.get("strategy").is_none());

    // The production writer and reader, rooted in a tempfile to avoid process-wide cwd changes.
    let root = tempfile::tempdir().unwrap();
    let sha = format!("{:x}", Sha256::digest(INTENT.as_bytes()));
    let path = super::record_in(&sha, &out, &root.path().join(super::DIR)).unwrap();
    let record = super::load_record_from(&sha, &path).expect("written native record reloads");
    assert_eq!(record["resume"], true);
    assert_eq!(record["strategy"], "native");
    let loaded = super::plan_of(&record).unwrap();
    assert_eq!(
        &loaded, original,
        "host must preserve the whole closed record"
    );

    let judge = Judge(true, AtomicUsize::new(0));
    let resumed = compile_with_cognition::<NoProvider>(
        &CompileRequest::create(INTENT).with_plan(loaded.clone()),
        Cognition {
            provider: None,
            seat: Some(&judge),
        },
    )
    .await
    .unwrap();
    assert_eq!(resumed.status, CompileStatus::Ready, "{resumed:#?}");
    assert_eq!(judge.1.load(Ordering::SeqCst), 1);
    assert_eq!(
        author.1.load(Ordering::SeqCst),
        2,
        "source was not regenerated"
    );
    let candidate = resumed.candidate.as_ref().expect("same candidate judged");
    assert_eq!(
        original["final"]["candidate_sha256"],
        format!("{:x}", Sha256::digest(candidate.as_bytes()))
    );

    // Do not repair an already invalid record or weaken the core's closed-key validation.
    let mut poisoned = record.clone();
    poisoned["plan"]["strategy"] = json!("native");
    let rejected = compile_with_cognition::<NoProvider>(
        &CompileRequest::create(INTENT).with_plan(super::plan_of(&poisoned).unwrap()),
        Cognition {
            provider: None,
            seat: Some(&judge),
        },
    )
    .await
    .unwrap();
    assert!(rejected.candidate.is_none());
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|d| d.target == "recorded_plan")
    );
    assert_eq!(
        judge.1.load(Ordering::SeqCst),
        1,
        "bad record never reaches a judge"
    );
}

#[test]
fn legacy_strategy_is_restored_only_when_missing() {
    assert_eq!(
        super::plan_of(&json!({"plan":{"operations":[]}, "strategy":"warm"})),
        Some(json!({"operations":[],"strategy":"warm"}))
    );
    assert_eq!(
        super::plan_of(&json!({"plan":{"strategy":"hot"}, "strategy":"warm"})),
        Some(json!({"strategy":"hot"}))
    );
}
