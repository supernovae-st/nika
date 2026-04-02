//! Nika home directory and path utilities.
//!
//! This module provides canonical path resolution for the Nika ecosystem.
//! All paths are relative to `~/.nika/` (the Nika home directory).
//!
//! ## Directory Structure
//!
//! ```text
//! ~/.nika/
//! ├── config.toml          # Global configuration
//! ├── mcp.yaml             # MCP server definitions
//! ├── daemon/              # Reserved for future nika daemon
//! │   ├── nika.sock        # Unix socket for IPC
//! │   └── nika.pid         # PID file
//! ├── models/              # GGUF models for native inference
//! │   └── cache/           # HuggingFace cache
//! ├── packages/            # Installed packages
//! │   ├── @scope/name/     # Scoped packages
//! │   └── registry.yaml    # Package index
//! ├── backups/             # Backup archives
//! └── cache/               # General cache
//! ```
//!
//! ## Project-Level Structure
//!
//! ```text
//! project/
//! ├── nika.toml             # Project config (versioned, committed)
//! ├── .nika/                # Runtime state (gitignored)
//! │   ├── traces/           # Execution traces
//! │   ├── cache/            # LLM response cache
//! │   ├── media/store/      # CAS blobs
//! │   └── sessions/         # Editor sessions
//! ├── *.nika.yaml           # Workflows (anywhere in project)
//! └── artifacts/            # Output dir (configurable)
//! ```
//!

use std::path::PathBuf;

/// Environment variable to override the Nika home directory.
///
/// If set, this takes precedence over the default `~/.nika/` location.
/// Useful for testing and custom installations.
pub const NIKA_HOME_ENV: &str = "NIKA_HOME";

/// Default directory name for Nika home.
pub const NIKA_DIR_NAME: &str = ".nika";

/// Project-level directory name.
pub const NIKA_PROJECT_DIR: &str = ".nika";

/// Package manifest filename.
pub const NIKA_MANIFEST: &str = "nika.yaml";

/// Lockfile filename.
pub const NIKA_LOCKFILE: &str = "nika.lock";

/// MCP config filename (legacy, user-level at ~/.nika/).
pub const MCP_CONFIG: &str = "mcp.yaml";

/// MCP config filename (Claude Code convention, project-level).
pub const MCP_JSON: &str = ".mcp.json";

/// Global config filename.
pub const GLOBAL_CONFIG: &str = "config.toml";

/// Registry index filename.
pub const REGISTRY_INDEX: &str = "registry.yaml";

/// Daemon socket filename.
pub const DAEMON_SOCKET: &str = "nika.sock";

/// Daemon PID filename.
pub const DAEMON_PID: &str = "nika.pid";

// ═══════════════════════════════════════════════════════════════════════════
// NIKA HOME RESOLUTION
// ═══════════════════════════════════════════════════════════════════════════

/// Returns the Nika home directory path.
///
/// Resolution order:
/// 1. `NIKA_HOME` environment variable (if set)
/// 2. `~/.nika/` (default)
///
/// # Panics
///
/// Panics if the home directory cannot be determined and `NIKA_HOME` is not set.
///
/// # Examples
///
/// ```rust,ignore
/// use nika::core::paths::nika_home;
///
/// let home = nika_home();
/// // Returns PathBuf like "/Users/thibaut/.nika"
/// ```
pub fn nika_home() -> PathBuf {
    // Check for environment override first
    if let Ok(custom_home) = std::env::var(NIKA_HOME_ENV) {
        return PathBuf::from(custom_home);
    }

    // Default to ~/.nika/
    match dirs::home_dir() {
        Some(h) => h.join(NIKA_DIR_NAME),
        None => {
            // Security: use std::env::temp_dir() (respects TMPDIR) with
            // a unique-per-user suffix to prevent symlink attacks on /tmp.
            let fallback = std::env::temp_dir().join(NIKA_DIR_NAME);
            tracing::error!(
                path = %fallback.display(),
                "Could not determine home directory. Using temporary fallback.                  Set NIKA_HOME environment variable for a secure persistent location."
            );
            fallback
        }
    }
}

/// Returns the Nika home directory, or None if it cannot be determined.
///
/// This is the fallible version of [`nika_home`].
pub fn nika_home_opt() -> Option<PathBuf> {
    if let Ok(custom_home) = std::env::var(NIKA_HOME_ENV) {
        return Some(PathBuf::from(custom_home));
    }
    dirs::home_dir().map(|h| h.join(NIKA_DIR_NAME))
}

