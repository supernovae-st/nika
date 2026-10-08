// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Which routes can run a one-shot `infer:` over ACP — the static meet
//! the access plan reads when a workflow declares `run.access.protocol:
//! acp`.
//!
//! An ACP session proves an agentic loop; a one-shot inference also needs
//! a pre-execution guarantee that no tool runs. The direct one-shots
//! (`codex exec`, `claude -p` …) carry such measured profiles, but a
//! direct CLI never satisfies an explicit `acp` selection. No route has
//! an attested tool-free ACP one-shot for Run in this build. Codex ACP is
//! not yet qualified: codex-acp 1.13.1 forwards `CODEX_CONFIG` into the
//! session configuration and codex 0.156.1's `features.shell_tool=false`
//! removes command execution, so a supported avenue exists, but it is
//! not yet a complete profile (`apply_patch` rides the model catalogue,
//! an empty MCP table merges the inherited servers instead of clearing
//! them, a cancellation after a tool notification is not pre-execution
//! isolation). The audited claude-agent-acp completion profile is
//! admitted for the authoring role only. Each refusal names the gap; an
//! `agent:` task on the same route still runs over ACP.

/// Why a route cannot run an `infer:` one-shot over ACP.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{witness}")]
#[non_exhaustive]
pub struct AcpOneShotRefused {
    /// The teaching witness.
    pub witness: String,
}

/// Meet `seat` for an ACP one-shot `infer:`.
///
/// # Errors
///
/// Every route in this build: none has an attested tool-free ACP
/// one-shot profile for Run.
pub fn meet_acp_one_shot(seat: &str) -> Result<(), AcpOneShotRefused> {
    let witness = match seat {
        "codex" => "`codex` has no qualified tool-free ACP one-shot profile yet: codex-acp \
                    1.13.1 forwards `CODEX_CONFIG` and `features.shell_tool=false` removes \
                    command execution, but `apply_patch` rides the model catalogue, an empty MCP \
                    table merges the inherited servers instead of clearing them, and a \
                    cancellation after a tool notification is not pre-execution isolation"
            .to_owned(),
        "claude-code" => "`claude-code`'s audited empty-tools ACP completion profile \
                          (claude-agent-acp 0.81.1) is admitted for the authoring role only, \
                          not yet for a Run `infer:` task"
            .to_owned(),
        other => format!("`{other}` has no attested tool-free ACP one-shot profile"),
    };
    Err(AcpOneShotRefused { witness })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_route_claims_an_acp_one_shot_it_has_not_attested() {
        for seat in ["codex", "claude-code", "kimi-code", "gemini-cli"] {
            let refused = meet_acp_one_shot(seat).expect_err(seat);
            assert!(refused.witness.contains(seat), "{refused}");
        }
        let codex = meet_acp_one_shot("codex").expect_err("codex").witness;
        for named in [
            "CODEX_CONFIG",
            "shell_tool",
            "apply_patch",
            "MCP",
            "cancellation",
        ] {
            assert!(codex.contains(named), "{named}: {codex}");
        }
        assert!(!codex.contains("exposes no empty-tools"), "{codex}");
    }
}
