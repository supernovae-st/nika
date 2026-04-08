// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! EventEmitter re-export + blanket impl for Arc.
//!
//! The trait itself is defined in nika-event. This module adds:
//! - Blanket impl for `Arc<T>` so `Arc<EventLog>` satisfies `EventEmitter`
//! - `EventSink` type alias for the common `Arc<dyn EventEmitter>` pattern
//!
//! Phase 5 of Constellation will flip 5 hot sites to use `EventSink`.

// Re-export from nika-event (via nika-core which doesn't have it — so we
// define the kernel-level alias here). The actual trait lives in nika-event.
// For now, we define a local mirror that will be unified in Phase 5.

use std::sync::Arc;

/// Type alias for the trait-object event sink used throughout the runtime.
///
/// Replaces concrete `EventLog` fields in structs that only need to emit events.
pub type EventSink = Arc<dyn EventEmitter>;

/// Trait for emitting events during workflow execution.
///
/// Mirrors `nika_event::EventEmitter`. Will be unified in Phase 5
/// when the blanket impl is added to nika-event directly.
pub trait EventEmitter: Send + Sync {
    /// Emit an event and return its sequence ID.
    fn emit(&self, kind_name: &str, payload: serde_json::Value) -> u64;
}

/// Blanket impl: `Arc<T: EventEmitter>` is also an `EventEmitter`.
///
/// This eliminates 40+ `.clone()` calls across the codebase — structs
/// can hold `Arc<dyn EventEmitter>` and pass it around cheaply.
impl<T: EventEmitter + ?Sized> EventEmitter for Arc<T> {
    fn emit(&self, kind_name: &str, payload: serde_json::Value) -> u64 {
        (**self).emit(kind_name, payload)
    }
}
