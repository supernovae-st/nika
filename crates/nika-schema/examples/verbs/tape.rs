// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The event tape — REAL engine telemetry, two renderers, one truth.
//!
//! This module grounds the theater in the canonical [`nika_event`]
//! vocabulary (the 11 `EventKind`s the engine actually emits). A demo
//! run is a `Vec<Event>` with deterministic ids and timestamps; from
//! that ONE tape:
//!
//! - `--events` renders the **tape view** — every telemetry event as a
//!   digestible line (relative time · kind glyph · stable wire slug ·
//!   compact fields), then the folded final card.
//! - `verbs workflow` renders the **motion view** — the same tape
//!   FOLDED into an animated DAG: lanes light up wave by wave, the
//!   binding dot travels on dependency rails, cost/token counters tick
//!   live, a progress bar fills.
//!
//! The fold (`TapeState × Event → TapeState`) is the contract's
//! run-card architecture (« the event stream · one truth · N
//! renderers »), reference-implemented. Pure throughout: a frame is
//! `render(fold(tape[..=k]), phase)` — playback is iteration, tests
//! pin frames, reduced motion is the last frame.

use std::fmt::Write as _;

use nika_event::{Event, EventKind};
use nika_types::id::{EventId, RunId};
use nika_types::resource::{KeyValue, Value};
use nika_types::timestamp::Timestamp;
use uuid::Uuid;

use crate::scenes;
use crate::theme::{Glyph, Theme, VerbKind};

/// Motion sub-frames per tape event (spinner/dot/typing advance between
/// state changes — the smoothness knob).
pub(crate) const PHASES: usize = 3;

/// Build one demo event with deterministic id/timestamp (L0 law: the
/// caller supplies time — nothing here reads a clock).
fn ev(seq: u128, at_ms: u64, kind: EventKind, fields: &[(&str, Value)]) -> Event {
    let mut e = Event::new(
        EventId::new(Uuid::from_u128(seq)),
        Timestamp::from_unix_ms(at_ms),
        kind,
    )
    .with_run(RunId::new(Uuid::from_u128(0xDA6)));
    for (k, v) in fields {
        e = e.with_field(KeyValue::new(*k, v.clone()));
    }
    e
}

fn s(v: &str) -> Value {
    Value::String(v.to_owned())
}

/// The canonical demo tape — a 4-task pipeline (fetch → extract →
/// {save · escalate}) exercising the FULL vocabulary: permit gates, a
/// transient-failure retry arc, streaming chunks, incremental cost
/// deltas, a `when:`-gated skip, a checkpoint. Deterministic.
///
/// Cost shape law: `cost_incurred` carries the SPEND (deltas the live
/// meter folds); `task_completed` carries the OUTCOME (exit · bytes ·
/// schema verdict — never re-added spend).
pub(crate) fn demo_tape() -> Vec<Event> {
    let mut tape = fetch_arc();
    tape.extend(extract_arc());
    tape.extend(save_arc());
    tape
}

/// Acts 1: start · the permits gate · fetch with its retry arc.
fn fetch_arc() -> Vec<Event> {
    use EventKind as K;
    let i = Value::Int;
    vec![
        ev(
            1,
            0,
            K::WorkflowStarted,
            &[("workflow", s("demo-pipeline")), ("tasks", i(4))],
        ),
        // the permits gate BEFORE dispatch — the declared boundary, observable
        ev(
            2,
            1,
            K::PermitChecked,
            &[
                ("task", s("fetch")),
                ("gate", s("exec")),
                ("subject", s("curl")),
                ("decision", s("allow")),
            ],
        ),
        ev(3, 2, K::TaskScheduled, &[("task", s("fetch"))]),
        ev(4, 3, K::TaskStarted, &[("task", s("fetch"))]),
        ev(
            5,
            4,
            K::VerbInvoked,
            &[
                ("task", s("fetch")),
                ("verb", s("exec")),
                ("program", s("curl")),
            ],
        ),
        // attempt 1 dies on a transient error → the retry arc (§3.1 ↻)
        ev(
            6,
            21,
            K::TaskRetrying,
            &[
                ("task", s("fetch")),
                ("attempt", i(1)),
                ("max_attempts", i(2)),
                ("reason", s("connection reset")),
            ],
        ),
        ev(7, 24, K::TaskStarted, &[("task", s("fetch"))]),
        ev(
            8,
            38,
            K::TaskCompleted,
            &[("task", s("fetch")), ("exit", i(0))],
        ),
    ]
}

