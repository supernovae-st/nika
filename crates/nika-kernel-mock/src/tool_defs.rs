// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `MockToolDefinitionProvider` — tool-definition test double.
//!
//! Implements the `Dyn` (Send-future) variant directly — the `MockShell`/
//! `MockToolExecutor` precedent: the consuming seams take the `Dyn` side.

use nika_kernel::ai::provider::ToolDef;
use nika_kernel::ai::tool_defs::{ToolDefinitionProviderDyn, ToolDefsError};

/// Programmable tool-definition source: a fixed universe or one error.
///
/// Build with [`MockToolDefinitionProvider::with_defs`] for the happy
/// path, [`MockToolDefinitionProvider::unavailable`] for the failure.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct MockToolDefinitionProvider {
    defs: Vec<ToolDef>,
    fail: Option<String>,
}

impl MockToolDefinitionProvider {
    /// An empty universe (the default-deny world).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A fixed tool universe.
    #[must_use]
    pub fn with_defs(defs: Vec<ToolDef>) -> Self {
        Self { defs, fail: None }
    }

    /// A source that fails every enumeration with `reason`.
    #[must_use]
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            defs: Vec::new(),
            fail: Some(reason.into()),
        }
    }
}

impl ToolDefinitionProviderDyn for MockToolDefinitionProvider {
    async fn tool_defs(&self) -> Result<Vec<ToolDef>, ToolDefsError> {
        match &self.fail {
            Some(reason) => Err(ToolDefsError::Unavailable {
                reason: reason.clone(),
            }),
            None => Ok(self.defs.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str) -> ToolDef {
        ToolDef::new(name, format!("{name} description"), serde_json::json!({}))
    }

    #[tokio::test]
    async fn with_defs_returns_the_universe() {
        let mock = MockToolDefinitionProvider::with_defs(vec![def("nika:read"), def("nika:write")]);
        let defs = mock.tool_defs().await.unwrap();
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0].name, "nika:read");
    }

    #[tokio::test]
    async fn empty_default_is_the_default_deny_world() {
        let mock = MockToolDefinitionProvider::new();
        assert!(mock.tool_defs().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn unavailable_fails_with_the_reason() {
        let mock = MockToolDefinitionProvider::unavailable("mcp down");
        let err = mock.tool_defs().await.unwrap_err();
        assert!(matches!(err, ToolDefsError::Unavailable { reason } if reason == "mcp down"));
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn mock_is_send_sync() {
        _assert_send_sync::<MockToolDefinitionProvider>();
    }
}
