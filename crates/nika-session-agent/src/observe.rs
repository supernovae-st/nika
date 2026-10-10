// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Session's tools as both loops reach them — Nika's loop calls them, an ACP agent's relay
//! wraps them — each real call reported as steps to the turn's sink: started, then finished or
//! failed with the time it took. A step names the call and the tool, never its arguments or its
//! reply, so what a host shows of an agent's work carries no file content or secret. Between
//! turns no sink watches, and nothing is reported.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

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
        let step = |state, elapsed_ms| ToolStep {
            call: id.clone(),
            name: call.name.clone(),
            state,
            elapsed_ms,
        };
        sink(&step(StepState::Started, None));
        let started = Instant::now();
        let reply = self.tools.call(call.clone());
        let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let state = if reply.is_error {
            StepState::Failed
        } else {
            StepState::Finished
        };
        sink(&step(state, Some(elapsed)));
        reply
    }
}

#[cfg(test)]
mod tests;
