// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika daemon` subcommand handler.
//!
//! Manages the background daemon lifecycle:
//! - `nika daemon start` — start the daemon (background or foreground)
//! - `nika daemon stop` — graceful shutdown via SIGTERM
//! - `nika daemon restart` — stop + start
//! - `nika daemon status` — show daemon state
//! - `nika daemon logs` — tail the daemon log file

use clap::Subcommand;
use std::io::IsTerminal;
use std::time::Duration;

use nika_engine::display::StatusIcon;

use nika_daemon::lifecycle::{
    check_pid_file, cleanup_stale_socket, daemonize, is_process_alive, remove_pid_file,
    send_sigterm, wait_for_shutdown_signal, write_pid_file,
};
use nika_daemon::{
    daemon_dir, daemon_log_path, daemon_pid_path, daemon_socket_path, DaemonClient, DaemonConfig,
    DaemonResponse, DaemonServer,
};
use nika_engine::error::NikaError;

/// Daemon management actions.
#[derive(Subcommand)]
pub enum DaemonAction {
    /// Start the background daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,
    },

    /// Stop the running daemon
    Stop,

    /// Restart the daemon (stop + start)
    Restart {
        /// Run in foreground after restart
        #[arg(long)]
        foreground: bool,
    },

    /// Show daemon status
    Status,

    /// Install daemon as system service (launchd on macOS, systemd on Linux)
    Install,

    /// Uninstall daemon system service
    Uninstall,

    /// Tail daemon log file
    Logs {
        /// Follow log output (like tail -f)
        #[arg(short, long)]
        follow: bool,

        /// Number of lines to show
        #[arg(short = 'n', long, default_value = "20")]
        lines: usize,
    },
}

pub async fn handle_daemon_command(action: DaemonAction, quiet: bool) -> Result<(), NikaError> {
    match action {
        DaemonAction::Start { foreground } => start_daemon(foreground, quiet).await,
        DaemonAction::Stop => stop_daemon().await,
        DaemonAction::Restart { foreground } => {
            let _ = stop_daemon().await;
            tokio::time::sleep(Duration::from_millis(500)).await;
            start_daemon(foreground, quiet).await
        }
        DaemonAction::Status => show_status().await,
        DaemonAction::Install => {
            nika_daemon::install::install().map_err(daemon_err)?;
            eprintln!("{} daemon service installed", StatusIcon::Ok);
            Ok(())
        }
        DaemonAction::Uninstall => {
            nika_daemon::install::uninstall().map_err(daemon_err)?;
            eprintln!("{} daemon service removed", StatusIcon::Ok);
            Ok(())
        }
        DaemonAction::Logs { follow, lines } => show_logs(follow, lines).await,
    }
}

async fn start_daemon(foreground: bool, quiet: bool) -> Result<(), NikaError> {
    let socket_path = daemon_socket_path();
    let pid_path = daemon_pid_path();

    // Check for existing daemon
    if let Some(pid) = check_pid_file(&pid_path).map_err(daemon_err)? {
        if !quiet {
            eprintln!("{} daemon already running (pid {})", StatusIcon::Fail, pid);
        }
        return Ok(());
    }

    // Clean up stale socket if needed
    let _ = cleanup_stale_socket(&socket_path, &pid_path);

    // Ensure daemon dir exists
    std::fs::create_dir_all(daemon_dir()).ok();

    if !foreground {
        // Self-exec pattern: spawn `nika daemon start --foreground` as detached child.
        // Tokio's runtime doesn't survive fork(), so we re-exec instead.
        let log_path = daemon_log_path();
        daemonize(&log_path).map_err(daemon_err)?;

        // Wait briefly then verify child started
        tokio::time::sleep(Duration::from_millis(500)).await;
        let client = DaemonClient::new(&socket_path).with_timeout(Duration::from_secs(3));
        match client.ping().await {
            Ok((version, _)) => {
                if !quiet {
                    eprintln!("{} daemon started (v{version})", StatusIcon::Ok);
                    eprintln!("  socket: {}", socket_path.display());
                    eprintln!("  logs:   {}", daemon_log_path().display());
                }
            }
            Err(_) => {
                if !quiet {
                    eprintln!(
                        "{} daemon spawned but not responding yet — check logs",
                        StatusIcon::Warn
                    );
                    eprintln!("  logs: {}", daemon_log_path().display());
                }
            }
        }
        return Ok(());
    }

    // Foreground mode: run server directly in this process
    write_pid_file(&pid_path).map_err(daemon_err)?;

    if !quiet {
        eprintln!(
            "{} daemon starting in foreground (pid {})",
            StatusIcon::Info,
            std::process::id()
        );
        eprintln!("  socket: {}", socket_path.display());
        eprintln!("  press Ctrl-C to stop");
    }

    // Create and run server
    let config = DaemonConfig {
        socket_path: socket_path.clone(),
        ..DaemonConfig::default()
    };
    let server = DaemonServer::new(config);
    let shutdown = server.shutdown_handle();

    // Spawn signal handler
    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        let _ = shutdown.send(true);
    });

    // Run server (blocks until shutdown)
    server.run().await.map_err(daemon_err)?;

    // Cleanup
    remove_pid_file(&pid_path);

    if foreground && !quiet {
        eprintln!("{} daemon stopped", StatusIcon::Ok);
    }

    Ok(())
}

