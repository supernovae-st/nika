// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The forecast reader — a bounded, fail-open scan of `.nika/traces/`
//! into per-run samples. One-voice law (store.rs:8-13): every file
//! parses through the SAME tolerant reader (`recover_events`) and the
//! terminal state folds through the SAME classifier (`fold_facts`) —
//! zero parallel parsers. The reader never errs: the sink is
//! infallible, so the reader is tolerant — every skip is COUNTED,
//! never silent.

use std::collections::BTreeSet;
use std::path::Path;

use nika_event::{Event, EventKind};
use nika_types::resource::Value;

use crate::RunView;
use crate::display::state::TaskState;
use crate::trace::retention::RetentionConfig;
use crate::trace::store::{TraceState, fold_facts};
use nika_dap::recover::recover_events;

use super::Window;

/// Hard cap on directory entries examined per gather — defends against
/// a `--no-gc` pileup: the forecast stays O(cap) whatever the history.
pub(crate) const SCAN_CAP: usize = 200;

/// The run-count window (C4 · hybrid with [`WINDOW_MS`], first reached).
pub(crate) const WINDOW_RUNS: usize = 50;

/// The age window in days (C4 · the render's « / 30 days » figure).
pub(crate) const WINDOW_DAYS: u32 = 30;

/// The age window in milliseconds, anchored on the newest MATCHING
/// run's `workflow_started` stamp — never on `now()` (determinism: the
/// same trace directory always forecasts the same).
pub(crate) const WINDOW_MS: i64 = WINDOW_DAYS as i64 * 86_400_000;

/// One task occurrence inside one recovered run.
#[derive(Debug, Clone)]
pub struct TaskSample {
    /// The task id (the workflow's vocabulary).
    pub id: String,
    /// Folded terminal state for this occurrence.
    pub state: TaskState,
    /// The occurrence was a `task_cache_hit` rehydration — excluded
    /// from duration/cost bands (it never ran here), counted apart.
    pub cached: bool,
    /// The model the run's notes named, when one did (`None` is a
    /// clean bucket of its own — exec/invoke tasks).
    pub model: Option<String>,
    /// Runtime-measured wall duration (the REAL one, not stamp span).
    pub duration_ms: Option<u64>,
    /// Per-task realized spend, when the terminal frame carried one.
    pub cost_usd: Option<f64>,
    /// The task retried at least once, then reached its terminal —
    /// C5: retried-then-completed is FLAKY, never failed.
    pub retried: bool,
    /// The `cost_unpriced` slug the frames carried, if any.
    pub unpriced: Option<String>,
}

/// One recovered run, reduced to its forecast facts.
#[derive(Debug, Clone)]
pub struct RunSample {
    /// Source identity from `workflow_started` — `None` when the run
    /// predates hash stamping (the fields are OPTIONAL): bucketed
    /// `Unknown`, never silently same/stale.
    pub sha256: Option<String>,
    /// The LF normal-form twin, when recorded.
    pub sha256_lf: Option<String>,
    /// `workflow_started` stamp (unix ms) — the window anchor.
    pub started_ms: i64,
    /// The run's terminal classification (`fold_facts` — one voice).
    pub state: TraceState,
    /// Wall-clock span folded from event stamps (ms).
    pub elapsed_ms: u64,
    /// The terminal frame's AUTHORITATIVE run total, when priced —
    /// absence is absence (`None`), never a fake zero.
    pub total_cost_usd: Option<f64>,
    /// Per-task occurrences, in first-seen order.
    pub tasks: Vec<TaskSample>,
}

/// Everything the reader hands `compute` — samples in newest-first
/// order plus the accounting every skip landed in.
#[derive(Debug, Default)]
pub struct Gathered {
    /// Matching runs inside the window, newest first.
    pub samples: Vec<RunSample>,
    /// The window accounting (caps · skips · retention knob).
    pub window: Window,
}

