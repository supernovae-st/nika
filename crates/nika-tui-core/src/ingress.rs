// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The engine ingress — the two doors the session law reads through:
//!
//! - [`GraphDoc`], the engine's canonical projection (`nika inspect
//!   --format json` · `graph_format: 3` · pinned to the engine's own
//!   number by `tests/wire_pins.rs`). Deserialized, never remodeled —
//!   a hand-typed graph type drifts from the binary that emits it (the
//!   2026-08-10 divergence, twice paid).
//! - [`run_from_journal`], the flight recorder's NDJSON folded into the
//!   session's [`Run`]. The fold reads the terminal events only
//!   (`task_completed` · `task_failed` · `task_cancelled`) — a step's
//!   START derives as `completed_ts − duration_ms` (`task_started` stamps
//!   batch-late on parallel waves — measured), and ns timestamps exceed
//!   2^53, so the arithmetic rides u128, never f64. (This line said `i128`
//!   while the code has always used `u128` — a Gate-11 lens read both.)

use serde::Deserialize;

use crate::model::{Failure, Run, Step, Verb};

/// The graph format this crate reads — the engine's (`nika-graph`
/// `GRAPH_FORMAT`), pinned by `tests/wire_pins.rs` so a bump on either
/// side fails there first (ADR-130 · a copied number is pinned, never
/// hand-maintained).
pub const GRAPH_FORMAT: u32 = 3;

/// The versioned projection envelope (`graph_format: 3`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[non_exhaustive]
pub struct GraphDoc {
    /// Envelope version — additive evolution only within 3.
    pub graph_format: u32,
    /// Workflow id (kebab-case).
    pub workflow: String,
    /// Topologically sorted nodes (wave order · stable).
    pub nodes: Vec<Node>,
    /// The typed edges (the closed six-kind enum).
    pub edges: Vec<Edge>,
}

impl GraphDoc {
    /// The constructor the `#[non_exhaustive]` marker requires (invariant
    /// #19): a field added later must not break a caller's build.
    #[must_use]
    pub const fn new(
        graph_format: u32,
        workflow: String,
        nodes: Vec<Node>,
        edges: Vec<Edge>,
    ) -> Self {
        Self {
            graph_format,
            workflow,
            nodes,
            edges,
        }
    }

    /// The same envelope with other nodes — the shape a revision test
    /// reaches for, and the one `..spread` can no longer express.
    #[must_use]
    pub fn with_nodes(&self, nodes: Vec<Node>) -> Self {
        Self {
            graph_format: self.graph_format,
            workflow: self.workflow.clone(),
            nodes,
            edges: self.edges.clone(),
        }
    }

    /// The TASKS — a cleanup unit (`kind: "finally"` · format 3) is a node
    /// the engine projects for the map, never a slot on the board.
    pub fn tasks(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(|n| n.kind != "finally")
    }
}

/// One task node — the engine's static facts, mirrored.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[non_exhaustive]
pub struct Node {
    /// Task id.
    pub id: String,
    /// `"task"` · `"finally"` — the population this node belongs to
    /// (format 3: a cleanup unit is a node, never a task; a reader that did
    /// not know `kind` counted it in a wave). Absent on an older document:
    /// a task.
    #[serde(default = "task_kind")]
    pub kind: String,
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
    /// ⭐ EVERY KEY THIS STRUCT DOES NOT NAME, kept verbatim.
    ///
    /// Without it the type was a sieve: serde drops what is not declared,
    /// so a field the engine adds tomorrow vanished before anything could
    /// see it — and `plan::print_of` promised the exact opposite («a field
    /// the engine adds tomorrow counts as a change without anyone having
    /// to foresee it»). A graph whose only diff was `sandbox: off →
    /// ON-DANGEROUS` marked `Kept`: the board told the operator nothing
    /// changed on the revision where the sandbox came down.
    ///
    /// `BTreeMap` and not a `serde_json::Map`: the fingerprint compares
    /// serialized bytes, so the key order must be the map's, never the
    /// wire's.
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}

