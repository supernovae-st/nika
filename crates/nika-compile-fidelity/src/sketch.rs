// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The sketch: the constrained intermediate between a request and a candidate (the plan's
//! W2, « bounded semantic actions over typed holes »). A sketch is structure only — the tasks,
//! their verbs and tools, the stated paths and hosts each one reaches, the data edges, the
//! gate that guards an effect, the loop over a list — with every literal a stated one. The
//! structural laws judge it before any text is written; the holes are the typed places a seat
//! fills (a prompt, a jq program, a schema, a builtin's argument, an argv); the document is
//! emitted deterministically from the sketch and the fills — the permits derived from what
//! the tasks reach (authority by construction, never written by a model), the bindings from
//! the edges, the gate as a `when:` over the prompt's answer. Knowledge and projection only:
//! no call, no file, no authority.
use serde_json::{Map, Value, json};

use super::hot::fold;

/// One task of a sketch: what it is and what it reaches, never how it is worded.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SketchTask {
    pub id: String,
    pub verb: Verb,
    /// `nika:<name>` or `mcp:<server>/<tool>`; an invoke's only.
    pub tool: Option<String>,
    /// The stated paths this task reads (a file, a folder glob).
    pub reads: Vec<String>,
    /// The stated paths this task writes.
    pub writes: Vec<String>,
    /// The stated hosts this task reaches.
    pub hosts: Vec<String>,
    /// The tasks this one follows by control (`after: {id: success}`).
    pub after: Vec<String>,
    /// The tasks this one reads by data (`with: {name: ${{ tasks.<from>.output }}}`).
    pub with: Vec<Edge>,
    /// The `nika:prompt` task whose answer guards this effect.
    pub gated_by: Option<String>,
    /// The task whose output is the list this task loops over.
    pub for_each: Option<String>,
    /// One line on what the task is for: the hole's prompt to the seat.
    pub purpose: String,
    /// An agent's turn bound (1..=1000), when the sketch states one; `None` keeps the historical
    /// emission (4).
    pub max_turns: Option<u32>,
    /// An agent's own tool whitelist, when the sketch states one (`[]` is no tool); `None` keeps
    /// the historical emission (no tool). Each agent keeps its own list.
    pub tools: Option<Vec<String>>,
    /// A loop's stop-at-first-error policy, when the sketch states one; `None` keeps the
    /// historical emission (`false`).
    pub fail_fast: Option<bool>,
}

impl SketchTask {
    /// A task with no reach, edge, control or purpose beyond its words: every optional control
    /// omitted (the historical emission) and every collection empty.
    #[must_use]
    pub fn new(id: impl Into<String>, verb: Verb, purpose: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            verb,
            tool: None,
            reads: Vec::new(),
            writes: Vec::new(),
            hosts: Vec::new(),
            after: Vec::new(),
            with: Vec::new(),
            gated_by: None,
            for_each: None,
            purpose: purpose.into(),
            max_turns: None,
            tools: None,
            fail_fast: None,
        }
    }
}

/// One data edge: the name the task reads under, and the task it reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub name: String,
    pub from: String,
}

/// The four verbs, and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Infer,
    Invoke,
    Exec,
    Agent,
}

impl Verb {
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Infer => "infer",
            Self::Invoke => "invoke",
            Self::Exec => "exec",
            Self::Agent => "agent",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        match word {
            "infer" => Some(Self::Infer),
            "invoke" => Some(Self::Invoke),
            "exec" => Some(Self::Exec),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

/// A sketch: a name, its tasks in order (a task references only earlier ones) and the workflow's
/// named results.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Sketch {
    pub name: String,
    pub tasks: Vec<SketchTask>,
    /// The workflow's named results, each `{name, from}` (an output name and the task whose
    /// output it is). `None` (omitted or null) keeps the historical single `result` of the last
    /// task; `Some([])` states no output; a list states exactly these.
    pub outputs: Option<Vec<Edge>>,
}

impl Sketch {
    /// A sketch of these tasks with its outputs omitted (the historical single `result`).
    #[must_use]
    pub fn new(name: impl Into<String>, tasks: Vec<SketchTask>) -> Self {
        Self {
            name: name.into(),
            tasks,
            outputs: None,
        }
    }
}

/// A typed place the seat fills: the task, the field (`prompt` · `schema` · `expression` ·
/// `command` · `args.<name>`), the kind of value expected, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hole {
    pub task: String,
    pub field: String,
    pub kind: &'static str,
    pub required: bool,
    pub why: String,
}

/// One filled hole.
#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    pub task: String,
    pub field: String,
    pub value: Value,
}

/// The fields a sketch task may carry, and an edge's: anything else is refused, never ignored.
const TASK_FIELDS: &[&str] = &[
    "id",
    "verb",
    "tool",
    "reads",
    "writes",
    "hosts",
    "after",
    "with",
    "gated_by",
    "for_each",
    "purpose",
    "max_turns",
    "tools",
    "fail_fast",
];
const EDGE_FIELDS: &[&str] = &["name", "from"];

/// `value` as an object whose keys are all in `fields`; the refusal names the path and the
/// closed set, never the value a refused key carries.
fn closed<'a>(
    value: &'a Value,
    fields: &[&str],
    at: &str,
) -> Result<&'a Map<String, Value>, String> {
    let map = value
        .as_object()
        .ok_or_else(|| format!("`{at}` must be an object"))?;
    // A key outside the closed set is untrusted input: it is counted, never repeated.
    if map.keys().any(|k| !fields.contains(&k.as_str())) {
        return Err(format!(
            "`{at}` carries a key outside its fields ({})",
            fields.join(", ")
        ));
    }
    Ok(map)
}

/// An optional string field: absent or null is none; any other type is refused.
fn optional_text(map: &Map<String, Value>, key: &str, at: &str) -> Result<Option<String>, String> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(format!("`{at}.{key}` must be a string")),
    }
}

/// An optional array of strings: absent or null is empty; a non-array or a non-string item is
/// refused by its index, never filtered out.
fn optional_texts(map: &Map<String, Value>, key: &str, at: &str) -> Result<Vec<String>, String> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(k, item)| {
                item.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("`{at}.{key}[{k}]` must be a string"))
            })
            .collect(),
        Some(_) => Err(format!("`{at}.{key}` must be an array of strings")),
    }
}

/// A task's data edges: each a closed `{name, from}` object with both strings present.
fn edges(map: &Map<String, Value>, at: &str) -> Result<Vec<Edge>, String> {
    Ok(edge_list(map, "with", at)?.unwrap_or_default())
}