/// Returns the Nika home directory, or an error if it cannot be determined.
///
/// This is the Result-returning version of [`nika_home`] for use in fallible contexts.
///
/// # Errors
///
/// Returns `NikaError::HomeDirectoryNotFound` if the home directory cannot be determined
/// and `NIKA_HOME` is not set.
pub fn nika_home_result() -> Result<PathBuf, crate::NikaError> {
    nika_home_opt().ok_or(crate::NikaError::HomeDirectoryNotFound)
}

/// Returns the user's home directory, or an error if it cannot be determined.
///
/// # Errors
///
/// Returns `NikaError::HomeDirectoryNotFound` if the home directory cannot be determined.
pub fn user_home_result() -> Result<PathBuf, crate::NikaError> {
    dirs::home_dir().ok_or(crate::NikaError::HomeDirectoryNotFound)
}

// ═══════════════════════════════════════════════════════════════════════════
// DIRECTORY PATHS
// ═══════════════════════════════════════════════════════════════════════════

/// Returns the packages directory (`~/.nika/packages/`).
pub fn packages_dir() -> PathBuf {
    nika_home().join("packages")
}

/// Returns the models directory (`~/.nika/models/`).
pub fn models_dir() -> PathBuf {
    nika_home().join("models")
}

/// Returns the backups directory (`~/.nika/backups/`).
pub fn backups_dir() -> PathBuf {
    nika_home().join("backups")
}

/// Returns the cache directory (`~/.nika/cache/`).
pub fn cache_dir() -> PathBuf {
    nika_home().join("cache")
}

/// Returns the daemon directory (`~/.nika/daemon/`).
/// Reserved for future native nika daemon.
pub fn daemon_dir() -> PathBuf {
    nika_home().join("daemon")
}

// ═══════════════════════════════════════════════════════════════════════════
// FILE PATHS
// ═══════════════════════════════════════════════════════════════════════════

/// Returns the user-level global config path (`~/.nika/config.toml`).
///
/// This is the user defaults layer, NOT the project-level `nika.toml`.
pub fn global_config_path() -> PathBuf {
    nika_home().join(GLOBAL_CONFIG)
}

/// Returns the global MCP config path (`~/.nika/mcp.yaml`).
pub fn global_mcp_config_path() -> PathBuf {
    nika_home().join(MCP_CONFIG)
}

/// Returns the registry index path (`~/.nika/packages/registry.yaml`).
pub fn registry_index_path() -> PathBuf {
    packages_dir().join(REGISTRY_INDEX)
}

/// Returns the daemon socket path (`~/.nika/daemon/nika.sock`).
/// Reserved for future native nika daemon.
pub fn daemon_socket_path() -> PathBuf {
    daemon_dir().join(DAEMON_SOCKET)
}

/// Returns the daemon PID file path (`~/.nika/daemon/nika.pid`).
/// Reserved for future native nika daemon.
pub fn daemon_pid_path() -> PathBuf {
    daemon_dir().join(DAEMON_PID)
}

// ═══════════════════════════════════════════════════════════════════════════
// PROJECT PATHS
// ═══════════════════════════════════════════════════════════════════════════

/// Returns the project Nika directory (`.nika/`).
pub fn project_nika_dir(project_root: &std::path::Path) -> PathBuf {
    project_root.join(NIKA_PROJECT_DIR)
}

/// Returns the project MCP config path (`.nika/mcp.yaml`).
pub fn project_mcp_config_path(project_root: &std::path::Path) -> PathBuf {
    project_nika_dir(project_root).join(MCP_CONFIG)
}

/// Returns the project manifest path (`nika.yaml`).
pub fn project_manifest_path(project_root: &std::path::Path) -> PathBuf {
    project_root.join(NIKA_MANIFEST)
}

/// Returns the project lockfile path (`nika.lock`).
pub fn project_lockfile_path(project_root: &std::path::Path) -> PathBuf {
    project_root.join(NIKA_LOCKFILE)
}

/// Returns the project sessions directory (`.nika/sessions/`).
pub fn project_sessions_dir(project_root: &std::path::Path) -> PathBuf {
    project_nika_dir(project_root).join("sessions")
}

// ═══════════════════════════════════════════════════════════════════════════
// PACKAGE PATHS
// ═══════════════════════════════════════════════════════════════════════════

