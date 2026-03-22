//! Media Module — Content-addressable storage and media processing
//!
//! Core media infrastructure for Nika:
//! - `CasStore`: Content-addressable storage with blake3 hashing
//! - `MediaProcessor`: Extract and process media from MCP responses
//! - `MediaRef`: Reference to a stored media file
//! - `MediaError`: Media pipeline errors (NIKA-251-259)
//!
//! The builtin media tools (nika:thumbnail, nika:import, etc.) remain
//! in the nika crate's runtime module.

pub mod detect;
pub mod error;
pub mod processor;
pub mod store;
pub mod types;

// Re-export key types
pub use error::MediaError;
pub use processor::MediaProcessor;
pub use store::CasStore;
pub use types::{MediaBudget, MediaRef, MediaType};
