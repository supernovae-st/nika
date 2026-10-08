// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An explicit reasoning effort is forwarded exactly or refused — never
//! dropped. A direct one-shot seat's command carries no effort selection,
//! so it refuses BEFORE the seat is spawned (the authoring loss the access
//! audit named: `InferRequest.reasoning_effort` used to vanish here).

// The scripted seats are shell scripts: unix only, declared under plain
// `cfg(test)` so the production-file filter reads this module as a test.
#![cfg(unix)]

use nika_kernel::ai::provider::{InferRequest, Message, ProviderInferDyn, ReasoningEffort, Role};

use super::tests::scripted_codex;
use super::*;

/// A scripted `codex` that leaves a marker for every prompt it receives,
/// then answers one plain turn — the witness that a prompt reached it.
fn marking_codex(marker: &Path) -> (tempfile::TempDir, PathBuf) {
    let item = serde_json::json!({"type":"item.completed", "item":{
        "id":"m", "type":"agent_message", "text":"the answer"}})
    .to_string();
    scripted_codex(&format!(
        "printf x >> '{}'\ncat >/dev/null\nprintf '%s\n' '{{\"type\":\"turn.started\"}}'\n\
         printf '%s\n' '{item}'\nprintf '%s\n' '{{\"type\":\"turn.completed\",\"usage\":\
         {{\"input_tokens\":1,\"output_tokens\":1}}}}'\n",
        marker.display()
    ))
}

fn seat(bin: PathBuf) -> InferGradeSeat {
    meet_with_adapter(
        "codex",
        StructuredOutputGrade::JsonSchema,
        Adapter::Codex(CodexExec::with_command(bin)),
    )
    .expect("scripted seat")
}

#[tokio::test]
async fn an_explicit_authoring_effort_is_refused_before_the_native_seat_runs() {
    let room = tempfile::tempdir().expect("tempdir");
    let marker = room.path().join("prompts");
    let (_dir, bin) = marking_codex(&marker);
    let backend = crate::authoring::HarnessAuthoring::with_test_seat("codex", seat(bin));
    let mut request = InferRequest::new(
        "codex/default",
        vec![Message::text(Role::User, "MECHANICS ONLY")],
    );
    request.reasoning_effort = Some(ReasoningEffort::High);
    let err = backend
        .infer(request)
        .await
        .expect_err("an explicit effort the native seat cannot carry refuses");
    let text = err.to_string();
    assert!(
        text.contains("`high`") && text.contains("nothing was sent"),
        "{text}"
    );
    assert!(
        !marker.exists(),
        "zero prompts: the seat was never handed the request"
    );
    let receipt = backend.descriptor().expect("descriptor");
    assert_eq!(receipt["observed"][1]["status"], "failed", "{receipt}");
}

#[tokio::test]
async fn without_an_effort_the_native_seat_still_answers() {
    let room = tempfile::tempdir().expect("tempdir");
    let marker = room.path().join("prompts");
    let (_dir, bin) = marking_codex(&marker);
    let backend = crate::authoring::HarnessAuthoring::with_test_seat("codex", seat(bin));
    let response = backend
        .infer(InferRequest::new(
            "codex/default",
            vec![Message::text(Role::User, "MECHANICS ONLY")],
        ))
        .await
        .expect("an effort-free request keeps the native contract");
    assert!(format!("{:?}", response.content).contains("the answer"));
    assert_eq!(
        std::fs::read_to_string(&marker).expect("one prompt"),
        "x",
        "exactly one prompt reached the seat"
    );
}

#[tokio::test]
async fn a_direct_seat_refuses_an_effort_it_cannot_carry() {
    let room = tempfile::tempdir().expect("tempdir");
    let marker = room.path().join("prompts");
    let (_dir, bin) = marking_codex(&marker);
    let err = seat(bin)
        .run(HarnessInferRequest::new("hi", "openai/gpt-5.5").with_effort(Some("xhigh".into())))
        .await
        .expect_err("the run path refuses too");
    assert!(err.to_string().contains("`xhigh`"), "{err}");
    assert!(!marker.exists(), "zero prompts");
}
