//! Jobs Daemon Integration Tests
//!
//! Tests the interaction between Jobs Daemon components:
//! - Config → Scheduler → State
//! - Retry → State persistence
//! - Notification dispatching
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test jobs_integration_test --features jobs
//! cargo nextest run --test jobs_integration_test --features jobs
//! ```

#![cfg(feature = "jobs")]
#![allow(clippy::arc_with_non_send_sync)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nika::jobs::config::{NotifyConfig, WebhookConfig};
use nika::jobs::{
    BackoffStrategy, CronTriggerConfig, ExecutionRecord, IntervalTriggerConfig, JobDefinition,
    JobExecutionStatus, JobScheduler, JobTrigger, JobsConfig, JobsError, RetryConfig, StateStore,
    WatchEvent, WatchTriggerConfig, WebhookTriggerConfig,
};

// ============================================================================
// Test Helpers
// ============================================================================

fn test_config() -> JobsConfig {
    JobsConfig {
        enabled: true,
        pid_file: PathBuf::from("/tmp/nika-test.pid"),
        state_db: PathBuf::from(":memory:"),
        log_dir: PathBuf::from("/tmp/nika-logs"),
        metrics_port: 9090,
        max_concurrent_jobs: 5,
        default_timeout_secs: 300,
        webhook: WebhookConfig::default(),
        notify: NotifyConfig::default(),
        definitions: vec![
            JobDefinition {
                name: "daily-report".to_string(),
                workflow: PathBuf::from("workflows/report.nika.yaml"),
                enabled: true,
                trigger: JobTrigger::Cron(CronTriggerConfig {
                    expression: "0 9 * * *".to_string(),
                    timezone: "UTC".to_string(),
                }),
                retry: RetryConfig::default(),
                timeout: Duration::from_secs(300),
                env: HashMap::new(),
                tags: vec![],
            },
            JobDefinition {
                name: "hourly-sync".to_string(),
                workflow: PathBuf::from("workflows/sync.nika.yaml"),
                enabled: true,
                trigger: JobTrigger::Interval(IntervalTriggerConfig {
                    every: Duration::from_secs(3600),
                    initial_delay: Duration::from_secs(0),
                }),
                retry: RetryConfig {
                    max_attempts: 3,
                    backoff: BackoffStrategy::Exponential,
                    initial_delay: Duration::from_millis(1000),
                    max_delay: Duration::from_millis(30000),
                    jitter: true,
                },
                timeout: Duration::from_secs(600),
                env: HashMap::new(),
                tags: vec![],
            },
        ],
    }
}

// ============================================================================
// Config → Scheduler Integration
// ============================================================================

#[test]
fn test_scheduler_loads_config_definitions() {
    let config = test_config();
    let state = Arc::new(StateStore::in_memory().expect("Failed to create in-memory store"));
    let (scheduler, _cmd_tx, _event_rx) = JobScheduler::new(config.clone(), state);

    // Scheduler should have all jobs from config
    let jobs = scheduler.list_jobs();
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().any(|j| j.definition.name == "daily-report"));
    assert!(jobs.iter().any(|j| j.definition.name == "hourly-sync"));
}

#[test]
fn test_scheduler_respects_enabled_flag() {
    let mut config = test_config();
    config.definitions[0].enabled = false; // Disable daily-report

    let state = Arc::new(StateStore::in_memory().expect("Failed to create in-memory store"));
    let (scheduler, _cmd_tx, _event_rx) = JobScheduler::new(config, state);
    let jobs = scheduler.list_jobs();
    let enabled_jobs: Vec<_> = jobs.iter().filter(|j| j.definition.enabled).collect();

    assert_eq!(enabled_jobs.len(), 1);
    assert_eq!(enabled_jobs[0].definition.name, "hourly-sync");
}

#[test]
fn test_scheduler_stats_initial() {
    let config = test_config();
    let state = Arc::new(StateStore::in_memory().expect("Failed to create in-memory store"));
    let (scheduler, _cmd_tx, _event_rx) = JobScheduler::new(config, state);

    let stats = scheduler.stats();
    assert_eq!(stats.total_jobs, 2);
    assert_eq!(stats.enabled_jobs, 2);
    assert_eq!(stats.running_jobs, 0);
    assert_eq!(stats.paused_jobs, 0);
}

// ============================================================================
// Scheduler → State Integration
// ============================================================================

