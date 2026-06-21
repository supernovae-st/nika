// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The production [`EventSink`] lanes — `--json` (NDJSON verbatim) and
//! the live TTY fold (`RunView` repaint per event).
//!
//! Both are consumers of the SAME stream (the fold law · spec §3): the
//! sink shape decides the surface, never the runtime. The sink contract
//! is INFALLIBLE (a write error never changes the run's verdict — it is
//! buffered and surfaced at the end).

use std::io::Write;

use nika_event::Event;
use nika_runtime::EventSink;

use crate::{RunView, Theme, frame, verdict_frame};

/// How the live fold reaches the terminal (spec §3.5 reduced surfaces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderMode {
    /// Rich TTY: in-place repaint per event (cursor control · the default).
    Live,
    /// Plain: silent fold, ONE final storyboard frame (no animation · no
    /// cursor escapes) — `--no-progress` and the piped/CI default.
    Plain,
    /// Quiet: silent fold, the COMPACT verdict card only (errors always) —
    /// `--quiet`.
    Quiet,
}

/// Writes one NDJSON line per event to the wrapped writer (the `--json`
/// lane · "NDJSON events verbatim · CI/agents" · spec §3). Never
/// coloured. Flushes per event so a tailing agent sees liveness.
pub struct JsonSink<W: Write> {
    writer: W,
    /// The first write error, buffered (the sink contract is infallible
    /// w.r.t. the run · the caller checks this after the run).
    error: Option<std::io::Error>,
}

impl<W: Write> JsonSink<W> {
    /// Wrap a writer (typically `io::stdout().lock()`).
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            error: None,
        }
    }

    /// The buffered write error, if delivery ever failed.
    pub fn into_error(self) -> Option<std::io::Error> {
        self.error
    }
}

impl<W: Write> EventSink for JsonSink<W> {
    fn emit(&mut self, event: Event) {
        if self.error.is_some() {
            return; // already broken · stop touching a dead pipe
        }
        // serde_json::to_writer never fails on a valid Event (only the
        // writer can) — fold any error into the buffer.
        let result = serde_json::to_writer(&mut self.writer, &event)
            .map_err(std::io::Error::from)
            .and_then(|()| self.writer.write_all(b"\n"))
            .and_then(|()| self.writer.flush());
        if let Err(e) = result {
            self.error = Some(e);
        }
    }
}

/// Folds each event into a [`RunView`] and repaints the frame (the live
/// TTY lane · spec §3). v0 is event-driven repaint only (no spinner
/// ticks · those arrive with a timed polish pass) — every event clears
/// the prior frame and redraws, so the screen tracks the run.
pub struct FoldSink<W: Write> {
    writer: W,
    theme: Theme,
    view: RunView,
    /// The surface this sink paints (spec §3.5) — `Live` repaints in place
    /// (TTY only · cursor control), `Plain`/`Quiet` fold silently and the
    /// caller prints ONE final frame (no escape noise in a captured log).
    mode: RenderMode,
    /// Lines painted by the previous frame (to clear before the redraw).
    last_lines: usize,
    error: Option<std::io::Error>,
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
            last_lines: 0,
            error: None,
        }
    }

    /// The folded view (the caller renders the FINAL frame + the failure
    /// card from it after the run · the verdict lives here).
    pub fn view(&self) -> &RunView {
        &self.view
    }

    /// Print the final frame once (the `Plain`/`Quiet` lanes · the caller
    /// calls this after the run · a no-op buffered error stays buffered).
    /// `Quiet` paints the compact verdict card; `Plain` the full storyboard.
    pub fn print_final(&mut self) {
        if self.error.is_some() {
            return;
        }
        let lines = match self.mode {
            RenderMode::Quiet => verdict_frame(&self.view, &self.theme),
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

    fn repaint(&mut self) -> std::io::Result<()> {
        // Move the cursor up over the previous frame and clear from there
        // down (ANSI · the spinner family). TTY-only — `emit` gates this.
        if self.last_lines > 0 {
            write!(self.writer, "\x1b[{}A", self.last_lines)?;
            write!(self.writer, "\x1b[0J")?;
        }
        let lines = frame(&self.view, &self.theme, 0);
        for line in &lines {
            writeln!(self.writer, "{line}")?;
        }
        self.last_lines = lines.len();
        self.writer.flush()
    }
}

impl<W: Write> EventSink for FoldSink<W> {
    fn emit(&mut self, event: Event) {
        if self.error.is_some() {
            return;
        }
        self.view.apply(&event);
        // Only `Live` repaints in place; `Plain`/`Quiet` fold silently (the
        // caller prints one final frame) — never leak cursor escapes into a
        // captured log.
        if self.mode == RenderMode::Live
            && let Err(e) = self.repaint()
        {
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
    fn json_sink_writes_one_ndjson_line_per_event() {
        let events = demo::success();
        let n = events.len();
        let mut buf = Vec::new();
        {
            let mut sink = JsonSink::new(&mut buf);
            for ev in &events {
                sink.emit(ev.clone());
            }
            assert!(sink.into_error().is_none(), "the vec writer never fails");
        }
        let text = String::from_utf8(buf).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), n, "one line per event");
        // Every line is a standalone JSON document (the NDJSON contract).
        for line in lines {
            let _: serde_json::Value =
                serde_json::from_str(line).expect("each line is one JSON Event");
        }
    }

    #[test]
    fn json_sink_is_never_coloured() {
        let mut buf = Vec::new();
        let mut sink = JsonSink::new(&mut buf);
        for ev in demo::failure() {
            sink.emit(ev);
        }
        let text = String::from_utf8(buf).expect("utf8");
        assert!(!text.contains('\x1b'), "--json carries zero ANSI escapes");
    }

    #[test]
    fn fold_sink_non_interactive_folds_silent_then_prints_one_frame() {
        let theme = Theme {
            color: false,
            ascii: true,
            animate: false,
        };
        let mut buf = Vec::new();
        {
            // Plain: no per-event repaint · one final storyboard frame.
            let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Plain);
            for ev in demo::success() {
                sink.emit(ev);
            }
            assert_eq!(sink.view().verdict, Some(true), "success folded");
            sink.print_final();
            assert!(sink.into_error().is_none());
        }
        let text = String::from_utf8(buf).expect("utf8");
        // Exactly ONE frame · the storyboard rows · ZERO cursor escapes.
        assert!(text.contains("fetch_top"), "the run painted: {text}");
        assert!(
            !text.contains('\x1b'),
            "non-interactive lane leaks no ANSI: {text}"
        );
    }

    #[test]
    fn fold_sink_quiet_prints_only_the_verdict_card() {
        let theme = Theme {
            color: false,
            ascii: true,
            animate: false,
        };
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
        let theme = Theme {
            color: false,
            ascii: false,
            animate: false,
        };
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
        let theme = Theme {
            color: false,
            ascii: false,
            animate: false,
        };
        let mut buf = Vec::new();
        let mut sink = FoldSink::new(&mut buf, theme, RenderMode::Plain);
        for ev in demo::failure() {
            sink.emit(ev);
        }
        assert_eq!(sink.view().verdict, Some(false));
    }
}
