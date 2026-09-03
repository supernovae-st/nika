// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-cli-host` — the host-integration member of the `nika-cli` unit
//! (L4 · size-cap split per D-2026-07-09-N1 · ADR-110).
//!
//! One architectural unit, two members (the same law as the run-composer
//! descent of 2026-07-22: compute descends, render stays — here the 15k
//! wall moves the HOST plane): client probes, wire writers, doctor
//! receipts, the vendored client matrix, the context envelope and the
//! trace-retention config. `nika-cli` re-exports every public item at
//! its historical path, so downstream callers see one unbroken surface.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)
)]

pub use display::theme::Theme;
pub use nika_display as display;

pub mod access;
pub mod catalog;
pub(crate) mod choice;
pub mod clients_registry;
pub(crate) mod context_envelope;
pub use context_envelope::find_git_root;
pub(crate) mod detect;
pub mod doctor;
pub mod door;
pub mod experience;
pub mod explain;
pub mod fix_ladder;
pub(crate) mod git;
pub mod harness;
pub mod help_card;
pub mod machine_truth;
pub mod metrics;
pub mod models_rung;
pub mod notify;
pub mod oracle;
pub mod output;
pub mod probe;
pub mod repair;
pub mod retention;
pub mod run_settlement;
pub mod source;
pub mod text;
pub mod var_inputs;
pub mod welcome;
pub mod wire;
