// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sandbox traits — capability-based isolation for WASM + MCP.
//!
//! Reserved for v0.100. Real implementations will live in
//! `nika-sandbox-landlock` (Linux) and `nika-sandbox-seatbelt` (macOS).
//! See `docs/architecture/forward-compat-invariants.md` Pattern 1.

use std::fmt;

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
    ///
    /// CANCEL SAFETY: cancel-safe — pure policy lookup, read-only.
    async fn check_capability(&self, cap: &Capability) -> Result<bool, SandboxError>;

    /// Enter the sandbox (apply restrictions).
    ///
    /// CANCEL SAFETY: NOT cancel-safe. Partial application of seccomp
    /// filters, chroot, or namespace setup may leave the process in an
    /// inconsistent sandbox state. Impls MUST apply restrictions in a
    /// single atomic transition or tear down on error.
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

/// Structured reason why a capability check failed.
///
/// Replaces the prior free-form `String` which could leak allowlist
/// contents to untrusted callers (P1-4, rust-security). Variants describe
/// the *class* of denial, not the specific resource that was being
/// requested. Use [`Capability`] on the host side if per-request detail
/// is needed in logs — `DenialKind` itself is safe to surface to guests.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DenialKind {
    /// Filesystem read path not in the granted capability set.
    FsReadNotGranted,
    /// Filesystem write path not in the granted capability set.
    FsWriteNotGranted,
    /// Network host not on the allowlist.
    NetworkHostNotAllowlisted,
    /// Process spawning disabled in this sandbox.
    ProcessSpawnDisabled,
    /// Environment variable key not in the allowlist.
    EnvKeyNotAllowlisted,
    /// Capability denied for a reason that does not fit the other
    /// variants. Does NOT carry free-form text (oracle vector).
    Unknown,
}

impl fmt::Display for DenialKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::FsReadNotGranted => "filesystem read not granted",
            Self::FsWriteNotGranted => "filesystem write not granted",
            Self::NetworkHostNotAllowlisted => "network host not allowlisted",
            Self::ProcessSpawnDisabled => "process spawn disabled",
            Self::EnvKeyNotAllowlisted => "environment variable not allowlisted",
            Self::Unknown => "capability denied",
        };
        f.write_str(s)
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
    #[error("capability denied: {kind}")]
    CapabilityDenied {
        /// Structured denial class (no free-form text; see `DenialKind`).
        kind: DenialKind,
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
            kind: DenialKind::NetworkHostNotAllowlisted,
        };
        assert!(err.to_string().contains("capability denied"));
        assert!(err.to_string().contains("network host"));

        let err = SandboxError::Unavailable {
            reason: "platform".into(),
        };
        assert!(err.to_string().contains("unavailable"));

        let err = SandboxError::SetupFailed {
            reason: "no privs".into(),
        };
        assert!(err.to_string().contains("setup failed"));
    }

    #[test]
    fn denial_kind_display_is_stable() {
        assert_eq!(
            DenialKind::FsReadNotGranted.to_string(),
            "filesystem read not granted"
        );
        assert_eq!(
            DenialKind::FsWriteNotGranted.to_string(),
            "filesystem write not granted"
        );
        assert_eq!(
            DenialKind::NetworkHostNotAllowlisted.to_string(),
            "network host not allowlisted"
        );
        assert_eq!(
            DenialKind::ProcessSpawnDisabled.to_string(),
            "process spawn disabled"
        );
        assert_eq!(
            DenialKind::EnvKeyNotAllowlisted.to_string(),
            "environment variable not allowlisted"
        );
        assert_eq!(DenialKind::Unknown.to_string(), "capability denied");
    }

    #[test]
    fn denial_kind_is_send_sync() {
        _assert_send_sync::<DenialKind>();
    }
}
