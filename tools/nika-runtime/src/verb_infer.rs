// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Infer verb dispatch adapter.
//!
//! Thin adapter that bridges `VerbCapabilities` to `nika_verb_infer::run()`.
//! Encapsulates the `infer_caps()` conversion and error mapping so callers
//! can dispatch infer tasks with a single call.
//!
//! This is the Infer arm of the runtime dispatch (W14-B3). The main
//! `dispatch()` match currently leaves its Infer arm as
//! `RuntimeError::NotImplemented` because converting
//! `AnalyzedInferAction` to `InferInput` requires template resolution,
//! provider chain resolution, skills loading, and canary injection
//! (all engine-internal). Session 15 moves that logic here when the
//! Runner is migrated and `task_dispatch` becomes the live path.
//!
//! In the meantime, the engine bridge will call `run_infer()` directly
//! after doing its template resolution + Shield setup (planned W15).

use crate::capabilities::VerbCapabilities;
use crate::error::RuntimeError;
use nika_event::EventLog;
use nika_verb_infer::{InferInput, InferOutput};

/// Dispatch an infer verb with a pre-built `InferInput`.
///
/// Borrows an `InferCaps` slice from `caps` and delegates to
/// `nika_verb_infer::run()`. The caller is responsible for template
/// resolution, provider chain resolution, skills loading, spotlight
/// wrapping, canary injection, and schema instruction (all happen
/// in the engine bridge during S14).
///
/// Returns the full [`InferOutput`] so the caller has access to both
/// the text and the response metadata (token usage, request_id,
/// cost_usd) for event forwarding and downstream processing.
pub async fn run_infer(
    input: &InferInput<'_>,
    caps: &VerbCapabilities,
    event_log: &EventLog,
) -> Result<InferOutput, RuntimeError> {
    let infer_caps = caps.infer_caps();
    nika_verb_infer::run(input, &infer_caps, event_log)
        .await
        .map_err(RuntimeError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time proof that `run_infer` returns a Send future so it
    /// can be spawned via `tokio::spawn`. Same GATE-S13-3 discipline as
    /// run_exec / run_fetch / run_invoke: nothing in the future must
    /// hold a !Send guard across .await.
    #[test]
    fn run_infer_future_is_send() {
        fn assert_send<T: Send>(_: &T) {}

        // Never awaited — just prove the type is Send at compile time.
        let _assert = |input: &InferInput<'static>,
                       caps: &'static VerbCapabilities,
                       ev: &'static EventLog| {
            let fut = run_infer(input, caps, ev);
            assert_send(&fut);
        };
    }
}
