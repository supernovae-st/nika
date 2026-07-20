// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tools capability gates (spec 01 §permits · NIKA-SEC-004) — split
//! out of `dispatch.rs` under the ADR-023 1,500-LOC ceiling: same
//! predicates, same posture, one predicate shared with the static
//! `permits_fit` scan so check≡run cannot drift.

use super::Dispatched;

/// The tools capability boundary (spec 01 §permits · NIKA-SEC-004): once a
/// workflow declares `permits`, every tool the body names — an `invoke:`
/// target or an `agent:` universe entry — must fit `permits.tools`. The
/// verdict is [`nika_schema::types::Permits::allows_tool`] itself — the
/// ONE predicate the
/// static `permits_fit` scan and the e-diff parity battery already pin —
/// so an omitted `tools:` under a declared block is DEFAULT-DENY and
/// check≡run cannot drift. `None` permits = nothing to enforce (today's
/// behavior · the exec gate's posture). A `workflow:` call never reaches
/// here — spec 14's containment law (NIKA-COMP-002) owns it.
pub(super) fn check_tool_permits(
    permits: Option<&nika_schema::types::Permits>,
    note: &str,
    tool: &str,
) -> Option<Dispatched> {
    let permits = permits?;
    if permits.allows_tool(tool) {
        return None;
    }
    Some(Dispatched::security_err(
        note,
        format!("tool {tool:?} is not in the `permits.tools` allowlist"),
    ))
}

/// The agent half of the tools boundary: every entry of the declared
/// `tools:` universe must fit `permits.tools` — one refusal for the
/// whole task (the run-time half of the static `permits_fit` scan).
pub(super) fn check_agent_tools_permits(
    permits: Option<&nika_schema::types::Permits>,
    tools: &[nika_schema::Spanned<String>],
) -> Option<Dispatched> {
    for tool in tools {
        if let Some(denial) = check_tool_permits(permits, "agent · ?", &tool.value) {
            return Some(denial);
        }
    }
    None
}
