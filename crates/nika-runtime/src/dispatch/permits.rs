// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tools capability gates (spec 01 §permits · NIKA-SEC-004) — split
//! out of `dispatch.rs` under the ADR-023 1,500-LOC ceiling: same
//! predicates, same posture, one predicate shared with the static
//! `permits_fit` scan so check≡run cannot drift.

use super::Dispatched;
use crate::witness::PermitWitness;
use nika_schema::types::Permits;

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
    witness: &PermitWitness,
) -> Option<Dispatched> {
    let Some(permits) = permits else {
        // F-O8 · absent = zero authority: every tool effect refused —
        // EXCEPT the pure-internal class (NEP-0003 law 1 · mirrors the
        // check-time exemption · check≡run: pure compute RUNS under the
        // legal zero, effects refuse).
        if nika_cap::is_pure_internal(tool) {
            witness.record(
                "tool",
                tool,
                "allow",
                "pure-internal exemption (NEP-0003 law 1)",
            );
            return None;
        }
        witness.record(
            "tool",
            tool,
            "deny",
            "absent permits: = zero authority (NEP-0003)",
        );
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
        witness.record("tool", tool, "allow", "permits.tools covers the id");
        return None;
    }
    witness.record("tool", tool, "deny", "not in the permits.tools allowlist");
    Some(Dispatched::security_err(
        note,
        format!("tool {tool:?} is not in the `permits.tools` allowlist"),
    ))
}

/// The data-as-code sink's RUN twin (NEP-0006 law 3 · LAW-AUTH-0327 ·
/// the dynamic case the static classifier defers): the RESOLVED
/// `nika:fetch` URL path is classified against the ONE closed list
/// (`nika_cap::code_bearing_path_class` · the same predicate the check
/// twin and the reference oracle read), honoring the task's declared
/// `inert:` door. Refusal = `NIKA-SEC-004` (one runtime refusal voice ·
/// the detail names the class and both repairs).
pub(super) fn check_fetch_sink(
    inert: Option<&str>,
    note: &str,
    tool: &str,
    args: &serde_json::Value,
    witness: &PermitWitness,
) -> Option<Dispatched> {
    if tool != "nika:fetch" {
        return None;
    }
    if let Some(because) = inert {
        witness.record(
            "sink",
            tool,
            "allow",
            format!("inert door declared · {because}"),
        );
        return None;
    }
    let url = args.get("url")?.as_str()?;
    let path = url::Url::parse(url).ok().map(|u| u.path().to_owned())?;
    let Some((class, ext)) = nika_cap::code_bearing_path_class(&path) else {
        witness.record(
            "sink",
            path,
            "allow",
            "no code-bearing class on the URL path",
        );
        return None;
    };
    witness.record(
        "sink",
        format!("{class} · {ext}"),
        "deny",
        "code-bearing fetch with no inert door (NEP-0006 law 1)",
    );
    Some(Dispatched::security_err(
        note,
        format!(
            "fetch of a code-bearing artifact refused ({class} class · `{ext}`) · {url} is a \
             program, not data: the read hides an execution sink (NEP-0006) · model the \
             acquisition as the exec it feeds (exec: + a program permit) · or declare the read \
             inert on the task (inert: \"<because>\")"
        ),
    ))
}

/// The agent half of the tools boundary: every entry of the declared
/// `tools:` universe must fit `permits.tools` — one refusal for the
/// whole task (the run-time half of the static `permits_fit` scan).
pub(super) fn check_agent_tools_permits(
    permits: Option<&nika_schema::types::Permits>,
    tools: &[nika_schema::Spanned<String>],
    witness: &PermitWitness,
) -> Option<Dispatched> {
    for tool in tools {
        if let Some(denial) = check_tool_permits(permits, "agent · ?", &tool.value, witness) {
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
    witness: &PermitWitness,
) -> Option<Dispatched> {
    use nika_schema::types::ExecPermit;
    let Some(permits) = permits else {
        // F-O8 · absent = zero authority: refuse before spawn (zero process).
        witness.record(
            "exec",
            program,
            "deny",
            "absent permits: = zero authority (NEP-0003)",
        );
        return Some(Dispatched::security_err(
            note,
            "no `permits:` block declared · zero authority (F-O8) — \
             declare `permits:` to grant exec (`nika check --infer-permits` \
             writes the tightest block)",
        ));
    };
    match &permits.exec {
        // Omitted or `false` → this workflow runs zero processes.
        None | Some(ExecPermit::No) => {
            witness.record(
                "exec",
                program,
                "deny",
                "exec is not permitted by the boundary",
            );
            Some(Dispatched::security_err(
                note,
                "exec is not permitted by the workflow `permits` boundary",
            ))
        }
        // `true` → any process (still blocklist-gated at the floor).
        Some(ExecPermit::Any) => {
            witness.record(
                "exec",
                program,
                "allow",
                "exec: true (blocklist floor stays on top)",
            );
            None
        }
        // A program allowlist → ARRAY form only (argv[0] must be listed); the
        // SHELL form cannot be verified (a pipeline can launch any program), so
        // it is refused — use the array form.
        Some(ExecPermit::Programs(allowed)) => {
            if !is_argv {
                witness.record(
                    "exec",
                    program,
                    "deny",
                    "shell form under a program allowlist is unverifiable",
                );
                return Some(Dispatched::security_err(
                    note,
                    "a shell-string command cannot be verified against a \
                     `permits.exec` program allowlist (a pipeline can launch \
                     any program) — use the array form",
                ));
            }
            if !allowed.iter().any(|p| p == program) {
                witness.record("exec", program, "deny", "not in the permits.exec allowlist");
                return Some(Dispatched::security_err(
                    note,
                    format!("program {program:?} is not in the `permits.exec` allowlist"),
                ));
            }
            witness.record("exec", program, "allow", "permits.exec lists the program");
            None
        }
        // #[non_exhaustive] · a future permit form fails CLOSED.
        Some(_) => {
            witness.record("exec", program, "deny", "unknown permit form fails closed");
            Some(Dispatched::security_err(
                note,
                "exec permit form not understood by this engine version",
            ))
        }
    }
}

/// NEP-0005 · the declared `permits.env:` passthrough for the spawn
/// site, witnessed (NEP-0007): the passed names ARE the exercised
/// authority — one `env` frame per spawn, allow-side by construction
/// (an undeclared name never reaches the composition).
pub(super) fn env_passthrough_witnessed(
    permits: Option<&Permits>,
    witness: &PermitWitness,
) -> Vec<String> {
    let names = permits
        .map(|p| p.env_passthrough().to_vec())
        .unwrap_or_default();
    witness.record(
        "env",
        names.join(","),
        "allow",
        "composed floor ∪ declared passthrough (NEP-0005)",
    );
    names
}
