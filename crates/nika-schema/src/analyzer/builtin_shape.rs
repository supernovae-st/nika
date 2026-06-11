// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Builtin arg-shape rules — the statically-checkable contracts of
//! `stdlib/builtins-v0.1.md` the JSON Schema cannot express (deep
//! conformance fixtures 009-012) · `nika:write` requires `content:` ·
//! `nika:done` is valid only inside an `agent:` tools whitelist (the
//! loop sentinel · `NIKA-BUILTIN-DONE-001`) · `nika:jq` takes exactly
//! `expression:` · `nika:wait` takes `duration:` XOR `until:`.

use crate::error::SchemaError;
use crate::raw::{RawAction, RawTask};
use crate::source::{Span, Spanned};

/// Run every builtin arg-shape rule over a task's action (and its
/// `on_finally:` cleanup actions — same `invoke:` surface).
pub(super) fn check_builtin_shapes(tasks: &[Spanned<RawTask>], errors: &mut Vec<SchemaError>) {
    for task in tasks {
        let id = task.value.id.value.as_str();
        check_action(&task.value.action, id, errors);
        for cleanup in &task.value.on_finally {
            check_action(&cleanup.value.action, id, errors);
        }
    }
}

fn check_action(action: &RawAction, task: &str, errors: &mut Vec<SchemaError>) {
    let RawAction::Invoke(invoke) = action else {
        return;
    };
    let tool = invoke.tool.value.as_str();
    let span = invoke.tool.span;
    let args = invoke.args.as_ref().map(|a| &a.value);
    let has = |key: &str| -> bool {
        matches!(args, Some(serde_json::Value::Object(map)) if map.contains_key(key))
    };

    match tool {
        "nika:write" if !has("content") => errors.push(shape(
            task,
            tool,
            "requires a `content:` arg — a write with nothing to write \
             (builtins-v0.1.md §nika:write)",
            span,
        )),
        "nika:done" => errors.push(shape(
            task,
            tool,
            "is the agent-loop completion sentinel — valid ONLY inside an \
             `agent:` tools whitelist · never a standalone invoke \
             (02-verbs.md §loop semantics · NIKA-BUILTIN-DONE-001)",
            span,
        )),
        "nika:jq" if !has("expression") => errors.push(shape(
            task,
            tool,
            "requires an `expression:` arg — exactly that name, never \
             `query`/`expr` (builtins-v0.1.md §nika:jq · one name everywhere)",
            span,
        )),
        "nika:wait" if has("duration") == has("until") => errors.push(shape(
            task,
            tool,
            "takes `duration:` XOR `until:` — exactly one \
             (builtins-v0.1.md §nika:wait)",
            span,
        )),
        _ => {}
    }
}

fn shape(task: &str, tool: &str, reason: &str, span: Span) -> SchemaError {
    SchemaError::BadBuiltinArgs {
        task: task.to_owned(),
        tool: tool.to_owned(),
        reason: reason.to_owned(),
        span: Some(span),
    }
}

#[cfg(test)]
mod tests {
    use crate::analyzer::analyze;
    use crate::error::SchemaError;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn has_shape_error(yaml: &str, tool: &str) -> bool {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        analyze(&wf)
            .err()
            .unwrap_or_default()
            .iter()
            .any(|e| matches!(e, SchemaError::BadBuiltinArgs { tool: t, .. } if t == tool))
    }

    #[test]
    fn shape_rules_table() {
        // (args yaml · tool · violates?) — one row per contract direction.
        let cases = [
            (r#"{ path: "./o" }"#, "nika:write", true),
            (r#"{ path: "./o", content: "hi" }"#, "nika:write", false),
            ("{}", "nika:done", true), // standalone · always the sentinel error
            (r#"{ input: [], query: "length" }"#, "nika:jq", true),
            (r#"{ input: [], expression: "length" }"#, "nika:jq", false),
            (
                r#"{ duration: "5s", until: "2026-12-01T00:00:00Z" }"#,
                "nika:wait",
                true, // both modes
            ),
            ("{}", "nika:wait", true),                     // neither mode
            (r#"{ duration: "5s" }"#, "nika:wait", false), // exactly one
        ];
        for (args, tool, violates) in cases {
            let yaml = format!(
                "nika: v1\nworkflow: t\ntasks:\n  - id: a\n    invoke:\n      \
                 tool: \"{tool}\"\n      args: {args}\n"
            );
            assert_eq!(
                has_shape_error(&yaml, tool),
                violates,
                "{tool} · args {args}"
            );
        }
    }

    #[test]
    fn done_in_agent_whitelist_is_legal_and_on_finally_is_checked() {
        // The sentinel is LEGAL as an agent tools entry…
        let agent = "nika: v1\nworkflow: t\ntasks:\n  - id: l\n    agent:\n      \
                     prompt: \"go\"\n      tools: [\"nika:done\"]\n";
        assert!(!has_shape_error(agent, "nika:done"));
        // …and cleanup actions face the same rules as task actions.
        let finally = "nika: v1\nworkflow: t\ntasks:\n  - id: w\n    \
                       exec: { command: echo }\n    on_finally:\n      - invoke:\n          \
                       tool: \"nika:write\"\n          args: { path: \"./log\" }\n";
        assert!(has_shape_error(finally, "nika:write"));
    }
}
