// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Doctor's opt-in capability sweep; no provider key, inference or download.
use nika_providers::probe::{ModelListing, ProviderProbe};
use std::time::Duration;

/// The model list beside a local engine's endpoint. The seeds and most
/// overrides are the chat route itself (`…/v1/chat/completions`), whose
/// list is `…/v1/models` (#1626 · `…/chat/completions/models` is a 404);
/// a bare API root (`…/v1`) gains `/models`.
fn endpoint(base: &str) -> Option<String> {
    let mut url = url::Url::parse(base).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    let path = url.path().trim_end_matches('/');
    let root = path.strip_suffix("/chat/completions").unwrap_or(path);
    url.set_path(&format!("{root}/models"));
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

    /// #1626 · every local seed is the chat route itself, and the sweep
    /// asked `…/v1/chat/completions/models` (a 404 on every engine). The
    /// list is the route's sibling, `…/v1/models`.
    #[test]
    fn the_chat_route_lists_its_models_at_its_sibling() {
        assert_eq!(
            endpoint("http://127.0.0.1:8080/v1/chat/completions"),
            Some("http://127.0.0.1:8080/v1/models".into())
        );
        assert_eq!(
            endpoint("http://gpu.lan:11434/v1/chat/completions/"),
            Some("http://gpu.lan:11434/v1/models".into())
        );
    }

    /// A loopback engine for one probe: answers `body` as `content_type`
    /// whatever the path, and reports the request line it was sent.
    #[allow(clippy::disallowed_methods)] // test seam — the sweep's own worker pattern
    fn one_shot_engine(
        content_type: &'static str,
        body: &'static str,
    ) -> (u16, std::sync::mpsc::Receiver<String>) {
        use std::io::{BufRead as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = std::io::BufReader::new(stream);
            let mut request = String::new();
            let _ = reader.read_line(&mut request);
            let mut header = String::new();
            while reader.read_line(&mut header).is_ok_and(|n| n > 2) {
                header.clear();
            }
            let mut stream = reader.into_inner();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = tx.send(request.trim_end().to_owned());
        });
        (port, rx)
    }

    /// The `llamacpp` row a run would hit, pointed at the loopback engine.
    fn llamacpp_at(port: u16) -> Vec<ProviderProbe> {
        let config = nika_providers::ProvidersConfig::new().with_base_url(
            "llamacpp",
            format!("http://127.0.0.1:{port}/v1/chat/completions"),
        );
        nika_providers::probe::collect_access_probes_env(config)
            .into_iter()
            .filter(|p| p.id == "llamacpp")
            .collect()
    }

    /// #1626 · a web app on the engine's port (the Lab's HTML on `:8080`):
    /// the sweep asks the models route, and a 200 `text/html` is no list.
    #[test]
    fn an_html_answer_on_the_models_route_is_no_model_list() {
        let (port, asked) =
            one_shot_engine("text/html", "<!DOCTYPE html><html lang=\"fr\"></html>");
        let mut rows = llamacpp_at(port);
        collect(&mut rows);
        assert_eq!(
            asked.recv_timeout(Duration::from_secs(5)).as_deref(),
            Ok("GET /v1/models HTTP/1.1")
        );
        let listing = rows[0].readiness.model_listing.as_ref().expect("observed");
        assert_eq!(listing.http_status, Some(200));
        assert_eq!(listing.protocol_compatible, Some(false));
        assert_eq!(listing.available(), None);
    }

    /// The other direction: an OpenAI-shaped JSON list on the same route
    /// is compatible and names its models.
    #[test]
    fn a_json_model_list_on_the_models_route_is_compatible() {
        let (port, asked) = one_shot_engine(
            "application/json",
            r#"{"object":"list","data":[{"id":"fake-local-model","object":"model"}]}"#,
        );
        let mut rows = llamacpp_at(port);
        collect(&mut rows);
        assert_eq!(
            asked.recv_timeout(Duration::from_secs(5)).as_deref(),
            Ok("GET /v1/models HTTP/1.1")
        );
        let listing = rows[0].readiness.model_listing.as_ref().expect("observed");
        assert_eq!(listing.protocol_compatible, Some(true));
        assert_eq!(listing.models, ["fake-local-model"]);
        assert_eq!(rows[0].readiness.model_available, Some(true));
    }
}
