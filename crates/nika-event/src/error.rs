// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Event-subsystem errors implementing [`NikaErrorCode`].
//!
//! Codes live in the **NIKA-801..803** slots of the Observability band
//! (800-819 — events are the engine chronicle surface). They were
//! minted 420-422 until 2026-08-19, inside the Shield reservation
//! (380-429): a full event buffer surfaced as « Shield security policy
//! blocked the operation ». The L0 emitters
//! ([`crate::NoOpEmitter`], [`crate::InMemoryEmitter`]) only ever raise
//! [`EventError::BufferFull`]; [`EventError::SerializationFailed`] is the
//! contract slot for future I/O-backed emitters (higher layers).

use nika_error::prelude::*;

/// NIKA-801: an emitter failed to serialize an event.
pub const NIKA_801: NikaCode = NikaCode {
    num: 801,
    category: Category::Observability,
    severity: Severity::Error,
    slug: "event-serialization-failed",
};

/// NIKA-802: a bounded emitter's buffer is full.
pub const NIKA_802: NikaCode = NikaCode {
    num: 802,
    category: Category::Observability,
    severity: Severity::Warning,
    slug: "event-buffer-full",
};

/// NIKA-803: an emitter's internal lock was poisoned by a prior panic.
pub const NIKA_803: NikaCode = NikaCode {
    num: 803,
    category: Category::Observability,
    severity: Severity::Error,
    slug: "event-lock-poisoned",
};

/// Errors raised while emitting an [`crate::Event`].
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[non_exhaustive]
pub enum EventError {
    /// NIKA-420: an emitter could not serialize the event.
    #[error("event serialization failed: {detail}")]
    #[diagnostic(help("check the event's structured fields are serializable"))]
    SerializationFailed {
        /// Human-readable serializer detail.
        detail: String,
    },

    /// NIKA-421: a bounded emitter has reached capacity.
    #[error("event buffer full: capacity {capacity} reached")]
    #[diagnostic(help("drain the emitter or raise its capacity"))]
    BufferFull {
        /// The capacity bound that was hit.
        capacity: usize,
    },

    /// NIKA-422: the emitter's internal lock was poisoned by a prior panic
    /// (test-only in practice — L0 `src/` never panics while holding it).
    #[error("event emitter lock poisoned")]
    #[diagnostic(help("the emitter saw a panic in another thread; rebuild it"))]
    LockPoisoned,
}

impl EventError {
    /// The stable NIKA-XXX code for this error.
    #[must_use]
    pub fn code(&self) -> NikaCode {
        match self {
            Self::SerializationFailed { .. } => NIKA_801,
            Self::BufferFull { .. } => NIKA_802,
            Self::LockPoisoned => NIKA_803,
        }
    }
}

impl NikaErrorCode for EventError {
    fn nika_code(&self) -> NikaCode {
        self.code()
    }

    fn is_transient(&self) -> bool {
        // A full buffer may drain; serialization + poison are not transient.
        matches!(self, Self::BufferFull { .. })
    }
}