/// A list of closed `{name, from}` objects under `key`: `None` when absent or null.
fn edge_list(map: &Map<String, Value>, key: &str, at: &str) -> Result<Option<Vec<Edge>>, String> {
    let items = match map.get(key) {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Array(items)) => items,
        Some(_) => {
            return Err(format!(
                "`{at}.{key}` must be an array of `{{name, from}}` objects"
            ));
        }
    };
    items
        .iter()
        .enumerate()
        .map(|(k, edge)| {
            let at = format!("{at}.{key}[{k}]");
            let edge = closed(edge, EDGE_FIELDS, &at)?;
            let field = |key: &str| {
                optional_text(edge, key, &at)?
                    .ok_or_else(|| format!("`{at}` has no `{key}` string"))
            };
            Ok(Edge {
                name: field("name")?,
                from: field("from")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()
        .map(Some)
}

/// An optional control of an exact JSON type: absent or null is none; another type is refused.
fn optional_u32(map: &Map<String, Value>, key: &str, at: &str) -> Result<Option<u32>, String> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("`{at}.{key}` must be a whole number")),
    }
}

fn optional_bool(map: &Map<String, Value>, key: &str, at: &str) -> Result<Option<bool>, String> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(format!("`{at}.{key}` must be a boolean")),
    }
}

/// One task read exactly: closed fields, exact types, `id` and `verb` present.
fn task_of(value: &Value, at: &str) -> Result<SketchTask, String> {
    let task = closed(value, TASK_FIELDS, at)?;
    let id = optional_text(task, "id", at)?.ok_or_else(|| format!("`{at}.id` must be a string"))?;
    let verb = optional_text(task, "verb", at)?
        .as_deref()
        .and_then(Verb::from_word)
        .ok_or_else(|| format!("`{at}.verb` is not infer|invoke|exec|agent"))?;
    Ok(SketchTask {
        id,
        verb,
        tool: optional_text(task, "tool", at)?,
        reads: optional_texts(task, "reads", at)?,
        writes: optional_texts(task, "writes", at)?,
        hosts: optional_texts(task, "hosts", at)?,
        after: optional_texts(task, "after", at)?,
        with: edges(task, at)?,
        gated_by: optional_text(task, "gated_by", at)?,
        for_each: optional_text(task, "for_each", at)?,
        purpose: optional_text(task, "purpose", at)?.unwrap_or_default(),
        max_turns: optional_u32(task, "max_turns", at)?,
        tools: match task.get("tools") {
            None | Some(Value::Null) => None,
            Some(_) => Some(optional_texts(task, "tools", at)?),
        },
        fail_fast: optional_bool(task, "fail_fast", at)?,
    })
}

impl Sketch {
    /// A sketch read exactly from the seat's JSON (`{name?, tasks: [{id, verb, tool?, reads?,
    /// writes?, hosts?, after?, with?: [{name, from}], gated_by?, for_each?, purpose?, max_turns?,
    /// tools?, fail_fast?}], outputs?: [{name, from}]}`).
    /// An omitted or null optional field is its empty value; a field outside the closed set, a
    /// value of the wrong type or a malformed array item is refused by its path, never dropped.
    ///
    /// # Errors
    /// The path and reason the record cannot be read: nothing is guessed.
    pub fn from_json(record: &Value) -> Result<Self, String> {
        let root = record.as_object();
        let name = root
            .map(|map| optional_text(map, "name", "sketch"))
            .transpose()?
            .flatten()
            .unwrap_or_default();
        let outputs = root
            .map(|map| edge_list(map, "outputs", "sketch"))
            .transpose()?
            .flatten();
        let tasks = record
            .get("tasks")
            .and_then(Value::as_array)
            .ok_or_else(|| "`tasks` is missing or not an array".to_owned())?;
        let tasks = tasks
            .iter()
            .enumerate()
            .map(|(k, task)| task_of(task, &format!("tasks[{k}]")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            name: if name.is_empty() {
                "sketch".to_owned()
            } else {
                name
            },
            tasks,
            outputs,
        })
    }
}

/// The structural laws over a sketch: ids, references, the gate, the verbs' tools, every
/// literal stated. Each refusal names the task and the fix; an empty list admits the sketch.
#[must_use]
pub fn structural_laws(sketch: &Sketch, intent: &str, allowed: &[String]) -> Vec<String> {
    structural_laws_observed(sketch, intent, allowed, None)
}

/// The same laws with the caller's observation of the stated files: a READ may also reach the
/// one file the observation places under a bare name the request states (the fidelity law's
/// placement), never a write or a host.
#[must_use]
pub fn structural_laws_observed(
    sketch: &Sketch,
    intent: &str,
    allowed: &[String],
    observed: Option<&Value>,
) -> Vec<String> {
    let placed: Vec<String> = (crate::hot::stated_sources(intent).iter())
        .filter_map(|name| crate::fidelity::placed(observed, name))
        .collect();
    let mut out = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let intent = fold(intent);
    let allowed: Vec<String> = allowed.iter().map(|a| fold(a)).collect();
    let stated = |literal: &str| {
        let needle = fold(literal);
        !needle.is_empty() && (intent.contains(&needle) || allowed.iter().any(|a| a == &needle))
    };
    if sketch.tasks.is_empty() {
        out.push("the sketch has no task".to_owned());
    }
    for task in &sketch.tasks {
        let id = task.id.as_str();
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            out.push(format!("`{id}` is not a snake_case task id"));
        }
        if seen.contains(&id) {
            out.push(format!("`{id}` is declared twice"));
        }
        edge_laws(task, &mut out);
        control_laws(task, &mut out);
        let earlier = |target: &str| seen.contains(&target);
        for from in task.with.iter().map(|e| e.from.as_str()) {
            if !earlier(from) {
                out.push(format!(
                    "`{id}` reads `{from}`, which is not an earlier task"
                ));
            }
        }
        for after in &task.after {
            if !earlier(after) {
                out.push(format!(
                    "`{id}` follows `{after}`, which is not an earlier task"
                ));
            }
        }
        if let Some(gate) = task.gated_by.as_deref() {
            match sketch.tasks.iter().find(|t| t.id == gate) {
                Some(g)
                    if earlier(gate)
                        && g.verb == Verb::Invoke
                        && g.tool.as_deref() == Some("nika:prompt") => {}
                _ => out.push(format!(
                    "`{id}` is gated by `{gate}`, which is not an earlier `nika:prompt` task"
                )),
            }
        }
        if let Some(items) = task.for_each.as_deref()
            && !earlier(items)
        {
            out.push(format!(
                "`{id}` loops over `{items}`, which is not an earlier task"
            ));
        }
        match (task.verb, task.tool.as_deref()) {
            (Verb::Invoke, Some(tool)) if tool.starts_with("nika:") || tool.starts_with("mcp:") => {
            }
            (Verb::Invoke, _) => out.push(format!(
                "`{id}` invokes no `nika:<tool>` or `mcp:<server>/<tool>`"
            )),
            (verb, Some(tool)) => out.push(format!(
                "`{id}` is `{}` and names a tool `{tool}`: only an invoke does",
                verb.word()
            )),
            _ => {}
        }
        for (what, literals, observable) in [
            ("path", &task.reads, true),
            ("path", &task.writes, false),
            ("host", &task.hosts, false),
        ] {
            for literal in literals {
                let bare = literal.strip_prefix("./").unwrap_or(literal);
                let realized = stated(literal) || (observable && placed.iter().any(|p| p == bare));
                if !realized {
                    out.push(format!(
                        "`{id}` reaches the {what} `{literal}`, which the request never states: only a stated literal or an answered value enters a sketch"
                    ));
                }
            }
        }
        seen.push(id);
    }
    output_laws(sketch, &mut out);
    out
}

