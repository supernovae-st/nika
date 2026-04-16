// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-types` — foundation value types for the Nika diamond.
//!
//! L0 leaf crate: pure, sync, zero I/O. No workspace dependencies.
//!
//! Contains all non-error types that were originally in `nika-error`:
//! IDs, Cost, `TrustLevel`, Checkpoint, Memory directives, Hash, Baggage,
//! Resource, Retry, Cancel, `TokenUsage`, Budget, Schema versions,
//! `CompressionPolicy`, and Role.
//!
//! # `no_std` support
//!
//! `nika-types` builds against `core + alloc` when the default `std` feature
//! is disabled (`--no-default-features`). The `std` feature is additive and
//! only reserves the seam for WASM guests and embedded targets — today no
//! code path is gated behind `cfg(feature = "std")`. See ADR-028 + Phase F
//! of the 2026-04-16 swarm-3 audit plan.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
        clippy::float_cmp,
    )
)]

extern crate alloc;

// ─── L0 foundational value types (ADR-033) ──────────────────────────
pub mod baggage;
pub mod budget;
pub mod cost;
pub mod hash;
pub mod id;
pub mod resource;
pub mod retry;
pub mod schema;
pub mod trust;

// ─── L0 shared types (descended from kernel, Phase 0) ──────────────
pub mod cancel;
pub mod checkpoint;
pub mod compression;
pub mod embedding;
pub mod memory;
pub mod role;
pub mod token_usage;

/// Convenience re-exports for common usage.
///
/// ```rust
/// use nika_types::prelude::*;
/// ```
pub mod prelude {
    // ADR-033 L0 foundational value types
    pub use crate::baggage::Baggage;
    pub use crate::budget::BudgetDirective;
    pub use crate::cost::Cost;
    pub use crate::hash::{Blake3Hash, BlobRef, ContentDigest};
    pub use crate::id::{
        CorrelationId, EventId, ModelId, ProviderId, RunId, SpanId, TaskId, TenantId, TraceId,
        WorkflowId,
    };
    pub use crate::resource::{KeyValue, Resource, Value};
    pub use crate::retry::{ErrorCategory, RetryConfig};
    pub use crate::schema::{EventSchemaVersion, TraceFormatVersion};
    pub use crate::trust::TrustLevel;

    // Phase 0 descended types
    pub use crate::cancel::CancelCtx;
    pub use crate::checkpoint::{AgentCheckpoint, CheckpointMessage, ToolCallRecord};
    pub use crate::compression::CompressionPolicy;
    pub use crate::embedding::{DistanceMetric, EmbeddingDtype, EmbeddingSpec};
    pub use crate::memory::{MemoryDirective, MemoryFrameRef, MemoryId, MemoryLevel};
    pub use crate::role::Role;
    pub use crate::token_usage::TokenUsage;
}

pub use prelude::*;
