// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure run protocol formatting shared by operator and literal input channels.
use crate::output::sh_word;
use nika_runtime::WorkflowPause;
use serde_json::Value;
use std::collections::BTreeMap;

/// ONE `{"paused":{…}}` line — the machine pause contract (ADR-099 rider
/// · additive beside the success/error envelopes): the prompt payload a
/// consumer needs to deliver an answer (`--answer <task>=<value>` at
/// resume · or a serve webhook later). The F-P4 approval ticket rides
/// additively (NEP-0013): the machine consumer sees EXACTLY what an
/// answer would sign — shown-hash · digest · nonce · mint · TTL.
/// `resume_carry` (issue 772 · additive) is the run's own `--var`/
/// `--model` tail, shell-quoted verbatim — the taught line's carry, so
/// a machine consumer reconstructing the resume command drops nothing
/// (the flag-less-resume refusal stays the backstop).
#[must_use]
pub fn paused_envelope_line(pause: &WorkflowPause, carry: &str) -> String {
    let approval = pause.approval.as_ref().map(|t| {
        serde_json::json!({
            "digest": t.digest(),
            "shown_hash": t.content_hash,
            "run_nonce": t.run_nonce,
            "minted_at_ms": t.minted_at_ms,
            "ttl_seconds": t.ttl_seconds,
        })
    });
    serde_json::json!({
        "paused": {
            "task": pause.task,
            "mode": pause.mode,
            "message": pause.message,
            "choices": pause.choices,
            "approval": approval,
            "resume_carry": carry,
        }
    })
    .to_string()
}

/// The stderr resume teaching a PAUSED machine run prints beside its
/// trace anchor — the pause sibling of the failure lane's `autopsy:`
/// line (stateful gauntlet 2026-07-11: the pause had everything the
/// command needs — file · trace · task · mode — and printed none of it).
/// The taught command carries ONE concrete answer and names the
/// alternatives BESIDE it, never inside it: a `|` in the command is a
/// shell PIPE, and the pasted `--answer ask=true|false` silently bound
/// `true` (a human gate answered by the shell — `human said: true`
/// with no human) while the piped-to `false` closed stdout and leaked
/// a broken-pipe panic. A taught line must be paste-safe by
/// construction — the run's own `--var`/`--model` carry rides verbatim
/// for the same reason (a required-input workflow refuses a var-less
/// resume · seo-live-review · 2026-07-31).
#[must_use]
pub fn resume_hint_line(
    file: &str,
    trace: &std::path::Path,
    pause: &WorkflowPause,
    carry: &str,
) -> String {
    let (value, alternatives) = match pause.mode.as_str() {
        "confirm" => ("true".to_owned(), " · or false".to_owned()),
        "choice" if !pause.choices.is_empty() => {
            let rest = &pause.choices[1..];
            let alts = if rest.is_empty() {
                String::new()
            } else {
                format!(" · or {}", rest.join(" · "))
            };
            (pause.choices[0].clone(), alts)
        }
        // `input` takes free text: the quotes make the placeholder
        // paste-safe (a bare <text> would redirect).
        _ => ("\"your answer\"".to_owned(), String::new()),
    };
    format!(
        "resume: nika run {file}{carry} --resume {} --answer {}={value}{alternatives}",
        trace.display(),
        pause.task,
    )
}

/// The run's re-invocation carry — every `--var` the operator passed +
/// the `--model` override, shell-quoted for a paste-able line. Built
/// once per run, threaded to every taught resume line.
#[must_use]
pub fn resume_carry(vars: &[String], model_override: Option<&str>) -> String {
    use std::fmt::Write as _;
    let mut carry = String::new();
    for var in vars {
        // write! to a String is infallible.
        let _ = write!(carry, " --var {}", sh_word(var));
    }
    if let Some(model) = model_override {
        let _ = write!(carry, " --model {}", sh_word(model));
    }
    carry
}

/// ONE `{"error":{"code":…,"message":…}}` line — the machine failure
/// contract (F6). `code` is the first NIKA wire code found in the message
/// (`null` when the failure class carries none, e.g. an unreadable file).
#[must_use]
pub fn error_envelope_line(message: &str) -> String {
    serde_json::json!({
        "error": { "code": first_nika_code(message), "message": message }
    })
    .to_string()
}

/// Best-effort wire-code extraction: the first `NIKA-…` token in a
/// diagnostic (findings render `[NIKA-PARSE-009]` · run details lead with
/// `NIKA-431 · …`). Builtin sub-namespaces can contain underscores
/// (`NIKA-BUILTIN-JSON_MERGE_PATCH-001`). No token, no code.
#[must_use]
pub fn first_nika_code(text: &str) -> Option<&str> {
    let start = text.find("NIKA-")?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c, '-' | '_')))
        .unwrap_or(rest.len());
    let code = rest[..end].trim_end_matches('-');
    // A bare `NIKA-` prefix with no digits is prose, not a code.
    (code.len() > "NIKA-".len() && code.bytes().any(|b| b.is_ascii_digit())).then_some(code)
}

/// The card's outputs note: `outputs → key (type) · key2 (type)` — the
/// export contract's shape at a glance (types only, never a data dump).
/// Two keys shown, the rest counted.
#[must_use]
pub fn outputs_note(outputs: &BTreeMap<String, Value>) -> Option<String> {
    if outputs.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = outputs
        .iter()
        .take(2)
        .map(|(key, value)| format!("{key} ({})", json_type_name(value)))
        .collect();
    if outputs.len() > 2 {
        parts.push(format!("+{} more", outputs.len() - 2));
    }
    Some(format!("outputs → {}", parts.join(" · ")))
}

/// The JSON type vocabulary for the outputs pointer — names only, never
/// values (a summary line, not a data leak into the scrollback).
#[must_use]
pub fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// The export contract's stdout payload (spec 01 §"What leaves a run"): the
/// resolved workflow `outputs:` as ONE JSON object on a single line. An
/// empty map (no `outputs:` declared · or references that no longer
/// resolve) renders `{}` — stdout is ALWAYS a single JSON object in
/// `--output json` mode, a stable machine contract for the composition
/// path (`exec: nika run sub --output json` + `capture: stdout`).
#[must_use]
pub fn outputs_json_line(outputs: &BTreeMap<String, Value>) -> String {
    serde_json::to_string(outputs).unwrap_or_else(|_| "{}".to_owned())
}

/// The one-line message for a findings-render envelope: the first line
/// carrying a wire code (the render wraps it in section noise), else the
/// first non-empty line.
#[must_use]
pub fn envelope_message(text: &str) -> &str {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let first = lines.next().unwrap_or(text);
    std::iter::once(first)
        .chain(lines)
        .find(|l| l.contains("NIKA-"))
        .unwrap_or(first)
        .trim()
}