/// The laws over one task's data-edge names: each a `snake_case` identifier the task reads as
/// `${{ with.<name> }}`, bound once, never `approved` or `items` where the assembler binds them
/// for the task's gate or loop. An invalid name is refused without being echoed.
fn edge_laws(task: &SketchTask, out: &mut Vec<String>) {
    let id = task.id.as_str();
    let mut names: Vec<&str> = Vec::new();
    for edge in &task.with {
        let name = edge.name.as_str();
        if !is_identifier(name) {
            out.push(format!(
                "`{id}` binds an edge whose name is not a snake_case identifier"
            ));
            continue;
        }
        if names.contains(&name) {
            out.push(format!("`{id}` binds the edge name `{name}` twice"));
        }
        names.push(name);
        let reserved = (name == "approved" && task.gated_by.is_some())
            || (name == "items" && task.for_each.is_some());
        if reserved {
            out.push(format!(
                "`{id}` binds the edge name `{name}`, which its gate or loop binds: rename the edge"
            ));
        }
    }
}

/// The language's own agent turn ceiling (`nika-schema` parser, the runtime's mirror).
const MAX_TURNS: std::ops::RangeInclusive<u32> = 1..=1000;

/// The laws over one task's controls: an agent's turn bound and tool whitelist only on an agent,
/// the bound inside the language's range, each tool a named `nika:`/`mcp:` tool once (never a
/// glob or a negation); a loop's failure policy only on a task that loops. Effects an agent tool
/// may carry are judged by the caller's tool owner.
fn control_laws(task: &SketchTask, out: &mut Vec<String>) {
    let id = task.id.as_str();
    let agent = task.verb == Verb::Agent;
    if task.max_turns.is_some() && !agent {
        out.push(format!(
            "`{id}` states `max_turns`, which only an agent carries"
        ));
    }
    if let Some(turns) = task.max_turns
        && !MAX_TURNS.contains(&turns)
    {
        out.push(format!(
            "`{id}` states `max_turns` outside {}..={}",
            MAX_TURNS.start(),
            MAX_TURNS.end()
        ));
    }
    if let Some(tools) = &task.tools {
        if !agent {
            out.push(format!(
                "`{id}` states `tools`, which only an agent carries"
            ));
        }
        let mut seen: Vec<&str> = Vec::new();
        for tool in tools {
            let named = (tool.starts_with("nika:") || tool.starts_with("mcp:"))
                && !tool.contains(['*', '?', '[', '!', ' ']);
            if !named {
                out.push(format!(
                    "`{id}` lists a tool that is not one named `nika:<tool>` or `mcp:<server>/<tool>`"
                ));
            } else if seen.contains(&tool.as_str()) {
                out.push(format!("`{id}` lists the tool `{tool}` twice"));
            }
            seen.push(tool);
        }
    }
    if task.fail_fast.is_some() && task.for_each.is_none() {
        out.push(format!(
            "`{id}` states `fail_fast`, which only a task that loops carries"
        ));
    }
}

/// The laws over the workflow's named results: each a `snake_case` identifier, named once, the
/// result of a task the sketch has (any task, not only an earlier one). Edge reservations do not
/// apply: an output name is the workflow's, never a task's binding.
fn output_laws(sketch: &Sketch, out: &mut Vec<String>) {
    let Some(outputs) = &sketch.outputs else {
        return;
    };
    let mut names: Vec<&str> = Vec::new();
    for (k, output) in outputs.iter().enumerate() {
        let name = output.name.as_str();
        if !is_identifier(name) {
            out.push(format!(
                "`outputs[{k}]` names a result that is not a snake_case identifier"
            ));
        } else if names.contains(&name) {
            out.push(format!("`outputs[{k}]` names the result `{name}` twice"));
        }
        names.push(name);
        if !sketch.tasks.iter().any(|t| t.id == output.from) {
            out.push(format!(
                "`outputs[{k}]` is the result of a task the sketch does not have"
            ));
        }
    }
}

fn is_identifier(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The typed holes a sketch leaves for the seat, in task order.
#[must_use]
pub fn holes(sketch: &Sketch) -> Vec<Hole> {
    let mut out = Vec::new();
    let mut hole = |task: &SketchTask, field: &str, kind: &'static str, required: bool| {
        out.push(Hole {
            task: task.id.clone(),
            field: field.to_owned(),
            kind,
            required,
            why: task.purpose.clone(),
        });
    };
    for task in &sketch.tasks {
        match (task.verb, task.tool.as_deref()) {
            (Verb::Infer, _) => {
                hole(task, "prompt", "text", true);
                hole(task, "schema", "json_schema", false);
            }
            (Verb::Agent, _) => hole(task, "prompt", "text", true),
            (Verb::Exec, _) => hole(task, "command", "argv", true),
            (Verb::Invoke, Some("nika:jq")) => hole(task, "expression", "jq", true),
            // One edge is the content itself; none or several need a template that reads them.
            (Verb::Invoke, Some("nika:write")) => {
                hole(task, "args.content", "template", task.with.len() != 1);
            }
            (Verb::Invoke, Some("nika:notify")) => {
                hole(task, "args.target", "url", true);
                hole(task, "args.message", "template", true);
            }
            (Verb::Invoke, Some("nika:fetch")) => {
                hole(task, "args.url", "url", true);
                hole(task, "args.mode", "extract_mode", false);
            }
            (Verb::Invoke, Some("nika:prompt")) => hole(task, "args.message", "text", true),
            (Verb::Invoke, Some("nika:read" | "nika:glob")) => {}
            (Verb::Invoke, _) => hole(task, "args", "json_object", true),
        }
    }
    out
}

