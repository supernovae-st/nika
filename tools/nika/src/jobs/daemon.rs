//! Jobs Daemon lifecycle management.
//!
//! The daemon orchestrates job scheduling, execution, and persistence.
//! It runs as a background process managed via PID file.

// StateStore uses Arc<RwLock<Connection>> which is intentionally non-Sync
#![allow(clippy::arc_with_non_send_sync)]

use crate::jobs::config::JobsConfig;
use crate::jobs::error::JobsError;
use crate::jobs::scheduler::{JobScheduler, SchedulerCommand, SchedulerEvent};
use crate::jobs::state::StateStore;
use parking_lot::RwLock;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::mpsc;

/// Current daemon status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonStatus {
    /// Daemon is starting up.
    Starting,
    /// Daemon is running and processing jobs.
    Running,
    /// Daemon is shutting down gracefully.
    ShuttingDown,
    /// Daemon is stopped.
    Stopped,
}

impl std::fmt::Display for DaemonStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonStatus::Starting => write!(f, "starting"),
            DaemonStatus::Running => write!(f, "running"),
            DaemonStatus::ShuttingDown => write!(f, "shutting_down"),
            DaemonStatus::Stopped => write!(f, "stopped"),
        }
    }
}

/// Runtime information about the daemon.
#[derive(Debug, Clone)]
pub struct DaemonInfo {
    /// Current status.
    pub status: DaemonStatus,
    /// Process ID.
    pub pid: u32,
    /// Number of registered jobs.
    pub job_count: usize,
    /// Number of active executions.
    pub active_executions: usize,
    /// Total executions since start.
    pub total_executions: u64,
    /// Uptime in seconds.
    pub uptime_secs: u64,
}

/// The Jobs Daemon orchestrator.
///
/// Manages the lifecycle of the job scheduler, including:
/// - PID file management
/// - Signal handling (SIGTERM, SIGINT)
/// - Scheduler coordination
/// - State persistence
pub struct JobsDaemon {
    config: Arc<JobsConfig>,
    state: Arc<StateStore>,
    status: Arc<RwLock<DaemonStatus>>,
    start_time: std::time::Instant,
    cmd_tx: Option<mpsc::Sender<SchedulerCommand>>,
}

impl JobsDaemon {
    /// Create a new daemon with the given configuration.
    pub fn new(config: JobsConfig) -> Result<Self, JobsError> {
        // Initialize state store
        let state = StateStore::new(&config.state_db)?;

        Ok(Self {
            config: Arc::new(config),
            state: Arc::new(state),
            status: Arc::new(RwLock::new(DaemonStatus::Stopped)),
            start_time: std::time::Instant::now(),
            cmd_tx: None,
        })
    }

    /// Create daemon from a configuration file.
    pub fn from_config_file(path: &Path) -> Result<Self, JobsError> {
        let content = fs::read_to_string(path).map_err(|e| JobsError::ConfigParseError {
            reason: format!("Failed to read config: {}", e),
        })?;

        let config: JobsConfig =
            toml::from_str(&content).map_err(|e| JobsError::ConfigParseError {
                reason: e.to_string(),
            })?;

        Self::new(config)
    }

    /// Get current daemon status.
    pub fn status(&self) -> DaemonStatus {
        *self.status.read()
    }

