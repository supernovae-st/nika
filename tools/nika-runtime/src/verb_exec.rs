// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exec verb dispatch adapter.
//!
//! Thin adapter that bridges `VerbCapabilities` to `nika_verb_exec::run()`.
//! Encapsulates the `exec_caps()` conversion and error mapping so callers
//! can dispatch exec tasks with a single call.
//!
//! This is the Exec arm of the runtime dispatch (S13-B4). The main
//! `dispatch()` match currently leaves its Exec arm as `todo!()` because
//! converting `AnalyzedExecAction` to `ExecInput` requires template
//! resolution (engine-internal). Session 14 moves that logic here when
//! the Runner is migrated.
//!
//! In the meantime, the engine bridge calls `run_exec()` directly after
//! doing its template resolution + security validation.

use crate::capabilities::VerbCapabilities;
use crate::error::RuntimeError;
use nika_event::EventLog;
use nika_verb_exec::ExecInput;

/// Dispatch an exec verb with a pre-built `ExecInput`.
///
/// Borrows an `ExecCaps` slice from `caps` and delegates to
/// `nika_verb_exec::run()`. The caller is responsible for template
/// resolution and security validation (both happen in the engine
/// bridge during S13).
///
/// Returns the trimmed stdout on success.
pub async fn run_exec(
    input: &ExecInput<'_>,
    caps: &VerbCapabilities,
    event_log: &EventLog,
) -> Result<String, RuntimeError> {
    let exec_caps = caps.exec_caps();
    nika_verb_exec::run(input, &exec_caps, event_log)
        .await
        .map_err(RuntimeError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time proof that `run_exec` returns a Send future so it can
    /// be spawned via `tokio::spawn`. This is the GATE-S13-3 discipline:
    /// nothing in the future must hold a `!Send` guard across `.await`.
    #[test]
    fn run_exec_future_is_send() {
        fn assert_send<T: Send>(_: &T) {}

        // We never actually await — just prove the type is Send at compile time.
        let _assert = |input: &ExecInput<'static>,
                       caps: &'static VerbCapabilities,
                       ev: &'static EventLog| {
            let fut = run_exec(input, caps, ev);
            assert_send(&fut);
        };
    }
}
