// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:jq` in a process of its own, for a rehearsal. In process, jaq's work and memory are not
//! bounded and a timed-out evaluation keeps running past the room's bound. Here the engine's own
//! binary, started in its helper mode, evaluates one call with the in-process evaluator itself:
//! the request and answer are the builtin's pure codec, framed by length so neither side waits
//! for the end of a pipe. One evaluation runs at a time; its request is bounded before anything
//! starts, its outputs while it runs, its time by the run's deadline. The helper bounds its own
//! heap allocations and CPU time. A deadline or a cap ends the process group, and the collection
//! drains and reaps it before answering. A bounded evaluation is never a program's failure: the
//! first one a run meets is kept for the room, which reports the run as stopped by that bound.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use nika_builtin::data::{JQ_HELPER_WORD, jq_answer, jq_frame, jq_request_bytes, jq_unframe};
use nika_exec_runner::{Caps, Collected, Lane};
use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolResult, ToolRunStart};

/// The request bytes one evaluation carries: twice a room's copy bound of input, and a program.
const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024 + 64 * 1024;

/// The answer bytes kept: the builtin's own 16 MiB value ceiling, and its framing.
const MAX_ANSWER_BYTES: usize = 16 * 1024 * 1024 + 4 * 1024;

/// The stderr bytes kept of a helper.
const MAX_STDERR_BYTES: usize = 64 * 1024;

/// How long a killed helper's outputs may still take to end.
const GRACE: Duration = Duration::from_secs(1);

/// What a refused evaluation is called, before its reason.
const BOUNDED: &str = "rehearsal jq bound";

/// The program a rehearsal starts to evaluate `nika:jq`: the engine's own binary, which the
/// host that owns it names. Without one a rehearsal runs no jq.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct JqHelper {
    program: PathBuf,
}

impl JqHelper {
    /// The helper at `program`.
    #[must_use]
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// The program started for each evaluation.
    #[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }
}

/// The first bound one run's evaluations met.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct JqBound {
    /// Why the evaluation ended.
    pub reason: String,
    /// Whether the helper's end was observed and reaped: false when its outputs did not end.
    pub reaped: bool,
}

/// One run's isolated jq evaluations, until the run's deadline.
#[derive(Debug)]
#[non_exhaustive]
pub struct IsolatedJq {
    helper: JqHelper,
    deadline: Instant,
    lane: Lane,
    bound: Mutex<Option<JqBound>>,
}

impl IsolatedJq {
    /// Evaluations by `helper`, none past `deadline`.
    #[must_use]
    pub fn new(helper: JqHelper, deadline: Instant) -> Self {
        Self {
            helper,
            deadline,
            lane: Lane::new(),
            bound: Mutex::new(None),
        }
    }

    /// The first bound an evaluation of this run met, if any.
    #[must_use]
    pub fn bound(&self) -> Option<JqBound> {
        self.bound.lock().ok().and_then(|bound| bound.clone())
    }

    /// Evaluate one `nika:jq` call: its result exactly as the in-process builtin renders it, or
    /// a refusal naming the bound it met.
    pub(super) async fn evaluate(&self, call: &ToolCall) -> Result<ToolResult, ToolExecError> {
        // The dispatcher's own fallback: a call without its run start reads the epoch.
        let run_start = call.run_start().map_or(0, ToolRunStart::unix_ns);
        let request = jq_request_bytes(&call.input, run_start);
        if request.len() > MAX_REQUEST_BYTES {
            let why = format!(
                "the request is {} bytes, over the {MAX_REQUEST_BYTES} one evaluation carries",
                request.len()
            );
            return Err(self.refuse(&why, true));
        }
        let caps = Caps::new(MAX_ANSWER_BYTES, MAX_STDERR_BYTES, GRACE);
        let collected = self
            .lane
            .collect(
                self.helper.program(),
                &[JQ_HELPER_WORD],
                &jq_frame(&request),
                caps,
                self.deadline,
            )
            .await;
        match collected {
            Ok(Collected::Exited { status, stdout, .. }) if status.success() => {
                let answer =
                    jq_unframe(&stdout).and_then(|bytes| jq_answer(call.id.as_str(), bytes));
                answer.ok_or_else(|| {
                    self.refuse("the helper answered nothing an evaluation gives", true)
                })
            }
            Ok(Collected::Exited { status, .. }) => {
                let why = format!("the helper ended with {status}: its heap or CPU bound");
                Err(self.refuse(&why, true))
            }
            Ok(Collected::Killed { ended, .. }) => {
                let why =
                    format!("the helper was ended by its {ended:?} bound, drained and reaped");
                Err(self.refuse(&why, true))
            }
            Ok(Collected::Abandoned { ended }) => {
                let why =
                    format!("the helper was ended by its {ended:?} bound; its outputs did not end");
                Err(self.refuse(&why, false))
            }
            Ok(_) => Err(self.refuse("the helper ended in a way unknown here", false)),
            Err(error) => Err(self.refuse(&format!("the helper could not run: {error}"), true)),
        }
    }

    /// Keep the first bound of this run, and refuse the call in its words.
    fn refuse(&self, reason: &str, reaped: bool) -> ToolExecError {
        if let Ok(mut bound) = self.bound.lock()
            && bound.is_none()
        {
            *bound = Some(JqBound {
                reason: reason.to_owned(),
                reaped,
            });
        }
        ToolExecError::NotAvailable {
            reason: format!("{BOUNDED} · {reason}"),
        }
    }
}