/// The fills read exactly from the seat's JSON (`{fills: [{task, field, value}]}`): each a
/// closed object whose `task` and `field` are nonempty strings and whose `value` is present.
/// Whether each fill matches a hole of the accepted sketch is [`complete_document`]'s law.
///
/// # Errors
/// The first fill that is not one, by its index or its `task.field`; never its value.
pub fn fills_from_json(record: &Value) -> Result<Vec<Fill>, String> {
    record
        .get("fills")
        .and_then(Value::as_array)
        .ok_or_else(|| "`fills` is missing or not an array".to_owned())?
        .iter()
        .enumerate()
        .map(|(k, f)| {
            // A fill's own names are untrusted until a sketch declares them: named by index.
            let at = format!("fills[{k}]");
            let map = closed(f, &["task", "field", "value"], &at)?;
            let task = optional_text(map, "task", &at)?.unwrap_or_default();
            let field = optional_text(map, "field", &at)?.unwrap_or_default();
            if task.is_empty() || field.is_empty() {
                return Err(format!("`{at}` names no task or no field"));
            }
            let value = map
                .get("value")
                .cloned()
                .ok_or_else(|| format!("`{at}` carries no `value`"))?;
            Ok(Fill { task, field, value })
        })
        .collect()
}

/// The arguments this task's sketch owns for its tool: every argument the assembler derives for
/// it (a stated path, its edge input, its edge-bound content, its channel), and the filesystem
/// reach the sketch states in `reads`/`writes` for the tools that take one (`path`, or a glob's
/// `pattern`) even where none is stated, so a whole `args` fill can never open a path the task
/// does not reach, whatever a permit would admit. Another tool's own argument of the same name
/// (`nika:grep`'s `pattern`, `nika:hash`'s `content`) stays the seat's to fill.
fn owned_args(task: &SketchTask) -> Vec<String> {
    let tool = task.tool.as_deref().unwrap_or_default();
    let mut owned: Vec<String> = default_args(task, tool).keys().cloned().collect();
    let reach = match tool {
        "nika:read" | "nika:grep" | "nika:write" | "nika:edit" => Some("path"),
        "nika:glob" => Some("pattern"),
        // A data tool reads its input by an edge the sketch states, never by a fill.
        "nika:jq" | "nika:convert" | "nika:validate" => Some("input"),
        _ => None,
    };
    if let Some(key) = reach
        && !owned.iter().any(|k| k == key)
    {
        owned.push(key.to_owned());
    }
    owned
}

/// The complete document of an accepted sketch and its fills, emitted only when every fill is
/// lawful against the sketch's own holes: each fill names a declared hole of this sketch, once,
/// with a value of the hole's kind; every
/// required hole is filled; a whole `args` object never carries a sketch-owned argument; a write's
/// content template reads every edge it is bound to. The emission is then [`document`]'s,
/// unchanged; [`document`] alone stays the partial projection the structural judge inspects.
///
/// # Errors
/// Every unlawful fill or unfilled required hole, each naming its `task.field` and never the
/// value it refuses.
pub fn complete_document(sketch: &Sketch, fills: &[Fill]) -> Result<Value, Vec<String>> {
    let holes = holes(sketch);
    let mut out = Vec::new();
    let mut seen: Vec<(&str, &str)> = Vec::new();
    for (k, fill) in fills.iter().enumerate() {
        let slot = format!("{}.{}", fill.task, fill.field);
        let Some(hole) = holes
            .iter()
            .find(|h| h.task == fill.task && h.field == fill.field)
        else {
            out.push(undeclared(sketch, fill, k));
            continue;
        };
        if seen.contains(&(hole.task.as_str(), hole.field.as_str())) {
            out.push(format!("fill `{slot}` is given twice: fill each hole once"));
            continue;
        }
        seen.push((hole.task.as_str(), hole.field.as_str()));
        if let Some(why) = kind_refusal(hole.kind, &fill.value) {
            out.push(format!("fill `{slot}` {why}"));
            continue;
        }
        if let Some(task) = sketch.tasks.iter().find(|t| t.id == fill.task) {
            owned_refusal(task, hole, &fill.value, &slot, &mut out);
        }
        if reads_a_task(&fill.value) {
            out.push(format!(
                "fill `{slot}` reads another task's output directly: a task reads only its edges (`${{{{ with.<name> }}}}`), which the sketch states"
            ));
        }
    }
    for hole in holes.iter().filter(|h| h.required) {
        if !seen.contains(&(hole.task.as_str(), hole.field.as_str())) {
            out.push(format!(
                "hole `{}.{}` is required and has no fill",
                hole.task, hole.field
            ));
        }
    }
    if out.is_empty() {
        Ok(document(sketch, fills))
    } else {
        Err(out)
    }
}

/// Why a fill names no hole: the sketch has no such task, the field is the sketch's own, or it is
/// simply not a hole the task leaves. Only names the sketch or the tool contract declares are
/// repeated (a task id, `args`, an argument the sketch owns); any other is named by its index.
fn undeclared(sketch: &Sketch, fill: &Fill, k: usize) -> String {
    let Some(task) = sketch.tasks.iter().find(|t| t.id == fill.task) else {
        return format!("`fills[{k}]` names a task the sketch does not have");
    };
    let owned = fill.field == "args"
        || fill
            .field
            .strip_prefix("args.")
            .is_some_and(|name| owned_args(task).iter().any(|k| k == name));
    if owned {
        format!(
            "fill `{}.{}` is owned by the sketch (its paths, edges and bindings): it is not a hole",
            fill.task, fill.field
        )
    } else {
        format!(
            "`fills[{k}]` names no hole of `{}`: fill only the listed holes",
            task.id
        )
    }
}

/// Whether a value is of a hole's kind; the refusal says what is expected, never the value.
fn kind_refusal(kind: &str, value: &Value) -> Option<String> {
    let text = value.as_str().filter(|t| !t.trim().is_empty());
    let ok = match kind {
        // The closed extract-mode set is the builtin contract's (`nika_cap`), judged on the
        // emitted arguments by the caller; here the hole asks for a mode word.
        "text" | "template" | "jq" | "url" | "extract_mode" => text.is_some(),
        "json_schema" | "json_object" => value.is_object(),
        "argv" => value.as_array().is_some_and(|items| {
            !items.is_empty()
                && items
                    .iter()
                    .all(|i| i.as_str().is_some_and(|t| !t.is_empty()))
        }),
        _ => false,
    };
    (!ok).then(|| match kind {
        "text" | "template" => "must be a nonempty string".to_owned(),
        "jq" => "must be a nonempty jq program string".to_owned(),
        "url" => "must be a nonempty URL string".to_owned(),
        "json_schema" => "must be a JSON schema object".to_owned(),
        "json_object" => "must be an argument object".to_owned(),
        "argv" => "must be a nonempty array of nonempty strings".to_owned(),
        "extract_mode" => "must be a nonempty extract mode string".to_owned(),
        other => format!("has no known kind `{other}`"),
    })
}

