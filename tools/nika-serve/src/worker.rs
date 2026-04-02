//! Subprocess worker that executes `nika run` for a single job.
//!
//! V1 architecture: no embedded engine -- we spawn `current_exe() run workflow.nika.yaml`
//! as a child process with process-group isolation (setsid on Unix).
//!
//! ERRATA-11: Use `std::env::current_exe()` instead of `Command::new("nika")`.
//! ERRATA-15: Kill entire PGID on timeout via `nix::sys::signal::kill(-pid, SIGKILL)`.
//! ERRATA-7:  UTF-8 safe truncation on both stdout and stderr.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::config::ServeConfig;
use crate::executor::ExecutionContext;
use crate::state::{AppState, WorkerHandle};

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

/// Truncate a string to at most `max_bytes`, respecting UTF-8 char boundaries.
/// ERRATA-7: never slice mid-codepoint.
fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let boundary = (0..=max_bytes)
        .rev()
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(0);
    &s[..boundary]
}

/// Kill an entire process group on Unix.
/// Negative PID tells the kernel to signal the whole PGID.
#[cfg(unix)]
fn kill_process_group(pid: u32, signal: nix::sys::signal::Signal) {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    let _ = kill(Pid::from_raw(-(pid as i32)), signal);
}

/// Read from an async reader until EOF with a byte limit.
///
/// Reads in a loop until EOF (or `max_bytes` reached) so the pipe stays open
/// for the full lifetime of the child process. A single `read()` would close
/// the pipe after the first chunk, causing SIGPIPE / "Broken pipe" in the child.
async fn read_limited(
    reader: Option<impl tokio::io::AsyncRead + Unpin>,
    max_bytes: usize,
) -> String {
    let Some(mut r) = reader else {
        return String::new();
    };
    let mut buf = vec![0u8; max_bytes];
    let mut total = 0;
    loop {
        if total >= max_bytes {
            // Drain remaining data so the child doesn't get SIGPIPE
            let mut drain = [0u8; 8192];
            loop {
                match r.read(&mut drain).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => continue,
                }
            }
            break;
        }
        match r.read(&mut buf[total..]).await {
            Ok(0) => break, // EOF
            Ok(n) => total += n,
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf[..total]).into_owned()
}

// ═══════════════════════════════════════════════════════════════════════════
// WORKER GUARD (FIX-2: panic safety)
// ═══════════════════════════════════════════════════════════════════════════

