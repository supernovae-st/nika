// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Doctor's opt-in capability sweep; no provider key, inference or download.
use nika_providers::probe::{ModelListing, ProviderProbe};
use std::time::Duration;

fn endpoint(base: &str) -> Option<String> {
    let mut url = url::Url::parse(base).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    url.set_path(&format!("{}/models", url.path().trim_end_matches('/')));
    Some(url.to_string())
}

/// One thread owns the diagnostic async runtime, including when the caller is
/// already on a runtime. All local-profile requests share the same one-second
/// transport cap and start together; no cloud profile or media URL is queried.
#[allow(clippy::disallowed_methods)] // opt-in diagnostic composition, not workflow scheduling
pub(super) fn collect(rows: &mut [ProviderProbe]) {
    let targets: Vec<_> = rows
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.readiness.access == nika_types::access::AccessClass::Local && p.id != "mock"
        })
        .map(|(index, p)| (index, endpoint(&p.endpoint)))
        .collect();
    for (index, _) in &targets {
        rows[*index].readiness.model_available = None;
        rows[*index].readiness.model_listing =
            Some(ModelListing::new("model-list probe did not complete"));
    }
    let results = std::thread::spawn(move || sweep(targets)).join();
    if let Ok(results) = results {
        for (index, listing) in results {
            if let Some(row) = rows.get_mut(index) {
                row.readiness.model_available = listing.available();
                row.readiness.model_listing = Some(listing);
            }
        }
    }
}

fn sweep(targets: Vec<(usize, Option<String>)>) -> Vec<(usize, ModelListing)> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let Ok(runtime) = runtime else {
        return targets
            .into_iter()
            .map(|(i, _)| (i, ModelListing::new("model-list runtime unavailable")))
            .collect();
    };
    runtime.block_on(async {
        let mut cfg = nika_http::HttpConfig::new();
        cfg.retry_protocol_nacks = false;
        cfg.max_redirects = 0;
        cfg.max_response_bytes = 262_144;
        cfg.timeout = Duration::from_millis(1000);
        // The operator requested --ping against the effective local-protocol
        // endpoints. Loopback/private addresses are the purpose of this probe.
        cfg.ssrf = nika_http::SsrfMode::Disabled;
        let http = nika_http::ReqwestHttp::with_config(cfg);
        let mut pending = tokio::task::JoinSet::new();
        let mut out = Vec::new();
        for (index, endpoint) in targets {
            let (Ok(http), Some(endpoint)) = (&http, endpoint) else {
                out.push((
                    index,
                    ModelListing::new("model-list endpoint or transport unavailable"),
                ));
                continue;
            };
            let http = http.clone();
            pending.spawn(async move {
                (
                    index,
                    bounded(nika_providers::probe::probe_model_listing(&http, &endpoint)).await,
                )
            });
        }
        while let Some(result) = pending.join_next().await {
            if let Ok(row) = result {
                out.push(row);
            }
        }
        out
    })
}

async fn bounded(probe: impl std::future::Future<Output = ModelListing>) -> ModelListing {
    tokio::time::timeout(Duration::from_millis(1000), probe)
        .await
        .unwrap_or_else(|_| ModelListing::new("model-list transport deadline exceeded"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn the_total_deadline_also_bounds_a_transport_that_never_resolves() {
        let started = std::time::Instant::now();
        let got = bounded(std::future::pending()).await;
        assert_eq!(got.available(), None);
        assert_eq!(got.protocol_compatible, None);
        assert_eq!(got.http_status, None);
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn model_endpoint_retains_base_path_but_never_url_credentials_or_query() {
        assert_eq!(
            endpoint("http://user:secret@127.0.0.1:1234/v1/?token=secret#secret"),
            Some("http://127.0.0.1:1234/v1/models".into())
        );
        assert_eq!(
            endpoint("https://gpu.example/custom/v1"),
            Some("https://gpu.example/custom/v1/models".into())
        );
        for raw in ["file:///tmp/model", "not a url", "ftp://gpu.example/v1"] {
            assert_eq!(endpoint(raw), None);
        }
    }
}
