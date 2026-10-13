// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Every workflow fragment the Session shows its author, judged as `nika check` judges a file:
//! the fenced blocks, the inline spans that read as workflow YAML, and the tool forms (a builtin
//! with the arguments it is shown with). A fragment is wrapped into the smallest workflow that
//! holds it: the tasks and constants it reads are stubbed and the boundary it needs is granted,
//! so the oracle judges the fragment's own form, never what the wrapping leaves out. The text
//! judged is the one the Session sends: the instructions and the tools of the first request its
//! author receives.

use std::sync::OnceLock;

use nika_cli_host::oracle::{AuditOptions, audit_source};
use serde_json::{Map, Value, json};

/// The keys of a workflow's envelope.
const ENVELOPE: [&str; 9] = [
    "nika", "model", "inputs", "const", "secrets", "permits", "run", "tasks", "outputs",
];
/// The keys of a task.
const TASK: [&str; 12] = [
    "with", "after", "when", "for_each", "invoke", "infer", "exec", "agent", "retry", "timeout",
    "on_error", "returns",
];
/// The four verbs, as a task writes them.
const VERBS: [&str; 4] = ["invoke:", "infer:", "exec:", "agent:"];
/// What a stubbed task does: one value, no effect.
const STUB: &str = "invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }";

/// What the Session sends its author: its instructions, and every text of its tools.
struct Served {
    instructions: String,
    tools: String,
}

/// The first request a journey's author receives, read once for every test here.
fn served() -> &'static Served {
    static SERVED: OnceLock<Served> = OnceLock::new();
    SERVED.get_or_init(|| {
        let journey = super::run("taught_assert");
        let body = &journey.agent.first().expect("the author was asked").body;
        let system = (body["messages"].as_array().into_iter().flatten())
            .find(|message| message["role"] == "system")
            .expect("the request opens with the Session's instructions");
        let instructions = match &system["content"] {
            Value::String(text) => text.clone(),
            parts => (parts.as_array().into_iter().flatten())
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        };
        let mut tools = Vec::new();
        strings(&body["tools"], &mut tools);
        Served {
            instructions,
            tools: tools.join("\n\n"),
        }
    })
}

/// Every string `value` holds, in order.
fn strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => out.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(map) => map.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}

/// The author's own instructions: the Session's text before the language it adds as reference.
fn instructions() -> &'static str {
    let text = served().instructions.as_str();
    text.split_once("# Reference:").map_or(text, |(own, _)| own)
}

/// The part of the instructions from the heading `from` up to the heading `to`.
fn section(from: &str, to: &str) -> &'static str {
    let text = served().instructions.as_str();
    let start = (text.find(from)).unwrap_or_else(|| panic!("the instructions hold `{from}`"));
    let rest = &text[start..];
    rest.find(to).map_or(rest, |end| &rest[..end])
}

/// A fragment the language refuses, and the oracle's reason.
struct Refused {
    fragment: String,
    why: String,
}

/// Asserts that every fragment `text` shows reads as the language reads it, naming each refused
/// one with why.
fn assert_checks_clean(what: &str, text: &str) {
    let said: Vec<String> = (refused(text).iter())
        .map(|refused| format!("{}\n    → {}", refused.fragment, refused.why))
        .collect();
    assert!(
        said.is_empty(),
        "{what} show forms the language refuses:\n{}",
        said.join("\n")
    );
}

