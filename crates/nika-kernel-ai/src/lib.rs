// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-kernel-ai` — the AI sibling of the split kernel (L0.5).
//!
//! Provider inference, memory (the Connectome contract surface),
//! vision, audio (R6 seam · stt·tts·vad), context compression,
//! `GenAI` `OTel` attributes — 17 traits
//! (`docs/architecture/kernel-split-census-2026-06-10.md`).
//!
//! **Zero implementations live here.** Contracts only.
//!
//! Downstream crates import through the `nika-kernel` facade
//! (`nika_kernel::ai::…` · `nika_kernel::Provider` · …) — depending on
//! this crate directly is reserved for kernel siblings.

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

pub mod audio;
pub mod context;
pub mod errors;
pub mod genai;
pub mod harness;
pub mod memory;
pub mod provider;
pub mod tool_defs;
pub mod vision;
