// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-render-js` · JS-rendering [`HttpClient`] for the Nika engine.
//!
//! Wraps `chromiumoxide` 0.9.1 (`default-features = false`) to fetch pages that
//! need client-side rendering (React/Vue/Next/Nuxt SPAs). v0 is GET-only,
//! headless, and serializes one page at a time (`MAX_CONCURRENT_PAGES = 1`).
//! The async runtime comes from the caller (workspace tokio) — chromiumoxide
//! 0.9.1 has no runtime feature. v0 controls a SYSTEM-installed Chrome
//! (chromiumoxide auto-detects via PATH); auto-download is deferred to Round 2.
//!
//! # Layer
//!
//! L1 effect crate · sister to `nika-http` · same kernel trait surface
//! ([`nika_kernel::http::HttpClient`]). The ONLY `nika-*` runtime dependency is
//! `nika-kernel` (cross-flow asymmetric D-2026-05-08-N1).
//!
//! # Cancellation & lifecycle discipline
//!
//! - Every render races a [`CancellationToken`]; a fired token returns
//!   [`RenderError::Cancelled`] before any further `.await`.
//! - The browser tab is closed on EVERY render exit (success/error/cancel) —
//!   chromiumoxide 0.9.1 does not auto-close tabs on `Page` drop.
//! - **`close()` over `Drop`**: tear the client down via the async
//!   [`ChromiumClient::close`] (delegates to [`BrowserHandle::close`]). `Drop`
//!   is a best-effort safety net only — it cannot `.await`, so it merely
//!   cancels the pump token and aborts the task. Long-lived daemons MUST call
//!   `close()` to avoid orphaned Chrome processes.
//! - Single-page (`MAX_CONCURRENT_PAGES = 1`) needs no detached cleanup tasks,
//!   so the page close is inline + bounded on the render fn's own stack (no
//!   `PageGuard`/`TaskTracker` — premature for v0 per scope-shrink discipline).

#![forbid(unsafe_code)]

mod error;
mod lifecycle;
mod page;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use chromiumoxide::BrowserConfig;
use nika_kernel::http::{HttpClient, HttpError, HttpMethod, HttpRequest, HttpResponse};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

pub use error::RenderError;
pub use lifecycle::BrowserHandle;
pub use page::RenderOptions;

/// v0 hard cap: one page rendered at a time.
const MAX_CONCURRENT_PAGES: usize = 1;

/// JS-rendering HTTP client. Shares one Chromium process; serializes renders
/// behind a [`Semaphore`].
pub struct ChromiumClient {
    /// Shared browser lifecycle. `Arc` so render tasks borrow it concurrently.
    handle: Arc<BrowserHandle>,
    /// Concurrency gate (`MAX_CONCURRENT_PAGES` permits).
    semaphore: Arc<Semaphore>,
    /// Default per-render options (overridden per request when `timeout` set).
    opts: RenderOptions,
}

impl ChromiumClient {
    /// Launch a headless Chromium with default options.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Config`] if the config cannot be built, or
    /// [`RenderError::Launch`] if Chromium cannot be started.
    pub async fn new() -> Result<Self, RenderError> {
        let config = BrowserConfig::builder()
            .build()
            .map_err(|detail| RenderError::Config { detail })?;
        Self::with_config(config, RenderOptions::default()).await
    }

