// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Live TTY folding and the finalized trace epilogue. Journal writing lives in
//! `nika_dap::journal`; every lane consumes the same typed runtime stream.

use std::io::Write;

use nika_event::Event;
use nika_runtime::EventSink;
use nika_types::id::ExecutionId;

use nika_dap::journal::TraceFileSink;

use crate::display::render::{stream_header, stream_settled_line, stream_summary};
use crate::display::state::str_field;
use crate::{RunView, Theme, frame, frame_with_outputs, verdict_frame};

/// How the live fold reaches the terminal (spec §3.5 reduced surfaces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderMode {
    /// Rich TTY: in-place repaint per event (cursor control · the default).
    Live,
    /// Plain: silent fold, ONE final storyboard frame (no animation · no
    /// cursor escapes) — `--no-progress` and the piped/CI default.
    Plain,
    /// Interactive thread: stream task settles, then let the outer thread
    /// place the resolved answer in its conversation.
    Thread,
    /// Quiet: silent fold, the COMPACT verdict card only (errors always) —
    /// `--quiet`.
    Quiet,
}

/// Fold events into the live [`RunView`]; spinner ticks repaint only Live.
pub struct FoldSink<W: Write> {
    writer: W,
    theme: Theme,
    view: RunView,
    /// Selected render surface (spec §3.5).
    mode: RenderMode,
    /// Paint the shape tails (`→ {…} · 312B · 90 tok`) on completed rows.
    /// The interactive-TTY comprehension surface — the run verb enables it
    /// for `Live` only (pipes · CI · `--no-outputs` stay byte-unchanged).
    outputs: bool,
    /// Gates trace-only teaching when journaling is disabled.
    trace_recorded: bool,
    /// The workflow path this fold is narrating — try rehearsals stage
    /// under `nika-try-<slug>/` and the fruit card must not name a
    /// discarded write (C12).
    source_path: Option<String>,
    /// Lines painted by the previous frame (to clear before the redraw).
    last_lines: usize,
    /// The spinner phase — advanced by the timer rider, read by every
    /// repaint (events repaint at the CURRENT phase, never reset it).
    tick: usize,
    /// The LIVING MAP's topology — the checked projection the run verb
    /// injects before driving. Every Live repaint redraws the wire art
    /// with each node painted by its CURRENT state (the same geometry
    /// `graph --format ascii` speaks); runs whose shape the wire law
    /// refuses fall back to the frame's own wave-column line.
    map: Option<(crate::verbs::graph::GraphDoc, Vec<Vec<usize>>)>,
    error: Option<std::io::Error>,
}

/// The shared handle the spinner rider and the event stream both hold —
/// the heartbeat's `Arc<Mutex<…>>` precedent applied to the fold. On the
/// run's current-thread executor the two never contend for real (the
/// rider only runs at await points); the lock is the proof, not a hot
/// path.
pub(super) type SharedFold<W> = std::sync::Arc<std::sync::Mutex<FoldSink<W>>>;

/// The [`EventSink`] face of a shared fold — lock, apply, repaint. A
/// poisoned lock (another holder panicked) goes silent: the render is
/// best-effort by contract, the verdict never rides here.
pub(super) struct FoldHandle<W: Write>(pub(super) SharedFold<W>);

/// Root-execution decorator for every projection of the runtime stream.
///
/// Event IDs remain minted by the runtime stamper. This seam only attaches
/// the already-admitted execution identity before the event fans out to the
/// human, JSON, and journal lanes.
pub(super) struct ExecutionSink<S> {
    inner: S,
    execution: ExecutionId,
}

impl<S> ExecutionSink<S> {
    pub(super) const fn new(inner: S, execution: ExecutionId) -> Self {
        Self { inner, execution }
    }

