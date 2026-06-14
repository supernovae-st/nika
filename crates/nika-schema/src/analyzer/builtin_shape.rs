// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Builtin arg-shape rules — the statically-checkable contracts of
//! `stdlib/builtins-v0.1.md` the JSON Schema cannot express (deep
//! conformance fixtures 009-012) · `nika:write` requires `content:` ·
//! `nika:done` is valid only inside an `agent:` tools whitelist (the
//! loop sentinel · `NIKA-BUILTIN-DONE-001`) · `nika:jq` takes exactly
//! `expression:` · `nika:wait` takes `duration:` XOR `until:`.

use nika_types::extract::{EXTRACT_MODE_NAMES, ExtractMode};

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

    // Flat required-arg contracts (`nika:write` content · `nika:jq`
    // expression · `nika:fetch` url · …) now live in the catalog's
    // `Builtin::required` and are checked by `check::tools::scan_missing_args`
    // — ONE source, no double report. This ladder keeps only the
    // NON-FLAT contracts the catalog set cannot express.
    match tool {
        "nika:done" => errors.push(shape(
            task,
            tool,
            "is the agent-loop completion sentinel — valid ONLY inside an \
             `agent:` tools whitelist · never a standalone invoke \
             (02-verbs.md §loop semantics · NIKA-BUILTIN-DONE-001)",
            span,
        )),
        "nika:wait" if has("duration") == has("until") => errors.push(shape(
            task,
            tool,
            "takes `duration:` XOR `until:` — exactly one \
             (builtins-v0.1.md §nika:wait)",
            span,
        )),
        // `nika:fetch` keeps its WHOLE contract here (incl. the required
        // `url:`) because the mode-dependent `jq`/`selector` pairings are
        // conditional — splitting `url` to the catalog set would double the
        // surface for one builtin. Its `Builtin::required` stays empty.
        "nika:fetch" => check_fetch_shape(task, tool, args, span, errors),
        _ => {}
    }
}

