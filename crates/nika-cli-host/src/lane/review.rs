// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A live child retained across a fresh Run question. Dropping it cancels it.
//! The same bounded reader folds the plain lane child ([`super::drive_child`]).
// This established host lane owns child process creation and pipes.
#![allow(clippy::disallowed_types)]
use super::{ChildSlot, RunSink, RunStory};
use nika_providers::admission::CostChallenge;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::io::{BufRead as _, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub(super) type RunResult = (u8, Option<PathBuf>, Vec<String>);
/// The most bytes one frame of the child may hold.
const FRAME_CAP: u64 = 1_048_576;
/// What an oversize frame leaves: not shown here. The run's trace keeps it,
/// its values beside the journal (NIK-131).
const OVERSIZE: &str = "a step's output over 1 MiB is not shown here: the run goes on, and its \
                        trace keeps the value beside its journal";
/// The most of the child's stderr kept: its first lines say why a run stopped before its first
/// frame; the rest is drained unread so the child never blocks on a full pipe.
const DIAGNOSTICS_CAP: usize = 8_192;
/// The lines and the length of one line of that reason a run's story carries.
const REASON_LINES: usize = 4;
const REASON_WIDTH: usize = 400;

/// How long a completed run's story waits for its stderr reader to reach the pipe's end.
const DIAGNOSTICS_GRACE: std::time::Duration = std::time::Duration::from_millis(500);

/// What the child wrote on stderr, up to [`DIAGNOSTICS_CAP`], as it arrives, and whether the
/// reader reached the pipe's end.
type Diagnostics = Arc<Mutex<(Vec<u8>, bool)>>;

/// Only the live parent owns this non-cloneable, non-serializable child.
#[non_exhaustive]
pub enum RunProgress {
    Complete(RunResult),
    Review(Box<PendingRun>),
}
/// Observation plus an open one-use reply pipe. This is not a saved decision.
#[non_exhaustive]
pub struct PendingRun {
    child: Child,
    output: std::io::BufReader<ChildStdout>,
    story: RunStory,
    slot: ChildSlot,
    challenge: Option<CostChallenge>,
    diagnostics: Diagnostics,
}
impl Drop for PendingRun {
    fn drop(&mut self) {
        let _ = self.slot.lock().map(|mut slot| slot.take()); // before the reap: no reused pid
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl PendingRun {
    /// Whether SIGINT reached the child in `slot`, sent under the lock it leaves before its reap.
    #[must_use]
    pub fn interrupt(slot: &ChildSlot) -> bool {
        let Ok(held) = slot.lock() else { return false };
        held.and_then(|pid| i32::try_from(pid).ok())
            .filter(|pid| *pid > 1)
            .is_some_and(|pid| kill(Pid::from_raw(pid), Signal::SIGINT).is_ok())
    }
    /// The first screen of the fresh Run question.
    #[must_use]
    pub fn question(&self) -> String {
        self.challenge
            .as_ref()
            .map_or_else(String::new, CostChallenge::display)
    }
    /// The complete evidence of the same live challenge. It writes nothing to
    /// the child: the nonce and the one-use reply stay exactly as displayed.
    #[must_use]
    pub fn details(&self) -> String {
        self.challenge
            .as_ref()
            .map_or_else(String::new, CostChallenge::details)
    }
    /// Exactly one fresh submitted human answer. EOF seals the whole response.
    /// Even `yes` cannot expand or replace any part of the child's challenge.
    #[must_use]
    pub fn answer(self, yes: bool, busy: &Sender<String>) -> RunResult {
        self.answer_observed(yes, busy)
    }
    /// [`Self::answer`], the run told to `sink`: its story and its frames, typed.
    #[must_use]
    pub fn answer_observed(mut self, yes: bool, sink: &dyn RunSink) -> RunResult {
        let result = (|| {
            let challenge = self.challenge.take().ok_or("no pending Run review")?;
            let response =
                serde_json::to_vec(&challenge.response(yes)).map_err(|e| e.to_string())?;
            let mut input = self.child.stdin.take().ok_or("Run reply channel closed")?;
            input.write_all(&response).map_err(|e| e.to_string())?;
            drop(input);
            self.read(sink, false)
        })();
        match result {
            Ok(false) => self.complete(),
            Ok(true) => self.refuse("duplicate Run review"),
            Err(why) => self.refuse(&why),
        }
    }
    /// Fold the child's frames until EOF (`false`) or a review question
    /// (`true`), each frame bounded; one that is not UTF-8 (never repaired:
    /// the review answers the exact bytes) or a read error stops. A frame past
    /// the cap is drained unread and the run goes on (live gauntlet,
    /// 2026-10-15: a task that fetched three job boards, more than 1 MiB of
    /// output, stopped the run itself). The child's trace keeps that frame,
    /// its large values beside the journal (NIK-131).
    pub(super) fn read(&mut self, sink: &dyn RunSink, allow_review: bool) -> Result<bool, String> {
        loop {
            let mut line = Vec::new();
            // Bound a child frame without changing the normal machine lane protocol.
            let size = self
                .output
                .by_ref()
                .take(FRAME_CAP + 1)
                .read_until(b'\n', &mut line)
                .map_err(|e| e.to_string())?;
            if size == 0 {
                return Ok(false);
            }
            if size as u64 > FRAME_CAP {
                if !line.ends_with(b"\n") {
                    self.output.skip_until(b'\n').map_err(|e| e.to_string())?;
                }
                sink.unread(OVERSIZE);
                continue;
            }
            let line = String::from_utf8(line).map_err(|_| "Run frame is not UTF-8")?;
            if serde_json::from_str::<serde_json::Value>(&line)
                .ok()
                .and_then(|v| v.get("schema").and_then(|s| s.as_str()).map(str::to_owned))
                .is_some_and(|s| s.starts_with("nika/run-cost-challenge"))
            {
                if !allow_review {
                    return Err("duplicate Run cost question".into());
                }
                self.challenge = Some(CostChallenge::parse(&line)?);
                return Ok(true);
            }
            self.story.tell(&line, sink);
        }
    }
    pub(super) fn complete(&mut self) -> RunResult {
        let _ = self.slot.lock().map(|mut slot| slot.take()); // before the reap: no reused pid
        let code = self
            .child
            .wait()
            .ok()
            .and_then(|s| s.code())
            .and_then(|c| u8::try_from(c).ok())
            .unwrap_or(3);
        // A run that stopped before saying why on its stream (an access path refused before the
        // first frame, NIKA-1800, writes its reason on stderr alone) says it from there: live
        // gauntlet, 2026-10-16, the person read only « exit 3 · refused by the environment ».
        if !matches!(code, 0 | 4 | 130)
            && self.story.lines.is_empty()
            && let Some(reason) = self.reason()
        {
            self.story.lines.push(reason);
        }
        (
            code,
            self.story.trace.take(),
            std::mem::take(&mut self.story.lines),
        )
    }
    /// The first lines the child wrote on stderr, as one story line; `None` when it wrote none.
    /// The child has exited: its reader gets a short grace to reach the pipe's end, never a wait
    /// on a process that inherited the pipe.
    fn reason(&self) -> Option<String> {
        let started = std::time::Instant::now();
        while started.elapsed() < DIAGNOSTICS_GRACE
            && self.diagnostics.lock().is_ok_and(|kept| !kept.1)
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let kept = self.diagnostics.lock().ok()?;
        let text = String::from_utf8_lossy(&kept.0);
        let lines: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(REASON_LINES)
            .map(|line| line.chars().take(REASON_WIDTH).collect())
            .collect();
        (!lines.is_empty())
            .then(|| format!("the run stopped before it started: {}", lines.join(" · ")))
    }
    fn refuse(&mut self, why: &str) -> RunResult {
        self.story.lines.push(format!(
            "Run cost review refused: {why}; no automatic retry"
        ));
        (3, None, std::mem::take(&mut self.story.lines))
    }
    /// The plain lane's stream stopped (an oversize frame, a read error): the
    /// child is ended by the drop that follows, and the run may already have
    /// had effects. The trace it named, if any, is kept.
    pub(super) fn cut(&mut self, why: &str) -> RunResult {
        self.story.lines.push(format!(
            "the run's stream stopped: {why} · the run was ended; it may have had effects before · no automatic retry"
        ));
        (
            3,
            self.story.trace.take(),
            std::mem::take(&mut self.story.lines),
        )
    }
}
/// Start `exe args` in `root` with stdout piped, its pid in `slot`: the
/// review question's reply pipe on stdin when `review`, else no stdin.
pub(super) fn spawn(
    exe: &Path,
    args: &[String],
    root: &Path,
    slot: &ChildSlot,
    review: bool,
) -> Result<PendingRun, RunResult> {
    let mut command = Command::new(exe);
    command.args(args);
    if review {
        command.arg("--cost-review-stdio").stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let spawn = (command.current_dir(root))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = spawn.map_err(|e| (3, None, vec![format!("the run could not start: {e}")]))?;
    let Some(output) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err((3, None, vec!["Run stdout unavailable".into()]));
    };
    let diagnostics = child
        .stderr
        .take()
        .map(keep_diagnostics)
        .unwrap_or_default();
    if let Ok(mut value) = slot.lock() {
        *value = Some(child.id());
    }
    Ok(PendingRun {
        child,
        output: std::io::BufReader::new(output),
        story: RunStory::default(),
        slot: slot.clone(),
        challenge: None,
        diagnostics,
    })
}

/// Keep the first [`DIAGNOSTICS_CAP`] bytes the child writes on stderr and drain the rest, on a
/// reader of its own: the lane reads stdout, and a child whose stderr pipe filled would block.
/// The reader ends at the pipe's end; the run's completion reads what it kept by then, never
/// waiting on a process that inherited the pipe.
#[allow(clippy::disallowed_methods)] // a blocking pipe reader beside the lane's own loop, not workflow scheduling
fn keep_diagnostics(mut stderr: ChildStderr) -> Diagnostics {
    let kept = Diagnostics::default();
    let writer = Arc::clone(&kept);
    std::thread::spawn(move || {
        let mut chunk = [0_u8; 4_096];
        while let Ok(read) = stderr.read(&mut chunk) {
            if read == 0 {
                break;
            }
            if let Ok(mut kept) = writer.lock() {
                let room = DIAGNOSTICS_CAP.saturating_sub(kept.0.len());
                kept.0.extend_from_slice(&chunk[..read.min(room)]);
            }
        }
        if let Ok(mut kept) = writer.lock() {
            kept.1 = true;
        }
    });
    kept
}
/// Explicitly negotiate v1 with this binary's local Run. Normal frames still fold
/// through `RunStory`, and the existing PID slot keeps terminal-exit cancellation.
#[must_use]
pub fn drive_reviewed_child(
    exe: &Path,
    args: &[String],
    root: &Path,
    busy: &Sender<String>,
    slot: &ChildSlot,
) -> RunProgress {
    drive_reviewed_child_observed(exe, args, root, busy, slot)
}
/// [`drive_reviewed_child`], the run told to `sink`: its story and its frames, typed.
#[must_use]
pub fn drive_reviewed_child_observed(
    exe: &Path,
    args: &[String],
    root: &Path,
    sink: &dyn RunSink,
    slot: &ChildSlot,
) -> RunProgress {
    let mut pending = match spawn(exe, args, root, slot, true) {
        Ok(pending) => pending,
        Err(result) => return RunProgress::Complete(result),
    };
    match pending.read(sink, true) {
        Ok(true) => RunProgress::Review(Box::new(pending)),
        Ok(false) => RunProgress::Complete(pending.complete()),
        Err(why) => RunProgress::Complete(pending.refuse(&why)),
    }
}
