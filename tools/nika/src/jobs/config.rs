//! Jobs Daemon configuration types.
//!
//! This module defines the canonical types for job configuration.
//! All other modules MUST align with these types.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::error::JobsError;

/// Root configuration for the Jobs Daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobsConfig {
    /// Whether the daemon is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Path to the PID file for daemon lifecycle.
    #[serde(default = "default_pid_file")]
    pub pid_file: PathBuf,

    /// Path to the SQLite state database.
    #[serde(default = "default_state_db")]
    pub state_db: PathBuf,

    /// Directory for job execution logs.
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,

    /// Port for metrics/health endpoint.
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,

    /// Maximum concurrent job executions.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_jobs: usize,

    /// Default timeout for job executions (seconds).
    #[serde(default = "default_timeout_secs")]
    pub default_timeout_secs: u64,

    /// Webhook server configuration.
    #[serde(default)]
    pub webhook: WebhookConfig,

    /// Notification channel configuration.
    #[serde(default)]
    pub notify: NotifyConfig,

    /// Job definitions.
    #[serde(default)]
    pub definitions: Vec<JobDefinition>,
}

impl Default for JobsConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            pid_file: default_pid_file(),
            state_db: default_state_db(),
            log_dir: default_log_dir(),
            metrics_port: default_metrics_port(),
            max_concurrent_jobs: default_max_concurrent(),
            default_timeout_secs: default_timeout_secs(),
            webhook: WebhookConfig::default(),
            notify: NotifyConfig::default(),
            definitions: Vec::new(),
        }
    }
}

impl JobsConfig {
    /// Load configuration from a TOML file.
    ///
    /// # Arguments
    /// * `path` - Path to the TOML configuration file
    ///
    /// # Returns
    /// * `Ok(JobsConfig)` - Parsed configuration
    /// * `Err(JobsError)` - If file not found or parse error
    pub fn from_file(path: &Path) -> Result<Self, JobsError> {
        if !path.exists() {
            return Err(JobsError::ConfigNotFound {
                path: path.to_path_buf(),
            });
        }

        let content = std::fs::read_to_string(path).map_err(|e| JobsError::IoError {
            reason: format!("Failed to read config file: {}", e),
        })?;

        toml::from_str(&content).map_err(|e| JobsError::ConfigParseError {
            reason: format!("Failed to parse TOML: {}", e),
        })
    }
}

fn default_enabled() -> bool {
    true
}

fn default_pid_file() -> PathBuf {
    PathBuf::from(".nika/jobs/daemon.pid")
}

fn default_state_db() -> PathBuf {
    PathBuf::from(".nika/jobs/state.db")
}

fn default_log_dir() -> PathBuf {
    PathBuf::from(".nika/jobs/logs")
}

fn default_metrics_port() -> u16 {
    9090
}

fn default_max_concurrent() -> usize {
    10
}

fn default_timeout_secs() -> u64 {
    300 // 5 minutes
}

/// Webhook server configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebhookConfig {
    /// Enable webhook server.
    #[serde(default)]
    pub enabled: bool,

    /// Port for webhook server.
    #[serde(default = "default_webhook_port")]
    pub port: u16,

    /// Bind address.
    #[serde(default = "default_webhook_host")]
    pub host: String,

    /// Authentication token (optional).
    pub auth_token: Option<String>,
}

fn default_webhook_port() -> u16 {
    8080
}

fn default_webhook_host() -> String {
    "127.0.0.1".to_string()
}

/// Notification configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotifyConfig {
    /// Slack webhook URL.
    pub slack_webhook: Option<String>,

    /// Email configuration.
    pub email: Option<EmailConfig>,

    /// Notify on job failure.
    #[serde(default = "default_true")]
    pub on_failure: bool,

    /// Notify on job success.
    #[serde(default)]
    pub on_success: bool,
}

fn default_true() -> bool {
    true
}

/// Email notification configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    /// SMTP server host.
    pub smtp_host: String,

    /// SMTP server port.
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,

    /// SMTP username.
    pub username: Option<String>,

    /// SMTP password.
    pub password: Option<String>,

    /// From address.
    pub from: String,

    /// To addresses.
    pub to: Vec<String>,
}

fn default_smtp_port() -> u16 {
    587
}

/// A job definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDefinition {
    /// Unique job name.
    pub name: String,

    /// Path to the workflow file.
    pub workflow: PathBuf,

    /// Whether the job is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Trigger configuration (determines when job runs).
    pub trigger: JobTrigger,

    /// Retry configuration.
    #[serde(default)]
    pub retry: RetryConfig,

    /// Timeout for this job (overrides default).
    #[serde(with = "humantime_serde", default = "default_job_timeout")]
    pub timeout: Duration,

    /// Environment variables for job execution.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,

    /// Tags for filtering/grouping.
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_job_timeout() -> Duration {
    Duration::from_secs(300)
}

