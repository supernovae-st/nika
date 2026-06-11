// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-infer-local` — the sovereign local inference sidecar.
//!
//! This crate sits at **L1.5** (service). It closes the one non-Rust,
//! non-ours link in the local-first chain: today `model: ollama/<x>` is the
//! "sovereign default", but ollama is an external **Go** daemon. This crate
//! is the all-Rust replacement — a local inference server we ship and
//! **spawn as a supervised child process**, that the engine reaches over the
//! **existing `OpenAiCompat` wire** (`nika-providers`). No new dispatch seam:
//! to the engine it looks like any other local OpenAI-compatible server,
//! except the server is ours, in Rust. Per **ADR-091**.
//!
//! # Why a sidecar (subprocess), not in-process
//!
//! The in-process GGUF runtime (`native`) was deferred because mistral.rs
//! crashed the host. A model runtime is the one place a segfault / GPU abort
//! / OOM can take down the whole process, and `catch_unwind` cannot contain
//! memory-corruption. Running it in a **child process the engine owns** (not
//! an external daemon) is the only model that contains those crashes: the
//! child dies, the lean core detects the broken pipe and restarts/degrades.
//!
//! # The candle backend is feature-gated (off by default)
//!
//! candle (+ `candle-transformers`) is the only heavy dependency and it lands
//! behind the `local-infer` feature so the default `nika` build links **zero**
//! inference deps. This v1 scaffold defines the architecture in pure Rust:
//!
//! - [`protocol`] — the OpenAI-compat chat request/response types (the wire
//!   contract the engine's existing `OpenAiCompat` client already speaks).
//! - [`Backend`] — the device-agnostic generation seam the candle backend
//!   will implement (`Device::Cpu | Metal | Cuda` abstracted by candle).
//! - [`MockBackend`] — a deterministic, dependency-free backend that makes the
//!   crate compile + test now AND powers the self-consistency check (our
//!   output → Nika's own `OpenAiCompat` parser → must round-trip).
//!
//! # SOTA invariants the real backend MUST honor (the candle commit's gates)
//!
//! KV-cache (O(n²) without it) · per-family chat template (wrong template =
//! garbage) · SSE token streaming · EOS + stop-sequence halt · full sampling
//! with a **seed** (determinism for tests) · GGUF quantization · lazy warm
//! model load. Structured output v1 = the schema-validation retry loop (the
//! pattern already proven in `eval/`); logit-masking is v2.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod backend;
#[cfg(feature = "local-infer")]
mod candle_backend;
mod error;
mod logits;
pub mod protocol;
mod sampling;
#[cfg(feature = "server")]
mod server;
mod stop;
mod template;

pub use backend::{Backend, BackendDyn, GenerationChunk, MockBackend};
#[cfg(feature = "local-infer")]
pub use candle_backend::CandleBackend;
pub use error::InferLocalError;
pub use logits::{apply_min_p, apply_repeat_penalty, apply_token_mask, apply_top_n_sigma};
pub use sampling::{DEFAULT_REPEAT_LAST_N, DEFAULT_SEED, SamplingConfig};
#[cfg(feature = "server")]
pub use server::{ServerHandle, serve};
pub use stop::StopController;
pub use template::ChatFamily;
