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
        // A run refused before its first frame speaks two other shapes on
        // the same stream: the check verdict document (`clean: false` and
        // its findings) and the error envelope (`{"error": {…}}`). Each
        // is one story line naming the reason — never a silent exit.
        let Some(kind) = frame.get("kind").and_then(serde_json::Value::as_str) else {
            let said = refusal_line(&frame)?;
            self.lines.push(said.clone());
            return Some(said);
        };
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

/// The one line a pre-run refusal document yields: the first finding of
/// a check verdict (with the count of the others), or the envelope's
/// message. `None` for any other kind-less object (the story ignores it).
fn refusal_line(frame: &serde_json::Value) -> Option<String> {
    if let Some(findings) = frame.get("findings").and_then(serde_json::Value::as_array) {
        if frame.get("clean").and_then(serde_json::Value::as_bool) == Some(true) {
            return None;
        }
        let finding = findings.iter().find(|f| f.get("message").is_some())?;
        let message = finding.get("message")?.as_str()?;
        let message = message.lines().next().unwrap_or_default();
        let code = finding
            .get("code")
            .and_then(serde_json::Value::as_str)
            .map_or(String::new(), |c| format!("[{c}] "));
        let more = findings.len().saturating_sub(1);
        return Some(if more == 0 {
            format!("✖ refused before the start · {code}{message}")
        } else {
            format!(
                "✖ refused before the start · {code}{message} · {more} more finding(s) — `nika check` lists them"
            )
        });
    }
    let error = frame.get("error")?;
    let message = error
        .get("message")
        .and_then(serde_json::Value::as_str)
        .or_else(|| error.as_str())
        .map_or_else(
            || error.to_string(),
            |m| m.lines().next().unwrap_or_default().to_owned(),
        );
    Some(format!("✖ refused · {message}"))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The story folds the lane's frames into one line each (the header,
    /// a task start, a settle with its count, a pause), keeps the trace
    /// the settle names, and says nothing for a frame it does not tell.
    #[test]
    fn the_story_folds_the_frames_and_keeps_the_trace() {
        let mut story = RunStory::default();
        assert_eq!(
            story.frame(r#"{"kind":"workflow_started","fields":[{"key":"workflow","value":"copy.nika"}]}"#).as_deref(),
            Some("running · copy.nika")
        );
        assert!(
            story
                .frame(r#"{"kind":"task_scheduled","fields":[{"key":"task","value":"a"}]}"#)
                .is_none()
        );
        assert!(
            story
                .frame(r#"{"kind":"task_scheduled","fields":[{"key":"task","value":"b"}]}"#)
                .is_none()
        );
        assert_eq!(
            story.frame(r#"{"kind":"task_started","fields":[{"key":"task","value":"a"},{"key":"note","value":"invoke · nika:read"}]}"#).as_deref(),
            Some("→ a · invoke · nika:read")
        );
        assert_eq!(
            story.frame(r#"{"kind":"task_completed","fields":[{"key":"task","value":"a"},{"key":"duration_ms","value":3}]}"#).as_deref(),
            Some("✔ a · 3 ms · 1/2")
        );
        assert_eq!(
            story
                .frame(r#"{"kind":"workflow_paused","fields":[{"key":"task","value":"approve"}]}"#)
                .as_deref(),
            Some("◇ paused · `approve` asks you")
        );
        assert!(
            story
                .frame(r#"{"kind":"permit_checked","fields":[]}"#)
                .is_none()
        );
        assert!(story.frame("not json at all").is_none());
        assert!(
            story
                .frame(r#"{"kind":"run_settled","receipt":{"trace_path":".nika/traces/t.ndjson"}}"#)
                .is_none()
        );
        assert_eq!(
            story.trace.as_deref(),
            Some(Path::new(".nika/traces/t.ndjson"))
        );
        assert_eq!(story.lines.len(), 4, "{:?}", story.lines);
    }

    /// A run refused before its first frame — the check verdict document
    /// or the error envelope on the same stream — is one story line that
    /// names the reason; a clean document and an unrelated object say nothing.
    #[test]
    fn a_refusal_before_the_start_names_its_reason() {
        let mut story = RunStory::default();
        let check = r#"{"clean":false,"findings":[{"code":"NIKA-AUTH-006","message":"invoke `nika:read` with a literal path under an absent `permits:` block (task `t`) — fix: add \"nika:read\" to permits.tools\nsecond line","severity":"error"},{"code":"NIKA-DRIFT-001","message":"x","severity":"warning"}],"report_version":1}"#;
        assert_eq!(
            story.frame(check).as_deref(),
            Some(
                "✖ refused before the start · [NIKA-AUTH-006] invoke `nika:read` with a literal path under an absent `permits:` block (task `t`) — fix: add \"nika:read\" to permits.tools · 1 more finding(s) — `nika check` lists them"
            )
        );
        let parse = r#"{"clean":false,"findings":[{"gate":"PARSE","kind":"parse","message":"cannot read missing.nika: ENOENT","severity":"error"}],"parse_fatal":true}"#;
        assert_eq!(
            story.frame(parse).as_deref(),
            Some("✖ refused before the start · cannot read missing.nika: ENOENT")
        );
        let envelope = r#"{"error":{"code":"NIKA-1709","message":"NIKA-1709 · refusing to start: the cost floor $0.01 exceeds --max-cost-usd $0.000001"}}"#;
        assert_eq!(
            story.frame(envelope).as_deref(),
            Some(
                "✖ refused · NIKA-1709 · refusing to start: the cost floor $0.01 exceeds --max-cost-usd $0.000001"
            )
        );
        assert!(story.frame(r#"{"clean":true,"findings":[]}"#).is_none());
        assert!(story.frame(r#"{"unrelated":1}"#).is_none());
        assert_eq!(story.lines.len(), 3);
    }

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
