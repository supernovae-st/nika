// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! LOT 3 · the task-body rungs (R2 · R3 · R4 · R5) — the forms the
//! nine-key sweep of 2026-08-11 retired INSIDE a task, one rung each,
//! all line-based and structure-aware like [`super::identity()`] (the R1 rung):
//!
//! - **R3** `output:` → `extract:` (same shape · the truthful word)
//! - **R4** `on_error: { fail_workflow: true }` → the key is DELETED (the
//!   default IS the failure · an `on_error:` left empty is deleted with it)
//!   · `fail_workflow: false` is NOT mechanical (it meant « do not fail the
//!   workflow » · `recover` or `skip`? · only the author knows) → STOP
//! - **R2** task-level `max_parallel:` / `fail_fast:` → INSIDE the
//!   `for_each:` block · a scalar `for_each: <expr>` becomes the block
//!   `for_each:` + `items: <expr>` first · a task carrying the knobs with no
//!   `for_each:` at all → STOP (they have no meaning without it)
//! - **R5** `declassify:` (a `{from, to: trusted, because}` list) and
//!   `inert: <reason>` → ONE `lift:` list · `{law: taint, from, because}`
//!   per declassify entry · `{law: data-as-code, because}` for the inert
//!   reason · a declassify entry whose `to:` is not `trusted`, or one
//!   missing `from`/`because`, → STOP
//!
//! Only lines at the TASK-BODY indent are touched (a task id sits at
//! indent N under a top-level `tasks:`; its body keys at N+2), so an
//! `output:` inside `args:` or a `with:` island is never renamed. Every
//! other line is byte-identical; a document with none of the forms is
//! [`Lot3Outcome::Clean`] (idempotent by contract).

/// The outcome of one LOT 3 pass over one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lot3Outcome {
    /// Mechanically migrated · `applied` names the rungs that fired.
    Changed {
        source: String,
        applied: Vec<&'static str>,
    },
    /// Nothing to migrate.
    Clean,
    /// Ambiguous or non-mechanical — each diagnostic names the case.
    Stop(Vec<String>),
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The key of a `key:` / `key: value` line, when it is one.
fn key_of(line: &str) -> Option<&str> {
    let t = line.trim_start();
    if t.starts_with('#') || t.starts_with('-') {
        return None;
    }
    let colon = t.find(':')?;
    let key = &t[..colon];
    let rest = &t[colon + 1..];
    if key.is_empty() || key.contains(' ') || key.contains('"') || key.contains('\'') {
        return None;
    }
    if !(rest.is_empty() || rest.starts_with(' ') || rest.starts_with('{') || rest.starts_with('['))
    {
        return None;
    }
    Some(key)
}

/// The value text after `key:` (trimmed · trailing comment kept out).
fn value_of(line: &str) -> String {
    let t = line.trim_start();
    let colon = t.find(':').unwrap_or(0);
    let rest = t[colon + 1..].trim();
    // a `# comment` after a scalar is not the value
    let mut depth_q = false;
    let mut out = String::new();
    for c in rest.chars() {
        if c == '"' || c == '\'' {
            depth_q = !depth_q;
        }
        if c == '#' && !depth_q && (out.is_empty() || out.ends_with(' ')) {
            break;
        }
        out.push(c);
    }
    out.trim().to_owned()
}

/// The [start, end) line span of one task's body under a top-level
/// `tasks:` map · `(task_start, body_indent, body_end)`.
struct TaskSpan {
    header: usize,
    body_indent: usize,
    end: usize,
}

