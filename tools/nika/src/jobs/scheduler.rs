//! Job scheduler with 4 trigger types: Cron, Webhook, Watch, Interval.
//!
//! The scheduler manages job triggers and coordinates with the executor.

use crate::jobs::config::{JobDefinition, JobTrigger, JobsConfig, WatchEvent};
use crate::jobs::error::JobsError;
use crate::jobs::state::{ExecutionRecord, JobExecutionStatus, StateStore};
use chrono::{DateTime, Utc};
use cron::Schedule;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Command sent to the scheduler.
#[derive(Debug, Clone)]
pub enum SchedulerCommand {
    /// Trigger a job manually.
    TriggerJob { job_name: String },
    /// Pause a job's schedule.
    PauseJob { job_name: String },
    /// Resume a paused job.
    ResumeJob { job_name: String },
    /// Reload job definitions.
    Reload,
    /// Graceful shutdown.
    Shutdown,
}

/// Event emitted by the scheduler.
#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    /// A job was triggered (ready to execute).
    JobTriggered {
        job_name: String,
        execution_id: String,
        trigger: TriggerSource,
    },
    /// Job execution completed.
    JobCompleted {
        job_name: String,
        execution_id: String,
        success: bool,
        duration_ms: u64,
    },
    /// Scheduler error.
    Error { message: String },
}

/// Source of a job trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerSource {
    /// Cron schedule fired.
    Cron,
    /// Webhook received.
    Webhook { path: String },
    /// File system change detected.
    Watch {
        paths: Vec<PathBuf>,
        event: WatchEventType,
    },
    /// Interval timer fired.
    Interval,
    /// Manual trigger via CLI/API.
    Manual,
}

/// Simplified watch event type for scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEventType {
    Create,
    Modify,
    Delete,
    Rename,
}

impl From<WatchEvent> for WatchEventType {
    fn from(event: WatchEvent) -> Self {
        match event {
            WatchEvent::Create => WatchEventType::Create,
            WatchEvent::Modify => WatchEventType::Modify,
            WatchEvent::Delete => WatchEventType::Delete,
            WatchEvent::Rename => WatchEventType::Rename,
        }
    }
}

/// State of a scheduled job.
#[derive(Debug, Clone)]
pub struct ScheduledJob {
    /// Job definition.
    pub definition: JobDefinition,
    /// Whether the job is paused.
    pub paused: bool,
    /// Next scheduled run time (for cron/interval).
    pub next_run: Option<DateTime<Utc>>,
    /// Currently running execution ID (if exclusive).
    pub running_execution: Option<String>,
}

/// The job scheduler manages triggers and coordinates execution.
pub struct JobScheduler {
    /// Configuration.
    config: Arc<JobsConfig>,
    /// State store for persistence.
    state: Arc<StateStore>,
    /// Scheduled jobs by name.
    jobs: Arc<RwLock<HashMap<String, ScheduledJob>>>,
    /// Command receiver.
    cmd_rx: mpsc::Receiver<SchedulerCommand>,
    /// Event sender.
    event_tx: mpsc::Sender<SchedulerEvent>,
    /// Shutdown flag.
    shutdown: Arc<RwLock<bool>>,
}

impl JobScheduler {
    /// Create a new scheduler.
    pub fn new(
        config: JobsConfig,
        state: Arc<StateStore>,
    ) -> (
        Self,
        mpsc::Sender<SchedulerCommand>,
        mpsc::Receiver<SchedulerEvent>,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::channel(100);
        let (event_tx, event_rx) = mpsc::channel(100);

        let jobs = Self::init_jobs(&config);

        let scheduler = Self {
            config: Arc::new(config),
            state,
            jobs: Arc::new(RwLock::new(jobs)),
            cmd_rx,
            event_tx,
            shutdown: Arc::new(RwLock::new(false)),
        };

        (scheduler, cmd_tx, event_rx)
    }

    /// Initialize scheduled jobs from configuration.
    fn init_jobs(config: &JobsConfig) -> HashMap<String, ScheduledJob> {
        let mut jobs = HashMap::new();

        for def in &config.definitions {
            let next_run = Self::calculate_next_run(&def.trigger);

            jobs.insert(
                def.name.clone(),
                ScheduledJob {
                    definition: def.clone(),
                    paused: !def.enabled,
                    next_run,
                    running_execution: None,
                },
            );
        }

        jobs
    }

