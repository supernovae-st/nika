//! # nika-sdk
//!
//! Rust SDK for the [Nika](https://github.com/supernovae-st/nika) workflow engine.
//!
//! Two transport modes:
//! - **remote** (default) — connects to `nika serve` via HTTP/SSE
//! - **embedded** — runs workflows in-process via `nika-engine`
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use nika_sdk::{Client, RunOptions};
//!
//! # async fn example() -> Result<(), nika_sdk::SdkError> {
//! let client = Client::remote("http://localhost:3000", "my-token")?;
//!
//! let job = client.submit("pipeline.nika.yaml", RunOptions::new()).await?;
//! let result = job.wait().await?;
//! println!("Output: {:?}", result.output);
//! # Ok(())
//! # }
//! ```

mod client;
mod error;
mod sse;
mod transport;
mod types;

#[cfg(feature = "remote")]
mod remote;

#[cfg(feature = "embedded")]
mod embedded;

#[cfg(test)]
mod mock;

// Public API
pub use client::{Artifact, Client, Job};
pub use error::SdkError;
pub use types::{ArtifactInfo, Event, EventStream, JobInfo, JobResult, JobStatus, RunOptions};