/// Act 2: the streaming infer with live cost deltas.
fn extract_arc() -> Vec<Event> {
    use EventKind as K;
    let i = Value::Int;
    vec![
        ev(9, 40, K::TaskScheduled, &[("task", s("extract"))]),
        ev(10, 41, K::TaskStarted, &[("task", s("extract"))]),
        ev(
            11,
            42,
            K::VerbInvoked,
            &[
                ("task", s("extract")),
                ("verb", s("infer")),
                ("model", s("anthropic/claude-sonnet-4-6")),
            ],
        ),
        // the stream arrives as chunks; the live meter ticks on deltas
        ev(
            12,
            58,
            K::InferChunk,
            &[("task", s("extract")), ("delta", s("\"The arti"))],
        ),
        ev(
            13,
            74,
            K::CostIncurred,
            &[
                ("task", s("extract")),
                ("tokens", i(180)),
                ("usd", Value::Float(0.0008)),
            ],
        ),
        ev(
            14,
            82,
            K::InferChunk,
            &[("task", s("extract")), ("delta", s("cle shows"))],
        ),
        ev(15, 96, K::CheckpointWritten, &[("tasks_done", i(1))]),
        ev(
            16,
            104,
            K::InferChunk,
            &[("task", s("extract")), ("delta", s("...\""))],
        ),
        ev(
            17,
            112,
            K::CostIncurred,
            &[
                ("task", s("extract")),
                ("tokens", i(232)),
                ("usd", Value::Float(0.0011)),
            ],
        ),
        ev(
            18,
            118,
            K::TaskCompleted,
            &[("task", s("extract")), ("schema", s("valid"))],
        ),
    ]
}

/// Act 3: the gated skip · the tool write · the verdict.
fn save_arc() -> Vec<Event> {
    use EventKind as K;
    let i = Value::Int;
    vec![
        ev(
            19,
            120,
            K::TaskSkipped,
            &[("task", s("escalate")), ("reason", s("when: false"))],
        ),
        ev(
            20,
            121,
            K::PermitChecked,
            &[
                ("task", s("save")),
                ("gate", s("tools")),
                ("subject", s("nika:write")),
                ("decision", s("allow")),
            ],
        ),
        ev(21, 121, K::TaskScheduled, &[("task", s("save"))]),
        ev(22, 122, K::TaskStarted, &[("task", s("save"))]),
        ev(
            23,
            123,
            K::VerbInvoked,
            &[("task", s("save")), ("verb", s("invoke"))],
        ),
        ev(
            24,
            124,
            K::ToolInvoked,
            &[
                ("task", s("save")),
                ("tool", s("nika:write")),
                ("path", s("./out.md")),
            ],
        ),
        ev(
            25,
            131,
            K::TaskCompleted,
            &[("task", s("save")), ("bytes", i(4200))],
        ),
        ev(
            26,
            133,
            K::WorkflowCompleted,
            &[("tasks", i(3)), ("usd", Value::Float(0.0019))],
        ),
    ]
}

// ── the fold · TapeState × Event → TapeState ────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowStatus {
    Pending,
    Scheduled,
    Running,
    Retrying,
    Done,
    Skipped,
    Failed,
    Cancelled,
}

/// One task lane in the folded view.
pub(crate) struct Row {
    pub(crate) wave: usize,
    pub(crate) name: &'static str,
    pub(crate) dep: Option<&'static str>,
    pub(crate) verb: Option<VerbKind>,
    pub(crate) status: RowStatus,
    /// The live detail (program · tool target · skip reason).
    pub(crate) note: String,
    /// The accumulated streamed output (`infer_chunk` deltas).
    pub(crate) stream: String,
    /// The transient retry annotation — shown ONLY while retrying (the
    /// verb detail in `note` survives the arc and returns on restart).
    pub(crate) retry_note: String,
}

/// The folded run state every renderer reads.
pub(crate) struct TapeState {
    pub(crate) rows: Vec<Row>,
    pub(crate) usd: f64,
    pub(crate) tokens: i64,
    pub(crate) checkpoints: usize,
    /// `permit_checked` evaluations seen: (allowed, denied).
    pub(crate) permits: (usize, usize),
    pub(crate) terminal: Option<bool>,
}

