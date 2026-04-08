// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![cfg(unix)]
//! Daemon auto-start installation — launchd (macOS) + systemd (Linux).
//!
//! `nika daemon install` generates and installs a platform-specific service
//! definition so the daemon starts automatically on login.
//!
//! - macOS: ~/Library/LaunchAgents/studio.supernovae.nika.daemon.plist
//! - Linux: ~/.config/systemd/user/nika-daemon.service

use std::path::PathBuf;

use tracing::info;

use crate::error::{DaemonError, DaemonResult};

/// Service label for launchd.
#[cfg(target_os = "macos")]
const LAUNCHD_LABEL: &str = "studio.supernovae.nika.daemon";

/// Service name for systemd.
#[cfg(target_os = "linux")]
const SYSTEMD_SERVICE: &str = "nika-daemon.service";

/// Install the daemon as a system service.
pub fn install() -> DaemonResult<()> {
    let exe = std::env::current_exe()
        .map_err(|e| DaemonError::Lifecycle(format!("cannot find current exe: {e}")))?;

    #[cfg(target_os = "macos")]
    {
        install_launchd(&exe)?;
    }

    #[cfg(target_os = "linux")]
    {
        install_systemd(&exe)?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        return Err(DaemonError::Lifecycle(
            "daemon install is only supported on macOS and Linux".into(),
        ));
    }

    Ok(())
}

/// Uninstall the daemon service.
pub fn uninstall() -> DaemonResult<()> {
    #[cfg(target_os = "macos")]
    {
        uninstall_launchd()?;
    }

    #[cfg(target_os = "linux")]
    {
        uninstall_systemd()?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        return Err(DaemonError::Lifecycle(
            "daemon uninstall is only supported on macOS and Linux".into(),
        ));
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// MACOS LAUNCHD
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
fn launchd_plist_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist"))
}

#[cfg(target_os = "macos")]
fn install_launchd(exe: &std::path::Path) -> DaemonResult<()> {
    let plist_path = launchd_plist_path();
    let log_path = crate::daemon_log_path();
    let err_path = crate::daemon_dir().join("nika.err.log");

    // Ensure LaunchAgents directory exists
    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>daemon</string>
        <string>start</string>
        <string>--foreground</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{err}</string>
    <key>ProcessType</key>
    <string>Background</string>
</dict>
</plist>
"#,
        label = LAUNCHD_LABEL,
        exe = exe.display(),
        log = log_path.display(),
        err = err_path.display(),
    );

    std::fs::write(&plist_path, plist)?;

    // launchctl bootstrap (modern API, replaces deprecated `launchctl load`)
    let uid_str = std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "501".to_string());
    let status = std::process::Command::new("launchctl")
        .args([
            "bootstrap",
            &format!("gui/{uid_str}"),
            &plist_path.to_string_lossy(),
        ])
        .status()
        .map_err(|e| DaemonError::Lifecycle(format!("launchctl bootstrap failed: {e}")))?;

    if !status.success() {
        // Try legacy load as fallback
        let _ = std::process::Command::new("launchctl")
            .args(["load", &plist_path.to_string_lossy()])
            .status();
    }

    info!(path = %plist_path.display(), "launchd service installed");
    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_launchd() -> DaemonResult<()> {
    let plist_path = launchd_plist_path();

    if !plist_path.exists() {
        return Err(DaemonError::Lifecycle("launchd plist not found".into()));
    }

    // launchctl bootout (modern API)
    let uid_str = std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "501".to_string());
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid_str}/{LAUNCHD_LABEL}")])
        .status();

    std::fs::remove_file(&plist_path)?;
    info!(path = %plist_path.display(), "launchd service removed");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// LINUX SYSTEMD
// ═══════════════════════════════════════════════════════════════════════════

/// Generate the systemd unit template for the given executable path.
///
/// Extracted to allow testing on all platforms.
#[cfg(any(target_os = "linux", test))]
fn systemd_unit_template(exe: &std::path::Path) -> String {
    format!(
        r#"[Unit]
Description=Nika Workflow Daemon
After=default.target

[Service]
Type=notify
ExecStart={exe} daemon start --foreground
Restart=always
RestartSec=5
EnvironmentFile=-%h/.nika/.env
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
"#,
        exe = exe.display(),
    )
}

#[cfg(target_os = "linux")]
fn systemd_unit_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".config/systemd/user")
        .join(SYSTEMD_SERVICE)
}

#[cfg(target_os = "linux")]
fn install_systemd(exe: &std::path::Path) -> DaemonResult<()> {
    let unit_path = systemd_unit_path();

    if let Some(parent) = unit_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let unit = systemd_unit_template(exe);

    std::fs::write(&unit_path, unit)?;

    // Enable and start
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "enable", SYSTEMD_SERVICE])
        .status();

    info!(path = %unit_path.display(), "systemd service installed");
    info!("NOTE: run `loginctl enable-linger` for the daemon to survive logout");
    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_systemd() -> DaemonResult<()> {
    let unit_path = systemd_unit_path();

    if !unit_path.exists() {
        return Err(DaemonError::Lifecycle("systemd unit not found".into()));
    }

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "disable", SYSTEMD_SERVICE])
        .status();

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "stop", SYSTEMD_SERVICE])
        .status();

    std::fs::remove_file(&unit_path)?;

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    info!(path = %unit_path.display(), "systemd service removed");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(target_os = "macos")]
    fn launchd_label_is_valid() {
        assert!(super::LAUNCHD_LABEL.contains("nika"));
        assert!(super::LAUNCHD_LABEL.starts_with("studio.supernovae"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn systemd_service_name() {
        assert!(super::SYSTEMD_SERVICE.ends_with(".service"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launchd_plist_path_under_home() {
        let path = super::launchd_plist_path();
        assert!(path.to_string_lossy().contains("LaunchAgents"));
        assert!(path.to_string_lossy().contains("nika"));
    }

    #[test]
    fn systemd_unit_has_restart_always_and_env_file() {
        let exe = std::path::Path::new("/usr/local/bin/nika");
        let unit = super::systemd_unit_template(exe);
        assert!(unit.contains("Restart=always"), "must have Restart=always");
        assert!(
            !unit.contains("Restart=on-failure"),
            "must NOT have Restart=on-failure"
        );
        assert!(
            unit.contains("EnvironmentFile=-%h/.nika/.env"),
            "must have EnvironmentFile for secrets"
        );
        assert!(unit.contains("RestartSec=5"), "must have RestartSec=5");
        assert!(
            unit.contains("Type=notify"),
            "must use Type=notify for sd_notify readiness signal"
        );
    }
}