impl Node {
    /// The constructor the `#[non_exhaustive]` marker requires (invariant
    /// #19). Only `id` and `verb` are arguments because only those two are
    /// required ON THE WIRE — every other field carries `#[serde(default)]`,
    /// so a thirteen-argument signature would encode a strictness the
    /// format does not have. Fields stay `pub`: set what you mean, leave
    /// the rest.
    #[must_use]
    pub fn new(id: String, verb: Verb) -> Self {
        Self {
            id,
            kind: task_kind(),
            verb,
            tool: None,
            model: None,
            when: None,
            fan_out: None,
            permits: Vec::new(),
            cost_interval: None,
            retry_max_attempts: None,
            timeout_ms: None,
            on_error: None,
            outputs: Vec::new(),
            extra: std::collections::BTreeMap::new(),
        }
    }
}

/// `for_each` fan-out description.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
#[non_exhaustive]
pub struct FanOut {
    /// `"list"` (literal) or `"expression"` (computed at run).
    pub kind: String,
    /// Element count when statically known.
    #[serde(default)]
    pub count: Option<u64>,
}

impl FanOut {
    /// The constructor the `#[non_exhaustive]` marker requires (invariant
    /// #19): a field added later must not break a caller's build. Four of
    /// the eight markers shipped without one — two Gate-11 lenses found the
    /// same gap independently, which is the whole point of running three.
    #[must_use]
    pub const fn new(kind: String, count: Option<u64>) -> Self {
        Self { kind, count }
    }
}

/// One typed edge.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
#[non_exhaustive]
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

impl Edge {
    /// The constructor the `#[non_exhaustive]` marker requires (invariant
    /// #19): a field added later must not break a caller's build. Four of
    /// the eight markers shipped without one — two Gate-11 lenses found the
    /// same gap independently, which is the whole point of running three.
    #[must_use]
    pub const fn new(
        from: String,
        to: String,
        kind: String,
        predicate: Option<String>,
        binding: Option<String>,
    ) -> Self {
        Self {
            from,
            to,
            kind,
            predicate,
            binding,
        }
    }
}

/// The one refusal the ingress owns — the bytes are not a journal this
/// engine wrote.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
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
/// `outcome` field (the journal's one nested stringification). The error
/// object is the engine's own `{code, message, transient}` — the first
/// fold expected a `why` that never exists, and the decode was dead code
/// until the swarm's correctness lens proved it.
#[derive(Deserialize)]
struct Outcome {
    payload: Option<OutcomePayload>,
}

#[derive(Deserialize)]
struct OutcomePayload {
    #[serde(default)]
    error: Option<OutcomeError>,
}

#[derive(Deserialize)]
struct OutcomeError {
    code: String,
    #[serde(default)]
    message: Option<String>,
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
            "task_completed" | "task_failed" | "task_cancelled" | "task_skipped" => {
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
                    let detail = line
                        .field("detail")
                        .and_then(|v| v.as_str())
                        .unwrap_or("failed");
                    let decoded = line
                        .field("outcome")
                        .and_then(|v| v.as_str())
                        .and_then(|s| serde_json::from_str::<Outcome>(s).ok())
                        .and_then(|o| o.payload)
                        .and_then(|p| p.error);
                    let code = decoded.as_ref().map_or_else(
                        || detail.chars().take_while(|c| *c != ' ').collect(),
                        |e| e.code.clone(),
                    );
                    // `why` is the error's clean message (never the doubled
                    // `detail` string, which repeats the code twice).
                    let why = decoded
                        .and_then(|e| e.message)
                        .unwrap_or_else(|| detail.to_owned());
                    Failure { code, why }
                });
                // Cancelled and skipped steps never STARTED — the studio's
                // convention is 0/0 (a cancellation stamps at teardown, and
                // a real timestamp would inflate its wave's end — measured
                // by the swarm on Ctrl-C runs).
                let timeless = matches!(line.kind.as_str(), "task_cancelled" | "task_skipped");
                let (never_born, skipped, blocked_by) = match line.kind.as_str() {
                    "task_cancelled" => (
                        Some(true),
                        None,
                        line.field("blocked_by")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned),
                    ),
                    "task_skipped" => (None, Some(true), None),
                    _ => (None, None, None),
                };
                steps.push(Step {
                    id: id.to_owned(),
                    start: if timeless { 0.0 } else { start.max(0.0) },
                    dur: if timeless { 0.0 } else { duration_ms / 1000.0 },
                    cost,
                    tokens,
                    failed,
                    never_born,
                    blocked_by,
                    skipped,
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

/// The population an older document did not name: a task.
fn task_kind() -> String {
    "task".to_owned()
}