/// The sketch-owned values a lawful kind may still try to replace: a whole `args` object that
/// names a sketch-owned argument, a write's content template that drops an edge it is bound to.
fn owned_refusal(task: &SketchTask, hole: &Hole, value: &Value, slot: &str, out: &mut Vec<String>) {
    if hole.field == "args"
        && let Some(object) = value.as_object()
    {
        for key in owned_args(task)
            .iter()
            .filter(|k| object.contains_key(k.as_str()))
        {
            out.push(format!(
                "fill `{slot}` carries `{key}`, which the sketch owns (its paths, edges and bindings): leave it out"
            ));
        }
    }
    if hole.field == "args.content"
        && let Some(template) = value.as_str()
    {
        for edge in &task.with {
            if !reads_binding(template, &edge.name) {
                out.push(format!(
                    "fill `{slot}` drops the edge `{}` the task is bound to: the template reads `${{{{ with.{} }}}}`",
                    edge.name, edge.name
                ));
            }
        }
    }
}

/// Whether any string of a value references `tasks.<id>` inside a `${{ }}` expression: a hidden
/// data edge the sketch never stated.
fn reads_a_task(value: &Value) -> bool {
    match value {
        Value::String(text) => text.split("${{").skip(1).any(|expr| {
            expr.split("}}")
                .next()
                .is_some_and(|inner| inner.contains("tasks."))
        }),
        Value::Array(items) => items.iter().any(reads_a_task),
        Value::Object(map) => map.values().any(reads_a_task),
        _ => false,
    }
}

/// Whether a template reads `with.<name>` as a whole name (not a longer name it prefixes).
fn reads_binding(template: &str, name: &str) -> bool {
    let needle = format!("with.{name}");
    template.match_indices(&needle).any(|(at, _)| {
        template[at + needle.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'))
    })
}

fn output_of(task: &str) -> Value {
    json!(format!("${{{{ tasks.{task}.output }}}}"))
}

fn filled<'a>(fills: &'a [Fill], task: &str, field: &str) -> Option<&'a Value> {
    fills
        .iter()
        .find(|f| f.task == task && f.field == field)
        .map(|f| &f.value)
}

/// What the tasks reach, accumulated as the document is stated: the tools invoked, the paths
/// read and written, the hosts fetched, whether a language step needs the human's model.
#[derive(Default)]
struct Reach {
    tools: Vec<String>,
    reads: Vec<String>,
    writes: Vec<String>,
    hosts: Vec<String>,
    needs_model: bool,
}

fn push_once(list: &mut Vec<String>, item: &str) {
    if !list.iter().any(|x| x == item) {
        list.push(item.to_owned());
    }
}

impl Reach {
    /// The permits derived from what the tasks reach — never written by hand.
    fn permits(mut self) -> Map<String, Value> {
        let mut permits = Map::new();
        self.tools.sort();
        permits.insert("tools".to_owned(), json!(self.tools));
        if !self.reads.is_empty() || !self.writes.is_empty() {
            let mut fs = Map::new();
            if !self.reads.is_empty() {
                fs.insert("read".to_owned(), json!(self.reads));
            }
            if !self.writes.is_empty() {
                fs.insert("write".to_owned(), json!(self.writes));
            }
            permits.insert("fs".to_owned(), Value::Object(fs));
        }
        if !self.hosts.is_empty() {
            permits.insert("net".to_owned(), json!({"http": self.hosts}));
        }
        permits
    }
}

/// One task's node: its bindings from the edges, the gate as a `when:`, the loop, the control
/// edges, then its verb with the filled holes; what it reaches joins `reach`.
fn task_node(task: &SketchTask, fills: &[Fill], reach: &mut Reach) -> Value {
    let mut node = Map::new();
    let mut with = Map::new();
    for edge in &task.with {
        with.insert(edge.name.clone(), output_of(&edge.from));
    }
    if let Some(gate) = &task.gated_by {
        with.insert("approved".to_owned(), output_of(gate));
        node.insert("when".to_owned(), json!("${{ with.approved == true }}"));
    }
    if let Some(items) = &task.for_each {
        with.insert("items".to_owned(), output_of(items));
        node.insert(
            "for_each".to_owned(),
            json!({"items": "${{ with.items }}", "fail_fast": task.fail_fast.unwrap_or(false)}),
        );
    }
    if !with.is_empty() {
        node.insert("with".to_owned(), Value::Object(with));
    }
    if !task.after.is_empty() {
        let after: Map<String, Value> = task
            .after
            .iter()
            .map(|a| (a.clone(), json!("success")))
            .collect();
        node.insert("after".to_owned(), Value::Object(after));
    }
    for path in &task.reads {
        push_once(&mut reach.reads, path);
    }
    for path in &task.writes {
        push_once(&mut reach.writes, path);
    }
    for host in &task.hosts {
        push_once(&mut reach.hosts, host);
    }
    if let Some(object) = verb_node(task, fills, reach).as_object() {
        node.extend(object.clone());
    }
    Value::Object(node)
}

/// The verb of a task with its filled holes.
fn verb_node(task: &SketchTask, fills: &[Fill], reach: &mut Reach) -> Value {
    let id = task.id.as_str();
    let fill = |field: &str| filled(fills, id, field).cloned();
    match (task.verb, task.tool.as_deref()) {
        (Verb::Infer, _) => {
            reach.needs_model = true;
            let mut infer = json!({"prompt": fill("prompt").unwrap_or_else(|| json!(""))});
            if let Some(schema) = fill("schema") {
                infer["schema"] = schema;
            }
            json!({"infer": infer})
        }
        (Verb::Agent, _) => {
            reach.needs_model = true;
            let tools = task.tools.clone().unwrap_or_default();
            for tool in &tools {
                push_once(&mut reach.tools, tool);
            }
            json!({"agent": {"prompt": fill("prompt").unwrap_or_else(|| json!("")),
                "max_turns": task.max_turns.unwrap_or(4), "tools": tools}})
        }
        (Verb::Exec, _) => {
            json!({"exec": {"command": fill("command").unwrap_or_else(|| json!([]))}})
        }
        (Verb::Invoke, tool) => {
            let tool = tool.unwrap_or_default();
            push_once(&mut reach.tools, tool);
            let mut args = default_args(task, tool);
            for f in fills.iter().filter(|f| f.task == id) {
                if let Some(key) = f.field.strip_prefix("args.") {
                    args.insert(key.to_owned(), f.value.clone());
                } else if f.field == "expression" {
                    args.insert("expression".to_owned(), f.value.clone());
                } else if f.field == "args"
                    && let Some(object) = f.value.as_object()
                {
                    args.extend(object.clone());
                }
            }
            json!({"invoke": {"tool": tool, "args": Value::Object(args)}})
        }
    }
}