impl TapeState {
    /// The demo pipeline's static shape (waves + deps known from the
    /// plan — the tape animates status over it).
    pub(crate) fn initial() -> Self {
        let lane = |wave, name, dep| Row {
            wave,
            name,
            dep,
            verb: None,
            status: RowStatus::Pending,
            note: String::new(),
            stream: String::new(),
            retry_note: String::new(),
        };
        Self {
            rows: vec![
                lane(0, "fetch", None),
                lane(1, "extract", Some("fetch")),
                lane(2, "save", Some("extract")),
                lane(2, "escalate", Some("extract")),
            ],
            usd: 0.0,
            tokens: 0,
            checkpoints: 0,
            permits: (0, 0),
            terminal: None,
        }
    }

    fn row_mut(&mut self, task: &str) -> Option<&mut Row> {
        self.rows.iter_mut().find(|r| r.name == task)
    }

    pub(crate) fn done(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| matches!(r.status, RowStatus::Done | RowStatus::Skipped))
            .count()
    }
}

fn field<'e>(e: &'e Event, key: &str) -> Option<&'e Value> {
    e.fields.iter().find(|kv| kv.key == key).map(|kv| &kv.value)
}

fn field_str<'e>(e: &'e Event, key: &str) -> Option<&'e str> {
    match field(e, key) {
        Some(Value::String(v)) => Some(v),
        _ => None,
    }
}

/// Apply one event to the state — the ONE transition function every
/// renderer shares. Total over all kinds (unknown future kinds no-op).
pub(crate) fn fold(state: &mut TapeState, e: &Event) {
    let task = field_str(e, "task").map(str::to_owned);
    match e.kind {
        EventKind::WorkflowCompleted => state.terminal = Some(true),
        // a cancelled run is terminal-not-ok (the verdict line reads
        // the bool; the tape view distinguishes the kinds upstream)
        EventKind::WorkflowFailed | EventKind::WorkflowCancelled => {
            state.terminal = Some(false);
        }
        EventKind::TaskScheduled => {
            if let Some(r) = task.and_then(|t| state.row_mut(&t)) {
                r.status = RowStatus::Scheduled;
            }
        }
        EventKind::TaskStarted => {
            if let Some(r) = task.and_then(|t| state.row_mut(&t)) {
                r.status = RowStatus::Running;
            }
        }
        EventKind::TaskCompleted => {
            // outcome only — spend arrives via `cost_incurred` deltas
            if let Some(r) = task.and_then(|t| state.row_mut(&t)) {
                r.status = RowStatus::Done;
            }
        }
        EventKind::TaskFailed => {
            if let Some(r) = task.and_then(|t| state.row_mut(&t)) {
                r.status = RowStatus::Failed;
            }
        }
        EventKind::TaskSkipped => {
            let reason = field_str(e, "reason").unwrap_or("skipped").to_owned();
            if let Some(r) = task.and_then(|t| state.row_mut(&t)) {
                r.status = RowStatus::Skipped;
                r.note = reason;
            }
        }
        EventKind::VerbInvoked => {
            let verb = match field_str(e, "verb") {
                Some("infer") => Some(VerbKind::Infer),
                Some("exec") => Some(VerbKind::Exec),
                Some("invoke") => Some(VerbKind::Invoke),
                Some("agent") => Some(VerbKind::Agent),
                _ => None,
            };
            let detail = field_str(e, "model")
                .or_else(|| field_str(e, "program"))
                .unwrap_or_default()
                .to_owned();
            if let Some(r) = task.and_then(|t| state.row_mut(&t)) {
                r.verb = verb;
                r.note = detail;
            }
        }
        EventKind::ToolInvoked => {
            let tool = field_str(e, "tool").unwrap_or("tool").to_owned();
            let path = field_str(e, "path").unwrap_or_default().to_owned();
            if let Some(r) = task.and_then(|t| state.row_mut(&t)) {
                r.note = format!("{tool} {path}");
            }
        }
        EventKind::CheckpointWritten => state.checkpoints += 1,
        EventKind::TaskRetrying => {
            let attempt = match field(e, "attempt") {
                Some(Value::Int(n)) => *n,
                _ => 0,
            };
            let max = match field(e, "max_attempts") {
                Some(Value::Int(n)) => *n,
                _ => 0,
            };
            if let Some(r) = task.and_then(|t| state.row_mut(&t)) {
                r.status = RowStatus::Retrying;
                r.retry_note = format!("attempt {attempt}/{max}");
            }
        }
        EventKind::TaskCancelled => {
            if let Some(r) = task.and_then(|t| state.row_mut(&t)) {
                r.status = RowStatus::Cancelled;
            }
        }
        EventKind::CostIncurred => {
            if let Some(Value::Int(tk)) = field(e, "tokens") {
                state.tokens += tk;
            }
            if let Some(Value::Float(u)) = field(e, "usd") {
                state.usd += u;
            }
        }
        EventKind::InferChunk => {
            let delta = field_str(e, "delta").unwrap_or_default().to_owned();
            if let Some(r) = task.and_then(|t| state.row_mut(&t)) {
                r.stream.push_str(&delta);
            }
        }
        EventKind::PermitChecked => {
            if field_str(e, "decision") == Some("deny") {
                state.permits.1 += 1;
            } else {
                state.permits.0 += 1;
            }
        }
        // WorkflowStarted carries no row state; future #[non_exhaustive]
        // kinds fold as a no-op, never a crash.
        _ => {}
    }
}