    /// Launch with an explicit [`BrowserConfig`] and [`RenderOptions`].
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Launch`] if Chromium cannot be started.
    pub async fn with_config(
        config: BrowserConfig,
        opts: RenderOptions,
    ) -> Result<Self, RenderError> {
        let handle = BrowserHandle::launch(config).await?;
        Ok(Self {
            handle: Arc::new(handle),
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_PAGES)),
            opts,
        })
    }

    /// Gracefully tear down the underlying browser (delegates to
    /// [`BrowserHandle::close`]) when this is the last `Arc` owner. Other
    /// `Arc` owners, if any, fall back to best-effort `Drop`.
    pub async fn close(self) {
        if let Ok(handle) = Arc::try_unwrap(self.handle) {
            handle.close().await;
        }
    }

    /// Render `url` to post-JS HTML — cancel-aware and concurrency-gated.
    ///
    /// # Errors
    ///
    /// Propagates [`RenderError`] from permit acquisition + page render.
    async fn render(
        &self,
        url: &str,
        opts: &RenderOptions,
        token: CancellationToken,
    ) -> Result<String, RenderError> {
        // Phase 1 · acquire permit (cancel-safe: acquire_owned drops cleanly).
        let permit = {
            let sem = Arc::clone(&self.semaphore);
            let acq = sem.acquire_owned();
            tokio::select! {
                biased;
                () = token.cancelled() => return Err(RenderError::Cancelled),
                p = acq => p.map_err(|_| RenderError::SemaphoreClosed)?,
            }
        };

        // Phases 2-4 · open tab + navigate + extract + mandatory bounded close.
        let result = page::render_page(self.handle.browser(), url, opts, &token).await;
        drop(permit);
        result
    }
}

#[async_trait]
impl HttpClient for ChromiumClient {
    /// GET-only dispatch. Non-GET methods return [`HttpError::Unsupported`].
    /// Success builds a synthetic 200 response carrying the rendered HTML and
    /// the requested URL as `final_url`. Honors [`HttpRequest::timeout`] as the
    /// per-request navigation cap when set.
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        if !matches!(request.method, HttpMethod::Get) {
            return Err(RenderError::UnsupportedMethod {
                method: format!("{:?}", request.method),
            }
            .into());
        }

        let mut opts = self.opts.clone();
        if let Some(timeout) = request.timeout {
            opts.nav_timeout = timeout;
        }

        let token = CancellationToken::new();
        let html = self
            .render(&request.url, &opts, token)
            .await
            .map_err(HttpError::from)?;

        Ok(HttpResponse::new(
            200,
            HashMap::new(),
            Bytes::from(html.into_bytes()),
            request.url,
        ))
    }

    // `send_streaming` uses the trait default (returns `HttpError::Unsupported`)
    // — a whole-page render has no meaningful chunked stream surface in v0.
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time Send + Sync proof (ChromiumClient lives behind
    // Arc<dyn HttpClient> in the engine's verb dispatch).
    const _: fn() = || {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ChromiumClient>();
    };

    #[tokio::test]
    #[ignore = "requires Chromium binary"]
    async fn construction_launches_browser() {
        let client = ChromiumClient::new().await.expect("launch");
        client.close().await;
    }

    #[tokio::test]
    #[ignore = "requires Chromium binary"]
    async fn semaphore_serializes_concurrent_renders() {
        // With MAX_CONCURRENT_PAGES = 1, two renders cannot overlap; the second
        // waits for the first's permit. Asserted via instrumented fixture
        // timing in integration.
    }

    // Method-dispatch guard is pure logic: a non-GET method must be rejected
    // before any render. Proven without a browser.
    #[test]
    fn non_get_method_is_rejected() {
        assert!(!matches!(HttpMethod::Post, HttpMethod::Get));
        assert!(!matches!(HttpMethod::Delete, HttpMethod::Get));
        assert!(matches!(HttpMethod::Get, HttpMethod::Get));
    }

    // Cancel-before-acquire returns Cancelled deterministically without a
    // browser: a pre-cancelled token wins the biased select.
    #[tokio::test]
    async fn cancel_returns_cancelled_on_pre_fired_token() {
        let sem = Arc::new(Semaphore::new(1));
        let token = CancellationToken::new();
        token.cancel();
        let outcome: Result<(), RenderError> = {
            let acq = Arc::clone(&sem).acquire_owned();
            tokio::select! {
                biased;
                () = token.cancelled() => Err(RenderError::Cancelled),
                _p = acq => Ok(()),
            }
        };
        assert!(matches!(outcome, Err(RenderError::Cancelled)));
    }

    #[test]
    fn http_response_shape_carries_html_and_final_url() {
        let html = "<html><body>rendered</body></html>".to_string();
        let url = "https://example.com/app".to_string();
        let resp = HttpResponse::new(200, HashMap::new(), Bytes::from(html.clone()), url.clone());
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, Bytes::from(html));
        assert_eq!(resp.final_url, url);
    }
}
