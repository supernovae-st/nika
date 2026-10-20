// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Session's tools as both loops reach them — Nika's loop calls them, an ACP agent's relay
//! wraps them — each real call reported as steps to the turn's sink: started, then finished or
//! failed with the time it took. A step names the call and the tool, never its arguments; of its
//! reply only a failed call's excerpt ([`ToolReply::excerpt`]: bounded, on one line, its secret
//! shapes redacted), so a host can say why a call failed and still shows no file content or
//! secret. A finished proposal may separately carry its producer's versioned verification,
//! unchanged and never interpreted as prose. Between turns no sink watches, and nothing is reported.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Instant, SystemTime};

use nika_session_change::tools::{SessionTools, ToolCall, ToolDef, ToolReply};

/// Where a tool call is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StepState {
    /// It began.
    Started,
    /// It answered.
    Finished,
    /// It answered with a failure.
    Failed,
}

/// One step of one tool call.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ToolStep {
    /// The call's identity: the one the model or the agent gave it, else `step-N`.
    pub call: String,
    /// The tool called.
    pub name: String,
    /// Where the call is.
    pub state: StepState,
    /// How long it took, in milliseconds, once it answered.
    pub elapsed_ms: Option<u64>,
    /// When it began, in Unix milliseconds.
    pub at_ms: u64,
    /// The size of its reply in bytes, once it answered (never the reply).
    pub reply_bytes: Option<u64>,
    /// Why it failed, as a host may show it ([`ToolReply::excerpt`]); `None` unless it failed.
    pub reply: Option<String>,
    /// Original compiler verification on a finished proposal, separate from the reply excerpt.
    pub verification: Option<serde_json::Value>,
}

/// Where a turn's steps go.
pub type StepSink = Arc<dyn Fn(&ToolStep) + Send + Sync>;

/// The Session's tools, each call reported to the sink of the turn under way.
pub struct Observed {
    tools: Arc<dyn SessionTools>,
    sink: Mutex<Option<StepSink>>,
    unnamed: AtomicU64,
}

impl Observed {
    /// The tools `tools`, watched by no sink yet.
    #[must_use]
    pub fn new(tools: Arc<dyn SessionTools>) -> Self {
        Self {
            tools,
            sink: Mutex::new(None),
            unnamed: AtomicU64::new(0),
        }
    }

    /// The sink the turn under way reports to; `None` between turns.
    pub fn watch(&self, sink: Option<StepSink>) {
        *self.sink() = sink;
    }

    fn sink(&self) -> MutexGuard<'_, Option<StepSink>> {
        self.sink.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl SessionTools for Observed {
    fn tools(&self) -> Vec<ToolDef> {
        self.tools.tools()
    }

    fn call(&self, call: ToolCall) -> ToolReply {
        let Some(sink) = self.sink().clone() else {
            return self.tools.call(call);
        };
        let id = call.meta.clone().unwrap_or_else(|| {
            let n = self.unnamed.fetch_add(1, Ordering::Relaxed) + 1;
            format!("step-{n}")
        });
        let at_ms = crate::tree::unix_ms(SystemTime::now());
        let step = |state, elapsed_ms, reply_bytes, why, verification| ToolStep {
            call: id.clone(),
            name: call.name.clone(),
            state,
            elapsed_ms,
            at_ms,
            reply_bytes,
            reply: why,
            verification,
        };
        sink(&step(StepState::Started, None, None, None, None));
        let started = Instant::now();
        let reply = self.tools.call(call.clone());
        let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let (state, why) = if reply.is_error {
            (StepState::Failed, Some(reply.excerpt()))
        } else {
            (StepState::Finished, None)
        };
        let size = u64::try_from(reply.text.len()).unwrap_or(u64::MAX);
        let verification = (call.name == "propose" && reply.ends_turn && !reply.is_error)
            .then(|| reply.verification.clone())
            .flatten();
        sink(&step(state, Some(elapsed), Some(size), why, verification));
        reply
    }
}

#[cfg(test)]
mod tests;
