// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The engine ingress — the two doors the session law reads through:
//!
//! - [`GraphDoc`], the engine's canonical projection (`nika inspect
//!   --format json` · `graph_format: 2`). Deserialized, never remodeled —
//!   a hand-typed graph type drifts from the binary that emits it (the
//!   2026-08-10 divergence, twice paid).
//! - [`run_from_journal`], the flight recorder's NDJSON folded into the
//!   session's [`Run`]. The fold reads the terminal events only
//!   (`task_completed` · `task_failed` · `task_cancelled`) — a step's
//!   START derives as `completed_ts − duration_ms` (`task_started` stamps
//!   batch-late on parallel waves — measured), and ns timestamps exceed
//!   2^53, so the arithmetic rides i128, never f64.

use serde::Deserialize;

use crate::model::{Failure, Run, Step, Verb};

/// The versioned projection envelope (`graph_format: 2`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
pub struct GraphDoc {
    /// Envelope version — additive evolution only within 2.
    pub graph_format: u32,
    /// Workflow id (kebab-case).
    pub workflow: String,
    /// Topologically sorted nodes (wave order · stable).
    pub nodes: Vec<Node>,
    /// The typed edges (the closed six-kind enum).
    pub edges: Vec<Edge>,
}

/// One task node — the engine's static facts, mirrored.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
pub struct Node {
    /// Task id.
    pub id: String,
    /// The verb.
    pub verb: Verb,
    /// `invoke:` tool id, when the verb is invoke.
    #[serde(default)]
    pub tool: Option<String>,
    /// Resolved `<provider>/<model>` (task override → workflow default).
    #[serde(default)]
    pub model: Option<String>,
    /// The `when:` business condition — post-gate, never the gate itself.
    #[serde(default)]
    pub when: Option<String>,
    /// `for_each` fan-out, when present.
    #[serde(default)]
    pub fan_out: Option<FanOut>,
    /// Per-task capability attribution (deterministic, BTree-ordered per
    /// family) — empty for a task with no pinnable effect.
    #[serde(default)]
    pub permits: Vec<String>,
    /// Static cost interval `[min_path, worst_case]` USD — priced
    /// inference only (no price, no interval).
    #[serde(default)]
    pub cost_interval: Option<[f64; 2]>,
    /// `retry.max_attempts`, when declared.
    #[serde(default)]
    pub retry_max_attempts: Option<u32>,
    /// `timeout:` as milliseconds, when declared.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// `on_error:` action — `recover` · `skip` · `fail_workflow`.
    #[serde(default)]
    pub on_error: Option<String>,
    /// Declared `output:` binding names, in source order.
    #[serde(default)]
    pub outputs: Vec<String>,
}

/// `for_each` fan-out description.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
pub struct FanOut {
    /// `"list"` (literal) or `"expression"` (computed at run).
    pub kind: String,
    /// Element count when statically known.
    #[serde(default)]
    pub count: Option<u64>,
}

/// One typed edge.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
pub struct Edge {
    /// Producer task id.
    pub from: String,
    /// Consumer task id.
    pub to: String,
    /// The CLOSED kind enum — `value` · `terminal-observation` ·
    /// `failure-observation` · `control` · `recovery` (· `finally`
    /// reserved).
    pub kind: String,
    /// The `after:` predicate — control edges only.
    #[serde(default)]
    pub predicate: Option<String>,
    /// The `with:` key that created a data/observation edge.
    #[serde(default)]
    pub binding: Option<String>,
}

/// The one refusal the ingress owns — the bytes are not a journal this
/// engine wrote.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IngressError {
    /// A journal line is not valid JSON.
    #[error("line {line}: not JSON — not a journal this engine wrote")]
    NotJson {
        /// The 1-based line number.
        line: usize,
    },
}

/// One journal line — `fields` is the key/value spine; values ride
/// `serde_json::Value` (typed: number · string · bool).
#[derive(Deserialize)]
struct Line {
    kind: String,
    #[serde(default)]
    timestamp: Option<u64>,
    #[serde(default)]
    fields: Vec<Field>,
    #[serde(default)]
    id: Option<LineId>,
}

