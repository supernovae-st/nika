// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sandbox traits — capability-based isolation for WASM + MCP.
//!
//! Reserved for v0.100. Real implementations will live in
//! `nika-sandbox-landlock` (Linux) and `nika-sandbox-seatbelt` (macOS).
//! See `docs/architecture/forward-compat-invariants.md` Pattern 1.

/// Capability-based sandbox for WASM plugins and MCP servers.
///
/// Reserved for v0.100. The capability model will gate:
/// - filesystem access (read/write/glob paths)
/// - network access (allowlisted hosts/ports)
/// - process spawning (enabled/disabled)
/// - environment variable access (allowlisted keys)
#[trait_variant::make(SandboxDyn: Send)]
pub trait Sandbox: Send + Sync {
    /// Check whether a capability is granted.
    async fn check_capability(&self, cap: &Capability) -> Result<bool, SandboxError>;

    /// Enter the sandbox (apply restrictions).
    async fn enter(&self) -> Result<(), SandboxError>;
}

/// A capability request.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Capability {
    /// Read a filesystem path.
    FsRead {
        /// The path pattern (glob-style).
        path: String,
    },
    /// Write to a filesystem path.
    FsWrite {
        /// The path pattern (glob-style).
        path: String,
    },
    /// Access a network host.
    Network {
        /// Host (domain or IP).
        host: String,
        /// Port (None = any).
        port: Option<u16>,
    },
    /// Spawn a subprocess.
    ProcessSpawn,
    /// Read an environment variable.
    EnvRead {
        /// Variable name pattern.
        key: String,
    },
}

impl Capability {
    /// Create a filesystem read capability.
    #[must_use]
    pub fn fs_read(path: impl Into<String>) -> Self {
        Self::FsRead { path: path.into() }
    }

    /// Create a filesystem write capability.
    #[must_use]
    pub fn fs_write(path: impl Into<String>) -> Self {
        Self::FsWrite { path: path.into() }
    }

    /// Create a network access capability.
    #[must_use]
    pub fn network(host: impl Into<String>, port: Option<u16>) -> Self {
        Self::Network {
            host: host.into(),
            port,
        }
    }

    /// Create an environment variable read capability.
    #[must_use]
    pub fn env_read(key: impl Into<String>) -> Self {
        Self::EnvRead { key: key.into() }
    }
}

/// Sandbox errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum SandboxError {
    /// Sandbox not available on this platform.
    #[error("sandbox unavailable: {reason}")]
    Unavailable {
        /// Why the sandbox is unavailable.
        reason: String,
    },

    /// Capability denied.
    #[error("capability denied: {reason}")]
    CapabilityDenied {
        /// What was denied and why.
        reason: String,
    },

    /// Sandbox setup failed.
    #[error("sandbox setup failed: {reason}")]
    SetupFailed {
        /// What went wrong during sandbox setup.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn sandbox_types_are_send_sync() {
        _assert_send_sync::<Capability>();
        _assert_send_sync::<SandboxError>();
    }

    #[test]
    fn capability_constructors() {
        let cap = Capability::fs_read("/tmp");
        assert!(matches!(cap, Capability::FsRead { path } if path == "/tmp"));

        let cap = Capability::fs_write("/var/log");
        assert!(matches!(cap, Capability::FsWrite { path } if path == "/var/log"));

        let cap = Capability::network("example.com", Some(443));
        assert!(
            matches!(cap, Capability::Network { host, port } if host == "example.com" && port == Some(443))
        );

        let cap = Capability::env_read("PATH");
        assert!(matches!(cap, Capability::EnvRead { key } if key == "PATH"));
    }

    #[test]
    fn capability_eq() {
        assert_eq!(Capability::ProcessSpawn, Capability::ProcessSpawn);
        assert_ne!(Capability::fs_read("/a"), Capability::fs_read("/b"),);
    }

    #[test]
    fn sandbox_error_display() {
        let err = SandboxError::CapabilityDenied {
            reason: "no network access".into(),
        };
        assert!(err.to_string().contains("capability denied"));

        let err = SandboxError::Unavailable {
            reason: "platform".into(),
        };
        assert!(err.to_string().contains("unavailable"));

        let err = SandboxError::SetupFailed {
            reason: "no privs".into(),
        };
        assert!(err.to_string().contains("setup failed"));
    }
}
