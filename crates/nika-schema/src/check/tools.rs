// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unknown-builtin + unknown-arg detection — `nika:raed` and `nika:jq`'s
//! `data:` typo caught statically, each with the deterministic « did you
//! mean ___? » (rustc's diagnostic model).
//!
//! The `nika:` namespace is CLOSED (23 canonical builtins · stdlib v0.1 ·
//! the same `nika_catalog::all_builtins()` the codegen enum reads — one
//! source, no drift), and so is each builtin's `args:` key set (the
//! `Builtin::args` vocabulary in the same catalog). A typo'd builtin OR a
//! typo'd arg key parses fine and only bites at runtime — a misspelled arg
//! is the worst case: the runtime silently ignores it (`nika:jq` with
//! `data:` instead of `input:` runs jq over `null` and returns `null`,
//! never an error). This check moves both failures to `nika check`, with
//! the fix attached. The `mcp:` namespace stays OPEN (server-defined tools
//! and their args are a runtime discovery concern); glob grant patterns in
//! an agent whitelist are grants, not calls — both skipped.

use nika_catalog::{all_builtins, find_builtin};

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

/// An `invoke` builtin call that passes an `args:` key the builtin does
/// not declare (the `nika:jq` `data:`-vs-`input:` footgun class).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct UnknownArg {
    /// Where the call is (task id · `<id> (on_finally)`).
    pub task: String,
    /// The builtin id as written (`nika:jq`).
    pub tool: String,
    /// The undeclared arg key as written (`data`).
    pub arg: String,
    /// The nearest declared arg key, when one is close enough (`input`
    /// for `inpit`) — the machine-applicable fix. `None` when the typo is
    /// too far for an honest guess (silence beats a wrong suggestion).
    pub suggestion: Option<String>,
}

/// Scan every `invoke` call (main verbs AND `on_finally` cleanups) for
/// `args:` keys outside the named builtin's declared vocabulary.
///
/// Only KNOWN `nika:` builtins are checked — an unknown builtin is already
/// reported by [`scan_unknown_tools`] (no double finding), `mcp:` tools are
/// the open namespace (server-defined args), and a non-object `args:` is a
/// shape defect the conformance pass owns.
#[must_use]
pub(super) fn scan_unknown_args(wf: &RawWorkflow) -> Vec<UnknownArg> {
    let mut findings = Vec::new();
    for task in &wf.tasks {
        let id = &task.value.id.value;
        collect_args(id, &task.value.action, &mut findings);
        for cleanup in &task.value.on_finally {
            collect_args(
                &format!("{id} (on_finally)"),
                &cleanup.value.action,
                &mut findings,
            );
        }
    }
    findings
}

/// Check one action's `args:` keys (only `invoke` carries an `args:` map).
fn collect_args(site: &str, action: &RawAction, out: &mut Vec<UnknownArg>) {
    let RawAction::Invoke(a) = action else {
        return; // exec/infer/agent have typed fields, not a free args map
    };
    let Some(name) = a.tool.value.strip_prefix("nika:") else {
        return; // mcp: args are server-defined (open namespace)
    };
    let Some(builtin) = find_builtin(name) else {
        return; // unknown builtin — scan_unknown_tools owns that finding
    };
    let Some(args) = a.args.as_ref().and_then(|a| a.value.as_object()) else {
        return; // absent or non-object args — nothing to validate here
    };
    for key in args.keys() {
        if builtin.args.contains(&key.as_str()) {
            continue;
        }
        let suggestion = did_you_mean(key, builtin.args.iter().copied()).map(str::to_owned);
        out.push(UnknownArg {
            task: site.to_owned(),
            tool: a.tool.value.clone(),
            arg: key.clone(),
            suggestion,
        });
    }
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

    // ── Finding #6 · unknown builtin arg keys ───────────────────────────

    fn arg_findings_of(yaml: &str) -> Vec<UnknownArg> {
        scan_unknown_args(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn jq_data_typo_is_caught_the_silent_null_footgun() {
        // The anchor case: `data:` instead of `input:` — the runtime would
        // ignore it and emit `null`. Caught here. (`data`→`input` is too
        // far for a suggestion, but the unknown-arg finding is the value.)
        let f = arg_findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", data: { a: 1 } } }\n",
        );
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].tool, "nika:jq");
        assert_eq!(f[0].arg, "data");
        assert_eq!(f[0].task, "t");
    }

    #[test]
    fn near_arg_typo_gets_the_did_you_mean_fix() {
        // `inpit` is one transposition from `input` → suggestion attached.
        let f = arg_findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", inpit: 1 } }\n",
        );
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].arg, "inpit");
        assert_eq!(f[0].suggestion.as_deref(), Some("input"));
    }

    #[test]
    fn declared_args_are_clean() {
        let f = arg_findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: { a: 1 } } }\n",
        );
        assert!(f.is_empty(), "every key is declared: {f:?}");
    }

    #[test]
    fn each_undeclared_key_is_its_own_finding() {
        let f = arg_findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: { tool: \"nika:read\", args: { path: \"./x\", mode: \"r\", extra: 1 } }\n",
        );
        let mut keys: Vec<&str> = f.iter().map(|u| u.arg.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["extra", "mode"],
            "path is declared, the other two are not"
        );
    }

    #[test]
    fn mcp_args_are_the_open_namespace() {
        let f = arg_findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: { tool: \"mcp:browser/navigate\", args: { whatever: 1 } }\n",
        );
        assert!(f.is_empty(), "server-defined args are not validated");
    }

    #[test]
    fn unknown_builtin_args_are_not_double_reported() {
        // `nika:raed` is reported by scan_unknown_tools; its args must NOT
        // also flag here (we can't know a typo'd builtin's vocabulary).
        let f = arg_findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: { tool: \"nika:raed\", args: { wat: 1 } }\n",
        );
        assert!(f.is_empty(), "unknown builtin owns its finding: {f:?}");
    }

    #[test]
    fn on_finally_invoke_args_are_checked() {
        let f = arg_findings_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { command: \"true\" }\n    on_finally:\n      - invoke: { tool: \"nika:write\", args: { path: \"x\", content: \"y\", appnd: true } }\n",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].task, "t (on_finally)");
        assert_eq!(f[0].arg, "appnd");
    }
}
