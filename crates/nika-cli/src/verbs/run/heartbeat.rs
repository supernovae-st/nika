// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The plain-lane liveness rider (#321): while a task is in flight the
//! run speaks on STDERR every ~10s — `still running · <task> · <n>s ·
//! <model>` — so a piped local-model run never reads as a hang. A
//! rider, never a surface: stdout stays the storyboard's, `--json`
//! keeps NDJSON (it already streams), and the rider can only ever ADD
//! stderr lines, never change an exit code.
//!
//! WHY the pulse is plan-driven, not event-driven (proven live on a 12s
//! `exec` task): the runtime's single-sink fold law emits each task's
//! whole story (`task_started … task_completed`) AT SETTLE — mid-flight
//! the stream is structurally mute, so an in-flight registry fed by
//! `task_started` never beats. What IS known mid-flight: the static
//! wave plan (the scheduler's truth · injected before driving), the
//! terminal frames as they land, and the workflow's own verb/model
//! declarations. The current wave = the first with an unsettled member;
//! its unsettled members are the in-flight set (wave members dispatch
//! together — the elapsed clock opens with the wave).

// The heartbeat IS a stderr surface (the same sanctioned exemption the
// run module carries): liveness cannot be deferred to a `VerbOutput`.
#![allow(clippy::disallowed_macros, clippy::print_stderr)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nika_event::{Event, EventKind};
use nika_runtime::EventSink;
use nika_schema::raw::{RawAction, RawWorkflow};

use crate::display::state::str_field;

/// The heartbeat cadence (~10s per #321). Beats fire at run-global
/// ticks for whatever is in flight past a 1s floor — a per-task
/// `>= PERIOD` floor would starve the exact population the beat exists
/// for (a 12s task straddles ONE tick and settles before the next).
const PERIOD: Duration = Duration::from_secs(10);

/// The pulse: everything the ticker needs to say WHO is running.
pub(super) struct RunPulse {
    /// The static wave plan (task ids · dispatch order) — the
    /// scheduler's truth, the same source the ∥ lane markers speak.
    plan: Vec<Vec<String>>,
    /// Per-task labels in the started-note vocabulary (`infer ·
    /// <model>` · `invoke · <tool>` · `exec`) — derived STATICALLY from
    /// the workflow (the stream cannot say it mid-flight).
    labels: BTreeMap<String, String>,
    /// Terminal frames folded live (the one mid-run signal the single-
    /// sink fold law delivers — each settle arrives when it happens).
    settled: BTreeSet<String>,
    /// When the CURRENT wave opened (members dispatch together; a
    /// member settling leaves the others' clock untouched).
    wave_opened: Instant,
    /// The current wave index (first wave with an unsettled member).
    wave: usize,
}

impl RunPulse {
    /// The beat lines for this instant: every unsettled member of the
    /// current wave in flight past the 1s floor (a sub-second overlap
    /// with a tick is noise, not a hang).
    fn beats(&self) -> Vec<String> {
        let secs = self.wave_opened.elapsed().as_secs();
        if secs < 1 {
            return Vec::new();
        }
        let Some(wave) = self.plan.get(self.wave) else {
            return Vec::new();
        };
        wave.iter()
            .filter(|id| !self.settled.contains(*id))
            .map(|id| heartbeat_line(id, secs, self.labels.get(id).map(String::as_str)))
            .collect()
    }

    /// Fold one settle: mark the task, and when that CLOSES the current
    /// wave, open the next unsettled one (its members dispatch now —
    /// the elapsed clock restarts with them).
    fn settle(&mut self, task: &str) {
        self.settled.insert(task.to_owned());
        let next = self
            .plan
            .iter()
            .position(|wave| wave.iter().any(|id| !self.settled.contains(id)))
            .unwrap_or(self.plan.len());
        if next != self.wave {
            self.wave = next;
            self.wave_opened = Instant::now();
        }
    }
}

/// A fresh shared pulse (the sink folds settles · the ticker reads) —
/// built at composition time, moments before wave 1 dispatches.
pub(super) fn shared(
    plan: Vec<Vec<String>>,
    labels: BTreeMap<String, String>,
) -> Arc<Mutex<RunPulse>> {
    Arc::new(Mutex::new(RunPulse {
        plan,
        labels,
        settled: BTreeSet::new(),
        wave_opened: Instant::now(),
        wave: 0,
    }))
}

