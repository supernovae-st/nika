// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What each task of a judged candidate touches, as the checker infers it from the bytes (R4
//! A11): the engine facts that decide which task a judge may still name as doing something the
//! request does not ask. A task's effects are the checker's own inference over the workflow
//! reduced to that task and its `const:` block, so a path named through a constant resolves as
//! the runtime resolves it; a face the checker cannot pin (a computed path, host or program, a
//! composed child) leaves the task open.
//!
//! The facts settle a task, never a name or a resemblance, when it invokes a tool with no effect
//! of its own and touches only what the request asks:
//! - every path it reads is one the request names, as the reader locates paths;
//! - every path it writes is one that a part of the request the judge answered `carried` names,
//!   and that no task of the candidate reads: the output that part asks, the path a rehearsal
//!   room reads back.
//!
//! Writing the output a carried part asks is that part, never an extra effect, and a task with
//! no effect has none the request could leave unasked: a settled task is never offered as one
//! doing something unasked. Every other task stays open (another verb, a nested workflow, a model
//! call, a process, the network, a tool outside that list, a write elsewhere or to a path a task
//! reads): whether it does something unasked stays the judge's.

use nika_compile::{stated_destinations, stated_sources};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawTask, RawWorkflow};
use serde_json::{Value, json};

/// The tools with no effect of their own: a read and a write (their paths are judged apart), a
/// search, a conversion, a jq program, a check.
const QUIET: [&str; 13] = [
    "nika:read",
    "nika:write",
    "nika:glob",
    "nika:grep",
    "nika:jq",
    "nika:assert",
    "nika:convert",
    "nika:validate",
    "nika:date",
    "nika:hash",
    "nika:json_diff",
    "nika:json_merge_patch",
    "nika:inspect",
];

/// What a question shown the facts is told they are.
const EFFECTS: &str = "`effects` lists what each task of the candidate touches, as the engine's checker infers it from these bytes (`reads`, `writes`, each path resolved through `const:`). A task the engine settles (`settled`) reads only paths the request names and writes only the output a part of the request judged carried states (`part`), a path no task reads: writing the output a carried part asks is that part, never an extra effect, so such a task is never offered as one doing something the request does not ask.";

/// What one task touches: the paths it reads and writes, and whether anything else (another
/// verb, a tool with an effect of its own, a face the checker cannot pin) leaves it open.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Touch {
    id: String,
    reads: Vec<String>,
    writes: Vec<String>,
    open: bool,
}

/// The engine facts of what a judged candidate's tasks touch, beside the paths its request names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Effects {
    /// Each task, in the candidate's order.
    tasks: Vec<Touch>,
    /// Every path the request names, without its leading `./`.
    named: Vec<String>,
}

impl Effects {
    /// The facts of `candidate` judged against the request `intent`; `None` when it does not
    /// parse.
    #[must_use]
    pub fn of(candidate: &str, intent: &str) -> Option<Self> {
        let workflow = nika_compile::parse(candidate).ok()?;
        let tasks = (workflow.tasks.iter())
            .map(|task| touch(&workflow, &task.value))
            .collect();
        Some(Self {
            tasks,
            named: located(intent),
        })
    }

    /// The tasks the facts leave open, in the candidate's order, given the parts the judge
    /// answered `carried` among `records` (the questions its verdict asked so far): the only
    /// ones a judge may name as doing something the request does not ask.
    #[must_use]
    pub fn open(&self, records: &[Value]) -> Vec<String> {
        let carried = carried(records);
        (self.tasks.iter())
            .filter(|touch| self.settles(touch, &carried).is_none())
            .map(|touch| touch.id.clone())
            .collect()
    }

    /// `instructions` followed by what the facts are; `state` gains them (`effects`: each task,
    /// what it reads and writes, and why the facts settle it, null when they leave it open).
    #[must_use]
    pub fn show(&self, state: &mut Value, records: &[Value], instructions: &str) -> String {
        state["effects"] = self.facts(records);
        format!("{instructions} {EFFECTS}")
    }

