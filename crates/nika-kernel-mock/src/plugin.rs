// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NullWasmPluginHost` — stub host for tests.

use nika_kernel::plugin::{PluginEnv, WasmPluginError, WasmPluginHost, WasmPluginLifecycle};

/// No-op WASM plugin host that always returns "not found".
///
/// Used when tests don't exercise WASM plugins but need a value for type
/// requirements. Every `call_plugin` returns `WasmPluginError::NotFound`.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NullWasmPluginHost;

impl NullWasmPluginHost {
    /// Create a new null WASM plugin host.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl WasmPluginHost for NullWasmPluginHost {
    async fn call_plugin(
        &self,
        plugin_name: &str,
        _input: &[u8],
    ) -> Result<Vec<u8>, WasmPluginError> {
        Err(WasmPluginError::NotFound {
            name: plugin_name.to_owned(),
        })
    }
}

/// No-op WASM lifecycle that always returns "not found" or empty.
///
/// Stub for tests that don't exercise plugin lifecycle management.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NullWasmPluginLifecycle;

impl NullWasmPluginLifecycle {
    /// Create a new null lifecycle stub.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl WasmPluginLifecycle for NullWasmPluginLifecycle {
    async fn load_plugin(&self, name: &str, _bytes: &[u8]) -> Result<(), WasmPluginError> {
        Err(WasmPluginError::NotFound {
            name: name.to_owned(),
        })
    }

    async fn unload_plugin(&self, name: &str) -> Result<(), WasmPluginError> {
        Err(WasmPluginError::NotFound {
            name: name.to_owned(),
        })
    }

    async fn list_plugins(&self) -> Result<Vec<String>, WasmPluginError> {
        Ok(Vec::new())
    }
}

/// No-op environment access that always returns `None`.
///
/// Stub for tests that don't exercise plugin environment access.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NullPluginEnv;

impl NullPluginEnv {
    /// Create a new null plugin env.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl PluginEnv for NullPluginEnv {
    async fn env_get(&self, _key: &str) -> Result<Option<String>, WasmPluginError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_host_returns_not_found() {
        let host = NullWasmPluginHost::new();
        let err = host.call_plugin("test-plugin", b"input").await.unwrap_err();
        assert!(matches!(err, WasmPluginError::NotFound { name } if name == "test-plugin"));
    }

    #[tokio::test]
    async fn null_lifecycle_load_returns_not_found() {
        let lc = NullWasmPluginLifecycle::new();
        let err = lc.load_plugin("test", b"wasm").await.unwrap_err();
        assert!(matches!(err, WasmPluginError::NotFound { name } if name == "test"));
    }

    #[tokio::test]
    async fn null_lifecycle_list_returns_empty() {
        let lc = NullWasmPluginLifecycle::new();
        let list = lc.list_plugins().await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn null_plugin_env_returns_none() {
        let env = NullPluginEnv::new();
        let val = env.env_get("HOME").await.unwrap();
        assert!(val.is_none());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn null_wasm_plugin_host_is_send_sync() {
        _assert_send_sync::<NullWasmPluginHost>();
        _assert_send_sync::<NullWasmPluginLifecycle>();
        _assert_send_sync::<NullPluginEnv>();
    }
}
