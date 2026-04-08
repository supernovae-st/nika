// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Event emission marker module.
//!
//! The canonical `EventEmitter` trait lives in `nika-event`, NOT here.
//! The blanket impl for `Arc<T>` and `EventSink` alias are also in `nika-event`.
//!
//! This module exists only to document the relationship:
//! - nika-kernel does NOT define EventEmitter (avoids signature divergence)
//! - Verb crates that need event emission depend on nika-event directly
//! - The blanket impl (`Arc<T: EventEmitter>` is EventEmitter) is in nika-event
//!
//! See `nika-event/src/emitter.rs` for the canonical trait, blanket impl,
//! EventSink alias, and NoopEmitter.
