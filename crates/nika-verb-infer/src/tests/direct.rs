// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An `infer:` on a seat's direct one-shot (no declared `run.access.protocol: acp`): the
//! seat's command carries the prompt, system, model and a `schema:`, never an image, so a
//! `vision:` input is refused before the seat is spawned (NIKA-432) instead of answered
//! without the images it names.
//!
//! Every input here also asks a reasoning effort, which a direct one-shot refuses before it
//! spawns anything: were the vision refusal ever lost, the seat would still refuse before a
//! process starts, so these tests never reach a real CLI on the machine that runs them.

use nika_error::traits::NikaErrorCode;
use nika_providers::ProvidersConfig;
use nika_types::access::AccessRequirement;

use super::*;

fn verb() -> InferVerb {
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    InferVerb::new(registry, "openai/gpt-5.5")
}

/// A direct-route input (no protocol declared) carrying the effort guard above.
fn ask(vision: Vec<VisionPart>) -> InferInput {
    let guard = AccessRequirement::new().with_effort(Some("high".to_owned()));
    let mut input = InferInput::new("describe the logo").with_requirement(Some(&guard));
    input.vision = vision;
    input
}

fn png() -> VisionPart {
    VisionPart::loaded("./in/logo.png", b"\x89PNG\r\n\x1a\nIHDR-bytes".to_vec())
}

/// Every direct one-shot refuses the image before its seat is spawned, and names the route
/// that carries it: the ACP one-shot where the seat has one that reads images, else an API.
#[tokio::test]
async fn a_vision_file_on_a_direct_seat_is_refused_before_the_seat_is_spawned() {
    for (seat, route) in [
        ("codex", "`run.access: { via: codex, protocol: acp }`"),
        (
            "claude-code",
            "`run.access: { via: claude-code, protocol: acp }`",
        ),
        ("grok-build", "run the task on an API route"),
        ("copilot", "run the task on an API route"),
    ] {
        let err = verb()
            .run_on_harness(seat, ask(vec![png()]))
            .await
            .expect_err("never answered without its image");
        match &err {
            VerbInferError::InvalidParam { param, detail } => {
                assert_eq!(*param, "vision", "{seat}");
                assert!(
                    detail.contains(&format!(
                        "`{seat}` serves this `infer:` through its direct one-shot"
                    )) && detail.contains("never an image")
                        && detail.contains(route)
                        && detail.ends_with("(nothing was sent)"),
                    "{seat}: {detail}"
                );
            }
            other => panic!("{seat}: expected the vision refusal, got {other:?}"),
        }
        assert_eq!(err.nika_code().to_string(), "NIKA-432", "{seat}");
        assert!(
            !err.is_transient(),
            "{seat}: a retry meets the same command"
        );
    }
}

/// A `vision:` URL rides no direct command either: the same refusal, never a dropped URL.
#[tokio::test]
async fn a_vision_url_on_a_direct_seat_is_refused_too() {
    let err = verb()
        .run_on_harness(
            "codex",
            ask(vec![VisionPart::url("https://example.com/logo.png")]),
        )
        .await
        .expect_err("a URL is not sent either");
    assert!(
        matches!(&err, VerbInferError::InvalidParam { param: "vision", detail }
            if detail.contains("never an image")),
        "{err:?}"
    );
}

/// Without `vision:` the direct seat meets its other gates: here the effort guard refuses,
/// so the vision refusal never fires on a task that names no image.
#[tokio::test]
async fn without_vision_the_direct_seat_meets_its_other_gates() {
    let err = verb()
        .run_on_harness("codex", ask(Vec::new()))
        .await
        .expect_err("the effort guard refuses");
    assert!(
        matches!(&err, VerbInferError::HarnessAccess { detail }
            if detail.contains("`high`") && detail.contains("nothing was sent")),
        "{err:?}"
    );
}
