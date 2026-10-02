// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A scripted author reaches the real observed room through the compile entry. The script
//! is a provider double; the room and its Runtime are real. All files are synthetic and the
//! existing room receipt helper records each host call before any assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod room_support;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use nika_onboard::compile::rehearse::{RehearsalFuture, Rehearse};
use nika_onboard::compile::room::ObservedRoom;
use nika_onboard::compile::{
    AuthoringPolicy, Cognition, CompileRequest, CompileStatus, NativeMode, Strategy,
    compile_with_cognition_rehearsed,
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const TEST: &str = concat!(
    module_path!(),
    "::a_native_candidate_rehearses_without_mutating_the_project"
);

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
        let answer = if let Some(choice) = approved {
            json!({"choice": choice})
        } else {
            self.calls.fetch_add(1, Ordering::SeqCst);
            json!({"candidate": self.candidate, "questions": [], "gaps": [], "notes": ""})
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: answer.to_string(),
            }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

struct RecordedRoom {
    room: ObservedRoom,
    calls: AtomicUsize,
}

impl Rehearse for RecordedRoom {
    fn bound(&self) -> Duration {
        self.room.bound()
    }
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }
    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move {
            let at = self.calls.fetch_add(1, Ordering::SeqCst);
            room_support::rehearsed(
                TEST,
                &format!("author-{at}"),
                &self.room,
                candidate,
                inputs,
                targets,
            )
            .await
        })
    }
}

#[tokio::test]
async fn a_native_candidate_rehearses_without_mutating_the_project() {
    let prefix = room_support::unique_prefix();
    let world = room_support::World::with_prefix(&prefix, &[("in/source.txt", room_support::BETA)]);
    world.put("out/copied.txt", "stale target");
    world.put("witness.txt", "unchanged witness");
    let before = world.files();
    let permissions = std::fs::metadata(world.project_path("in/source.txt"))
        .unwrap()
        .permissions();
    let source = room_support::lowered_copy(&prefix)
        .expect("HARNESS_INVALID: fixture program")
        .text;
    let _ = room_support::admitted_digest_of(&source);
    let author = Author {
        candidate: source,
        calls: AtomicUsize::new(0),
    };
    let room = RecordedRoom {
        room: world.room(),
        calls: AtomicUsize::new(0),
    };
    let intent = format!(
        "Read {} and write its unchanged text to {}.",
        world.path("in/source.txt"),
        world.path("out/copied.txt")
    );
    let request = CompileRequest::create(intent).with_authoring_policy(
        AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
            .with_native(NativeMode::Only)
            .with_repairs(0),
    );
    let out = compile_with_cognition_rehearsed(
        &request,
        Cognition {
            provider: Some(&author),
            seat: None,
        },
        Some(&room),
    )
    .await
    .unwrap();
    assert_eq!(
        world.files(),
        before,
        "the source, old target, witness and scratch parent are unchanged"
    );
    assert_eq!(
        std::fs::metadata(world.project_path("in/source.txt"))
            .unwrap()
            .permissions()
            .readonly(),
        permissions.readonly()
    );
    #[cfg(unix)]
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(world.project_path("in/source.txt"))
                .unwrap()
                .permissions()
        ),
        std::os::unix::fs::PermissionsExt::mode(&permissions),
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        out.provenance.strategy,
        Some(Strategy::Native),
        "HARNESS_INVALID: the author was not reached"
    );
    assert_eq!(author.calls.load(Ordering::SeqCst), 1);
    assert_eq!(room.calls.load(Ordering::SeqCst), 1);
    let record = &out.provenance.decision.as_ref().unwrap()["rehearsal"]["reports"][0];
    assert_eq!(record["outcome"]["kind"], "passed");
    assert_eq!(record["read_back"][0]["text"], room_support::BETA);
    assert_eq!(record["room"]["cleaned"], true);
    assert_eq!(record["ledger"]["drained"], true);
    for kind in ["network", "provider", "spawn", "prompt", "secret", "child"] {
        assert_eq!(record["effects"][kind], 0, "{kind}: {record}");
    }
}
