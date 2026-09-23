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

/// A sketch: a name and its tasks in order (a task references only earlier ones).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sketch {
    pub name: String,
    pub tasks: Vec<SketchTask>,
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

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn text(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or_default().to_owned()
}

impl Sketch {
    /// A sketch read from the seat's JSON (`{name, tasks: [{id, verb, tool?, reads?, writes?,
    /// hosts?, after?, with?: [{name, from}], gated_by?, for_each?, purpose?}]}`).
    ///
    /// # Errors
    /// The path and reason the record cannot be read: nothing is guessed.
    pub fn from_json(record: &Value) -> Result<Self, String> {
        let name = text(record.get("name"));
        let tasks = record
            .get("tasks")
            .and_then(Value::as_array)
            .ok_or_else(|| "`tasks` is missing or not an array".to_owned())?;
        let mut out = Vec::new();
        for (k, task) in tasks.iter().enumerate() {
            let id = text(task.get("id"));
            let verb = Verb::from_word(&text(task.get("verb")))
                .ok_or_else(|| format!("tasks[{k}].verb is not infer|invoke|exec|agent"))?;
            let with = task
                .get("with")
                .and_then(Value::as_array)
                .map(|edges| {
                    edges
                        .iter()
                        .map(|e| Edge {
                            name: text(e.get("name")),
                            from: text(e.get("from")),
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.push(SketchTask {
                id,
                verb,
                tool: task.get("tool").and_then(Value::as_str).map(str::to_owned),
                reads: strings(task.get("reads")),
                writes: strings(task.get("writes")),
                hosts: strings(task.get("hosts")),
                after: strings(task.get("after")),
                with,
                gated_by: task
                    .get("gated_by")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                for_each: task
                    .get("for_each")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                purpose: text(task.get("purpose")),
            });
        }
        Ok(Self {
            name: if name.is_empty() {
                "sketch".to_owned()
            } else {
                name
            },
            tasks: out,
        })
    }
}

/// The structural laws over a sketch: ids, references, the gate, the verbs' tools, every
/// literal stated. Each refusal names the task and the fix; an empty list admits the sketch.
#[must_use]
pub fn structural_laws(sketch: &Sketch, intent: &str, allowed: &[String]) -> Vec<String> {
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
        for (what, literals) in [
            ("path", &task.reads),
            ("path", &task.writes),
            ("host", &task.hosts),
        ] {
            for literal in literals {
                if !stated(literal) {
                    out.push(format!(
                        "`{id}` reaches the {what} `{literal}`, which the request never states: only a stated literal or an answered value enters a sketch"
                    ));
                }
            }
        }
        seen.push(id);
    }
    out
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
            (Verb::Invoke, Some("nika:write")) => {
                hole(task, "args.content", "template", task.with.is_empty());
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

/// The fills read from the seat's JSON (`{fills: [{task, field, value}]}`).
///
/// # Errors
/// The fill that is not one.
pub fn fills_from_json(record: &Value) -> Result<Vec<Fill>, String> {
    record
        .get("fills")
        .and_then(Value::as_array)
        .ok_or_else(|| "`fills` is missing or not an array".to_owned())?
        .iter()
        .enumerate()
        .map(|(k, f)| {
            let task = text(f.get("task"));
            let field = text(f.get("field"));
            if task.is_empty() || field.is_empty() {
                return Err(format!("fills[{k}] names no task or no field"));
            }
            Ok(Fill {
                task,
                field,
                value: f.get("value").cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
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
            json!({"items": "${{ with.items }}", "fail_fast": false}),
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
            let mut infer =
                json!({"prompt": fill("prompt").unwrap_or_else(|| json!("")), "max_tokens": 800});
            if let Some(schema) = fill("schema") {
                infer["schema"] = schema;
            }
            json!({"infer": infer})
        }
        (Verb::Agent, _) => {
            reach.needs_model = true;
            json!({"agent": {"prompt": fill("prompt").unwrap_or_else(|| json!("")), "max_turns": 4, "tools": []}})
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
    if let Some(last) = sketch.tasks.last() {
        root.insert("outputs".to_owned(), json!({"result": output_of(&last.id)}));
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
/// the input it reads by its first edge, the webhook channel, the article mode.
fn default_args(task: &SketchTask, tool: &str) -> Map<String, Value> {
    let first_edge = task
        .with
        .first()
        .map(|e| json!(format!("${{{{ with.{} }}}}", e.name)));
    let path = |list: &[String]| list.first().map(|p| json!(p));
    let (key, value) = match tool {
        "nika:read" | "nika:grep" => ("path", path(&task.reads)),
        "nika:glob" => ("pattern", path(&task.reads)),
        "nika:write" | "nika:edit" => ("path", path(&task.writes)),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use serde_json::json;

    use super::{Sketch, document, fills_from_json, holes, structural_laws};

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
        assert_eq!(doc["tasks"]["summarize"]["infer"]["max_tokens"], 800);
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
        assert!(
            structural_laws(
                &Sketch {
                    name: "e".into(),
                    tasks: vec![]
                },
                INTENT,
                &[]
            )[0]
            .contains("no task")
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
