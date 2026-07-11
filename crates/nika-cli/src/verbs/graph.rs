// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE graph projector (spec §6) — `--json` is the canonical
//! projection; mermaid/dot/ASCII derive from it, never from the workflow
//! directly. Versioned envelope (`graph_format: 1`) · nodes topologically
//! sorted (stable order = stable layouts) · `edges.kind` closed enum ·
//! the static graph NEVER carries run state.

use std::fmt::Write as _;

use serde::Serialize;

use nika_schema::check::CheckReport;
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::types::{OnErrorAction, WhenGate};

use crate::verbs::{VerbOutput, load_checked};

/// The versioned projection envelope (spec §6 · `graph_format: 1`).
#[derive(Debug, Serialize)]
pub struct GraphDoc {
    /// Envelope version — additive evolution only.
    pub graph_format: u32,
    /// Workflow id (kebab-case).
    pub workflow: String,
    /// Topologically sorted nodes (wave order · stable).
    pub nodes: Vec<Node>,
    /// `depends_on` edges (the closed kind enum grows with the spec).
    pub edges: Vec<Edge>,
}

/// One task node — static facts only.
#[derive(Debug, Serialize)]
pub struct Node {
    /// Task id.
    pub id: String,
    /// The verb (`infer` · `exec` · `invoke` · `agent`).
    pub verb: &'static str,
    /// `invoke:` tool id, when the verb is invoke.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Resolved `<provider>/<model>` (task override → workflow default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The `when:` gate (`"true"`/`"false"` literal or the CEL island).
    pub when: Option<String>,
    /// `for_each` fan-out, when present.
    pub fan_out: Option<FanOut>,
    /// Per-task permit strings (spec §6 envelope field · always present).
    /// Empty TODAY: the analyzer aggregates effects into the workflow
    /// boundary (`infer_permits`) without exposing per-task attribution —
    /// this fills when that projector ships, the field IS the contract.
    pub permits: Vec<String>,
    /// Static cost interval `[min_path, worst_case]` USD — only for
    /// priced inference tasks (the cost lane's honesty: no price, no
    /// interval).
    pub cost_interval: Option<[f64; 2]>,
    /// `retry.max_attempts`, when a retry policy is declared (spec 05).
    /// One voice: clients read the DECLARED policy here instead of
    /// re-parsing the YAML.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_max_attempts: Option<u32>,
    /// `timeout:` as milliseconds, when declared (spec 03 · the parsed
    /// duration — unambiguous where the source string form is not).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// `on_error:` action — `recover` · `skip` · `fail_workflow` (spec 05).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_error: Option<&'static str>,
    /// Declared `output:` binding names, in source order (spec 04) — what
    /// this task PRODUCES for `${{ tasks.<id>.<name> }}` readers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
}

