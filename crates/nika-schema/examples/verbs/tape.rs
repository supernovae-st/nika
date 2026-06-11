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
/// {save · escalate}) exercising the verb spread + a `when:`-gated
/// skip + a checkpoint. Deterministic: two calls are identical.
pub(crate) fn demo_tape() -> Vec<Event> {
    use EventKind as K;
    vec![
        ev(
            1,
            0,
            K::WorkflowStarted,
            &[("workflow", s("demo-pipeline")), ("tasks", Value::Int(4))],
        ),
        ev(2, 2, K::TaskScheduled, &[("task", s("fetch"))]),
        ev(3, 3, K::TaskStarted, &[("task", s("fetch"))]),
        ev(
            4,
            4,
            K::VerbInvoked,
            &[
                ("task", s("fetch")),
                ("verb", s("exec")),
                ("program", s("curl")),
            ],
        ),
        ev(
            5,
            38,
            K::TaskCompleted,
            &[("task", s("fetch")), ("exit", Value::Int(0))],
        ),
        ev(6, 40, K::TaskScheduled, &[("task", s("extract"))]),
        ev(7, 41, K::TaskStarted, &[("task", s("extract"))]),
        ev(
            8,
            42,
            K::VerbInvoked,
            &[
                ("task", s("extract")),
                ("verb", s("infer")),
                ("model", s("anthropic/claude-sonnet-4-6")),
            ],
        ),
        ev(
            9,
            96,
            K::CheckpointWritten,
            &[("tasks_done", Value::Int(1))],
        ),
        ev(
            10,
            118,
            K::TaskCompleted,
            &[
                ("task", s("extract")),
                ("tokens", Value::Int(412)),
                ("usd", Value::Float(0.0019)),
            ],
        ),
        ev(
            11,
            120,
            K::TaskSkipped,
            &[("task", s("escalate")), ("reason", s("when: false"))],
        ),
        ev(12, 121, K::TaskScheduled, &[("task", s("save"))]),
        ev(13, 122, K::TaskStarted, &[("task", s("save"))]),
        ev(
            14,
            123,
            K::VerbInvoked,
            &[("task", s("save")), ("verb", s("invoke"))],
        ),
        ev(
            15,
            124,
            K::ToolInvoked,
            &[
                ("task", s("save")),
                ("tool", s("nika:write")),
                ("path", s("./out.md")),
            ],
        ),
        ev(
            16,
            131,
            K::TaskCompleted,
            &[("task", s("save")), ("bytes", Value::Int(4200))],
        ),
        ev(
            17,
            133,
            K::WorkflowCompleted,
            &[("tasks", Value::Int(3)), ("usd", Value::Float(0.0019))],
        ),
    ]
}

// ── the fold · TapeState × Event → TapeState ────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowStatus {
    Pending,
    Scheduled,
    Running,
    Done,
    Skipped,
    Failed,
}

/// One task lane in the folded view.
pub(crate) struct Row {
    pub(crate) wave: usize,
    pub(crate) name: &'static str,
    pub(crate) dep: Option<&'static str>,
    pub(crate) verb: Option<VerbKind>,
    pub(crate) status: RowStatus,
    /// The live detail (program · streamed note · tool target).
    pub(crate) note: String,
}

/// The folded run state every renderer reads.
pub(crate) struct TapeState {
    pub(crate) rows: Vec<Row>,
    pub(crate) usd: f64,
    pub(crate) tokens: i64,
    pub(crate) checkpoints: usize,
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
        EventKind::WorkflowFailed => state.terminal = Some(false),
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
            if let Some(Value::Int(tk)) = field(e, "tokens") {
                state.tokens += tk;
            }
            if let Some(Value::Float(u)) = field(e, "usd") {
                state.usd += u;
            }
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
            RowStatus::Done => t.ok(t.glyph(Glyph::Ok)),
            RowStatus::Skipped => t.dim(t.glyph(Glyph::Gated)),
            RowStatus::Failed => t.err(t.glyph(Glyph::Err)),
        };
        let name = format!("{:8}", row.name);
        let name = match row.status {
            RowStatus::Pending | RowStatus::Skipped => t.dim(&name),
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
        let note = if row.note.is_empty() {
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
        "{done}/{total} {} {} tk {} ${:.4} {} ckpt {}",
        t.middot(),
        state.tokens,
        t.middot(),
        state.usd,
        t.middot(),
        state.checkpoints
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
            EventKind::WorkflowFailed,
            EventKind::TaskScheduled,
            EventKind::TaskStarted,
            EventKind::TaskCompleted,
            EventKind::TaskFailed,
            EventKind::TaskSkipped,
            EventKind::VerbInvoked,
            EventKind::ToolInvoked,
            EventKind::CheckpointWritten,
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
    fn tape_line_is_pinned() {
        let t = Theme::new(false, true);
        let tape = demo_tape();
        let line = tape_line(&tape[3], 0, t);
        assert_eq!(
            line,
            " +   4ms ➜ verb_invoked         task=fetch verb=exec program=curl"
        );
    }

    #[test]
    fn final_motion_frame_is_pinned() {
        let t = Theme::new(false, true);
        let f = workflow_frame(total_steps() - 1, t);
        let expected = concat!(
            " ◆ run demo-pipeline · live (folded from the event tape)\n",
            "   w0 ✔ fetch    exec    curl\n",
            "   w1 ✔ extract  infer   ← fetch  anthropic/claude-sonnet-4-6\n",
            "   w2 ✔ save     invoke  ← extract  nika:write ./out.md\n",
            "   w2 ⊘ escalate         ← extract  when: false\n",
            "   ▰▰▰▰▰▰▰▰▰▰ 4/4 · 412 tk · $0.0019 · ckpt 1 ✔ run complete\n",
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
    }
}