/// The document a sketch and its fills state: the envelope, the permits derived from what
/// the tasks reach, the tasks with their bindings, gates and loops, the last task's output.
#[must_use]
pub fn document(sketch: &Sketch, fills: &[Fill]) -> Value {
    let mut reach = Reach::default();
    let mut tasks = Map::new();
    for task in &sketch.tasks {
        tasks.insert(task.id.clone(), task_node(task, fills, &mut reach));
    }
    let mut root = Map::new();
    root.insert("nika".to_owned(), json!(sketch.name));
    if reach.needs_model {
        root.insert("model".to_owned(), json!("mock/echo"));
    }
    let consts = placeholders(fills);
    if !consts.is_empty() {
        root.insert("const".to_owned(), Value::Object(consts));
    }
    root.insert("permits".to_owned(), Value::Object(reach.permits()));
    root.insert("tasks".to_owned(), Value::Object(tasks));
    match &sketch.outputs {
        // The historical single result of the last task, for a sketch that states no outputs.
        None => {
            if let Some(last) = sketch.tasks.last() {
                root.insert("outputs".to_owned(), json!({"result": output_of(&last.id)}));
            }
        }
        Some(outputs) if outputs.is_empty() => {}
        Some(outputs) => {
            let named: Map<String, Value> = outputs
                .iter()
                .map(|o| (o.name.clone(), output_of(&o.from)))
                .collect();
            root.insert("outputs".to_owned(), Value::Object(named));
        }
    }
    Value::Object(root)
}

/// Every `const.<slug>` a fill references, declared empty: the placeholder of a value the
/// request leaves open (an endpoint, a name), asked as a question and granted by its answer.
fn placeholders(fills: &[Fill]) -> Map<String, Value> {
    let mut out = Map::new();
    for fill in fills {
        let mut texts = Vec::new();
        super::fidelity::strings(&fill.value, &mut texts);
        for text in texts {
            let mut rest = text.as_str();
            while let Some(at) = rest.find("const.") {
                let after = &rest[at + "const.".len()..];
                let end = after
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(after.len());
                if end > 0 {
                    out.entry(after[..end].to_owned())
                        .or_insert_with(|| json!(""));
                }
                rest = &after[end..];
            }
        }
    }
    out
}

/// The arguments a builtin takes from the sketch itself: the stated path it reads or writes,
/// the input it reads by its first edge (a program bound to several reads the object of their
/// names), the webhook channel, the article mode.
fn default_args(task: &SketchTask, tool: &str) -> Map<String, Value> {
    let output_name = |name: &str| json!(format!("${{{{ with.{name} }}}}"));
    let first_edge = task.with.first().map(|e| output_name(&e.name));
    let path = |list: &[String]| list.first().map(|p| json!(p));
    let (key, value) = match tool {
        "nika:read" | "nika:grep" => ("path", path(&task.reads)),
        "nika:glob" => ("pattern", path(&task.reads)),
        "nika:write" | "nika:edit" => ("path", path(&task.writes)),
        "nika:jq" if task.with.len() > 1 => {
            let edges = task
                .with
                .iter()
                .map(|e| (e.name.clone(), output_name(&e.name)));
            ("input", Some(Value::Object(edges.collect())))
        }
        "nika:jq" | "nika:convert" | "nika:validate" => ("input", first_edge.clone()),
        "nika:notify" => ("channel", Some(json!("webhook"))),
        "nika:fetch" => ("mode", Some(json!("article"))),
        _ => ("", None),
    };
    let mut args = Map::new();
    if let Some(value) = value {
        args.insert(key.to_owned(), value);
    }
    if matches!(tool, "nika:write" | "nika:edit")
        && let Some(content) = first_edge
    {
        args.insert("content".to_owned(), content);
    }
    args
}

