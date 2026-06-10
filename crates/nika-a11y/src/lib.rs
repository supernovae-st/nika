// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-a11y` · accessibility-tree-query L1 effect crate (M2.3).
//!
//! Implements the L0.5 `nika_kernel::io::a11y::AccessibilityTreeDyn` trait
//! (the `Send` variant — the local `AccessibilityTree` arrives via the
//! kernel's one-way blanket impl) —
//! `snapshot` + `find` + `resolve_ref` — exposing the active window's
//! accessibility tree as `AxNode` records (id · role · label · value · bbox ·
//! children · attributes). The OS query is delegated to the safe **`accessibility`**
//! crate (macOS `AXUIElement` · the unsafe `ApplicationServices` FFI is
//! encapsulated), so this crate honours `unsafe_code = "forbid"` — the same
//! sovereign posture `nika-screen` gets from `xcap` and `nika-ocr` from `ocrs`.
//!
//! **B.2 skeleton** · the `AccessibilityTree` methods return `BackendNotWired`;
//! B.3 wires the macOS `AXUIElement` walk inside `spawn_blocking`. Error
//! taxonomy NIKA-1200..1206 (ADR-081 nika-a11y sub-range NIKA-1200..1299).
//!
//! **ADR-081 Guard 3 (AX-secure-field redaction · MANDATORY-at-admission)** is
//! headless-complete at B.2 · a pure recursive tree-transform strips `value`
//! from any secure-text node (macOS `AXSecureTextField` subrole · AT-SPI
//! `STATE_SENSITIVE`) so passwords never leave the crate. macOS-first per
//! `docs/crate-specs/nika-a11y.md` §4 (Linux AT-SPI / Windows UIA backends
//! land additively when a consumer signal arrives · LOCK-031 spirit).

// Test-only relaxation of the workspace zero-unwrap / zero-expect deny lints
// (production src stays clean · enforced by clippy unwrap_used = deny).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod error;
mod tree;

pub use error::A11yError;
pub use tree::AxBackend;
