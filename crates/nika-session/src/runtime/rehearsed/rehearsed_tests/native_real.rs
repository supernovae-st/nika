// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Explicitly selected only: scripted native generation, a real observed room and the existing
//! Copy selector share the Session account. This does not exercise a paid provider transport.

use super::*;
use crate::authoring::Reading;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use nika_onboard::compile::{
    AuthoringPolicy, Cognition, CompileRequest, CompileStatus, NativeMode,
    compile_with_cognition_rehearsed,
};

struct Author {
    candidate: String,
    calls: AtomicUsize,
}

impl ProviderInferDyn for Author {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let keys = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema["properties"]["choice"]["enum"].as_array(),
            _ => None,
        };
        let approved = ["faithful", "carried"]
            .into_iter()
            .find(|key| keys.is_some_and(|keys| keys.iter().any(|value| value == *key)));
        let reply = if let Some(choice) = approved {
            json!({"choice": choice})
        } else {
            self.calls.fetch_add(1, Ordering::SeqCst);
            json!({"candidate": self.candidate, "questions": [], "gaps": [], "notes": ""})
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: reply.to_string(),
            }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

#[test]
#[ignore = "runs real rehearsal hosts: select explicitly with per-call receipts"]
fn a_scripted_native_author_and_copy_share_one_real_room_account() {
    const TEST: &str = "a_scripted_native_author_and_copy_share_one_real_room_account";
    let root = project();
    write(root.path(), "out/copied.txt", "stale target");
    write(root.path(), "witness.txt", "unchanged witness");
    let files = || {
        [SOURCE, "out/copied.txt", "witness.txt"]
            .map(|path| std::fs::read(root.path().join(path)).expect("fixture"))
    };
    let before = files();
    let source_mode = std::fs::metadata(root.path().join(SOURCE))
        .expect("synthetic fixture must be available")
        .permissions();
    let subruns = Arc::new(AtomicUsize::new(0));
    let counted = subruns.clone();
    let (mut s, prompts, _) = open_with(root.path(), move |world| {
        Box::new(Logged {
            inner: ObservedRoom::new(world),
            test: TEST,
            subrun: counted.fetch_add(1, Ordering::SeqCst),
        })
    });
    let round = AuthoringRound::new(INTENT);
    let fixture = compile_in(
        &DETERMINISTIC,
        &s.project_context(),
        &round.request(),
        INTENT,
    )
    .expect("HARNESS_INVALID: compiler fixture");
    assert_eq!(
        fixture.status,
        CompileStatus::Ready,
        "HARNESS_INVALID: {fixture:#?}"
    );
    let author = Author {
        candidate: fixture.candidate.expect("HARNESS_INVALID: candidate"),
        calls: AtomicUsize::new(0),
    };
    let request = CompileRequest::create(INTENT).with_authoring_policy(
        AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
            .with_native(NativeMode::Only)
            .with_repairs(0),
    );
    let out = s
        .rehearse_dispatch_at(INTENT, None, |_, host| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("HARNESS_INVALID: executor");
            runtime
                .block_on(compile_with_cognition_rehearsed(
                    &request,
                    Cognition {
                        provider: Some(&author),
                        seat: None,
                    },
                    Some(host),
                ))
                .map_err(crate::authoring::AuthoringError::from)
        })
        .expect("native dispatch");
    assert_eq!(
        author.calls.load(Ordering::SeqCst),
        1,
        "HARNESS_INVALID: native route"
    );
    assert_eq!(s.rehearsals.turn.attempts, 1);
    assert_eq!(files(), before, "no source, target or witness mutation");
    let (_, preview) = proposal(s.settle(round, Reading::Ready(out)));
    assert!(preview.contains("held on every world"), "{preview}");
    assert_eq!(
        s.rehearsals.turn.attempts, 4,
        "native one plus Copy three, each once"
    );
    assert_eq!(subruns.load(Ordering::SeqCst), 4);
    assert_eq!(files(), before);
    assert_eq!(
        std::fs::metadata(root.path().join(SOURCE))
            .expect("synthetic fixture must be available")
            .permissions(),
        source_mode
    );
    assert!(!root.path().join(LANDED).exists());
    facts(s.consent("yes"));
    assert!(root.path().join(LANDED).exists());
    assert_eq!(files(), before, "SaveOnly does not execute the workflow");
    assert!(
        prompts
            .lock()
            .expect("synthetic fixture must be available")
            .is_empty()
    );
}
