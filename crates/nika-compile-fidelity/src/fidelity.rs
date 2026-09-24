// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The fidelity laws: deterministic checks of a candidate `.nika` document against the
//! ORIGINAL request and the reader's plan — every source the request states is read, every
//! destination it states is written, an effect a stated approval holds back runs only on a
//! confirm `nika:prompt`'s yes, a prohibited effect is absent, no path or host the
//! request never wrote (the human's answers allowed). Every refusal is one structured
//! diagnostic a seat can repair from. Pure over (request · plan · projected document); the
//! compile crate's native door and its cold verification both read them here.
//! Moved from nika-compile to the reader at the 15k prod-LOC wall (2026-09-22), unchanged.

use crate::plan::{Effect, EffectPolicy, EffectVerb, Plan};
use nika_compile_reader::objects;
use nika_compile_reader::paths::{self, PathShape};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

mod final_gate;
mod records;
pub use final_gate::unbound_final_gate;
pub use records::raw_text_as_records;

/// One structured diagnostic the judge returns and the seat repairs from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// `parse` · `check` · `source` · `destination` · `gate` · `prohibition` · `literal` ·
    /// `question` · `records`.
    pub kind: &'static str,
    pub message: String,
}

/// Every string literal of the candidate (const values, args, prompts), for the literal laws.
pub fn strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
        Value::Object(map) => map.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

/// The task ids that reach the world (a POST/PUT/PATCH/DELETE fetch, a notify, a write, an
/// emit, an edit) and the task ids that ask a human (`nika:prompt`).
#[must_use]
pub fn effect_and_gate_tasks(doc: &Value) -> (Vec<String>, Vec<String>) {
    let mut effects = Vec::new();
    let mut gates = Vec::new();
    for (id, task) in doc
        .get("tasks")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let Some(invoke) = task.get("invoke") else {
            continue;
        };
        let tool = invoke
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let method = invoke
            .get("args")
            .and_then(|a| a.get("method"))
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .to_ascii_uppercase();
        match tool {
            "nika:prompt" => gates.push(id.clone()),
            "nika:write" | "nika:notify" | "nika:emit" | "nika:edit" => effects.push(id.clone()),
            "nika:fetch" if method != "GET" => effects.push(id.clone()),
            _ => {}
        }
    }
    (effects, gates)
}

/// Whether `task` depends, transitively through `with:` bindings, `when:` guards and `after:`
/// edges, on any of `roots`.
#[must_use]
pub fn depends_on(doc: &Value, task: &str, roots: &[String]) -> bool {
    let tasks = doc.get("tasks").and_then(Value::as_object);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut stack = vec![task.to_owned()];
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(node) = tasks.and_then(|t| t.get(&id)) else {
            continue;
        };
        let mut refs: Vec<String> = Vec::new();
        let mut texts = Vec::new();
        strings(node.get("with").unwrap_or(&Value::Null), &mut texts);
        strings(node.get("when").unwrap_or(&Value::Null), &mut texts);
        for text in texts {
            let mut rest = text.as_str();
            while let Some(at) = rest.find("tasks.") {
                let name: String = rest[at + 6..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    refs.push(name);
                }
                rest = &rest[at + 6..];
            }
        }
        for key in node
            .get("after")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|m| m.keys())
        {
            refs.push(key.clone());
        }
        for r in refs {
            if roots.contains(&r) {
                return true;
            }
            stack.push(r);
        }
    }
    false
}

/// The fidelity laws over a projected candidate: the stated paths realized, the stated
/// approval respected, the prohibited effect absent, no invented path or host (a value the
/// human answered is `allowed`). `clarified` holds the whole names the human typed for the
/// read's source question ([`clarified_sources`]), the only answer through which a stated
/// source reads whole.
pub fn laws(
    intent: &str,
    plan: &Plan,
    doc: &Value,
    allowed: &[String],
    waived: &[String],
    clarified: &[String],
    out: &mut Vec<Diagnostic>,
) {
    let mut literals = Vec::new();
    strings(doc, &mut literals);
    stated_paths(intent, doc, waived, clarified, out);
    approvals(plan, doc, out);
    invented_gates(plan, doc, out);
    dropped_effects(plan, doc, out);
    unnamed_writes(plan, doc, out);
    prohibitions(plan, doc, out);
    raw_text_as_records(doc, out);
    invented(intent, &literals, allowed, out);
}

/// The values a human answered, as the texts a candidate may carry without inventing them.
#[must_use]
pub fn allowed_values(answers: &BTreeMap<String, String>) -> Vec<String> {
    answers
        .values()
        .map(|raw| {
            serde_json::from_str::<Value>(raw)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| raw.clone())
        })
        .collect()
}

/// The key of the read's source question: the assembler asks it when the request leaves the
/// file a read opens undecided (a directory, a placeholder, a name whose extent it leaves
/// open), and its typed answer is a JSON array of exact file paths.
pub const SOURCE_PATHS: &str = "const.source_paths";

/// The whole file names the human typed for the read's source question, each item read by
/// the path law the assembler binds it with: that one answer only, never another answered
/// value, a revision's literal or a candidate's.
#[must_use]
pub fn clarified_sources(answers: &BTreeMap<String, String>) -> Vec<String> {
    let typed = answers
        .get(SOURCE_PATHS)
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
    typed
        .as_ref()
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| match item.as_str().and_then(paths::token) {
            Some(PathShape::File(file)) => Some(file),
            _ => None,
        })
        .collect()
}

/// Whether one `permits.fs` entry covers a path: the path itself, a directory glob
/// (`./data/**`, `./reports/*`), or a pattern whose fixed head and tail the path carries.
/// A leading `./` names the same relative path; parent components are never collapsed.
#[must_use]
pub fn covers(entry: &str, path: &str) -> bool {
    let path = path.trim_start_matches("./").trim_end_matches('/');
    let entry = entry.trim_start_matches("./").trim_end_matches('/');
    if entry == path {
        return true;
    }
    if let Some(dir) = entry
        .strip_suffix("/**")
        .or_else(|| entry.strip_suffix("/*"))
    {
        return path == dir || path.starts_with(&format!("{dir}/"));
    }
    match (entry.find('*'), entry.rfind('*')) {
        (Some(first), Some(last)) => {
            let head = &entry[..first];
            let tail = &entry[last + 1..];
            path.starts_with(head) && path.ends_with(tail) && path.len() >= head.len() + tail.len()
        }
        _ => false,
    }
}