    /// Calculate next run time for a trigger.
    fn calculate_next_run(trigger: &JobTrigger) -> Option<DateTime<Utc>> {
        match trigger {
            JobTrigger::Cron(cron) => Self::next_cron_time(&cron.expression).ok(),
            JobTrigger::Interval(interval) => {
                Some(Utc::now() + chrono::Duration::from_std(interval.initial_delay).ok()?)
            }
            JobTrigger::Webhook(_) => None, // Event-driven, no scheduled time
            JobTrigger::Watch(_) => None,   // Event-driven, no scheduled time
        }
    }

    /// Parse cron expression and get next occurrence.
    fn next_cron_time(expression: &str) -> Result<DateTime<Utc>, JobsError> {
        let schedule =
            Schedule::from_str(expression).map_err(|_| JobsError::InvalidCronExpression {
                expression: expression.to_string(),
            })?;

        schedule
            .upcoming(Utc)
            .next()
            .ok_or_else(|| JobsError::TriggerConfigError {
                reason: "Cron expression has no upcoming occurrences".to_string(),
            })
    }

    /// Run the scheduler main loop.
    pub async fn run(mut self) -> Result<(), JobsError> {
        let check_interval = Duration::from_secs(1);

        loop {
            if *self.shutdown.read() {
                break;
            }

            // Process commands
            while let Ok(cmd) = self.cmd_rx.try_recv() {
                self.handle_command(cmd).await?;
            }

            // Check for due jobs
            self.check_scheduled_jobs().await?;

            // Sleep before next check
            tokio::time::sleep(check_interval).await;
        }

        Ok(())
    }

    /// Handle a scheduler command.
    async fn handle_command(&mut self, cmd: SchedulerCommand) -> Result<(), JobsError> {
        match cmd {
            SchedulerCommand::TriggerJob { job_name } => {
                self.trigger_job(&job_name, TriggerSource::Manual).await?;
            }
            SchedulerCommand::PauseJob { job_name } => {
                let mut jobs = self.jobs.write();
                if let Some(job) = jobs.get_mut(&job_name) {
                    job.paused = true;
                }
            }
            SchedulerCommand::ResumeJob { job_name } => {
                let mut jobs = self.jobs.write();
                if let Some(job) = jobs.get_mut(&job_name) {
                    job.paused = false;
                    // Recalculate next run time
                    job.next_run = Self::calculate_next_run(&job.definition.trigger);
                }
            }
            SchedulerCommand::Reload => {
                // Re-initialize jobs from config
                let jobs = Self::init_jobs(&self.config);
                *self.jobs.write() = jobs;
            }
            SchedulerCommand::Shutdown => {
                *self.shutdown.write() = true;
            }
        }

        Ok(())
    }

    /// Check for jobs that are due to run.
    async fn check_scheduled_jobs(&self) -> Result<(), JobsError> {
        let now = Utc::now();
        let jobs_to_trigger: Vec<(String, JobTrigger)> = {
            let jobs = self.jobs.read();
            jobs.iter()
                .filter(|(_, job)| {
                    !job.paused
                        && job.running_execution.is_none()
                        && job.next_run.map(|t| t <= now).unwrap_or(false)
                })
                .map(|(name, job)| (name.clone(), job.definition.trigger.clone()))
                .collect()
        };

        for (job_name, trigger) in jobs_to_trigger {
            let source = match trigger {
                JobTrigger::Cron(_) => TriggerSource::Cron,
                JobTrigger::Interval(_) => TriggerSource::Interval,
                _ => continue, // Webhook and Watch are event-driven
            };

            self.trigger_job(&job_name, source).await?;
        }

        Ok(())
    }