/// `nika:fetch` static contracts (stdlib §fetch + extract-modes-v0.1.md ·
/// conformance `stdlib/extract-modes/001..004`): the mode set is CLOSED
/// at v0.1 · `jq:` pairs with `mode: jq` exactly · `selector:` pairs
/// with `mode: selector` exactly. A templated `mode:` (`${{ … }}`) is
/// runtime business — statically silent.
fn check_fetch_shape(
    task: &str,
    tool: &str,
    args: Option<&serde_json::Value>,
    span: Span,
    errors: &mut Vec<SchemaError>,
) {
    let object = args.and_then(serde_json::Value::as_object);

    // `url:` is fetch's one REQUIRED argument — a fetch with nothing to
    // fetch should die at check time, not at runtime (review lens 1 P1).
    if !object.is_some_and(|map| map.contains_key("url")) {
        errors.push(shape(
            task,
            tool,
            "requires a `url:` argument — a fetch with nothing to fetch \
             (builtins-v0.1.md §nika:fetch)",
            span,
        ));
        return;
    }
    let mode = match object.and_then(|map| map.get("mode")) {
        // Absent → the spec default (markdown).
        None => Some(ExtractMode::Markdown),
        Some(serde_json::Value::String(raw)) => {
            if raw.contains("${{") {
                None // templated — the pairing is unknowable statically
            } else if let Ok(parsed) = raw.parse::<ExtractMode>() {
                Some(parsed)
            } else {
                errors.push(shape(
                    task,
                    tool,
                    &format!(
                        "`mode: {raw}` is not a stdlib v0.1 extract mode — the set \
                         is closed: {EXTRACT_MODE_NAMES} (extract-modes-v0.1.md)"
                    ),
                    span,
                ));
                return;
            }
        }
        Some(_) => {
            errors.push(shape(
                task,
                tool,
                "`mode:` must be a string (extract-modes-v0.1.md)",
                span,
            ));
            return;
        }
    };

    let has = |key: &str| object.is_some_and(|map| map.contains_key(key));
    let Some(mode) = mode else { return };

    if has("jq") && mode != ExtractMode::Jq {
        errors.push(shape(
            task,
            tool,
            "`jq:` is «a jq expression · only with mode: jq» — pair it with \
             `mode: jq` or drop it (builtins-v0.1.md §nika:fetch)",
            span,
        ));
    }
    if mode == ExtractMode::Jq && !has("jq") {
        errors.push(shape(
            task,
            tool,
            "`mode: jq` requires the `jq:` expression to apply \
             (builtins-v0.1.md §nika:fetch)",
            span,
        ));
    }
    if has("selector") && mode != ExtractMode::Selector {
        errors.push(shape(
            task,
            tool,
            "`selector:` pairs with `mode: selector` only \
             (extract-modes-v0.1.md §selector)",
            span,
        ));
    }
    if mode == ExtractMode::Selector && !has("selector") {
        errors.push(shape(
            task,
            tool,
            "`mode: selector` requires the `selector:` CSS selector \
             (extract-modes-v0.1.md §selector)",
            span,
        ));
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
        // NOTE: flat required-arg contracts (`nika:write` content · `nika:jq`
        // expression) moved to the catalog `Builtin::required` set + the
        // `check::tools::scan_missing_args` check — they are NOT shape-rule
        // findings anymore (tested there). This table keeps the non-flat
        // contracts: the `done` sentinel · the `wait` XOR · `fetch` pairings.
        let cases = [
            ("{}", "nika:done", true), // standalone · always the sentinel error
            (
                r#"{ duration: "5s", until: "2026-12-01T00:00:00Z" }"#,
                "nika:wait",
                true, // both modes
            ),
            ("{}", "nika:wait", true),                     // neither mode
            (r#"{ duration: "5s" }"#, "nika:wait", false), // exactly one
            // nika:fetch — the closed mode set + arg pairings
            // (conformance stdlib/extract-modes/001..004).
            ("{}", "nika:fetch", true),                 // url: is REQUIRED
            ("{ mode: markdown }", "nika:fetch", true), // still no url
            (r#"{ url: "https://x.test" }"#, "nika:fetch", false), // default markdown
            (
                r#"{ url: "https://x.test", mode: article }"#,
                "nika:fetch",
                false,
            ),
            (
                r#"{ url: "https://x.test", mode: raw }"#,
                "nika:fetch",
                false,
            ),
            (
                r#"{ url: "https://x.test", mode: html }"#,
                "nika:fetch",
                true,
            ), // 001: not a mode
            (
                r#"{ url: "https://x.test", mode: llm-txt }"#,
                "nika:fetch",
                true,
            ), // RESERVED
            (
                r#"{ url: "https://x.test", mode: markdown, jq: ".x" }"#,
                "nika:fetch",
                true, // 003: jq only with mode: jq
            ),
            (
                r#"{ url: "https://x.test", jq: ".x" }"#,
                "nika:fetch",
                true, // jq with the DEFAULT mode (markdown) — same violation
            ),
            (
                r#"{ url: "https://x.test", mode: jq, jq: ".items | map(.name)" }"#,
                "nika:fetch",
                false, // 004: the valid pairing
            ),
            (r#"{ url: "https://x.test", mode: jq }"#, "nika:fetch", true), // jq needs jq:
            (
                r#"{ url: "https://x.test", mode: selector }"#,
                "nika:fetch",
                true,
            ), // needs selector:
            (
                r#"{ url: "https://x.test", mode: selector, selector: "div.c" }"#,
                "nika:fetch",
                false,
            ),
            (
                r#"{ url: "https://x.test", mode: text, selector: "div.c" }"#,
                "nika:fetch",
                true, // selector: only with mode: selector
            ),
            (
                r#"{ url: "https://x.test", mode: "${{ inputs.m }}" }"#,
                "nika:fetch",
                false, // templated mode — runtime business, statically silent
            ),
            (r#"{ url: "https://x.test", mode: 5 }"#, "nika:fetch", true), // not a string
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
        // …and cleanup actions face the same shape rules as task actions —
        // a `nika:wait` with neither duration NOR until in an on_finally is
        // the XOR violation (a flat-required miss like `nika:write` content
        // is now the missing-args check's concern · tested there).
        let finally = "nika: v1\nworkflow: t\ntasks:\n  - id: w\n    \
                       exec: { command: echo }\n    on_finally:\n      - invoke:\n          \
                       tool: \"nika:wait\"\n          args: {}\n";
        assert!(has_shape_error(finally, "nika:wait"));
    }
}
