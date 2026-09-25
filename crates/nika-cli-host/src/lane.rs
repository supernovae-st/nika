// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The machine lane as a child: a door that owns the terminal (the
//! renderer's viewport) runs `nika run --json` as a child of the binary
//! with pipes only, so nothing the run prints reaches the terminal; each
//! frame becomes one line of the run's story, handed to a busy sink as it
//! happens and kept for the block the transcript commits; the exit code
//! is the child's, the trace the settle frame's. A size-cap member of the
//! nika-cli unit hosts it (D-2026-07-09-N1 · ADR-110).

mod request;
pub use request::{RunHostOptions, resume_args, run_args};
mod review;
pub use review::{PendingRun, RunProgress, drive_reviewed_child};

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// The environment exit code (spec §4): the lane could not start.
const ENV: u8 = 3;

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
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return (ENV, None, vec![format!("no executor for the run: {error}")]),
    };
    runtime.block_on(child_story(exe, args, root, busy, slot))
}

/// The child's frames, one story line each, until it settles.
async fn child_story(
    exe: &Path,
    args: &[String],
    root: &Path,
    busy: &Sender<String>,
    slot: &ChildSlot,
) -> (u8, Option<PathBuf>, Vec<String>) {
    use tokio::io::AsyncBufReadExt as _;
    let mut child = match tokio::process::Command::new(exe)
        .args(args)
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return (ENV, None, vec![format!("the run could not start: {error}")]);
        }
    };
    if let Ok(mut guard) = slot.lock() {
        *guard = child.id();
    }
    let mut story = RunStory::default();
    if let Some(stdout) = child.stdout.take() {
        let mut lines = tokio::io::BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(said) = story.frame(&line) {
                let _ = busy.send(said);
            }
        }
    }
    let status = child.wait().await;
    if let Ok(mut guard) = slot.lock() {
        *guard = None;
    }
    let code = match status {
        Ok(status) => status
            .code()
            .and_then(|c| u8::try_from(c).ok())
            .unwrap_or(ENV),
        Err(_) => ENV,
    };
    (code, story.trace, story.lines)
}

pub use nika_display::run_story::RunStory;

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
        assert_eq!(code, ENV);
        assert!(trace.is_none());
        assert!(
            lines
                .first()
                .is_some_and(|l| l.starts_with("the run could not start: ")),
            "{lines:?}"
        );
    }
}
