// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-kernel-core` — the BASE sibling of the split kernel (L0.5).
//!
//! Carries the foundation every other kernel sibling depends on
//! (`docs/architecture/kernel-split-census-2026-06-10.md`) ·
//!
//! - [`io`] — filesystem, HTTP, shell process, blob storage, clock,
//!   screen capture, OCR, accessibility, input, browser (19 traits)
//! - [`infra`] — audit, billing, events, metrics, tracing, IDs,
//!   secrets (7 traits)
//! - [`cancel`] · [`sealed`] · [`types`] — shared root primitives
//!
//! **Zero implementations live here.** Contracts only — implementations
//! live in L1 effect crates; test mocks in `nika-kernel-mock`.
//!
//! Downstream crates import through the `nika-kernel` facade
//! (`nika_kernel::io::…` · `nika_kernel::Fs` · …) — depending on this
//! crate directly is reserved for kernel siblings.

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

pub mod cancel;
pub mod errors;
pub mod infra;
pub mod io;
pub mod sealed;
pub mod types;