// ── renderer 1 · the tape view (`--events`) ─────────────────────────

/// The glyph + paint for an event kind (total · future kinds dim).
fn kind_mark(e: &Event, t: Theme) -> String {
    match e.kind {
        EventKind::WorkflowStarted => t.accent(t.glyph(Glyph::Banner)),
        EventKind::WorkflowCompleted => t.verdict_ok(t.glyph(Glyph::Ok)),
        EventKind::WorkflowFailed => t.verdict_err(t.glyph(Glyph::Err)),
        EventKind::TaskStarted => t.accent(t.glyph(Glyph::Pending)),
        EventKind::TaskCompleted => t.ok(t.glyph(Glyph::Ok)),
        EventKind::TaskFailed => t.err(t.glyph(Glyph::Err)),
        EventKind::TaskSkipped => t.dim(t.glyph(Glyph::Gated)),
        EventKind::VerbInvoked => match field_str(e, "verb") {
            Some("infer") => t.verb(VerbKind::Infer, t.glyph(Glyph::Hint)),
            Some("exec") => t.verb(VerbKind::Exec, t.glyph(Glyph::Hint)),
            Some("agent") => t.verb(VerbKind::Agent, t.glyph(Glyph::Hint)),
            _ => t.verb(VerbKind::Invoke, t.glyph(Glyph::Hint)),
        },
        EventKind::ToolInvoked => t.verb(VerbKind::Invoke, t.glyph(Glyph::Hint)),
        EventKind::CheckpointWritten => t.dim(t.glyph(Glyph::Fix)),
        EventKind::TaskRetrying => t.warn(t.glyph(Glyph::Retry)),
        EventKind::TaskCancelled | EventKind::WorkflowCancelled => t.dim(t.glyph(Glyph::Cancelled)),
        // spend wears the COST family (magenta — the verb-gate logic)
        EventKind::CostIncurred => t.verb(VerbKind::Infer, "$"),
        EventKind::InferChunk => t.verb(VerbKind::Infer, t.glyph(Glyph::Hint)),
        // the security gate: green allow · red deny — auditable at a glance
        EventKind::PermitChecked => {
            if field_str(e, "decision") == Some("deny") {
                t.err(t.glyph(Glyph::Err))
            } else {
                t.ok(t.glyph(Glyph::Ok))
            }
        }
        // TaskScheduled + future kinds: the quiet pending mark.
        _ => t.dim(t.glyph(Glyph::Pending)),
    }
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => format!("{f}"),
        _ => String::from("?"),
    }
}

/// One tape line: `  +  4ms ➜ verb_invoked   task=fetch verb=exec …`.
pub(crate) fn tape_line(e: &Event, t0_ms: i64, t: Theme) -> String {
    let rel = e.timestamp.unix_ms().saturating_sub(t0_ms);
    let fields: Vec<String> = e
        .fields
        .iter()
        .map(|kv| format!("{}={}", kv.key, value_text(&kv.value)))
        .collect();
    format!(
        " {} {} {:20} {}",
        t.dim(&format!("+{rel:>4}ms")),
        kind_mark(e, t),
        e.kind.as_str(),
        t.dim(&fields.join(" "))
    )
}

