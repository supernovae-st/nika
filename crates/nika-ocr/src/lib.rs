// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-ocr` · OCR text-extraction L1 effect crate (M2.2).
//!
//! Implements the L0.5 `nika_kernel::io::ocr::OcrEngineDyn` trait (the `Send`
//! variant — the local `OcrEngine` arrives via the kernel's one-way blanket
//! impl) — `read` +
//! `read_region` — extracting `TextRegion` records (text · bbox · confidence ·
//! BCP-47 language) from a captured RGBA8 `Frame`. The OCR inference is
//! delegated to the **pure-Rust `ocrs`** backend (`rten` runtime · no C system
//! dependency), so this crate honours `unsafe_code = "forbid"` — the same
//! sovereign posture `nika-screen` gets from `xcap`.
//!
//! The `OcrEngine` methods validate their input purely (frame RGBA8 length +
//! sub-region bounds · headless-testable) then run the real `ocrs` pipeline
//! inside `tokio::task::spawn_blocking` (the engine is sync + CPU-bound).
//! [`OcrBackend::with_models`] loads the detection + recognition `.rten`
//! weights from explicit local paths. Error taxonomy NIKA-1101..1109
//! (ADR-081 nika-ocr sub-range NIKA-1100..1199 · NIKA-1100 = retired B.2
//! placeholder slot).
//!
//! Model files (`.rten` detection + recognition) are provisioned locally by
//! the caller (sovereignty · telemetry-canon §0 · zero cloud · never an
//! auto-download); the engine takes a path. See `docs/crate-specs/nika-ocr.md`.

// Test-only relaxation of the workspace zero-unwrap / zero-expect deny lints
// (production src stays clean · enforced by clippy unwrap_used = deny).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod error;
mod recognize;

pub use error::OcrError;
pub use recognize::OcrBackend;
