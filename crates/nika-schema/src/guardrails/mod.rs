// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Guardrails — agent output validation configuration.
//!
//! Guardrails are post-generation checks applied to LLM output before
//! accepting it as a task result. Each guardrail type defines a check
//! and an escalation action (retry, fail, escalate).

pub mod types;
pub mod validation;

pub use types::{
    EscalationAction, GuardrailConfig, LengthGuardrail, LlmGuardrail, RegexGuardrail,
    SchemaGuardrail,
};
pub use validation::validate_guardrail;
