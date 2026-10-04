// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The rows a review shows of one `nika check` report, beside the report's other renders
//! (descended from `nika-session`'s change preview, C10): the report's first findings and hints,
//! what the workflow reaches when it runs and the spend it can reach. Pure text over the
//! report, no theme and no I/O: the caller keeps the verdict, the path and every authority.

// Candidate task faces share this pure review owner; consent and file placement stay callers.
use nika_schema::raw::{RawAction, RawInvokeTarget};
use nika_schema::{FileId, ParseMode};

/// What one task does, in the review's words: the verb and the tool or
/// model it names (`default_model` stands in for an `infer` without its
/// own). From the parser, never from prose.
#[must_use]
pub fn task_face(task: &nika_schema::raw::RawTask, default_model: Option<&str>) -> String {
    let what = match &task.action {
        RawAction::Infer(infer) => match infer
            .model
            .as_ref()
            .map(|m| m.value.as_str())
            .or(default_model)
        {
            Some(model) => format!("infer · {model}"),
            None => "infer · (no model named)".to_owned(),
        },
        RawAction::Exec(_) => "exec · runs a program".to_owned(),
        RawAction::Agent(_) => "agent · a bounded multi-turn loop".to_owned(),
        RawAction::Invoke(invoke) => match &invoke.target {
            RawInvokeTarget::Tool(tool) => builtin_face(&tool.value),
            RawInvokeTarget::Workflow(_) => "invoke · another workflow".to_owned(),
        },
        _ => "(a verb this review does not name)".to_owned(),
    };
    let each = if task.for_each.is_some() {
        " · for each item"
    } else {
        ""
    };
    format!("{what}{each}")
}

/// A builtin's face in the review's words: what it does, never its id —
/// `jq`, `glob` or `assert` are machine words to the human who asked for
/// a brief, and `/show` keeps the bytes. A tool this review does not
/// know (an MCP tool, a builtin newer than this list) keeps its id.
fn builtin_face(tool: &str) -> String {
    match tool {
        "nika:read" => "reads a file",
        "nika:write" => "writes a file",
        "nika:edit" => "edits a file",
        "nika:glob" => "lists files",
        "nika:grep" => "searches text",
        "nika:jq" => "shapes the data",
        "nika:json_diff" => "compares data",
        "nika:json_merge_patch" => "merges data",
        "nika:validate" => "validates data",
        "nika:assert" => "checks a condition",
        "nika:decide" => "decides a branch",
        "nika:done" => "marks the work done",
        "nika:prompt" => "asks a human",
        "nika:fetch" => "fetches from the web",
        "nika:notify" => "sends a notification",
        "nika:emit" => "emits an event",
        "nika:log" => "logs a line",
        "nika:wait" => "waits",
        "nika:date" => "reads the clock",
        "nika:uuid" => "makes an id",
        "nika:hash" => "hashes data",
        "nika:convert" => "converts a document",
        "nika:compose" => "composes a document",
        "nika:inspect" => "inspects a workflow",
        "nika:chart" => "draws a chart",
        "nika:image_generate" => "generates an image",
        "nika:image_fx" => "transforms an image",
        "nika:tts_generate" => "speaks text aloud",
        _ => tool,
    }
    .to_owned()
}

/// The candidate's tasks in order — one line each: the id, the verb, the
/// tool or model it names, whether it runs per item. From the parser,
/// never from prose.
#[must_use]
pub fn plan_lines(candidate: &str) -> Vec<String> {
    plan_lines_in_order(candidate, &[])
}

/// [`plan_lines`] in RUN order: the check's waves (indices into the file's
/// tasks) first, then anything the waves left out in file order. The file
/// lists its tasks alphabetically; a human reads what runs first, first.
#[must_use]
pub fn plan_lines_in_order(candidate: &str, waves: &[Vec<usize>]) -> Vec<String> {
    let Ok(wf) = nika_schema::parse(candidate, FileId::new(0), ParseMode::Strict) else {
        return vec!["(the candidate does not parse; the check below says why)".to_owned()];
    };
    let mut order: Vec<usize> = waves
        .iter()
        .flatten()
        .copied()
        .filter(|i| *i < wf.tasks.len())
        .collect();
    for i in 0..wf.tasks.len() {
        if !order.contains(&i) {
            order.push(i);
        }
    }
    let default_model = wf.model.as_ref().map(|m| m.value.clone());
    order
        .iter()
        .enumerate()
        .filter_map(|(n, i)| wf.tasks.get(*i).map(|t| (n, t)))
        .map(|(i, task)| {
            let task = &task.value;
            format!(
                "  {}. {} · {}",
                i + 1,
                task.value_id(),
                task_face(task, default_model.as_deref())
            )
        })
        .collect()
}

