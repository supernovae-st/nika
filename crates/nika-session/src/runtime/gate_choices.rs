// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The paused trace owns finite gate options; prose never supplies them.

use super::{GateId, SessionRuntime};

impl SessionRuntime {
    /// The pending gate's exact displayed message and identity. Consumers may use
    /// this as their native control's question when its gate identity matches.
    /// It is read-only, performs no I/O and grants no answer or resume authority.
    #[must_use]
    pub fn waiting_gate_prompt(&self) -> Option<(GateId, String)> {
        let gate = self.pending_gate.as_ref()?;
        Some((GateId::new(&gate.trace, &gate.task), gate.message.clone()))
    }

    /// The pending gate's recorded expiry and the digest of the same trace bytes.
    /// Unknown timing or a changed gate returns None. This display-only projection
    /// uses the runtime ticket's TTL law; it neither answers nor refreshes the gate.
    #[must_use]
    pub fn waiting_gate_expiry(&self) -> Option<(GateId, bool, String)> {
        self.waiting_gate_expiry_at(jiff::Timestamp::now().as_millisecond())
    }

    fn waiting_gate_expiry_at(&self, now_ms: i64) -> Option<(GateId, bool, String)> {
        let gate = self.pending_gate.as_ref()?;
        let raw = std::fs::read_to_string(&gate.trace).ok()?;
        let facts = nika_trace::run_view::RunFacts::of(&gate.trace, &raw)?;
        if facts.pause_gate()?
            != (
                gate.task.as_str(),
                gate.message.as_str(),
                gate.mode.as_str(),
            )
            || facts.pause_shown() != (gate.values.as_ref(), gate.withheld.as_ref())
        {
            return None;
        }
        Some((
            GateId::new(&gate.trace, &gate.task),
            facts.pause_approval_expired(now_ms)?,
            nika_event::source_id::sha256_hex(raw.as_bytes()),
        ))
    }

    /// The active choice gate, its exact recorded options and the digest of the same
    /// trace bytes. Consumers recheck all three before submitting a scoped answer.
    /// Missing, malformed, empty or secret-marker options are not projected. Older
    /// traces without these facts need a new pause to supply native choices.
    /// This read-only projection grants no answer, trust or resume authority.
    #[must_use]
    pub fn waiting_gate_choices(&self) -> Option<(GateId, Vec<String>, String)> {
        let gate = self
            .pending_gate
            .as_ref()
            .filter(|gate| gate.mode == "choice")?;
        let raw = std::fs::read_to_string(&gate.trace).ok()?;
        let facts = nika_trace::run_view::RunFacts::of(&gate.trace, &raw)?;
        if facts.pause_gate()?
            != (
                gate.task.as_str(),
                gate.message.as_str(),
                gate.mode.as_str(),
            )
            || facts.pause_shown() != (gate.values.as_ref(), gate.withheld.as_ref())
        {
            return None;
        }
        let choices = facts.pause_choices()?;
        // Runtime resume's reserved marker sentinel names references, not resolved
        // choice values. Never present one as a value the human can select.
        if choices.is_empty() || choices.iter().any(|choice| choice.contains('\u{f8ff}')) {
            return None;
        }
        Some((
            GateId::new(&gate.trace, &gate.task),
            choices.to_vec(),
            nika_event::source_id::sha256_hex(raw.as_bytes()),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
