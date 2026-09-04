// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-event` — canonical event log + trace types + emitter for the Nika diamond.
//!
//! This crate sits at **L0**: pure, zero I/O, zero async.
//!
//! # Three pieces
//!
//! - [`Event`] — the immutable event envelope (caller supplies id + timestamp;
//!   L0 never reads a clock — that's the L1 `nika-clock` effect).
//! - [`EventKind`] — the canonical engine taxonomy (`#[non_exhaustive]`):
//!   workflow + task lifecycle + the 4-verb dispatch surface.
//! - [`Emitter`] — the object-safe sink trait, with [`NoOpEmitter`] and
//!   [`InMemoryEmitter`] (bounded or unbounded) L0 impls.
//!
//! ```
//! use nika_event::{Event, EventKind, Emitter, InMemoryEmitter};
//! use nika_types::id::EventId;
//! use nika_types::timestamp::Timestamp;
//! use uuid::Uuid;
//!
//! let sink = InMemoryEmitter::unbounded();
//! let ev = Event::new(
//!     EventId::new(Uuid::nil()),
//!     Timestamp::from_unix_ms(1),
//!     EventKind::WorkflowStarted,
//! );
//! sink.emit(ev).expect("unbounded emit never fails");
//! assert_eq!(sink.len(), 1);
//! ```
//!
//! # Domain boundary
//!
//! This is the **engine** chronicle (runtime events). The **studio**
//! keeps a separate chronicle in its own private tree: a disjoint
//! domain, same NDJSON spirit, different taxonomy — never conflated.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
        clippy::float_cmp,
    )
)]

pub mod emitter;
pub mod error;
pub mod event;
#[cfg(feature = "serde")]
pub mod fold;
pub mod kind;
pub mod settlement;
pub mod source_id;
pub mod stamp;

pub use emitter::{Emitter, InMemoryEmitter, NoOpEmitter};
pub use error::EventError;
pub use event::Event;
pub use kind::{EventClass, EventKind};

/// Convenience re-exports for common usage.
pub mod prelude {
    pub use crate::emitter::{Emitter, InMemoryEmitter, NoOpEmitter};
    pub use crate::error::EventError;
    pub use crate::event::Event;
    pub use crate::kind::{EventClass, EventKind};
    pub use crate::settlement::{
        CostQualifier, RunCause, RunSettlement, RunState, SettlementError, Spend, TaskTally,
    };
}
