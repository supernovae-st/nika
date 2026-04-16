// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-kernel` — trait contracts for every side effect in the Nika diamond.
//!
//! This crate sits at **L0.5**: above L0 pure types (nika-error, nika-catalog)
//! and below L1 effect implementations (nika-fs, nika-http, etc.).
//!
//! **Zero implementations live here.** This crate defines contracts only.
//! Implementations live in their respective crates; test mocks live in
//! `nika-kernel-mock` (L0.5 companion).
//!
//! # Architecture
//!
//! Modules are organized into 5 groups matching the planned kernel split:
//!
//! - **io** — filesystem, HTTP, shell process, blob storage, clock
//! - **ai** — provider inference, memory, context compression
//! - **runtime** — agent loop, tool execution
//! - **plugin** — WASM host, sandboxing, observability
//! - **infra** — billing, events, metrics, tracing, IDs, secrets
//!
//! All group modules are re-exported at the crate root for backward
//! compatibility — `nika_kernel::provider::*` continues to work.
//!
//! # Trait Hierarchy
//!
//! ```text
//! Clock (sync + async sleep)
//!
//! Fs = FsRead + FsWrite + FsMeta + FsList
//! HttpClient = HttpGet + HttpPost
//! ShellExecutor = ShellRun + ShellCancel
//! BlobStore (atomic, no split)
//! Provider = ProviderInfer + ProviderStream + ProviderMeta
//!   + opt-in: ProviderEmbed, ProviderVision
//! MemoryStore = MemoryRemember + MemoryRecall + MemoryForget
//!   + EmbeddingProvider
//! ToolExecutor = ToolExecute + ToolBatch
//! ContextCompressor
//! ```

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
        clippy::float_cmp,
        clippy::manual_string_new,
        clippy::unnecessary_literal_bound,
    )
)]

// ─── Group modules (organized by future sub-crate boundary) ─────────
pub mod ai;
pub mod infra;
pub mod io;
pub mod plugin;
pub mod runtime;

// ─── Shared root modules ────────────────────────────────────────────
pub mod cancel;
pub mod checkpoint;
pub mod errors;
pub mod sealed;
pub mod types;

// ─── Backward-compat re-exports (preserve crate::module_name paths) ─
pub use ai::{context, memory, provider};
pub use infra::{billing, event_sink, id_gen, metrics, secret, trace};
pub use io::{blob, clock, fs, http, process};
pub use plugin::sandbox;
pub use runtime::{agent, tool_executor};

// ─── Flat re-exports (existing public API, unchanged) ───────────────
pub use agent::{
    AgentLoopConfig, AgentOutcome, AgentStopReason, CompressionPolicy, PlanningStrategy,
    ReflectionConfig, ToolErrorPolicy,
};
pub use billing::BillingSink;
pub use blob::{BlobError, BlobMetadata, BlobStore};
pub use cancel::CancelCtx;
pub use checkpoint::{AgentCheckpoint, CheckpointMessage, ToolCallRecord};
pub use clock::Clock;
pub use context::{CompressedContext, ContextCompressor};
pub use event_sink::{Event, EventSink};
pub use fs::{FileMetadata, Fs, FsList, FsMeta, FsRead, FsWrite};
pub use http::{
    HttpClient, HttpError, HttpGet, HttpMethod, HttpPost, HttpRequest, HttpResponse,
    HttpStreamResponse,
};
pub use id_gen::IdGenerator;
pub use memory::{
    EmbeddingProvider, MemoryDirective, MemoryError, MemoryForget, MemoryFrame, MemoryFrameRef,
    MemoryHit, MemoryId, MemoryLevel, MemoryRecall, MemoryRemember, MemoryStore, RecallQuery,
    embed_batch_sequential,
};
pub use metrics::{MetricTag, MetricsExporter};
pub use plugin::{
    PluginEnv, PluginFs, PluginHttp, WasmPluginError, WasmPluginHost, WasmPluginLifecycle,
};
pub use process::{ShellCancel, ShellCommand, ShellError, ShellExecutor, ShellResult, ShellRun};
pub use provider::{
    ContentBlock, InferEvent, InferEventStream, InferRequest, InferResponse, Message, Provider,
    ProviderEmbed, ProviderError, ProviderExtras, ProviderInfer, ProviderMeta, ProviderStream,
    ProviderVision, ResponseFormat, Role, StopReason, TokenUsage, ToolChoice, ToolDef,
};
pub use sandbox::{Capability, Sandbox, SandboxError};
pub use secret::{Secret, SecretRef, SecretResolver};
pub use tool_executor::{
    ToolBatch, ToolCall, ToolCallId, ToolExecError, ToolExecute, ToolExecutor, ToolResult,
};
pub use trace::{SpanGuard, TracerProvider};
pub use types::{TaskId, WorkflowMeta};

/// Convenience re-export hub for L2+ verb crates.
///
/// Depending on `nika-kernel` alone gives access to foundation types
/// (ids, cost, trust, baggage, retry, checkpoint, cancel, memory, role,
/// token usage, resource, hash, schema, budget, compression) plus the
/// `NikaError` / `NikaResult` hierarchy — no need to add explicit
/// `nika-error` or `nika-types` deps.
///
/// Kernel traits (`Provider`, `Clock`, `Fs`, etc.) stay at the crate
/// root; import them alongside the prelude:
///
/// ```rust
/// use nika_kernel::{prelude::*, Provider, InferRequest};
/// ```
pub mod prelude {
    pub use nika_error::prelude::*;
}

#[cfg(test)]
mod prelude_tests {
    //! Pins the prelude contract so the upcoming `nika-error` carve-out
    //! (Q2 split seam) cannot silently drop a re-exported symbol without
    //! a verb-crate compile failure.

    #[allow(clippy::wildcard_imports)]
    use crate::prelude::*;

    #[test]
    fn value_types_reexported() {
        let _: Cost = Cost::default();
        let _: TrustLevel = TrustLevel::TRUSTED;
        let _: Role = Role::Assistant;
        let _: Baggage = Baggage::new();
        let _: RetryConfig = RetryConfig::new(3);
        let _: TokenUsage = TokenUsage::new(0, 0);
        let _: Blake3Hash = Blake3Hash::new([0u8; 32]);
        let _: fn() -> NikaResult<()> = || Ok(());
    }

    #[test]
    fn error_hierarchy_reexported() {
        let _: NikaCode = codes::NIKA_001;
        let _: Severity = Severity::Error;
        let _: CoreError = CoreError::Validation {
            reason: String::new(),
        };
    }

    #[test]
    fn id_and_hash_cluster_reexported() {
        fn _accepts<T>() {}
        _accepts::<RunId>();
        _accepts::<WorkflowId>();
        _accepts::<EventId>();
        _accepts::<TraceId>();
        _accepts::<SpanId>();
        _accepts::<TaskId>();
        _accepts::<BlobRef>();
        _accepts::<ContentDigest>();
        _accepts::<ErrorCategory>();
        _accepts::<CompressionPolicy>();
        _accepts::<CancelCtx>();
        _accepts::<AgentCheckpoint>();
    }
}