    pub(super) fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: EventSink> EventSink for ExecutionSink<S> {
    fn emit(&mut self, event: Event) {
        self.inner.emit(event.with_execution(self.execution));
    }
}

impl<W: Write> EventSink for FoldHandle<W> {
    fn emit(&mut self, event: Event) {
        if let Ok(mut fold) = self.0.lock() {
            fold.emit(event);
        }
    }
}

/// The spinner rider — ticks the shared fold ~10×/s while the run sits
/// at an await point (exactly the provider/subprocess wait a frozen
/// frame misreads as a hang). The caller aborts it the moment the run
/// settles; the executor's drop reaps it regardless.
pub(super) fn spawn_spinner<W: Write + Send + 'static>(
    fold: SharedFold<W>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tick.tick().await; // the immediate first tick is not a frame
        loop {
            tick.tick().await;
            match fold.lock() {
                Ok(mut fold) => fold.spin(),
                Err(_) => return, // poisoned — best-effort silence
            }
        }
    })
}

impl<W: Write> FoldSink<W> {
    /// Wrap a writer + the resolved theme + the render mode. `Live` does the
    /// in-place repaint (TTY); `Plain`/`Quiet` fold silently for the caller's
    /// final-frame print.
    pub fn new(writer: W, theme: Theme, mode: RenderMode) -> Self {
        Self {
            writer,
            theme,
            view: RunView::new(),
            mode,
            outputs: false,
            trace_recorded: true,
            source_path: None,
            last_lines: 0,
            tick: 0,
            map: None,
            error: None,
        }
    }

    /// One spinner beat — advance the phase and repaint, but ONLY while
    /// something is actually running on the animated Live surface (a
    /// settled screen must not churn; sober registers never tick).
    pub(super) fn spin(&mut self) {
        if self.mode != RenderMode::Live
            || !self.theme.animate
            || self.error.is_some()
            || !self
                .view
                .rows()
                .iter()
                .any(|r| r.state == crate::TaskState::Running)
        {
            return;
        }
        self.tick = self.tick.wrapping_add(1);
        if let Err(e) = self.repaint() {
            self.error = Some(e);
        }
    }

    /// Enable the shape tails on completed rows (the interactive-TTY
    /// comprehension surface). Off by default — every existing register
    /// keeps its exact bytes until a caller opts in.
    pub fn show_outputs(&mut self, on: bool) {
        self.outputs = on;
    }

    /// Declare whether this run records a trace journal (default true) —
    /// the disabled-journal lanes (`examples run` · `--no-trace-file`)
    /// turn it off so the close never teaches a door to a file that was
    /// deliberately never written.
    pub fn set_trace_recorded(&mut self, on: bool) {
        self.trace_recorded = on;
    }

    /// Pin the workflow path so the fruit card can detect a try room.
    pub fn set_source_path(&mut self, path: impl Into<String>) {
        self.source_path = Some(path.into());
    }

    /// The folded view (the caller renders the FINAL frame + the failure
    /// card from it after the run · the verdict lives here).
    pub fn view(&self) -> &RunView {
        &self.view
    }

    /// Inject the static wave plan (task ids per wave · from the check
    /// report) — feeds the ∥ lane markers + the DAG-shape glyph. Side
    /// information only: the fold itself never changes.
    pub fn set_plan(&mut self, waves: Vec<Vec<String>>) {
        self.view.set_plan(waves);
    }

    /// Print the final frame once (the `Plain`/`Quiet` lanes · the caller
    /// calls this after the run · a no-op buffered error stays buffered).
    /// `Quiet` paints the compact verdict card; `Plain` CLOSES its
    /// narration (#321 — the rows spoke at settle · the meter + the
    /// failure card end the story, never a repeated storyboard).
    pub fn print_final(&mut self) {
        if self.error.is_some() {
            return;
        }
        let lines = match self.mode {
            RenderMode::Quiet => verdict_frame(&self.view, &self.theme),
            RenderMode::Thread => Vec::new(),
            // The plain close carries the FRUIT block (A-2): the files
            // the run materialized + the model's last word — composed
            // here (sizes are a stat, the display crate holds no I/O).
            RenderMode::Plain => stream_summary(
                &self.view,
                &self.theme,
                &super::epilogue::fruit_notes(
                    &self.view,
                    self.trace_recorded,
                    self.source_path
                        .as_deref()
                        .and_then(super::example::try_rehearsal_slug),
                ),
            ),
            _ if self.outputs => frame_with_outputs(&self.view, &self.theme, 0),
            _ => frame(&self.view, &self.theme, 0),
        };
        for line in &lines {
            if let Err(e) = writeln!(self.writer, "{line}") {
                self.error = Some(e);
                return;
            }
        }
        if let Err(e) = self.writer.flush() {
            self.error = Some(e);
        }
    }

