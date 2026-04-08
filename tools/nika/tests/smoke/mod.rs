// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Smoke tests for Nika binary execution.
//!
//! These tests validate the CLI binary works correctly:
//! - Binary compilation
//! - Command execution
//! - Exit codes
//! - Output format
//!
//! Run with: cargo test --test smoke_tests

mod cli_tests;
mod binary_tests;
