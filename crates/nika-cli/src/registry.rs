// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The registry composition shim — `nika-registry-client` (L2) does the
//! resolve/verify/cache work over an injected http seam; THIS file is
//! the L4 half that constructs the production pieces (the
//! client · the blocking executor · the canonical cache root). The same
//! composer split as `verbs/run` vs `nika-runtime`.

pub use nika_registry_client::{RegistryError, Resolved, is_registry_ref};

use nika_registry_client::{MAX_INDEX_BYTES, RegistryClient, default_cache_root};

/// Resolve a registry ref over the real network into the canonical
/// cache (`~/.nika/registry/`), blocking the current thread — the
/// CLI-level seam that runs BEFORE any workflow is parsed.
///
/// # Errors
///
/// Every refusal in the trust chain: a ref that does not parse, a name
/// that resolves nowhere (`NIKA-REG-001`), an advisory withdrawal
/// (`NIKA-REG-002`), a digest mismatch (`NIKA-REG-003` — nothing is
/// written), a tampered cache record (`NIKA-REG-004`), a registry shape
/// this engine cannot vet (`NIKA-REG-005`), plus the honest offline /
/// transport / environment failures. Each message teaches its fix.
pub fn resolve_blocking(arg: &str) -> Result<Resolved, RegistryError> {
    let root = default_cache_root()?;
    let http = registry_http()?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| RegistryError::env(format!("cannot start the fetch runtime: {e}")))?;
    rt.block_on(RegistryClient::new(http, root).resolve(arg))
}

/// The registry fetch client: SSRF enforcement stays ON (public https
/// hosts only), transport capped well under attacker-sized bodies.
// `HttpConfig` is `#[non_exhaustive]` → field assignment, not a struct
// literal (the same idiom as the run composer).
#[allow(clippy::field_reassign_with_default)]
fn registry_http() -> Result<nika_http::ReqwestHttp, RegistryError> {
    let mut config = nika_http::HttpConfig::default();
    config.max_response_bytes = MAX_INDEX_BYTES as u64;
    nika_http::ReqwestHttp::with_config(config)
        .map_err(|e| RegistryError::env(format!("cannot initialize the fetch client: {e}")))
}
