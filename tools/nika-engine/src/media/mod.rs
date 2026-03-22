//! Media pipeline — re-exports from nika-media crate.
//!
//! Core types (CasStore, MediaRef, MediaError) come from nika-media.
//! Test modules stay here (depend on runtime/store types).

pub use nika_media::*;

#[cfg(test)]
mod tests_e2e;
// Note: tests_compression_deep.rs uses stale 1-byte CAS markers.
// Not included until updated to 4-byte NK framing.