async fn stop_daemon() -> Result<(), NikaError> {
    let pid_path = daemon_pid_path();

    match check_pid_file(&pid_path).map_err(daemon_err)? {
        Some(pid) => {
            eprintln!("{} stopping daemon (pid {})", StatusIcon::Info, pid);

            // Send SIGTERM
            send_sigterm(pid).map_err(daemon_err)?;

            // Wait for process to exit (up to 5 seconds)
            for _ in 0..50 {
                if !is_process_alive(pid) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            // Cleanup
            remove_pid_file(&pid_path);
            let _ = std::fs::remove_file(daemon_socket_path());

            eprintln!("{} daemon stopped", StatusIcon::Ok);
            Ok(())
        }
        None => {
            eprintln!("{} daemon is not running", StatusIcon::Warn);
            Ok(())
        }
    }
}

async fn show_status() -> Result<(), NikaError> {
    let socket_path = daemon_socket_path();
    let pid_path = daemon_pid_path();

    // Check PID file
    match check_pid_file(&pid_path).map_err(daemon_err)? {
        None => {
            println!("{} daemon is not running", StatusIcon::Fail);
            println!("  socket: {}", socket_path.display());
            println!("  start:  nika daemon start");
            return Ok(());
        }
        Some(pid) => {
            // Try to ping (may take up to 2s)
            let client = DaemonClient::new(&socket_path).with_timeout(Duration::from_secs(2));
            let use_spinner = std::io::stderr().is_terminal();

            if use_spinner {
                let spinner = cliclack::spinner();
                spinner.start("Pinging daemon...");
                match client.send(nika_daemon::DaemonRequest::Status).await {
                    Ok(DaemonResponse::StatusInfo {
                        pid,
                        uptime_secs,
                        services,
                    }) => {
                        spinner.stop(format!("{} daemon is running (pid {pid})", StatusIcon::Ok));
                        println!("  uptime:   {}", format_uptime(uptime_secs));
                        println!("  socket:   {}", socket_path.display());
                        println!("  services: {}", services.join(", "));
                    }
                    Ok(other) => {
                        spinner.stop(format!(
                            "{} daemon responded unexpectedly: {other:?}",
                            StatusIcon::Warn
                        ));
                    }
                    Err(_) => {
                        spinner.stop(format!(
                            "{} daemon PID {pid} exists but not responding",
                            StatusIcon::Warn
                        ));
                        println!("  socket: {}", socket_path.display());
                        println!("  try:    nika daemon restart");
                    }
                }
            } else {
                eprintln!("Pinging daemon...");
                match client.send(nika_daemon::DaemonRequest::Status).await {
                    Ok(DaemonResponse::StatusInfo {
                        pid,
                        uptime_secs,
                        services,
                    }) => {
                        println!("{} daemon is running", StatusIcon::Ok);
                        println!("  pid:      {}", pid);
                        println!("  uptime:   {}", format_uptime(uptime_secs));
                        println!("  socket:   {}", socket_path.display());
                        println!("  services: {}", services.join(", "));
                    }
                    Ok(other) => {
                        println!(
                            "{} daemon responded unexpectedly: {:?}",
                            StatusIcon::Warn,
                            other
                        );
                    }
                    Err(_) => {
                        println!(
                            "{} daemon PID {} exists but not responding",
                            StatusIcon::Warn,
                            pid
                        );
                        println!("  socket: {}", socket_path.display());
                        println!("  try:    nika daemon restart");
                    }
                }
            }
        }
    }

    Ok(())
}

async fn show_logs(follow: bool, lines: usize) -> Result<(), NikaError> {
    let log_path = daemon_log_path();

    if !log_path.exists() {
        eprintln!(
            "{} no daemon log file found at {}",
            StatusIcon::Warn,
            log_path.display()
        );
        return Ok(());
    }

    if follow {
        // Use tail -f for simplicity. `kill_on_drop(true)` is mandatory
        // per invariant #11 — otherwise Ctrl-C / future-drop orphans an
        // infinite `tail -f` child process.
        let mut child = tokio::process::Command::new("tail")
            .args(["-f", "-n", &lines.to_string()])
            .arg(&log_path)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| NikaError::Execution(format!("failed to spawn tail: {e}")))?;

        child
            .wait()
            .await
            .map_err(|e| NikaError::Execution(format!("tail failed: {e}")))?;
    } else {
        let content = tokio::fs::read_to_string(&log_path)
            .await
            .map_err(|e| NikaError::Execution(format!("failed to read log: {e}")))?;

        let all_lines: Vec<&str> = content.lines().collect();
        let start = all_lines.len().saturating_sub(lines);
        for line in &all_lines[start..] {
            println!("{}", line);
        }
    }

    Ok(())
}

fn format_uptime(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    if hours > 0 {
        format!("{}h {}m {}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

fn daemon_err(e: nika_daemon::DaemonError) -> NikaError {
    NikaError::Execution(format!("daemon: {e}"))
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uptime_seconds() {
        assert_eq!(format_uptime(42), "42s");
    }

    #[test]
    fn format_uptime_minutes() {
        assert_eq!(format_uptime(125), "2m 5s");
    }

    #[test]
    fn format_uptime_hours() {
        assert_eq!(format_uptime(3661), "1h 1m 1s");
    }

    #[test]
    fn format_uptime_zero() {
        assert_eq!(format_uptime(0), "0s");
    }

    #[test]
    fn format_uptime_exact_hour() {
        assert_eq!(format_uptime(3600), "1h 0m 0s");
    }

    #[test]
    fn format_uptime_24h() {
        assert_eq!(format_uptime(86400), "24h 0m 0s");
    }

    #[test]
    fn daemon_action_variants_exist() {
        // Ensure all variants compile
        let _ = DaemonAction::Start { foreground: false };
        let _ = DaemonAction::Stop;
        let _ = DaemonAction::Restart { foreground: false };
        let _ = DaemonAction::Status;
        let _ = DaemonAction::Logs {
            follow: false,
            lines: 20,
        };
    }

    #[test]
    fn is_terminal_check_compiles() {
        // Verify the TTY check pattern used in show_status compiles
        let _tty = std::io::stderr().is_terminal();
    }
}
