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
//! an attested tool-free ACP one-shot for Run in this build: codex-acp
//! exposes no empty-tools session profile (it marks the session root
//! trusted), and the audited claude-agent-acp completion profile is
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
        "codex" => "`codex` has no attested tool-free ACP one-shot profile (codex-acp exposes \
                    no empty-tools session; its measured empty-tools one-shot is the direct \
                    `codex exec`)"
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
        let codex = meet_acp_one_shot("codex").expect_err("codex");
        assert!(codex.witness.contains("codex exec"), "{codex}");
    }
}
