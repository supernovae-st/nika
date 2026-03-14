//! Store Module - state management (v0.1)
//!
//! Thread-safe storage for task execution results.
//! Uses DashMap for lock-free concurrent access.
//!
//! Key types:
//! - `DataStore`: Central storage for task results
//! - `TaskResult`: Execution result with status and output
//! - `TaskStatus`: Success or failure status
//! - `LoadedContext`: Loaded workflow context files

pub mod context;
mod datastore;

// Re-export all public types
pub use context::LoadedContext;
pub use datastore::{DataStore, TaskResult, TaskStatus};