    /// The extra-operation question as the facts settle it with no call, when they leave no task
    /// open: `only_requested`, with what settled each task, as the verdict records it.
    #[must_use]
    pub fn settled(&self, records: &[Value]) -> Value {
        json!({"question": "verify-extra", "settled": "only_requested", "by": "engine",
            "effects": self.facts(records)})
    }

    /// Each task as a question is shown it: what it reads and writes, and why the facts settle
    /// it (null when they leave it open).
    fn facts(&self, records: &[Value]) -> Value {
        let carried = carried(records);
        let tasks: Vec<Value> = (self.tasks.iter())
            .map(|touch| {
                json!({"task": touch.id, "reads": touch.reads, "writes": touch.writes,
                    "settled": self.settles(touch, &carried)})
            })
            .collect();
        Value::Array(tasks)
    }

    /// Why the facts settle `touch` under the `carried` parts: the request's paths it reads and
    /// each output it writes with the carried part that states it; `None` when it is open, reads
    /// a path the request does not name, or writes a path no carried part names or a task reads.
    fn settles(&self, touch: &Touch, carried: &[&str]) -> Option<Value> {
        let named = |path: &String| self.named.contains(&plain(path));
        if touch.open || !touch.reads.iter().all(named) {
            return None;
        }
        let read = |path: &str| {
            (self.tasks.iter()).any(|task| task.reads.iter().any(|read| plain(read) == plain(path)))
        };
        let mut writes = Vec::new();
        for path in &touch.writes {
            let stated = |part: &&&str| located(part).contains(&plain(path));
            let part = carried.iter().find(stated).filter(|_| !read(path))?;
            writes.push(json!({"path": path, "part": part}));
        }
        Some(json!({"reads": touch.reads, "writes": writes}))
    }
}

/// What `task` touches, as the checker infers it over the workflow reduced to that task and its
/// constants: open when it is no invoke of a quiet tool, reaches the network or a process, calls
/// a tool with an effect of its own, or has a face the checker cannot pin.
fn touch(workflow: &RawWorkflow, task: &RawTask) -> Touch {
    let mut alone = workflow.clone();
    alone
        .tasks
        .retain(|kept| kept.value.id.value == task.id.value);
    let inferred = nika_check::infer_permits(&alone);
    let permits = &inferred.permits;
    let quiet = |tool: &str| QUIET.contains(&tool);
    let invoked = match &task.action {
        RawAction::Invoke(invoke) => match &invoke.target {
            RawInvokeTarget::Tool(tool) => quiet(&tool.value),
            RawInvokeTarget::Workflow(_) => false,
        },
        _ => false,
    };
    let open = !invoked
        || inferred.partial.any()
        || permits.net.is_some()
        || permits.allows_exec()
        || !(permits.tools.iter().flatten()).all(|tool| quiet(tool));
    let (reads, writes) = (permits.fs.as_ref())
        .map(|fs| (fs.read.clone(), fs.write.clone()))
        .unwrap_or_default();
    Touch {
        id: task.id.value.clone(),
        reads,
        writes,
        open,
    }
}

/// The parts of the request the judge answered `carried` among `records`, over the bytes or over
/// a run of them, as each question's record names its part.
fn carried(records: &[Value]) -> Vec<&str> {
    let part = |id: &str| id.starts_with("verify-part-") || id.starts_with("verify-observed-part-");
    (records.iter())
        .filter(|record| record["choice"] == "carried")
        .filter(|record| record["question"].as_str().is_some_and(part))
        .filter_map(|record| record["clause"]["text"].as_str())
        .collect()
}

/// The paths `text` names, as the reader locates them, each without its leading `./`.
fn located(text: &str) -> Vec<String> {
    let mut paths = stated_sources(text);
    paths.extend(stated_destinations(text));
    paths.iter().map(|path| plain(path)).collect()
}

/// A path without its leading `./`: `./out/x.json` and `out/x.json` name one file.
fn plain(path: &str) -> String {
    path.trim_start_matches("./").to_owned()
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;