/// The fragments `text` shows that the language refuses.
fn refused(text: &str) -> Vec<Refused> {
    let mut out = Vec::new();
    for fragment in fenced(text) {
        workflow(&fragment, &mut out);
    }
    for paragraph in paragraphs(text) {
        let tools = tools_named(&paragraph);
        for line in (paragraph.lines()).filter(|line| line.contains("`nika:` prefix")) {
            for (name, args) in signatures(line) {
                let stubs: Vec<String> = args.iter().map(|arg| format!("{arg}: \"x\"")).collect();
                let shown = format!("{name} {{{}}}", args.join(", "));
                let args = format!("{{ {} }}", stubs.join(", "));
                arguments(&format!("nika:{name}"), &args, &shown, &mut out);
            }
        }
        for span in spans(&paragraph) {
            match (tool_form(span), key_of(span)) {
                (Some((tool, args)), _) => arguments(tool, args, span, &mut out),
                (None, Some(key)) if !placeholder(span) => {
                    if ENVELOPE.contains(&key) || TASK.contains(&key) || holds_task(span) {
                        workflow(span, &mut out);
                    } else if !tools.is_empty() {
                        let args = format!("{{ {span} }}");
                        let known = (tools.iter())
                            .any(|tool| unknown(tool, &args).is_ok_and(|rows| rows.is_empty()));
                        if !known {
                            let why = format!(
                                "no tool the paragraph names declares it ({})",
                                tools.join(", ")
                            );
                            out.push(Refused {
                                fragment: span.to_owned(),
                                why,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// The oracle's verdict on `source` and its report; the parser's refusal when it does not parse.
fn judged(source: &str) -> Result<(bool, Value), String> {
    let options = AuditOptions::new(None, None);
    let audit = audit_source(source, "fragment.nika", None, None, options)
        .map_err(|error| error.to_string())?;
    let report = serde_json::to_value(&audit.report).expect("a check report serializes");
    Ok((audit.verdict.clean, report))
}

/// Refuses `fragment` unless the workflow wrapping it checks clean.
fn workflow(fragment: &str, out: &mut Vec<Refused>) {
    let mut source = completed(&wrapped(fragment));
    let mut verdict = judged(&source);
    // A fragment that grants nothing is granted what it needs: its form is judged, never a
    // boundary it leaves to the workflow around it.
    let grant = match &verdict {
        Ok((false, report)) if !top_keys(&source).contains(&"permits") => {
            Some(granted(&source, report))
        }
        _ => None,
    };
    if let Some(widened) = grant {
        source = widened;
        verdict = judged(&source);
    }
    let why = match verdict {
        Ok((true, _)) => return,
        Ok((false, report)) => findings(&report).join(" · "),
        Err(why) => why,
    };
    out.push(Refused {
        fragment: fragment.to_owned(),
        why,
    });
}

/// Refuses `shown` when `tool` does not parse with `args` or does not declare one of them.
fn arguments(tool: &str, args: &str, shown: &str, out: &mut Vec<Refused>) {
    let why = match unknown(tool, args) {
        Ok(rows) if rows.is_empty() => return,
        Ok(rows) => format!("`{tool}` declares no {}", rows.join(", ")),
        Err(why) => why,
    };
    out.push(Refused {
        fragment: shown.to_owned(),
        why,
    });
}

/// The arguments among `args` (a flow map) that `tool` does not declare, as the checker names
/// them; the parser's refusal when the form does not parse. A declared argument whose stub value
/// the tool refuses is not one of them: the form shows its name, never its value.
fn unknown(tool: &str, args: &str) -> Result<Vec<String>, String> {
    let source = format!(
        "nika: fragment\npermits:\n  tools: [\"{tool}\"]\ntasks:\n  shown:\n    invoke:\n      tool: \"{tool}\"\n      args: {args}\n"
    );
    let (_, report) = judged(&source)?;
    let rows = (report["unknown_args"].as_array().into_iter().flatten())
        .filter(|row| row["invalid_value"].is_null())
        .chain(report["unknown_tools"].as_array().into_iter().flatten());
    Ok(rows
        .map(|row| {
            (row["arg"].as_str().or_else(|| row["tool"].as_str()))
                .map_or_else(|| row.to_string(), str::to_owned)
        })
        .collect())
}

/// Every finding of `report`, as its code and its message.
fn findings(report: &Value) -> Vec<String> {
    let mut rows = Vec::new();
    codes(report, &mut rows);
    if rows.is_empty() {
        rows.push(format!("not clean: {}", report["capability_escapes"]));
    }
    rows
}

/// The code and the message of every finding `value` holds, in order.
fn codes(value: &Value, rows: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            let code = map.get("code").and_then(Value::as_str);
            if let (Some(code), Some(message)) = (code, map.get("message").and_then(Value::as_str))
            {
                rows.push(format!("{code}: {message}"));
            }
            map.values().for_each(|inner| codes(inner, rows));
        }
        Value::Array(items) => items.iter().for_each(|inner| codes(inner, rows)),
        _ => {}
    }
}

/// The smallest workflow that holds `fragment`: a whole workflow as it is (named when it is not),
/// envelope keys beside one stub task, a task's keys as one task, else a map of tasks.
fn wrapped(fragment: &str) -> String {
    let keys = top_keys(fragment);
    let first = keys.first().copied().unwrap_or_default();
    if keys.contains(&"tasks") {
        if keys.contains(&"nika") {
            return fragment.to_owned();
        }
        return format!("nika: fragment\n{fragment}");
    }
    if ENVELOPE.contains(&first) {
        return format!("nika: fragment\n{fragment}\ntasks:\n  only:\n    {STUB}\n");
    }
    if TASK.contains(&first) {
        let verb = if VERBS.iter().any(|verb| fragment.contains(verb)) {
            String::new()
        } else {
            format!("    {STUB}\n")
        };
        return format!(
            "nika: fragment\ntasks:\n  shown:\n{}{verb}",
            indented(fragment, 4)
        );
    }
    format!("nika: fragment\ntasks:\n{}", indented(fragment, 2))
}

/// The keys `text` writes at its top level, in order.
fn top_keys(text: &str) -> Vec<&str> {
    (text.lines())
        .filter(|line| !line.is_empty() && !line.starts_with([' ', '\t']))
        .filter_map(|line| line.split_once(':').map(|(key, _)| key.trim()))
        .collect()
}

/// `text` with each line moved right by `by` spaces.
fn indented(text: &str, by: usize) -> String {
    let pad = " ".repeat(by);
    let mut out = String::new();
    for line in text.lines() {
        if !line.trim().is_empty() {
            out.push_str(&pad);
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// `source` with a stub for each task it reads or waits for, and for each constant it reads,
/// that it does not define.
fn completed(source: &str) -> String {
    let defined = defined_tasks(source);
    let mut stubbed: Vec<String> = Vec::new();
    for task in references(source, "tasks.")
        .into_iter()
        .chain(after_keys(source))
    {
        if !defined.contains(&task) && !stubbed.contains(&task) {
            stubbed.push(task);
        }
    }
    let mut source = source.to_owned();
    for task in stubbed {
        source = inserted(&source, "tasks:", &format!("  {task}:\n    {STUB}"), 1);
    }
    let undefined = |name: &String| {
        let entry = format!("{name}:");
        !(source.lines()).any(|line| line.starts_with(' ') && line.trim_start().starts_with(&entry))
    };
    let rows: Vec<String> = (references(&source, "const.").into_iter())
        .filter(undefined)
        .map(|name| format!("  {name}: \"https://example.com/{name}\""))
        .collect();
    if rows.is_empty() {
        source
    } else if top_keys(&source).contains(&"const") {
        inserted(&source, "const:", &rows.join("\n"), 1)
    } else {
        inserted(
            &source,
            "tasks:",
            &format!("const:\n{}", rows.join("\n")),
            0,
        )
    }
}

/// The ids of the tasks `source` defines: the keys two spaces in under `tasks:`.
fn defined_tasks(source: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut inside = false;
    for line in source.lines() {
        if !line.is_empty() && !line.starts_with(' ') {
            inside = line.starts_with("tasks:");
            continue;
        }
        let entry = (line.strip_prefix("  ")).filter(|rest| inside && !rest.starts_with(' '));
        if let Some((id, _)) = entry.and_then(|rest| rest.split_once(':')) {
            ids.push(id.trim().to_owned());
        }
    }
    ids
}

/// The names `text` reads after `prefix` (`tasks.` → `top` in `${{ tasks.top.output }}`), each
/// once; a prefix inside a longer name reads nothing.
fn references(text: &str, prefix: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (at, _) in text.match_indices(prefix) {
        let before = text[..at].chars().next_back();
        if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
            continue;
        }
        let name: String = (text[at + prefix.len()..].chars())
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
            .collect();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The tasks the flow `after:` maps of `text` wait for.
fn after_keys(text: &str) -> Vec<String> {
    (text.match_indices("after: {"))
        .filter_map(|(at, found)| text[at + found.len()..].split('}').next())
        .flat_map(|map| map.split(','))
        .filter_map(|entry| {
            entry
                .split_once(':')
                .map(|(task, _)| task.trim().to_owned())
        })
        .collect()
}

/// `source` with `text` placed `offset` lines after the first line that starts with `anchor`
/// (0: before it, 1: after it).
fn inserted(source: &str, anchor: &str, text: &str, offset: usize) -> String {
    let mut lines: Vec<&str> = source.lines().collect();
    let at = (lines.iter())
        .position(|line| line.starts_with(anchor))
        .expect("the workflow holds its anchor");
    lines.insert(at + offset, text);
    lines.join("\n") + "\n"
}

/// `source` granted what its report says it needs.
fn granted(source: &str, report: &Value) -> String {
    let needed = &report["permits"]["needed"];
    let mut grant = Map::new();
    if listed(&needed["tools"]) {
        grant.insert("tools".to_owned(), needed["tools"].clone());
    }
    let fs: Map<String, Value> = (needed["fs"].as_object().into_iter().flatten())
        .filter(|(_, paths)| listed(paths))
        .map(|(kind, paths)| (kind.clone(), paths.clone()))
        .collect();
    if !fs.is_empty() {
        grant.insert("fs".to_owned(), Value::Object(fs));
    }
    if listed(&needed["net"]["http"]) {
        grant.insert("net".to_owned(), json!({"http": needed["net"]["http"]}));
    }
    let permits = format!("permits: {}", Value::Object(grant));
    inserted(source, "tasks:", &permits, 0)
}

/// Whether `value` is a list that holds something.
fn listed(value: &Value) -> bool {
    value.as_array().is_some_and(|items| !items.is_empty())
}

/// The fenced `yaml` blocks of `text`, each moved left by its own indentation.
fn fenced(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut open: Option<Vec<&str>> = None;
    for line in text.lines() {
        let fence = line.trim();
        if open.is_none() {
            if fence == "```yaml" {
                open = Some(Vec::new());
            }
        } else if fence == "```" {
            blocks.extend(open.take().map(|lines| dedented(&lines)));
        } else if let Some(lines) = open.as_mut() {
            lines.push(line);
        }
    }
    blocks
}

/// `lines` moved left by the indentation they share.
fn dedented(lines: &[&str]) -> String {
    let indent = (lines.iter())
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    (lines.iter())
        .map(|line| line.get(indent..).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The paragraphs of `text` outside its fenced blocks: a blank line or a list item starts one.
fn paragraphs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut inside_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            inside_fence = !inside_fence;
            continue;
        }
        if inside_fence {
            continue;
        }
        if line.trim().is_empty() || line.trim_start().starts_with("- ") {
            if !current.is_empty() {
                out.push(current.join("\n"));
            }
            current.clear();
        }
        if !line.trim().is_empty() {
            current.push(line);
        }
    }
    if !current.is_empty() {
        out.push(current.join("\n"));
    }
    out
}

/// The inline code spans of `paragraph`.
fn spans(paragraph: &str) -> Vec<&str> {
    (paragraph.lines())
        .flat_map(|line| line.split('`').skip(1).step_by(2))
        .collect()
}

/// The builtins `paragraph` names (`nika:jq`), each once.
fn tools_named(paragraph: &str) -> Vec<String> {
    let mut tools: Vec<String> = Vec::new();
    for span in spans(paragraph) {
        let name: String = (span.strip_prefix("nika:").unwrap_or_default().chars())
            .take_while(|c| c.is_ascii_lowercase() || *c == '_')
            .collect();
        let tool = format!("nika:{name}");
        if !name.is_empty() && !tools.contains(&tool) {
            tools.push(tool);
        }
    }
    tools
}

/// A builtin shown with its arguments (`nika:convert {from: csv, to: json}`): the tool and the
/// flow map.
fn tool_form(span: &str) -> Option<(&str, &str)> {
    let (tool, args) = span.split_once(' ')?;
    let named = (tool.strip_prefix("nika:")).is_some_and(|name| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
    });
    (named && args.starts_with('{') && args.ends_with('}')).then_some((tool, args))
}

/// The key a span opens with when it reads as YAML (`after: { tests: success }` → `after`).
fn key_of(span: &str) -> Option<&str> {
    let (key, rest) = span.split_once(':')?;
    let named = key.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && (key.chars()).all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    (named && rest.starts_with(' ') && !rest.trim().is_empty()).then_some(key)
}

/// Whether a span stands for a form rather than showing one (`<kebab-id>`, `…`).
fn placeholder(span: &str) -> bool {
    (span.contains('<') && span.contains('>')) || span.contains("...") || span.contains('…')
}

/// Whether a span's value holds a task's keys (`read_source: { with: …, invoke: … }`).
fn holds_task(span: &str) -> bool {
    let value = span.split_once(':').map_or("", |(_, value)| value);
    TASK.iter()
        .any(|field| value.contains(&format!("{field}:")))
}

/// The builtins a `nika:`-prefix line lists with their arguments (`write {path, content}`).
fn signatures(line: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for (at, _) in line.match_indices(" {") {
        let mut name: Vec<char> = (line[..at].chars().rev())
            .take_while(|c| c.is_ascii_lowercase() || *c == '_')
            .collect();
        name.reverse();
        let body = line[at + 2..].split('}').next().unwrap_or_default();
        let names_only =
            (body.chars()).all(|c| c.is_ascii_lowercase() || matches!(c, '_' | ',' | ' '));
        if !name.is_empty() && names_only {
            let args: Vec<String> = (body.split(','))
                .map(|arg| arg.trim().to_owned())
                .filter(|arg| !arg.is_empty())
                .collect();
            out.push((name.into_iter().collect::<String>(), args));
        }
    }
    out
}

/// The author's own instructions and its tools show only forms the language reads: every
/// fragment checks clean once wrapped into the smallest workflow that holds it.
#[test]
fn every_fragment_the_author_instructions_show_checks_clean() {
    assert_checks_clean("the author's instructions", instructions());
    assert_checks_clean("the author's tools", &served().tools);
}

/// The compile card lines the Session includes show only forms the language reads: each builtin
/// with the arguments it declares, each fragment parsed and checked.
#[test]
fn every_fragment_the_card_lines_show_checks_clean() {
    let card = section("# The language in one page", "# Output conventions");
    assert_checks_clean("the card lines", card);
}

/// The check goes red on a copy of the instructions that shows one refused form, an `after:` list
/// in place of the map: the instructions themselves are never edited for it.
#[test]
#[should_panic(expected = "never a list")]
fn an_after_list_in_a_copy_of_the_instructions_is_refused() {
    let copy = instructions().replace("after: { checked: success }", "after: [checked]");
    assert_ne!(copy, instructions(), "the instructions show the after: map");
    assert_checks_clean("a copy of the instructions", &copy);
}