/// The full tape view: every event, then the folded final card.
pub(crate) fn render_tape(t: Theme) -> String {
    let tape = demo_tape();
    let t0: i64 = tape.first().map_or(0, |e| e.timestamp.unix_ms());
    let mut out = String::new();
    let _ = writeln!(
        out,
        " {} {} {}",
        t.accent(t.glyph(Glyph::Banner)),
        t.bold("event tape"),
        t.dim(&format!(
            "{} {} events {} one truth, two renderers",
            t.middot(),
            tape.len(),
            t.middot()
        ))
    );
    for e in &tape {
        let _ = writeln!(out, "{}", tape_line(e, t0, t));
    }
    let bar = if t.unicode_glyphs() { "┄" } else { "-" };
    let _ = writeln!(out, "{}", t.dim(&bar.repeat(46)));
    out.push_str(&workflow_frame(total_steps() - 1, t));
    out
}

/// Renderer 3 — the machine wire: the SAME tape as NDJSON, one event
/// per line, serde-verbatim (the contract's `--json` surface: what a
/// real `nika run --json` streams to CI/agents). Never coloured; the
/// contract bytes are the contract.
pub(crate) fn render_ndjson() -> Result<String, serde_json::Error> {
    let mut out = String::new();
    for e in demo_tape() {
        out.push_str(&serde_json::to_string(&e)?);
        out.push('\n');
    }
    Ok(out)
}

// ── renderer 2 · the motion view (`verbs workflow`) ─────────────────

/// Total animation steps (each event expands into PHASES sub-frames).
pub(crate) fn total_steps() -> usize {
    demo_tape().len() * PHASES
}

/// A live progress bar `▰▰▰▱▱` (`===--` in ascii), done/total lanes.
fn progress(done: usize, total: usize, width: usize, t: Theme) -> String {
    let filled = (done * width).div_ceil(total.max(1)).min(width);
    let (on, off) = if t.unicode_glyphs() {
        ("▰", "▱")
    } else {
        ("=", "-")
    };
    format!(
        "{}{}",
        t.ok(&on.repeat(filled)),
        t.dim(&off.repeat(width - filled))
    )
}