/// Locate every task body in the document.
fn task_spans(lines: &[&str]) -> Vec<TaskSpan> {
    let mut spans = Vec::new();
    // the top-level `tasks:` line
    let Some(tasks_idx) = lines
        .iter()
        .position(|l| indent_of(l) == 0 && key_of(l) == Some("tasks"))
    else {
        return spans;
    };
    // the tasks block ends at the next top-level key
    let mut block_end = tasks_idx + 1;
    while block_end < lines.len() {
        let l = lines[block_end];
        if !l.trim().is_empty() && indent_of(l) == 0 {
            break;
        }
        block_end += 1;
    }
    // task ids · the first indent level inside the block
    let id_indent = lines[tasks_idx + 1..block_end]
        .iter()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|l| indent_of(l))
        .min()
        .unwrap_or(2);
    let mut i = tasks_idx + 1;
    while i < block_end {
        let l = lines[i];
        if !l.trim().is_empty() && indent_of(l) == id_indent && key_of(l).is_some() {
            // body = lines until the next id-indent key
            let mut j = i + 1;
            while j < block_end {
                let m = lines[j];
                if !m.trim().is_empty() && indent_of(m) <= id_indent {
                    break;
                }
                j += 1;
            }
            let body_indent = lines[i + 1..j]
                .iter()
                .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
                .map(|l| indent_of(l))
                .min()
                .unwrap_or(id_indent + 2);
            spans.push(TaskSpan {
                header: i,
                body_indent,
                end: j,
            });
            i = j;
        } else {
            i += 1;
        }
    }
    spans
}

/// The [start, end) of a nested block whose header sits at `header`
/// (deeper-indented lines · blank lines inside allowed).
fn nested_end(lines: &[&str], header: usize, limit: usize) -> usize {
    let base = indent_of(lines[header]);
    let mut j = header + 1;
    while j < limit {
        let l = lines[j];
        if l.trim().is_empty() || indent_of(l) > base {
            j += 1;
        } else {
            break;
        }
    }
    while j > header + 1 && lines[j - 1].trim().is_empty() {
        j -= 1;
    }
    j
}

/// One parsed `declassify:` entry.
struct Declassify {
    from: String,
    because: String,
}

/// Parse a `declassify:` block (list of `{from, to, because}` mappings ·
/// block or flow items) · `Err(note)` on anything non-mechanical.
fn parse_declassify(lines: &[&str], header: usize, end: usize) -> Result<Vec<Declassify>, String> {
    let mut out = Vec::new();
    let mut cur: Option<(Option<String>, Option<String>, Option<String>)> = None; // from · to · because
    let flush = |cur: &mut Option<(Option<String>, Option<String>, Option<String>)>,
                 out: &mut Vec<Declassify>|
     -> Result<(), String> {
        if let Some((from, to, because)) = cur.take() {
            let from =
                from.ok_or("a `declassify` entry without `from:` — migrate to `lift:` by hand")?;
            let because = because
                .ok_or("a `declassify` entry without `because:` — migrate to `lift:` by hand")?;
            if to.as_deref() != Some("trusted") {
                return Err(format!(
                    "`declassify` entry `{from}` lifts to `{}` — only `to: trusted` maps to `lift: {{law: taint}}` · migrate by hand",
                    to.unwrap_or_default()
                ));
            }
            out.push(Declassify { from, because });
        }
        Ok(())
    };
    // flow form on the header line · `declassify: [{…}]`
    let header_value = value_of(lines[header]);
    if header_value.starts_with('[') {
        return Err("a flow-style `declassify: [...]` — migrate to `lift:` by hand".to_owned());
    }
    for raw in &lines[header + 1..end] {
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let item = t.strip_prefix('-').map(str::trim);
        let (is_new, body) = match item {
            Some(b) => (true, b),
            None => (false, t),
        };
        if is_new {
            flush(&mut cur, &mut out)?;
            cur = Some((None, None, None));
        }
        let Some(slot) = cur.as_mut() else {
            return Err(
                "`declassify:` carries a line outside any `- ` entry — migrate by hand".to_owned(),
            );
        };
        // flow mapping item `- { from: x, to: trusted, because: y }`
        if body.starts_with('{') {
            let inner = body.trim_start_matches('{').trim_end_matches('}');
            for part in inner.split(',') {
                let Some((k, v)) = part.split_once(':') else {
                    continue;
                };
                assign(slot, k.trim(), unquote(v.trim()))?;
            }
            continue;
        }
        let Some((k, v)) = body.split_once(':') else {
            return Err(format!(
                "`declassify` line `{body}` is not `key: value` — migrate by hand"
            ));
        };
        assign(slot, k.trim(), unquote(v.trim()))?;
    }
    flush(&mut cur, &mut out)?;
    Ok(out)
}

