// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Invoke verb dispatch adapter.
//!
//! Thin adapter that bridges `VerbCapabilities` to `nika_verb_invoke::run()`.
//! Encapsulates the `invoke_caps()` conversion and error mapping so callers
//! can dispatch invoke tasks with a single call.
//!
//! This is the Invoke arm of the runtime dispatch (S13-C3). The main
//! `dispatch()` match currently leaves its Invoke arm as an error stub
//! because converting `AnalyzedInvokeAction` to `InvokeInput` requires
//! template resolution (engine-internal). Session 14 moves that logic
//! here when the Runner is migrated.
//!
//! In the meantime, the engine bridge calls `nika_verb_invoke::run()`
//! directly for the builtin path (see `nika-engine`'s
//! `runtime/executor/invoke.rs`).

use crate::capabilities::VerbCapabilities;
use crate::error::RuntimeError;
use nika_event::EventLog;
use nika_verb_invoke::InvokeInput;

/// Dispatch an invoke verb with a pre-built `InvokeInput`.
///
/// Borrows an `InvokeCaps` slice from `caps` and delegates to
/// `nika_verb_invoke::run()`. The caller is responsible for template
/// resolution (happens in the engine bridge during S13).
pub async fn run_invoke(
    input: &InvokeInput,
    caps: &VerbCapabilities,
    event_log: &EventLog,
) -> Result<String, RuntimeError> {
    let invoke_caps = caps.invoke_caps();
    nika_verb_invoke::run(input, &invoke_caps, event_log)
        .await
        .map_err(RuntimeError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GATE-S13-3 discipline: the run_invoke future must be Send.
    #[test]
    fn run_invoke_future_is_send() {
        fn assert_send<T: Send>(_: &T) {}

        let _assert = |input: &InvokeInput,
                       caps: &'static VerbCapabilities,
                       ev: &'static EventLog| {
            let fut = run_invoke(input, caps, ev);
            assert_send(&fut);
        };
    }
}