    /// The buffered write error, if any.
    pub fn into_error(self) -> Option<std::io::Error> {
        self.error
    }

    /// Take the buffered write error out of a SHARED fold (the spinner
    /// arc keeps ownership behind the lock — `into_error`'s by-value
    /// twin for that seam).
    pub(super) fn take_error(&mut self) -> Option<std::io::Error> {
        self.error.take()
    }

    /// Inject the run's checked topology — the living map draws from the
    /// NEXT repaint on. Live surface only; sober modes never store it.
    pub fn set_map(&mut self, doc: crate::verbs::graph::GraphDoc, waves: Vec<Vec<usize>>) {
        if self.mode == RenderMode::Live && self.theme.accents {
            self.view.external_map = true;
            self.map = Some((doc, waves));
        }
    }

    /// The wire art with every node painted by its current state —
    /// `None` when no topology was injected or the wire law refuses the
    /// shape (the frame's own wave-column line covers that run).
    fn living_map(&self) -> Option<String> {
        let (doc, waves) = self.map.as_ref()?;
        let states: std::collections::BTreeMap<&str, &crate::display::state::TaskRow> = self
            .view
            .rows()
            .iter()
            .map(|r| (r.id.as_str(), r))
            .collect();
        let theme = self.theme;
        let tick = self.tick;
        // Zero-alloc live probe: the rows ARE the state — a 10 Hz repaint
        // asks, it never copies.
        let rows = self.view.rows();
        let running = |id: &str| {
            rows.iter()
                .any(|r| r.id == id && matches!(r.state, crate::display::state::TaskState::Running))
        };
        let graph = crate::wires::wire_graph(doc, waves);
        crate::wires::render_with(
            &graph,
            theme,
            &move |id, verb| {
                use crate::display::state::TaskState;
                let row = states.get(id).copied();
                match row.map(|r| &r.state) {
                    Some(TaskState::Running) => (
                        theme.verb_spin(Some(verb), tick),
                        theme.paint(crate::display::theme::Role::Strong, id),
                    ),
                    Some(TaskState::Ok) => (
                        theme.verb_glyph(verb),
                        theme.paint(crate::display::theme::Role::Good, id),
                    ),
                    Some(TaskState::Failed) => (
                        theme.verb_glyph(verb),
                        theme.paint(crate::display::theme::Role::Bad, id),
                    ),
                    Some(TaskState::Skipped | TaskState::Cancelled) => (
                        theme.paint(crate::display::theme::Role::Dim, "⊘ "),
                        theme.paint(crate::display::theme::Role::Dim, id),
                    ),
                    _ => (
                        theme.paint(crate::display::theme::Role::Dim, "· "),
                        theme.paint(crate::display::theme::Role::Dim, id),
                    ),
                }
            },
            Some((&running, tick)),
        )
    }

    fn repaint(&mut self) -> std::io::Result<()> {
        // Move the cursor up over the previous frame and clear from there
        // down (ANSI · the spinner family). TTY-only — `emit` gates this.
        // A REDRAW rides inside a DEC-2026 synchronized-output frame
        // (`CSI ?2026h … ?2026l`): kitty/WezTerm/Ghostty/Rio hold the
        // screen until the frame closes (no clear-then-paint flicker);
        // every other terminal ignores the pair (zero-cost no-op). The
        // FIRST paint appends without clearing — nothing can tear, so it
        // stays escape-free (the first-frame law the tests pin). A write
        // error mid-frame leaves the pair unclosed once, harmlessly: the
        // sink goes silent (buffered error) and DEC-2026 terminals
        // time the frame out on their own.
        let redraw = self.last_lines > 0;
        if redraw {
            write!(self.writer, "\x1b[?2026h")?;
            write!(self.writer, "\x1b[{}A", self.last_lines)?;
            write!(self.writer, "\x1b[0J")?;
        }
        let mut lines = if self.outputs {
            frame_with_outputs(&self.view, &self.theme, self.tick)
        } else {
            frame(&self.view, &self.theme, self.tick)
        };
        if let Some(art) = self.living_map() {
            // The map leads the frame — blank-separated, repainted with
            // the storyboard so the running node's motion turns in place.
            let mut led: Vec<String> = art.lines().map(str::to_owned).collect();
            led.push(String::new());
            led.extend(lines);
            lines = led;
        }
        for line in &lines {
            writeln!(self.writer, "{line}")?;
        }
        if redraw {
            write!(self.writer, "\x1b[?2026l")?;
        }
        self.last_lines = lines.len();
        self.writer.flush()
    }

