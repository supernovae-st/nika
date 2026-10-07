// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The machine lane as a child: a door that owns the terminal (the
//! renderer's viewport) runs `nika run --json` as a child of the binary
//! with pipes only, so nothing the run prints reaches the terminal; each
//! frame becomes one line of the run's story and one typed frame, told to
//! the sink as it happens, the story kept for the block the transcript
//! commits; the exit code is the child's, the trace the settle frame's.
//! Every frame is bounded (1 MiB) by the one reader both paths share. A
//! size-cap member of the nika-cli unit hosts it (D-2026-07-09-N1 · ADR-110).

mod request;
pub use request::{RunHostOptions, resume_args, run_args, run_args_with_access};
mod review;
pub use review::{PendingRun, RunProgress, drive_reviewed_child, drive_reviewed_child_observed};

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// The renderer's run child, by pid, while it runs: the door that leaves
/// while a run is in flight ends it (SIGTERM: the engine cancels and the
/// trace says so) instead of leaving an orphan working in the dark.
pub type ChildSlot = std::sync::Arc<std::sync::Mutex<Option<u32>>>;

/// Run the lane as a child and fold its frames: (exit code, the trace the
/// settle named, the story). The child's pid rides `slot` while it runs
/// so the door that leaves can end it.
#[must_use]
pub fn drive_child(
    exe: &Path,
    args: &[String],
    root: &Path,
    busy: &Sender<String>,
    slot: &ChildSlot,
) -> (u8, Option<PathBuf>, Vec<String>) {
    drive_child_observed(exe, args, root, busy, slot)
}

/// [`drive_child`], the run told to `sink`: its story and its frames, typed.
/// An oversize frame, one that is not UTF-8 or a stream that cannot be read
/// ends the child and is said, never taken for a settled run.
#[must_use]
pub fn drive_child_observed(
    exe: &Path,
    args: &[String],
    root: &Path,
    sink: &dyn RunSink,
    slot: &ChildSlot,
) -> (u8, Option<PathBuf>, Vec<String>) {
    let mut child = match review::spawn(exe, args, root, slot, false) {
        Ok(child) => child,
        Err(result) => return result,
    };
    match child.read(sink, false) {
        Ok(_) => child.complete(),
        Err(why) => child.cut(&why),
    }
}

