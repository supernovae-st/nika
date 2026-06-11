// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Capability-escape detection — does the body FIT the declared `permits:`?
//!
//! Per spec `01-envelope.md` §permits · once `permits:` is present every
//! category is default-deny. This scan flags the **statically-detectable**
//! escapes (`nika check` surface · the runtime `NIKA-SEC-004` catches the
//! dynamic remainder) · an `exec:` task under a `false`/omitted permit or
//! a program outside the allowlist · an `invoke:`/`agent` tool outside
//! `permits.tools`. `fs`/`net` escapes are mostly dynamic (the path/host
//! is often a `${{ }}` value) and live primarily at the runtime check.

use crate::raw::{RawAction, RawWorkflow};
use crate::types::{ExecPermit, Permits};

/// A statically-detectable effect outside the declared boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CapabilityEscape {
    /// The offending task.
    pub task: String,
    /// The capability category (`exec`, `tools`).
    pub category: &'static str,
    /// Human detail (the specific tool/program that escaped).
    pub detail: String,
}

/// Scan a workflow for capability escapes. Empty when no `permits:` block
/// is declared (absent = today's behavior, nothing to enforce).
#[must_use]
pub(super) fn scan_escapes(wf: &RawWorkflow) -> Vec<CapabilityEscape> {
    let Some(permits) = wf.permits.as_ref().map(|p| &p.value) else {
        return Vec::new();
    };
    let mut escapes = Vec::new();
    for task in &wf.tasks {
        let id = &task.value.id.value;
        match &task.value.action {
            RawAction::Exec(a) => check_exec(id, &a.command.value, permits, &mut escapes),
            RawAction::Invoke(a) => {
                if !permits.allows_tool(&a.tool.value) {
                    escapes.push(CapabilityEscape {
                        task: id.clone(),
                        category: "tools",
                        detail: format!("invoke tool `{}` is outside permits.tools", a.tool.value),
                    });
                }
            }
            RawAction::Agent(a) => {
                for tool in &a.tools {
                    if !permits.allows_tool(&tool.value) {
                        escapes.push(CapabilityEscape {
                            task: id.clone(),
                            category: "tools",
                            detail: format!("agent tool `{}` is outside permits.tools", tool.value),
                        });
                    }
                }
            }
            RawAction::Infer(_) => {}
        }
    }
    escapes
}

/// An `exec:` task under a `permits:` boundary. A `false`/omitted permit
/// denies any exec; a program allowlist applies to the literal leading
/// program token (dynamic/pipeline heads are a runtime concern).
fn check_exec(id: &str, command: &str, permits: &Permits, out: &mut Vec<CapabilityEscape>) {
    if !permits.allows_exec() {
        out.push(CapabilityEscape {
            task: id.to_owned(),
            category: "exec",
            detail: "exec task under a boundary that forbids shells".to_owned(),
        });
        return;
    }
    if let Some(ExecPermit::Programs(_)) = permits.exec.as_ref()
        && let Some(program) = leading_program(command)
        && !permits.allows_program(program)
    {
        out.push(CapabilityEscape {
            task: id.to_owned(),
            category: "exec",
            detail: format!("program `{program}` is outside permits.exec allowlist"),
        });
    }
}

/// The leading program token of a command string, when it is a literal
/// bare program. `None` when the head is dynamic (`${{ }}`) — a runtime
/// concern, not a static one.
fn leading_program(command: &str) -> Option<&str> {
    let head = command.split_whitespace().next()?;
    if head.contains('$') || head.contains('{') {
        return None;
    }
    Some(head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn escapes_of(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn no_permits_block_no_escapes() {
        let y = "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { command: \"rm -rf /\" }\n";
        assert!(
            escapes_of(y).is_empty(),
            "absent permits = nothing to enforce"
        );
    }

    #[test]
    fn exec_under_false_permit_escapes() {
        let y = "nika: v1\nworkflow: w\npermits: { exec: false }\ntasks:\n  - id: t\n    exec: { command: \"echo hi\" }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].category, "exec");
    }

    #[test]
    fn exec_outside_program_allowlist_escapes() {
        let y = "nika: v1\nworkflow: w\npermits: { exec: [\"git\", \"cargo\"] }\ntasks:\n  - id: ok\n    exec: { command: \"git status\" }\n  - id: bad\n    exec: { command: \"rm -rf x\" }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "git allowed, rm escapes");
        assert_eq!(e[0].task, "bad");
        assert!(e[0].detail.contains("rm"));
    }

    #[test]
    fn invoke_outside_tools_escapes() {
        let y = "nika: v1\nworkflow: w\npermits: { tools: [\"nika:read\"] }\ntasks:\n  - id: t\n    invoke: { tool: \"nika:write\", args: { path: \"x\", content: \"y\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].category, "tools");
        assert!(e[0].detail.contains("nika:write"));
    }

    #[test]
    fn invoke_inside_tools_glob_is_clean() {
        let y = "nika: v1\nworkflow: w\npermits: { tools: [\"mcp:browser/*\"] }\ntasks:\n  - id: t\n    invoke: { tool: \"mcp:browser/navigate\", args: { url: \"x\" } }\n";
        assert!(escapes_of(y).is_empty());
    }

    #[test]
    fn agent_tool_outside_permits_escapes() {
        let y = "nika: v1\nworkflow: w\npermits: { tools: [\"nika:fetch\"] }\ntasks:\n  - id: t\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:fetch\", \"nika:write\"]\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "fetch allowed, write escapes");
        assert!(e[0].detail.contains("nika:write"));
    }

    #[test]
    fn dynamic_program_is_not_statically_flagged() {
        let y = "nika: v1\nworkflow: w\npermits: { exec: [\"git\"] }\nvars: { cmd: \"git\" }\ntasks:\n  - id: t\n    exec: { command: \"${{ vars.cmd }} status\" }\n";
        assert!(escapes_of(y).is_empty(), "dynamic head = runtime check");
    }
}
