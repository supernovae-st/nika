// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Which model steps feed which results: a static walk over a candidate's data edges, back from
//! each result sink to the tasks whose output reaches it.
//!
//! A request may need a model-written part (a summary, a classification, a translation, a draft).
//! Whether it does is the judge's typed answer; whether the bytes carry it is this walk's fact: a
//! model step (`infer:` · `agent:`) on the data path of the sink the judge chose. A model step no
//! sink reads is decorative, and a child-workflow call on the path hides its own steps. The walk is
//! pure, so the Session reruns it on a revision without a judge.
//!
//! Data edges are the language's own: a task reads another only through its `with:` bindings (a
//! `group.<g>` read reaches every member), and `outputs:` read task records at the boundary.
//! `after:` orders without carrying a value, so a step that only gates a write never feeds it.

use std::collections::{BTreeMap, BTreeSet};

use nika_schema::expression::{NamespaceRef, expr_refs, scan_templates};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawTask, RawWorkflow};
use nika_schema::{FileId, ParseMode};
use serde_json::Value;

/// A result sink: where a requested result leaves the workflow.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ModelSink {
    /// The effect task that writes or sends the result (its id). A writing agent is its own sink.
    Task(String),
    /// A workflow `outputs:` key.
    Output(String),
}

/// The model steps and child calls on each sink's data path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModelPaths {
    /// Every sink, in file order: the effect tasks, then the `outputs:` keys. The judge chooses the
    /// sink a model-written part shapes from this list.
    pub sinks: Vec<ModelSink>,
    /// `(task, sink)`: an `infer:` or `agent:` task whose output reaches `sink`.
    pub steps: Vec<(String, ModelSink)>,
    /// `(task, child, sink)`: a child-workflow call on `sink`'s data path, with the child's target
    /// (its path or registry pin); the child's own steps are not seen here.
    pub children: Vec<(String, String, ModelSink)>,
    /// The model steps whose output reaches no sink, in file order.
    pub unreached: Vec<String>,
    /// `(sink, destination)`: the literal path or URL a sink task writes or sends to, when its
    /// bytes state it whole (a templated one is the run's to resolve, never named here).
    pub destinations: Vec<(ModelSink, String)>,
}

/// The tools whose call reaches the world with a result: the set the approval laws read
/// ([`crate::fidelity::effect_and_gate_tasks`]), with a non-GET `nika:fetch`.
const EFFECT_TOOLS: [&str; 4] = ["nika:write", "nika:notify", "nika:emit", "nika:edit"];

/// The model steps and child calls on each result sink's data path in `source`, or `None` when
/// `source` does not parse.
#[must_use]
pub fn model_steps_on_paths(source: &str) -> Option<ModelPaths> {
    let wf = nika_schema::parse(source, FileId::new(0), ParseMode::Strict).ok()?;
    let tasks: BTreeMap<&str, &RawTask> = (wf.tasks.iter())
        .map(|t| (t.value.id.value.as_str(), &t.value))
        .collect();
    let members = group_members(&wf);
    let mut paths = ModelPaths::default();
    for task in &wf.tasks {
        if is_sink(&task.value) {
            let sink = ModelSink::Task(task.value.id.value.clone());
            if let Some(to) = destination(&task.value) {
                paths.destinations.push((sink.clone(), to));
            }
            paths.sinks.push(sink);
        }
    }
    for (key, _) in &wf.outputs {
        paths.sinks.push(ModelSink::Output(key.value.clone()));
    }
    let mut reached = BTreeSet::new();
    for sink in paths.sinks.clone() {
        let start = match &sink {
            ModelSink::Task(id) => vec![id.clone()],
            ModelSink::Output(key) => output_reads(&wf, key),
        };
        for id in upstream(&tasks, &members, start) {
            let Some(task) = tasks.get(id.as_str()) else {
                continue;
            };
            match &task.action {
                RawAction::Infer(_) | RawAction::Agent(_) => {
                    reached.insert(id.clone());
                    paths.steps.push((id, sink.clone()));
                }
                RawAction::Invoke(call) => {
                    if let RawInvokeTarget::Workflow(child) = &call.target {
                        paths.children.push((id, child.value.clone(), sink.clone()));
                    }
                }
                _ => {}
            }
        }
    }
    paths.unreached = (wf.tasks.iter())
        .map(|t| &t.value)
        .filter(|t| matches!(t.action, RawAction::Infer(_) | RawAction::Agent(_)))
        .map(|t| t.id.value.clone())
        .filter(|id| !reached.contains(id))
        .collect();
    Some(paths)
}

