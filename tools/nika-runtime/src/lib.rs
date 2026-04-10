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
//! - [`dispatch`] — 5-arm match on `TaskAction`, calls per-verb free functions
//!
//! ## Session 13 scope
//!
//! During S13, `dispatch()` is built with 5 arms but is NOT the live code
//! path. The engine's `task_dispatch` continues to call `TaskExecutor` verb
//! methods. Each verb method becomes a bridge that delegates to the
//! corresponding `nika_verb_*::run` function. Session 14 switches the
//! Runner to call `dispatch()` directly.

pub mod capabilities;
pub mod dispatch;
pub mod error;
