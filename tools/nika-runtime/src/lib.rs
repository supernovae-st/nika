// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika Runtime — verb dispatch, capability bundles, and runtime orchestration.
//!
//! L3 in the diamond layer. Depends on:
//! - `nika-kernel` (L0.5) for trait definitions
//! - `nika-core` (L0) for AST types
//! - `nika-event` (L1) for `EventLog`
//!
//! ## Key exports
//!
//! - [`VerbCapabilities`] — all `Arc<dyn Trait>` dependencies for verb execution
//! - [`dispatch::ResolvedAction`] — enum of pre-built typed verb inputs
//! - [`dispatch::dispatch`] — 5-arm match that executes resolved actions
//!
//! ## Session 21 state
//!
//! `dispatch()` accepts `ResolvedAction` (pre-resolved typed inputs) and
//! delegates to verb crate adapters. Exec and Invoke arms are LIVE.
//! Fetch, Infer, and Agent arms return `NotImplemented`.
//! The engine builds `ResolvedAction` after template resolution and
//! security validation, then calls `dispatch()` for execution.

pub mod capabilities;
pub mod dispatch;
pub mod error;
pub mod verb_exec;
pub mod verb_fetch;
pub mod verb_infer;
pub mod verb_invoke;

/// Re-export of the exec verb crate so callers can build `ExecInput`
/// without depending on `nika-verb-exec` directly.
pub use nika_verb_exec as exec;

/// Re-export of the fetch verb crate so callers can build `FetchInput`
/// without depending on `nika-verb-fetch` directly.
pub use nika_verb_fetch as fetch;

/// Re-export of the infer verb crate so callers can build `InferInput`
/// without depending on `nika-verb-infer` directly.
pub use nika_verb_infer as infer;

/// Re-export of the invoke verb crate so callers can build `InvokeInput`
/// without depending on `nika-verb-invoke` directly.
pub use nika_verb_invoke as invoke;