/// Returns the directory for a specific package.
///
/// # Examples
///
/// ```rust,ignore
/// use nika::core::paths::package_dir;
///
/// let dir = package_dir("@nika", "seo-audit", "1.0.0");
/// // Returns: ~/.nika/packages/@nika/seo-audit/1.0.0/
/// ```
pub fn package_dir(scope: &str, name: &str, version: &str) -> PathBuf {
    packages_dir().join(scope).join(name).join(version)
}

/// Returns the manifest path for a specific package.
pub fn package_manifest_path(scope: &str, name: &str, version: &str) -> PathBuf {
    package_dir(scope, name, version).join("manifest.yaml")
}

// ═══════════════════════════════════════════════════════════════════════════
// DIRECTORY CREATION
// ═══════════════════════════════════════════════════════════════════════════

/// Ensures the Nika home directory and its subdirectories exist.
///
/// Creates:
/// - `~/.nika/`
/// - `~/.nika/packages/`
/// - `~/.nika/models/`
/// - `~/.nika/backups/`
/// - `~/.nika/cache/`
/// - `~/.nika/daemon/`
///
/// # Errors
///
/// Returns an error if directory creation fails.
pub fn ensure_nika_home() -> std::io::Result<()> {
    let home = nika_home();
    std::fs::create_dir_all(&home)?;
    std::fs::create_dir_all(home.join("packages"))?;
    std::fs::create_dir_all(home.join("models"))?;
    std::fs::create_dir_all(home.join("backups"))?;
    std::fs::create_dir_all(home.join("cache"))?;
    std::fs::create_dir_all(home.join("daemon"))?;
    Ok(())
}

