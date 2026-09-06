// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A value that LOOKS like a reference and is not one (#1395). The
//! mustache island « {{ inputs.topic }} » (Jinja · Airflow · GitHub
//! Actions) and the shell sigil « $name » as a whole `with:` value are
//! neither refused nor resolved: the engine sends the literal text, so
//! `topic=boundaries` reached a model as « {{ inputs.topic }} » under a
//! green check. The scan names the literal and the one reference form;
//! the parent ladder wraps each row into its `silent-literal` hint.
//!
//! Carved into the analysis substrate at the parent's 15k wall: the rule
//! reads the AST alone.

use nika_schema::raw::{RawAction, RawTask, RawWorkflow};

/// Every silent literal of the workflow: `(task id, advice)` rows in
/// declaration order — effect fields first, then the `with:` bindings.
#[must_use]
pub fn scan(wf: &RawWorkflow) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        scan_task(&task.value, &mut out);
    }
    out
}

fn scan_task(t: &RawTask, out: &mut Vec<(String, String)>) {
    let id = t.id.value.as_str();
    for text in effect_fields(&t.action) {
        for island in mustache_refs(text) {
            out.push((id.to_owned(), format!(
                "`{{{{ {island} }}}}` in `{id}` is literal text here (a mustache island, not a reference) — the reference form is `${{{{ {island} }}}}`"
            )));
        }
    }
    for (name, v) in &t.with {
        let bound = name.value.as_str();
        for text in json_strings(&v.value) {
            for island in mustache_refs(text) {
                out.push((id.to_owned(), format!(
                    "`{{{{ {island} }}}}` bound to `with.{bound}` of `{id}` is literal text (a mustache island, not a reference) — the reference form is `${{{{ {island} }}}}`"
                )));
            }
            if let Some(ident) = shell_sigil(text) {
                out.push((id.to_owned(), format!(
                    "`${ident}` bound to `with.{bound}` of `{id}` is literal text (a shell sigil, not a reference) — a binding reads `${{{{ tasks.{ident}.output }}}}` (an upstream output) or `${{{{ inputs.{ident} }}}}`"
                )));
            }
        }
    }
}

/// The text fields an action sends somewhere: a prompt (+ system), an
/// argv / shell line (+ stdin · env values), an invoke's args strings.
fn effect_fields(action: &RawAction) -> Vec<&str> {
    match action {
        RawAction::Exec(a) => {
            let mut fields = a.command.text_fragments();
            if let Some(stdin) = &a.stdin {
                fields.push(stdin.value.as_str());
            }
            for (_, v) in &a.env {
                fields.push(v.value.as_str());
            }
            fields
        }
        RawAction::Invoke(a) => a
            .args
            .as_ref()
            .map(|args| json_strings(&args.value))
            .unwrap_or_default(),
        RawAction::Infer(a) => prompt_system(a.prompt.value.as_str(), a.system.as_ref()),
        RawAction::Agent(a) => prompt_system(a.prompt.value.as_str(), a.system.as_ref()),
        // The variant is not printed: an action carries prompts and args
        // that may hold resolved secrets, and a panic line is a log.
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and analyzer ship together; fail loud beats silently-wrong output"
        )]
        _ => unreachable!("unknown action variant"),
    }
}

fn prompt_system<'a>(
    prompt: &'a str,
    system: Option<&'a nika_schema::Spanned<String>>,
) -> Vec<&'a str> {
    let mut fields = vec![prompt];
    if let Some(system) = system {
        fields.push(system.value.as_str());
    }
    fields
}

/// Every string inside a JSON value, depth-first.
fn json_strings(value: &serde_json::Value) -> Vec<&str> {
    match value {
        serde_json::Value::String(s) => vec![s.as_str()],
        serde_json::Value::Array(items) => items.iter().flat_map(json_strings).collect(),
        serde_json::Value::Object(map) => map.values().flat_map(json_strings).collect(),
        _ => Vec::new(),
    }
}

/// The reference-shaped mustache islands of one string: « {{ inputs.x }} »
/// whose head is one of the five namespaces or a loop local, and which
/// is NOT the « ${{ » island (the dollar in front is the whole difference).
fn mustache_refs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("{{") {
        let sigil = at > 0 && rest.as_bytes()[at - 1] == b'$';
        let after = &rest[at + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let inner = after[..end].trim();
        if !sigil && reference_shaped(inner) {
            out.push(inner.to_owned());
        }
        rest = &after[end + 2..];
    }
    out
}

/// « inputs.x » · « tasks.a.output » · « item » · « index » — a head from
/// the reference vocabulary, then dotted identifiers (or nothing).
fn reference_shaped(inner: &str) -> bool {
    let (head, tail) = inner.split_once('.').unwrap_or((inner, ""));
    let head_ok = matches!(
        head,
        "inputs" | "const" | "secrets" | "with" | "tasks" | "item" | "index" | "group"
    );
    head_ok
        && tail
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '[' | ']' | '-'))
}

/// A whole value that is a shell sigil — « $a » · « $tasks.a.output » —
/// and nothing else (a « ${{ » island starts with a brace, a « $100 »
/// with a digit: neither is a sigil). Returns the bare identifier.
fn shell_sigil(text: &str) -> Option<&str> {
    let ident = text.strip_prefix('$')?;
    let first = ident.chars().next()?;
    (first.is_ascii_alphabetic() || first == '_')
        .then_some(ident)
        .filter(|i| {
            i.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn islands_and_sigils_are_recognized_edge_by_edge() {
        assert_eq!(
            mustache_refs("Write about {{ inputs.topic }}"),
            ["inputs.topic"]
        );
        assert!(mustache_refs("real ${{ inputs.topic }} island").is_empty());
        assert!(mustache_refs("{{ not a ref }}").is_empty());
        assert_eq!(
            mustache_refs("{{tasks.a.output}} and {{ item }}"),
            ["tasks.a.output", "item"]
        );
        assert_eq!(shell_sigil("$a"), Some("a"));
        assert_eq!(shell_sigil("$tasks.a.output"), Some("tasks.a.output"));
        assert_eq!(shell_sigil("$100 budget"), None);
        assert_eq!(shell_sigil("${{ inputs.x }}"), None);
        assert_eq!(shell_sigil("plain"), None);
    }
}
