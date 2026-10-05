// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Activity presentation lives with the shared display vocabulary. This compatibility path
//! preserves the exact types; production phases are still emitted by Session and Compiler.
pub use nika_display::activity::*;

/// Observe compiler calls without parsing progress prose or asserting a served model.
pub async fn observe<F: std::future::Future>(future: F) -> F::Output {
    let Some(listener) = current_listener() else {
        return future.await;
    };
    let sink: crate::compile::observe::ActivitySink = std::sync::Arc::new(move |call| {
        use crate::compile::observe::CallState as Source;
        let state = match call.state {
            Source::Started => CallState::Started,
            Source::Finished => CallState::Finished,
            Source::Cancelled => CallState::Cancelled,
            _ => return,
        };
        listener(&call_activity(call.ordinal, call.role, call.model, state));
    });
    crate::compile::observe::observe_activity(sink, future).await
}

pub use nika_display::front_door::SESSION_HELP;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn call_identity_and_failure_lifecycle_are_not_success_or_served_model_claims() {
        for state in [
            CallState::Started,
            CallState::Finished,
            CallState::Cancelled,
        ] {
            let activity = call_activity(7, "fill-repair", "requested/fixture", state);
            let mark = activity.call.as_ref().unwrap();
            assert_eq!(mark.ordinal, 7);
            assert_eq!(mark.model, "requested/fixture");
            assert_eq!(mark.role, "fill-repair");
            assert_eq!(mark.state, state);
            assert_eq!(activity.phase, Phase::Repairing);
            assert_eq!(activity.done, state == CallState::Finished);
            assert!(!activity.note.contains("succeeded"));
            assert!(activity.note.contains("requested"));
        }
    }
}