    /// The plain narration (#321): the header the moment the workflow
    /// starts, one storyboard line the moment each task settles — the
    /// SAME content the final frame would show, streamed (zero cursor
    /// control · a piped consumer sees progress when it happens, and a
    /// local-model run never reads as a hang). Non-narrating events
    /// write nothing (no flush churn on dispatch/checkpoint frames).
    fn narrate(&mut self, event: &Event) -> std::io::Result<()> {
        use nika_event::EventKind;
        let lines: Vec<String> = match event.kind {
            EventKind::WorkflowStarted => stream_header(&self.view, &self.theme),
            EventKind::TaskCompleted
            | EventKind::TaskFailed
            | EventKind::TaskSkipped
            | EventKind::TaskCancelled
            | EventKind::TaskCacheHit => str_field(event, "task")
                .and_then(|task| stream_settled_line(&self.view, task, &self.theme, self.outputs))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        if lines.is_empty() {
            return Ok(());
        }
        for line in &lines {
            writeln!(self.writer, "{line}")?;
        }
        self.writer.flush()
    }
}

impl<W: Write> EventSink for FoldSink<W> {
    fn emit(&mut self, event: Event) {
        if self.error.is_some() {
            return;
        }
        self.view.apply(&event);
        // `Live` repaints in place (TTY cursor control) · `Plain`
        // narrates at settle (#321 · zero escapes) · `Quiet` folds
        // silently for the compact final card.
        let painted = match self.mode {
            RenderMode::Live => self.repaint(),
            RenderMode::Plain | RenderMode::Thread => self.narrate(&event),
            _ => Ok(()),
        };
        if let Err(e) = painted {
            self.error = Some(e);
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::demo;

    #[test]
    fn anchor_advertises_the_full_64_hex_head() {
        // #333 · byte-exact parity with `trace verify`: the taught loop
        // (compare the two printed heads) must close with `==`, so the
        // anchor carries the WHOLE sha256 hex — never a truncated form.
        let head = "a".repeat(64);
        let line = anchor_line(
            std::path::Path::new(".nika/traces/x.ndjson"),
            7,
            &head,
            false,
        );
        assert_eq!(
            line,
            format!("trace: .nika/traces/x.ndjson · 7 events · chain {head}")
        );
        assert!(
            line.ends_with(&head),
            "the head is printed whole, byte-comparable against trace verify's"
        );
    }

    /// #321 — the plain lane NARRATES: the header at workflow start, one
    /// storyboard line at each settle, the meter as the close — top to
    /// bottom ONCE (no repeated frame) · zero cursor escapes (the
    /// CI-capture contract) · a piped consumer sees progress live.
    #[test]
    fn fold_sink_plain_narrates_at_settle_then_closes_with_the_meter() {
        let theme = Theme::new(false, true, false);
        let mut buf = Vec::new();
        {
            let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Plain);
            // The run verb injects the plan BEFORE driving — the streamed
            // header counts from it (rows don't exist at header time).
            sink.set_plan(vec![
                vec!["fetch_top".to_owned()],
                vec!["extract_ai".to_owned()],
                vec!["summarize".to_owned()],
                vec!["write_md".to_owned(), "notify_slack".to_owned()],
            ]);
            for ev in demo::success() {
                sink.emit(ev);
            }
            assert_eq!(sink.view().verdict, Some(true), "success folded");
            sink.print_final();
            assert!(sink.into_error().is_none());
        }
        let text = String::from_utf8(buf).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();
        // The header opens the story, plan-counted, permits greeting next.
        assert!(
            lines[0].contains("veille-news · 5 tasks"),
            "header first, counted from the plan: {}",
            lines[0]
        );
        assert!(lines[1].contains("permits"), "the greeting: {}", lines[1]);
        // One line per settled task, in SETTLE order, exactly once each.
        assert_eq!(
            text.matches("fetch_top").count(),
            1,
            "a row speaks once — never a repeated storyboard: {text}"
        );
        let fetch = lines.iter().position(|l| l.contains("fetch_top"));
        let write = lines.iter().position(|l| l.contains("write_md"));
        let meter = lines.iter().position(|l| l.contains("done"));
        assert!(
            fetch < write && write < meter,
            "settle order, then the meter closes: {text}"
        );
        assert!(
            lines[meter.expect("meter")].contains("5/5 done"),
            "the close counts the run: {text}"
        );
        // ZERO cursor escapes — the CI-capture contract survives.
        assert!(
            !text.contains('\x1b'),
            "non-interactive lane leaks no ANSI: {text}"
        );
    }

    #[test]
    fn fold_sink_quiet_prints_only_the_verdict_card() {
        let theme = Theme::new(false, true, false);
        let mut buf = Vec::new();
        {
            let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Quiet);
            for ev in demo::success() {
                sink.emit(ev);
            }
            sink.print_final();
            assert!(sink.into_error().is_none());
        }
        let text = String::from_utf8(buf).expect("utf8");
        // Quiet = the verdict line only · NO per-task storyboard · no escapes.
        assert!(text.contains("veille-news"), "verdict line: {text}");
        assert!(
            !text.contains("fetch_top"),
            "quiet hides the per-task rows: {text}"
        );
        assert!(!text.contains('\x1b'), "quiet leaks no ANSI: {text}");
    }