    /// Trigger a job execution.
    pub async fn trigger_job(
        &self,
        job_name: &str,
        source: TriggerSource,
    ) -> Result<String, JobsError> {
        let execution_id = Uuid::new_v4().to_string();

        // Check if job exists and is not already running
        {
            let mut jobs = self.jobs.write();
            let job = jobs
                .get_mut(job_name)
                .ok_or_else(|| JobsError::JobNotFound {
                    name: job_name.to_string(),
                })?;

            if job.running_execution.is_some() {
                return Err(JobsError::JobAlreadyRunning {
                    name: job_name.to_string(),
                });
            }

            job.running_execution = Some(execution_id.clone());
        }

        // Create execution record
        let trigger_str = match &source {
            TriggerSource::Cron => "cron",
            TriggerSource::Webhook { .. } => "webhook",
            TriggerSource::Watch { .. } => "watch",
            TriggerSource::Interval => "interval",
            TriggerSource::Manual => "manual",
        };

        let record = ExecutionRecord {
            id: execution_id.clone(),
            job_name: job_name.to_string(),
            status: JobExecutionStatus::Queued,
            trigger: trigger_str.to_string(),
            started_at: Utc::now(),
            ended_at: None,
            duration_ms: None,
            error: None,
            attempt: 1,
            output: None,
        };

        self.state.insert_execution(&record)?;

        // Emit event
        let _ = self
            .event_tx
            .send(SchedulerEvent::JobTriggered {
                job_name: job_name.to_string(),
                execution_id: execution_id.clone(),
                trigger: source,
            })
            .await;

        Ok(execution_id)
    }

