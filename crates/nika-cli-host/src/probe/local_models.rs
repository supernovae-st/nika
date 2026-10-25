// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Doctor's opt-in capability sweep; no provider key, inference or download.
use nika_kernel::http::{HttpError, HttpGetDyn, HttpRequest, HttpResponse};
use nika_providers::probe::{ModelListing, ProviderProbe};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

/// Whether one model-route answer is an API answering (#1626). Only a 2xx
/// body that is not JSON at all (a web app's page on the engine's port)
/// is not; a 401, a 404 (`nika model serve` has no list route) and a JSON
/// body of any shape are an API, listed or not.
fn answers_as_api(status: u16, body: &[u8]) -> bool {
    !(200..300).contains(&status) || serde_json::from_slice::<serde_json::Value>(body).is_ok()
}

/// The sweep's transport, observed: it hands the listing probe the same
/// answer and records whether that answer was an API answering, so the
/// `--ping` row can tell a web page from an engine that lists no models.
struct Observed<'a, H> {
    inner: &'a H,
    /// `NO_ANSWER` until an HTTP answer arrives (a transport error or the
    /// deadline leaves it there), then `API` or `PAGE`.
    answer: AtomicU8,
}

const NO_ANSWER: u8 = 0;
const API: u8 = 1;
const PAGE: u8 = 2;

impl<H: HttpGetDyn> HttpGetDyn for Observed<'_, H> {
    async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let response = self.inner.get(request).await?;
        let kind = if answers_as_api(response.status, &response.body) {
            API
        } else {
            PAGE
        };
        self.answer.store(kind, Ordering::Relaxed);
        Ok(response)
    }
}

impl<H> Observed<'_, H> {
    /// The `reachable` rung this answer earns: `Some(true)` for an API
    /// answering, `Some(false)` for a page that is not one, `None` when
    /// no HTTP answer arrived (the connect verdict stands alone).
    fn reachable(&self) -> Option<bool> {
        match self.answer.load(Ordering::Relaxed) {
            API => Some(true),
            PAGE => Some(false),
            _ => None,
        }
    }
}

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
        rows[*index].readiness.reachable = None;
        rows[*index].readiness.model_available = None;
        rows[*index].readiness.model_listing =
            Some(ModelListing::new("model-list probe did not complete"));
    }
    let results = std::thread::spawn(move || sweep(targets)).join();
    if let Ok(results) = results {
        for (index, listing, reachable) in results {
            if let Some(row) = rows.get_mut(index) {
                // The rung `doctor --ping` fills (#1626): the engine's
                // route answered as an API, or a page sits on its port.
                row.readiness.reachable = reachable;
                row.readiness.model_available = listing.available();
                row.readiness.model_listing = Some(listing);
            }
        }
    }
}