trait TaskId {
    fn value_id(&self) -> &str;
}

impl TaskId for nika_schema::raw::RawTask {
    fn value_id(&self) -> &str {
        &self.id.value
    }
}

/// The report's first findings (`code · message`, at most eight) and hints (`kind · advice`, at
/// most four), in the report's order.
#[must_use]
pub fn finding_rows(report: &nika_check::CheckReport) -> (Vec<String>, Vec<String>) {
    let findings = report
        .findings
        .iter()
        .take(8)
        .map(|f| format!("{} · {}", f.code.as_deref().unwrap_or("-"), f.message))
        .collect();
    let hints = report
        .hints
        .iter()
        .take(4)
        .map(|h| format!("{} · {}", h.kind, h.advice))
        .collect();
    (findings, hints)
}

/// What the workflow reaches when it runs, from the report's own
/// permits (needed) and requirements: one row per effect class present.
#[must_use]
pub fn effect_rows(report: &nika_check::CheckReport) -> Vec<String> {
    let mut rows = Vec::new();
    let needed = &report.permits.needed;
    if let Some(fs) = &needed.fs {
        if !fs.read.is_empty() {
            rows.push(format!("reads {}", fs.read.join(" · ")));
        }
        if !fs.write.is_empty() {
            rows.push(format!("writes {}", fs.write.join(" · ")));
        }
    }
    if let Some(net) = &needed.net
        && !net.http.is_empty()
    {
        rows.push(format!("network {}", net.http.join(" · ")));
    }
    match &needed.exec {
        Some(nika_cap::ExecPermit::Any) => rows.push("runs any program".to_owned()),
        Some(nika_cap::ExecPermit::Programs(p)) if !p.is_empty() => {
            rows.push(format!("runs {}", p.join(" · ")));
        }
        _ => {}
    }
    if let Some(tools) = &needed.tools {
        if !tools.is_empty() {
            rows.push(format!("tools {}", tools.join(" · ")));
        }
        if tools.iter().any(|t| t == "nika:prompt") {
            rows.push("pauses for a human answer (`nika:prompt`)".to_owned());
        }
    }
    if let Some(env) = &needed.env
        && !env.is_empty()
    {
        rows.push(format!("environment {}", env.join(" · ")));
    }
    for m in &report.requirements.models {
        rows.push(format!("model {} (tasks {})", m.model, m.tasks.join(" · ")));
    }
    for s in &report.requirements.secrets {
        rows.push(format!(
            "secret {} (key {} · a reference, never a value)",
            s.name, s.key
        ));
    }
    rows.extend(spend_rows(&report.cost));
    rows
}

/// The spend a run can reach, from the check's own cost envelope, never
/// from a total read alone. A workflow with no model call spends nothing
/// on inference. A mock model is a proven zero. A model with no catalog
/// price is its own row: unknown, never counted as free. A missing token
/// bound or an unknown fan-out stays unbounded.
fn spend_rows(cost: &nika_check::CostCeiling) -> Vec<String> {
    if cost.tasks.is_empty() && cost.composed.is_empty() {
        return vec![
            "model output estimate · $0 · no direct model task in these checked bytes".to_owned(),
        ];
    }
    let no_price = cost
        .tasks
        .iter()
        .filter(|t| t.unbounded_reason == Some(nika_check::UnboundedReason::NoPrice))
        .count();
    let unbounded = cost
        .tasks
        .iter()
        .filter(|t| {
            t.usd.is_none() && t.unbounded_reason != Some(nika_check::UnboundedReason::NoPrice)
        })
        .count()
        + cost.composed.iter().filter(|c| c.has_unbounded).count();
    let mut rows = Vec::new();
    if no_price > 0 {
        rows.push(format!(
            "model output estimate · unknown · {no_price} model task(s) with no catalog price — never counted as free"
        ));
    }
    if unbounded > 0 {
        rows.push(
            "model output estimate · unbounded: a token or iteration bound is missing; Run admission is separate"
                .to_owned(),
        );
    }
    if no_price == 0 && unbounded == 0 {
        let all_mock = cost.composed.is_empty()
            && cost.tasks.iter().all(|t| {
                t.model
                    .as_deref()
                    .is_some_and(|m| m == "mock" || m.starts_with("mock/"))
            });
        rows.push(if all_mock {
            "model output estimate · $0 · mock model tasks".to_owned()
        } else {
            format!(
                "model output estimate ≤ ${:.4} at catalog prices · input tokens and other charges excluded",
                cost.bounded_total_usd
            )
        });
    }
    rows
}

#[cfg(test)]
mod tests;