/// Per-task labels in the runtime's started-note vocabulary, derived
/// statically: `infer`/`agent` name the model they WILL resolve (the
/// task's own `model:` · else the effective default — a `--model`
/// override already substituted by the caller); `invoke` names its
/// tool; `exec` stays bare (its argv0 may be template-shaped until run
/// time — never invent).
pub(super) fn task_labels(wf: &RawWorkflow, default_model: &str) -> BTreeMap<String, String> {
    wf.tasks
        .iter()
        .map(|t| {
            let task = &t.value;
            let label = match &task.action {
                RawAction::Infer(a) => model_label(
                    "infer",
                    a.model.as_ref().map(|m| m.value.as_str()),
                    default_model,
                ),
                RawAction::Agent(a) => model_label(
                    "agent",
                    a.model.as_ref().map(|m| m.value.as_str()),
                    default_model,
                ),
                RawAction::Invoke(a) => match &a.target {
                    nika_schema::raw::RawInvokeTarget::Tool(t) => {
                        format!("invoke · {}", t.value)
                    }
                    nika_schema::raw::RawInvokeTarget::Workflow(w) => {
                        format!("invoke · workflow:{}", w.value)
                    }
                },
                RawAction::Exec(_) => "exec".to_owned(),
                // `#[non_exhaustive]` — a FUTURE verb speaks its verb
                // name (honest, never invented detail).
                other => other.verb().to_owned(),
            };
            (task.id.value.clone(), label)
        })
        .collect()
}

/// `<verb> · <model>` — the task's own model wins, else the effective
/// default; a modelless resolve stays bare (never an invented cell).
fn model_label(verb: &str, task_model: Option<&str>, default_model: &str) -> String {
    let model = task_model.unwrap_or(default_model);
    if model.is_empty() {
        verb.to_owned()
    } else {
        format!("{verb} · {model}")
    }
}

/// The stream-side half: an [`EventSink`] rider folding terminal frames
/// into the pulse. An inert handle (`None`) keeps the caller's wiring
/// branch-free — the `TraceFileSink::disabled` idiom.
pub(super) struct HeartbeatSink {
    pulse: Option<Arc<Mutex<RunPulse>>>,
}

impl HeartbeatSink {
    /// Wrap the shared pulse (`None` = permanently silent no-op).
    pub(super) fn new(pulse: Option<Arc<Mutex<RunPulse>>>) -> Self {
        Self { pulse }
    }
}

impl EventSink for HeartbeatSink {
    fn emit(&mut self, event: Event) {
        let Some(pulse) = &self.pulse else { return };
        // Terminal kinds ONLY — the fold law delivers these live; the
        // started/retrying/recovered story arrives at settle anyway.
        if !matches!(
            event.kind,
            EventKind::TaskCompleted
                | EventKind::TaskFailed
                | EventKind::TaskSkipped
                | EventKind::TaskCancelled
                | EventKind::TaskCacheHit
        ) {
            return;
        }
        let Some(task) = str_field(&event, "task") else {
            return; // a terminal frame without a task settles nothing
        };
        // A poisoned lock = another holder panicked; the heartbeat is
        // best-effort by contract — go silent, never propagate.
        if let Ok(mut pulse) = pulse.lock() {
            pulse.settle(task);
        }
    }
}

/// The timer half — spawned on the run's current-thread executor (it
/// ticks whenever the driven future is at an await point, exactly the
/// provider/subprocess wait the hang perception comes from). The caller
/// aborts it the moment the run settles (and the executor's drop reaps
/// it regardless — it can never outlive the run).
pub(super) fn spawn_ticker(pulse: Arc<Mutex<RunPulse>>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(PERIOD);
        // A long blocking stretch must not burst-fire stale beats.
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tick.tick().await; // the immediate first tick is not a heartbeat
        loop {
            tick.tick().await;
            // Collect under the lock · print after (stderr I/O never
            // holds the pulse against the event stream).
            let beats = match pulse.lock() {
                Ok(pulse) => pulse.beats(),
                Err(_) => return, // poisoned — best-effort silence
            };
            for line in beats {
                eprintln!("{line}");
            }
        }
    })
}

