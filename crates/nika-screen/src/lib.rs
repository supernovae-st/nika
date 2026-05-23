// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-screen` — screen-capture L1 effect crate.
//!
//! Implements the L0.5 [`nika_kernel::io::screen::ScreenCapture`] trait
//! (display enumeration · full-display + sub-region capture · continuous
//! frame stream) on macOS / Linux / Windows. The **first computer-use L1
//! effect crate** (Phase 2 M2.1).
//!
//! # Layer
//!
//! L1 — effect implementation, async. Depends only on L0 / L0.5. The OS
//! capture FFI is encapsulated by the `xcap` dependency (B.3+), keeping this
//! crate `unsafe_code = forbid`-clean.
//!
//! # Security guards (ADR-081)
//!
//! This crate owns 2 of the 7 forever L1 guards:
//! - **Guard 6** — capture-LED indicator ([`guards::LedIndicator`]).
//! - **Guard 7** — user consent UX ([`guards::ConsentGate`]).
//!
//! Both are MANDATORY-at-admission per ADR-081.
//!
//! # Error codes
//!
//! NIKA-1000..1099 reserved sub-range — see [`error::ScreenError`].
//!
//! # Status
//!
//! **Alpha.** All four `ScreenCapture` methods are wired via `xcap` (single-shot
//! B.3 + continuous stream B.4) and the ADR-081 guards are enforced (B.5).
//! OS-native consent/LED FFI is deferred; pixel-capture tests are `#[ignore]`-
//! gated on screen-recording permission (TCC).

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
        clippy::float_cmp,
    )
)]

pub mod capture;
pub mod error;
pub mod guards;

pub use capture::ScreenBackend;
pub use error::ScreenError;
pub use guards::{Consent, ConsentGate, LedIndicator, LedScope};