fn assign(
    slot: &mut (Option<String>, Option<String>, Option<String>),
    key: &str,
    value: &str,
) -> Result<(), String> {
    match key {
        "from" => slot.0 = Some(value.to_owned()),
        "to" => slot.1 = Some(value.to_owned()),
        "because" => slot.2 = Some(value.to_owned()),
        other => {
            return Err(format!(
                "`declassify` entry carries `{other}:` — `lift:` knows `law`, `from`, `because` · migrate by hand"
            ));
        }
    }
    Ok(())
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Quote a `because:` for YAML (double quotes · inner quotes escaped).
fn yaml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The per-line edit plan one pass accumulates · applied at the end.
#[derive(Default)]
struct Edits {
    replace: Vec<(usize, String)>,
    delete: Vec<usize>,
    insert_after: Vec<(usize, Vec<String>)>,
    applied: Vec<&'static str>,
    notes: Vec<String>,
}

impl Edits {
    fn fired(&mut self, rung: &'static str) {
        if !self.applied.contains(&rung) {
            self.applied.push(rung);
        }
    }
    fn is_empty(&self) -> bool {
        self.replace.is_empty() && self.delete.is_empty() && self.insert_after.is_empty()
    }
    /// Rebuild the document · byte-identical outside the touched lines. An
    /// insertion attached to a deleted line is still honoured.
    fn render(&self, lines: &[&str]) -> String {
        let mut out: Vec<String> = Vec::with_capacity(lines.len() + 8);
        for (idx, line) in lines.iter().enumerate() {
            if !self.delete.contains(&idx) {
                match self.replace.iter().find(|(r, _)| *r == idx) {
                    Some((_, text)) => out.push(text.clone()),
                    None => out.push((*line).to_owned()),
                }
            }
            for (after, block) in &self.insert_after {
                if *after == idx {
                    out.extend(block.iter().cloned());
                }
            }
        }
        out.join("\n")
    }
}

/// What one task body carries for the rungs that need the whole body
/// (R2 needs `for_each` AND the knobs · R5 gathers every door).
#[derive(Default)]
struct TaskFacts {
    for_each_header: Option<usize>,
    for_each_scalar: Option<String>,
    for_each_flow: bool,
    knobs: Vec<(usize, String, String)>,
    lift_entries: Vec<String>,
    lift_anchor: Option<usize>,
}

/// R4 · the flow form `on_error: { fail_workflow: true[, on_codes: …] }`.
fn r4_flow(idx: usize, line: &str, pad: &str, task: &str, edits: &mut Edits) {
    let v = value_of(line);
    if !v.contains("fail_workflow") {
        return;
    }
    let inner = v.trim_start_matches('{').trim_end_matches('}');
    let mut kept: Vec<&str> = Vec::new();
    for part in inner.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if let Some((k, val)) = part.split_once(':')
            && k.trim() == "fail_workflow"
        {
            if val.trim() != "true" {
                edits.notes.push(format!(
                    "task `{task}`: `on_error: {{ fail_workflow: false }}` meant « do not fail the workflow » — `recover` or `skip`? only the author knows · migrate by hand"
                ));
                return;
            }
            continue;
        }
        kept.push(part);
    }
    if kept.is_empty() {
        edits.delete.push(idx);
    } else {
        edits
            .replace
            .push((idx, format!("{pad}on_error: {{ {} }}", kept.join(", "))));
    }
    edits.fired("r4-fail-workflow");
}

