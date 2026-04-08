// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![cfg(unix)]
//! Daemon lifecycle — PID file, daemonize, signal handling.
//!
//! Handles the full lifecycle of the daemon process:
//! - PID file creation/cleanup and stale detection
//! - Socket cleanup on startup
//! - Daemonize (double-fork on Unix)
//! - Signal handling (SIGTERM → graceful shutdown)

use std::path::Path;

use tracing::{debug, info, warn};

use crate::error::{DaemonError, DaemonResult};

// ═══════════════════════════════════════════════════════════════════════════
// PID FILE
// ═══════════════════════════════════════════════════════════════════════════

/// Write the current process PID to a file.
/// C2 fix: Use create_new(true) for exclusive creation to prevent TOCTOU race.
pub fn write_pid_file(path: &Path) -> DaemonResult<()> {
    use std::io::Write;

    let pid = std::process::id();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Exclusive create: fails if file already exists (prevents two daemons racing)
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                DaemonError::Lifecycle(format!(
                    "PID file already exists at {} — another daemon may be starting",
                    path.display()
                ))
            } else {
                DaemonError::Connection(e)
            }
        })?;

    write!(file, "{pid}")?;
    debug!(pid, path = %path.display(), "wrote PID file (exclusive)");
    Ok(())
}

/// Read the PID from a PID file.
pub fn read_pid_file(path: &Path) -> DaemonResult<u32> {
    let content = std::fs::read_to_string(path)?;
    content.trim().parse::<u32>().map_err(|_| {
        DaemonError::Protocol(format!("invalid PID in {}: {:?}", path.display(), content))
    })
}

/// Remove the PID file.
pub fn remove_pid_file(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!(error = %e, path = %path.display(), "failed to remove PID file");
        }
    }
}

/// Check if a process with the given PID is alive.
#[cfg(unix)]
pub fn is_process_alive(pid: u32) -> bool {
    // kill(pid, 0) checks if the process exists without sending a signal
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid as i32),
        None, // Signal 0 = check existence
    )
    .is_ok()
}

#[cfg(not(unix))]
pub fn is_process_alive(_pid: u32) -> bool {
    // Conservative: assume alive on non-Unix
    true
}