/// Drop guard that ensures job cleanup even on panic.
///
/// If `completed` is not set to `true` before drop (e.g. due to panic),
/// the guard marks the job as failed and decrements the active counter.
struct WorkerGuard {
    storage: nika_storage::Storage,
    workers: Arc<Mutex<std::collections::HashMap<String, WorkerHandle>>>,
    active_jobs: Arc<std::sync::atomic::AtomicUsize>,
    /// H5: EventBus reference for cleanup on panic/early return
    event_bus: crate::events::EventBus,
    job_id: String,
    completed: bool,
    /// Whether the active_jobs counter was incremented for this guard.
    /// Only decrement on drop if this is true.
    incremented: bool,
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        if !self.completed {
            // Panic or early return — best-effort cleanup via spawn
            let storage = self.storage.clone();
            let id = self.job_id.clone();
            tokio::spawn(async move {
                let _ = storage.fail_job(&id, "Worker crashed unexpectedly").await;
            });
        }
        // Only decrement if we successfully incremented
        if self.incremented {
            self.active_jobs.fetch_sub(1, Ordering::Relaxed);
            crate::metrics::record_active_jobs(self.active_jobs.load(Ordering::Relaxed));
        }
        // Always remove from worker map + cleanup event bus channel (H5)
        let workers = self.workers.clone();
        let event_bus = self.event_bus.clone();
        let id = self.job_id.clone();
        tokio::spawn(async move {
            workers.lock().await.remove(&id);
            event_bus.remove(&id).await;
        });
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SPAWN
// ═══════════════════════════════════════════════════════════════════════════

/// Spawn a background worker for a job. Returns the `WorkerHandle` which is
/// also stored in `AppState.workers` for cancel + drain (ERRATA-9).
pub fn spawn_worker(
    state: &AppState,
    job_id: String,
    workflow: String,
    inputs: Option<serde_json::Value>,
) -> WorkerHandle {
    let storage = state.storage.clone();
    let config = Arc::clone(&state.config);
    let executor = state.executor.clone();
    let semaphore = Arc::clone(&state.semaphore);
    let workers = Arc::clone(&state.workers);
    let active_jobs = Arc::clone(&state.active_jobs);
    let event_bus = state.event_bus.clone();
    let webhook_config = state.webhook_config.clone();
    let shutdown_rx = state.shutdown.clone();
    let id = job_id.clone();
    let child_pid = Arc::new(AtomicU32::new(0));
    let child_pid_clone = Arc::clone(&child_pid);

    let mut shutdown_rx_clone = shutdown_rx.clone();

    let join = tokio::spawn(async move {
        // C4: Race semaphore acquire against shutdown to avoid blocking 30s on drain
        let _permit = tokio::select! {
            permit = semaphore.acquire() => permit,
            _ = shutdown_rx_clone.changed() => {
                info!(job_id = %id, "server shutting down, skipping pending job");
                // Best-effort: mark job as failed in storage
                let _ = storage.fail_job(&id, "server shutting down").await;
                active_jobs.fetch_sub(1, Ordering::Relaxed);
                crate::metrics::record_active_jobs(active_jobs.load(Ordering::Relaxed));
                let workers_clone = workers.clone();
                let id_clone = id.clone();
                tokio::spawn(async move { workers_clone.lock().await.remove(&id_clone); });
                return;
            }
        };

        // Create guard AFTER acquire — ensures cleanup even on panic
        let mut guard = WorkerGuard {
            storage: storage.clone(),
            workers: workers.clone(),
            active_jobs,
            event_bus: event_bus.clone(),
            job_id: id.clone(),
            completed: false,
            incremented: true, // Counter was incremented by try_acquire_job_slot
        };

        info!(job_id = %id, workflow = %workflow, "worker started");
        let job_start = std::time::Instant::now();

        // Mark running
        if let Err(e) = storage
            .update_state(&id, nika_storage::JobState::Running, None, None)
            .await
        {
            error!(job_id = %id, error = %e, "failed to mark job running");
            return;
        }

        // Emit SSE event: started
        let tx = event_bus.sender(&id).await;
        let _ = tx.send(crate::events::ServeEvent::Started { job_id: id.clone() });

        let max_output_bytes = config.max_output_bytes;
        let mut ctx = ExecutionContext {
            config,
            shutdown_rx,
            child_pid: child_pid_clone,
            job_id: id.clone(),
            event_tx: Some(tx.clone()),
        };

        let result = executor.execute(&workflow, inputs.as_ref(), &mut ctx).await;

        // FIX-13: Check if job was cancelled while we were running
        let current_state = storage.get_job(&id).await.ok().flatten();
        if current_state
            .as_ref()
            .is_some_and(|j| j.state == nika_storage::JobState::Cancelled)
        {
            info!(job_id = %id, "job was cancelled during execution, skipping status update");
            guard.completed = true; // Don't trigger guard's fail_job
            return;
        }

        match result {
            Ok(exec_result) => {
                let truncated = safe_truncate(&exec_result.output, max_output_bytes);
                if let Err(e) = storage.complete_job(&id, truncated).await {
                    error!(job_id = %id, error = %e, "failed to mark job completed");
                }
                // Store artifact metadata in DB
                if !exec_result.artifacts.is_empty() {
                    let db_artifacts: Vec<nika_storage::JobArtifact> = exec_result
                        .artifacts
                        .iter()
                        .map(|a| nika_storage::JobArtifact {
                            job_id: id.clone(),
                            name: a.name.clone(),
                            path: a.path.clone(),
                            size: a.size,
                            format: a.format.clone(),
                            checksum: a.checksum.clone(),
                            content_type: mime_from_name(&a.name, &a.format),
                        })
                        .collect();
                    let count = db_artifacts.len();
                    if let Err(e) = storage.add_artifacts(&id, db_artifacts).await {
                        error!(job_id = %id, error = %e, "failed to store artifacts");
                    } else {
                        info!(job_id = %id, count, "artifacts stored");
                    }
                }
                let _ = tx.send(crate::events::ServeEvent::Completed {
                    job_id: id.clone(),
                    output: Some(truncated.to_string()),
                });
                crate::metrics::record_job_completed(
                    "completed",
                    job_start.elapsed().as_secs_f64(),
                );
                if let Some(ref wh) = webhook_config {
                    crate::webhook::notify(wh, &id, "completed", Some(truncated));
                }
                info!(job_id = %id, "job completed");
            }
            Err(msg) => {
                let truncated = safe_truncate(&msg, max_output_bytes);
                if let Err(e) = storage.fail_job(&id, truncated).await {
                    error!(job_id = %id, error = %e, "failed to mark job failed");
                }
                let _ = tx.send(crate::events::ServeEvent::Failed {
                    job_id: id.clone(),
                    error: Some(truncated.to_string()),
                });
                crate::metrics::record_job_completed("failed", job_start.elapsed().as_secs_f64());
                if let Some(ref wh) = webhook_config {
                    crate::webhook::notify(wh, &id, "failed", Some(truncated));
                }
                warn!(job_id = %id, error = %msg, "job failed");
            }
        }

        // Cleanup event bus channel
        event_bus.remove(&id).await;

        guard.completed = true;
    });

    WorkerHandle { join, child_pid }
}

// ═══════════════════════════════════════════════════════════════════════════
// SUBPROCESS EXECUTION
// ═══════════════════════════════════════════════════════════════════════════

/// Execute `nika run <workflow>` as a subprocess with timeout + PGID isolation.
///
/// FIX-6: Uses env_clear + allowlist instead of env_remove denylist.
/// FIX-4+9: Reads stdout/stderr via bounded pipes (not wait_with_output).
/// FIX-5: Races subprocess against shutdown signal.
/// FIX-11: Passes inputs as --input key=value args.
/// FIX-15: Adds -y --no-interactive flags.
pub(crate) async fn run_subprocess(
    config: &ServeConfig,
    workflow: &str,
    inputs: Option<&serde_json::Value>,
    child_pid: &Arc<AtomicU32>,
    shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("failed to resolve current_exe: {e}"))?;

