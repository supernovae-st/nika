// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation the Session's selected intelligence leads (ADR-153).
//!
//! A person's line starts a run: the model reads the conversation, answers or calls the
//! Session's tools, reads their replies and goes on until it answers without a call. No step,
//! turn, token or time quota ends a run; Stop, a call that waits for the person, or a failure
//! does, and a route's real context window is kept by compaction, never by forgetting turns.
//!
//! - [`run`] · the loop over one [`run::Model`] and the Session's
//!   [`SessionTools`](nika_session_change::tools::SessionTools): the calls it makes, the replies
//!   it records, the turn a call ends until the person answers. An agent with its own loop (an
//!   ACP agent, [`run::Conversant`]) leads under the same contract: it reaches the tools through
//!   a [`run::Relay`], and the tree records its calls, replies and answers the same way.
//! - [`steer`] · the person's lines while a run is under way: a steering line enters after the
//!   current calls, a follow-up line when the run would end, and Stop returns both unsent; each
//!   line has an identity and a state (waiting, entered with its citation, returned).
//! - [`observe`] · the Session's tools as both loops reach them, each real call reported to the
//!   turn's sink as steps (started, finished or failed, the time it took), never its arguments.
//! - [`tree`] · the Session tree: every entry a conversation records, each with its parent,
//!   one chained line per entry; the branch a model reads, and the person's cited lines, the
//!   only entries that carry authority.
//! - [`compact`] · earlier entries folded into a summary when the route's window requires it,
//!   the person's words and the values' provenance kept.
//! - [`event`] · what a run tells a host while it happens (`nika/session-events@0`).
//!
//! The loop carries calls and replies and never interprets them: the Session's tools act on
//! the Session, and only the Session's doors save, run or consent. A member of the
//! `nika-session` unit: `nika-session` → `nika-session-agent` → `nika-session-change`, never back.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod compact;
pub mod event;
pub mod observe;
pub mod run;
pub mod steer;
pub mod tree;

pub use event::{AgentEvent, End};
pub use observe::{Observed, StepSink, StepState, ToolStep};
pub use run::{
    Agent, AgentError, Beat, Conversant, Led, LedEnd, Model, ModelError, Outcome, Relay, Reply,
    Request, Store,
};
pub use steer::{QueueMode, QueueRefused, Queued, QueuedState, Steering};
pub use tree::{Entry, EntryId, EntryKind, Tree, TreeError};
