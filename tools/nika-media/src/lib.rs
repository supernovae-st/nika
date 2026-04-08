// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Media Module — Content-addressable storage and media processing
//!
//! Core media infrastructure for Nika:
//! - `CasStore`: Content-addressable storage with blake3 hashing
//! - `MediaProcessor`: Extract and process media from MCP responses
//! - `MediaRef`: Reference to a stored media file
//! - `MediaError`: Media pipeline errors (NIKA-251-259)
//!
pub mod detect;
pub mod error;
pub mod processor;
pub mod store;
pub mod tools;
pub mod types;

// Re-export key types
pub use error::MediaError;
pub use processor::MediaProcessor;
pub use store::CasStore;
pub use types::{MediaBudget, MediaRef, MediaType};