fn granted(doc: &Value, axis: &str, path: &str) -> bool {
    doc.pointer(&format!("/permits/fs/{axis}"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|entry| covers(entry, path))
}

/// Law 1: every path the request names is under the boundary (`permits.fs.read` or
/// `permits.fs.write`): the boundary is what a candidate can open, and a path the request
/// names that the boundary does not cover is a dropped clause, whatever a prompt says about
/// it. (The reader's source/destination split is a hint for the seat, never the law: a
/// destination it reads as a source must still be opened one way or the other.)
///
/// A path the boundary does not cover is judged at each place the request writes it, in the
/// role the reader gives that place (a destination connector before it makes it a
/// destination, `objects::destination_at`), and each occurrence must be realized:
/// - inside a longer stated path (`./archive/équipe.txt` holding `équipe.txt`), it is that
///   path's, judged as that path;
/// - as a source, through the whole name the human typed for the source question when the
///   candidate reads it (the reader cuts `Notes équipe.txt` opening a sentence to
///   `équipe.txt`);
/// - as a destination, only by write permission covering the literal the reader read there
///   (`dans Copie équipe.txt`), never by a source answer.
///
/// An occurrence outside all of them is a file of its own (a separately named `équipe.txt`).
pub fn stated_paths(
    intent: &str,
    doc: &Value,
    waived: &[String],
    clarified: &[String],
    out: &mut Vec<Diagnostic>,
) {
    let mut stated = crate::hot::stated_sources(intent);
    stated.extend(crate::hot::stated_destinations(intent));
    // The reader deliberately leaves an unquoted multiword compound name unresolved.
    // Its extent still cannot disappear from native fidelity: shortening it to the last
    // word names a different file. Dynamic placeholders remain for the typed answer door.
    stated.extend(
        paths::literals(intent)
            .into_iter()
            .filter_map(|shape| match shape {
                PathShape::Placeholder(path)
                    if path.contains('/') && !path.contains(['<', '>', '{', '}', '$']) =>
                {
                    Some(path)
                }
                _ => None,
            }),
    );
    stated.dedup();
    let literals: Vec<String> = paths::literals(intent)
        .into_iter()
        .filter_map(|shape| match shape {
            PathShape::File(text)
            | PathShape::Directory(text)
            | PathShape::Glob(text)
            | PathShape::Placeholder(text) => Some(text),
            // An unknown future shape cannot witness a covered destination.
            _ => None,
        })
        .collect();
    let typed = spans(
        intent,
        clarified.iter().filter(|name| granted(doc, "read", name)),
    );
    let written = spans(
        intent,
        literals.iter().filter(|name| granted(doc, "write", name)),
    );
    let lower = intent.to_lowercase();
    for path in &stated {
        if waived.contains(path) || granted(doc, "read", path) || granted(doc, "write", path) {
            continue;
        }
        let owners = spans(
            intent,
            stated.iter().filter(|other| other.len() > path.len()),
        );
        let realized = |at: usize| {
            let held = |places: &[(usize, usize)]| {
                places
                    .iter()
                    .any(|&(from, to)| from <= at && at + path.len() <= to)
            };
            // Lowering can change byte lengths (`İ`): the offset in `lower` is the lowered
            // prefix's length, never `at`.
            let lowered = intent.get(..at).map_or(0, |head| head.to_lowercase().len());
            let destination = objects::destination_at(&lower, lowered).is_some();
            held(&owners) || held(if destination { &written } else { &typed })
        };
        let found = occurrences(intent, path);
        if found.is_empty() || found.iter().any(|&at| !realized(at)) {
            let boundary = doc
                .pointer("/permits/fs")
                .map_or_else(|| "none".to_owned(), Value::to_string);
            out.push(Diagnostic { kind: "path", message: format!("UNREALIZED PATH: the request names `{path}`; no `permits.fs.read` or `permits.fs.write` entry covers it (permits.fs = {boundary}), so no task opens it. Read it (nika:read / nika:glob + fs.read) or write it (nika:write + fs.write) as the request means, or name it in `gaps` if it cannot be reached.") });
        }
    }
}

/// Every place the request writes one of `names`, as a byte span.
fn spans<'a>(intent: &str, names: impl Iterator<Item = &'a String>) -> Vec<(usize, usize)> {
    names
        .flat_map(|name| {
            occurrences(intent, name)
                .into_iter()
                .map(move |at| (at, at + name.len()))
        })
        .collect()
}

