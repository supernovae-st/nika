// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A live child retained across a fresh Run question. Dropping it cancels it.
// This established host lane owns child process creation and pipes.
#![allow(clippy::disallowed_types)]
use super::{ChildSlot, RunStory};
use nika_providers::admission::CostChallenge;
use std::io::{BufRead as _, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::Sender;

pub(super) type RunResult = (u8, Option<PathBuf>, Vec<String>);
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
    pub fn answer(mut self, yes: bool, busy: &Sender<String>) -> RunResult {
        let result = (|| {
            let challenge = self.challenge.take().ok_or("no pending Run review")?;
            let response =
                serde_json::to_vec(&challenge.response(yes)).map_err(|e| e.to_string())?;
            let mut input = self.child.stdin.take().ok_or("Run reply channel closed")?;
            input.write_all(&response).map_err(|e| e.to_string())?;
            drop(input);
            self.read(busy, false)
        })();
        match result {
            Ok(false) => self.complete(),
            Ok(true) => self.refuse("duplicate Run review"),
            Err(why) => self.refuse(&why),
        }
    }
    fn read(&mut self, busy: &Sender<String>, allow_review: bool) -> Result<bool, String> {
        loop {
            let mut line = String::new();
            // Bound a child frame without changing the normal machine lane protocol.
            let size = self
                .output
                .by_ref()
                .take(1_048_577)
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            if size == 0 {
                return Ok(false);
            }
            if size > 1_048_576 {
                return Err("Run frame exceeds 1 MiB".into());
            }
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
            if let Some(said) = self.story.frame(&line) {
                let _ = busy.send(said);
            }
        }
    }
    fn complete(&mut self) -> RunResult {
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
    let spawn = Command::new(exe)
        .args(args)
        .arg("--cost-review-stdio")
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match spawn {
        Ok(child) => child,
        Err(e) => {
            return RunProgress::Complete((3, None, vec![format!("the run could not start: {e}")]));
        }
    };
    let Some(output) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return RunProgress::Complete((3, None, vec!["Run stdout unavailable".into()]));
    };
    if let Ok(mut value) = slot.lock() {
        *value = Some(child.id());
    }
    let mut pending = PendingRun {
        child,
        output: std::io::BufReader::new(output),
        story: RunStory::default(),
        slot: slot.clone(),
        challenge: None,
    };
    match pending.read(busy, true) {
        Ok(true) => RunProgress::Review(Box::new(pending)),
        Ok(false) => RunProgress::Complete(pending.complete()),
        Err(why) => RunProgress::Complete(pending.refuse(&why)),
    }
}
