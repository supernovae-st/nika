//! Media pipeline: extraction, detection, storage, and processing.
//!
//! Handles binary content (images, audio, documents) from MCP tool results.
//! Uses Content-Addressable Storage (CAS) with blake3 hashing.

pub(crate) mod detect;
pub(crate) mod error;
pub(crate) mod processor;
pub(crate) mod store;
#[cfg(test)]
mod tests_e2e;
pub(crate) mod types;

pub(crate) use error::MediaError;
pub(crate) use processor::MediaProcessor;
pub(crate) use store::CasStore;
pub(crate) use types::{MediaBudget, MediaRef};