/// The literal path or URL an invoked sink writes or sends to: its `path` argument, else its
/// `url`, when the bytes state it whole.
fn destination(task: &RawTask) -> Option<String> {
    let RawAction::Invoke(call) = &task.action else {
        return None;
    };
    let args = &call.args.as_ref()?.value;
    let to = args.get("path").or_else(|| args.get("url"))?.as_str()?;
    (!to.contains("${{")).then(|| to.to_owned())
}

/// Whether `task` reaches the world with a result: an effect tool, a non-GET fetch, or an agent
/// granted an effect tool.
fn is_sink(task: &RawTask) -> bool {
    match &task.action {
        RawAction::Invoke(call) => {
            let RawInvokeTarget::Tool(tool) = &call.target else {
                return false;
            };
            if EFFECT_TOOLS.contains(&tool.value.as_str()) {
                return true;
            }
            let method = (call.args.as_ref())
                .and_then(|args| args.value.get("method"))
                .and_then(Value::as_str)
                .unwrap_or("GET");
            tool.value == "nika:fetch" && !method.eq_ignore_ascii_case("GET")
        }
        RawAction::Agent(agent) => {
            (agent.tools.iter()).any(|t| EFFECT_TOOLS.contains(&t.value.as_str()))
        }
        _ => false,
    }
}

/// The task ids an `outputs:` value reads.
fn output_reads(wf: &RawWorkflow, key: &str) -> Vec<String> {
    (wf.outputs.iter())
        .filter(|(name, _)| name.value == key)
        .flat_map(|(_, decl)| refs(&Value::String(decl.value().value.clone())))
        .filter_map(|r| match r {
            NamespaceRef::Tasks { id, .. } => Some(id),
            _ => None,
        })
        .collect()
}

/// The members of each `group:`, by group name.
fn group_members(wf: &RawWorkflow) -> BTreeMap<String, Vec<String>> {
    let mut members: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for task in &wf.tasks {
        if let Some(group) = &task.value.group {
            (members.entry(group.value.clone()).or_default()).push(task.value.id.value.clone());
        }
    }
    members
}

/// `start` and every task whose output reaches it through `with:` bindings, in visiting order.
fn upstream(
    tasks: &BTreeMap<&str, &RawTask>,
    members: &BTreeMap<String, Vec<String>>,
    start: Vec<String>,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    let mut stack = start;
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(task) = tasks.get(id.as_str()) else {
            continue;
        };
        order.push(id);
        for (_, value) in &task.with {
            for r in refs(&value.value) {
                match r {
                    NamespaceRef::Tasks { id, .. } => stack.push(id),
                    NamespaceRef::Group(name) => {
                        stack.extend(members.get(&name).cloned().unwrap_or_default());
                    }
                    _ => {}
                }
            }
        }
    }
    order
}

/// The namespace references of every `${{ }}` island in a value (a string, or any string inside a
/// list or a map).
fn refs(value: &Value) -> Vec<NamespaceRef> {
    let mut texts = Vec::new();
    strings(value, &mut texts);
    let mut out = Vec::new();
    for text in texts {
        if let Ok(islands) = scan_templates(text) {
            for island in &islands {
                out.extend(expr_refs(&island.expr));
            }
        }
    }
    out
}

fn strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => out.push(text),
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(map) => map.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