fn sweep(targets: Vec<(usize, Option<String>)>) -> Vec<(usize, ModelListing, Option<bool>)> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let Ok(runtime) = runtime else {
        return targets
            .into_iter()
            .map(|(i, _)| (i, ModelListing::new("model-list runtime unavailable"), None))
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
                    None,
                ));
                continue;
            };
            let http = http.clone();
            pending.spawn(async move {
                let observed = Observed {
                    inner: &http,
                    answer: AtomicU8::new(NO_ANSWER),
                };
                let listing = bounded(nika_providers::probe::probe_model_listing(
                    &observed, &endpoint,
                ))
                .await;
                (index, listing, observed.reachable())
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

    /// A loopback engine for one probe: answers `status` with `body` as
    /// `content_type` whatever the path, and reports the request line it
    /// was sent.
    #[allow(clippy::disallowed_methods)] // test seam — the sweep's own worker pattern
    fn one_shot_engine(
        status: &'static str,
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
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = tx.send(request.trim_end().to_owned());
        });
        (port, rx)
    }

    /// A loopback engine that accepts and never answers (a slow engine, a
    /// model still loading): it holds the socket past the sweep's deadline.
    #[allow(clippy::disallowed_methods)] // test seam — the sweep's own worker pattern
    fn mute_engine() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                std::thread::sleep(Duration::from_secs(3));
                drop(stream);
            }
        });
        port
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

    /// The `llamacpp` row after one sweep against a one-shot engine; the
    /// engine was asked the models route, nothing else.
    fn swept(
        status: &'static str,
        content_type: &'static str,
        body: &'static str,
    ) -> ProviderProbe {
        let (port, asked) = one_shot_engine(status, content_type, body);
        let mut rows = llamacpp_at(port);
        collect(&mut rows);
        assert_eq!(
            asked.recv_timeout(Duration::from_secs(5)).as_deref(),
            Ok("GET /v1/models HTTP/1.1")
        );
        rows.remove(0)
    }

    /// #1626 · a web app on the engine's port (the Lab's HTML on `:8080`):
    /// the sweep asks the models route, a 200 `text/html` is no list, and
    /// it is the one answer that is not an API — the ping does not stand.
    #[test]
    fn an_html_answer_on_the_models_route_is_a_page_not_an_engine() {
        let row = swept(
            "200 OK",
            "text/html",
            "<!DOCTYPE html><html lang=\"fr\"></html>",
        );
        let listing = row.readiness.model_listing.as_ref().expect("observed");
        assert_eq!(listing.http_status, Some(200));
        assert_eq!(listing.protocol_compatible, Some(false));
        assert_eq!(listing.available(), None);
        assert_eq!(row.readiness.reachable, Some(false));
        assert!(!crate::probe::answers_as_engine(Some(&row)));
    }

    /// The other direction: an OpenAI-shaped JSON list on the same route
    /// is compatible, names its models, and the engine stands.
    #[test]
    fn a_json_model_list_on_the_models_route_is_compatible() {
        let row = swept(
            "200 OK",
            "application/json",
            r#"{"object":"list","data":[{"id":"fake-local-model","object":"model"}]}"#,
        );
        let listing = row.readiness.model_listing.as_ref().expect("observed");
        assert_eq!(listing.protocol_compatible, Some(true));
        assert_eq!(listing.models, ["fake-local-model"]);
        assert_eq!(row.readiness.model_available, Some(true));
        assert_eq!(row.readiness.reachable, Some(true));
        assert!(crate::probe::answers_as_engine(Some(&row)));
    }

    /// Review round 1 · the answers that are an API without a model list
    /// keep the engine standing: `nika model serve` has no list route
    /// (404), a key-protected vLLM or llama-server refuses an unkeyed list
    /// (401), Ollama with nothing pulled lists `data: null`, and a JSON
    /// body of another shape is still an API answering.
    #[test]
    fn an_api_without_a_model_list_still_stands_as_an_engine() {
        let cases = [
            (
                "404 Not Found",
                r#"{"error":{"type":"invalid_request_error","message":"unknown route"}}"#,
                Some(404),
                None,
            ),
            (
                "401 Unauthorized",
                r#"{"error":"missing bearer token"}"#,
                Some(401),
                None,
            ),
            (
                "200 OK",
                r#"{"object":"list","data":null}"#,
                Some(200),
                Some(false),
            ),
            ("200 OK", r#"{"status":"ok"}"#, Some(200), None),
        ];
        for (status, body, http_status, available) in cases {
            let row = swept(status, "application/json", body);
            let listing = row.readiness.model_listing.as_ref().expect("observed");
            assert_eq!(listing.http_status, http_status, "{status} {body}");
            assert_eq!(listing.available(), available, "{status} {body}");
            assert_eq!(row.readiness.reachable, Some(true), "{status} {body}");
            assert!(
                crate::probe::answers_as_engine(Some(&row)),
                "{status} {body}"
            );
        }
    }

    /// Review round 1 · an engine slower than the one-second deadline gives
    /// no HTTP answer at all: nothing is judged beyond the connect, so the
    /// connect verdict stands.
    #[test]
    fn a_timeout_judges_nothing_beyond_the_connect() {
        let mut rows = llamacpp_at(mute_engine());
        let started = std::time::Instant::now();
        collect(&mut rows);
        assert!(started.elapsed() < Duration::from_secs(3));
        let listing = rows[0].readiness.model_listing.as_ref().expect("observed");
        assert_eq!(listing.http_status, None);
        assert_eq!(listing.protocol_compatible, None);
        assert_eq!(rows[0].readiness.reachable, None);
        assert!(crate::probe::answers_as_engine(rows.first()));
    }

    #[test]
    fn only_a_2xx_body_that_is_not_json_is_not_an_api() {
        assert!(!answers_as_api(200, b"<!DOCTYPE html>"));
        assert!(!answers_as_api(200, b""));
        assert!(!answers_as_api(204, b"ok"));
        assert!(answers_as_api(200, b"{}"));
        assert!(answers_as_api(200, br#""text""#));
        for status in [301, 401, 403, 404, 500] {
            assert!(answers_as_api(status, b"<html>"), "{status}");
        }
    }
}