#[tokio::test]
async fn test_state_store_records_execution() {
    let store = StateStore::in_memory().expect("Failed to create in-memory store");

    // Create and save an execution record
    let record = ExecutionRecord::new(
        "exec-001".to_string(),
        "daily-report".to_string(),
        "cron".to_string(),
    );
    store
        .insert_execution(&record)
        .expect("Failed to insert execution");

    // Verify state persistence
    let executions = store
        .list_executions(Some("daily-report"), 10)
        .expect("Failed to list");
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].job_name, "daily-report");
    assert_eq!(executions[0].id, "exec-001");
    assert_eq!(executions[0].status, JobExecutionStatus::Queued);
}

#[tokio::test]
async fn test_execution_status_transitions() {
    let store = StateStore::in_memory().expect("Failed to create in-memory store");

    // Create and save initial execution
    let mut record = ExecutionRecord::new(
        "exec-002".to_string(),
        "hourly-sync".to_string(),
        "interval".to_string(),
    );
    store.insert_execution(&record).expect("Failed to insert");

    // Transition: Queued → Running
    record.mark_running();
    store.update_execution(&record).expect("Failed to update");

    let loaded = store.get_execution("exec-002").expect("Failed to get");
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().status, JobExecutionStatus::Running);

    // Transition: Running → Completed
    record.mark_completed(Some("Success output".to_string()));
    store.update_execution(&record).expect("Failed to update");

    let loaded = store.get_execution("exec-002").expect("Failed to get");
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.status, JobExecutionStatus::Completed);
    assert!(loaded.output.is_some());
}

#[tokio::test]
async fn test_execution_failure_persisted() {
    let store = StateStore::in_memory().expect("Failed to create in-memory store");

    let mut record = ExecutionRecord::new(
        "exec-003".to_string(),
        "daily-report".to_string(),
        "manual".to_string(),
    );
    store.insert_execution(&record).expect("Failed to insert");

    // Mark as failed
    record.mark_failed("Connection timeout".to_string());
    store.update_execution(&record).expect("Failed to update");

    let loaded = store
        .get_execution("exec-003")
        .expect("Failed to get")
        .unwrap();
    assert_eq!(loaded.status, JobExecutionStatus::Failed);
    assert_eq!(loaded.error, Some("Connection timeout".to_string()));
}

#[tokio::test]
async fn test_execution_cancellation() {
    let store = StateStore::in_memory().expect("Failed to create in-memory store");

    let mut record = ExecutionRecord::new(
        "exec-004".to_string(),
        "hourly-sync".to_string(),
        "webhook".to_string(),
    );
    store.insert_execution(&record).expect("Failed to insert");

    record.mark_running();
    store.update_execution(&record).expect("Failed to update");

    record.mark_cancelled();
    store.update_execution(&record).expect("Failed to update");

    let loaded = store
        .get_execution("exec-004")
        .expect("Failed to get")
        .unwrap();
    assert_eq!(loaded.status, JobExecutionStatus::Cancelled);
}

// ============================================================================
// State → Stats Integration
// ============================================================================

#[tokio::test]
async fn test_job_stats_computed_from_executions() {
    let store = StateStore::in_memory().expect("Failed to create in-memory store");

    // Create multiple executions with different outcomes
    let mut exec1 = ExecutionRecord::new(
        "exec-101".to_string(),
        "daily-report".to_string(),
        "cron".to_string(),
    );
    exec1.mark_running();
    exec1.mark_completed(None);
    store.insert_execution(&exec1).expect("insert");

    let mut exec2 = ExecutionRecord::new(
        "exec-102".to_string(),
        "daily-report".to_string(),
        "cron".to_string(),
    );
    exec2.mark_running();
    exec2.mark_failed("Error".to_string());
    store.insert_execution(&exec2).expect("insert");

    let mut exec3 = ExecutionRecord::new(
        "exec-103".to_string(),
        "daily-report".to_string(),
        "cron".to_string(),
    );
    exec3.mark_running();
    exec3.mark_completed(None);
    store.insert_execution(&exec3).expect("insert");

    // Get stats
    let stats = store
        .get_stats("daily-report")
        .expect("Failed to get stats");
    assert_eq!(stats.total_executions, 3);
    assert_eq!(stats.successful, 2);
    assert_eq!(stats.failed, 1);
}

// ============================================================================
// Retry Logic Tests
// ============================================================================