/// Scan `dir` for the newest runs of `workflow_name`, bounded and
/// fail-open. A missing directory gathers empty — gather never errs.
#[must_use]
pub fn gather(dir: &Path, workflow_name: &str) -> Gathered {
    let mut out = Gathered {
        window: Window::new(),
        ..Gathered::default()
    };
    out.window.retention_keep = resolved_retention().keep_last;

    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    // Filename lexico-DESC = newest first: the names embed the run's
    // ISO stamp (content-derived), where mtime is not (a clone/rsync
    // rewrites it) — the `traces_glance` precedent.
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            name.ends_with(".ndjson").then_some(name)
        })
        .collect();
    names.sort_unstable();
    names.reverse();

    for name in names {
        if out.window.files_examined >= SCAN_CAP || out.samples.len() >= WINDOW_RUNS {
            break;
        }
        out.window.files_examined += 1;
        // TOCTOU is a fact of life here: retention GC can race the
        // scan — a vanished file is COUNTED unreadable, never an error.
        let Ok(raw) = std::fs::read_to_string(dir.join(&name)) else {
            out.window.files_unreadable += 1;
            continue;
        };
        let Ok(recovered) = recover_events(&raw, &name) else {
            out.window.files_unreadable += 1;
            continue;
        };
        if recovered.truncated_note.is_some() {
            out.window.files_truncated += 1;
        }
        let (run_workflow, state, _paused_task) = fold_facts(&recovered.events);
        if run_workflow != workflow_name {
            continue;
        }
        if let Some(sample) = fold_sample(&recovered.events, state) {
            out.samples.push(sample);
        }
    }

    // The 30-day cut, anchored on the newest MATCHING run (samples are
    // newest-first, so the anchor is the head).
    if let Some(anchor) = out.samples.first().map(|s| s.started_ms) {
        let floor = anchor.saturating_sub(WINDOW_MS);
        out.samples.retain(|s| s.started_ms >= floor);
    }
    out
}

/// Reduce one recovered run to its sample: the shared `RunView` fold
/// (rows · elapsed) plus the two facts it cannot carry — per-task
/// retry attribution (`RunView.retries` is global) and the
/// `cost_unpriced` slug (`apply_task_completed` drops it).
pub(crate) fn fold_sample(events: &[Event], state: TraceState) -> Option<RunSample> {
    let mut view = RunView::new();
    for event in events {
        view.apply(event);
    }
    let started = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)?;
    let started_ms = started.timestamp.unix_ms();
    let sha256 = str_field(started, "workflow_sha256").map(str::to_owned);
    let sha256_lf = str_field(started, "workflow_sha256_lf").map(str::to_owned);

    let mut retried: BTreeSet<String> = BTreeSet::new();
    let mut unpriced: Vec<(String, String)> = Vec::new();
    let mut total_cost_usd = None;
    for event in events {
        match event.kind {
            EventKind::TaskRetrying => {
                if let Some(task) = str_field(event, "task") {
                    retried.insert(task.to_owned());
                }
            }
            EventKind::TaskCompleted | EventKind::TaskFailed => {
                if let (Some(task), Some(slug)) =
                    (str_field(event, "task"), str_field(event, "cost_unpriced"))
                {
                    unpriced.push((task.to_owned(), slug.to_owned()));
                }
            }
            EventKind::WorkflowCompleted | EventKind::WorkflowFailed => {
                if let Some(total) = float_field(event, "total_cost_usd") {
                    total_cost_usd = Some(total);
                }
            }
            // Dispatch/stream/checkpoint kinds carry no forecast fact —
            // and so do shipped kinds like `task_recovered` (#313: a
            // resume repair marker, not a duration/cost sample). Future
            // `#[non_exhaustive]` kinds ride the same arm: fold nothing
            // rather than guessing.
            _ => {}
        }
    }

    let tasks = view
        .rows()
        .iter()
        .map(|row| TaskSample {
            id: row.id.clone(),
            state: row.state,
            cached: row.cached,
            model: row.model.clone(),
            duration_ms: row.wall_ms(),
            cost_usd: row.cost_usd,
            retried: retried.contains(&row.id),
            unpriced: unpriced
                .iter()
                .find(|(task, _)| *task == row.id)
                .map(|(_, slug)| slug.clone()),
        })
        .collect();

    Some(RunSample {
        sha256,
        sha256_lf,
        started_ms,
        state,
        elapsed_ms: view.elapsed_ms,
        total_cost_usd,
        tasks,
    })
}

/// The active retention config — the same knobs GC enforces (ONE
/// resolution path · `RetentionConfig::from_env`), so the window line
/// can say WHY n saturates at keep-last-N.
fn resolved_retention() -> RetentionConfig {
    RetentionConfig::from_env().0
}

/// One string field off an event (the journal's additive KV vocabulary
/// — the store/state twins are private, ~6 lines each by design).
fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event.fields.iter().find(|kv| kv.key == key).and_then(|kv| {
        if let Value::String(s) = &kv.value {
            Some(s.as_str())
        } else {
            None
        }
    })
}

