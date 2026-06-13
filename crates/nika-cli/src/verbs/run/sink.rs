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

use crate::{RunView, Theme, frame};

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
    /// Lines painted by the previous frame (to clear before the redraw).
    last_lines: usize,
    error: Option<std::io::Error>,
}

impl<W: Write> FoldSink<W> {
    /// Wrap a writer + the resolved theme.
    pub fn new(writer: W, theme: Theme) -> Self {
        Self {
            writer,
            theme,
            view: RunView::new(),
            last_lines: 0,
            error: None,
        }
    }

    /// The folded view (the caller renders the FINAL frame + the failure
    /// card from it after the run · the verdict lives here).
    pub fn view(&self) -> &RunView {
        &self.view
    }

    /// The buffered write error, if any.
    pub fn into_error(self) -> Option<std::io::Error> {
        self.error
    }

    fn repaint(&mut self) -> std::io::Result<()> {
        // Move the cursor up over the previous frame and clear each line
        // (ANSI · the same family the spinner loop uses) — a non-TTY
        // writer still receives the escapes but they are harmless in a
        // captured log only when colour is off; the live lane gates on
        // IsTerminal upstream (the composer picks JsonSink for non-TTY).
        if self.last_lines > 0 {
            write!(self.writer, "\x1b[{}A", self.last_lines)?;
            write!(self.writer, "\x1b[0J")?; // clear from cursor down
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
        if let Err(e) = self.repaint() {
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
    fn fold_sink_tracks_the_verdict_and_paints() {
        let theme = Theme {
            color: false,
            ascii: true,
            animate: false,
        };
        let mut buf = Vec::new();
        {
            let mut sink = FoldSink::new(&mut buf, theme);
            for ev in demo::success() {
                sink.emit(ev);
            }
            assert_eq!(sink.view().verdict, Some(true), "success folded");
            assert!(sink.into_error().is_none());
        }
        // The final repaint carries the storyboard rows (ascii theme).
        let text = String::from_utf8(buf).expect("utf8");
        assert!(text.contains("fetch_top"), "the run painted: {text}");
    }

    #[test]
    fn fold_sink_failure_keeps_the_false_verdict() {
        let theme = Theme {
            color: false,
            ascii: false,
            animate: false,
        };
        let mut buf = Vec::new();
        let mut sink = FoldSink::new(&mut buf, theme);
        for ev in demo::failure() {
            sink.emit(ev);
        }
        assert_eq!(sink.view().verdict, Some(false));
    }
}