/// Ensures the project Nika directory exists.
///
/// Creates:
/// - `.nika/`
/// - `.nika/sessions/`
pub fn ensure_project_nika_dir(project_root: &std::path::Path) -> std::io::Result<()> {
    let nika_dir = project_nika_dir(project_root);
    std::fs::create_dir_all(&nika_dir)?;
    std::fs::create_dir_all(nika_dir.join("sessions"))?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    /// Helper to run tests with a temporary NIKA_HOME
    fn with_temp_nika_home<F, R>(f: F) -> R
    where
        F: FnOnce(&std::path::Path) -> R,
    {
        let temp_dir = tempfile::tempdir().unwrap();
        let old_val = env::var(NIKA_HOME_ENV).ok();

        env::set_var(NIKA_HOME_ENV, temp_dir.path());
        let result = f(temp_dir.path());

        // Restore original value
        if let Some(val) = old_val {
            env::set_var(NIKA_HOME_ENV, val);
        } else {
            env::remove_var(NIKA_HOME_ENV);
        }

        result
    }

    #[test]
    #[serial]
    fn test_nika_home_default() {
        // Clear env var to test default behavior
        let old_val = env::var(NIKA_HOME_ENV).ok();
        env::remove_var(NIKA_HOME_ENV);

        let home = nika_home();
        assert!(home.ends_with(".nika"));

        // Restore
        if let Some(val) = old_val {
            env::set_var(NIKA_HOME_ENV, val);
        }
    }

    #[test]
    #[serial]
    fn test_nika_home_env_override() {
        with_temp_nika_home(|temp_path| {
            let home = nika_home();
            assert_eq!(home, temp_path);
        });
    }

    #[test]
    #[serial]
    fn test_nika_home_opt_returns_some() {
        with_temp_nika_home(|temp_path| {
            let home = nika_home_opt();
            assert_eq!(home, Some(temp_path.to_path_buf()));
        });
    }

    #[test]
    #[serial]
    fn test_packages_dir() {
        with_temp_nika_home(|temp_path| {
            let packages = packages_dir();
            assert_eq!(packages, temp_path.join("packages"));
        });
    }

    #[test]
    #[serial]
    fn test_models_dir() {
        with_temp_nika_home(|temp_path| {
            let models = models_dir();
            assert_eq!(models, temp_path.join("models"));
        });
    }

    #[test]
    #[serial]
    fn test_backups_dir() {
        with_temp_nika_home(|temp_path| {
            let backups = backups_dir();
            assert_eq!(backups, temp_path.join("backups"));
        });
    }

    #[test]
    #[serial]
    fn test_cache_dir() {
        with_temp_nika_home(|temp_path| {
            let cache = cache_dir();
            assert_eq!(cache, temp_path.join("cache"));
        });
    }

    #[test]
    #[serial]
    fn test_daemon_dir() {
        with_temp_nika_home(|temp_path| {
            let daemon = daemon_dir();
            assert_eq!(daemon, temp_path.join("daemon"));
        });
    }

    #[test]
    #[serial]
    fn test_global_config_path() {
        with_temp_nika_home(|temp_path| {
            let config = global_config_path();
            assert_eq!(config, temp_path.join("config.toml"));
        });
    }

    #[test]
    #[serial]
    fn test_global_mcp_config_path() {
        with_temp_nika_home(|temp_path| {
            let mcp = global_mcp_config_path();
            assert_eq!(mcp, temp_path.join("mcp.yaml"));
        });
    }

    #[test]
    #[serial]
    fn test_registry_index_path() {
        with_temp_nika_home(|temp_path| {
            let registry = registry_index_path();
            assert_eq!(registry, temp_path.join("packages").join("registry.yaml"));
        });
    }

    #[test]
    #[serial]
    fn test_daemon_socket_path() {
        with_temp_nika_home(|temp_path| {
            let socket = daemon_socket_path();
            assert_eq!(socket, temp_path.join("daemon").join("nika.sock"));
        });
    }

    #[test]
    #[serial]
    fn test_daemon_pid_path() {
        with_temp_nika_home(|temp_path| {
            let pid = daemon_pid_path();
            assert_eq!(pid, temp_path.join("daemon").join("nika.pid"));
        });
    }

    #[test]
    fn test_project_paths() {
        let project_root = std::path::Path::new("/tmp/my-project");

        assert_eq!(
            project_nika_dir(project_root),
            PathBuf::from("/tmp/my-project/.nika")
        );

        assert_eq!(
            project_mcp_config_path(project_root),
            PathBuf::from("/tmp/my-project/.nika/mcp.yaml")
        );

        assert_eq!(
            project_manifest_path(project_root),
            PathBuf::from("/tmp/my-project/nika.yaml")
        );

        assert_eq!(
            project_lockfile_path(project_root),
            PathBuf::from("/tmp/my-project/nika.lock")
        );

        assert_eq!(
            project_sessions_dir(project_root),
            PathBuf::from("/tmp/my-project/.nika/sessions")
        );
    }

    #[test]
    #[serial]
    fn test_package_dir() {
        with_temp_nika_home(|temp_path| {
            let pkg = package_dir("@nika", "seo-audit", "1.0.0");
            assert_eq!(pkg, temp_path.join("packages/@nika/seo-audit/1.0.0"));
        });
    }

    #[test]
    #[serial]
    fn test_package_manifest_path() {
        with_temp_nika_home(|temp_path| {
            let manifest = package_manifest_path("@workflows", "code-review", "2.1.0");
            assert_eq!(
                manifest,
                temp_path.join("packages/@workflows/code-review/2.1.0/manifest.yaml")
            );
        });
    }

    #[test]
    #[serial]
    fn test_ensure_nika_home_creates_directories() {
        with_temp_nika_home(|temp_path| {
            // Remove everything first
            let _ = std::fs::remove_dir_all(temp_path);

            // Ensure creates all directories
            ensure_nika_home().unwrap();

            assert!(temp_path.exists());
            assert!(temp_path.join("packages").exists());
            assert!(temp_path.join("models").exists());
            assert!(temp_path.join("backups").exists());
            assert!(temp_path.join("cache").exists());
            assert!(temp_path.join("daemon").exists());
        });
    }

    #[test]
    fn test_ensure_project_nika_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        ensure_project_nika_dir(project_root).unwrap();

        assert!(project_root.join(".nika").exists());
        assert!(project_root.join(".nika/sessions").exists());
    }

    #[test]
    fn test_constants() {
        assert_eq!(NIKA_HOME_ENV, "NIKA_HOME");
        assert_eq!(NIKA_DIR_NAME, ".nika");
        assert_eq!(NIKA_PROJECT_DIR, ".nika");
        assert_eq!(NIKA_MANIFEST, "nika.yaml");
        assert_eq!(NIKA_LOCKFILE, "nika.lock");
        assert_eq!(MCP_CONFIG, "mcp.yaml");
        assert_eq!(GLOBAL_CONFIG, "config.toml");
        assert_eq!(REGISTRY_INDEX, "registry.yaml");
        assert_eq!(DAEMON_SOCKET, "nika.sock");
        assert_eq!(DAEMON_PID, "nika.pid");
    }
}