/// One float field off an event — the Int arm coerces like the state
/// fold's twin (state.rs): an integer-typed total is still a total.
fn float_field(event: &Event, key: &str) -> Option<f64> {
    event
        .fields
        .iter()
        .find(|kv| kv.key == key)
        .and_then(|kv| match &kv.value {
            Value::Float(f) => Some(*f),
            #[allow(clippy::cast_precision_loss)] // display-only magnitude
            Value::Int(i) => Some(*i as f64),
            _ => None,
        })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::trace::store::tests::{stage_trace, temp_store};
    use nika_types::id::EventId;
    use nika_types::resource::KeyValue;
    use nika_types::timestamp::Timestamp;
    use std::time::Duration;
    use uuid::Uuid;

    /// One journal event with arbitrary KV fields (the store helper
    /// carries exactly one string field — forecasts need several).
    pub(crate) fn ev(kind: EventKind, ms: u64, fields: &[(&str, Value)]) -> Event {
        let mut event = Event::new(EventId::new(Uuid::nil()), Timestamp::from_unix_ms(ms), kind);
        for (key, value) in fields {
            event = event.with_field(KeyValue::new(*key, value.clone()));
        }
        event
    }

    pub(crate) fn ndjson(events: &[Event]) -> String {
        let mut body = String::new();
        for e in events {
            body.push_str(&serde_json::to_string(e).expect("event serializes"));
            body.push('\n');
        }
        body
    }

    /// A complete run journal: started (with hashes) · tasks · terminal.
    pub(crate) fn run_body(
        workflow: &str,
        sha: Option<&str>,
        started_ms: u64,
        tasks: &[Event],
        terminal: Option<Event>,
    ) -> String {
        let mut events = Vec::new();
        let mut fields = vec![("workflow", Value::string(workflow))];
        if let Some(sha) = sha {
            fields.push(("workflow_sha256", Value::string(sha)));
            fields.push(("workflow_sha256_lf", Value::string(sha)));
        }
        events.push(ev(EventKind::WorkflowStarted, started_ms, &fields));
        events.extend_from_slice(tasks);
        if let Some(t) = terminal {
            events.push(t);
        }
        ndjson(&events)
    }

    /// One completed-task frame with duration + optional cost/model.
    pub(crate) fn task_done(
        id: &str,
        ms: u64,
        dur: u64,
        cost: Option<f64>,
        model: Option<&str>,
    ) -> Event {
        let mut fields = vec![
            ("task", Value::string(id)),
            ("duration_ms", Value::Int(i64::try_from(dur).unwrap_or(0))),
        ];
        if let Some(c) = cost {
            fields.push(("cost_usd", Value::Float(c)));
        }
        if let Some(m) = model {
            fields.push(("note", Value::string(format!("infer · {m}"))));
        }
        ev(EventKind::TaskCompleted, ms, &fields)
    }

    pub(crate) fn done(ms: u64, total: Option<f64>) -> Event {
        let mut fields = vec![("workflow", Value::string("wf"))];
        if let Some(t) = total {
            fields.push(("total_cost_usd", Value::Float(t)));
        }
        ev(EventKind::WorkflowCompleted, ms, &fields)
    }

    /// ISO-shaped trace name at a lexicographic position: `idx` orders
    /// newest-last alphabetically, so idx 99 is the newest.
    fn name(idx: usize) -> String {
        format!("2026-07-08T{:02}-00-00Z-{idx:04}.ndjson", idx % 24)
    }

    #[test]
    fn gathers_newest_first_and_matches_by_name() {
        let dir = temp_store("forecast-gather-basic");
        let old = run_body(
            "wf",
            Some("aaaa"),
            1_000,
            &[task_done("t", 1_010, 100, None, None)],
            Some(done(1_020, None)),
        );
        let new = run_body(
            "wf",
            Some("aaaa"),
            2_000,
            &[task_done("t", 2_010, 200, None, None)],
            Some(done(2_020, None)),
        );
        let other = run_body("other", Some("bbbb"), 3_000, &[], Some(done(3_010, None)));
        stage_trace(
            &dir,
            "2026-07-08T01-00-00Z-0001.ndjson",
            &old,
            Duration::from_secs(60),
        );
        stage_trace(
            &dir,
            "2026-07-08T02-00-00Z-0002.ndjson",
            &new,
            Duration::from_secs(30),
        );
        stage_trace(
            &dir,
            "2026-07-08T03-00-00Z-0003.ndjson",
            &other,
            Duration::from_secs(10),
        );
        let g = gather(&dir, "wf");
        assert_eq!(g.samples.len(), 2);
        assert_eq!(g.samples[0].started_ms, 2_000); // newest first
        assert_eq!(g.window.files_examined, 3);
        assert_eq!(g.window.files_unreadable, 0);
    }

    #[test]
    fn window_is_anchored_on_newest_matching_run_never_now() {
        let dir = temp_store("forecast-gather-window");
        // A 60-day-old pair: both far from now(), 1ms apart from each
        // other — an anchor on now() would drop both; the per-workflow
        // anchor keeps both.
        let t0: u64 = 3_000_000_000; // ≫ WINDOW_MS so the cut bites
        let within = run_body("wf", None, t0, &[], Some(done(t0 + 10, None)));
        let anchor = run_body("wf", None, t0 + 1, &[], Some(done(t0 + 11, None)));
        // And one BEYOND 30d before the anchor: dropped by the cut.
        let win_ms = u64::try_from(WINDOW_MS).unwrap_or(0);
        let ancient_ms = (t0 + 1).saturating_sub(win_ms + 1);
        let ancient = run_body(
            "wf",
            None,
            ancient_ms,
            &[],
            Some(done(ancient_ms + 5, None)),
        );
        stage_trace(
            &dir,
            "2026-07-08T01-00-00Z-0001.ndjson",
            &ancient,
            Duration::from_secs(90),
        );
        stage_trace(
            &dir,
            "2026-07-08T02-00-00Z-0002.ndjson",
            &within,
            Duration::from_secs(60),
        );
        stage_trace(
            &dir,
            "2026-07-08T03-00-00Z-0003.ndjson",
            &anchor,
            Duration::from_secs(30),
        );
        let g = gather(&dir, "wf");
        assert_eq!(g.samples.len(), 2, "the ancient run exits the window");
        assert!(
            g.samples
                .iter()
                .all(|s| s.started_ms >= i64::try_from(t0).unwrap_or(i64::MAX))
        );
    }

    #[test]
    fn caps_bound_the_scan() {
        let dir = temp_store("forecast-gather-caps");
        for idx in 0..60 {
            let ms = 10_000 + u64::try_from(idx).unwrap_or(0);
            let body = run_body("wf", None, ms, &[], Some(done(ms + 1, None)));
            stage_trace(&dir, &name(idx), &body, Duration::from_secs(5));
        }
        let g = gather(&dir, "wf");
        assert_eq!(g.samples.len(), WINDOW_RUNS, "K caps the collection");
        assert!(g.window.files_examined <= SCAN_CAP);
    }

    #[test]
    fn skips_are_counted_never_silent() {
        let dir = temp_store("forecast-gather-skips");
        stage_trace(
            &dir,
            "2026-07-08T01-00-00Z-dead.ndjson",
            "not json at all\n",
            Duration::from_secs(9),
        );
        let good = run_body("wf", None, 5_000, &[], Some(done(5_010, None)));
        // A valid prefix + a WELL-FORMED line whose kind this binary does
        // not know (a FUTURE engine's trace): serde refuses the LINE —
        // truncation counted, prefix USED. (`task_recovered` stopped
        // qualifying the day #313 shipped it — a genuinely unknown slug
        // is the only honest fixture here.)
        let future = format!("{good}{{\"kind\":\"zz_future_kind\",\"x\":1}}\n");
        stage_trace(
            &dir,
            "2026-07-08T02-00-00Z-future.ndjson",
            &future,
            Duration::from_secs(8),
        );
        // And the crash shape: a syntactically torn tail.
        let torn = format!("{good}{{\"kind\":\"task_started\",\"torn\":");
        stage_trace(
            &dir,
            "2026-07-08T03-00-00Z-torn.ndjson",
            &torn,
            Duration::from_secs(7),
        );
        let g = gather(&dir, "wf");
        assert_eq!(g.window.files_unreadable, 1, "first-line-dead file counted");
        assert_eq!(
            g.window.files_truncated, 2,
            "unknown-kind line AND torn tail each counted"
        );
        assert_eq!(g.samples.len(), 2, "both valid prefixes are used");
    }

    /// `task_recovered` (#313) DESERIALIZES and folds nothing — the `_`
    /// arm is load-bearing with a REAL shipped variant, never a forgery.
    #[test]
    fn shipped_no_fact_kinds_ride_the_wildcard_arm() {
        let dir = temp_store("forecast-gather-recovered");
        let repair = ev(
            EventKind::TaskRecovered,
            5_005,
            &[("task", Value::string("t"))],
        );
        let body = run_body(
            "wf",
            None,
            5_000,
            &[repair, task_done("t", 5_010, 40, None, None)],
            Some(done(5_020, None)),
        );
        stage_trace(
            &dir,
            "2026-07-08T06-00-00Z-0006.ndjson",
            &body,
            Duration::from_secs(1),
        );
        let g = gather(&dir, "wf");
        assert_eq!(
            g.window.files_truncated, 0,
            "a shipped kind never truncates"
        );
        assert_eq!(g.samples.len(), 1);
        let run = &g.samples[0];
        assert_eq!(run.state, TraceState::Succeeded);
        let t = run.tasks.iter().find(|t| t.id == "t").expect("test value");
        assert_eq!(t.duration_ms, Some(40), "the completion sample survived");
    }

    /// The plan's retention-interaction fixture: 12 finished traces ·
    /// default GC keeps 10 · the reader sees EXACTLY the kept window and
    /// the report carries the knob that explains why.
    #[test]
    fn retention_bounds_the_window_and_the_report_names_it() {
        use crate::trace::retention;
        use std::time::SystemTime;
        let dir = temp_store("forecast-gather-retention");
        for i in 0..12u64 {
            let ms = 100_000 + i * 1_000;
            let body = run_body("wf", None, ms, &[], Some(done(ms + 10, None)));
            stage_trace(
                &dir,
                &format!("2026-07-08T{:02}-00-00Z-{i:04}.ndjson", 1 + i),
                &body,
                Duration::from_secs((12 - i) * 3_600),
            );
        }
        let gc = retention::collect(&dir, &RetentionConfig::default(), SystemTime::now());
        assert!(gc.is_some(), "12 finished under keep-10 → 2 rotate");
        let g = gather(&dir, "wf");
        assert_eq!(
            g.samples.len(),
            10,
            "the reader sees exactly the kept window"
        );
        assert_eq!(
            g.window.retention_keep,
            RetentionConfig::default().keep_last
        );
    }

    #[test]
    fn missing_dir_gathers_empty() {
        let g = gather(Path::new("/nonexistent/nika/traces"), "wf");
        assert!(g.samples.is_empty());
        assert_eq!(g.window.files_examined, 0);
    }

    #[test]
    fn per_task_facts_fold_retry_cache_unpriced_and_totals() {
        let dir = temp_store("forecast-gather-facts");
        let retry = ev(
            EventKind::TaskRetrying,
            6_005,
            &[("task", Value::string("flaky"))],
        );
        let flaky_done = task_done("flaky", 6_010, 50, Some(0.01), Some("mock/echo"));
        let cached = ev(
            EventKind::TaskCacheHit,
            6_012,
            &[("task", Value::string("warm"))],
        );
        let unpriced = ev(
            EventKind::TaskCompleted,
            6_020,
            &[
                ("task", Value::string("local")),
                ("duration_ms", Value::Int(30)),
                ("cost_unpriced", Value::string("local_model")),
            ],
        );
        let body = run_body(
            "wf",
            Some("cafe"),
            6_000,
            &[retry, flaky_done, cached, unpriced],
            Some(done(6_030, Some(0.01))),
        );
        stage_trace(
            &dir,
            "2026-07-08T04-00-00Z-0004.ndjson",
            &body,
            Duration::from_secs(3),
        );
        let g = gather(&dir, "wf");
        assert_eq!(g.samples.len(), 1);
        let run = &g.samples[0];
        assert_eq!(run.total_cost_usd, Some(0.01));
        assert_eq!(run.sha256.as_deref(), Some("cafe"));
        let flaky = run
            .tasks
            .iter()
            .find(|t| t.id == "flaky")
            .expect("test value");
        assert!(flaky.retried, "C5: retried-then-completed is flaky");
        assert_eq!(flaky.state, TaskState::Ok);
        assert_eq!(flaky.model.as_deref(), Some("mock/echo"));
        let warm = run
            .tasks
            .iter()
            .find(|t| t.id == "warm")
            .expect("test value");
        assert!(warm.cached);
        let local = run
            .tasks
            .iter()
            .find(|t| t.id == "local")
            .expect("test value");
        assert_eq!(local.unpriced.as_deref(), Some("local_model"));
        assert_eq!(local.cost_usd, None, "absence is absence, never $0");
    }

    #[test]
    fn elapsed_is_stamp_span_never_duration_sum() {
        // Two overlapping tasks: starts 0/10 · durations 100/100 ·
        // terminal stamps 100/110 → elapsed 110, never 200.
        let dir = temp_store("forecast-gather-elapsed");
        let t1 = task_done("a", 100, 100, None, None);
        let t2 = task_done("b", 110, 100, None, None);
        let body = run_body("wf", None, 0, &[t1, t2], Some(done(110, None)));
        stage_trace(
            &dir,
            "2026-07-08T05-00-00Z-0005.ndjson",
            &body,
            Duration::from_secs(2),
        );
        let g = gather(&dir, "wf");
        assert_eq!(g.samples[0].elapsed_ms, 110);
    }
}
