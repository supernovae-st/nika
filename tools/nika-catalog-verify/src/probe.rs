// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Registry probes — small, focused async functions that check whether a
//! catalog entry is resolvable upstream.
//!
//! Every probe returns `Ok(())` on success and a [`ProbeError`] on
//! failure. The error variant captures **why** the probe failed so the
//! report can distinguish "network down" from "package deleted" from
//! "registry returned a 5xx".

use std::time::Duration;

use reqwest::{Client, Method, StatusCode};
use thiserror::Error;

/// Every probe timeout. 10s is plenty for an HTTP HEAD/GET against a
/// public registry; anything slower = network problem, not data drift.
pub(crate) const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Reason a probe failed. `#[non_exhaustive]` so future registry types
/// (crates.io, GitHub Packages) can add variants without breaking match.
#[derive(Debug, Error)]
#[non_exhaustive]
pub(crate) enum ProbeError {
    /// Package / endpoint confirmed absent (404 from authoritative source).
    #[error("not found upstream: {0}")]
    NotFound(String),

    /// Registry replied with a non-OK status that is not 404.
    #[error("registry returned {status}: {url}")]
    BadStatus {
        /// The URL probed.
        url: String,
        /// The observed HTTP status.
        status: StatusCode,
    },

    /// Network failure (DNS, TCP, TLS, timeout, unreachable, etc).
    #[error("network error probing {url}: {source}")]
    Network {
        /// The URL probed.
        url: String,
        /// Underlying reqwest error.
        #[source]
        source: reqwest::Error,
    },
}

/// Build a reqwest client with sane defaults for catalog verification.
pub(crate) fn build_client() -> Result<Client, reqwest::Error> {
    Client::builder()
        .timeout(PROBE_TIMEOUT)
        .user_agent(concat!(
            "nika-catalog-verify/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/supernovae-st/nika)",
        ))
        .build()
}

/// Probe an npm package. Hits `registry.npmjs.org/<pkg>/latest` — fast,
/// authoritative, returns 404 for deleted packages.
pub(crate) async fn probe_npm(client: &Client, package: &str) -> Result<(), ProbeError> {
    let url = format!(
        "https://registry.npmjs.org/{}/latest",
        urlencoded_package(package)
    );
    http_probe(client, Method::GET, &url).await
}

/// Probe a `PyPI` package. Hits `pypi.org/pypi/<pkg>/json`.
pub(crate) async fn probe_pypi(client: &Client, package: &str) -> Result<(), ProbeError> {
    let url = format!("https://pypi.org/pypi/{}/json", package.replace('/', "%2F"));
    http_probe(client, Method::GET, &url).await
}

/// Probe a crates.io package. Hits `crates.io/api/v1/crates/<pkg>`.
pub(crate) async fn probe_cargo(client: &Client, package: &str) -> Result<(), ProbeError> {
    let url = format!("https://crates.io/api/v1/crates/{package}");
    http_probe(client, Method::GET, &url).await
}

/// Probe an OCI image manifest. `identifier` is `<registry>/<image>` —
/// e.g. `ghcr.io/grafana/mcp-grafana`. Tag defaults to `latest`.
///
/// Note: many OCI registries require authentication for manifest reads.
/// A 401/403 here generally means "registry exists" and is not treated
/// as drift — only 404 and network errors are.
pub(crate) async fn probe_oci(client: &Client, identifier: &str) -> Result<(), ProbeError> {
    let (registry, image) = identifier
        .split_once('/')
        .ok_or_else(|| ProbeError::NotFound(format!("malformed OCI identifier {identifier:?}")))?;
    let url = format!("https://{registry}/v2/{image}/manifests/latest");
    match http_probe(client, Method::HEAD, &url).await {
        Ok(()) => Ok(()),
        // 401/403 = registry answered, likely auth gate — treat as present.
        Err(ProbeError::BadStatus { status, .. })
            if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN =>
        {
            Ok(())
        }
        other => other,
    }
}

/// Probe a remote MCP endpoint. Uses HEAD with a short timeout —
/// we only care that the URL answers, not the body.
pub(crate) async fn probe_remote(client: &Client, url: &str) -> Result<(), ProbeError> {
    http_probe(client, Method::HEAD, url).await
}

// ─── internals ──────────────────────────────────────────────────────────

/// Minimal URL-escape for npm package names. npm accepts `@scope/name`
/// unescaped in its JSON API, but percent-encoding the slash is also
/// valid and avoids quirks with scoped packages.
fn urlencoded_package(pkg: &str) -> String {
    if pkg.starts_with('@') {
        // @scope/name → @scope%2Fname
        pkg.replacen('/', "%2F", 1)
    } else {
        pkg.to_string()
    }
}

async fn http_probe(client: &Client, method: Method, url: &str) -> Result<(), ProbeError> {
    let resp = client
        .request(method, url)
        .send()
        .await
        .map_err(|source| ProbeError::Network {
            url: url.to_string(),
            source,
        })?;
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    if status == StatusCode::NOT_FOUND {
        return Err(ProbeError::NotFound(url.to_string()));
    }
    Err(ProbeError::BadStatus {
        url: url.to_string(),
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_npm_package_url_encodes_slash() {
        assert_eq!(
            urlencoded_package("@modelcontextprotocol/server-filesystem"),
            "@modelcontextprotocol%2Fserver-filesystem"
        );
    }

    #[test]
    fn plain_npm_package_passes_through() {
        assert_eq!(urlencoded_package("redis-mcp"), "redis-mcp");
    }

    #[test]
    fn probe_timeout_is_ten_seconds() {
        assert_eq!(PROBE_TIMEOUT, Duration::from_secs(10));
    }
}