/// R4 · the block form · the `fail_workflow:` line is deleted (and the
/// `on_error:` header with it when nothing else remains).
fn r4_block(lines: &[&str], idx: usize, end: usize, task: &str, edits: &mut Edits) {
    let mut fw_line: Option<usize> = None;
    let mut others = 0usize;
    for (k, m) in lines[idx + 1..end].iter().enumerate() {
        if m.trim().is_empty() || m.trim_start().starts_with('#') {
            continue;
        }
        match key_of(m) {
            Some("fail_workflow") => {
                if value_of(m) != "true" {
                    edits.notes.push(format!(
                        "task `{task}`: `on_error.fail_workflow: false` meant « do not fail the workflow » — `recover` or `skip`? only the author knows · migrate by hand"
                    ));
                    return;
                }
                fw_line = Some(idx + 1 + k);
            }
            Some(_) => others += 1,
            None => {}
        }
    }
    if let Some(fw) = fw_line {
        edits.delete.push(fw);
        if others == 0 {
            edits.delete.push(idx);
        }
        edits.fired("r4-fail-workflow");
    }
}

/// One task body · the line-local rungs fire as they are met, the
/// body-wide facts (R2 · R5) are gathered for the settle step.
fn migrate_task(lines: &[&str], span: &TaskSpan, edits: &mut Edits) {
    let task = key_of(lines[span.header]).unwrap_or("?").to_owned();
    let pad = " ".repeat(span.body_indent);
    let inner = " ".repeat(span.body_indent + 2);
    let mut facts = TaskFacts::default();
    for i in span.header + 1..span.end {
        let l = lines[i];
        if l.trim().is_empty() || indent_of(l) != span.body_indent {
            continue;
        }
        match key_of(l) {
            Some("output") => {
                edits
                    .replace
                    .push((i, l.replacen("output:", "extract:", 1)));
                edits.fired("r3-extract");
            }
            Some("on_error") => {
                if value_of(l).starts_with('{') {
                    r4_flow(i, l, &pad, &task, edits);
                } else {
                    r4_block(lines, i, nested_end(lines, i, span.end), &task, edits);
                }
            }
            Some("for_each") => {
                facts.for_each_header = Some(i);
                let v = value_of(l);
                if v.starts_with('{') {
                    facts.for_each_flow = true;
                } else if !v.is_empty() {
                    facts.for_each_scalar = Some(v);
                }
            }
            Some(k @ ("max_parallel" | "fail_fast")) => {
                facts.knobs.push((i, k.to_owned(), value_of(l)));
            }
            Some("declassify") => {
                let end = nested_end(lines, i, span.end);
                match parse_declassify(lines, i, end) {
                    Ok(entries) => {
                        for e in entries {
                            facts.lift_entries.push(format!(
                                "{inner}- {{ law: taint, from: {}, because: {} }}",
                                e.from,
                                yaml_str(&e.because)
                            ));
                        }
                        edits.delete.extend(i..end);
                        facts.lift_anchor.get_or_insert(i);
                    }
                    Err(note) => edits.notes.push(format!("task `{task}`: {note}")),
                }
            }
            Some("inert") => {
                let v = value_of(l);
                if v.is_empty() || v.starts_with('|') || v.starts_with('>') {
                    edits.notes.push(format!(
                        "task `{task}`: `inert:` with a block-scalar reason — migrate to `lift: [{{law: data-as-code, because}}]` by hand"
                    ));
                } else {
                    facts.lift_entries.push(format!(
                        "{inner}- {{ law: data-as-code, because: {} }}",
                        yaml_str(unquote(&v))
                    ));
                    edits.delete.push(i);
                    facts.lift_anchor.get_or_insert(i);
                }
            }
            _ => {}
        }
    }
    settle_task(span, &task, &pad, &inner, facts, edits);
}