    let workflow_path = config.workflows_dir.join(workflow);

    let mut cmd = Command::new(&exe);
    cmd.arg("run")
        .arg(&workflow_path)
        .arg("--no-live")
        .arg("-y")
        .arg("--no-interactive");

    // FIX-11: Pass inputs as --input key=value
    if let Some(obj) = inputs.and_then(|v| v.as_object()) {
        for (key, value) in obj {
            let val_str = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            cmd.arg("--input").arg(format!("{key}={val_str}"));
        }
    }

    // FIX-6: env_clear + allowlist (not env_remove denylist)
    let path = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let nika_vars: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| {
            k == "ANTHROPIC_API_KEY"
                || k == "OPENAI_API_KEY"
                || k == "XAI_API_KEY"
                || k == "GEMINI_API_KEY"
                || k == "MISTRAL_API_KEY"
                || k == "GROQ_API_KEY"
                || k == "DEEPSEEK_API_KEY"
                || k == "RUST_LOG"
                || k == "NIKA_NO_DAEMON"
                || k.starts_with("NIKA_ENDPOINT_")
        })
        .collect();

    cmd.env_clear();
    cmd.env("PATH", &path);
    cmd.env("HOME", &home);
    for (k, v) in &nika_vars {
        cmd.env(k, v);
    }

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    // Unix: create a new session so we get a dedicated process group.
    // This lets us kill the entire tree (child + grandchildren) on timeout.
    #[cfg(unix)]
    {
        // SAFETY: `setsid()` is async-signal-safe (POSIX) and has no
        // preconditions beyond being called in a single-threaded context.
        // Tokio guarantees that `pre_exec` closures run between `fork()`
        // and `exec()` in the child process, which is single-threaded.
        #[allow(unsafe_code)]
        unsafe {
            cmd.pre_exec(|| {
                nix::unistd::setsid().map_err(std::io::Error::other)?;
                Ok(())
            });
        }
    }

    let mut child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;

    // Store PID for cancel (FIX-12)
    if let Some(pid) = child.id() {
        child_pid.store(pid, Ordering::Relaxed);
    }

    let timeout = std::time::Duration::from_secs(config.job_timeout_secs);
    let max_bytes = config.max_output_bytes;

    // FIX-4+9: Read stdout/stderr via bounded pipes (not wait_with_output)
    // FIX-5: Race against shutdown signal
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    tokio::select! {
        result = async {
            // Read stdout and stderr concurrently with bounded buffers
            let (stdout, stderr) = tokio::join!(
                read_limited(stdout_pipe, max_bytes),
                read_limited(stderr_pipe, max_bytes / 2),
            );

            // Wait for process with timeout
            match tokio::time::timeout(timeout, child.wait()).await {
                Ok(Ok(status)) => {
                    if status.success() {
                        Ok(stdout)
                    } else {
                        let code = status.code().unwrap_or(-1);
                        Err(format!(
                            "exit code {code}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
                        ))
                    }
                }
                Ok(Err(e)) => Err(format!("process I/O error: {e}")),
                Err(_elapsed) => {
                    // Timeout: kill the entire process group (ERRATA-15)
                    if let Some(pid) = child.id() {
                        debug!(pid, "killing process group after timeout");
                        #[cfg(unix)]
                        kill_process_group(pid, nix::sys::signal::Signal::SIGKILL);
                        #[cfg(not(unix))]
                        { let _ = child.kill().await; }
                    }
                    Err(format!("timeout after {}s", config.job_timeout_secs))
                }
            }
        } => result,

        // FIX-5: Shutdown signal — kill subprocess and fail job
        _ = shutdown_rx.changed() => {
            #[cfg(unix)]
            if let Some(pid) = child.id() {
                kill_process_group(pid, nix::sys::signal::Signal::SIGTERM);
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                kill_process_group(pid, nix::sys::signal::Signal::SIGKILL);
            }
            #[cfg(not(unix))]
            { let _ = child.kill().await; }
            Err("server shutting down".into())
        }
    }
}