#[derive(Deserialize)]
struct Field {
    key: String,
    value: serde_json::Value,
}

#[derive(Deserialize)]
struct LineId {
    uuid: String,
}

impl Line {
    fn field(&self, key: &str) -> Option<&serde_json::Value> {
        self.fields.iter().find(|f| f.key == key).map(|f| &f.value)
    }
}

/// The outcome's decoded payload — carried as a JSON STRING inside the
/// `outcome` field (the journal's one nested stringification).
#[derive(Deserialize)]
struct Outcome {
    payload: Option<OutcomePayload>,
}

#[derive(Deserialize)]
struct OutcomePayload {
    #[serde(default)]
    error: Option<Failure>,
}

/// Fold a journal's NDJSON bytes into the session's [`Run`].
///
/// Steps appear in terminal-event order (the journal's own order — the
/// run's truth). A line that does not parse refuses the whole fold: half
/// a journal is no journal.
///
/// # Errors
///
/// [`IngressError::NotJson`] with the 1-based line number — the bytes are
/// not a journal this engine wrote.
pub fn run_from_journal(bytes: &str) -> Result<Run, IngressError> {
    let mut trace = String::new();
    let mut t0_ns: Option<u128> = None;
    let mut steps: Vec<Step> = Vec::new();
    for (i, raw) in bytes.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        let line: Line =
            serde_json::from_str(raw).map_err(|_| IngressError::NotJson { line: i + 1 })?;
        match line.kind.as_str() {
            "workflow_started" => {
                if let Some(id) = &line.id {
                    trace.clone_from(&id.uuid);
                }
                t0_ns = line.timestamp.map(u128::from);
            }
            "task_completed" | "task_failed" | "task_cancelled" => {
                let Some(id) = line.field("task").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let duration_ms = line
                    .field("duration_ms")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                // ns timestamps exceed 2^53 — u128 arithmetic, f64 only at
                // the very end. start = completed_ts − duration_ms, RELATIVE
                // to the run's first event (task_started stamps batch-late
                // on parallel waves — measured).
                #[allow(clippy::cast_precision_loss)]
                // RELATIVE seconds — sub-ms precision is the display's ceiling, never the law's
                let start = match (line.timestamp, t0_ns) {
                    (Some(ts), Some(t0)) => {
                        (u128::from(ts).saturating_sub(t0) as f64 / 1e9) - duration_ms / 1000.0
                    }
                    _ => 0.0,
                };
                let tokens = line.field("tokens").and_then(serde_json::Value::as_u64);
                let cost = line.field("cost_usd").and_then(serde_json::Value::as_f64);
                let failed = (line.kind == "task_failed").then(|| {
                    let why = line
                        .field("detail")
                        .and_then(|v| v.as_str())
                        .unwrap_or("failed")
                        .to_owned();
                    let code = line
                        .field("outcome")
                        .and_then(|v| v.as_str())
                        .and_then(|s| serde_json::from_str::<Outcome>(s).ok())
                        .and_then(|o| o.payload)
                        .and_then(|p| p.error)
                        .map_or_else(
                            || why.chars().take_while(|c| *c != ' ').collect(),
                            |e| e.code,
                        );
                    Failure { code, why }
                });
                let (never_born, blocked_by) = if line.kind == "task_cancelled" {
                    (
                        Some(true),
                        line.field("blocked_by")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned),
                    )
                } else {
                    (None, None)
                };
                steps.push(Step {
                    id: id.to_owned(),
                    start: start.max(0.0),
                    dur: duration_ms / 1000.0,
                    cost,
                    tokens,
                    failed,
                    never_born,
                    blocked_by,
                });
            }
            _ => {}
        }
    }
    Ok(Run {
        trace,
        when: String::new(),
        output: String::new(),
        steps,
    })
}