#[test]
fn test_backoff_strategy_delay_calculation() {
    use nika::jobs::retry::calculate_delay;

    // Constant backoff
    let constant_config = RetryConfig {
        max_attempts: 3,
        backoff: BackoffStrategy::Constant,
        initial_delay: Duration::from_millis(500),
        max_delay: Duration::from_millis(5000),
        jitter: false,
    };
    let delay = calculate_delay(&constant_config, 1);
    assert_eq!(delay, Duration::from_millis(500));
    let delay = calculate_delay(&constant_config, 5);
    assert_eq!(delay, Duration::from_millis(500));

    // Linear backoff
    let linear_config = RetryConfig {
        max_attempts: 5,
        backoff: BackoffStrategy::Linear,
        initial_delay: Duration::from_millis(100),
        max_delay: Duration::from_millis(1000),
        jitter: false,
    };
    let delay1 = calculate_delay(&linear_config, 1);
    let delay3 = calculate_delay(&linear_config, 3);
    assert!(delay3 > delay1);
    let delay_capped = calculate_delay(&linear_config, 20);
    assert!(delay_capped <= Duration::from_millis(1000));

    // Exponential backoff
    let exp_config = RetryConfig {
        max_attempts: 10,
        backoff: BackoffStrategy::Exponential,
        initial_delay: Duration::from_millis(100),
        max_delay: Duration::from_millis(5000),
        jitter: false,
    };
    let delay1 = calculate_delay(&exp_config, 1);
    let delay2 = calculate_delay(&exp_config, 2);
    let delay3 = calculate_delay(&exp_config, 3);
    assert!(delay2 > delay1);
    assert!(delay3 > delay2);
    let delay_capped = calculate_delay(&exp_config, 10);
    assert!(delay_capped <= Duration::from_millis(5000));
}

#[test]
fn test_retryable_error_detection() {
    use nika::jobs::retry::is_retryable_error;

    // Transient errors should be retryable
    let timeout_err = JobsError::ExecutionTimeout {
        name: "test".to_string(),
        timeout_secs: 60,
    };
    assert!(is_retryable_error(&timeout_err));

    // Permanent errors should not be retryable
    let not_found = JobsError::JobNotFound {
        name: "missing".to_string(),
    };
    assert!(!is_retryable_error(&not_found));
}