/// Job trigger types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobTrigger {
    /// Cron-based scheduling.
    Cron(CronTriggerConfig),

    /// Webhook-triggered execution.
    Webhook(WebhookTriggerConfig),

    /// File system watch trigger.
    Watch(WatchTriggerConfig),

    /// Fixed interval trigger.
    Interval(IntervalTriggerConfig),
}

/// Cron trigger configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTriggerConfig {
    /// Cron expression (e.g., "0 0 * * *" for daily at midnight).
    pub expression: String,

    /// Timezone for cron evaluation (default: UTC).
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

/// Webhook trigger configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookTriggerConfig {
    /// Endpoint path (e.g., "/jobs/my-job/trigger").
    pub path: String,

    /// HTTP method (default: POST).
    #[serde(default = "default_http_method")]
    pub method: String,

    /// Required secret in Authorization header.
    pub secret: Option<String>,
}

fn default_http_method() -> String {
    "POST".to_string()
}

/// File watch trigger configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchTriggerConfig {
    /// Paths to watch (supports globs).
    pub paths: Vec<String>,

    /// Debounce duration to prevent rapid re-triggers.
    #[serde(with = "humantime_serde", default = "default_debounce")]
    pub debounce: Duration,

    /// Watch for specific events.
    #[serde(default = "default_watch_events")]
    pub events: Vec<WatchEvent>,
}

fn default_debounce() -> Duration {
    Duration::from_secs(5)
}

fn default_watch_events() -> Vec<WatchEvent> {
    vec![WatchEvent::Create, WatchEvent::Modify]
}

/// File watch events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchEvent {
    Create,
    Modify,
    Delete,
    Rename,
}

/// Interval trigger configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntervalTriggerConfig {
    /// Interval duration between executions.
    #[serde(with = "humantime_serde")]
    pub every: Duration,

    /// Delay before first execution.
    #[serde(with = "humantime_serde", default)]
    pub initial_delay: Duration,
}

/// Retry configuration for failed jobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum retry attempts.
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    /// Backoff strategy.
    #[serde(default)]
    pub backoff: BackoffStrategy,

    /// Initial delay before first retry.
    #[serde(with = "humantime_serde", default = "default_initial_delay")]
    pub initial_delay: Duration,

    /// Maximum delay between retries.
    #[serde(with = "humantime_serde", default = "default_max_delay")]
    pub max_delay: Duration,

    /// Add jitter to retry delays.
    #[serde(default = "default_true")]
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            backoff: BackoffStrategy::default(),
            initial_delay: default_initial_delay(),
            max_delay: default_max_delay(),
            jitter: true,
        }
    }
}

fn default_max_attempts() -> u32 {
    3
}

fn default_initial_delay() -> Duration {
    Duration::from_secs(1)
}

fn default_max_delay() -> Duration {
    Duration::from_secs(60)
}

/// Backoff strategy for retries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackoffStrategy {
    /// Constant delay between retries.
    Constant,

    /// Linear increase (delay * attempt).
    Linear,

    /// Exponential increase (delay * 2^attempt).
    #[default]
    Exponential,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = JobsConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_concurrent_jobs, 10);
        assert_eq!(config.metrics_port, 9090);
    }

    #[test]
    fn test_cron_trigger_serde() {
        let trigger = JobTrigger::Cron(CronTriggerConfig {
            expression: "0 0 * * *".to_string(),
            timezone: "UTC".to_string(),
        });

        let json = serde_json::to_string(&trigger).unwrap();
        assert!(json.contains("cron"));
        assert!(json.contains("0 0 * * *"));
    }

    #[test]
    fn test_interval_trigger_serde() {
        let trigger = JobTrigger::Interval(IntervalTriggerConfig {
            every: Duration::from_secs(3600),
            initial_delay: Duration::from_secs(0),
        });

        let json = serde_json::to_string(&trigger).unwrap();
        assert!(json.contains("interval"));
    }

    #[test]
    fn test_retry_config_default() {
        let retry = RetryConfig::default();
        assert_eq!(retry.max_attempts, 3);
        assert_eq!(retry.backoff, BackoffStrategy::Exponential);
        assert!(retry.jitter);
    }

    #[test]
    fn test_job_definition() {
        let job = JobDefinition {
            name: "test-job".to_string(),
            workflow: PathBuf::from("workflow.nika.yaml"),
            enabled: true,
            trigger: JobTrigger::Interval(IntervalTriggerConfig {
                every: Duration::from_secs(60),
                initial_delay: Duration::from_secs(0),
            }),
            retry: RetryConfig::default(),
            timeout: Duration::from_secs(120),
            env: std::collections::HashMap::new(),
            tags: vec!["test".to_string()],
        };

        assert_eq!(job.name, "test-job");
        assert!(job.enabled);
    }
}
