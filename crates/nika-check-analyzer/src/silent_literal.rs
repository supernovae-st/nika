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
//!
//! A bare island « {{ name }} » is named only when `name` is a declared
//! input or a `with:` key of its task (#1609): then the reference it meant
//! is certain. Any other bare word stays silent — a wrong suggestion is
//! worse than none.

use std::collections::BTreeSet;

use nika_schema::raw::{RawAction, RawTask, RawWorkflow};

/// Every silent literal of the workflow: `(task id, advice)` rows in
/// declaration order — effect fields first, then the `with:` bindings.
#[must_use]
pub fn scan(wf: &RawWorkflow) -> Vec<(String, String)> {
    let inputs: BTreeSet<&str> = wf
        .inputs
        .iter()
        .map(|(name, _)| name.value.as_str())
        .collect();
    let mut out = Vec::new();
    for task in &wf.tasks {
        scan_task(&task.value, &inputs, &mut out);
    }
    out
}

fn scan_task(t: &RawTask, inputs: &BTreeSet<&str>, out: &mut Vec<(String, String)>) {
    let id = t.id.value.as_str();
    let with: BTreeSet<&str> = t.with.iter().map(|(name, _)| name.value.as_str()).collect();
    // In an effect field a bare name reads the task's own binding first,
    // then a workflow input; a `with:` value cannot read `with.` itself.
    let in_body = |name: &str| {
        if with.contains(name) {
            Some(format!("with.{name}"))
        } else {
            inputs.contains(name).then(|| format!("inputs.{name}"))
        }
    };
    let in_binding = |name: &str| inputs.contains(name).then(|| format!("inputs.{name}"));
    for text in effect_fields(&t.action) {
        for (island, reference) in islands(text, &in_body) {
            out.push((id.to_owned(), format!(
                "`{{{{ {island} }}}}` in `{id}` is literal text here (a mustache island, not a reference) — the reference form is `${{{{ {reference} }}}}`"
            )));
        }
    }
    for (name, v) in &t.with {
        let bound = name.value.as_str();
        for text in json_strings(&v.value) {
            for (island, reference) in islands(text, &in_binding) {
                out.push((id.to_owned(), format!(
                    "`{{{{ {island} }}}}` bound to `with.{bound}` of `{id}` is literal text (a mustache island, not a reference) — the reference form is `${{{{ {reference} }}}}`"
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

/// The silent islands of one string with the reference each one meant: a
/// reference-shaped island as written, and a bare name `resolve` knows as
/// the reference it names.
fn islands(text: &str, resolve: &dyn Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = mustache_refs(text)
        .into_iter()
        .map(|island| (island.clone(), island))
        .collect();
    for name in bare_refs(text) {
        if let Some(reference) = resolve(&name) {
            out.push((name, reference));
        }
    }
    out
}

/// The bare-name mustache islands of one string — « {{ name }} », one
/// identifier, outside the reference vocabulary (those are
/// [`mustache_refs`]) and not the « ${{ » island.
fn bare_refs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("{{") {
        let sigil = at > 0 && rest.as_bytes()[at - 1] == b'$';
        let after = &rest[at + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let inner = after[..end].trim();
        if !sigil && is_bare_name(inner) && !reference_shaped(inner) {
            out.push(inner.to_owned());
        }
        rest = &after[end + 2..];
    }
    out
}

/// One expression identifier: a letter or `_`, then letters, digits or
/// `_`. A dashed name is left out: `${{ inputs.my-name }}` does not parse,
/// so the hint would suggest a reference the check refuses.
fn is_bare_name(inner: &str) -> bool {
    let mut chars = inner.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
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
        "inputs" | "const" | "secrets" | "with" | "tasks" | "item" | "index" | "group" | "run"
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
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn rows_of(yaml: &str) -> Vec<(String, String)> {
        scan(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    /// #1609 · a bare `{{ name }}` that names a declared input or a `with:`
    /// key of its task is the same silent literal as `{{ inputs.name }}`:
    /// the run writes the braces. Measured on 0.123.2: the namespaced form
    /// drew the hint and the bare one drew nothing. An undeclared bare word
    /// stays silent, since a wrong suggestion is worse than none.
    #[test]
    fn a_bare_island_naming_a_declared_input_is_named() {
        let input = rows_of(
            "nika: w\ninputs:\n  name: { type: string, default: \"Ada\" }\npermits:\n  tools: [\"nika:write\"]\ntasks:\n  save:\n    invoke:\n      tool: nika:write\n      args: { path: \"./out.txt\", content: \"Hello {{ name }}\" }\n",
        );
        assert_eq!(input.len(), 1, "{input:?}");
        assert_eq!(input[0].0, "save");
        assert!(input[0].1.contains("${{ inputs.name }}"), "{}", input[0].1);

        let bound = rows_of(
            "nika: w\nmodel: mock/echo\ntasks:\n  greet:\n    infer: { prompt: \"hi\", max_tokens: 8 }\n  say:\n    with:\n      text: ${{ tasks.greet.output }}\n    infer: { prompt: \"Repeat {{ text }}\", max_tokens: 8 }\n",
        );
        assert_eq!(bound.len(), 1, "{bound:?}");
        assert_eq!(bound[0].0, "say");
        assert!(bound[0].1.contains("${{ with.text }}"), "{}", bound[0].1);

        let unknown = rows_of(
            "nika: w\ninputs:\n  name: { type: string, default: \"Ada\" }\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: \"Hello {{ unknown }}\", max_tokens: 8 }\n",
        );
        assert!(unknown.is_empty(), "{unknown:?}");

        let real = rows_of(
            "nika: w\ninputs:\n  name: { type: string, default: \"Ada\" }\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: \"Hello ${{ inputs.name }}\", max_tokens: 8 }\n",
        );
        assert!(real.is_empty(), "{real:?}");
    }

    /// A dashed name is no expression identifier: `${{ inputs.my-name }}`
    /// fails the check with NIKA-VAR-005, so the hint never suggests it.
    /// The plain sibling in the same text is still named.
    #[test]
    fn a_dashed_bare_island_draws_no_reference_the_check_refuses() {
        let rows = rows_of(
            "nika: w\ninputs:\n  name: { type: string, default: \"Ada\" }\n  my-name: { type: string, default: \"Bo\" }\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: \"Hello {{ name }} {{ my-name }}\", max_tokens: 8 }\n",
        );
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert!(rows[0].1.contains("${{ inputs.name }}"), "{}", rows[0].1);
        assert!(
            rows.iter().all(|(_, hint)| !hint.contains("my-name")),
            "{rows:?}"
        );
    }

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
        assert_eq!(
            mustache_refs("for {{ run.scheduled_for }}"),
            ["run.scheduled_for"]
        );
    }
}
