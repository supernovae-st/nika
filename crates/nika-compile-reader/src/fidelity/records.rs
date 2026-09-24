// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Law 23: the text a read returns is not records. `nika:read` returns ONE string; a `nika:jq`
//! that receives it untouched and whose first operation needs an array or an object passes
//! Check (a builtin's output declares no shape) and fails at Run with NIKA-BUILTIN-JQ-001
//! (measured 2026-09-23 on three seats: `[.[] | select(.id == 42)]` over a read's text).
//!
//! Narrow by construction: only a single text-mode read bound whole to a whole `input`, only
//! the expression's first operation, only the path forms below and the forms of
//! `assets/record_forms.txt`, each measured to fail on a string on the engine's own jq. A
//! `fromjson` first, a string operation, `try`, `?`, `//` and any unknown form are left alone.

use super::{Diagnostic, tool_of};
use serde_json::Value;

/// The first operations that need an array or an object, one per line: a call as its name and
/// `(` (`map(`), a bare name alone (`keys`). Each failed on a read's text (2026-09-24).
const RECORD_FORMS: &str = include_str!("../../assets/record_forms.txt");

/// Law 23 over a candidate (see the module note).
pub fn raw_text_as_records(doc: &Value, out: &mut Vec<Diagnostic>) {
    let Some(tasks) = doc.get("tasks").and_then(Value::as_object) else {
        return;
    };
    for (id, task) in tasks {
        if tool_of(doc, id) != "nika:jq" || task.get("for_each").is_some() {
            continue;
        }
        let (Some(input), Some(expression)) = (
            task.pointer("/invoke/args/input").and_then(Value::as_str),
            task.pointer("/invoke/args/expression")
                .and_then(Value::as_str),
        ) else {
            continue;
        };
        let (Some(read), Some(form)) = (
            read_behind(doc, task, input),
            first_needs_records(expression),
        ) else {
            continue;
        };
        out.push(Diagnostic { kind: "records", message: format!("RAW TEXT AS RECORDS: the task `{id}` applies `{form}` to the text the `nika:read` task `{read}` returns: one string, not records; at Run this is NIKA-BUILTIN-JQ-001. Parse it first: begin the expression with `fromjson | ` for a JSON file, or convert a CSV with `nika:convert` (`from: csv, to: json`) and read that output.") });
    }
}

/// The read task whose untouched text `input` is: `input` is exactly one `${{ with.<key> }}`,
/// that binding exactly one `${{ tasks.<read>.output }}`, and the read a single text-mode
/// `nika:read`.
fn read_behind(doc: &Value, task: &Value, input: &str) -> Option<String> {
    let key = sole_reference(input, "with.")?;
    let bound = task.get("with")?.get(key.as_str())?.as_str()?;
    let reference = sole_reference(bound, "tasks.")?;
    let read = reference.strip_suffix(".output")?;
    let node = doc.get("tasks")?.get(read)?;
    let text_mode = node
        .pointer("/invoke/args/binary")
        .is_none_or(|binary| binary.as_bool() == Some(false));
    (tool_of(doc, read) == "nika:read" && node.get("for_each").is_none() && text_mode)
        .then(|| read.to_owned())
}

