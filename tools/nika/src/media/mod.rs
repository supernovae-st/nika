//! Media pipeline: extraction, detection, storage, and processing.
//!
//! Handles binary content (images, audio, documents) from MCP tool results.
//! Uses Content-Addressable Storage (CAS) with blake3 hashing.

pub mod error;

pub use error::MediaError;