/// Render one motion frame: fold the tape prefix, then draw the DAG
/// lanes + live counters. Pure in (step, theme).
pub(crate) fn workflow_frame(step: usize, t: Theme) -> String {
    let tape = demo_tape();
    let upto = (step / PHASES).min(tape.len() - 1);
    let mut state = TapeState::initial();
    for e in &tape[..=upto] {
        fold(&mut state, e);
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        " {} {} {}",
        t.accent(t.glyph(Glyph::Banner)),
        t.bold("run demo-pipeline"),
        t.dim(&format!("{} live (folded from the event tape)", t.middot()))
    );

    for row in &state.rows {
        let glyph = match row.status {
            RowStatus::Pending => t.dim(t.glyph(Glyph::Pending)),
            RowStatus::Scheduled => t.accent(t.glyph(Glyph::Pending)),
            RowStatus::Running => scenes::spin(step, t),
            RowStatus::Retrying => t.warn(t.glyph(Glyph::Retry)),
            RowStatus::Done => t.ok(t.glyph(Glyph::Ok)),
            RowStatus::Skipped => t.dim(t.glyph(Glyph::Gated)),
            RowStatus::Failed => t.err(t.glyph(Glyph::Err)),
            RowStatus::Cancelled => t.dim(t.glyph(Glyph::Cancelled)),
        };
        let name = format!("{:8}", row.name);
        let name = match row.status {
            RowStatus::Pending | RowStatus::Skipped | RowStatus::Cancelled => t.dim(&name),
            _ => name,
        };
        let verb = row.verb.map_or_else(
            || t.dim(&format!("{:6}", "")),
            |k| t.verb(k, &format!("{:6}", k.name())),
        );
        // the dependency rail: while running, the upstream value
        // TRAVELS in (the dot advances with the global step)
        let dep = match (&row.dep, row.status) {
            (Some(d), RowStatus::Running) => format!(
                "  {} {}",
                scenes::rail(step % 4, 4, t),
                t.dim(&format!("{d}.output"))
            ),
            (Some(d), _) => t.dim(&format!("  {} {d}", t.glyph(Glyph::Dep))),
            (None, _) => String::new(),
        };
        // detail priority: the transient retry annotation while
        // retrying · then the live stream (the run is TALKING) · then
        // the static verb detail
        let note = if row.status == RowStatus::Retrying {
            format!("  {}", t.warn(&row.retry_note))
        } else if !row.stream.is_empty() {
            format!("  {}", t.dim(&row.stream))
        } else if row.note.is_empty() {
            String::new()
        } else {
            format!("  {}", t.dim(&row.note))
        };
        let _ = writeln!(
            out,
            "   {} {} {name} {verb}{dep}{note}",
            t.dim(&format!("w{}", row.wave)),
            glyph
        );
    }

    // live footer: progress + counters tick as the tape folds
    let done = state.done();
    let total = state.rows.len();
    let counters = format!(
        "{done}/{total} {} {} tk {} ${:.4} {} ckpt {} {} permits {}",
        t.middot(),
        state.tokens,
        t.middot(),
        state.usd,
        t.middot(),
        state.checkpoints,
        t.middot(),
        state.permits.0
    );
    let closing = match state.terminal {
        Some(true) => t.verdict_ok(&format!("{} run complete", t.glyph(Glyph::Ok))),
        Some(false) => t.verdict_err(&format!("{} run failed", t.glyph(Glyph::Err))),
        None => t.dim("running"),
    };
    let _ = writeln!(
        out,
        "   {} {} {closing}",
        progress(done, total, 10, t),
        t.dim(&counters)
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tape_is_deterministic() {
        assert_eq!(demo_tape(), demo_tape());
    }

    #[test]
    fn fold_is_total_over_every_event_kind() {
        // every canonical kind folds without panicking — including the
        // failure kinds the happy demo tape never emits.
        let kinds = [
            EventKind::WorkflowStarted,
            EventKind::WorkflowCompleted,
            EventKind::WorkflowCancelled,
            EventKind::TaskScheduled,
            EventKind::TaskStarted,
            EventKind::TaskCompleted,
            EventKind::TaskFailed,
            EventKind::TaskSkipped,
            EventKind::TaskRetrying,
            EventKind::TaskCancelled,
            EventKind::VerbInvoked,
            EventKind::ToolInvoked,
            EventKind::CheckpointWritten,
            EventKind::CostIncurred,
            EventKind::InferChunk,
            EventKind::PermitChecked,
            EventKind::WorkflowFailed,
        ];
        let mut state = TapeState::initial();
        for (i, kind) in kinds.into_iter().enumerate() {
            let e = ev(100 + i as u128, i as u64, kind, &[("task", s("fetch"))]);
            fold(&mut state, &e);
        }
        assert_eq!(
            state.terminal,
            Some(false),
            "WorkflowFailed folded last-wins"
        );
    }

    #[test]
    fn tape_lines_are_pinned() {
        let t = Theme::new(false, true);
        let tape = demo_tape();
        // the dispatch line (now at index 4 — the permit gate precedes it)
        assert_eq!(
            tape_line(&tape[4], 0, t),
            " +   4ms ➜ verb_invoked         task=fetch verb=exec program=curl"
        );
        // the security gate — green ✔, gate + subject + decision visible
        assert_eq!(
            tape_line(&tape[1], 0, t),
            " +   1ms ✔ permit_checked       task=fetch gate=exec subject=curl decision=allow"
        );
        // the retry arc — yellow ↻ with the attempt counter
        assert_eq!(
            tape_line(&tape[5], 0, t),
            " +  21ms ↻ task_retrying        task=fetch attempt=1 max_attempts=2 reason=connection reset"
        );
        // the spend delta — magenta $ (the COST verb-gate family)
        assert_eq!(
            tape_line(&tape[12], 0, t),
            " +  74ms $ cost_incurred        task=extract tokens=180 usd=0.0008"
        );
    }

    #[test]
    fn final_motion_frame_is_pinned() {
        let t = Theme::new(false, true);
        let f = workflow_frame(total_steps() - 1, t);
        let expected = concat!(
            " ◆ run demo-pipeline · live (folded from the event tape)\n",
            "   w0 ✔ fetch    exec    curl\n",
            "   w1 ✔ extract  infer   ← fetch  \"The article shows...\"\n",
            "   w2 ✔ save     invoke  ← extract  nika:write ./out.md\n",
            "   w2 ⊘ escalate         ← extract  when: false\n",
            "   ▰▰▰▰▰▰▰▰▰▰ 4/4 · 412 tk · $0.0019 · ckpt 1 · permits 2 ✔ run complete\n",
        );
        assert_eq!(f, expected);
    }

    #[test]
    fn ascii_tape_and_motion_are_pure_ascii() {
        let t = Theme::new(false, false);
        assert!(render_tape(t).is_ascii());
        assert!(workflow_frame(total_steps() - 1, t).is_ascii());
        assert!(workflow_frame(7, t).is_ascii());
    }

    #[test]
    fn counters_tick_as_the_tape_folds() {
        let t = Theme::new(false, true);
        let early = workflow_frame(0, t);
        let late = workflow_frame(total_steps() - 1, t);
        assert!(early.contains("0 tk"), "{early}");
        assert!(late.contains("412 tk"), "{late}");
        assert!(early.contains("running"));
        assert!(late.contains("run complete"));
        // the meter ticks MID-RUN (the first cost delta lands before the
        // second): some frame shows the partial spend — the live ~$ law.
        let mid = (0..total_steps())
            .map(|n| workflow_frame(n, t))
            .find(|f| f.contains("180 tk"));
        assert!(mid.is_some(), "no frame showed the first cost delta");
        // and the stream is visible WHILE extract runs (the run talks)
        let talking = (0..total_steps())
            .map(|n| workflow_frame(n, t))
            .find(|f| f.contains("\"The arti") && f.contains("1/4"));
        assert!(talking.is_some(), "no frame showed the live stream");
    }

    #[test]
    fn ndjson_is_the_wire_format_verbatim() {
        let nd = render_ndjson().expect("serializes");
        let lines: Vec<&str> = nd.lines().collect();
        assert_eq!(lines.len(), demo_tape().len(), "one event per line");
        // every line parses back and carries the snake_case kind slug
        for (line, e) in lines.iter().zip(demo_tape()) {
            let v: serde_json::Value = serde_json::from_str(line).expect("parses");
            assert_eq!(
                v.get("kind").and_then(|k| k.as_str()),
                Some(e.kind.as_str()),
                "kind slug mismatch on {line}"
            );
        }
        // the first line, byte-exact (the wire is a contract) — pinned
        // from reality: ids nest as {"uuid": …} (the newtype's named
        // field) and Value serializes untagged (bare string/int).
        assert_eq!(
            lines[0],
            concat!(
                "{\"id\":{\"uuid\":\"00000000-0000-0000-0000-000000000001\"},",
                "\"timestamp\":0,\"kind\":\"workflow_started\",",
                "\"run\":{\"uuid\":\"00000000-0000-0000-0000-000000000da6\"},",
                "\"correlation\":null,",
                "\"fields\":[{\"key\":\"workflow\",\"value\":\"demo-pipeline\"},",
                "{\"key\":\"tasks\",\"value\":4}]}"
            )
        );
    }

    /// THE telemetry correspondence (SOTA law): every UI state of the
    /// contract §3.1 state machine is REACHABLE from canonical events —
    /// the vocabulary covers the display, pinned.
    #[test]
    fn every_row_status_is_event_reachable() {
        use EventKind as K;
        let drive = |kinds: &[K]| {
            let mut st = TapeState::initial();
            for (n, k) in kinds.iter().enumerate() {
                let e = ev(900 + n as u128, n as u64, *k, &[("task", s("fetch"))]);
                fold(&mut st, &e);
            }
            st.rows[0].status
        };
        assert!(drive(&[]) == RowStatus::Pending);
        assert!(drive(&[K::TaskScheduled]) == RowStatus::Scheduled);
        assert!(drive(&[K::TaskStarted]) == RowStatus::Running);
        assert!(drive(&[K::TaskStarted, K::TaskRetrying]) == RowStatus::Retrying);
        assert!(drive(&[K::TaskStarted, K::TaskCompleted]) == RowStatus::Done);
        assert!(drive(&[K::TaskSkipped]) == RowStatus::Skipped);
        assert!(drive(&[K::TaskStarted, K::TaskFailed]) == RowStatus::Failed);
        assert!(drive(&[K::TaskStarted, K::TaskCancelled]) == RowStatus::Cancelled);
    }
}
