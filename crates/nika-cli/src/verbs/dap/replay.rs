//! The replay session — a read-only debugger over a recorded run.
//!
//! The journal is total, so time travel is a cursor: STOPS are the task
//! settles in journal order; `next` walks forward, `stepBack` walks
//! backward, `continue` runs to the next breakpointed task, and
//! variables at any stop are simply the outputs settled so far. Nothing
//! executes — replay = re-render, NEVER re-execute (the trace-replay
//! law), which is why `supportsStepBack` is honest here.
//!
//! Breakpoints map file lines to tasks the ansibug way: a client line
//! SNAPS BACKWARD to the nearest task-start line at or above it —
//! `verified` echoes the snapped line so the editor moves the dot.

use nika_event::EventKind;

/// One task settle — a place the cursor can stand.
pub(crate) struct Stop {
    pub(crate) task: String,
    /// Terminal kind, human form (`completed` · `failed` · …).
    pub(crate) kind: &'static str,
    /// The recorded output (stamped successes carry it).
    pub(crate) output: Option<String>,
    /// 1-based task-start line in the workflow source (0 = unknown —
    /// a task the journal knows but the current source doesn't).
    pub(crate) line: u32,
}

pub(crate) struct ReplaySession {
    /// Client-side workflow path — every stack frame points here.
    pub(crate) workflow_path: String,
    pub(crate) workflow_name: String,
    /// The #210 identity check: Some(true) = the CURRENT yaml differs
    /// from the bytes the run executed (breakpoint lines may be off) ·
    /// None = the journal predates `workflow_sha256`.
    pub(crate) drifted: Option<bool>,
    /// `(task id, 1-based start line)` in document order.
    task_lines: Vec<(String, u32)>,
    pub(crate) stops: Vec<Stop>,
    /// Current stop index.
    pub(crate) cursor: usize,
    /// Verified breakpoint lines (snapped to task starts).
    breakpoints: Vec<u32>,
}

impl ReplaySession {
    /// Build from the launch arguments' two files.
    pub(crate) fn load(workflow_path: &str, replay_path: &str) -> Result<Self, String> {
        let yaml = std::fs::read_to_string(workflow_path)
            .map_err(|e| format!("cannot read workflow {workflow_path}: {e}"))?;
        let raw = std::fs::read_to_string(replay_path)
            .map_err(|e| format!("cannot read journal {replay_path}: {e}"))?;
        let recovered = super::super::run::recover_events(&raw, replay_path)?;
        Self::from_parts(workflow_path, &yaml, &recovered.events)
    }

    /// The #210 identity check against the CURRENT source bytes.
    fn drift_of(yaml: &str, events: &[nika_event::Event]) -> Option<bool> {
        let recorded = events
            .iter()
            .find(|e| e.kind == EventKind::WorkflowStarted)
            .and_then(|e| field_str(e, "workflow_sha256"))?;
        Some(super::super::run::sha256_hex(yaml.as_bytes()) != recorded)
    }

    /// The testable core: source text + folded events.
    pub(crate) fn from_parts(
        workflow_path: &str,
        yaml: &str,
        events: &[nika_event::Event],
    ) -> Result<Self, String> {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Lenient,
        )
        .map_err(|e| format!("workflow does not parse: {e}"))?;
        let task_lines: Vec<(String, u32)> = wf
            .tasks
            .iter()
            .map(|t| {
                (
                    t.value.id.value.clone(),
                    line_of_offset(yaml, t.span.start.0 as usize),
                )
            })
            .collect();

        let workflow_name = events
            .iter()
            .find(|e| e.kind == EventKind::WorkflowStarted)
            .and_then(|e| field_str(e, "workflow").map(ToOwned::to_owned))
            .ok_or_else(|| "no workflow_started event — not a run journal".to_owned())?;

