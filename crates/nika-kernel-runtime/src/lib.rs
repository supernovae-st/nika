// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-kernel-runtime` — the runtime sibling of the split kernel (L0.5).
//!
//! Tool execution (3 traits) + the agent-loop type surfaces
//! (`docs/architecture/kernel-split-census-2026-06-10.md`). The L2/L3
//! trait-growth bucket — agent/checkpoint contracts land HERE.
//!
//! **Zero implementations live here.** Contracts only.
//!
//! Downstream crates import through the `nika-kernel` facade
//! (`nika_kernel::runtime::…` · `nika_kernel::ToolExecutor` · …) —
//! depending on this crate directly is reserved for kernel siblings.

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

pub mod agent;
pub mod errors;
pub mod tool_executor;