pub use nika_display::run_story::{RunFrame, RunSink, RunStory};

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The lane as a child: its stdout frames become the story (and reach
    /// the busy sink as they happen), its exit code is the child's, the
    /// trace is the settle's, the pid slot is cleared once it ends; a
    /// child that cannot start is the environment exit with its reason.
    #[test]
    fn drive_child_folds_a_real_child_and_reports_its_exit() {
        let (busy, heard) = std::sync::mpsc::channel();
        let slot: ChildSlot = std::sync::Arc::default();
        let script = concat!(
            "printf '%s\\n' '{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"w.nika\"}]}'",
            " '{\"kind\":\"task_scheduled\",\"fields\":[]}'",
            " '{\"kind\":\"task_completed\",\"fields\":[{\"key\":\"task\",\"value\":\"t\"},{\"key\":\"duration_ms\",\"value\":1}]}'",
            " '{\"kind\":\"run_settled\",\"receipt\":{\"trace_path\":\".nika/traces/x.ndjson\"}}'",
            "; printf 'noise on stderr\\n' >&2; exit 4"
        );
        let (code, trace, lines) = drive_child(
            Path::new("/bin/sh"),
            &["-c".to_owned(), script.to_owned()],
            Path::new("/"),
            &busy,
            &slot,
        );
        assert_eq!(code, 4, "the child's own exit");
        assert_eq!(trace.as_deref(), Some(Path::new(".nika/traces/x.ndjson")));
        assert_eq!(
            lines,
            vec!["running · w.nika".to_owned(), "✔ t · 1 ms · 1/1".to_owned()]
        );
        let heard: Vec<String> = heard.try_iter().collect();
        assert_eq!(
            heard, lines,
            "every line reached the busy sink as it happened"
        );
        assert!(
            slot.lock().expect("slot").is_none(),
            "no pid once the child ended"
        );
        let (code, trace, lines) = drive_child(
            Path::new("/nonexistent/nika-lane-binary"),
            &[],
            Path::new("/"),
            &busy,
            &slot,
        );
        assert_eq!(code, 3, "the environment exit");
        assert!(trace.is_none());
        assert!(
            lines
                .first()
                .is_some_and(|l| l.starts_with("the run could not start: ")),
            "{lines:?}"
        );
    }

    /// A sink that takes the frames too hears the very story lines a busy
    /// sender hears, and every machine frame typed: the runtime event, then
    /// the settlement that closes the stream, both naming their execution.
    #[test]
    fn drive_child_tells_a_typed_sink_the_frames_it_decoded() {
        use nika_display::run_story::{EventKind, RunState};
        #[derive(Default)]
        struct Typed {
            lines: std::sync::Mutex<Vec<String>>,
            frames: std::sync::Mutex<Vec<RunFrame>>,
        }
        impl RunSink for Typed {
            fn said(&self, line: String) {
                self.lines.lock().expect("lines").push(line);
            }
            fn frame(&self, frame: RunFrame) {
                self.frames.lock().expect("frames").push(frame);
            }
        }
        let script = concat!(
            "printf '%s\\n' ",
            "'{\"chain\":\"c1\",\"correlation\":null,\"execution\":{\"uuid\":\"01a0ef11-0212-70de-a8b3-99de9427fccc\"},",
            "\"fields\":[{\"key\":\"task\",\"value\":\"t\"},{\"key\":\"duration_ms\",\"value\":1}],",
            "\"id\":{\"uuid\":\"01a0ef11-03b2-71ee-9ad4-17a755fad3ae\"},\"kind\":\"task_completed\",\"run\":null,\"timestamp\":1}'",
            " '{\"kind\":\"run_settled\",\"status\":\"succeeded\",\"cause\":\"normal\",",
            "\"execution\":{\"uuid\":\"01a0ef11-0212-70de-a8b3-99de9427fccc\"},",
            "\"spend\":{\"priced_calls\":0,\"qualifier\":\"unmetered\",\"unpriced_calls\":0},\"evidence\":\"none\"}'"
        );
        let typed = Typed::default();
        let slot: ChildSlot = std::sync::Arc::default();
        let (code, _, lines) = drive_child_observed(
            Path::new("/bin/sh"),
            &["-c".to_owned(), script.to_owned()],
            Path::new("/"),
            &typed,
            &slot,
        );
        assert_eq!(code, 0);
        assert_eq!(lines, vec!["✔ t · 1 ms · 1/0".to_owned()]);
        assert_eq!(*typed.lines.lock().expect("lines"), lines);
        let frames = typed.frames.lock().expect("frames");
        assert!(
            matches!(
                frames.as_slice(),
                [RunFrame::Event(event), RunFrame::Settled(settled)]
                    if event.kind == EventKind::TaskCompleted
                        && settled.settlement.state == RunState::Succeeded
                        && event.execution.is_some()
                        && settled.execution == event.execution
            ),
            "{frames:?}"
        );
    }

    /// A frame over 1 MiB on the plain lane stops the reading: the child is
    /// ended and reaped (its pid leaves the slot), the story says the stream
    /// stopped and that effects may have happened, and the exit is the
    /// environment's, never the child's success.
    #[test]
    fn an_oversize_frame_ends_the_child_and_is_said() {
        let (busy, _heard) = std::sync::mpsc::channel();
        let slot: ChildSlot = std::sync::Arc::default();
        let script = "head -c 1048600 /dev/zero | tr '\\0' 'a'; echo; exec sleep 30";
        let started = std::time::Instant::now();
        let (code, trace, lines) = drive_child(
            Path::new("/bin/sh"),
            &["-c".to_owned(), script.to_owned()],
            Path::new("/"),
            &busy,
            &slot,
        );
        assert_eq!(code, 3);
        assert!(trace.is_none());
        assert!(
            lines.last().is_some_and(
                |l| l.contains("Run frame exceeds 1 MiB") && l.contains("may have had effects")
            ),
            "{lines:?}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "the child was ended, not waited for"
        );
        assert!(slot.lock().expect("slot").is_none(), "reaped");
    }

    /// A frame that is not UTF-8 is refused like an oversize one, never
    /// repaired: what a review answers must be the child's exact bytes.
    #[test]
    fn a_frame_that_is_not_utf8_ends_the_child_and_is_said() {
        let (busy, _heard) = std::sync::mpsc::channel();
        let slot: ChildSlot = std::sync::Arc::default();
        let script = "printf '\\377\\376 not text\\n'; exec sleep 30";
        let (code, trace, lines) = drive_child(
            Path::new("/bin/sh"),
            &["-c".to_owned(), script.to_owned()],
            Path::new("/"),
            &busy,
            &slot,
        );
        assert_eq!((code, trace), (3, None));
        assert!(
            lines.last().is_some_and(
                |l| l.contains("Run frame is not UTF-8") && l.contains("may have had effects")
            ),
            "{lines:?}"
        );
        assert!(slot.lock().expect("slot").is_none(), "reaped");
    }
}
