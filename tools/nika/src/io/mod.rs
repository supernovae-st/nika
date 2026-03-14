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

// Re-export main functions for convenience
pub use atomic::{write_append, write_atomic, write_fail, write_unique};
pub use security::{resolve_artifact_dir, validate_artifact_path, DEFAULT_ARTIFACT_DIR};
pub use template::TemplateResolver;
pub use writer::{ArtifactWriter, WriteRequest, WriteResult};
