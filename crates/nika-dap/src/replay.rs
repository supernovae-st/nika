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
    /// The chain check (P2): the journal fails verification — its
    /// claims are unverified. False for intact/torn/unchained.
    pub(crate) chain_broken: bool,
    /// The torn-tail diagnostic from journal recovery, when the last
    /// line was cut mid-write — surfaced as a launch output event.
    pub(crate) truncated_note: Option<String>,
    /// `(task id, 1-based start line)` in document order.
    task_lines: Vec<(String, u32)>,
    pub(crate) stops: Vec<Stop>,
    /// Current stop index — private: only this module's movement verbs
    /// may steer it (the totality clamps are belt-and-braces on top).
    cursor: usize,
    /// Verified breakpoint lines (snapped to task starts).
    breakpoints: Vec<u32>,
}

/// File-side twin of the wire's `MAX_FRAME_BYTES`: launch paths come
/// from the DAP client, and an unbounded `read_to_string` turned a
/// special file (`/dev/zero` · a FIFO) into a silent hang and a huge
/// file into a 1:1 RAM map — the exact class the stdio layer already
/// refuses before allocating. 64 MiB fits any real journal (a 50k-settle
/// run measures ~12 MiB) with headroom.
const MAX_LAUNCH_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// `read_to_string` with the guards the stdio frames get: regular files
/// only (a device/FIFO would hang the read), size checked BEFORE the
/// allocation.
fn bounded_read(path: &str, what: &str) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("cannot read {what} {path}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("cannot read {what} {path}: not a regular file"));
    }
    if meta.len() > MAX_LAUNCH_FILE_BYTES {
        return Err(format!(
            "cannot read {what} {path}: {} bytes exceeds the {} MiB launch cap",
            meta.len(),
            MAX_LAUNCH_FILE_BYTES / (1024 * 1024)
        ));
    }
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {what} {path}: {e}"))
}

impl ReplaySession {
    /// Build from the launch arguments' two files.
    pub(crate) fn load(workflow_path: &str, replay_path: &str) -> Result<Self, String> {
        let yaml = bounded_read(workflow_path, "workflow")?;
        let raw = bounded_read(replay_path, "journal")?;
        let recovered =
            crate::recover::recover_events(&raw, replay_path).map_err(|e| e.to_string())?;
        let mut session = Self::from_parts(workflow_path, &yaml, &recovered.events)?;
        // A torn tail is the crashed-run scenario this debugger exists
        // for — the note was computed and then silently dropped here
        // (the 0.96.0 review's finding): the person debugging a crash
        // must know they are seeing a partial run.
        session.truncated_note = recovered.truncated_note;
        // Verify before trusting: a broken chain replays (warn, never
        // block — coherent with tamper-EVIDENT), but the debugger says
        // so before the first stop.
        session.chain_broken = matches!(
            crate::chain::walk(&raw),
            crate::chain::Verdict::Broken { .. }
        );
        Ok(session)
    }