/// Where the request writes `literal` verbatim with no letter or digit continuing it on
/// either side: the byte offset of each occurrence.
fn occurrences(text: &str, literal: &str) -> Vec<usize> {
    text.match_indices(literal)
        .map(|(at, _)| at)
        .filter(|&at| {
            let before = text.get(..at).and_then(|head| head.chars().next_back());
            let after = text
                .get(at + literal.len()..)
                .and_then(|tail| tail.chars().next());
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
        .collect()
}

/// Law 3: a stated approval gates the effect family it names (a send · a notify · a publish;
/// a write when the write is the approved effect): each such effect task runs only on a
/// human's typed yes (the guard of `final_gate`), as does a task whose effect the law cannot
/// read. A final approval the reader bound to no effect holds the final effects (Law 3b).
pub fn approvals(plan: &Plan, doc: &Value, out: &mut Vec<Diagnostic>) {
    let approved: Vec<EffectVerb> = plan
        .effects
        .iter()
        .filter(|e| e.policy == EffectPolicy::HumanFirst)
        .map(|e| e.verb)
        .collect();
    if approved.is_empty() {
        if unbound_final_gate(plan) {
            final_gate::unbound_approval(doc, out);
        }
        return;
    }
    let (effects, gates) = effect_and_gate_tasks(doc);
    if gates.is_empty() {
        out.push(Diagnostic { kind: "gate", message: "MISSING APPROVAL: the request asks a human before the effect; no `nika:prompt` task exists. Add a review task and gate the effect on its output (`with: { approved: ${{ tasks.review.output }} }`, `when: \"${{ with.approved == true }}\"`).".to_owned() });
        return;
    }
    let writes_gated = approved.iter().any(|v| matches!(v, EffectVerb::Write));
    let sends_gated = approved.iter().any(|v| !matches!(v, EffectVerb::Write));
    for task in &effects {
        let tool = tool_of(doc, task);
        let concerned = if tool == "nika:write" || tool == "nika:edit" {
            writes_gated
        } else {
            sends_gated
        };
        if concerned && !final_gate::affirmed(doc, task) {
            out.push(final_gate::order(task));
        }
    }
    final_gate::unproven(doc, out);
}

/// Law 21: an approval the request never states is not a gate. A `nika:prompt` that guards an
/// effect when no effect of the plan is human-first makes an unattended run answer the gate
/// with its default and skip the effect — a run that exits 0 and does nothing (measured
/// 2026-09-23: the weekly recap's send never reached the webhook). The effect runs as stated,
/// or the seat asks whether it should. An unbound final approval (Law 3b) licenses the gate
/// over the final effects and what runs after them, never over an earlier effect.
pub fn invented_gates(plan: &Plan, doc: &Value, out: &mut Vec<Diagnostic>) {
    if plan
        .effects
        .iter()
        .any(|e| e.policy == EffectPolicy::HumanFirst)
    {
        return;
    }
    let (effects, gates) = effect_and_gate_tasks(doc);
    if gates.is_empty() {
        return;
    }
    let finals = if unbound_final_gate(plan) {
        final_gate::final_effects(doc, &effects)
    } else {
        Vec::new()
    };
    for task in &effects {
        if finals.contains(task) || depends_on(doc, task, &finals) {
            continue;
        }
        if depends_on(doc, task, &gates) {
            out.push(Diagnostic {
                kind: "gate",
                message: format!(
                    "INVENTED GATE: the request states no approval before the effect task `{task}`, yet a `nika:prompt` guards it; unattended, the gate answers its default and the effect is skipped — a run that exits 0 and does nothing. Remove the gate (the effect runs as stated), or ask under `questions` whether a human should approve it."
                ),
            });
        }
    }
}

/// Law 22: an effect the request states is a task. A write is judged by its stated path; every
/// other effect (a send, a creation, an update, a payment…) the plan carries and the request
/// does not forbid must be carried by an effect task — a `nika:fetch` that is not a GET, a
/// `nika:notify`, a `nika:emit`, or an `mcp:` tool. A candidate that reads, drafts and asks but
/// never sends (measured 2026-09-23, « ask me before anything is sent ») is refused by name:
/// nothing silently disappears.
pub fn dropped_effects(plan: &Plan, doc: &Value, out: &mut Vec<Diagnostic>) {
    let stated: Vec<&crate::plan::Effect> = plan
        .effects
        .iter()
        .filter(|e| {
            e.verb != EffectVerb::Write
                && !matches!(e.policy, EffectPolicy::Forbidden | EffectPolicy::Conflict)
        })
        .collect();
    if stated.is_empty() {
        return;
    }
    let (effects, _) = effect_and_gate_tasks(doc);
    let carried = !effects.is_empty()
        || doc
            .get("tasks")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .any(|(id, _)| tool_of(doc, id).starts_with("mcp:"));
    if carried {
        return;
    }
    for effect in stated {
        out.push(Diagnostic {
            kind: "effect",
            message: format!(
                "DROPPED EFFECT: the request states `{}` (« {} ») and no task carries it — no `nika:fetch` beyond GET, no `nika:notify`, no `nika:emit`, no `mcp:` tool. Realize it (a destination the request leaves open is ONE `const.<name>_endpoint` question), never drop it.",
                effect.verb.word(),
                effect.evidence.trim()
            ),
        });
    }
}

/// Law 22b: a write into a file the request leaves unnamed (« dans un fichier », « into a new
/// file »: a planned write whose target names no single file) is carried by a `nika:write`
/// task. No stated path witnesses it (Law 1 sees none) and Law 22 leaves writes to their paths,
/// so without this law a candidate that drafts and writes nothing passed. The path is ONE asked
/// placeholder the answer completes, never an invented path and never a dropped write.
fn unnamed_writes(plan: &Plan, doc: &Value, out: &mut Vec<Diagnostic>) {
    let (results, copies): (Vec<&Effect>, Vec<&Effect>) = plan
        .effects
        .iter()
        .filter(|e| {
            e.verb == EffectVerb::Write
                && matches!(e.policy, EffectPolicy::Automatic | EffectPolicy::HumanFirst)
                && paths::single_file(&e.target).is_none()
        })
        .partition(|e| asks_produced(plan, e));
    let (witnesses, produced) = unnamed_witnesses(plan, doc);
    // A witness that writes a produced result covers a requested result first; the witnesses
    // left cover the writes the request asks for without a transformation (a copy).
    let spare = witnesses.saturating_sub(produced.min(results.len()));
    let unwritten = results
        .iter()
        .skip(produced)
        .map(|e| (e, RESULT_CONTENT))
        .chain(copies.iter().skip(spare).map(|e| (e, "")));
    for (effect, owed) in unwritten {
        out.push(Diagnostic {
            kind: "destination",
            message: format!(
                "UNWRITTEN DESTINATION: the request asks to write into a file it does not name (« {} ») and no `nika:write` task carries it. Write it with `nika:write` to `${{{{ const.output_path }}}}`: declare `output_path: \"\"` under `const:`, ask `const.output_path` in `questions`, and leave `permits.fs.write: [\"\"]` for the answer to complete. Never invent the path, never drop the write.{owed}",
                effect.evidence.trim()
            ),
        });
    }
}

/// What the diagnostic of an unwritten requested result adds to the unnamed-destination one.
const RESULT_CONTENT: &str = " The request asks for a produced result there: the write's `content` must read it, `${{ tasks.<producer>.output }}` directly or through a `with:` binding the content uses; `after:` only orders and `when:` only guards, neither carries data.";

/// The `nika:write` tasks that can carry writes into unnamed files, one destination each, and
/// how many of them write a produced result. A witness writes no file the request names and no
/// file the candidate reads: a write of another destination or back over the source witnesses
/// nothing.
fn unnamed_witnesses(plan: &Plan, doc: &Value) -> (usize, usize) {
    let named: Vec<String> = plan
        .effects
        .iter()
        .filter(|e| e.verb == EffectVerb::Write)
        .filter_map(|e| paths::single_file(&e.target))
        .collect();
    let (effects, _) = effect_and_gate_tasks(doc);
    let witnesses: Vec<&String> = effects
        .iter()
        .filter(|task| {
            tool_of(doc, task) == "nika:write"
                && written_path(doc, task).is_none_or(|path| {
                    !named.iter().any(|name| covers(name, &path)) && !granted(doc, "read", &path)
                })
        })
        .collect();
    let produced = witnesses
        .iter()
        .filter(|task| carries_produced(doc, task))
        .count();
    (witnesses.len(), produced)
}

/// Whether the request asks an unnamed write for a produced result: its words lie in the clause
/// of a step that transforms material (a draft, an extraction, a classification, a
/// computation), where the reader's unnamed-destination floor finds them. A copy's write lies in
/// no such clause, so the source it carries as it is remains its content. The clause says that
/// a result is owed, not which of several producing tasks owes it.
fn asks_produced(plan: &Plan, effect: &Effect) -> bool {
    let evidence = effect.evidence.trim();
    !evidence.is_empty()
        && plan.steps.iter().any(|step| {
            let clause = step.evidence.trim();
            step.op.carries_constraints()
                && !clause.is_empty()
                && (clause.contains(evidence) || evidence.contains(clause))
        })
}

/// Whether a write task writes a produced result: its `content` argument reads, as data, the
/// output of a producing task (an `infer`, an `agent`, a `nika:jq` computation), directly or
/// through tool tasks (a conversion, a projection) followed by the same data edges. Each task is
/// followed once, so a cycle reads nothing.
fn carries_produced(doc: &Value, task: &str) -> bool {
    let Some(tasks) = doc.get("tasks").and_then(Value::as_object) else {
        return false;
    };
    let Some(write) = tasks.get(task) else {
        return false;
    };
    let mut stack = data_sources(write, write.pointer("/invoke/args/content"));
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some(id) = stack.pop() {
        let Some(node) = tasks.get(&id).filter(|_| seen.insert(id.clone())) else {
            continue;
        };
        if node.get("infer").is_some()
            || node.get("agent").is_some()
            || node.pointer("/invoke/tool").and_then(Value::as_str) == Some("nika:jq")
        {
            return true;
        }
        stack.extend(data_sources(node, node.pointer("/invoke/args")));
    }
    false
}

/// The tasks whose output a value of a task reads as data: a `${{ tasks.<id>.output… }}` the
/// value writes, or a `${{ with.<alias> }}` it uses whose binding on the same task reads one.
/// What a task waits for (`after:`) or is guarded by (`when:`) is not data, and a binding the
/// value never uses carries nothing.
fn data_sources(node: &Value, value: Option<&Value>) -> Vec<String> {
    let outputs = |text: &str| -> Vec<String> {
        reads(text, "tasks.")
            .filter(|(_, rest)| rest.starts_with(".output"))
            .map(|(id, _)| id.to_owned())
            .collect()
    };
    let mut texts = Vec::new();
    strings(value.unwrap_or(&Value::Null), &mut texts);
    let mut ids = Vec::new();
    for text in &texts {
        ids.extend(outputs(text));
        for (alias, _) in reads(text, "with.") {
            let mut bound = Vec::new();
            let binding = node.pointer(&format!("/with/{alias}"));
            strings(binding.unwrap_or(&Value::Null), &mut bound);
            for binding in &bound {
                ids.extend(outputs(binding));
            }
        }
    }
    ids
}

/// The `<scope><name>` reads inside the `${{ … }}` expressions of a text, each with what follows
/// the name. A scope glued to a longer name (`subtasks.`, `x.with.`) is not one.
fn reads<'a>(text: &'a str, scope: &'a str) -> impl Iterator<Item = (&'a str, &'a str)> {
    text.split("${{")
        .skip(1)
        .filter_map(|chunk| chunk.split_once("}}").map(|(expression, _)| expression))
        .flat_map(move |expression| {
            expression.match_indices(scope).filter_map(move |(at, _)| {
                let glued = expression[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.');
                let rest = &expression[at + scope.len()..];
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(rest.len());
                (!glued && end > 0).then_some((&rest[..end], &rest[end..]))
            })
        })
}

/// The literal path a write task writes: its `path` argument, or the `const:` value a bare
/// `${{ const.<name> }}` argument reads; `None` for any other expression.
fn written_path(doc: &Value, task: &str) -> Option<String> {
    let arg = doc
        .get("tasks")?
        .get(task)?
        .pointer("/invoke/args/path")?
        .as_str()?
        .trim();
    match arg
        .strip_prefix("${{")
        .and_then(|rest| rest.strip_suffix("}}"))
        .and_then(|inner| inner.trim().strip_prefix("const."))
    {
        Some(name) => doc
            .get("const")?
            .get(name.trim())?
            .as_str()
            .map(str::to_owned),
        None => Some(arg.to_owned()),
    }
}

/// Law 4: an effect the request forbids is absent.
pub fn prohibitions(plan: &Plan, doc: &Value, out: &mut Vec<Diagnostic>) {
    let (effects, _) = effect_and_gate_tasks(doc);
    for effect in plan
        .effects
        .iter()
        .filter(|e| e.policy == EffectPolicy::Forbidden)
    {
        let family = match effect.verb {
            EffectVerb::Write => effects.iter().any(|t| tool_of(doc, t) == "nika:write"),
            EffectVerb::Send | EffectVerb::Notify | EffectVerb::Publish | EffectVerb::Other => {
                effects
                    .iter()
                    .any(|t| matches!(tool_of(doc, t), "nika:fetch" | "nika:notify" | "nika:emit"))
            }
            _ => false,
        };
        if family {
            out.push(Diagnostic { kind: "prohibition", message: format!("PROHIBITED EFFECT: the request forbids `{}` ({}); the candidate performs it. Remove the effect.", effect.verb.word(), effect.target.trim()) });
        }
    }
}

/// Law 2: no invented path or host — every `./…`, `~/…`, `http(s)://…` literal of the
/// candidate is in the request (a glob's stem counts), was answered by the human, or rides a
/// `${{ }}` reference.
pub fn invented(intent: &str, literals: &[String], allowed: &[String], out: &mut Vec<Diagnostic>) {
    // The request and the human's answers are the words a path may be composed from.
    let mut lower = intent.to_lowercase();
    for value in allowed {
        lower.push('\n');
        lower.push_str(&value.to_lowercase());
    }
    for literal in literals {
        for token in literal.split_whitespace() {
            let token = token
                .trim_matches(|c: char| matches!(c, '"' | '\'' | ',' | ';' | ')' | '(' | '`'))
                .trim_end_matches(['?', '!', ':', '.', '¿', '¡']);
            let path_like =
                token.starts_with("./") || token.starts_with("../") || token.starts_with("~/");
            let host_like = token.starts_with("http://") || token.starts_with("https://");
            if !(path_like || host_like)
                || token.contains("${{")
                || lower.contains(&token.to_lowercase())
                || allowed.iter().any(|a| a == token || a.contains(token))
            {
                continue;
            }
            let stem = token.trim_end_matches("/**").trim_end_matches("/*");
            if lower.contains(&stem.to_lowercase()) || (path_like && composed_from(&lower, token)) {
                continue;
            }
            out.push(Diagnostic { kind: "literal", message: format!("INVENTED LITERAL: `{token}` is not in the request. Use the request's own path or host, or declare a `const:` placeholder and ask for it.") });
        }
    }
}

/// A path composed from the request's own words (`./catalog/<slug>.md` with the slug listed
/// in the request) is not invented: every directory and the file's stem appear in the request;
/// a pure glob segment composes nothing.
fn composed_from(lower: &str, token: &str) -> bool {
    let body = token
        .trim_start_matches("./")
        .trim_start_matches("../")
        .trim_start_matches("~/");
    let segments: Vec<&str> = body
        .split('/')
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect();
    !segments.is_empty()
        && segments.iter().all(|segment| {
            let stem = segment.rsplit_once('.').map_or(*segment, |(stem, _)| stem);
            let stem = stem.trim_matches('*');
            stem.is_empty() || lower.contains(&stem.to_lowercase())
        })
}

#[must_use]
pub fn tool_of<'a>(doc: &'a Value, task: &str) -> &'a str {
    doc.get("tasks")
        .and_then(|t| t.get(task))
        .and_then(|t| t.get("invoke"))
        .and_then(|i| i.get("tool"))
        .and_then(Value::as_str)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stated_send_no_task_carries_is_dropped_and_a_carried_one_is_not() {
        let mut plan = Plan::default();
        plan.effects.push(crate::plan::Effect::new(
            crate::plan::EffectVerb::Send,
            "the brief",
            "ask me before anything is sent",
            EffectPolicy::HumanFirst,
        ));
        let dropped = serde_json::json!({"tasks": {
            "draft": {"infer": {"prompt": "brief"}},
            "review": {"with": {"brief": "${{ tasks.draft.output }}"}, "invoke": {"tool": "nika:prompt", "args": {"message": "Send?"}}}
        }});
        let mut out = Vec::new();
        dropped_effects(&plan, &dropped, &mut out);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(
            out[0].message.contains("DROPPED EFFECT") && out[0].message.contains("`send`"),
            "{}",
            out[0].message
        );
        let carried = serde_json::json!({"tasks": {
            "draft": {"infer": {"prompt": "brief"}},
            "send": {"with": {"brief": "${{ tasks.draft.output }}"}, "invoke": {"tool": "nika:fetch", "args": {"url": "${{ const.send_endpoint }}", "method": "POST"}}}
        }});
        let mut out = Vec::new();
        dropped_effects(&plan, &carried, &mut out);
        assert!(out.is_empty(), "{out:?}");
        let mut forbidden = Plan::default();
        forbidden.effects.push(crate::plan::Effect::new(
            crate::plan::EffectVerb::Send,
            "the brief",
            "never send it",
            EffectPolicy::Forbidden,
        ));
        let mut out = Vec::new();
        dropped_effects(&forbidden, &dropped, &mut out);
        assert!(
            out.is_empty(),
            "a forbidden effect is rightly absent: {out:?}"
        );
    }

    #[test]
    fn a_gate_the_request_never_states_is_invented_and_a_stated_one_is_not() {
        let doc = serde_json::json!({"tasks": {
            "draft": {"infer": {"prompt": "recap"}},
            "review": {"with": {"recap": "${{ tasks.draft.output }}"}, "invoke": {"tool": "nika:prompt", "args": {"message": "send?", "default": false}}},
            "send": {"with": {"approved": "${{ tasks.review.output }}"}, "when": "${{ with.approved == true }}", "invoke": {"tool": "nika:fetch", "args": {"url": "${{ const.send_endpoint }}", "method": "POST"}}}
        }});
        let mut unstated = Plan::default();
        unstated.effects.push(crate::plan::Effect::new(
            crate::plan::EffectVerb::Send,
            "le résumé",
            "envoie le résumé",
            EffectPolicy::Automatic,
        ));
        let mut out = Vec::new();
        invented_gates(&unstated, &doc, &mut out);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(
            out[0].message.contains("INVENTED GATE") && out[0].message.contains("`send`"),
            "{}",
            out[0].message
        );
        let mut stated = Plan::default();
        stated.effects.push(crate::plan::Effect::new(
            crate::plan::EffectVerb::Send,
            "le résumé",
            "demande-moi avant d'envoyer",
            EffectPolicy::HumanFirst,
        ));
        let mut out = Vec::new();
        invented_gates(&stated, &doc, &mut out);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn a_permit_entry_covers_the_path_it_names_or_globs() {
        assert!(covers("./data/paiements.csv", "./data/paiements.csv"));
        assert!(covers("./data/**", "./data/paiements.csv"));
        assert!(covers("./data/**", "./data/2026/paiements.csv"));
        assert!(covers("./reports/*", "./reports/juillet.csv"));
        assert!(covers("./reports/*.csv", "./reports/juillet.csv"));
        assert!(covers("./reports/**", "./reports/"));
        assert!(!covers("./data/**", "./out/rapport.md"));
        assert!(!covers("./reports/*.csv", "./reports/notes.md"));
        assert!(!covers("./data/paiements.csv", "./data/paiements.csv.bak"));
    }

    #[test]
    fn explicit_current_directory_is_the_same_relative_path() {
        let doc = serde_json::json!({"permits": {"fs": {
            "read": ["./entree.txt"], "write": ["./sortie.txt"]
        }}});
        let mut diagnostics = Vec::new();
        stated_paths(
            "Copie entree.txt dans sortie.txt.",
            &doc,
            &[],
            &[],
            &mut diagnostics,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(covers("entree.txt", "./entree.txt"));
        assert!(covers("././entree.txt", "entree.txt"));
        assert!(covers("./data/**", "data/a.txt"));
        assert!(covers("data/*.csv", "./data/a.csv"));
        assert!(!covers("./entree.txt", "../entree.txt"));
        assert!(!covers("./entree.txt", "/entree.txt"));
        assert!(!covers("./entree.txt", "dir/../entree.txt"));
        assert!(!covers("./entree.txt", ".entree.txt"));
    }

    #[test]
    fn relative_compound_paths_cannot_drop_their_literal_directory_prefix() {
        for intent in [
            "Copie le fichier dossier source/notes.txt vers dossier sortie/notes.txt.",
            "Copie \"dossier source/notes.txt\" vers \"dossier sortie/notes.txt\".",
        ] {
            assert_eq!(
                owes(intent, &["source/notes.txt"], &["sortie/notes.txt"], &[]),
                ["dossier source/notes.txt", "dossier sortie/notes.txt"],
                "{intent}"
            );
            assert!(
                owes(
                    intent,
                    &["dossier source/notes.txt"],
                    &["dossier sortie/notes.txt"],
                    &[]
                )
                .is_empty()
            );
        }
        let other = "Copy the file team notes/input.json to team notes/output.json";
        assert_eq!(
            owes(other, &["notes/input.json"], &["notes/output.json"], &[]),
            ["team notes/input.json", "team notes/output.json"]
        );
        assert!(
            owes(
                other,
                &["team notes/input.json"],
                &["team notes/output.json"],
                &[]
            )
            .is_empty()
        );
        assert_eq!(
            owes(
                "Read data/input.json and write to out/result.json",
                &["input.json"],
                &["result.json"],
                &[]
            ),
            ["data/input.json", "out/result.json"]
        );
    }

    /// The paths `stated_paths` still owes for `intent` once the candidate reads `read`,
    /// writes `write` and the human typed `clarified` for the source question.
    fn owes(intent: &str, read: &[&str], write: &[&str], clarified: &[&str]) -> Vec<String> {
        let doc = serde_json::json!({"permits": {"fs": {"read": read, "write": write}}});
        let clarified: Vec<String> = clarified.iter().map(|name| (*name).to_owned()).collect();
        let mut diagnostics = Vec::new();
        stated_paths(intent, &doc, &[], &clarified, &mut diagnostics);
        diagnostics
            .iter()
            .filter_map(|d| d.message.split('`').nth(1).map(str::to_owned))
            .collect()
    }

    /// The same, the candidate writing `sortie.txt`.
    fn owed(intent: &str, read: &[&str], clarified: &[&str]) -> Vec<String> {
        owes(intent, read, &["sortie.txt"], clarified)
    }

    #[test]
    fn a_destination_occurrence_is_realized_by_a_write_never_by_a_source_answer() {
        let destined = "Notes équipe.txt doit aller dans Copie équipe.txt.";
        let notes = ["Notes équipe.txt"];
        let both = ["Notes équipe.txt", "Copie équipe.txt"];
        // Both names typed as sources and read: nothing writes the `équipe.txt` after `dans`.
        assert_eq!(owes(destined, &both, &[], &both), ["équipe.txt"]);
        // Writing the literal the reader read there realizes it, whatever the typed answer.
        assert!(owes(destined, &notes, &["Copie équipe.txt"], &notes).is_empty());
        assert!(owes(destined, &notes, &["Copie équipe.txt"], &both).is_empty());
        // A write elsewhere does not.
        assert_eq!(
            owes(destined, &notes, &["sortie.txt"], &both),
            ["équipe.txt"]
        );
        // Lowering `İ` lengthens the text: each occurrence keeps its own offset and role (the
        // destination right after `«` would otherwise be sliced inside that character).
        let dotted = "İci : Notes équipe.txt doit aller dans «équipe.txt».";
        assert_eq!(owes(dotted, &notes, &[], &notes), ["équipe.txt"]);
        assert!(owes(dotted, &notes, &["équipe.txt"], &notes).is_empty());
    }

    #[test]
    fn a_suffix_inside_a_longer_stated_path_is_that_paths_occurrence() {
        let archived = "Notes équipe.txt doit être comparé avec ./archive/équipe.txt.";
        let notes = ["Notes équipe.txt"];
        let both = ["Notes équipe.txt", "./archive/équipe.txt"];
        assert!(owes(archived, &both, &[], &notes).is_empty());
        // The rooted path is still owed on its own.
        assert_eq!(
            owes(archived, &notes, &[], &notes),
            ["./archive/équipe.txt"]
        );
        // A separately named `équipe.txt` stays a file of its own.
        let beside =
            "Notes équipe.txt et équipe.txt doivent être comparés avec ./archive/équipe.txt.";
        assert_eq!(owes(beside, &both, &[], &notes), ["équipe.txt"]);
    }

    #[test]
    fn a_typed_whole_name_realizes_the_occurrences_it_holds_and_no_other() {
        let opening = "Notes équipe.txt doit être copié tel quel dans sortie.txt.";
        assert_eq!(owed(opening, &["Notes équipe.txt"], &[]), ["équipe.txt"]);
        assert!(owed(opening, &["Notes équipe.txt"], &["Notes équipe.txt"]).is_empty());
        let lowercase = "Copie notes équipe.txt tel quel dans sortie.txt.";
        assert!(owed(lowercase, &["notes équipe.txt"], &["notes équipe.txt"]).is_empty());
        let worded = "Lis le fichier Notes de réunion.txt et écris son contenu dans sortie.txt.";
        let whole = "Notes de réunion.txt";
        assert!(owed(worded, &[whole], &[whole]).is_empty());
        // A separately named `équipe.txt` stays a file of its own.
        let beside = "Notes équipe.txt doit être fusionné avec équipe.txt dans sortie.txt.";
        let name = "Notes équipe.txt";
        assert_eq!(owed(beside, &[name], &[name]), ["équipe.txt"]);
        assert!(owed(beside, &[name, "équipe.txt"], &[name]).is_empty());
        // A typed name the request never writes, or one the candidate never opens, holds nothing.
        assert_eq!(
            owed(opening, &["autre.txt"], &["autre.txt"]),
            ["équipe.txt"]
        );
        assert_eq!(
            owed(opening, &["autre.txt"], &["Notes équipe.txt"]),
            ["équipe.txt"]
        );
        // An unquoted traversal: the folder and the file the reader split are the one name.
        let traversal = "Lis ../Partage/Notes équipe.txt et écris son contenu dans sortie.txt.";
        let exact = "../Partage/Notes équipe.txt";
        assert_eq!(
            owed(traversal, &[exact], &[]),
            ["../Partage/Notes", "équipe.txt"]
        );
        assert!(owed(traversal, &[exact], &[exact]).is_empty());
        // The source answer never realizes a stated destination, even inside its own span.
        let doc = serde_json::json!({"permits": {"fs": {"read": ["Notes dans sortie.txt"]}}});
        let typed = ["Notes dans sortie.txt".to_owned()];
        let mut diagnostics = Vec::new();
        stated_paths(
            "Lis Notes dans sortie.txt.",
            &doc,
            &[],
            &typed,
            &mut diagnostics,
        );
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(
            diagnostics[0].message.contains("`sortie.txt`"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn the_typed_source_answer_is_the_only_clarification() {
        let answers = BTreeMap::from([
            (
                SOURCE_PATHS.to_owned(),
                r#"["«Notes de réunion.txt»", "Notes équipe.txt", "./in/", "./a.md"]"#.to_owned(),
            ),
            (
                "const.output_path".to_owned(),
                r#""Copie équipe.txt""#.to_owned(),
            ),
        ]);
        assert_eq!(
            clarified_sources(&answers),
            ["Notes de réunion.txt", "Notes équipe.txt", "./a.md"]
        );
        assert!(clarified_sources(&BTreeMap::new()).is_empty());
    }

    #[test]
    fn a_path_composed_from_the_requests_words_is_not_invented() {
        let lower = "for each slug solar-lamp, wind-chime read ./catalog/<slug>.md";
        assert!(composed_from(lower, "./catalog/solar-lamp.md"));
        assert!(composed_from(lower, "./catalog/*.md"));
        assert!(!composed_from(lower, "./catalog/moon-rock.md"));
        assert!(!composed_from(lower, "./archive/solar-lamp.md"));
    }

    #[test]
    fn the_answered_values_are_the_texts_a_candidate_may_carry() {
        let answers = BTreeMap::from([
            (
                "const.send_endpoint".to_owned(),
                "\"https://x.invalid/h\"".to_owned(),
            ),
            ("const.limit".to_owned(), "5".to_owned()),
        ]);
        // A BTreeMap answers in key order: `const.limit` before `const.send_endpoint`.
        assert_eq!(
            allowed_values(&answers),
            vec!["5".to_owned(), "https://x.invalid/h".to_owned()]
        );
    }

    #[test]
    fn the_effect_tasks_and_the_gates_are_read_from_the_document() {
        let doc = serde_json::json!({"tasks": {
            "review": {"invoke": {"tool": "nika:prompt", "args": {"message": "ok?"}}},
            "send": {"with": {"approved": "${{ tasks.review.output }}"}, "invoke": {"tool": "nika:fetch", "args": {"url": "https://x.invalid", "method": "POST"}}},
            "look": {"invoke": {"tool": "nika:fetch", "args": {"url": "https://x.invalid"}}},
            "save": {"after": {"send": "success"}, "invoke": {"tool": "nika:write", "args": {"path": "./out/x.md", "content": "c"}}}
        }});
        let (effects, gates) = effect_and_gate_tasks(&doc);
        assert_eq!(gates, vec!["review".to_owned()]);
        assert_eq!(effects, vec!["save".to_owned(), "send".to_owned()]);
        assert!(depends_on(&doc, "send", &gates));
        assert!(depends_on(&doc, "save", &gates));
        assert!(!depends_on(&doc, "look", &gates));
    }

    #[test]
    fn a_write_into_an_unnamed_file_is_owed_a_write_task() {
        let unnamed = |policy: EffectPolicy| {
            let mut plan = Plan::default();
            plan.effects.push(crate::plan::Effect::new(
                crate::plan::EffectVerb::Write,
                "un fichier",
                "dans un fichier",
                policy,
            ));
            plan
        };
        // The S98 J02 shape: a draft, nothing written.
        let drafted = serde_json::json!({"tasks": {
            "draft": {"infer": {"prompt": "Résume les notes fournies."}}
        }});
        let mut out = Vec::new();
        unnamed_writes(&unnamed(EffectPolicy::Automatic), &drafted, &mut out);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].kind, "destination");
        assert!(
            out[0].message.starts_with("UNWRITTEN DESTINATION")
                && out[0].message.contains("« dans un fichier »"),
            "{}",
            out[0].message
        );
        let written = serde_json::json!({"tasks": {
            "draft": {"infer": {"prompt": "Résume les notes fournies."}},
            "write_output": {"with": {"text": "${{ tasks.draft.output }}"}, "invoke": {"tool": "nika:write", "args": {"path": "${{ const.output_path }}", "content": "${{ with.text }}"}}}
        }});
        let mut out = Vec::new();
        unnamed_writes(&unnamed(EffectPolicy::HumanFirst), &written, &mut out);
        assert!(out.is_empty(), "{out:?}");
        // A prohibited or undecided write owes no task, and a named one is Law 1's.
        let mut named = Plan::default();
        named.effects.push(crate::plan::Effect::new(
            crate::plan::EffectVerb::Write,
            "./out/resume.md",
            "dans ./out/resume.md",
            EffectPolicy::Automatic,
        ));
        for plan in [
            unnamed(EffectPolicy::Forbidden),
            unnamed(EffectPolicy::Undecided),
            named,
        ] {
            let mut out = Vec::new();
            unnamed_writes(&plan, &drafted, &mut out);
            assert!(out.is_empty(), "{:?}: {out:?}", plan.effects);
        }
    }

    #[test]
    fn every_unnamed_destination_is_owed_its_own_write_task() {
        let effect = |target: &str, evidence: &str| {
            crate::plan::Effect::new(
                crate::plan::EffectVerb::Write,
                target,
                evidence,
                EffectPolicy::Automatic,
            )
        };
        let mut j02 = Plan::default();
        j02.effects.push(effect("un fichier", "dans un fichier"));
        // The assembler's own shape: the drafted text to the asked path.
        let asked = serde_json::json!({"tasks": {
            "draft": {"infer": {"prompt": "Résume les notes fournies."}},
            "write_output": {"with": {"text": "${{ tasks.draft.output }}"}, "invoke": {"tool": "nika:write", "args": {"path": "${{ const.output_path }}", "content": "${{ with.text }}"}}}
        }});
        let mut out = Vec::new();
        unnamed_writes(&j02, &asked, &mut out);
        assert!(out.is_empty(), "{out:?}");
        // A write of another (named) destination or back over the source witnesses nothing, and
        // two unnamed destinations are owed two write tasks.
        let mut beside = Plan::default();
        beside.effects.push(effect(
            "./out/dates.json",
            "Extrais les dates de ./agenda.md dans ./out/dates.json",
        ));
        beside.effects.push(effect("un fichier", "dans un fichier"));
        let mut two = j02.clone();
        two.effects
            .push(effect("un nouveau fichier", "dans un nouveau fichier"));
        for (plan, doc) in [
            (
                &beside,
                serde_json::json!({"tasks": {
                    "extract": {"infer": {"prompt": "dates"}},
                    "draft": {"infer": {"prompt": "résumé"}},
                    "write_dates": {"with": {"c": "${{ tasks.extract.output }}"}, "invoke": {"tool": "nika:write", "args": {"path": "./out/dates.json", "content": "${{ with.c }}"}}}
                }}),
            ),
            (
                &j02,
                serde_json::json!({"permits": {"fs": {"read": ["./notes.md"], "write": ["./notes.md"]}}, "tasks": {
                    "read": {"invoke": {"tool": "nika:read", "args": {"path": "./notes.md"}}},
                    "draft": {"with": {"notes": "${{ tasks.read.output }}"}, "infer": {"prompt": "Résume ${{ with.notes }}"}},
                    "write_back": {"with": {"c": "${{ tasks.draft.output }}"}, "invoke": {"tool": "nika:write", "args": {"path": "./notes.md", "content": "${{ with.c }}"}}}
                }}),
            ),
            (&two, asked.clone()),
        ] {
            let mut out = Vec::new();
            unnamed_writes(plan, &doc, &mut out);
            assert_eq!(out.len(), 1, "{:?}: {out:?}", plan.effects);
        }
    }

    /// The write task of `write_output` with this content argument.
    fn written(content: &str) -> Value {
        serde_json::json!({"tool": "nika:write", "args": {"path": "${{ const.output_path }}", "content": content}})
    }

    /// The unnamed-write diagnostics for « Résume mes notes dans un fichier » (the file lies in
    /// the draft's clause) over a draft, this write task and any extra tasks.
    fn requested_result(write_output: &Value, extra: &Value) -> Vec<Diagnostic> {
        use crate::plan::{Op, Step};
        let mut plan = Plan::default();
        plan.steps.push(Step::new(
            Op::Draft,
            "Résume mes notes dans un fichier",
            "mes notes dans un fichier",
            Vec::new(),
        ));
        plan.effects.push(Effect::new(
            EffectVerb::Write,
            "un fichier",
            "dans un fichier",
            EffectPolicy::Automatic,
        ));
        let mut doc = serde_json::json!({"tasks": {
            "draft": {"infer": {"prompt": "Résume ${{ inputs.item }}"}},
            "write_output": write_output
        }});
        if let (Some(tasks), Some(extra)) = (doc["tasks"].as_object_mut(), extra.as_object()) {
            tasks.extend(extra.clone());
        }
        let mut out = Vec::new();
        unnamed_writes(&plan, &doc, &mut out);
        out
    }

    #[test]
    fn a_requested_result_is_written_by_content_that_reads_a_produced_output() {
        // The produced text read directly, through a `with:` binding the content uses, or through
        // a conversion that reads it the same way.
        for (task, extra) in [
            (
                serde_json::json!({"invoke": written("${{ tasks.draft.output }}")}),
                serde_json::json!({}),
            ),
            (
                serde_json::json!({"with": {"text": "${{ tasks.draft.output.body }}"}, "invoke": written("Résumé : ${{ with.text }}")}),
                serde_json::json!({}),
            ),
            (
                serde_json::json!({"with": {"content": "${{ tasks.draft_json.output }}"}, "invoke": written("${{ with.content }}")}),
                serde_json::json!({"draft_json": {"with": {"data": "${{ tasks.draft.output }}"}, "invoke": {"tool": "nika:convert", "args": {"input": "${{ with.data }}", "from": "json", "to": "yaml"}}}}),
            ),
        ] {
            let out = requested_result(&task, &extra);
            assert!(out.is_empty(), "{task}: {out:?}");
        }
    }

    #[test]
    fn order_guards_unused_bindings_and_raw_input_are_no_requested_result() {
        // A literal written after the draft, guarded by it or beside a binding it never uses,
        // the raw input, and a cycle of tools carry no produced result.
        for (task, extra) in [
            (
                serde_json::json!({"after": {"draft": "success"}, "invoke": written("done")}),
                serde_json::json!({}),
            ),
            (
                serde_json::json!({"when": "${{ tasks.draft.status == 'success' }}", "invoke": written("done")}),
                serde_json::json!({}),
            ),
            (
                serde_json::json!({"with": {"text": "${{ tasks.draft.output }}"}, "invoke": written("done")}),
                serde_json::json!({}),
            ),
            (
                serde_json::json!({"invoke": written("${{ inputs.item }}")}),
                serde_json::json!({}),
            ),
            (
                serde_json::json!({"with": {"x": "${{ tasks.left.output }}"}, "invoke": written("${{ with.x }}")}),
                serde_json::json!({
                    "left": {"with": {"y": "${{ tasks.right.output }}"}, "invoke": {"tool": "nika:convert", "args": {"input": "${{ with.y }}"}}},
                    "right": {"with": {"y": "${{ tasks.left.output }}"}, "invoke": {"tool": "nika:convert", "args": {"input": "${{ with.y }}"}}}
                }),
            ),
        ] {
            let out = requested_result(&task, &extra);
            assert_eq!(out.len(), 1, "{task}: {out:?}");
            assert!(
                out[0].message.contains("produced result"),
                "{}",
                out[0].message
            );
        }
    }

    #[test]
    fn a_copy_into_an_unnamed_file_carries_its_source_as_it_is() {
        use crate::plan::{Op, Step};
        // A copy asks for no transformation: the source read as it is is the write's content.
        let mut copy = Plan::default();
        copy.steps.push(Step::new(
            Op::Read,
            "Copie notes.md dans un fichier",
            "notes.md",
            Vec::new(),
        ));
        copy.effects.push(Effect::new(
            EffectVerb::Write,
            "un fichier",
            "Copie notes.md dans un fichier",
            EffectPolicy::Automatic,
        ));
        let copied = serde_json::json!({"tasks": {
            "read_source": {"invoke": {"tool": "nika:read", "args": {"path": "notes.md"}}},
            "write_output": {"with": {"text": "${{ tasks.read_source.output }}"}, "invoke": written("${{ with.text }}")}
        }});
        let mut out = Vec::new();
        unnamed_writes(&copy, &copied, &mut out);
        assert!(out.is_empty(), "{out:?}");
    }
}
