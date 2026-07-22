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
/// check≡run cannot drift. F-O8 « absent = zero authority »: `None`
/// permits = every tool refused (the check-time twin is NIKA-AUTH-006).
/// A `workflow:` call never reaches
/// here — spec 14's containment law (NIKA-COMP-002) owns it.
pub(super) fn check_tool_permits(
    permits: Option<&nika_schema::types::Permits>,
    note: &str,
    tool: &str,
) -> Option<Dispatched> {
    let Some(permits) = permits else {
        // F-O8 · absent = zero authority: every tool effect refused.
        return Some(Dispatched::security_err(
            note,
            format!(
                "tool {tool:?} refused: no `permits:` block declared · zero \
                 authority (F-O8) — declare `permits:` to grant it \
                 (`nika check --infer-permits` writes the tightest block)"
            ),
        ));
    };
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

/// The exec capability boundary (spec 01 §permits · NIKA-SEC-004): once a
/// workflow declares `permits`, the exec sink enforces it. Returns `Some(error)`
/// when the command is refused, `None` when permitted. F-O8 « absent = zero
/// authority »: NO `permits:` block = every exec refused before spawn (the
/// check-time twin is NIKA-AUTH-006 · the blocklist floor stays on top,
/// independent; operator policy is nika-policy's job, s8).
///
/// A program allowlist (`Programs`) governs `argv[0]` of the ARRAY form (the
/// unambiguous program); the SHELL form is REFUSED under an allowlist because a
/// pipeline can launch any program, so a single leading token cannot verify it
/// (use the array form). This is STRICTER than the static `nika check` for
/// shell-under-allowlist — the safe direction.
pub(super) fn check_exec_permits(
    permits: Option<&nika_schema::types::Permits>,
    note: &str,
    program: &str,
    is_argv: bool,
) -> Option<Dispatched> {
    use nika_schema::types::ExecPermit;
    let Some(permits) = permits else {
        // F-O8 · absent = zero authority: refuse before spawn (zero process).
        return Some(Dispatched::security_err(
            note,
            "no `permits:` block declared · zero authority (F-O8) — \
             declare `permits:` to grant exec (`nika check --infer-permits` \
             writes the tightest block)",
        ));
    };
    match &permits.exec {
        // Omitted or `false` → this workflow runs zero processes.
        None | Some(ExecPermit::No) => Some(Dispatched::security_err(
            note,
            "exec is not permitted by the workflow `permits` boundary",
        )),
        // `true` → any process (still blocklist-gated at the floor).
        Some(ExecPermit::Any) => None,
        // A program allowlist → ARRAY form only (argv[0] must be listed); the
        // SHELL form cannot be verified (a pipeline can launch any program), so
        // it is refused — use the array form.
        Some(ExecPermit::Programs(allowed)) => {
            if !is_argv {
                return Some(Dispatched::security_err(
                    note,
                    "a shell-string command cannot be verified against a \
                     `permits.exec` program allowlist (a pipeline can launch \
                     any program) — use the array form",
                ));
            }
            if !allowed.iter().any(|p| p == program) {
                return Some(Dispatched::security_err(
                    note,
                    format!("program {program:?} is not in the `permits.exec` allowlist"),
                ));
            }
            None
        }
        // #[non_exhaustive] · a future permit form fails CLOSED.
        Some(_) => Some(Dispatched::security_err(
            note,
            "exec permit form not understood by this engine version",
        )),
    }
}
