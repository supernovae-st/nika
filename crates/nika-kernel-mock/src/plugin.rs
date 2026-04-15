// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NullWasmPluginHost` — stub host for tests.

use nika_kernel::plugin::{WasmPluginError, WasmPluginHost};

/// No-op WASM plugin host that always returns "not found".
///
/// Used when tests don't exercise WASM plugins but need a value for type
/// requirements. Every `call_plugin` returns `WasmPluginError::NotFound`.
#[derive(Clone, Debug, Default)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_host_returns_not_found() {
        let host = NullWasmPluginHost::new();
        let err = host.call_plugin("test-plugin", b"input").await.unwrap_err();
        assert!(matches!(err, WasmPluginError::NotFound { name } if name == "test-plugin"));
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn null_wasm_plugin_host_is_send_sync() {
        _assert_send_sync::<NullWasmPluginHost>();
    }
}