/// One heartbeat: `still running · <task> · <n>s · <model>`. The model
/// rides bare off the `infer · <m>` / `agent · <m>` vocabulary (the
/// issue's shape); other verbs speak their label verbatim (`invoke ·
/// nika:fetch` — WHAT is running is the point); a labelless task stays
/// two-celled, never invents.
fn heartbeat_line(task: &str, secs: u64, label: Option<&str>) -> String {
    let mut line = format!("still running · {task} · {secs}s");
    if let Some(label) = label {
        let what = label
            .strip_prefix("infer · ")
            .or_else(|| label.strip_prefix("agent · "))
            .unwrap_or(label);
        if !what.is_empty() {
            line.push_str(" · ");
            line.push_str(what);
        }
    }
    line
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::demo;
    use nika_types::resource::{KeyValue, Value};

    fn task(n: &str) -> KeyValue {
        KeyValue::new("task", Value::String(n.to_owned()))
    }

    /// An instant `secs` in the past — ages the wave clock without
    /// sleeping (checked: the process epoch has minutes of headroom).
    fn aged(secs: u64) -> Instant {
        Instant::now()
            .checked_sub(Duration::from_secs(secs))
            .expect("epoch headroom")
    }

    /// The issue's exact shape: the model rides bare (the `infer · `
    /// prefix strips), other verbs speak their label verbatim, and a
    /// labelless task never grows an invented cell.
    #[test]
    fn heartbeat_line_speaks_the_model_or_the_verb() {
        assert_eq!(
            heartbeat_line("think", 15, Some("infer · ollama/qwen3.5:4b")),
            "still running · think · 15s · ollama/qwen3.5:4b"
        );
        assert_eq!(
            heartbeat_line("scout", 31, Some("agent · mock/echo")),
            "still running · scout · 31s · mock/echo"
        );
        assert_eq!(
            heartbeat_line("fetch", 12, Some("invoke · nika:fetch")),
            "still running · fetch · 12s · invoke · nika:fetch"
        );
        assert_eq!(
            heartbeat_line("bare", 10, None),
            "still running · bare · 10s"
        );
    }

    /// The labels derive from the WORKFLOW (the stream is mute
    /// mid-flight): the task's own model wins · the effective default
    /// fills modelless infer/agent · invoke names its tool · exec
    /// stays bare.
    #[test]
    fn task_labels_speak_the_static_truth() {
        let yaml = "nika: t\nmodel: ollama/qwen3.5:4b\ntasks:\n  think:\n    infer: { prompt: hi }\n  pinned:\n    infer: { prompt: hi, model: \"mock/echo\" }\n  fetch:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.com\" } }\n  build:\n    exec: { command: [\"sleep\", \"1\"] }\n";
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let labels = task_labels(&wf, "ollama/qwen3.5:4b");
        assert_eq!(labels["think"], "infer · ollama/qwen3.5:4b");
        assert_eq!(labels["pinned"], "infer · mock/echo", "task model wins");
        assert_eq!(labels["fetch"], "invoke · nika:fetch");
        assert_eq!(labels["build"], "exec");
    }

    /// The pulse walks the plan: wave 1's unsettled members beat; a
    /// settle that CLOSES the wave opens the next (fresh clock); the
    /// exhausted plan beats nothing. Every terminal kind settles; an
    /// inert sink folds nothing.
    #[test]
    fn pulse_beats_the_current_waves_unsettled_members() {
        let plan = vec![
            vec!["nap".to_owned(), "twin".to_owned()],
            vec!["after".to_owned()],
        ];
        let labels: BTreeMap<String, String> =
            [("after".to_owned(), "infer · mock/echo".to_owned())].into();
        let pulse = shared(plan, labels);
        let mut sink = HeartbeatSink::new(Some(Arc::clone(&pulse)));

        // Age the wave clock past the 1s floor without sleeping.
        pulse.lock().expect("unpoisoned").wave_opened = aged(11);
        let beats = pulse.lock().expect("unpoisoned").beats();
        assert_eq!(
            beats,
            vec![
                "still running · nap · 11s".to_owned(),
                "still running · twin · 11s".to_owned(),
            ],
            "wave 1 in flight — both members beat"
        );

        // One sibling settles — the wave stays open, the other beats on.
        sink.emit(demo::bare_event(EventKind::TaskCompleted, 5).with_field(task("twin")));
        let beats = pulse.lock().expect("unpoisoned").beats();
        assert_eq!(beats.len(), 1, "the settled sibling went quiet");
        assert!(beats[0].contains("nap"), "{beats:?}");

        // The wave closes — the next opens with a FRESH clock (under
        // the floor → quiet), then ages into its own beat.
        sink.emit(demo::bare_event(EventKind::TaskFailed, 6).with_field(task("nap")));
        assert!(
            pulse.lock().expect("unpoisoned").beats().is_empty(),
            "a just-opened wave is under the 1s floor"
        );
        pulse.lock().expect("unpoisoned").wave_opened = aged(12);
        let beats = pulse.lock().expect("unpoisoned").beats();
        assert_eq!(
            beats,
            vec!["still running · after · 12s · mock/echo".to_owned()],
            "wave 2 beats with its static model label"
        );

        // The plan exhausts — silence.
        sink.emit(demo::bare_event(EventKind::TaskSkipped, 7).with_field(task("after")));
        assert!(pulse.lock().expect("unpoisoned").beats().is_empty());

        // The inert handle folds nothing (the non-Plain lanes' shape).
        let mut inert = HeartbeatSink::new(None);
        inert.emit(demo::bare_event(EventKind::TaskCompleted, 8).with_field(task("ghost")));
    }
}
