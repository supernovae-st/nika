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

use crate::plan::{EffectPolicy, EffectVerb, Plan};
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
/// The reader reads a name it cannot bound as its last word (`Notes équipe.txt` opening a
/// sentence names `équipe.txt`); when the human typed the whole name for the source question
/// and the candidate reads it, a stated source every occurrence of which the request writes
/// inside that name is that name, and only then: an occurrence outside it is still a file
/// of its own, and a stated destination is never a source's.
pub fn stated_paths(
    intent: &str,
    doc: &Value,
    waived: &[String],
    clarified: &[String],
    out: &mut Vec<Diagnostic>,
) {
    let sources = crate::hot::stated_sources(intent);
    let mut paths = sources.clone();
    paths.extend(crate::hot::stated_destinations(intent));
    paths.dedup();
    let spans: Vec<(usize, usize)> = clarified
        .iter()
        .filter(|name| granted(doc, "read", name))
        .flat_map(|name| {
            occurrences(intent, name)
                .into_iter()
                .map(move |at| (at, at + name.len()))
        })
        .collect();
    for path in paths {
        let read_whole = sources.contains(&path) && inside(intent, &path, &spans);
        if waived.iter().any(|w| w == &path) || read_whole {
            continue;
        }
        if !granted(doc, "read", &path) && !granted(doc, "write", &path) {
            let boundary = doc
                .pointer("/permits/fs")
                .map_or_else(|| "none".to_owned(), Value::to_string);
            out.push(Diagnostic { kind: "path", message: format!("UNREALIZED PATH: the request names `{path}`; no `permits.fs.read` or `permits.fs.write` entry covers it (permits.fs = {boundary}), so no task opens it. Read it (nika:read / nika:glob + fs.read) or write it (nika:write + fs.write) as the request means, or name it in `gaps` if it cannot be reached.") });
        }
    }
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

/// Whether the request writes `path`, and every time inside one of `spans`.
fn inside(intent: &str, path: &str, spans: &[(usize, usize)]) -> bool {
    let found = occurrences(intent, path);
    !found.is_empty()
        && found.iter().all(|&at| {
            spans
                .iter()
                .any(|&(from, to)| from <= at && at + path.len() <= to)
        })
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

    /// The paths `stated_paths` still owes for `intent` once the candidate reads `read` and
    /// the human typed `clarified` for the source question.
    fn owed(intent: &str, read: &[&str], clarified: &[&str]) -> Vec<String> {
        let doc = serde_json::json!({"permits": {"fs": {"read": read, "write": ["sortie.txt"]}}});
        let clarified: Vec<String> = clarified.iter().map(|name| (*name).to_owned()).collect();
        let mut diagnostics = Vec::new();
        stated_paths(intent, &doc, &[], &clarified, &mut diagnostics);
        diagnostics
            .iter()
            .filter_map(|d| d.message.split('`').nth(1).map(str::to_owned))
            .collect()
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
}
