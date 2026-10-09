// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Session owns whether its monetary observation admits a bounded decision call.
//! The selected host adapter and its journal live beside the shared `TypeSafe` transport.
pub(crate) use nika_cli_host::compile::typesafe::session::SessionSeat;
pub use nika_cli_host::compile::typesafe::session::{DECISION_ENV, DECISION_SCHEMA, DecisionSetup};
pub(crate) use nika_providers::admission::admit_unpriced_companion as admit;

pub(super) fn finish(
    out: &mut super::CompileOutcome,
    request: &super::CompileRequest,
    consulted: Option<SessionSeat>,
) {
    if let Some(mut receipt) = consulted.and_then(SessionSeat::finish) {
        if let Some(level) = request
            .authoring
            .as_ref()
            .and_then(|policy| policy.reasoning)
        {
            receipt["reasoning_effort"] = serde_json::Value::from(format!(
                "not applicable · the named level `{}` rides the LLM calls only; the TypeSafe request carries no effort",
                level.word()
            ));
        }
        nika_onboard::knowledge::pin::stamp_seat(out, receipt);
    }
}