#[tokio::test]
async fn test_retry_executor_basic() {
    use nika::jobs::retry::RetryExecutor;
    use std::sync::atomic::{AtomicU32, Ordering};

    let config = RetryConfig {
        max_attempts: 3,
        backoff: BackoffStrategy::Constant,
        initial_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(100),
        jitter: false,
    };
    let executor = RetryExecutor::new(config);

    // Test successful execution on first try
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();
    let result: Result<i32, JobsError> = executor
        .execute(|| {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, JobsError>(42)
            }
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

// ============================================================================
// Trigger Type Tests
// ============================================================================

#[test]
fn test_cron_trigger_parsing() {
    let trigger = JobTrigger::Cron(CronTriggerConfig {
        expression: "0 */5 * * * *".to_string(),
        timezone: "UTC".to_string(),
    });

    if let JobTrigger::Cron(cron) = trigger {
        assert_eq!(cron.expression, "0 */5 * * * *");
        assert_eq!(cron.timezone, "UTC");
    } else {
        panic!("Expected Cron trigger");
    }
}

#[test]
fn test_interval_trigger_config() {
    let trigger = JobTrigger::Interval(IntervalTriggerConfig {
        every: Duration::from_secs(300),
        initial_delay: Duration::from_secs(10),
    });

    if let JobTrigger::Interval(interval) = trigger {
        assert_eq!(interval.every, Duration::from_secs(300));
        assert_eq!(interval.initial_delay, Duration::from_secs(10));
    } else {
        panic!("Expected Interval trigger");
    }
}

#[test]
fn test_webhook_trigger_config() {
    let trigger = JobTrigger::Webhook(WebhookTriggerConfig {
        path: "/hooks/deploy".to_string(),
        method: "POST".to_string(),
        secret: Some("my-secret".to_string()),
    });

    if let JobTrigger::Webhook(webhook) = trigger {
        assert_eq!(webhook.path, "/hooks/deploy");
        assert_eq!(webhook.method, "POST");
        assert_eq!(webhook.secret, Some("my-secret".to_string()));
    } else {
        panic!("Expected Webhook trigger");
    }
}

#[test]
fn test_watch_trigger_config() {
    let trigger = JobTrigger::Watch(WatchTriggerConfig {
        paths: vec!["data/**/*.json".to_string(), "config/*.toml".to_string()],
        events: vec![WatchEvent::Create, WatchEvent::Modify],
        debounce: Duration::from_millis(500),
    });

    if let JobTrigger::Watch(watch) = trigger {
        assert_eq!(watch.paths.len(), 2);
        assert_eq!(watch.events.len(), 2);
        assert_eq!(watch.debounce, Duration::from_millis(500));
    } else {
        panic!("Expected Watch trigger");
    }
}

// ============================================================================
// Notification Integration
// ============================================================================

#[test]
fn test_job_event_type_emoji() {
    use nika::jobs::notify::JobEventType;

    assert_eq!(JobEventType::Started.emoji(), "🚀");
    assert_eq!(JobEventType::Success.emoji(), "✅");
    assert_eq!(JobEventType::Failed.emoji(), "❌");
    assert_eq!(JobEventType::Retrying.emoji(), "🔄");
    assert_eq!(JobEventType::MaxRetriesExceeded.emoji(), "💀");
    assert_eq!(JobEventType::TimedOut.emoji(), "⏰");
}

#[test]
fn test_job_event_type_slack_color() {
    use nika::jobs::notify::JobEventType;

    assert_eq!(JobEventType::Success.slack_color(), "good");
    assert_eq!(JobEventType::Failed.slack_color(), "danger");
    assert_eq!(JobEventType::Retrying.slack_color(), "warning");
}

#[tokio::test]
async fn test_notification_dispatcher_creation() {
    use nika::jobs::notify::NotificationDispatcher;

    // Empty config - no channels configured
    let config = NotifyConfig::default();
    let dispatcher = NotificationDispatcher::new(config);
    assert!(dispatcher.is_ok());

    let dispatcher = dispatcher.unwrap();
    assert!(!dispatcher.has_channels());
    assert_eq!(dispatcher.channel_count(), 0);
}

// ============================================================================
// Error Code Verification
// ============================================================================

#[test]
fn test_jobs_error_codes_in_range() {
    // Jobs errors should be in NIKA-200 to NIKA-279 range
    let errors = vec![
        JobsError::JobNotFound {
            name: "x".to_string(),
        },
        JobsError::DaemonAlreadyRunning { pid: 1234 },
        JobsError::DaemonNotRunning,
        JobsError::ExecutionFailed {
            name: "x".to_string(),
            reason: "y".to_string(),
        },
    ];

    for err in errors {
        let msg = format!("{}", err);
        // All Jobs errors should contain NIKA-2XX
        assert!(
            msg.contains("[NIKA-2") || msg.contains("NIKA-2"),
            "Error should have NIKA-2XX code: {}",
            msg
        );
    }
}

// ============================================================================
// Config Serialization Round-Trip
// ============================================================================

#[test]
fn test_config_toml_round_trip() {
    let config = test_config();

    // Serialize to TOML
    let toml_str = toml::to_string(&config).expect("Failed to serialize config");
    assert!(toml_str.contains("daily-report"));
    assert!(toml_str.contains("hourly-sync"));

    // Deserialize back
    let parsed: JobsConfig = toml::from_str(&toml_str).expect("Failed to parse config");
    assert_eq!(parsed.definitions.len(), 2);
    assert_eq!(parsed.max_concurrent_jobs, 5);
}

#[test]
fn test_config_with_all_trigger_types() {
    let config = JobsConfig {
        enabled: true,
        pid_file: PathBuf::from(".nika/jobs.pid"),
        state_db: PathBuf::from(".nika/jobs.db"),
        log_dir: PathBuf::from(".nika/logs"),
        metrics_port: 9090,
        max_concurrent_jobs: 10,
        default_timeout_secs: 300,
        webhook: WebhookConfig::default(),
        notify: NotifyConfig::default(),
        definitions: vec![
            JobDefinition {
                name: "cron-job".to_string(),
                workflow: PathBuf::from("w1.yaml"),
                enabled: true,
                trigger: JobTrigger::Cron(CronTriggerConfig {
                    expression: "0 0 * * *".to_string(),
                    timezone: "UTC".to_string(),
                }),
                retry: RetryConfig::default(),
                timeout: Duration::from_secs(300),
                env: HashMap::new(),
                tags: vec![],
            },
            JobDefinition {
                name: "interval-job".to_string(),
                workflow: PathBuf::from("w2.yaml"),
                enabled: true,
                trigger: JobTrigger::Interval(IntervalTriggerConfig {
                    every: Duration::from_secs(60),
                    initial_delay: Duration::from_secs(0),
                }),
                retry: RetryConfig::default(),
                timeout: Duration::from_secs(300),
                env: HashMap::new(),
                tags: vec![],
            },
            JobDefinition {
                name: "webhook-job".to_string(),
                workflow: PathBuf::from("w3.yaml"),
                enabled: true,
                trigger: JobTrigger::Webhook(WebhookTriggerConfig {
                    path: "/webhook".to_string(),
                    method: "POST".to_string(),
                    secret: None,
                }),
                retry: RetryConfig::default(),
                timeout: Duration::from_secs(300),
                env: HashMap::new(),
                tags: vec![],
            },
            JobDefinition {
                name: "watch-job".to_string(),
                workflow: PathBuf::from("w4.yaml"),
                enabled: true,
                trigger: JobTrigger::Watch(WatchTriggerConfig {
                    paths: vec!["*.json".to_string()],
                    events: vec![WatchEvent::Create],
                    debounce: Duration::from_millis(100),
                }),
                retry: RetryConfig::default(),
                timeout: Duration::from_secs(300),
                env: HashMap::new(),
                tags: vec![],
            },
        ],
    };

    let toml_str = toml::to_string(&config).expect("serialize");
    let parsed: JobsConfig = toml::from_str(&toml_str).expect("parse");

    assert_eq!(parsed.definitions.len(), 4);

    // Verify each trigger type preserved
    assert!(matches!(parsed.definitions[0].trigger, JobTrigger::Cron(_)));
    assert!(matches!(
        parsed.definitions[1].trigger,
        JobTrigger::Interval(_)
    ));
    assert!(matches!(
        parsed.definitions[2].trigger,
        JobTrigger::Webhook(_)
    ));
    assert!(matches!(
        parsed.definitions[3].trigger,
        JobTrigger::Watch(_)
    ));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_empty_job_list() {
    let config = JobsConfig {
        enabled: true,
        pid_file: PathBuf::from("/tmp/test.pid"),
        state_db: PathBuf::from(":memory:"),
        log_dir: PathBuf::from("/tmp/logs"),
        metrics_port: 9090,
        max_concurrent_jobs: 5,
        default_timeout_secs: 300,
        webhook: WebhookConfig::default(),
        notify: NotifyConfig::default(),
        definitions: vec![],
    };

    let state = Arc::new(StateStore::in_memory().expect("create store"));
    let (scheduler, _cmd_tx, _event_rx) = JobScheduler::new(config, state);
    assert!(scheduler.list_jobs().is_empty());
}

#[tokio::test]
async fn test_state_store_handles_missing_execution() {
    let store = StateStore::in_memory().expect("create store");

    let result = store.get_execution("nonexistent-exec-id").expect("get");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_execution_record_lifecycle() {
    let mut record = ExecutionRecord::new(
        "exec-lifecycle".to_string(),
        "test-job".to_string(),
        "manual".to_string(),
    );

    // Initial state - started_at is set on creation, ended_at is None
    assert_eq!(record.status, JobExecutionStatus::Queued);
    assert!(record.ended_at.is_none());

    // Transition to Running
    record.mark_running();
    assert_eq!(record.status, JobExecutionStatus::Running);
    assert!(record.ended_at.is_none());

    // Transition to Completed
    record.mark_completed(Some("output".to_string()));
    assert_eq!(record.status, JobExecutionStatus::Completed);
    assert!(record.ended_at.is_some());
    assert_eq!(record.output, Some("output".to_string()));
}

#[tokio::test]
async fn test_concurrent_executions_tracked() {
    let store = StateStore::in_memory().expect("create store");

    // Simulate 3 concurrent executions
    for i in 0..3 {
        let record = ExecutionRecord::new(
            format!("exec-concurrent-{}", i),
            "concurrent-job".to_string(),
            "interval".to_string(),
        );
        store.insert_execution(&record).expect("insert");
    }

    let executions = store
        .list_executions(Some("concurrent-job"), 10)
        .expect("list");
    assert_eq!(executions.len(), 3);
}

// ============================================================================
// Integration Flow Tests
// ============================================================================

#[tokio::test]
async fn test_full_execution_lifecycle() {
    // This test simulates a complete job execution lifecycle:
    // 1. Create scheduler with config
    // 2. Record execution start
    // 3. Track progress through state transitions
    // 4. Record completion

    let config = test_config();
    let state = Arc::new(StateStore::in_memory().expect("create store"));
    let (_scheduler, _cmd_tx, _event_rx) = JobScheduler::new(config.clone(), state.clone());

    // Simulate job execution
    let mut record = ExecutionRecord::new(
        "exec-full-lifecycle".to_string(),
        "daily-report".to_string(),
        "cron".to_string(),
    );

    // 1. Insert initial record (Queued)
    state.insert_execution(&record).expect("insert");
    let loaded = state
        .get_execution("exec-full-lifecycle")
        .expect("get")
        .unwrap();
    assert_eq!(loaded.status, JobExecutionStatus::Queued);

    // 2. Start execution (Running)
    record.mark_running();
    state.update_execution(&record).expect("update");
    let loaded = state
        .get_execution("exec-full-lifecycle")
        .expect("get")
        .unwrap();
    assert_eq!(loaded.status, JobExecutionStatus::Running);
    // started_at is DateTime<Utc>, not Option - no need to check is_some()

    // 3. Complete execution
    record.mark_completed(Some("Report generated successfully".to_string()));
    state.update_execution(&record).expect("update");
    let loaded = state
        .get_execution("exec-full-lifecycle")
        .expect("get")
        .unwrap();
    assert_eq!(loaded.status, JobExecutionStatus::Completed);
    assert!(loaded.ended_at.is_some());
    assert_eq!(
        loaded.output,
        Some("Report generated successfully".to_string())
    );

    // 4. Verify stats
    let stats = state.get_stats("daily-report").expect("get stats");
    assert_eq!(stats.total_executions, 1);
    assert_eq!(stats.successful, 1);
    assert_eq!(stats.failed, 0);
}

#[tokio::test]
async fn test_retry_flow_with_state() {
    let store = StateStore::in_memory().expect("create store");

    // Simulate a job that fails twice then succeeds
    let job_name = "retry-test-job";

    // Attempt 1: Failed
    let mut record1 = ExecutionRecord::new(
        "retry-exec-1".to_string(),
        job_name.to_string(),
        "manual".to_string(),
    );
    record1.mark_running();
    record1.mark_failed("Transient error".to_string());
    store.insert_execution(&record1).expect("insert");

    // Attempt 2: Failed
    let mut record2 = ExecutionRecord::new(
        "retry-exec-2".to_string(),
        job_name.to_string(),
        "manual".to_string(),
    );
    record2.mark_running();
    record2.mark_failed("Another transient error".to_string());
    store.insert_execution(&record2).expect("insert");

    // Attempt 3: Success
    let mut record3 = ExecutionRecord::new(
        "retry-exec-3".to_string(),
        job_name.to_string(),
        "manual".to_string(),
    );
    record3.mark_running();
    record3.mark_completed(None);
    store.insert_execution(&record3).expect("insert");

    // Verify stats
    let stats = store.get_stats(job_name).expect("get stats");
    assert_eq!(stats.total_executions, 3);
    assert_eq!(stats.successful, 1);
    assert_eq!(stats.failed, 2);
}

#[tokio::test]
async fn test_multiple_triggers_same_job() {
    let store = StateStore::in_memory().expect("create store");
    let job_name = "multi-trigger-job";

    // Execution from cron trigger
    let record1 = ExecutionRecord::new(
        "exec-cron-1".to_string(),
        job_name.to_string(),
        "cron".to_string(),
    );
    store.insert_execution(&record1).expect("insert");

    // Execution from webhook trigger
    let record2 = ExecutionRecord::new(
        "exec-webhook-1".to_string(),
        job_name.to_string(),
        "webhook".to_string(),
    );
    store.insert_execution(&record2).expect("insert");

    // Execution from manual trigger
    let record3 = ExecutionRecord::new(
        "exec-manual-1".to_string(),
        job_name.to_string(),
        "manual".to_string(),
    );
    store.insert_execution(&record3).expect("insert");

    // All executions should be tracked
    let executions = store.list_executions(Some(job_name), 10).expect("list");
    assert_eq!(executions.len(), 3);

    // Verify different trigger sources
    let triggers: Vec<_> = executions.iter().map(|e| e.trigger.as_str()).collect();
    assert!(triggers.contains(&"cron"));
    assert!(triggers.contains(&"webhook"));
    assert!(triggers.contains(&"manual"));
}