mod record;
pub use record::{bound_answers, contract_projection, read_basis, replayed, replayed_observed};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use serde_json::json;

    use super::{Sketch, complete_document, document, fills_from_json, holes, structural_laws};

    const INTENT: &str = "Chaque lundi matin, lis ./tickets.json, résume les tickets ouverts, demande-moi avant d'envoyer le résumé à http://127.0.0.1:8793/hook";

    fn recap() -> Sketch {
        Sketch::from_json(&json!({
            "name": "recap",
            "tasks": [
                {"id": "read_tickets", "verb": "invoke", "tool": "nika:read", "reads": ["./tickets.json"], "purpose": "the tickets"},
                {"id": "open_only", "verb": "invoke", "tool": "nika:jq", "with": [{"name": "document", "from": "read_tickets"}], "purpose": "keep the open tickets"},
                {"id": "summarize", "verb": "infer", "with": [{"name": "tickets", "from": "open_only"}], "purpose": "summarize the open tickets"},
                {"id": "review", "verb": "invoke", "tool": "nika:prompt", "with": [{"name": "summary", "from": "summarize"}], "purpose": "ask before sending"},
                {"id": "send", "verb": "invoke", "tool": "nika:notify", "hosts": ["127.0.0.1"], "with": [{"name": "summary", "from": "summarize"}], "gated_by": "review", "purpose": "send the summary"}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn a_gated_recap_sketch_is_admitted_lists_its_holes_and_states_its_document() {
        let sketch = recap();
        assert_eq!(structural_laws(&sketch, INTENT, &[]), Vec::<String>::new());
        let fields: Vec<String> = holes(&sketch)
            .iter()
            .map(|h| format!("{}.{}", h.task, h.field))
            .collect();
        assert_eq!(
            fields,
            [
                "open_only.expression",
                "summarize.prompt",
                "summarize.schema",
                "review.args.message",
                "send.args.target",
                "send.args.message",
            ]
        );
        assert!(
            holes(&sketch)
                .iter()
                .find(|h| h.field == "schema")
                .is_some_and(|h| !h.required)
        );
        let fills = fills_from_json(&json!({"fills": [
            {"task": "open_only", "field": "expression", "value": "fromjson | map(select(.status == \"open\"))"},
            {"task": "summarize", "field": "prompt", "value": "Summarize: ${{ with.tickets }}"},
            {"task": "review", "field": "args.message", "value": "Send this summary?"},
            {"task": "send", "field": "args.target", "value": "http://127.0.0.1:8793/hook"},
            {"task": "send", "field": "args.message", "value": "${{ with.summary }}"}
        ]}))
        .unwrap();
        let doc = document(&sketch, &fills);
        assert_eq!(doc["nika"], "recap");
        assert_eq!(
            doc["model"], "mock/echo",
            "an infer needs the human's model"
        );
        assert_eq!(
            doc["permits"]["tools"],
            json!(["nika:jq", "nika:notify", "nika:prompt", "nika:read"])
        );
        assert_eq!(doc["permits"]["fs"]["read"], json!(["./tickets.json"]));
        assert_eq!(doc["permits"]["net"]["http"], json!(["127.0.0.1"]));
        assert_eq!(
            doc["tasks"]["read_tickets"]["invoke"]["args"]["path"],
            "./tickets.json"
        );
        assert_eq!(
            doc["tasks"]["open_only"]["invoke"]["args"]["input"],
            "${{ with.document }}"
        );
        assert_eq!(
            doc["tasks"]["open_only"]["invoke"]["args"]["expression"],
            "fromjson | map(select(.status == \"open\"))"
        );
        assert!(
            doc["tasks"]["summarize"]["infer"]
                .get("max_tokens")
                .is_none()
        );
        assert_eq!(
            doc["tasks"]["send"]["with"]["approved"],
            "${{ tasks.review.output }}"
        );
        assert_eq!(doc["tasks"]["send"]["when"], "${{ with.approved == true }}");
        assert_eq!(doc["tasks"]["send"]["invoke"]["args"]["channel"], "webhook");
        assert_eq!(
            doc["tasks"]["send"]["invoke"]["args"]["target"],
            "http://127.0.0.1:8793/hook"
        );
        assert_eq!(doc["outputs"]["result"], "${{ tasks.send.output }}");
    }

    #[test]
    fn the_structural_laws_name_an_invented_path_a_bad_reference_and_a_gate_that_is_no_prompt() {
        let sketch = Sketch::from_json(&json!({"tasks": [
            {"id": "read", "verb": "invoke", "tool": "nika:read", "reads": ["./secrets.json"]},
            {"id": "draft", "verb": "infer", "with": [{"name": "x", "from": "later"}], "tool": "nika:jq"},
            {"id": "send", "verb": "invoke", "tool": "nika:notify", "gated_by": "draft"},
            {"id": "Bad Id", "verb": "exec"}
        ]}))
        .unwrap();
        let refusals = structural_laws(&sketch, INTENT, &["./allowed.csv".to_owned()]);
        let text = refusals.join("\n");
        assert!(
            text.contains("`read` reaches the path `./secrets.json`"),
            "{text}"
        );
        assert!(
            text.contains("`draft` reads `later`, which is not an earlier task"),
            "{text}"
        );
        assert!(
            text.contains("`draft` is `infer` and names a tool"),
            "{text}"
        );
        assert!(
            text.contains("`send` is gated by `draft`, which is not an earlier `nika:prompt` task"),
            "{text}"
        );
        assert!(
            text.contains("`Bad Id` is not a snake_case task id"),
            "{text}"
        );
        assert!(Sketch::from_json(&json!({"tasks": [{"id": "x", "verb": "think"}]})).is_err());
        assert!(structural_laws(&Sketch::new("e", vec![]), INTENT, &[])[0].contains("no task"));
    }

    #[test]
    fn the_partial_projection_stays_open_while_complete_emission_requires_every_hole() {
        let sketch = recap();
        // The structural judge's projection needs no fill at all.
        let partial = document(&sketch, &[]);
        assert_eq!(
            partial["tasks"]["read_tickets"]["invoke"]["args"]["path"],
            "./tickets.json"
        );
        let refusals = complete_document(&sketch, &[]).unwrap_err().join("\n");
        for slot in [
            "open_only.expression",
            "summarize.prompt",
            "review.args.message",
            "send.args.target",
            "send.args.message",
        ] {
            assert!(
                refusals.contains(&format!("hole `{slot}` is required")),
                "{refusals}"
            );
        }
        assert!(
            !refusals.contains("summarize.schema"),
            "optional: {refusals}"
        );
    }

    #[test]
    fn a_lawful_fill_set_emits_exactly_the_partial_projection_plus_its_values() {
        let sketch = recap();
        let fills = fills_from_json(&json!({"fills": [
            {"task": "open_only", "field": "expression", "value": "fromjson | map(select(.status == \"open\"))"},
            {"task": "summarize", "field": "prompt", "value": "Summarize: ${{ with.tickets }}"},
            {"task": "review", "field": "args.message", "value": "Send this summary?"},
            {"task": "send", "field": "args.target", "value": "http://127.0.0.1:8793/hook"},
            {"task": "send", "field": "args.message", "value": "${{ with.summary }}"}
        ]}))
        .unwrap();
        assert_eq!(
            complete_document(&sketch, &fills).unwrap(),
            document(&sketch, &fills),
            "validation never changes what the assembler emits"
        );
    }

    #[test]
    fn edge_names_are_unique_identifiers_that_never_shadow_a_gate_or_a_loop_binding() {
        let laws = |tasks: serde_json::Value| {
            structural_laws(
                &Sketch::from_json(&json!({"tasks": tasks})).unwrap(),
                "compare the documents in ./docs/*.md, ask me, then write ./out/report.md",
                &[],
            )
            .join("\n")
        };
        let gated = laws(json!([
            {"id": "list", "verb": "invoke", "tool": "nika:glob", "reads": ["./docs/*.md"]},
            {"id": "ask", "verb": "invoke", "tool": "nika:prompt"},
            {"id": "report", "verb": "invoke", "tool": "nika:write", "writes": ["./out/report.md"],
             "gated_by": "ask", "with": [{"name": "approved", "from": "list"}]}
        ]));
        assert!(
            gated.contains("`report` binds the edge name `approved`"),
            "{gated}"
        );
        let looped = laws(json!([
            {"id": "list", "verb": "invoke", "tool": "nika:glob", "reads": ["./docs/*.md"]},
            {"id": "each", "verb": "invoke", "tool": "nika:read", "reads": ["./docs/*.md"],
             "for_each": "list", "with": [{"name": "items", "from": "list"}]}
        ]));
        assert!(
            looped.contains("`each` binds the edge name `items`"),
            "{looped}"
        );
        // Without a gate or a loop the same names are ordinary edges.
        let plain = laws(json!([
            {"id": "list", "verb": "invoke", "tool": "nika:glob", "reads": ["./docs/*.md"]},
            {"id": "report", "verb": "infer", "with": [{"name": "items", "from": "list"}, {"name": "approved", "from": "list"}]}
        ]));
        assert!(!plain.contains("binds the edge name"), "{plain}");
    }

    #[test]
    fn a_malformed_graph_value_is_refused_by_its_path_and_a_legitimate_null_is_empty() {
        let read = |task: serde_json::Value| Sketch::from_json(&json!({"tasks": [task]}));
        let ok = read(
            json!({"id": "a", "verb": "invoke", "tool": "nika:read", "reads": ["./a"],
            "writes": null, "with": null, "gated_by": null, "purpose": null}),
        )
        .unwrap();
        assert!(ok.tasks[0].writes.is_empty() && ok.tasks[0].gated_by.is_none());
        for (task, path) in [
            (
                json!({"id": "a", "verb": "invoke", "reads": ["./a", 1]}),
                "tasks[0].reads[1]",
            ),
            (
                json!({"id": "a", "verb": "invoke", "reads": "./a"}),
                "tasks[0].reads",
            ),
            (
                json!({"id": "a", "verb": "infer", "purpose": 3}),
                "tasks[0].purpose",
            ),
            (
                json!({"id": "a", "verb": "infer", "extra": "x"}),
                "tasks[0]",
            ),
            (
                json!({"id": "a", "verb": "infer", "with": [{"name": "x"}]}),
                "tasks[0].with[0]",
            ),
            (
                json!({"id": "a", "verb": "infer", "with": [7]}),
                "tasks[0].with[0]",
            ),
            (json!("a"), "tasks[0]"),
        ] {
            let error = read(task.clone()).unwrap_err();
            assert!(error.contains(path), "{task}: {error}");
        }
    }

    #[test]
    fn a_whole_args_fill_keeps_another_tools_own_argument_and_never_a_sketch_owned_one() {
        let sketch = Sketch::from_json(&json!({"name": "tools", "tasks": [
            {"id": "notes", "verb": "invoke", "tool": "nika:read", "reads": ["./notes.md"]},
            {"id": "find", "verb": "invoke", "tool": "nika:grep", "reads": ["./notes.md"]},
            {"id": "digest", "verb": "invoke", "tool": "nika:hash", "with": [{"name": "text", "from": "notes"}]},
            {"id": "when", "verb": "invoke", "tool": "nika:date"},
            {"id": "patch", "verb": "invoke", "tool": "nika:edit", "reads": ["./notes.md"], "writes": ["./notes.md"], "with": [{"name": "text", "from": "notes"}]}
        ]}))
        .unwrap();
        let args = |task: &str, value: serde_json::Value| json!({"task": task, "field": "args", "value": value});
        let lawful = fills_from_json(&json!({"fills": [
            args("find", json!({"pattern": "TODO"})),
            args("digest", json!({"content": "${{ with.text }}"})),
            args("when", json!({"input": "2026-10-03"})),
            args("patch", json!({"old_string": "a", "new_string": "b"}))
        ]}))
        .unwrap();
        let doc = complete_document(&sketch, &lawful).unwrap();
        assert_eq!(doc["tasks"]["find"]["invoke"]["args"]["pattern"], "TODO");
        assert_eq!(doc["tasks"]["find"]["invoke"]["args"]["path"], "./notes.md");
        assert_eq!(
            doc["tasks"]["patch"]["invoke"]["args"]["path"],
            "./notes.md"
        );
        let hijack = fills_from_json(&json!({"fills": [
            args("find", json!({"pattern": "TODO", "path": "./other.md"})),
            args("digest", json!({"content": "x"})),
            args("when", json!({"input": "x"})),
            args("patch", json!({"path": "./other.md", "content": "x"}))
        ]}))
        .unwrap();
        let refusals = complete_document(&sketch, &hijack).unwrap_err().join("\n");
        assert!(
            refusals.contains("`find.args` carries `path`"),
            "{refusals}"
        );
        assert!(
            refusals.contains("`patch.args` carries `path`"),
            "{refusals}"
        );
        assert!(
            refusals.contains("`patch.args` carries `content`"),
            "{refusals}"
        );
        assert!(!refusals.contains("digest"), "{refusals}");
        assert!(!refusals.contains("when"), "{refusals}");
        assert!(!refusals.contains("`pattern`"), "{refusals}");
        // A data tool without an edge cannot take its input, or any task's output, by a fill.
        let loose = Sketch::from_json(&json!({"name": "loose", "tasks": [
            {"id": "notes", "verb": "invoke", "tool": "nika:read", "reads": ["./notes.md"]},
            {"id": "shape", "verb": "invoke", "tool": "nika:convert"},
            {"id": "say", "verb": "infer", "with": [{"name": "text", "from": "notes"}]}
        ]}))
        .unwrap();
        let hidden = fills_from_json(&json!({"fills": [
            args("shape", json!({"input": "x", "from": "csv", "to": "json"})),
            {"task": "say", "field": "prompt", "value": "Summarize ${{ tasks.notes.output }}"}
        ]}))
        .unwrap();
        let refusals = complete_document(&loose, &hidden).unwrap_err().join("\n");
        assert!(
            refusals.contains("`shape.args` carries `input`"),
            "{refusals}"
        );
        assert!(
            refusals.contains("`say.prompt` reads another task's output"),
            "{refusals}"
        );
    }

    #[test]
    fn a_placeholder_a_fill_references_is_declared_as_an_empty_const() {
        let sketch = Sketch::from_json(&json!({"name": "lookup", "tasks": [
            {"id": "lookup", "verb": "invoke", "tool": "nika:fetch", "purpose": "the CRM record"}
        ]}))
        .unwrap();
        let fills = fills_from_json(&json!({"fills": [
            {"task": "lookup", "field": "args.url", "value": "${{ const.crm_endpoint }}/contacts"}
        ]}))
        .unwrap();
        let doc = document(&sketch, &fills);
        assert_eq!(doc["const"]["crm_endpoint"], "");
        assert_eq!(
            doc["tasks"]["lookup"]["invoke"]["args"]["url"],
            "${{ const.crm_endpoint }}/contacts"
        );
        assert!(
            doc["permits"].get("net").is_none(),
            "no host is granted before the answer: {doc}"
        );
    }

    #[test]
    fn a_loop_and_a_write_take_their_bindings_from_the_sketch() {
        let sketch = Sketch::from_json(&json!({"name": "docs", "tasks": [
            {"id": "list", "verb": "invoke", "tool": "nika:glob", "reads": ["./docs/*.md"]},
            {"id": "read_each", "verb": "invoke", "tool": "nika:read", "for_each": "list", "reads": ["./docs/*.md"]},
            {"id": "compare", "verb": "infer", "with": [{"name": "texts", "from": "read_each"}]},
            {"id": "report", "verb": "invoke", "tool": "nika:write", "writes": ["./out/report.md"], "with": [{"name": "text", "from": "compare"}], "after": ["compare"]}
        ]}))
        .unwrap();
        let intent = "compare the documents in ./docs/*.md and write ./out/report.md";
        assert!(
            structural_laws(&sketch, intent, &[]).is_empty(),
            "{:?}",
            structural_laws(&sketch, intent, &[])
        );
        let doc = document(&sketch, &[]);
        assert_eq!(
            doc["tasks"]["read_each"]["for_each"]["items"],
            "${{ with.items }}"
        );
        assert_eq!(
            doc["tasks"]["read_each"]["with"]["items"],
            "${{ tasks.list.output }}"
        );
        assert_eq!(
            doc["tasks"]["report"]["invoke"]["args"]["content"],
            "${{ with.text }}"
        );
        assert_eq!(doc["tasks"]["report"]["after"]["compare"], "success");
        assert_eq!(doc["permits"]["fs"]["write"], json!(["./out/report.md"]));
        assert!(
            !holes(&sketch)
                .iter()
                .any(|h| h.task == "report" && h.required),
            "the write's content comes from its edge"
        );
    }
}