    #[test]
    fn fold_sink_interactive_repaints_in_place() {
        let theme = Theme::new(false, false, false);
        let mut buf = Vec::new();
        {
            let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Live);
            for ev in demo::success() {
                sink.emit(ev);
            }
            assert!(sink.into_error().is_none());
        }
        let text = String::from_utf8(buf).expect("utf8");
        // The Live lane DOES use cursor control (the live redraw).
        assert!(text.contains('\x1b'), "Live repaints in place");
    }

    #[test]
    fn fold_sink_failure_keeps_the_false_verdict() {
        let theme = Theme::new(false, false, false);
        let mut buf = Vec::new();
        let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Plain);
        for ev in demo::failure() {
            sink.emit(ev);
        }
        assert_eq!(sink.view().verdict, Some(false));
    }

    /// The outputs knob is OPT-IN per sink: the same output-carrying
    /// stream paints tails only after `show_outputs(true)` — the piped
    /// (`Plain`) register stays byte-free of tails unless a caller
    /// explicitly asks (the run verb only ever asks for `Live`).
    #[test]
    fn fold_sink_tails_are_opt_in() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let theme = Theme::new(false, true, false);
        let completed = demo::bare_event(EventKind::TaskCompleted, 5)
            .with_field(KeyValue::new("task", Value::String("audit".into())))
            .with_field(KeyValue::new("output", Value::String("[1,2]".into())));
        let render = |opt_in: bool| {
            let mut buf = Vec::new();
            let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Plain);
            sink.show_outputs(opt_in);
            sink.emit(completed.clone());
            sink.print_final();
            String::from_utf8(buf).expect("utf8")
        };
        assert!(!render(false).contains("->"), "default: no tails");
        assert!(
            render(true).contains("-> [2] · 5B"),
            "opted in: the tail rides: {}",
            render(true)
        );
    }

    /// The repaint clears the PRIOR frame only when one exists (`last_lines >
    /// 0`). On the FIRST Live event there is nothing to clear, so no cursor-up
    /// escape is written — a mutated guard (`== 0` / `>= 0`) would emit a
    /// spurious `\x1b[0A` over an empty screen, so the first frame must be
    /// escape-free (theme has color off · the only ANSI here would be cursor
    /// control).
    #[test]
    fn fold_sink_live_first_frame_writes_no_cursor_jump() {
        let theme = Theme::new(false, false, false);
        let mut buf = Vec::new();
        let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Live);
        let events = demo::success();
        sink.emit(events[0].clone()); // the FIRST event · last_lines was 0
        let text = String::from_utf8(buf).expect("utf8");
        assert!(
            !text.contains('\x1b'),
            "no prior frame to clear → no cursor-up escape: {text:?}"
        );
    }

    /// Every Live REDRAW rides inside one DEC-2026 synchronized-output
    /// frame: `?2026h` opens BEFORE the cursor-up + clear, `?2026l`
    /// closes AFTER the last repainted line — the flicker-free contract
    /// on kitty/WezTerm/Ghostty (a no-op pair everywhere else). The
    /// FIRST paint (append-only) carries no pair.
    #[test]
    fn fold_sink_live_redraws_inside_synchronized_frames() {
        let theme = Theme::new(false, false, false);
        let mut buf = Vec::new();
        {
            let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Live);
            for ev in demo::success() {
                sink.emit(ev);
            }
            assert!(sink.into_error().is_none());
        }
        let text = String::from_utf8(buf).expect("utf8");
        let opens = text.matches("\x1b[?2026h").count();
        let closes = text.matches("\x1b[?2026l").count();
        let frames = demo::success().len();
        assert_eq!(opens, frames - 1, "every redraw opens a frame: {text:?}");
        assert_eq!(opens, closes, "every open closes");
        // The pair BRACKETS the redraw: the open is IMMEDIATELY followed
        // by the cursor-up escape (h before any clearing), and the very
        // last write closes the final frame (l after the repaint).
        assert!(
            text.contains("\x1b[?2026h\x1b["),
            "h opens before the cursor-up: {text:?}"
        );
        assert!(
            text.ends_with("\x1b[?2026l"),
            "the last write closes the frame"
        );
    }

    /// The spinner beat: while a task runs on the animated Live surface,
    /// `spin()` advances the braille phase and repaints; once everything
    /// settled it writes NOTHING (a finished screen never churns).
    #[test]
    fn spin_advances_the_running_glyph_and_stops_at_settle() {
        use crate::display::theme::ROUNDTRIP;
        let theme = Theme::new(false, false, true); // colour off · animated
        let mut fold = FoldSink::new(Vec::new(), theme, RenderMode::Live);
        // Feed the demo stream up to the FIRST running state — the
        // prefix length is the stream's business, not this test's.
        let events = demo::success();
        let mut fed = 0usize;
        for event in &events {
            fold.emit(event.clone());
            fed += 1;
            if fold
                .view()
                .rows()
                .iter()
                .any(|r| r.state == crate::TaskState::Running)
            {
                break;
            }
        }
        assert!(
            fold.view()
                .rows()
                .iter()
                .any(|r| r.state == crate::TaskState::Running),
            "the demo stream never ran a task?"
        );
        let before = fold.writer.len();
        fold.spin();
        fold.spin();
        let painted = String::from_utf8_lossy(&fold.writer[before..]).into_owned();
        // Two beats = two frames of the running verb's OWN motion (the
        // demo's running row is `invoke` → roundtrip, ticks 1 then 2).
        assert!(
            painted.contains(ROUNDTRIP[1]) && painted.contains(ROUNDTRIP[2]),
            "{painted:?}"
        );

        // Settle everything — the beat goes silent.
        for event in events.iter().skip(fed) {
            fold.emit(event.clone());
        }
        let settled = fold.writer.len();
        fold.spin();
        assert_eq!(fold.writer.len(), settled, "no churn after settle");
    }

    /// The sober lanes are tick-blind: Plain never repaints on spin.
    #[test]
    fn spin_is_a_no_op_off_the_live_lane() {
        let theme = Theme::new(false, false, true);
        let mut fold = FoldSink::new(Vec::new(), theme, RenderMode::Plain);
        for event in demo::success().iter().take(3) {
            fold.emit(event.clone());
        }
        let before = fold.writer.len();
        fold.spin();
        assert_eq!(fold.writer.len(), before, "Plain stays event-driven");
    }

    struct RefusingWriter(std::io::ErrorKind);

    impl Write for RefusingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(self.0, "injected note refusal"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn trace_note_errors_keep_the_finalized_path_on_both_streams() {
        let path = std::path::PathBuf::from(".nika/traces/exact.ndjson");
        for note in [TraceNote::Stdout, TraceNote::Stderr] {
            let mut stdout = RefusingWriter(std::io::ErrorKind::PermissionDenied);
            let mut stderr = RefusingWriter(std::io::ErrorKind::PermissionDenied);
            let write = write_trace_note(
                note,
                &mut stdout,
                &mut stderr,
                &path,
                "trace: exact",
                false,
                None,
            );
            let surfaced = TraceSurface::noted(Some(path.clone()), write);
            assert_eq!(surfaced.path, Some(path.clone()));
            assert_eq!(
                surfaced.note_error.map(|error| error.kind()),
                Some(std::io::ErrorKind::PermissionDenied)
            );
        }
    }
}