    /// Get daemon runtime information.
    pub fn info(&self) -> DaemonInfo {
        let status = *self.status.read();
        DaemonInfo {
            status,
            pid: std::process::id(),
            job_count: self.config.definitions.len(),
            active_executions: 0,
            total_executions: 0,
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }

    /// Start the daemon.
    ///
    /// This will:
    /// 1. Check if daemon is already running via PID file
    /// 2. Write PID file
    /// 3. Initialize and start the scheduler
    /// 4. Set up signal handlers
    /// 5. Run the event loop
    pub async fn start(&mut self) -> Result<(), JobsError> {
        // Check if already running
        if let Some(pid) = self.read_pid_file()? {
            if self.is_process_running(pid) {
                return Err(JobsError::DaemonAlreadyRunning { pid });
            }
            // Stale PID file, remove it
            self.remove_pid_file()?;
        }

        // Update status
        *self.status.write() = DaemonStatus::Starting;

        // Write PID file
        self.write_pid_file()?;

        // Ensure log directory exists
        if let Some(parent) = self.config.log_dir.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::create_dir_all(&self.config.log_dir).ok();

        // Create scheduler
        let (scheduler, cmd_tx, mut event_rx) =
            JobScheduler::new((*self.config).clone(), Arc::clone(&self.state));
        self.cmd_tx = Some(cmd_tx.clone());

        // Update status to running
        *self.status.write() = DaemonStatus::Running;

        // Event processing loop with signal handling
        let status = Arc::clone(&self.status);
        let shutdown_cmd_tx = cmd_tx.clone();

        // Run scheduler directly (SQLite Connection is not Send, so we can't spawn)
        tokio::select! {
            // Run the scheduler
            result = scheduler.run() => {
                match result {
                    Ok(()) => tracing::info!("Scheduler stopped gracefully"),
                    Err(e) => tracing::error!("Scheduler error: {}", e),
                }
            }

            // Process scheduler events
            _ = async {
                while let Some(event) = event_rx.recv().await {
                    self.handle_event(event).await;
                }
            } => {}

            // Handle SIGTERM/SIGINT
            _ = async {
                let _ = signal::ctrl_c().await;
                tracing::info!("Received shutdown signal");
                *status.write() = DaemonStatus::ShuttingDown;
                let _ = shutdown_cmd_tx.send(SchedulerCommand::Shutdown).await;
            } => {}
        }

        // Cleanup
        self.remove_pid_file()?;
        *self.status.write() = DaemonStatus::Stopped;

        Ok(())
    }

    /// Stop the daemon.
    pub async fn stop(&self) -> Result<(), JobsError> {
        if *self.status.read() == DaemonStatus::Stopped {
            return Err(JobsError::DaemonNotRunning);
        }

        *self.status.write() = DaemonStatus::ShuttingDown;

        if let Some(ref cmd_tx) = self.cmd_tx {
            cmd_tx.send(SchedulerCommand::Shutdown).await.map_err(|_| {
                JobsError::DaemonStopFailed {
                    reason: "Failed to send shutdown command".to_string(),
                }
            })?;
        }

        Ok(())
    }

    /// Trigger a job manually.
    pub async fn trigger_job(&self, job_name: &str) -> Result<String, JobsError> {
        if *self.status.read() != DaemonStatus::Running {
            return Err(JobsError::DaemonNotRunning);
        }

        if let Some(ref cmd_tx) = self.cmd_tx {
            cmd_tx
                .send(SchedulerCommand::TriggerJob {
                    job_name: job_name.to_string(),
                })
                .await
                .map_err(|_| JobsError::ChannelClosed)?;

            // Generate execution ID (actual ID comes from scheduler event)
            Ok(format!("{}-{}", job_name, chrono::Utc::now().timestamp()))
        } else {
            Err(JobsError::DaemonNotRunning)
        }
    }

    /// Pause a job.
    pub async fn pause_job(&self, job_name: &str) -> Result<(), JobsError> {
        if let Some(ref cmd_tx) = self.cmd_tx {
            cmd_tx
                .send(SchedulerCommand::PauseJob {
                    job_name: job_name.to_string(),
                })
                .await
                .map_err(|_| JobsError::ChannelClosed)?;
            Ok(())
        } else {
            Err(JobsError::DaemonNotRunning)
        }
    }

    /// Resume a job.
    pub async fn resume_job(&self, job_name: &str) -> Result<(), JobsError> {
        if let Some(ref cmd_tx) = self.cmd_tx {
            cmd_tx
                .send(SchedulerCommand::ResumeJob {
                    job_name: job_name.to_string(),
                })
                .await
                .map_err(|_| JobsError::ChannelClosed)?;
            Ok(())
        } else {
            Err(JobsError::DaemonNotRunning)
        }
    }

    /// Reload configuration.
    pub async fn reload(&self) -> Result<(), JobsError> {
        if let Some(ref cmd_tx) = self.cmd_tx {
            cmd_tx
                .send(SchedulerCommand::Reload)
                .await
                .map_err(|_| JobsError::ChannelClosed)?;
            Ok(())
        } else {
            Err(JobsError::DaemonNotRunning)
        }
    }

    /// Handle scheduler events.
    async fn handle_event(&self, event: SchedulerEvent) {
        match event {
            SchedulerEvent::JobTriggered {
                job_name,
                execution_id,
                trigger,
            } => {
                tracing::info!(
                    job = %job_name,
                    execution_id = %execution_id,
                    trigger = ?trigger,
                    "Job triggered"
                );
            }
            SchedulerEvent::JobCompleted {
                job_name,
                execution_id,
                success,
                duration_ms,
            } => {
                if success {
                    tracing::info!(
                        job = %job_name,
                        execution_id = %execution_id,
                        duration_ms = duration_ms,
                        "Job completed successfully"
                    );
                } else {
                    tracing::warn!(
                        job = %job_name,
                        execution_id = %execution_id,
                        duration_ms = duration_ms,
                        "Job failed"
                    );
                }
            }
            SchedulerEvent::Error { message } => {
                tracing::error!(error = %message, "Scheduler error");
            }
        }
    }

    // === PID File Management ===

    /// Read PID from file.
    fn read_pid_file(&self) -> Result<Option<u32>, JobsError> {
        let path = &self.config.pid_file;

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path).map_err(|e| JobsError::PidFileError {
            reason: format!("Failed to read PID file: {}", e),
        })?;

        let pid: u32 = content
            .trim()
            .parse()
            .map_err(|e| JobsError::PidFileError {
                reason: format!("Invalid PID in file: {}", e),
            })?;

        Ok(Some(pid))
    }

