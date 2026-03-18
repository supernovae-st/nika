//! Media pipeline: extraction, detection, storage, and processing.
//!
//! Handles binary content (images, audio, documents) from MCP tool results.
//! Uses Content-Addressable Storage (CAS) with blake3 hashing.

pub mod detect;
pub mod error;
pub mod processor;
pub mod store;
pub mod types;

pub use detect::{detect_mime, DetectedMime, DetectionSource};
pub use error::MediaError;
pub use processor::MediaProcessor;
pub use store::{CasStore, CleanResult, StoreResult};
pub use types::{MediaBudget, MediaRef, MediaType};
