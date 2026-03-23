//! Media pipeline — re-exports from nika-media crate.
//!
//! Core types (CasStore, MediaRef, MediaError) come from nika-media.
//! Test modules stay here (depend on runtime/store types).

pub use nika_media::*;

#[cfg(test)]
mod tests_e2e;