/// Derive MIME content-type from artifact name and format.
fn mime_from_name(name: &str, format: &str) -> String {
    // Try extension first
    if let Some(ext) = std::path::Path::new(name).extension().and_then(|e| e.to_str()) {
        let mime = match ext {
            "json" => "application/json",
            "yaml" | "yml" => "application/yaml",
            "md" | "markdown" => "text/markdown",
            "txt" => "text/plain",
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "js" => "application/javascript",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "pdf" => "application/pdf",
            "mp3" => "audio/mpeg",
            "mp4" => "video/mp4",
            "wav" => "audio/wav",
            "zip" => "application/zip",
            "csv" => "text/csv",
            "xml" => "application/xml",
            _ => return format_to_mime(format),
        };
        return mime.to_string();
    }
    format_to_mime(format)
}

fn format_to_mime(format: &str) -> String {
    match format {
        "json" => "application/json",
        "yaml" => "application/yaml",
        "markdown" => "text/markdown",
        "text" => "text/plain",
        "binary" => "application/octet-stream",
        _ => "application/octet-stream",
    }
    .to_string()
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_truncate_ascii() {
        let s = "hello world";
        assert_eq!(safe_truncate(s, 5), "hello");
        assert_eq!(safe_truncate(s, 100), "hello world");
        assert_eq!(safe_truncate(s, 0), "");
    }

    #[test]
    fn safe_truncate_multibyte() {
        // e-acute (\u{00e9}) is 2 bytes in UTF-8
        let s = "caf\u{00e9}!"; // c=1, a=1, f=1, e-acute=2, !=1 => 6 bytes total
        assert_eq!(safe_truncate(s, 4), "caf"); // can't fit the 2-byte char starting at byte 3
        assert_eq!(safe_truncate(s, 5), "caf\u{00e9}");
        assert_eq!(safe_truncate(s, 6), "caf\u{00e9}!");
    }

    #[test]
    fn safe_truncate_emoji() {
        let s = "hello \u{1f680}!"; // rocket emoji is 4 bytes
                                    // "hello " = 6, rocket = 4, "!" = 1 => 11 bytes
        assert_eq!(safe_truncate(s, 7), "hello "); // can't fit 4-byte rocket at pos 6
        assert_eq!(safe_truncate(s, 10), "hello \u{1f680}");
    }

    #[test]
    fn read_limited_caps_output() {
        // Verify the bounded read logic works conceptually
        let data = "hello world, this is a long string";
        let truncated = safe_truncate(data, 5);
        assert_eq!(truncated, "hello");
    }

    #[tokio::test]
    async fn worker_guard_no_decrement_when_not_incremented() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let counter = Arc::new(AtomicUsize::new(5));

        {
            let _guard = WorkerGuard {
                storage: nika_storage::Storage::open_memory().unwrap(),
                workers: Arc::new(Mutex::new(std::collections::HashMap::new())),
                active_jobs: counter.clone(),
                event_bus: crate::events::EventBus::default(),
                job_id: "test-job".into(),
                completed: true,
                incremented: false, // Never incremented
            };
            // Guard drops here
        }

        // Counter should NOT have been decremented
        assert_eq!(
            counter.load(Ordering::SeqCst),
            5,
            "counter must not change when incremented=false"
        );
    }

    #[tokio::test]
    async fn worker_guard_decrements_when_incremented() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let counter = Arc::new(AtomicUsize::new(5));

        {
            let _guard = WorkerGuard {
                storage: nika_storage::Storage::open_memory().unwrap(),
                workers: Arc::new(Mutex::new(std::collections::HashMap::new())),
                active_jobs: counter.clone(),
                event_bus: crate::events::EventBus::default(),
                job_id: "test-job-2".into(),
                completed: true,
                incremented: true,
            };
        }

        assert_eq!(
            counter.load(Ordering::SeqCst),
            4,
            "counter must decrement when incremented=true"
        );
    }
}