        let stops: Vec<Stop> = events
            .iter()
            .filter_map(|e| {
                let kind = match e.kind {
                    EventKind::TaskCompleted => "completed",
                    EventKind::TaskFailed => "failed",
                    EventKind::TaskSkipped => "skipped",
                    EventKind::TaskCancelled => "cancelled",
                    EventKind::TaskCacheHit => "cache hit",
                    _ => return None,
                };
                let task = field_str(e, "task")?.to_owned();
                let line = task_lines
                    .iter()
                    .find(|(id, _)| *id == task)
                    .map_or(0, |(_, l)| *l);
                Some(Stop {
                    task,
                    kind,
                    output: field_str(e, "output").map(ToOwned::to_owned),
                    line,
                })
            })
            .collect();
        if stops.is_empty() {
            return Err("the journal records no task settles — nothing to replay".to_owned());
        }
        Ok(Self {
            workflow_path: workflow_path.to_owned(),
            workflow_name,
            drifted: Self::drift_of(yaml, events),
            task_lines,
            stops,
            cursor: 0,
            breakpoints: Vec::new(),
        })
    }

    /// Set (replace-all, per the DAP contract) breakpoints for the one
    /// source. Returns `(verified, snapped_line)` per requested line.
    pub(crate) fn set_breakpoints(&mut self, lines: &[u32]) -> Vec<(bool, u32)> {
        self.breakpoints.clear();
        lines
            .iter()
            .map(|&want| {
                // Snap BACKWARD: the nearest task start at or above.
                let snapped = self
                    .task_lines
                    .iter()
                    .map(|(_, l)| *l)
                    .filter(|&l| l <= want)
                    .max();
                match snapped {
                    Some(line) => {
                        self.breakpoints.push(line);
                        (true, line)
                    }
                    None => (false, want),
                }
            })
            .collect()
    }

    pub(crate) fn current(&self) -> &Stop {
        // cursor is clamped by every mutation — the indexing is total.
        &self.stops[self.cursor.min(self.stops.len() - 1)]
    }

    /// Forward to the next breakpointed stop. `false` = ran off the end.
    pub(crate) fn run_forward(&mut self) -> bool {
        let mut i = self.cursor + 1;
        while i < self.stops.len() {
            if self.breakpoints.contains(&self.stops[i].line) {
                self.cursor = i;
                return true;
            }
            i += 1;
        }
        false
    }

    /// Backward to the previous breakpointed stop (floor: first stop).
    pub(crate) fn run_backward(&mut self) {
        let mut i = self.cursor;
        while i > 0 {
            i -= 1;
            if self.breakpoints.contains(&self.stops[i].line) {
                self.cursor = i;
                return;
            }
        }
        self.cursor = 0;
    }

    /// One stop forward. `false` = already at the last settle.
    pub(crate) fn step(&mut self) -> bool {
        if self.cursor + 1 < self.stops.len() {
            self.cursor += 1;
            return true;
        }
        false
    }

    pub(crate) fn step_back(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Variables at the cursor: every settle up to and including it —
    /// the output when recorded, else the terminal kind.
    pub(crate) fn variables(&self) -> Vec<(String, String)> {
        self.stops[..=self.cursor.min(self.stops.len() - 1)]
            .iter()
            .map(|s| {
                let value = s.output.clone().unwrap_or_else(|| format!("({})", s.kind));
                (s.task.clone(), value)
            })
            .collect()
    }
}

/// 1-based line of a byte offset (the span is byte-addressed).
fn line_of_offset(text: &str, offset: usize) -> u32 {
    // Byte-wise walk — string slicing would PANIC on a non-char-boundary
    // offset (multi-byte chars upstream of a span are enough).
    let mut line: u32 = 1;
    for (i, b) in text.bytes().enumerate() {
        if i >= offset {
            break;
        }
        if b == b'\n' {
            line = line.saturating_add(1);
        }
    }
    line
}