/// The path of a string that is exactly one `${{ <prefix><path> }}` template, else None.
fn sole_reference(text: &str, prefix: &str) -> Option<String> {
    let inner = text.trim().strip_prefix("${{")?.strip_suffix("}}")?.trim();
    let path = inner.strip_prefix(prefix)?;
    (!path.is_empty()
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.'))
    .then(|| path.to_owned())
}

/// The first operation of `expression` when it is one this law proves needs records. An
/// alternative (`//`) anywhere, or a `?` right after the operation, may swallow the error: not
/// proven, left alone.
fn first_needs_records(expression: &str) -> Option<String> {
    if expression.contains("//") {
        return None;
    }
    // Array construction and grouping wrap the first term without changing its input.
    let term = expression.trim_start_matches(|c: char| c == '[' || c == '(' || c.is_whitespace());
    let (form, rest) = if let Some(inner) = term.strip_prefix("select(") {
        let inner = inner.trim_start();
        let path = path_form(inner)?;
        (format!("select({path}"), &inner[path.len()..])
    } else if let Some(path) = path_form(term) {
        (path.to_owned(), &term[path.len()..])
    } else {
        let call = call_form(term)?;
        (call.to_owned(), &term[call.len()..])
    };
    (!rest.trim_start().starts_with('?')).then_some(form)
}

/// `.[]`, `.[<integer>]`, `.["<key>"]` or `.<name>` at the start of `term`.
fn path_form(term: &str) -> Option<&str> {
    let body = term.strip_prefix('.')?;
    let len = if body.starts_with("[]") {
        2
    } else if let Some(index) = body.strip_prefix('[') {
        let digits = index.strip_prefix('-').unwrap_or(index);
        let count = digits.chars().take_while(char::is_ascii_digit).count();
        if count > 0 && digits[count..].starts_with(']') {
            1 + (index.len() - digits.len()) + count + 1
        } else if let Some(key) = index.strip_prefix('"') {
            1 + 1 + key.find("\"]")? + 2
        } else {
            return None;
        }
    } else {
        let name = body
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .count();
        if name == 0 || body.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        name
    };
    term.get(..1 + len)
}

/// A listed call (`map(`…) or a listed bare name standing alone at the start of `term`.
fn call_form(term: &str) -> Option<&'static str> {
    RECORD_FORMS.lines().find(|form| {
        if form.ends_with('(') {
            return term.starts_with(form);
        }
        term.strip_prefix(form).is_some_and(|after| {
            after.is_empty()
                || after.starts_with(|c: char| c.is_whitespace() || "|)],;".contains(c))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// read → jq over the read's untouched text, the expression under test.
    fn over_read_text(expression: &str) -> Value {
        serde_json::json!({"tasks": {
            "read_source": {"invoke": {"tool": "nika:read", "args": {"path": "./tickets.json"}}},
            "find": {"with": {"text": "${{ tasks.read_source.output }}"},
                     "invoke": {"tool": "nika:jq", "args": {"input": "${{ with.text }}", "expression": expression}}}
        }})
    }

    fn findings(doc: &Value) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        raw_text_as_records(doc, &mut out);
        out
    }

    /// The table is one form per line: a jq name, then `(` for a call; no blank, no space.
    #[test]
    fn the_record_forms_table_is_one_measured_form_per_line() {
        let forms: Vec<&str> = RECORD_FORMS.lines().collect();
        assert_eq!(forms.len(), 23, "{forms:?}");
        for form in forms {
            let name = form.strip_suffix('(').unwrap_or(form);
            assert!(
                !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{form:?}"
            );
        }
    }

    #[test]
    fn every_proven_first_operation_over_a_reads_text_is_refused_before_ready() {
        // Each form below failed on a string with NIKA-BUILTIN-JQ-001 on the engine's jq
        // (2026-09-24); the first is the measured seat failure. Red at 4a06aa3a: no law exists.
        for (expression, form) in [
            ("[.[] | select(.id == 42)] | first | tojson", ".[]"),
            (".[]", ".[]"),
            (".[0]", ".[0]"),
            (".[-1]", ".[-1]"),
            (".id", ".id"),
            (".[\"id\"]", ".[\"id\"]"),
            ("map(.id)", "map("),
            ("select(.id == 42)", "select(.id"),
            ("group_by(.team) | map(length)", "group_by("),
            ("sort_by(.id)", "sort_by("),
            ("unique_by(.id)", "unique_by("),
            ("min_by(.id)", "min_by("),
            ("max_by(.id)", "max_by("),
            ("to_entries", "to_entries"),
            ("with_entries(.)", "with_entries("),
            ("keys", "keys"),
            ("keys_unsorted", "keys_unsorted"),
            ("add", "add"),
            ("first", "first"),
            ("last | .id", "last"),
            ("sort", "sort"),
            ("unique", "unique"),
            ("(.[] | .id)", ".[]"),
            ("[ .[] ]", ".[]"),
            ("map_values(.)", "map_values("),
            ("from_entries", "from_entries"),
            ("any", "any"),
            ("all", "all"),
            ("min", "min"),
            ("max", "max"),
            ("join(\",\")", "join("),
            ("transpose", "transpose"),
        ] {
            let out = findings(&over_read_text(expression));
            assert_eq!(out.len(), 1, "{expression}: {out:?}");
            assert!(
                out[0].message.contains(&format!("`{form}`"))
                    && out[0].message.contains("`find`")
                    && out[0].message.contains("`read_source`")
                    && out[0].message.contains("fromjson"),
                "{expression}: {}",
                out[0].message
            );
        }
    }

    #[test]
    fn string_operations_a_parse_first_and_unknown_forms_are_left_alone() {
        for expression in [
            "fromjson | [.[] | select(.id == 42)] | first | tojson",
            "split(\"\\n\") | map(rtrimstr(\"\\r\")) | if .[-1] == \"\" then .[:-1] else . end",
            "gsub(\"(?m)^(?<l>.+)$\"; \"OK: \\(.l)\")",
            ".[0:3]",
            ".[2:]",
            "length",
            "test(\"a\")",
            "ascii_downcase",
            "ltrimstr(\"[\")",
            ".",
            "..",
            "select(length > 0)",
            "try .[] catch \"x\"",
            ".[]?",
            ".id?",
            ".id // \"none\"",
            "fromjson? // []",
            "if type == \"string\" then fromjson else . end | .[0].id",
            "reduce .[] as $x (0; . + 1)",
            "first(.[])",
            "flatten",
            "reverse",
            "keysmith",
            "additional",
            "firstname",
            "tojson",
            "explode",
            "@base64",
            "# a comment first\n.[]",
        ] {
            let out = findings(&over_read_text(expression));
            assert!(out.is_empty(), "{expression}: {out:?}");
        }
    }

    #[test]
    fn only_the_untouched_text_of_one_text_mode_read_is_followed() {
        // A parsed value, an object input, a binary read, a fanned-out read, a fetch and a
        // binding that reshapes the output are not the raw text of one read.
        let parsed = serde_json::json!({"tasks": {
            "read_source": {"invoke": {"tool": "nika:read", "args": {"path": "./data.json"}}},
            "parse": {"with": {"text": "${{ tasks.read_source.output }}"},
                      "invoke": {"tool": "nika:jq", "args": {"input": "${{ with.text }}", "expression": "fromjson"}}},
            "count": {"with": {"records": "${{ tasks.parse.output }}"},
                      "invoke": {"tool": "nika:jq", "args": {"input": "${{ with.records }}", "expression": "group_by(.team) | map({key: .[0].team, value: length}) | from_entries"}}}
        }});
        assert!(findings(&parsed).is_empty());
        let object_input = serde_json::json!({"tasks": {
            "read_source": {"invoke": {"tool": "nika:read", "args": {"path": "./data.json"}}},
            "count": {"with": {"text": "${{ tasks.read_source.output }}"},
                      "invoke": {"tool": "nika:jq", "args": {"input": {"records": "${{ with.text }}"}, "expression": ".records | length"}}}
        }});
        assert!(findings(&object_input).is_empty());
        let mut binary = over_read_text(".[]");
        binary["tasks"]["read_source"]["invoke"]["args"]["binary"] = serde_json::json!(true);
        assert!(findings(&binary).is_empty());
        let mut fanned = over_read_text(".[]");
        fanned["tasks"]["read_source"]["for_each"] =
            serde_json::json!({"items": "${{ inputs.paths }}"});
        assert!(findings(&fanned).is_empty());
        let mut fetched = over_read_text(".[]");
        fetched["tasks"]["read_source"]["invoke"]["tool"] = serde_json::json!("nika:fetch");
        assert!(findings(&fetched).is_empty());
        let mut reshaped = over_read_text(".[]");
        reshaped["tasks"]["find"]["with"]["text"] =
            serde_json::json!("${{ tasks.read_source.output.items }}");
        assert!(findings(&reshaped).is_empty());
        let mut elsewhere = over_read_text(".[]");
        elsewhere["tasks"]["find"]["invoke"]["args"]["input"] =
            serde_json::json!("prefix ${{ with.text }}");
        assert!(findings(&elsewhere).is_empty());
        // A text-mode read stated explicitly is still text.
        let mut explicit = over_read_text(".[]");
        explicit["tasks"]["read_source"]["invoke"]["args"]["binary"] = serde_json::json!(false);
        assert_eq!(findings(&explicit).len(), 1);
    }

    #[test]
    fn a_jq_fanned_out_over_items_is_not_judged_as_one_read() {
        let mut doc = over_read_text(".[]");
        doc["tasks"]["find"]["for_each"] =
            serde_json::json!({"items": "${{ inputs.names }}", "fail_fast": true});
        assert!(findings(&doc).is_empty());
    }
}
