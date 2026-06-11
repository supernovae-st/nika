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
use nika_schema::types::WhenGate;

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
    /// Static cost interval `[min_path, worst_case]` USD — only for
    /// priced inference tasks (the cost lane's honesty: no price, no
    /// interval).
    pub cost_interval: Option<[f64; 2]>,
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
                .and_then(|c| Some([c.min_path_usd?, c.usd?]));
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
    edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));

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
            .map(|d| format!(" · {d}"))
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
        doc.workflow
    );
    for node in &doc.nodes {
        let detail = node
            .tool
            .as_deref()
            .or(node.model.as_deref())
            .map(|d| format!("\\n{d}"))
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

/// Output format for the `graph` verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    /// The canonical JSON projection (machine surface · never coloured).
    Json,
    /// Mermaid flowchart text.
    Mermaid,
    /// Graphviz dot text.
    Dot,
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
    };
    VerbOutput::ok(text)
}
