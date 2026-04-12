// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! I/O utilities for Nika
//!
//! Shared file operations used across the codebase.
//!
//! # Modules
//!
//! - [`atomic`] - Atomic file write operations (temp + rename pattern)
//! - [`security`] - Path validation — re-exported from the `nika-security::path` L1 module (extracted S18)
//! - [`template`] - Variable interpolation for artifact paths
//! - [`writer`] - Artifact writer combining all the above

pub mod atomic;
pub mod template;
pub mod writer;

// Submodules are pub — consumers use io::atomic::write_atomic directly
