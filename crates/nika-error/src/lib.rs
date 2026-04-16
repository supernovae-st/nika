// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-error` — canonical error infrastructure for the Nika diamond.
//!
//! Design: **Option C+** (trait-based error hierarchy).
//!
//! - [`NikaErrorCode`] trait — implemented by per-crate error enums
//! - [`NikaError`] wrapper — `Box<dyn NikaErrorCode>`, the unified type
//! - [`CoreError`] enum — cross-cutting errors (`Validation`, `NotFound`, `Internal`)
//! - [`NikaCode`] struct — dual wire ("NIKA-140") + typed (num, category, slug)
//!
//! See `BRAINSTORM_PHASE1_DECISIONS.md` §D2 for rationale.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
        clippy::float_cmp,
    )
)]

pub mod codes;
pub mod core_error;
pub mod nika_error;
pub mod traits;

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
pub mod memory;
pub mod role;
pub mod token_usage;

/// Convenience re-exports for common usage.
///
/// ```rust
/// use nika_error::prelude::*;
/// ```
pub mod prelude {
    pub use crate::codes::{self, Category, NikaCode, Severity};
    pub use crate::core_error::CoreError;
    pub use crate::nika_error::{NikaError, NikaResult};
    pub use crate::traits::NikaErrorCode;

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
    pub use crate::memory::{MemoryDirective, MemoryFrameRef, MemoryId, MemoryLevel};
    pub use crate::role::Role;
    pub use crate::token_usage::TokenUsage;
}

pub use prelude::*;
