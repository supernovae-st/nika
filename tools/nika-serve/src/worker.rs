//! Subprocess worker that executes `nika run` for a single job.
//!
//! V1 architecture: no embedded engine -- we spawn `current_exe() run workflow.nika.yaml`
//! as a child process with process-group isolation (setsid on Unix).
//!
//! ERRATA-11: Use `std::env::current_exe()` instead of `Command::new("nika")`.
//! ERRATA-15: Kill entire PGID on timeout via `nix::sys::signal::kill(-pid, SIGKILL)`.
//! ERRATA-7:  UTF-8 safe truncation on both stdout and stderr.

use std::sync::Arc;

use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::config::ServeConfig;
use crate::state::AppState;

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
fn kill_process_group(pid: u32) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    // Negative PID = signal the process group whose PGID equals |pid|
    let _ = kill(Pid::from_raw(-(pid as i32)), Signal::SIGKILL);
}

// ═══════════════════════════════════════════════════════════════════════════
// SPAWN
// ═══════════════════════════════════════════════════════════════════════════

/// Spawn a background worker for a job. Returns the `JoinHandle` which is
/// also stored in `AppState.workers` for cancel + drain (ERRATA-9).
pub fn spawn_worker(state: &AppState, job_id: String, workflow: String) -> JoinHandle<()> {
    let storage = state.storage.clone();
    let config = Arc::clone(&state.config);
    let semaphore = Arc::clone(&state.semaphore);
    let workers = Arc::clone(&state.workers);
    let id = job_id.clone();

    tokio::spawn(async move {
        // Acquire concurrency permit (blocks if max_concurrent workers are busy)
        let _permit = semaphore.acquire().await;

        info!(job_id = %id, workflow = %workflow, "worker started");

        // Mark running
        if let Err(e) = storage
            .update_state(&id, nika_storage::JobState::Running, None, None)
            .await
        {
            error!(job_id = %id, error = %e, "failed to mark job running");
            remove_worker(&workers, &id).await;
            return;
        }

        let result = run_subprocess(&config, &workflow).await;

        match result {
            Ok(output) => {
                let truncated = safe_truncate(&output, config.max_output_bytes);
                if let Err(e) = storage.complete_job(&id, truncated).await {
                    error!(job_id = %id, error = %e, "failed to mark job completed");
                }
                info!(job_id = %id, "job completed");
            }
            Err(msg) => {
                let truncated = safe_truncate(&msg, config.max_output_bytes);
                if let Err(e) = storage.fail_job(&id, truncated).await {
                    error!(job_id = %id, error = %e, "failed to mark job failed");
                }
                warn!(job_id = %id, error = %msg, "job failed");
            }
        }

        remove_worker(&workers, &id).await;
    })
}

/// Remove a worker from the tracking map after completion.
async fn remove_worker(
    workers: &Arc<Mutex<std::collections::HashMap<String, JoinHandle<()>>>>,
    job_id: &str,
) {
    workers.lock().await.remove(job_id);
}

// ═══════════════════════════════════════════════════════════════════════════
// SUBPROCESS EXECUTION
// ═══════════════════════════════════════════════════════════════════════════

/// Execute `nika run <workflow>` as a subprocess with timeout + PGID isolation.
///
/// The subprocess inherits the parent environment minus secrets
/// (`NIKA_SERVE_TOKEN`, `NIKA_SERVE_DB`) to prevent accidental leakage.
async fn run_subprocess(config: &ServeConfig, workflow: &str) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("failed to resolve current_exe: {e}"))?;

    let workflow_path = config.workflows_dir.join(workflow);

    let mut cmd = Command::new(&exe);
    cmd.arg("run")
        .arg(&workflow_path)
        .arg("--no-live")
        .env_remove("NIKA_SERVE_TOKEN")
        .env_remove("NIKA_SERVE_DB")
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

    let child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;

    // Capture the PID before we await (needed for PGID kill on timeout).
    let child_pid = child.id();

    let timeout = std::time::Duration::from_secs(config.job_timeout_secs);

    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                Ok(stdout.into_owned())
            } else {
                let code = output.status.code().unwrap_or(-1);
                let truncated_stdout = safe_truncate(&stdout, config.max_output_bytes / 2);
                let truncated_stderr = safe_truncate(&stderr, config.max_output_bytes / 2);
                Err(format!(
                    "exit code {code}\n--- stdout ---\n{truncated_stdout}\n--- stderr ---\n{truncated_stderr}"
                ))
            }
        }
        Ok(Err(e)) => Err(format!("process I/O error: {e}")),
        Err(_elapsed) => {
            // Timeout: kill the entire process group (ERRATA-15)
            if let Some(pid) = child_pid {
                debug!(pid, "killing process group after timeout");
                #[cfg(unix)]
                kill_process_group(pid);

                #[cfg(not(unix))]
                {
                    let _ = child.kill().await;
                }
            }

            Err(format!("timeout after {}s", config.job_timeout_secs))
        }
    }
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
}