/// The body-wide rungs · R5 lands the `lift:` list where the first door
/// stood · R2 moves the knobs into `for_each:` (or STOPS).
fn settle_task(
    span: &TaskSpan,
    task: &str,
    pad: &str,
    inner: &str,
    mut facts: TaskFacts,
    edits: &mut Edits,
) {
    if !facts.lift_entries.is_empty() {
        let anchor = facts.lift_anchor.unwrap_or(span.header);
        let mut block = vec![format!("{pad}lift:")];
        block.append(&mut facts.lift_entries);
        edits.insert_after.push((anchor.saturating_sub(1), block));
        edits.fired("r5-lift");
    }
    if facts.knobs.is_empty() {
        return;
    }
    let names = facts
        .knobs
        .iter()
        .map(|k| k.1.as_str())
        .collect::<Vec<_>>()
        .join("` / `");
    match facts.for_each_header {
        None => edits.notes.push(format!(
            "task `{task}`: `{names}` has no meaning without `for_each:` — the task carries no fan-out · remove or restructure by hand"
        )),
        Some(_) if facts.for_each_flow => edits.notes.push(format!(
            "task `{task}`: a flow-style `for_each: {{...}}` — `{names}` moves inside it by hand"
        )),
        Some(fe) => {
            let mut block: Vec<String> = Vec::new();
            if let Some(items) = facts.for_each_scalar.take() {
                edits.replace.push((fe, format!("{pad}for_each:")));
                block.push(format!("{inner}items: {items}"));
            }
            for (idx, key, val) in &facts.knobs {
                block.push(format!("{inner}{key}: {val}"));
                edits.delete.push(*idx);
            }
            edits.insert_after.push((fe, block));
            edits.fired("r2-for-each");
        }
    }
}

