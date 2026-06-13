// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unknown-builtin detection — `nika:raed` caught statically, with the
//! deterministic « did you mean `nika:read`? » (rustc's diagnostic model).
//!
//! The `nika:` namespace is CLOSED (23 canonical builtins · stdlib v0.1 ·
//! the same `nika_catalog::all_builtins()` the codegen enum reads — one
//! source, no drift). A typo'd builtin parses fine and only dies at
//! runtime dispatch; this check moves that failure to `nika check`, with
//! the fix attached. The `mcp:` namespace stays OPEN (server-defined
//! tools are a runtime discovery concern), and glob entries in `agent
//! tools:` are grants, not calls — both skipped.

use nika_catalog::all_builtins;

use crate::raw::{RawAction, RawWorkflow};

use crate::suggest::did_you_mean;

/// An invoke/agent tool naming a `nika:` builtin that does not exist.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct UnknownTool {
    /// Where the tool is named (task id · `<id> (on_finally)`).
    pub task: String,
    /// The unknown tool id as written (`nika:raed`).
    pub tool: String,
    /// The nearest canonical builtin, when one is close enough
    /// (`nika:read`) — the machine-applicable fix.
    pub suggestion: Option<String>,
}

/// Scan every invoke tool + agent tool entry (main verbs AND `on_finally`
/// cleanups) for unknown `nika:` builtins.
#[must_use]
pub(super) fn scan_unknown_tools(wf: &RawWorkflow) -> Vec<UnknownTool> {
    let mut findings = Vec::new();
    for task in &wf.tasks {
        let id = &task.value.id.value;
        collect(id, &task.value.action, &mut findings);
        for cleanup in &task.value.on_finally {
            collect(
                &format!("{id} (on_finally)"),
                &cleanup.value.action,
                &mut findings,
            );
        }
    }
    findings
}

/// Check one action's tool names.
fn collect(site: &str, action: &RawAction, out: &mut Vec<UnknownTool>) {
    match action {
        RawAction::Invoke(a) => check_tool(site, &a.tool.value, out),
        RawAction::Agent(a) => {
            for tool in &a.tools {
                // a glob entry is a grant pattern, not a concrete call —
                // `nika:*` is checked at runtime against the real dispatch
                if !tool.value.contains('*') {
                    check_tool(site, &tool.value, out);
                }
            }
        }
        RawAction::Exec(_) | RawAction::Infer(_) => {}
    }
}

/// Validate one concrete tool id against the closed `nika:` catalog.
fn check_tool(site: &str, tool: &str, out: &mut Vec<UnknownTool>) {
    let Some(name) = tool.strip_prefix("nika:") else {
        return; // mcp:<server>/<tool> is open — runtime discovery
    };
    if all_builtins().iter().any(|b| b.name == name) {
        return;
    }
    let suggestion = did_you_mean(name, all_builtins().iter().map(|b| b.name))
        .map(|nearest| format!("nika:{nearest}"));
    out.push(UnknownTool {
        task: site.to_owned(),
        tool: tool.to_owned(),
        suggestion,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn findings_of(yaml: &str) -> Vec<UnknownTool> {
        scan_unknown_tools(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn typo_d_builtin_is_caught_with_the_fix() {
        let f = findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].tool, "nika:raed");
        assert_eq!(f[0].suggestion.as_deref(), Some("nika:read"));
    }

    #[test]
    fn canonical_builtins_are_clean() {
        let f = findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n  - id: b\n    invoke: { tool: \"nika:json_merge_patch\", args: { target: {}, patch: {} } }\n",
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn mcp_tools_are_open_namespace() {
        let f = findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: { tool: \"mcp:browser/navigate\", args: { url: \"x\" } }\n",
        );
        assert!(f.is_empty(), "server-defined tools are runtime-discovered");
    }

    #[test]
    fn agent_glob_grants_are_skipped_concrete_entries_checked() {
        let f = findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:*\", \"nika:fetc\", \"mcp:browser/*\"]\n",
        );
        assert_eq!(f.len(), 1, "only the concrete typo flags: {f:?}");
        assert_eq!(f[0].suggestion.as_deref(), Some("nika:fetch"));
    }

    #[test]
    fn the_two_loop_only_builtins_are_known_not_flagged() {
        // `nika:done` and `nika:compose` are loop-only `nika:` builtins
        // (ADR-093) — granting them in an agent whitelist must NOT raise an
        // unknown-tool finding (the contract that keeps `nika:compose` in
        // the closed catalog rather than a separate `agent:` namespace).
        let f = findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:done\", \"nika:compose\"]\n",
        );
        assert!(f.is_empty(), "loop-only builtins are catalogued: {f:?}");
    }

    #[test]
    fn on_finally_cleanup_tools_are_checked() {
        let f = findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { command: \"true\" }\n    on_finally:\n      - invoke: { tool: \"nika:wrte\", args: { path: \"x\", content: \"y\" } }\n",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].task, "t (on_finally)");
        assert_eq!(f[0].suggestion.as_deref(), Some("nika:write"));
    }

    #[test]
    fn far_typo_gets_no_wrong_guess() {
        let f = findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: { tool: \"nika:zzzzzzz\", args: {} }\n",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].suggestion, None, "silence beats a wrong suggestion");
    }
}