/// Where the run journal's `trace:` pointer lands (per lane).
#[derive(Clone, Copy)]
pub(super) enum TraceNote {
    /// Human storytelling surfaces (`Live` · `Plain`).
    Stdout,
    /// Machine lanes whose stdout is a byte-exact contract.
    Stderr,
    /// Quiet keeps the compact-card promise.
    Silent,
}
#[derive(Default)]
pub(super) struct TraceSurface {
    pub(super) path: Option<std::path::PathBuf>,
    pub(super) note_error: Option<std::io::Error>,
}

impl TraceSurface {
    fn noted(path: Option<std::path::PathBuf>, note: std::io::Result<()>) -> Self {
        Self {
            path,
            note_error: note.err(),
        }
    }
}

/// Surface the finalized journal. Fs failure remains a rider; note failure is
/// typed with the exact path (`BrokenPipe` → 141, other I/O → ENV).
/// `teardown` enters the seal.
pub(super) fn surface_trace(
    mut trace: TraceFileSink,
    note: TraceNote,
    autopsy: Option<&str>,
    workflow_hash: Option<&str>,
    teardown: Option<&nika_dap::seal::SealTeardown>,
    sensitive: bool,
) -> TraceSurface {
    // Seal before fsync so the signature is part of the durable chain.
    let sealed = nika_dap::journal::seal_journal_with(&mut trace, workflow_hash, teardown);
    trace.finalize();
    let path = trace.path().map(std::path::Path::to_path_buf);
    let head = trace.chain_head().to_owned();
    let count = trace.chain_len();
    if let Some(e) = trace.into_error() {
        let mut stderr = std::io::stderr().lock();
        let note = match &path {
            Some(p) => writeln!(
                stderr,
                "nika run: trace file {}: {e} — the run itself is unaffected",
                p.display()
            ),
            None => writeln!(
                stderr,
                "nika run: trace file: {e} — the run itself is unaffected"
            ),
        };
        return TraceSurface::noted(None, note);
    }
    let Some(path) = path else {
        return TraceSurface::default();
    };
    let anchor = anchor_line(&path, count, &head, sealed);
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let written = write_trace_note(
        note,
        &mut stdout,
        &mut stderr,
        &path,
        &anchor,
        sensitive,
        autopsy,
    );
    TraceSurface::noted(Some(path), written)
}