    /// Write current PID to file.
    fn write_pid_file(&self) -> Result<(), JobsError> {
        let path = &self.config.pid_file;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| JobsError::PidFileError {
                reason: format!("Failed to create PID directory: {}", e),
            })?;
        }

        let pid = std::process::id();
        fs::write(path, pid.to_string()).map_err(|e| JobsError::PidFileError {
            reason: format!("Failed to write PID file: {}", e),
        })?;

        Ok(())
    }

    /// Remove PID file.
    fn remove_pid_file(&self) -> Result<(), JobsError> {
        let path = &self.config.pid_file;

        if path.exists() {
            fs::remove_file(path).map_err(|e| JobsError::PidFileError {
                reason: format!("Failed to remove PID file: {}", e),
            })?;
        }

        Ok(())
    }

    /// Check if a process is running.
    #[cfg(unix)]
    fn is_process_running(&self, pid: u32) -> bool {
        // Use kill -0 to check if process exists
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    fn is_process_running(&self, _pid: u32) -> bool {
        // On non-Unix platforms, assume process is not running
        false
    }

    /// Stop a daemon by PID file (static method for CLI).
    pub fn stop_by_pid_file(pid_file: &Path) -> Result<(), JobsError> {
        if !pid_file.exists() {
            return Err(JobsError::DaemonNotRunning);
        }

        let content = fs::read_to_string(pid_file).map_err(|e| JobsError::PidFileError {
            reason: format!("Failed to read PID file: {}", e),
        })?;

        let pid: u32 = content
            .trim()
            .parse()
            .map_err(|e| JobsError::PidFileError {
                reason: format!("Invalid PID in file: {}", e),
            })?;

        // Send SIGTERM
        #[cfg(unix)]
        {
            std::process::Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status()
                .map_err(|e| JobsError::DaemonStopFailed {
                    reason: format!("Failed to send SIGTERM: {}", e),
                })?;
        }

        #[cfg(not(unix))]
        {
            return Err(JobsError::DaemonStopFailed {
                reason: "Process termination not supported on this platform".to_string(),
            });
        }

        // Remove PID file after sending signal
        fs::remove_file(pid_file).ok();

        Ok(())
    }

    /// Send SIGHUP to the running daemon to trigger a config reload.
    ///
    /// This is the CLI-friendly way to reload a running daemon.
    /// The daemon must have SIGHUP handling configured.
    pub fn reload_by_signal(pid_file: &Path) -> Result<(), JobsError> {
        if !pid_file.exists() {
            return Err(JobsError::DaemonNotRunning);
        }

        let content = fs::read_to_string(pid_file).map_err(|e| JobsError::PidFileError {
            reason: format!("Failed to read PID file: {}", e),
        })?;

        let pid: u32 = content
            .trim()
            .parse()
            .map_err(|e| JobsError::PidFileError {
                reason: format!("Invalid PID in file: {}", e),
            })?;

        // Send SIGHUP for reload
        #[cfg(unix)]
        {
            std::process::Command::new("kill")
                .args(["-HUP", &pid.to_string()])
                .status()
                .map_err(|e| JobsError::IoError {
                    reason: format!("Failed to send SIGHUP: {}", e),
                })?;
        }

        #[cfg(not(unix))]
        {
            return Err(JobsError::IoError {
                reason: "Signal-based reload not supported on this platform".to_string(),
            });
        }

        Ok(())
    }

    /// Get daemon status from PID file (static method for CLI).
    pub fn get_status_from_pid_file(pid_file: &Path) -> DaemonStatus {
        if !pid_file.exists() {
            return DaemonStatus::Stopped;
        }

        match fs::read_to_string(pid_file) {
            Ok(content) => {
                if let Ok(pid) = content.trim().parse::<u32>() {
                    #[cfg(unix)]
                    {
                        let is_running = std::process::Command::new("kill")
                            .args(["-0", &pid.to_string()])
                            .status()
                            .map(|s| s.success())
                            .unwrap_or(false);

                        if is_running {
                            DaemonStatus::Running
                        } else {
                            DaemonStatus::Stopped
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        DaemonStatus::Stopped
                    }
                } else {
                    DaemonStatus::Stopped
                }
            }
            Err(_) => DaemonStatus::Stopped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(temp_dir: &TempDir) -> JobsConfig {
        JobsConfig {
            enabled: true,
            pid_file: temp_dir.path().join("daemon.pid"),
            state_db: temp_dir.path().join("state.db"),
            log_dir: temp_dir.path().join("logs"),
            ..Default::default()
        }
    }

    #[test]
    fn test_daemon_status_display() {
        assert_eq!(DaemonStatus::Starting.to_string(), "starting");
        assert_eq!(DaemonStatus::Running.to_string(), "running");
        assert_eq!(DaemonStatus::ShuttingDown.to_string(), "shutting_down");
        assert_eq!(DaemonStatus::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_daemon_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);
        let daemon = JobsDaemon::new(config);
        assert!(daemon.is_ok());

        let daemon = daemon.unwrap();
        assert_eq!(daemon.status(), DaemonStatus::Stopped);
    }

    #[test]
    fn test_daemon_info() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);
        let daemon = JobsDaemon::new(config).unwrap();

        let info = daemon.info();
        assert_eq!(info.status, DaemonStatus::Stopped);
        assert_eq!(info.job_count, 0);
        assert!(info.pid > 0);
    }

    #[test]
    fn test_pid_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);
        let daemon = JobsDaemon::new(config).unwrap();

        // No PID file initially
        assert!(daemon.read_pid_file().unwrap().is_none());

        // Write PID file
        daemon.write_pid_file().unwrap();
        let pid = daemon.read_pid_file().unwrap();
        assert!(pid.is_some());
        assert_eq!(pid.unwrap(), std::process::id());

        // Remove PID file
        daemon.remove_pid_file().unwrap();
        assert!(daemon.read_pid_file().unwrap().is_none());
    }

    #[test]
    fn test_get_status_from_pid_file() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("test.pid");

        // No file = stopped
        assert_eq!(
            JobsDaemon::get_status_from_pid_file(&pid_file),
            DaemonStatus::Stopped
        );

        // Invalid content = stopped
        fs::write(&pid_file, "invalid").unwrap();
        assert_eq!(
            JobsDaemon::get_status_from_pid_file(&pid_file),
            DaemonStatus::Stopped
        );
    }

    #[tokio::test]
    async fn test_stop_not_running() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);
        let daemon = JobsDaemon::new(config).unwrap();

        let result = daemon.stop().await;
        assert!(matches!(result, Err(JobsError::DaemonNotRunning)));
    }

    #[tokio::test]
    async fn test_trigger_job_not_running() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);
        let daemon = JobsDaemon::new(config).unwrap();

        let result = daemon.trigger_job("test-job").await;
        assert!(matches!(result, Err(JobsError::DaemonNotRunning)));
    }

    #[tokio::test]
    async fn test_pause_job_not_running() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);
        let daemon = JobsDaemon::new(config).unwrap();

        let result = daemon.pause_job("test-job").await;
        assert!(matches!(result, Err(JobsError::DaemonNotRunning)));
    }

    #[tokio::test]
    async fn test_resume_job_not_running() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);
        let daemon = JobsDaemon::new(config).unwrap();

        let result = daemon.resume_job("test-job").await;
        assert!(matches!(result, Err(JobsError::DaemonNotRunning)));
    }

    #[tokio::test]
    async fn test_reload_not_running() {
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(&temp_dir);
        let daemon = JobsDaemon::new(config).unwrap();

        let result = daemon.reload().await;
        assert!(matches!(result, Err(JobsError::DaemonNotRunning)));
    }

    #[test]
    fn test_stop_by_pid_file_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("nonexistent.pid");

        let result = JobsDaemon::stop_by_pid_file(&pid_file);
        assert!(matches!(result, Err(JobsError::DaemonNotRunning)));
    }
}