    /// Mark a job execution as completed.
    pub fn complete_execution(
        &self,
        job_name: &str,
        execution_id: &str,
        success: bool,
        duration_ms: u64,
        error: Option<String>,
        output: Option<String>,
    ) -> Result<(), JobsError> {
        // Get existing record and update it
        let existing = self.state.get_execution(execution_id)?.ok_or_else(|| {
            JobsError::ExecutionNotFound {
                execution_id: execution_id.to_string(),
            }
        })?;

        let updated_record = ExecutionRecord {
            status: if success {
                JobExecutionStatus::Completed
            } else {
                JobExecutionStatus::Failed
            },
            ended_at: Some(Utc::now()),
            duration_ms: Some(duration_ms),
            error,
            output,
            ..existing
        };

        self.state.update_execution(&updated_record)?;

        // Clear running execution and calculate next run
        {
            let mut jobs = self.jobs.write();
            if let Some(job) = jobs.get_mut(job_name) {
                job.running_execution = None;

                // Calculate next run for interval/cron jobs
                match &job.definition.trigger {
                    JobTrigger::Cron(cron) => {
                        job.next_run = Self::next_cron_time(&cron.expression).ok();
                    }
                    JobTrigger::Interval(interval) => {
                        job.next_run =
                            Some(Utc::now() + chrono::Duration::from_std(interval.every).unwrap());
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Handle a webhook trigger.
    pub async fn handle_webhook(&self, path: &str) -> Result<String, JobsError> {
        // Find job with matching webhook path
        let job_name = {
            let jobs = self.jobs.read();
            jobs.iter()
                .find(|(_, job)| {
                    if let JobTrigger::Webhook(config) = &job.definition.trigger {
                        config.path == path
                    } else {
                        false
                    }
                })
                .map(|(name, _)| name.clone())
        };

        match job_name {
            Some(name) => {
                self.trigger_job(
                    &name,
                    TriggerSource::Webhook {
                        path: path.to_string(),
                    },
                )
                .await
            }
            None => Err(JobsError::JobNotFound {
                name: format!("webhook:{}", path),
            }),
        }
    }

    /// Handle a file watch event.
    pub async fn handle_watch_event(
        &self,
        changed_paths: Vec<PathBuf>,
        event_type: WatchEventType,
    ) -> Result<Vec<String>, JobsError> {
        let mut triggered_ids = Vec::new();

        // Find all jobs that match the changed paths
        let matching_jobs: Vec<String> = {
            let jobs = self.jobs.read();
            jobs.iter()
                .filter(|(_, job)| {
                    if let JobTrigger::Watch(config) = &job.definition.trigger {
                        Self::paths_match(
                            &changed_paths,
                            &config.paths,
                            &config.events,
                            &event_type,
                        )
                    } else {
                        false
                    }
                })
                .map(|(name, _)| name.clone())
                .collect()
        };

        for job_name in matching_jobs {
            match self
                .trigger_job(
                    &job_name,
                    TriggerSource::Watch {
                        paths: changed_paths.clone(),
                        event: event_type.clone(),
                    },
                )
                .await
            {
                Ok(id) => triggered_ids.push(id),
                Err(JobsError::JobAlreadyRunning { .. }) => {
                    // Skip, job is already running (debounce)
                }
                Err(e) => return Err(e),
            }
        }

        Ok(triggered_ids)
    }

    /// Check if changed paths match the watch configuration.
    fn paths_match(
        changed: &[PathBuf],
        patterns: &[String],
        allowed_events: &[WatchEvent],
        event_type: &WatchEventType,
    ) -> bool {
        // Check if event type is allowed
        let event_allowed = allowed_events.iter().any(|e| {
            let e_type: WatchEventType = (*e).into();
            &e_type == event_type
        });

        if !event_allowed {
            return false;
        }

        // Check if any changed path matches any pattern
        for path in changed {
            let path_str = path.to_string_lossy();
            for pattern in patterns {
                if glob_match(pattern, &path_str) {
                    return true;
                }
            }
        }

        false
    }

    /// Get job status.
    pub fn get_job(&self, name: &str) -> Option<ScheduledJob> {
        self.jobs.read().get(name).cloned()
    }

    /// List all scheduled jobs.
    pub fn list_jobs(&self) -> Vec<ScheduledJob> {
        self.jobs.read().values().cloned().collect()
    }

    /// Get jobs statistics.
    pub fn stats(&self) -> SchedulerStats {
        let jobs = self.jobs.read();
        let total = jobs.len();
        let enabled = jobs.values().filter(|j| !j.paused).count();
        let running = jobs
            .values()
            .filter(|j| j.running_execution.is_some())
            .count();

        SchedulerStats {
            total_jobs: total,
            enabled_jobs: enabled,
            running_jobs: running,
            paused_jobs: total - enabled,
        }
    }
}

/// Scheduler statistics.
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    pub total_jobs: usize,
    pub enabled_jobs: usize,
    pub running_jobs: usize,
    pub paused_jobs: usize,
}

/// Simple glob matching for watch paths.
fn glob_match(pattern: &str, path: &str) -> bool {
    // Simple glob implementation supporting * and **
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();

    glob_match_parts(&pattern_parts, &path_parts)
}

fn glob_match_parts(pattern: &[&str], path: &[&str]) -> bool {
    if pattern.is_empty() {
        return path.is_empty();
    }

    if path.is_empty() {
        return pattern.iter().all(|p| *p == "**");
    }

    let p = pattern[0];

    if p == "**" {
        // ** matches zero or more directories
        for i in 0..=path.len() {
            if glob_match_parts(&pattern[1..], &path[i..]) {
                return true;
            }
        }
        false
    } else if p == "*" {
        // * matches any single component
        glob_match_parts(&pattern[1..], &path[1..])
    } else if p.contains('*') {
        // Pattern like *.yaml
        let prefix = p.split('*').next().unwrap_or("");
        let suffix = p.split('*').next_back().unwrap_or("");
        if path[0].starts_with(prefix) && path[0].ends_with(suffix) {
            glob_match_parts(&pattern[1..], &path[1..])
        } else {
            false
        }
    } else {
        // Exact match
        p == path[0] && glob_match_parts(&pattern[1..], &path[1..])
    }
}

#[cfg(test)]
#[allow(clippy::arc_with_non_send_sync)]
mod tests {
    use super::*;
    use crate::jobs::config::{CronTriggerConfig, IntervalTriggerConfig, WebhookTriggerConfig};

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("foo/bar.yaml", "foo/bar.yaml"));
        assert!(!glob_match("foo/bar.yaml", "foo/baz.yaml"));
    }

    #[test]
    fn test_glob_match_star() {
        assert!(glob_match("foo/*.yaml", "foo/bar.yaml"));
        assert!(glob_match("foo/*.yaml", "foo/baz.yaml"));
        assert!(!glob_match("foo/*.yaml", "bar/baz.yaml"));
    }

    #[test]
    fn test_glob_match_double_star() {
        assert!(glob_match("**/*.yaml", "foo/bar.yaml"));
        assert!(glob_match("**/*.yaml", "foo/bar/baz.yaml"));
        assert!(glob_match("data/**/*.json", "data/foo/bar.json"));
    }

    #[test]
    fn test_trigger_source_display() {
        let source = TriggerSource::Cron;
        assert_eq!(source, TriggerSource::Cron);

        let source = TriggerSource::Webhook {
            path: "/jobs/test".to_string(),
        };
        assert_eq!(
            source,
            TriggerSource::Webhook {
                path: "/jobs/test".to_string()
            }
        );
    }

    #[test]
    fn test_next_cron_time_valid() {
        // Every minute (6-field cron: second minute hour day_of_month month day_of_week)
        let result = JobScheduler::next_cron_time("0 * * * * *");
        assert!(result.is_ok());
        assert!(result.unwrap() > Utc::now());
    }

    #[test]
    fn test_next_cron_time_invalid() {
        let result = JobScheduler::next_cron_time("invalid cron");
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_next_run_cron() {
        // 6-field cron: second minute hour day_of_month month day_of_week
        let trigger = JobTrigger::Cron(CronTriggerConfig {
            expression: "0 * * * * *".to_string(),
            timezone: "UTC".to_string(),
        });

        let next = JobScheduler::calculate_next_run(&trigger);
        assert!(next.is_some());
    }

    #[test]
    fn test_calculate_next_run_interval() {
        let trigger = JobTrigger::Interval(IntervalTriggerConfig {
            every: Duration::from_secs(60),
            initial_delay: Duration::from_secs(10),
        });

        let next = JobScheduler::calculate_next_run(&trigger);
        assert!(next.is_some());
        // Should be ~10 seconds from now
        let delta = next.unwrap() - Utc::now();
        assert!(delta.num_seconds() <= 11 && delta.num_seconds() >= 9);
    }

    #[test]
    fn test_calculate_next_run_webhook() {
        let trigger = JobTrigger::Webhook(WebhookTriggerConfig {
            path: "/jobs/test".to_string(),
            method: "POST".to_string(),
            secret: None,
        });

        let next = JobScheduler::calculate_next_run(&trigger);
        assert!(next.is_none()); // Webhooks are event-driven
    }

    #[test]
    fn test_scheduler_stats() {
        let config = JobsConfig {
            definitions: vec![
                JobDefinition {
                    name: "job1".to_string(),
                    workflow: PathBuf::from("workflow1.yaml"),
                    enabled: true,
                    trigger: JobTrigger::Interval(IntervalTriggerConfig {
                        every: Duration::from_secs(60),
                        initial_delay: Duration::ZERO,
                    }),
                    retry: Default::default(),
                    timeout: Duration::from_secs(300),
                    env: Default::default(),
                    tags: vec![],
                },
                JobDefinition {
                    name: "job2".to_string(),
                    workflow: PathBuf::from("workflow2.yaml"),
                    enabled: false,
                    trigger: JobTrigger::Interval(IntervalTriggerConfig {
                        every: Duration::from_secs(120),
                        initial_delay: Duration::ZERO,
                    }),
                    retry: Default::default(),
                    timeout: Duration::from_secs(300),
                    env: Default::default(),
                    tags: vec![],
                },
            ],
            ..Default::default()
        };

        let state = Arc::new(StateStore::in_memory().unwrap());
        let (scheduler, _, _) = JobScheduler::new(config, state);

        let stats = scheduler.stats();
        assert_eq!(stats.total_jobs, 2);
        assert_eq!(stats.enabled_jobs, 1);
        assert_eq!(stats.paused_jobs, 1);
        assert_eq!(stats.running_jobs, 0);
    }

    #[test]
    fn test_paths_match() {
        let changed = vec![PathBuf::from("data/input.json")];
        let patterns = vec!["data/**/*.json".to_string()];
        let events = vec![WatchEvent::Create, WatchEvent::Modify];

        assert!(JobScheduler::paths_match(
            &changed,
            &patterns,
            &events,
            &WatchEventType::Create
        ));
        assert!(JobScheduler::paths_match(
            &changed,
            &patterns,
            &events,
            &WatchEventType::Modify
        ));
        assert!(!JobScheduler::paths_match(
            &changed,
            &patterns,
            &events,
            &WatchEventType::Delete
        ));
    }
}
