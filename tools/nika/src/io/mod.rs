//! I/O utilities for Nika
//!
//! Shared file operations used across the codebase.
//!
//! # Modules
//!
//! - [`atomic`] - Atomic file write operations (temp + rename pattern)
//! - [`security`] - Path validation and security for artifact output
//! - [`template`] - Variable interpolation for artifact paths
//! - [`writer`] - Artifact writer combining all the above

pub mod atomic;
pub mod security;
pub mod template;
pub mod writer;

// Submodules are pub — consumers use io::atomic::write_atomic directly
