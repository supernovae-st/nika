// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The machine lane as a child: a door that owns the terminal (the
//! renderer's viewport) runs `nika run --json` as a child of the binary
//! with pipes only, so nothing the run prints reaches the terminal; each
//! frame becomes one line of the run's story, handed to a busy sink as it
//! happens and kept for the block the transcript commits; the exit code
//! is the child's, the trace the settle frame's. A size-cap member of the
//! nika-cli unit hosts it (D-2026-07-09-N1 · ADR-110).

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

/// The run's story, folded from the machine lane's frames: one short
/// line per task settle, the header, the summary, the pause — the words
/// the busy row shows and the transcript keeps.
#[derive(Default)]
pub struct RunStory {
    /// Every line said so far, for the block the transcript commits.
    pub lines: Vec<String>,
    /// The trace the settle frame named.
    pub trace: Option<PathBuf>,
    total: usize,
    done: usize,
}

impl RunStory {
    /// One frame; the line it adds to the story, if any.
    pub fn frame(&mut self, line: &str) -> Option<String> {
        let frame: serde_json::Value = serde_json::from_str(line).ok()?;
        let kind = frame.get("kind")?.as_str()?;
        let field = |key: &str| -> Option<String> {
            frame
                .get("fields")?
                .as_array()?
                .iter()
                .find(|f| f.get("key").and_then(|k| k.as_str()) == Some(key))?
                .get("value")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
        };
        let said = match kind {
            "workflow_started" => format!("running · {}", field("workflow").unwrap_or_default()),
            "task_scheduled" => {
                self.total += 1;
                return None;
            }
            "task_started" => format!(
                "→ {} · {}",
                field("task").unwrap_or_default(),
                field("note").unwrap_or_default()
            ),
            "task_completed" => {
                self.done += 1;
                format!(
                    "✔ {} · {} ms · {}/{}",
                    field("task").unwrap_or_default(),
                    field("duration_ms").unwrap_or_default(),
                    self.done,
                    self.total
                )
            }
            "task_cache_hit" => {
                self.done += 1;
                format!("↺ {} · from the cache", field("task").unwrap_or_default())
            }
            "task_failed" => format!(
                "✖ {} · {}",
                field("task").unwrap_or_default(),
                field("detail")
                    .unwrap_or_default()
                    .lines()
                    .next()
                    .unwrap_or_default()
            ),
            "task_skipped" => format!("· {} skipped", field("task").unwrap_or_default()),
            "task_cancelled" => format!("· {} cancelled", field("task").unwrap_or_default()),
            "workflow_paused" => format!(
                "◇ paused · `{}` asks you",
                field("task").unwrap_or_default()
            ),
            "workflow_completed" | "workflow_failed" | "workflow_cancelled" => format!(
                "{} · {}/{} tasks · {} ms",
                field("status").unwrap_or_else(|| kind.to_owned()),
                field("tasks_ok").unwrap_or_default(),
                field("tasks_total").unwrap_or_default(),
                field("elapsed_ms").unwrap_or_default()
            ),
            "run_settled" => {
                self.trace = frame
                    .get("receipt")
                    .and_then(|r| r.get("trace_path"))
                    .and_then(|p| p.as_str())
                    .map(PathBuf::from);
                return None;
            }
            _ => return None,
        };
        self.lines.push(said.clone());
        Some(said)
    }
}
