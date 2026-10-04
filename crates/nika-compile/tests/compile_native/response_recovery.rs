// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The public creation entry's recovery controls: the attached knowledge reaches the first
//! semantic call, an invalid bound is named under a supported policy, a sketch failure is
//! terminal. The source door's own syntax-feedback and transport law lives beside its owner
//! (`nika-compile-cognition`'s `cognition/native/response_recovery_tests.rs`).
use super::*;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use std::collections::VecDeque;
use std::sync::Mutex;

enum Reply {
    Answer(Box<InferResponse>),
    Failed,
    Pending,
}

struct Seat {
    replies: Mutex<VecDeque<Reply>>,
    requests: Mutex<Vec<InferRequest>>,
}

impl Seat {
    fn new(replies: impl IntoIterator<Item = Reply>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl ProviderInferDyn for Seat {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.requests.lock().unwrap().push(request);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected call");
        match reply {
            Reply::Answer(response) => Ok(*response),
            Reply::Failed => Err(ProviderError::Connection {
                reason: "uncertain transport after dispatch".into(),
            }),
            Reply::Pending => std::future::pending().await,
        }
    }
}

fn completed(text: &str) -> InferResponse {
    InferResponse::new(
        vec![ContentBlock::Text { text: text.into() }],
        TokenUsage::new(100, 50),
        StopReason::EndTurn,
    )
}

fn reply(text: &str) -> Reply {
    Reply::Answer(Box::new(completed(text)))
}

fn good() -> String {
    answer(&candidate_a("./data/paiements.csv"), &json!([]))
}

#[tokio::test]
async fn sketch_syntax_failure_keeps_its_existing_terminal_contract() {
    let seat = Seat::new([reply(r#"{"tasks": !}"#), reply(&good())]);
    let request =
        CompileRequest::create(CASE_A).with_authoring_policy(policy(NativeMode::Sketch, 3));
    let out = Box::pin(compile_with_provider(&request, &seat))
        .await
        .unwrap();
    assert_ne!(out.status, CompileStatus::Ready);
    assert!(out.candidate.is_none());
    assert_eq!(seat.calls(), 1);
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 1);
}

#[tokio::test]
async fn attached_knowledge_reaches_the_first_open_generation_with_answers_and_world() {
    use nika_compile::{AuthoringKnowledge, KnowledgeReference};
    use nika_kernel::ai::provider::ResponseFormat;
    let pack = AuthoringKnowledge {
        references: vec![KnowledgeReference {
            id: "block:test-filter-total".into(),
            kind: "block".into(),
            text: "SYNTHETIC-REFERENCE-FILTER-TOTAL".into(),
        }],
        ..AuthoringKnowledge::default()
    };
    let world = json!({"observed":[{"path":"./data/paiements.csv","columns":["statut","montant"],"state":"observed","kind":"csv","values":{"statut":["payé"]}}]});
    let request = CompileRequest::create(CASE_A)
        .with_authoring_policy(policy(NativeMode::Escalate, 1))
        .with_authoring_knowledge(pack)
        .with_knowledge(world)
        .answer("model", "\"deepseek/deepseek-flash\"");
    // The first open generation is the private plan, reading the attached context; a plan with
    // no candidate escalates to the sketch door, whose graph and fills the compiler emits.
    let seat = Seat::new([
        reply("no plan here"),
        reply(&graph_a("./data/paiements.csv")),
        reply(&fills_a()),
    ]);
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&request, &Judged::approving(&seat))
        .await
        .unwrap();
    assert_eq!(seat.calls(), 3);
    let context = &out.provenance.authoring.as_ref().unwrap().context;
    assert_eq!(context[0]["call"], "plan");
    assert_eq!(context[1]["call"], "sketch");
    assert_eq!(context[2]["call"], "fill");
    assert_eq!(
        context[0]["semantic_context"]["world_sha256"]
            .as_str()
            .map(str::len),
        Some(64),
        "{:#}",
        context[0]
    );
    let requests = seat.requests.lock().unwrap();
    let text = format!("{:?}", requests[0].messages);
    for evidence in [
        CASE_A,
        "SYNTHETIC-REFERENCE-FILTER-TOTAL",
        "observed_world",
        "statut",
        "answers_already_given",
        "deepseek/deepseek-flash",
    ] {
        assert!(text.contains(evidence), "missing {evidence}");
    }
    for request in requests.iter() {
        let schema = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema.clone(),
            _ => Value::Null,
        };
        assert!(schema.is_object(), "every authoring call has a schema");
        assert!(
            schema["properties"].get("candidate").is_none()
                && schema["properties"].get("candidate_lines").is_none(),
            "no whole-source schema: {schema}"
        );
    }
    assert_eq!(native_record(&out)["accepted"], true, "{out:#?}");
    let candidate = out.candidate.as_deref().unwrap_or_default();
    assert!(
        !candidate.contains("SYNTHETIC-REFERENCE"),
        "a reference is never authority"
    );
}

#[tokio::test]
async fn invalid_initial_limit_cannot_override_the_hard_limit() {
    // A supported semantic policy, so the retired source-only mode cannot mask the bound.
    for native in [NativeMode::Sketch, NativeMode::Escalate] {
        for initial in [0, 8193] {
            let seat = Seat::new([]);
            let policy = AuthoringPolicy::new("mock/authoring", 8192, Duration::from_secs(2))
                .with_native(native)
                .with_initial_max_tokens(initial);
            let out = compile_with_provider(
                &CompileRequest::create(CASE_A).with_authoring_policy(policy),
                &seat,
            )
            .await
            .unwrap();
            assert!(out.candidate.is_none());
            assert_eq!(seat.calls(), 0);
            assert!(
                out.diagnostics
                    .iter()
                    .any(|d| d.target == "authoring_policy" && d.message.contains("output tokens")),
                "{native:?} {initial}: the bound is named: {out:#?}"
            );
        }
    }
}

/// The sketch door's own failure law at the public entry: a failed or timed-out opening call is
/// terminal (no retry), keeps its provider cause and unknown usage, and never asks the human to
/// replace a request whose validity it says nothing about.
#[tokio::test]
async fn sketch_failures_report_their_cause_without_retry_or_a_replacement_request() {
    for reply in [Reply::Failed, Reply::Pending] {
        let seat = Seat::new([reply]);
        let mut bounded = policy(NativeMode::Sketch, 5);
        bounded.timeout = Duration::from_millis(20);
        let request = CompileRequest::create(CASE_A).with_authoring_policy(bounded);
        let out = Box::pin(compile_with_provider(&request, &seat))
            .await
            .unwrap();
        assert!(out.candidate.is_none(), "{out:#?}");
        assert_eq!(seat.calls(), 1);
        assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
        assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
        let receipt = out.provenance.authoring.as_ref().unwrap();
        assert_eq!(
            (receipt.calls, receipt.input_tokens, receipt.output_tokens),
            (1, None, None)
        );
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.target == "authoring_provider"),
            "the cause is reported: {out:#?}"
        );
    }
}
