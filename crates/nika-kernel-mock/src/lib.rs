// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-kernel-mock` — deterministic in-memory test doubles for kernel traits.
//!
//! Every mock is `Send + Sync + Clone`, backed by `Arc<Mutex<...>>` or
//! `Arc<RwLock<...>>`. Clones share state for multi-task test scenarios.
//!
//! # Mock types
//!
//! | Mock | Implements | Behavior |
//! |---|---|---|
//! | [`NullAuditSink`] | `AuditSink` | Always-`Ok`, persists nowhere |
//! | [`FailingAuditSink`] | `AuditSink` | Always `PersistFailed` (error-path tests) |
//! | [`MockClock`] | `Clock` | Controllable time, instant sleep |
//! | [`MockFs`] | `Fs` | In-memory `BTreeMap<PathBuf, Vec<u8>>` |
//! | [`MockHttp`] | `HttpClient` | FIFO response queue + call recording |
//! | [`MockShell`] | `ShellExecutor` | Scripted results + call recording |
//! | [`MockBlob`] | `BlobStore` | In-memory `BTreeMap<String, Bytes>` |
//! | [`MockProvider`] | `Provider` | Deterministic responses + stream synthesis |
//! | [`NullMemoryStore`] | `MemoryStore` | accept-all writes, empty recalls |
//! | [`NullEmbeddingProvider`] | `EmbeddingProvider` | Zero vectors |
//! | [`NullToolExecutor`] | `ToolExecuteDyn`+`ToolBatchDyn` | Returns `NotAvailable` |
//! | [`MockToolExecutor`] | `ToolExecuteDyn`+`ToolBatchDyn` | FIFO results + call recording |
//! | [`MockToolDefinitionProvider`] | `ToolDefinitionProviderDyn` | Fixed tool universe or one error |
//! | [`NullContextCompressor`] | `ContextCompressor` | Returns `None` |

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
    )
)]

pub mod audit;
pub mod billing;
pub mod blob;
pub mod clock;
pub mod compressor;
pub mod event_sink;
pub mod filesystem;
pub mod http;
pub mod id_gen;
pub mod memory;
pub mod metrics;
pub mod plugin;
pub mod provider;
pub mod sandbox;
pub mod secret;
pub mod shell;
pub mod tool_defs;
pub mod tool_executor;
pub mod trace;

pub use audit::{FailingAuditSink, NullAuditSink};
pub use billing::NullBillingSink;
pub use blob::MockBlob;
pub use clock::MockClock;
pub use compressor::NullContextCompressor;
pub use event_sink::NullEventSink;
pub use filesystem::MockFs;
pub use http::MockHttp;
pub use id_gen::{MockIdGenerator, SequentialIdGenerator};
pub use memory::{NullEmbeddingProvider, NullMemoryStore};
pub use metrics::NullMetricsExporter;
pub use plugin::{NullPluginEnv, NullWasmPluginHost, NullWasmPluginLifecycle};
pub use provider::MockProvider;
pub use sandbox::NullSandbox;
pub use secret::NullSecretResolver;
pub use shell::MockShell;
pub use tool_defs::MockToolDefinitionProvider;
pub use tool_executor::{MockToolExecutor, NullToolExecutor};
pub use trace::NullTracerProvider;