#[allow(clippy::too_many_arguments)]
fn write_trace_note(
    note: TraceNote,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    path: &std::path::Path,
    anchor: &str,
    sensitive: bool,
    autopsy: Option<&str>,
) -> std::io::Result<()> {
    match note {
        TraceNote::Stdout => {
            writeln!(stdout, "    {anchor}")?;
            if sensitive {
                writeln!(
                    stdout,
                    "    note: this trace keeps full task outputs in plaintext (sensitive data included) · retention is doctor's `traces` line · remove: nika trace rm {}",
                    path.display()
                )?;
            }
            if let Some(task) = autopsy {
                writeln!(
                    stdout,
                    "    autopsy: nika trace peek {} {task} · replay: nika trace replay {} · or F5 in VS Code",
                    path.display(),
                    path.display()
                )?;
            }
            stdout.flush()?;
        }
        TraceNote::Stderr => {
            writeln!(stderr, "nika run: {anchor}")?;
            stderr.flush()?;
        }
        TraceNote::Silent => {}
    }
    Ok(())
}

/// Full chain head, byte-exact with `nika trace verify` (#333).
fn anchor_line(path: &std::path::Path, count: usize, head: &str, sealed: bool) -> String {
    let proof = if sealed { " · sealed" } else { "" };
    format!(
        "trace: {} · {count} events · chain {head}{proof}",
        path.display()
    )
}
