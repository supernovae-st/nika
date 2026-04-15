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
//! ~20 atomic traits grouped into ~6 super-traits via blanket implementations.
//! All async traits use [`trait_variant`] for zero-overhead object-safe companions
//! (`XxxDyn: Send`).
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

pub mod agent;
pub mod billing;
pub mod blob;
pub mod cancel;
pub mod checkpoint;
pub mod clock;
pub mod context;
pub mod errors;
pub mod event_sink;
pub mod fs;
pub mod http;
pub mod id_gen;
pub mod memory;
pub mod metrics;
pub mod observability;
pub mod plugin;
pub mod process;
pub mod provider;
pub mod sandbox;
pub mod sealed;
pub mod secret;
pub mod tool_executor;
pub mod trace;
pub mod types;

// Re-export the most commonly used items for convenience.
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
};
pub use metrics::{MetricTag, MetricsExporter};
pub use observability::{MetricEvent, ObservabilityError, ObservabilitySink, SpanEvent};
pub use plugin::{PluginFs, PluginHttp, WasmPluginError, WasmPluginHost};
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