/// Check if the PID file refers to a still-running daemon.
/// Returns Ok(pid) if alive, Err(StalePid) if stale.
pub fn check_pid_file(path: &Path) -> DaemonResult<Option<u32>> {
    if !path.exists() {
        return Ok(None);
    }

    let pid = read_pid_file(path)?;

    if is_process_alive(pid) {
        Ok(Some(pid))
    } else {
        info!(pid, "stale PID file detected — removing");
        remove_pid_file(path);
        Ok(None)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SOCKET CLEANUP
// ═══════════════════════════════════════════════════════════════════════════

/// Remove a stale socket file if the daemon PID is dead.
///
/// Detection strategy (defense in depth):
/// 1. If PID file exists and process is alive → refuse (AlreadyRunning)
/// 2. If socket exists but no PID or PID dead → try connect as final check
/// 3. If connect fails (ConnectionRefused/timeout) → safe to remove socket
pub fn cleanup_stale_socket(socket_path: &Path, pid_path: &Path) -> DaemonResult<()> {
    if !socket_path.exists() {
        return Ok(());
    }

    match check_pid_file(pid_path)? {
        Some(pid) => {
            // Daemon is alive per PID — refuse
            Err(DaemonError::AlreadyRunning { pid })
        }
        None => {
            // PID dead or no PID file — try connecting as extra safety check
            #[cfg(unix)]
            {
                use std::os::unix::net::UnixStream;
                if UnixStream::connect(socket_path).is_ok() {
                    // Something is listening! Don't remove.
                    warn!(path = %socket_path.display(), "socket responds but no PID file — refusing to remove");
                    return Err(DaemonError::Lifecycle(
                        "socket is active but no PID file found — manually investigate".into(),
                    ));
                }
            }

            // Socket is stale — safe to remove
            info!(path = %socket_path.display(), "removing stale socket");
            std::fs::remove_file(socket_path)?;
            Ok(())
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DAEMONIZE
// ═══════════════════════════════════════════════════════════════════════════

/// Daemonize by re-executing the current binary with `--foreground`.
///
/// Why self-exec instead of double-fork: Tokio's async runtime (thread pools,
/// epoll/kqueue FDs, timer wheels) does NOT survive `fork()`. The correct
/// pattern for async Rust daemons is to spawn a new process via
/// `Command::new(current_exe())` with `--foreground` flag, then exit the parent.
///
/// The child process starts a fresh tokio runtime with zero inherited state.
/// This is the same pattern used by Turborepo's daemon.
#[cfg(unix)]
pub fn daemonize(log_path: &std::path::Path) -> DaemonResult<()> {
    let exe = std::env::current_exe()
        .map_err(|e| DaemonError::Lifecycle(format!("cannot find current exe: {e}")))?;

    // Open log file for daemon stdout/stderr
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| DaemonError::Lifecycle(format!("cannot open log file: {e}")))?;
    let log_stderr = log_file
        .try_clone()
        .map_err(|e| DaemonError::Lifecycle(format!("cannot clone log fd: {e}")))?;

    // Spawn detached child: `nika daemon start --foreground`
    let child = std::process::Command::new(exe)
        .args(["daemon", "start", "--foreground"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(log_stderr))
        .spawn()
        .map_err(|e| DaemonError::Lifecycle(format!("failed to spawn daemon: {e}")))?;

    info!(pid = child.id(), "daemon spawned in background");
    Ok(())
}

#[cfg(not(unix))]
pub fn daemonize(_log_path: &std::path::Path) -> DaemonResult<()> {
    Err(DaemonError::Lifecycle(
        "background daemon is not supported on this platform — use --foreground".into(),
    ))
}

// ═══════════════════════════════════════════════════════════════════════════
// SIGNAL HANDLING
// ═══════════════════════════════════════════════════════════════════════════

/// Send SIGTERM to a process.
#[cfg(unix)]
pub fn send_sigterm(pid: u32) -> DaemonResult<()> {
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid as i32),
        nix::sys::signal::Signal::SIGTERM,
    )
    .map_err(|e| DaemonError::Lifecycle(format!("failed to send SIGTERM to pid {pid}: {e}")))?;
    Ok(())
}

#[cfg(not(unix))]
pub fn send_sigterm(_pid: u32) -> DaemonResult<()> {
    Err(DaemonError::Lifecycle(
        "SIGTERM not supported on this platform".into(),
    ))
}

/// Install signal handlers and return a future that resolves on shutdown signal.
///
/// Handles:
/// - SIGTERM → graceful shutdown
/// - SIGINT → graceful shutdown
/// - SIGHUP → (future: config reload)
#[cfg(unix)]
pub async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    // Use .ok() instead of .expect() — signal setup can fail in restricted
    // environments (containers, sandboxes). With Restart=always, systemd
    // handles termination directly if signal handlers can't be registered.
    let sigterm = signal(SignalKind::terminate()).ok();
    let sigint = signal(SignalKind::interrupt()).ok();

    if sigterm.is_none() && sigint.is_none() {
        warn!("no signal handlers could be registered — daemon will rely on systemd for shutdown");
    }

    async fn wait_for_signal(mut sig: Option<tokio::signal::unix::Signal>) {
        match sig.as_mut() {
            Some(s) => {
                s.recv().await;
            }
            None => std::future::pending::<()>().await,
        }
    }

    tokio::select! {
        biased;
        _ = wait_for_signal(sigterm) => {
            info!("received SIGTERM");
        }
        _ = wait_for_signal(sigint) => {
            info!("received SIGINT");
        }
    }
}

#[cfg(not(unix))]
pub async fn wait_for_shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        warn!(error = %e, "Ctrl-C handler failed — daemon will rely on OS for shutdown");
        std::future::pending::<()>().await;
    }
    info!("received Ctrl-C");
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_file_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");

        write_pid_file(&pid_path).unwrap();

        let pid = read_pid_file(&pid_path).unwrap();
        assert_eq!(pid, std::process::id());
    }

    #[test]
    fn pid_file_read_nonexistent_returns_error() {
        let result = read_pid_file(Path::new("/tmp/nonexistent_nika_test.pid"));
        assert!(result.is_err());
    }

    #[test]
    fn pid_file_read_invalid_content() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("bad.pid");
        std::fs::write(&pid_path, "not-a-number").unwrap();

        let result = read_pid_file(&pid_path);
        assert!(result.is_err());
    }

    #[test]
    fn pid_file_remove() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");
        std::fs::write(&pid_path, "1234").unwrap();

        remove_pid_file(&pid_path);
        assert!(!pid_path.exists());
    }

    #[test]
    fn pid_file_remove_nonexistent_no_panic() {
        remove_pid_file(Path::new("/tmp/nonexistent_nika_test.pid"));
        // Should not panic
    }

    #[test]
    fn is_process_alive_for_current_process() {
        assert!(is_process_alive(std::process::id()));
    }

    #[test]
    fn is_process_alive_for_dead_process() {
        // PID 99999999 is almost certainly not running
        assert!(!is_process_alive(99_999_999));
    }

    #[test]
    fn check_pid_file_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");

        let result = check_pid_file(&pid_path).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn check_pid_file_alive_process() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");

        // Write current process PID (guaranteed alive)
        write_pid_file(&pid_path).unwrap();

        let result = check_pid_file(&pid_path).unwrap();
        assert_eq!(result, Some(std::process::id()));
    }

    #[test]
    fn check_pid_file_stale_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");

        // Write a PID that doesn't exist
        std::fs::write(&pid_path, "99999999").unwrap();

        let result = check_pid_file(&pid_path).unwrap();
        assert_eq!(result, None);
        // Stale file should be removed
        assert!(!pid_path.exists());
    }

    #[test]
    fn cleanup_stale_socket_no_socket() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let pid = dir.path().join("test.pid");

        // No socket = nothing to clean
        let result = cleanup_stale_socket(&sock, &pid);
        assert!(result.is_ok());
    }

    #[test]
    fn cleanup_stale_socket_removes_dead_daemon_socket() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let pid_path = dir.path().join("test.pid");

        // Create a stale socket + PID file with dead PID
        std::fs::write(&sock, "stale-socket").unwrap();
        std::fs::write(&pid_path, "99999999").unwrap();

        let result = cleanup_stale_socket(&sock, &pid_path);
        assert!(result.is_ok());
        assert!(!sock.exists());
    }

    #[test]
    fn cleanup_stale_socket_refuses_if_daemon_alive() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let pid_path = dir.path().join("test.pid");

        // Create socket + PID file with current PID (alive)
        std::fs::write(&sock, "active-socket").unwrap();
        write_pid_file(&pid_path).unwrap();

        let result = cleanup_stale_socket(&sock, &pid_path);
        assert!(matches!(
            result.unwrap_err(),
            DaemonError::AlreadyRunning { .. }
        ));
    }

    #[test]
    fn pid_file_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("nested").join("deep").join("test.pid");

        write_pid_file(&pid_path).unwrap();
        assert!(pid_path.exists());
    }
}