fn field_str<'e>(event: &'e nika_event::Event, key: &str) -> Option<&'e str> {
    event.fields.iter().find_map(|f| match (&f.key, &f.value) {
        (k, nika_types::resource::Value::String(s)) if k == key => Some(s.as_str()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use nika_event::Event;
    use nika_types::id::EventId;
    use nika_types::resource::{KeyValue, Value as FieldValue};
    use nika_types::timestamp::Timestamp;
    use uuid::Uuid;

    use super::*;

    const YAML: &str = "nika: v1\nworkflow: demo\ntasks:\n  - id: alpha\n    exec:\n      command: [\"true\"]\n  - id: beta\n    depends_on: [alpha]\n    exec:\n      command: [\"true\"]\n  - id: gamma\n    depends_on: [beta]\n    exec:\n      command: [\"true\"]\n";

    fn ev(seed: u8, kind: EventKind, fields: &[(&str, &str)]) -> Event {
        let mut e = Event::new(
            EventId::new(Uuid::from_bytes([seed; 16])),
            Timestamp::from_unix_ns(i64::from(seed) * 1_000),
            kind,
        );
        for (k, v) in fields {
            e = e.with_field(KeyValue::new(*k, FieldValue::String((*v).to_owned())));
        }
        e
    }

    fn session() -> ReplaySession {
        let events = vec![
            ev(1, EventKind::WorkflowStarted, &[("workflow", "demo")]),
            ev(
                2,
                EventKind::TaskCompleted,
                &[("task", "alpha"), ("output", "\"a\"")],
            ),
            ev(3, EventKind::TaskCompleted, &[("task", "beta")]),
            ev(
                4,
                EventKind::TaskSkipped,
                &[("task", "gamma"), ("when", "${{ false }}")],
            ),
            ev(5, EventKind::WorkflowCompleted, &[("workflow", "demo")]),
        ];
        ReplaySession::from_parts("/w.nika.yaml", YAML, &events).expect("session builds")
    }

    #[test]
    fn stops_walk_the_settles_and_variables_accumulate() {
        let mut s = session();
        assert_eq!(s.stops.len(), 3);
        assert_eq!(s.current().task, "alpha");
        assert_eq!(s.variables().len(), 1);

        assert!(s.step());
        assert_eq!(s.current().task, "beta");
        // alpha's recorded output + beta's kind (no output recorded).
        let vars = s.variables();
        assert_eq!(vars[0], ("alpha".to_owned(), "\"a\"".to_owned()));
        assert_eq!(vars[1].1, "(completed)");

        assert!(s.step());
        assert_eq!(s.current().kind, "skipped");
        assert!(!s.step(), "the log is total — the last settle is the end");

        s.step_back();
        assert_eq!(s.current().task, "beta");
    }

    #[test]
    fn breakpoints_snap_backward_to_task_starts() {
        let mut s = session();
        // alpha starts line 3 · beta line 7 · gamma line 11 (doc order).
        let lines: Vec<u32> = s.task_lines.iter().map(|(_, l)| *l).collect();
        assert_eq!(lines.len(), 3);
        // A line INSIDE beta's block snaps back to beta's start.
        let verdicts = s.set_breakpoints(&[lines[1] + 1, 1]);
        assert_eq!(verdicts[0], (true, lines[1]));
        // Line 1 (above every task) cannot verify.
        assert!(!verdicts[1].0);

        // continue runs to beta; a second continue runs off the end.
        assert!(s.run_forward());
        assert_eq!(s.current().task, "beta");
        assert!(!s.run_forward(), "no further breakpoints → terminated");

        // reverseContinue with no earlier breakpoint floors at stop 0.
        s.run_backward();
        assert_eq!(s.current().task, "alpha");
    }

    #[test]
    fn line_of_offset_survives_multibyte_text() {
        // « é » is 2 bytes — an offset INSIDE it must not panic.
        let text = "é\nx";
        assert_eq!(line_of_offset(text, 1), 1);
        assert_eq!(line_of_offset(text, 3), 2);
        assert_eq!(line_of_offset(text, 999), 2);
    }

    #[test]
    fn drift_verdict_tracks_the_recorded_sha() {
        // No workflow_sha256 on the started frame → pre-#210 journal → None.
        assert_eq!(session().drifted, None);

        // A recorded sha that MATCHES the current bytes → not drifted.
        let sha = crate::verbs::run::sha256_hex(YAML.as_bytes());
        let make = |recorded: &str| {
            let events = vec![
                ev(
                    1,
                    EventKind::WorkflowStarted,
                    &[("workflow", "demo"), ("workflow_sha256", recorded)],
                ),
                ev(2, EventKind::TaskCompleted, &[("task", "alpha")]),
            ];
            ReplaySession::from_parts("/w.nika.yaml", YAML, &events).expect("builds")
        };
        assert_eq!(make(&sha).drifted, Some(false));
        assert_eq!(make(&"ab".repeat(32)).drifted, Some(true));
    }

    #[test]
    fn a_journal_without_settles_is_refused() {
        let only_start = vec![ev(1, EventKind::WorkflowStarted, &[("workflow", "demo")])];
        assert!(ReplaySession::from_parts("/w.nika.yaml", YAML, &only_start).is_err());
    }
}
