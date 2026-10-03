// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A live child retained across a fresh Run question. Dropping it cancels it.
//! The same bounded reader folds the plain lane child ([`super::drive_child`]).
// This established host lane owns child process creation and pipes.
#![allow(clippy::disallowed_types)]
use super::{ChildSlot, RunSink, RunStory};
use nika_providers::admission::CostChallenge;
use std::io::{BufRead as _, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::Sender;

pub(super) type RunResult = (u8, Option<PathBuf>, Vec<String>);
/// The most bytes one frame of the child may hold.
const FRAME_CAP: u64 = 1_048_576;
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
}
impl Drop for PendingRun {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Ok(mut slot) = self.slot.lock() {
            *slot = None;
        }
    }
}
impl PendingRun {
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
    /// (`true`), each frame bounded; an oversize frame, one that is not UTF-8
    /// (never repaired: the review answers the exact bytes) or a read error stops.
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
                return Err("Run frame exceeds 1 MiB".into());
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
        let code = self
            .child
            .wait()
            .ok()
            .and_then(|s| s.code())
            .and_then(|c| u8::try_from(c).ok())
            .unwrap_or(3);
        (
            code,
            self.story.trace.take(),
            std::mem::take(&mut self.story.lines),
        )
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
        .stderr(Stdio::null())
        .spawn();
    let mut child = spawn.map_err(|e| (3, None, vec![format!("the run could not start: {e}")]))?;
    let Some(output) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err((3, None, vec!["Run stdout unavailable".into()]));
    };
    if let Ok(mut value) = slot.lock() {
        *value = Some(child.id());
    }
    Ok(PendingRun {
        child,
        output: std::io::BufReader::new(output),
        story: RunStory::default(),
        slot: slot.clone(),
        challenge: None,
    })
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
