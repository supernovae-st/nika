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
//! | [`MockClock`] | `Clock` | Controllable time, instant sleep |
//! | [`MockFs`] | `Fs` | In-memory `BTreeMap<PathBuf, Vec<u8>>` |
//! | [`MockHttp`] | `HttpClient` | FIFO response queue + call recording |
//! | [`MockShell`] | `ShellExecutor` | Scripted results + call recording |
//! | [`MockBlob`] | `BlobStore` | In-memory `BTreeMap<String, Bytes>` |
//! | [`MockProvider`] | `Provider` | Deterministic responses + stream synthesis |
//! | [`NullMemoryStore`] | `MemoryStore` | accept-all writes, empty recalls |
//! | [`NullEmbeddingProvider`] | `EmbeddingProvider` | Zero vectors |
//! | [`NullToolExecutor`] | `ToolExecutor` | Returns `NotAvailable` |
//! | [`MockToolExecutor`] | `ToolExecutor` | FIFO results + call recording |
//! | [`NullContextCompressor`] | `ContextCompressor` | Returns `None` |

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
    )
)]

pub mod blob;
pub mod clock;
pub mod compressor;
pub mod filesystem;
pub mod http;
pub mod memory;
pub mod provider;
pub mod shell;
pub mod tool_executor;

pub use blob::MockBlob;
pub use clock::MockClock;
pub use compressor::NullContextCompressor;
pub use filesystem::MockFs;
pub use http::MockHttp;
pub use memory::{NullEmbeddingProvider, NullMemoryStore};
pub use provider::MockProvider;
pub use shell::MockShell;
pub use tool_executor::{MockToolExecutor, NullToolExecutor};
