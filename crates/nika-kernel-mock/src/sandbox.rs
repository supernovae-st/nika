// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NullSandbox` — allow-all sandbox for tests.

use nika_kernel::sandbox::{Capability, Sandbox, SandboxError};

/// No-op sandbox that allows all capabilities.
///
/// Used when tests don't exercise sandboxing but need a value for type
/// requirements. Always grants every capability and enters without error.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NullSandbox;

impl NullSandbox {
    /// Create a new null sandbox.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Sandbox for NullSandbox {
    async fn check_capability(&self, _cap: &Capability) -> Result<bool, SandboxError> {
        Ok(true) // allow-all
    }

    async fn enter(&self) -> Result<(), SandboxError> {
        Ok(()) // no restrictions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_sandbox_allows_all_capabilities() {
        let sandbox = NullSandbox::new();
        let cap = Capability::fs_read("/etc/passwd");
        assert!(sandbox.check_capability(&cap).await.unwrap());

        let cap = Capability::network("evil.com", None);
        assert!(sandbox.check_capability(&cap).await.unwrap());

        let cap = Capability::ProcessSpawn;
        assert!(sandbox.check_capability(&cap).await.unwrap());
    }

    #[tokio::test]
    async fn null_sandbox_enter_succeeds() {
        let sandbox = NullSandbox::new();
        assert!(sandbox.enter().await.is_ok());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn null_sandbox_is_send_sync() {
        _assert_send_sync::<NullSandbox>();
    }
}
