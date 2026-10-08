// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! LIVE qualification of a declared model + native effort over a REAL ACP
//! adapter on this machine — ignored by default, never part of CI.
//!
//! It spawns the registered adapter exactly as a Run seat does and drives
//! the engine's own client: two discovery turns that refuse BEFORE any
//! prompt (an unoffered model, then an unoffered effort, so the refusals
//! list what the live session offers), then one real configured turn with
//! the requested model and effort, read back before its single prompt.
//! Each step prints one JSON line: requested, transmitted and configured
//! values, the provenance, and the answer's length (never its text, never
//! a credential). The responder stays unknown: an ACP prompt result names
//! no model, and an answer is not an attestation.
//!
//! ```text
//! NIKA_LIVE_ACP=codex NIKA_LIVE_MODEL=openai/<offered> NIKA_LIVE_EFFORT=<offered> \
//!   cargo test -p nika-harness --locked live_acp -- --ignored --nocapture
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::disallowed_methods)] // a live, operator-run qualification reads its env

use std::pin::Pin;

use futures_core::Stream as _;
use nika_kernel::ai::harness::{
    AgentBackend as _, HarnessError, HarnessEvent, HarnessOutcome, HarnessRequest, ModelProvenance,
    PermissionDecision,
};

const PROBE: &str = "nika-probe-unoffered";

async fn turn(
    seat: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<HarnessOutcome, HarnessError> {
    let harness = nika_harness::seat_from_id(seat)
        .expect("registry row")
        .expect("the adapter is installed");
    let scratch = tempfile::tempdir().expect("an empty, isolated cwd");
    let mut request = HarnessRequest::new(
        "Answer with exactly the word OK and nothing else. Do not use any tool.",
        scratch.path(),
    )
    .with_requested_effort(effort.map(str::to_owned));
    if let Some(model) = model {
        request = request.with_requested_model(model);
    }
    let mut stream = harness.run_agent(request).await?;
    loop {
        match std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
            Some(Ok(HarnessEvent::Completed { outcome })) => return Ok(*outcome),
            Some(Ok(HarnessEvent::PermissionAsked { reply, .. })) => {
                // No authority is granted to a qualification turn.
                reply.respond(PermissionDecision::Deny);
            }
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(error),
            None => panic!("the stream ended without an outcome"),
        }
    }
}

fn line(
    step: &str,
    model: Option<&str>,
    effort: Option<&str>,
    result: &Result<HarnessOutcome, HarnessError>,
) {
    let row = match result {
        Ok(outcome) => {
            let s = &outcome.selection;
            serde_json::json!({"step": step, "requested_model": model, "requested_effort": effort,
                "outcome": "completed", "configured_model": outcome.observed_model,
                "model_source": outcome.observed_model_source.map(ModelProvenance::as_str),
                "model_option": s.model_option, "transmitted_model": s.transmitted_model,
                "effort_option": s.effort_option, "transmitted_effort": s.transmitted_effort,
                "configured_effort": s.configured_effort,
                "effort_source": s.configured_effort_source.map(ModelProvenance::as_str),
                "changed_mid_turn": s.changed_mid_turn, "answer_bytes": outcome.output.len(),
                "answer_is_ok": outcome.output.trim().trim_end_matches('.').eq_ignore_ascii_case("ok"),
                "usage_reported": outcome.usage.is_some(), "responder": null})
        }
        Err(error) => serde_json::json!({"step": step, "requested_model": model,
            "requested_effort": effort, "outcome": "refused", "error": error.to_string()}),
    };
    // The receipt IS this qualification's output (stdout, one JSON line per step).
    #[allow(clippy::disallowed_macros, clippy::print_stdout)]
    {
        println!("LIVE {row}");
    }
}

/// The `offers: a · b` list a refusal teaches, as values.
fn offered(error: &HarnessError) -> Vec<String> {
    let text = error.to_string();
    let Some(start) = text.find("offers: ") else {
        return Vec::new();
    };
    let tail = &text[start + "offers: ".len()..];
    let end = tail.find(" (").unwrap_or(tail.len());
    tail[..end]
        .split(" · ")
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .collect()
}

#[tokio::test]
#[ignore = "live: spawns a real ACP adapter and spends one subscription turn"]
async fn live_acp_selection() {
    let seat = std::env::var("NIKA_LIVE_ACP").expect("NIKA_LIVE_ACP=<seat id>");
    let prefix = std::env::var("NIKA_LIVE_PREFIX").unwrap_or_else(|_| seat.clone());
    // 1 · an unoffered model refuses before any prompt and lists the offer.
    let probe_model = format!("{prefix}/{PROBE}");
    let models = turn(&seat, Some(&probe_model), None).await;
    line("discover_models", Some(&probe_model), None, &models);
    let models = offered(models.as_ref().expect_err("an unoffered model must refuse"));
    assert!(!models.is_empty(), "the refusal lists the live offer");
    let model =
        std::env::var("NIKA_LIVE_MODEL").unwrap_or_else(|_| format!("{prefix}/{}", models[0]));
    // 2 · an unoffered effort refuses after the model selection, before any prompt.
    let efforts = turn(&seat, Some(&model), Some(PROBE)).await;
    line("discover_efforts", Some(&model), Some(PROBE), &efforts);
    let efforts = offered(
        efforts
            .as_ref()
            .expect_err("an unoffered effort must refuse"),
    );
    let effort = std::env::var("NIKA_LIVE_EFFORT")
        .ok()
        .or_else(|| efforts.last().cloned());
    // 3 · the real turn: model + effort configured and read back, ONE prompt.
    let real = turn(&seat, Some(&model), effort.as_deref()).await;
    line("configured_turn", Some(&model), effort.as_deref(), &real);
    let outcome = real.expect("the configured turn completes");
    if let Some(effort) = &effort {
        assert_eq!(
            outcome.selection.configured_effort.as_deref(),
            Some(effort.as_str())
        );
    }
}
