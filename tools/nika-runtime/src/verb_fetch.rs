// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Fetch verb dispatch adapter.
//!
//! Thin adapter that bridges `VerbCapabilities` to `nika_verb_fetch::run()`.
//! Mirrors the exec and invoke adapter pattern.
//!
//! This is the Fetch arm of the runtime dispatch (S13-D2). The main
//! `dispatch()` match currently leaves its Fetch arm as an error stub
//! because converting `AnalyzedFetchAction` to `FetchInput` requires
//! template resolution (engine-internal). Session 14 moves that logic
//! here when the Runner is migrated.

use crate::capabilities::VerbCapabilities;
use crate::error::RuntimeError;
use nika_event::EventLog;
use nika_verb_fetch::FetchInput;

/// Dispatch a fetch verb with a pre-built `FetchInput`.
///
/// Borrows a `FetchCaps` slice from `caps` and delegates to
/// `nika_verb_fetch::run()`.
pub async fn run_fetch(
    input: &FetchInput<'_>,
    caps: &VerbCapabilities,
    event_log: &EventLog,
) -> Result<String, RuntimeError> {
    let fetch_caps = caps.fetch_caps();
    nika_verb_fetch::run(input, &fetch_caps, event_log)
        .await
        .map_err(RuntimeError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GATE-S13-3 discipline: the run_fetch future must be Send.
    #[test]
    fn run_fetch_future_is_send() {
        fn assert_send<T: Send>(_: &T) {}

        let _assert = |input: &FetchInput<'static>,
                       caps: &'static VerbCapabilities,
                       ev: &'static EventLog| {
            let fut = run_fetch(input, caps, ev);
            assert_send(&fut);
        };
    }
}
