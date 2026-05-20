// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Single-page render: navigate, settle, extract serialized post-JS HTML.
//!
//! Every chromiumoxide op is wrapped in a bounded [`tokio::time::timeout`] so a
//! hung renderer never blocks the caller. The page is ALWAYS closed on exit —
//! success, error, and cancel — because chromiumoxide 0.9.1 does NOT auto-close
//! tabs on `Page` drop (each abandoned tab leaks renderer memory).

use std::time::Duration;

use chromiumoxide::{Browser, Page};
use tokio_util::sync::CancellationToken;

use crate::error::RenderError;

/// Bounded timeout for `Page::close`.
const PAGE_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

/// Per-render tunables. Defaults target SPA hydration (React/Vue/Next/Nuxt).
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Hard cap on `Page::goto` navigation.
    pub nav_timeout: Duration,
    /// Cap on the post-load network-settle wait (`Page::wait_for_navigation`).
    pub networkidle_timeout: Duration,
    /// Optional `User-Agent` override applied before navigation.
    pub user_agent: Option<String>,
    /// Optional `Accept-Language` value (informational in v0).
    pub accept_language: Option<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            nav_timeout: Duration::from_secs(30),
            networkidle_timeout: Duration::from_secs(10),
            user_agent: None,
            accept_language: None,
        }
    }
}

impl RenderOptions {
    /// The configured navigation timeout in whole milliseconds, saturating.
    ///
    /// Used to populate [`RenderError::NavTimeout`] when a navigation exceeds
    /// [`RenderOptions::nav_timeout`].
    #[must_use]
    pub fn nav_timeout_ms(&self) -> u64 {
        u64::try_from(self.nav_timeout.as_millis()).unwrap_or(u64::MAX)
    }
}

/// Render `url` to fully-serialized post-JS HTML.
///
/// Flow · open a tab (cancellable) → race navigation+settle+extract against
/// `token` → ALWAYS close the tab (bounded). `token` short-circuits to
/// [`RenderError::Cancelled`] before any further `.await`.
///
/// # Errors
///
/// - [`RenderError::Cancelled`] if `token` fires.
/// - [`RenderError::NewPage`] / [`RenderError::Navigation`] /
///   [`RenderError::NavTimeout`] / [`RenderError::Extract`] on chromiumoxide
///   failures.
pub async fn render_page(
    browser: &Browser,
    url: &str,
    opts: &RenderOptions,
    token: &CancellationToken,
) -> Result<String, RenderError> {
    // Phase 2 · open the tab (cancellable). `Page::close` consumes `self`, so
    // we own `page` and move it into close() on every exit path below.
    let page: Page = tokio::select! {
        biased;
        () = token.cancelled() => return Err(RenderError::Cancelled),
        p = browser.new_page("about:blank") =>
            p.map_err(|e| RenderError::NewPage { source: Box::new(e) })?,
    };

    // Phase 3 · navigate + settle + extract, raced against cancellation. The
    // borrowed `&page` future completes before the Phase 4 move into close().
    let result = tokio::select! {
        biased;
        () = token.cancelled() => Err(RenderError::Cancelled),
        r = goto_and_extract(&page, url, opts) => r,
    };

    // Phase 4 · ALWAYS close the tab, bounded. A failed close on an already
    // dead browser must not mask `result`.
    let _ = tokio::time::timeout(PAGE_CLOSE_TIMEOUT, page.close()).await;

    result
}

/// Navigate + settle + extract on a borrowed page. Split out so the caller's
/// `select!` can race it against cancellation while retaining `page` for the
/// mandatory close.
async fn goto_and_extract(
    page: &Page,
    url: &str,
    opts: &RenderOptions,
) -> Result<String, RenderError> {
    // Best-effort `User-Agent` override before navigation. A failure to set the
    // UA must not abort the render — we proceed with Chrome's default.
    if let Some(ua) = opts.user_agent.as_deref() {
        let _ = page.set_user_agent(ua).await;
    }

    // Bounded `Page::goto`. Outer = timeout, inner = chromiumoxide Result.
    match tokio::time::timeout(opts.nav_timeout, page.goto(url)).await {
        Err(_) => {
            return Err(RenderError::NavTimeout {
                elapsed_ms: opts.nav_timeout_ms(),
            })
        }
        Ok(Err(e)) => {
            return Err(RenderError::Navigation {
                source: Box::new(e),
            })
        }
        Ok(Ok(_)) => {}
    }

    // Bounded settle. A settle timeout is non-fatal — SPAs may never reach
    // full idle; we extract whatever is rendered so far.
    let _ = tokio::time::timeout(opts.networkidle_timeout, page.wait_for_navigation()).await;

    // chromiumoxide 0.9.1 · `Page::content()` -> Result<String>.
    page.content().await.map_err(|e| RenderError::Extract {
        source: Box::new(e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_targets_spa_timeouts() {
        let o = RenderOptions::default();
        assert_eq!(o.nav_timeout, Duration::from_secs(30));
        assert_eq!(o.networkidle_timeout, Duration::from_secs(10));
        assert!(o.user_agent.is_none());
        assert!(o.accept_language.is_none());
    }

    #[test]
    fn timeout_config_is_independent() {
        let o = RenderOptions {
            nav_timeout: Duration::from_secs(15),
            networkidle_timeout: Duration::from_secs(3),
            user_agent: Some("nika-render/0".to_string()),
            accept_language: Some("en-US".to_string()),
        };
        assert_eq!(o.nav_timeout, Duration::from_secs(15));
        assert_eq!(o.networkidle_timeout, Duration::from_secs(3));
        assert_eq!(o.user_agent.as_deref(), Some("nika-render/0"));
        assert_eq!(o.accept_language.as_deref(), Some("en-US"));
    }

    #[test]
    fn nav_timeout_ms_converts() {
        assert_eq!(RenderOptions::default().nav_timeout_ms(), 30_000);
    }

    // Integration stubs — require a live Chromium + fixture servers (gated).
    #[tokio::test]
    #[ignore = "integration · requires Chromium + React fixture server"]
    async fn renders_react_csr_app() {}

    #[tokio::test]
    #[ignore = "integration · requires Chromium + Vue fixture server"]
    async fn renders_vue_csr_app() {}

    #[tokio::test]
    #[ignore = "integration · requires Chromium + Next.js fixture server"]
    async fn renders_next_app() {}

    #[tokio::test]
    #[ignore = "integration · requires Chromium + Nuxt fixture server"]
    async fn renders_nuxt_app() {}
}