    /// The #210 identity check against the CURRENT source bytes — in
    /// content terms, not byte terms: an editor re-encoding CRLF↔LF (or
    /// adding a BOM) cannot move a breakpoint line, and the 0.96.0
    /// review proved the raw compare cried wolf on exactly that. Raw
    /// match first; then LF normal forms (against the recorded raw for
    /// LF-recorded files, against `workflow_sha256_lf` for
    /// CRLF-recorded ones). Only a CONTENT change survives all three.
    fn drift_of(yaml: &str, events: &[nika_event::Event]) -> Option<bool> {
        // ONE comparator, two callers (the resume path owes the same
        // answer) — the CRLF/BOM nuance above lives there now, stated
        // once so the two surfaces cannot answer differently.
        crate::resume::source_drifted(yaml, events)
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
            chain_broken: false,
            truncated_note: None,
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
        // Same totality law as current(): a wild cursor stands on the
        // last stop (stops is non-empty by construction — from_parts
        // refuses an empty journal).
        let mut i = self.cursor.min(self.stops.len() - 1);
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
    /// the output when recorded, else the terminal kind. Recorded
    /// outputs are BORROWED: the per-call clone of the whole settled
    /// prefix was the deep-cursor latency cliff on very long runs
    /// (the 0.96.0 review's fan-out finding).
    pub(crate) fn variables(&self) -> Vec<(&str, std::borrow::Cow<'_, str>)> {
        self.stops[..=self.cursor.min(self.stops.len() - 1)]
            .iter()
            .map(|s| {
                let value = s.output.as_deref().map_or_else(
                    || std::borrow::Cow::Owned(format!("({})", s.kind)),
                    std::borrow::Cow::Borrowed,
                );
                (s.task.as_str(), value)
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

    const YAML: &str = "nika: demo\ntasks:\n  alpha:\n    exec:\n      command: [\"true\"]\n  beta:\n    after:\n      alpha: success\n    exec:\n      command: [\"true\"]\n  gamma:\n    after:\n      beta: success\n    exec:\n      command: [\"true\"]\n";

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
        assert_eq!(vars[0].0, "alpha");
        assert_eq!(vars[0].1, "\"a\"");
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
        let sha = nika_event::source_id::sha256_hex(YAML.as_bytes());
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
    fn a_reencode_is_not_drift_but_an_edit_is() {
        // The four quadrants of the encoding rule. Raw shas differ in
        // the two re-encode cases; the LF normal forms agree — only a
        // content change may say drifted.
        let crlf = YAML.replace('\n', "\r\n");
        let raw = |text: &str| nika_event::source_id::sha256_hex(text.as_bytes());
        let started = |fields: &[(&str, &str)]| vec![ev(1, EventKind::WorkflowStarted, fields)];

        // LF-recorded · current CRLF → the LF form matches the recorded raw.
        let rec_lf = started(&[("workflow_sha256", &raw(YAML))]);
        assert_eq!(ReplaySession::drift_of(&crlf, &rec_lf), Some(false));

        // CRLF-recorded (journal carries the _lf sibling) · current LF.
        let rec_crlf = started(&[
            ("workflow_sha256", &raw(&crlf)),
            ("workflow_sha256_lf", &raw(YAML)),
        ]);
        assert_eq!(ReplaySession::drift_of(YAML, &rec_crlf), Some(false));

        // BOM added by an editor → not an edit either.
        let bom = format!("\u{feff}{YAML}");
        assert_eq!(ReplaySession::drift_of(&bom, &rec_lf), Some(false));

        // A real content edit still drifts, whatever the encoding.
        let edited = YAML.replace("alpha", "omega");
        assert_eq!(ReplaySession::drift_of(&edited, &rec_lf), Some(true));
        assert_eq!(
            ReplaySession::drift_of(&edited.replace('\n', "\r\n"), &rec_crlf),
            Some(true)
        );
    }

    #[test]
    fn bounded_read_refuses_a_non_regular_file() {
        // /dev/zero hung the adapter for the full client timeout (the
        // 0.96.0 review's finding) — a device is refused BEFORE the read.
        let err = bounded_read("/dev/zero", "journal").expect_err("device refused");
        assert!(err.contains("not a regular file"), "{err}");
    }

    #[test]
    fn load_surfaces_the_torn_tail_note() {
        // A journal cut mid-write replays its valid prefix — WITH the
        // note (it was computed and silently dropped before). Real files
        // through the real load(): events serialized with the module's
        // own helper, the last line torn.
        let dir = std::env::temp_dir().join(format!("nika-dap-torn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let wf = dir.join("t.nika.yaml");
        std::fs::write(&wf, YAML).expect("wf");
        let journal = dir.join("torn.ndjson");
        let lines = [
            serde_json::to_string(&ev(1, EventKind::WorkflowStarted, &[("workflow", "demo")]))
                .expect("json"),
            serde_json::to_string(&ev(2, EventKind::TaskCompleted, &[("task", "alpha")]))
                .expect("json"),
            "{\"kind\":\"task_star".to_owned(), // torn mid-write
        ];
        std::fs::write(&journal, lines.join("\n")).expect("journal");
        let s = ReplaySession::load(wf.to_str().expect("utf8"), journal.to_str().expect("utf8"))
            .expect("valid prefix replays");
        assert!(s.truncated_note.is_some(), "the torn tail must be surfaced");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_journal_without_settles_is_refused() {
        let only_start = vec![ev(1, EventKind::WorkflowStarted, &[("workflow", "demo")])];
        assert!(ReplaySession::from_parts("/w.nika.yaml", YAML, &only_start).is_err());
    }

    #[test]
    fn the_launch_cap_sits_at_the_documented_boundary() {
        // The doc comment and the error message both speak 64 MiB —
        // arithmetic drift in the constant would silently move the
        // contract, so the figure and the strictly-greater guard are
        // both pinned here (sparse files: no real 64 MiB is written).
        assert_eq!(MAX_LAUNCH_FILE_BYTES, 67_108_864);
        let dir = std::env::temp_dir().join(format!("nika-dap-cap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let at_cap = dir.join("at-cap.ndjson");
        std::fs::File::create(&at_cap)
            .and_then(|f| f.set_len(MAX_LAUNCH_FILE_BYTES))
            .expect("sparse at-cap file");
        assert!(
            bounded_read(at_cap.to_str().expect("utf8"), "journal").is_ok(),
            "exactly AT the cap reads — the guard is strictly-greater"
        );
        let over = dir.join("over.ndjson");
        std::fs::File::create(&over)
            .and_then(|f| f.set_len(MAX_LAUNCH_FILE_BYTES + 1))
            .expect("sparse over file");
        let err = bounded_read(over.to_str().expect("utf8"), "journal")
            .expect_err("one byte over refuses");
        assert!(err.contains("exceeds the 64 MiB launch cap"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_terminal_kind_becomes_a_stop() {
        // failed · cancelled · cache hit each carry their own fold arm —
        // losing one silently drops that settle from the replay.
        let events = vec![
            ev(1, EventKind::WorkflowStarted, &[("workflow", "demo")]),
            ev(2, EventKind::TaskFailed, &[("task", "alpha")]),
            ev(3, EventKind::TaskCancelled, &[("task", "beta")]),
            ev(4, EventKind::TaskCacheHit, &[("task", "gamma")]),
        ];
        let s = ReplaySession::from_parts("/w.nika.yaml", YAML, &events).expect("builds");
        let kinds: Vec<&str> = s.stops.iter().map(|st| st.kind).collect();
        assert_eq!(kinds, ["failed", "cancelled", "cache hit"]);
    }

    #[test]
    fn a_wild_cursor_is_clamped_by_the_total_indexing() {
        // current()/variables()/run_backward() promise totality even
        // for a cursor past the end — the defensive min IS the
        // contract, not decoration.
        let mut s = session();
        s.cursor = 999;
        assert_eq!(s.current().task, "gamma");
        assert_eq!(s.variables().len(), 3, "the whole settled prefix");

        // run_backward from a wild cursor: clamps to the last stop,
        // then walks back INTO the breakpoint — no panic, no skip.
        let beta_line = s.task_lines[1].1;
        s.set_breakpoints(&[beta_line]);
        s.cursor = 999;
        s.run_backward();
        assert_eq!(s.current().task, "beta");

        // The clamp must land ON the last stop, not past it: standing
        // (clamped) on gamma, a breakpoint on gamma itself is BEHIND
        // no one — backward floors to the first stop, never re-lands
        // on the stop it stands on (kills the len-vs-len-1 mutant).
        let gamma_line = s.task_lines[2].1;
        s.set_breakpoints(&[gamma_line]);
        s.cursor = 999;
        s.run_backward();
        assert_eq!(
            s.current().task,
            "alpha",
            "wild cursor stands ON the last stop — backward never re-visits it"
        );
    }

    #[test]
    fn run_backward_stops_at_the_previous_breakpoint() {
        let mut s = session();
        let beta_line = s.task_lines[1].1;
        s.set_breakpoints(&[beta_line]);
        s.cursor = 2;
        s.run_backward();
        assert_eq!(
            s.current().task,
            "beta",
            "walks back INTO the breakpoint, never past it to the floor"
        );
    }
}