/// The LOT 3 task-body migration. See the module doc for the contract.
#[must_use]
pub fn lot3(source: &str) -> Lot3Outcome {
    let lines: Vec<&str> = source.split('\n').collect();
    let spans = task_spans(&lines);
    if spans.is_empty() {
        return Lot3Outcome::Clean;
    }
    let mut edits = Edits::default();
    for span in &spans {
        migrate_task(&lines, span, &mut edits);
    }
    if !edits.notes.is_empty() {
        return Lot3Outcome::Stop(edits.notes);
    }
    if edits.is_empty() {
        return Lot3Outcome::Clean;
    }
    Lot3Outcome::Changed {
        source: edits.render(&lines),
        applied: edits.applied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(src: &str) -> (String, Vec<&'static str>) {
        match lot3(src) {
            Lot3Outcome::Changed { source, applied } => (source, applied),
            other => panic!("expected Changed · got {other:?}"),
        }
    }

    fn stop(src: &str) -> Vec<String> {
        match lot3(src) {
            Lot3Outcome::Stop(n) => n,
            other => panic!("expected Stop · got {other:?}"),
        }
    }

    #[test]
    fn r3_output_becomes_extract_at_the_task_body_only() {
        let src = "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n    output:\n      x: \".x\"\n  b:\n    invoke:\n      tool: nika:jq\n      args:\n        output: keep-me\n";
        let (out, applied) = changed(src);
        assert!(out.contains("    extract:\n      x: \".x\"\n"), "{out}");
        assert!(
            out.contains("        output: keep-me\n"),
            "an args key is not the task key · {out}"
        );
        assert_eq!(applied, vec!["r3-extract"]);
        assert_eq!(lot3(&out), Lot3Outcome::Clean, "idempotent");
    }

    #[test]
    fn r4_fail_workflow_true_is_deleted_flow_and_block() {
        let flow = "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n    on_error: { fail_workflow: true }\n";
        let (out, applied) = changed(flow);
        assert!(!out.contains("on_error"), "{out}");
        assert_eq!(applied, vec!["r4-fail-workflow"]);
        let block = "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n    on_error:\n      fail_workflow: true\n      on_codes: [NIKA-EXEC-001]\n";
        let (out, _) = changed(block);
        assert!(
            out.contains("    on_error:\n      on_codes: [NIKA-EXEC-001]\n"),
            "the other key survives · {out}"
        );
        assert!(!out.contains("fail_workflow"), "{out}");
    }

    #[test]
    fn r4_fail_workflow_false_stops() {
        let notes = stop(
            "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n    on_error: { fail_workflow: false }\n",
        );
        assert!(notes[0].contains("only the author knows"), "{notes:?}");
    }

    #[test]
    fn r2_knobs_move_inside_the_for_each_block_and_a_scalar_becomes_items() {
        let src = "nika: t\ntasks:\n  fan:\n    for_each: ${{ inputs.items }}\n    max_parallel: 4\n    fail_fast: false\n    exec: { command: [\"true\"] }\n";
        let (out, applied) = changed(src);
        assert_eq!(
            out,
            "nika: t\ntasks:\n  fan:\n    for_each:\n      items: ${{ inputs.items }}\n      max_parallel: 4\n      fail_fast: false\n    exec: { command: [\"true\"] }\n"
        );
        assert_eq!(applied, vec!["r2-for-each"]);
        let block = "nika: t\ntasks:\n  fan:\n    for_each:\n      items: ${{ inputs.items }}\n    max_parallel: 2\n    exec: { command: [\"true\"] }\n";
        let (out, _) = changed(block);
        assert!(
            out.contains(
                "    for_each:\n      max_parallel: 2\n      items: ${{ inputs.items }}\n"
            ),
            "{out}"
        );
    }

    #[test]
    fn r2_knobs_without_for_each_stop() {
        let notes =
            stop("nika: t\ntasks:\n  a:\n    max_parallel: 4\n    exec: { command: [\"true\"] }\n");
        assert!(
            notes[0].contains("no meaning without `for_each:`"),
            "{notes:?}"
        );
    }

    #[test]
    fn r5_declassify_and_inert_become_one_lift_list() {
        let src = "nika: t\ntasks:\n  load:\n    invoke:\n      tool: nika:read\n      args: { path: \"${{ inputs.p }}\" }\n    declassify:\n      - from: inputs.p\n        to: trusted\n        because: \"the bound confines it\"\n    inert: \"the value is data\"\n";
        let (out, applied) = changed(src);
        assert!(
            out.contains("    lift:\n      - { law: taint, from: inputs.p, because: \"the bound confines it\" }\n      - { law: data-as-code, because: \"the value is data\" }\n"),
            "{out}"
        );
        assert!(
            !out.contains("declassify") && !out.contains("inert:"),
            "{out}"
        );
        assert_eq!(applied, vec!["r5-lift"]);
        assert_eq!(lot3(&out), Lot3Outcome::Clean, "idempotent");
    }

    #[test]
    fn r5_declassify_to_something_else_stops() {
        let notes = stop(
            "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n    declassify:\n      - { from: inputs.p, to: public, because: x }\n",
        );
        assert!(notes[0].contains("only `to: trusted`"), "{notes:?}");
    }

    #[test]
    fn a_nine_key_task_body_is_clean() {
        let src = "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n    extract:\n      x: \".x\"\n    lift:\n      - { law: data-as-code, because: \"x\" }\n";
        assert_eq!(lot3(src), Lot3Outcome::Clean);
        assert_eq!(lot3("nika: t\n"), Lot3Outcome::Clean, "no tasks at all");
    }

    #[test]
    fn several_rungs_fire_in_one_pass_and_report_each() {
        let src = "nika: t\ntasks:\n  a:\n    for_each: ${{ inputs.items }}\n    max_parallel: 3\n    exec: { command: [\"true\"] }\n    output:\n      x: \".x\"\n    on_error: { fail_workflow: true }\n";
        let (out, applied) = changed(src);
        assert!(
            out.contains("      max_parallel: 3\n")
                && out.contains("    extract:\n")
                && !out.contains("on_error"),
            "{out}"
        );
        assert_eq!(
            applied,
            vec!["r3-extract", "r4-fail-workflow", "r2-for-each"]
        );
    }
}