/// `for_each` fan-out description.
#[derive(Debug, Serialize)]
pub struct FanOut {
    /// `"list"` (literal · known count) or `"expression"` (unknown).
    pub kind: &'static str,
    /// Element count for the literal-list form.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

/// One dependency edge.
#[derive(Debug, Serialize)]
pub struct Edge {
    /// Upstream task id.
    pub from: String,
    /// Downstream task id.
    pub to: String,
    /// Closed enum — `depends_on` today (spec §6).
    pub kind: &'static str,
}

/// Project a checked workflow into the canonical graph document.
///
/// Requires a clean-conformance report: without a valid DAG order there
/// is no topological node list to project.
#[must_use]
pub fn project(wf: &RawWorkflow, report: &CheckReport) -> GraphDoc {
    let workflow = wf
        .workflow
        .as_ref()
        .map_or_else(|| "workflow".to_owned(), |w| w.value.clone());
    let default_model = wf.model.as_ref().map(|m| m.value.clone());

    let mut nodes = Vec::with_capacity(wf.tasks.len());
    for wave in &report.waves {
        for &index in wave {
            let task = &wf.tasks[index].value;
            let (verb, tool, model) = action_facts(&task.action, default_model.as_deref());
            let cost_interval = report
                .cost
                .tasks
                .iter()
                .find(|c| c.task == task.id.value)
                .and_then(|c| c.min_path_usd.zip(c.usd).map(|(min, max)| [min, max]));
            nodes.push(Node {
                id: task.id.value.clone(),
                verb,
                tool,
                model,
                when: task.when.as_ref().map(|w| match &w.value {
                    WhenGate::Literal(b) => b.to_string(),
                    WhenGate::Expr(e) => e.clone(),
                    // #[non_exhaustive] future gate forms name themselves.
                    other => format!("{other:?}"),
                }),
                permits: Vec::new(),
                fan_out: task.for_each.as_ref().map(|f| match &f.value {
                    nika_schema::raw::ForEachValue::List(items) => FanOut {
                        kind: "list",
                        count: items.as_array().map(|a| a.len() as u64),
                    },
                    nika_schema::raw::ForEachValue::Expression(_) => FanOut {
                        kind: "expression",
                        count: None,
                    },
                    other => {
                        // #[non_exhaustive]: refuse silently-wrong output.
                        unreachable!("unknown for_each form: {other:?}")
                    }
                }),
                cost_interval,
                retry_max_attempts: task.retry.as_ref().map(|r| r.value.max_attempts),
                timeout_ms: task
                    .timeout
                    .as_ref()
                    .map(|t| u64::try_from(t.value.as_millis()).unwrap_or(u64::MAX)),
                on_error: task.on_error.as_ref().map(|o| match &o.value.action {
                    OnErrorAction::Recover(_) => "recover",
                    OnErrorAction::Skip => "skip",
                    OnErrorAction::FailWorkflow => "fail_workflow",
                    // #[non_exhaustive]: refuse silently-wrong output (the
                    // for_each precedent — enum and binary ship together).
                    other => unreachable!("unknown on_error action: {other:?}"),
                }),
                outputs: task.output.iter().map(|(k, _)| k.value.clone()).collect(),
            });
        }
    }

    let mut edges = Vec::new();
    for task in &wf.tasks {
        for dep in &task.value.depends_on {
            edges.push(Edge {
                from: dep.value.clone(),
                to: task.value.id.value.clone(),
                kind: "depends_on",
            });
        }
    }
    // Stable edge order: by (from, to) — independent of authoring order.
    // Then dedup: `depends_on: [x, x]` must not lie about cardinality.
    edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    edges.dedup_by(|a, b| a.from == b.from && a.to == b.to);

    GraphDoc {
        graph_format: 1,
        workflow,
        nodes,
        edges,
    }
}

/// (verb, tool, resolved model) for one action.
fn action_facts(
    action: &RawAction,
    default_model: Option<&str>,
) -> (&'static str, Option<String>, Option<String>) {
    match action {
        RawAction::Infer(infer) => (
            "infer",
            None,
            infer
                .model
                .as_ref()
                .map(|m| m.value.clone())
                .or_else(|| default_model.map(str::to_owned)),
        ),
        RawAction::Exec(_) => ("exec", None, None),
        RawAction::Invoke(invoke) => ("invoke", Some(invoke.tool.value.clone()), None),
        RawAction::Agent(agent) => (
            "agent",
            None,
            agent
                .model
                .as_ref()
                .map(|m| m.value.clone())
                .or_else(|| default_model.map(str::to_owned)),
        ),
        // #[non_exhaustive]: a future verb must teach this projector its
        // name before it can be drawn.
        other => unreachable!("unknown verb: {other:?}"),
    }
}

/// Mermaid renderer — derives from the projection, labels carry the verb.
#[must_use]
pub fn to_mermaid(doc: &GraphDoc) -> String {
    let mut out = String::from("graph TD\n");
    for node in &doc.nodes {
        let detail = node
            .tool
            .as_deref()
            .or(node.model.as_deref())
            .map(|d| format!(" · {}", mermaid_escape(d)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  {id}[\"{id} · {verb}{detail}\"]",
            id = node.id,
            verb = node.verb
        );
    }
    for edge in &doc.edges {
        let _ = writeln!(out, "  {} --> {}", edge.from, edge.to);
    }
    out
}

/// Graphviz dot renderer — derives from the projection.
#[must_use]
pub fn to_dot(doc: &GraphDoc) -> String {
    let mut out = format!(
        "digraph \"{}\" {{\n  rankdir=TB;\n  node [shape=box];\n",
        dot_escape(&doc.workflow)
    );
    for node in &doc.nodes {
        let detail = node
            .tool
            .as_deref()
            .or(node.model.as_deref())
            .map(|d| format!("\\n{}", dot_escape(d)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  \"{id}\" [label=\"{id}\\n{verb}{detail}\"];",
            id = node.id,
            verb = node.verb
        );
    }
    for edge in &doc.edges {
        let _ = writeln!(out, "  \"{}\" -> \"{}\";", edge.from, edge.to);
    }
    out.push_str("}\n");
    out
}

/// Escape a label fragment for a Graphviz dot quoted string. A `"` or `\`
/// in the tool/model would otherwise terminate the `label="…"` and emit
/// broken dot (e.g. a templated `model: ${{ vars.m }}` or an MCP tool name).
fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Escape a label fragment for a Mermaid `["…"]` node label. A literal `"`
/// would close the label early (then a stray `]` closes the node); brackets
/// are entity-escaped defensively. Mermaid reads `#NN;`/`#name;` HTML entities.
fn mermaid_escape(s: &str) -> String {
    s.replace('"', "#quot;")
        .replace('[', "#91;")
        .replace(']', "#93;")
}

/// Output format for the `graph` verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    /// The canonical JSON projection (machine surface · never coloured).
    Json,
    /// Mermaid flowchart text.
    Mermaid,
    /// Graphviz dot text.
    Dot,
    /// The terminal drawing (waves as columns · real wires) — falls back
    /// to the wave listing when the one-rail layout cannot be truthful.
    Ascii,
}

/// The `nika graph <file>` verb.
#[must_use]
pub fn run(path: &str, format: GraphFormat) -> VerbOutput {
    let (wf, report) = match load_checked(path) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    if !report.conformance.is_empty() {
        let mut text = String::from("cannot project: no valid DAG order while conformance fails\n");
        for c in &report.conformance {
            let _ = writeln!(text, "  [{}] {}", c.code, c.message);
        }
        return VerbOutput::file(text);
    }
    let doc = project(&wf, &report);
    let text = match format {
        GraphFormat::Json => match serde_json::to_string_pretty(&doc) {
            Ok(json) => json,
            Err(e) => return VerbOutput::env(format!("cannot serialize graph: {e}")),
        },
        GraphFormat::Mermaid => to_mermaid(&doc),
        GraphFormat::Dot => to_dot(&doc),
        GraphFormat::Ascii => to_ascii(&doc, &report),
    };
    VerbOutput::ok(text)
}

/// The terminal drawing — real wires when the layout can be truthful,
/// the wave listing otherwise (never a wrong picture). Plain theme:
/// `graph` is a projector surface, its text is often piped.
fn to_ascii(doc: &GraphDoc, report: &CheckReport) -> String {
    let theme = crate::display::theme::Theme::new(false, false, false);
    if let Some(art) = crate::wires::render(doc, &report.waves, theme) {
        return format!("{art}\n");
    }
    // Honest fallback: one row per wave — order without invented wires.
    let mut text = String::new();
    let mut cursor = 0usize;
    for (i, wave) in report.waves.iter().enumerate() {
        let ids: Vec<&str> = doc.nodes[cursor..cursor + wave.len()]
            .iter()
            .map(|n| n.id.as_str())
            .collect();
        cursor += wave.len();
        let _ = writeln!(text, "  wave {} · {}", i + 1, ids.join(" · "));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn renders(yaml: &str) -> (String, String) {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_schema::check(&wf);
        let doc = project(&wf, &report);
        (to_mermaid(&doc), to_dot(&doc))
    }

    /// A model with chars special to mermaid (`"` `]`) or dot (`"`) must be
    /// ESCAPED — an unescaped quote closed the label early and emitted broken
    /// markup (a templated `model: ${{ vars.m }}` or an MCP tool name hits this
    /// too). Regression: graph used to interpolate the model raw.
    #[test]
    fn special_chars_in_model_are_escaped_in_both_renders() {
        // model value parses to ·  mock/echo"]x
        let (mermaid, dot) = renders(
            "nika: v1\nworkflow: adv\nmodel: \"mock/echo\\\"]x\"\ntasks:\n  - id: a\n    infer: { prompt: \"p\", max_tokens: 5 }\n",
        );
        assert!(
            mermaid.contains("#quot;"),
            "mermaid quote unescaped:\n{mermaid}"
        );
        assert!(
            !mermaid.contains("echo\"]"),
            "raw breaker leaked:\n{mermaid}"
        );
        assert!(dot.contains("echo\\\"]"), "dot quote unescaped:\n{dot}");
    }

    /// Declared policy PROJECTS (one voice — clients stop re-parsing the
    /// YAML): retry budget, timeout in ms, the `on_error` action, and the
    /// declared output names. Undeclared policy stays ABSENT on the wire
    /// (`skip_serializing` — no fake defaults).
    #[test]
    fn declared_policy_projects_and_undeclared_stays_absent() {
        let yaml = "nika: v1\nworkflow: policy\nmodel: mock/echo\ntasks:\n  - id: guarded\n    infer: { prompt: \"p\", max_tokens: 5 }\n    timeout: \"30s\"\n    retry:\n      max_attempts: 3\n    on_error:\n      skip: true\n    output:\n      summary: \".text\"\n      title: \".title\"\n  - id: bare\n    depends_on: [guarded]\n    infer: { prompt: \"q\", max_tokens: 5 }\n";
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_schema::check(&wf);
        let doc = project(&wf, &report);

        let guarded = doc
            .nodes
            .iter()
            .find(|n| n.id == "guarded")
            .expect("guarded projected");
        assert_eq!(guarded.retry_max_attempts, Some(3));
        assert_eq!(guarded.timeout_ms, Some(30_000));
        assert_eq!(guarded.on_error, Some("skip"));
        assert_eq!(guarded.outputs, vec!["summary", "title"]);

        let bare = doc
            .nodes
            .iter()
            .find(|n| n.id == "bare")
            .expect("bare projected");
        assert_eq!(bare.retry_max_attempts, None);
        assert_eq!(bare.timeout_ms, None);
        assert_eq!(bare.on_error, None);
        assert!(bare.outputs.is_empty());

        // The WIRE stays additive: undeclared policy keys are absent.
        let json = serde_json::to_string(&doc).expect("serializes");
        assert!(json.contains("\"retry_max_attempts\":3"));
        assert!(json.contains("\"on_error\":\"skip\""));
        let bare_json = serde_json::to_string(doc.nodes.iter().find(|n| n.id == "bare").unwrap())
            .expect("serializes");
        assert!(!bare_json.contains("retry_max_attempts"));
        assert!(!bare_json.contains("timeout_ms"));
        assert!(!bare_json.contains("on_error"));
        assert!(!bare_json.contains("outputs"));
    }
}
