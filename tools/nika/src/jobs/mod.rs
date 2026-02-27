//! Jobs Daemon — Background workflow scheduler for Nika.
//!
//! This module provides a daemon for scheduled workflow execution with:
//! - 4 trigger types: Cron, Webhook, Watch, Interval
//! - SQLite state persistence
//! - Retry with exponential backoff
//! - Notification channels (Slack, Email)
//!
//! # Usage
//!
//! ```bash
//! # Start the daemon
//! nika jobs start
//!
//! # Check status
//! nika jobs status
//!
//! # Stop the daemon
//! nika jobs stop
//!
//! # List jobs
//! nika jobs list
//!
//! # Trigger a job manually
//! nika jobs trigger <job-name>
//! ```
//!
//! # Configuration
//!
//! Jobs are configured in `.nika/jobs.toml`:
//!
//! ```toml
//! [daemon]
//! enabled = true
//! max_concurrent_jobs = 10
//!
//! [[jobs]]
//! name = "daily-report"
//! workflow = "workflows/report.nika.yaml"
//! [jobs.trigger]
//! type = "cron"
//! expression = "0 9 * * *"
//! timezone = "America/New_York"
//!
//! [[jobs]]
//! name = "on-change"
//! workflow = "workflows/process.nika.yaml"
//! [jobs.trigger]
//! type = "watch"
//! paths = ["data/**/*.json"]
//! debounce = "5s"
//! ```

pub mod config;
pub mod error;

#[cfg(feature = "jobs")]
pub mod daemon;
#[cfg(feature = "jobs")]
pub mod notify;
#[cfg(feature = "jobs")]
pub mod retry;
#[cfg(feature = "jobs")]
pub mod scheduler;
#[cfg(feature = "jobs")]
pub mod state;

// Re-exports for convenience
pub use config::{
    BackoffStrategy, CronTriggerConfig, IntervalTriggerConfig, JobDefinition, JobTrigger,
    JobsConfig, RetryConfig, WatchEvent, WatchTriggerConfig, WebhookTriggerConfig,
};
pub use error::JobsError;

#[cfg(feature = "jobs")]
pub use daemon::{DaemonStatus, JobsDaemon};
#[cfg(feature = "jobs")]
pub use scheduler::JobScheduler;
#[cfg(feature = "jobs")]
pub use state::{ExecutionRecord, JobExecutionStatus, JobStats, StateStore};

/// Result type for Jobs Daemon operations.
pub type JobsResult<T> = Result<T, JobsError>;
