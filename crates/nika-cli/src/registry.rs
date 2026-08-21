// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! L4 registry compose: resolve/verify/cache over the L2 client.

pub use nika_registry_client::{RegistryError, Resolved, is_registry_ref};

use nika_registry_client::{MAX_INDEX_BYTES, RegistryClient, default_cache_root};

pub use nika_cli_host::repair::repair_target_for_path;
#[cfg(test)]
pub(crate) use nika_cli_host::repair::repair_target_for_path_under;

/// Resolve a `registry:` ref into the canonical cache.
///
/// # Errors
/// Trust-chain refusals (`NIKA-REG-*`) plus offline/transport failures.
pub fn resolve_blocking(arg: &str) -> Result<Resolved, RegistryError> {
    let root = default_cache_root()?;
    let http = registry_http()?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| RegistryError::env(format!("cannot start the fetch runtime: {e}")))?;
    rt.block_on(RegistryClient::new(http, root).resolve(arg))
}

/// SSRF-on fetch client; `HttpConfig` is non-exhaustive so fields are assigned.
#[allow(clippy::field_reassign_with_default)]
fn registry_http() -> Result<nika_http::ReqwestHttp, RegistryError> {
    let mut config = nika_http::HttpConfig::default();
    config.max_response_bytes = MAX_INDEX_BYTES as u64;
    nika_http::ReqwestHttp::with_config(config)
        .map_err(|e| RegistryError::env(format!("cannot initialize the fetch client: {e}")))
}
