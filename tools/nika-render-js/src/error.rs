// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `RenderError` — JavaScript rendering error taxonomy.
//!
//! 9 canonical variants with `NIKA-CHRM-NNN` codes embedded in the
//! `#[error]` Display message. Cohérent with the NIKA-XXX canon for the
//! brouillon engine (`security.md` Nika Shield); Diamond OLY-XXX prefixes
//! stay disjoint (cross-flow D-2026-05-08-N1).
//!
//! Variant lifecycle ·
//! - `#[non_exhaustive]` MANDATORY · adding variants stays MINOR (no break)
//! - `error_code()` returns a stable grep-anchor (`NIKA-CHRM-001` … `-009`)
//! - `is_transient()` classifies retry-eligible vs structural failures
//! - `From<RenderError> for HttpError` maps onto the kernel trait surface

use nika_kernel::http::HttpError;

/// Boxed dynamic source error preserving the upstream chromiumoxide chain.
type Source = Box<dyn std::error::Error + Send + Sync>;

/// Errors emitted by the headless-Chrome HTTP client.
///
/// Every variant embeds its `NIKA-CHRM-NNN` code at the start of the
/// Display message so logs · traces · cockpit panels can grep-anchor.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RenderError {
    /// Failed to launch the headless Chrome process.
    ///
    /// Typically transient: system out of file descriptors, low memory,
    /// or transient OS resource contention. Retry with backoff is safe.
    #[error("NIKA-CHRM-001 · failed to launch headless Chrome")]
    Launch {
        /// Underlying chromiumoxide launch error.
        #[source]
        source: Source,
    },

    /// `BrowserConfig` could not be built from the supplied options.
    ///
    /// Structural: the configuration is invalid · caller must fix inputs.
    #[error("NIKA-CHRM-002 · invalid browser config: {detail}")]
    Config {
        /// Human-readable reason from the chromiumoxide config builder.
        detail: String,
    },

    /// `Browser::new_page` failed to allocate a tab.
    ///
    /// Transient: Chrome may be momentarily out of tab budget or the
    /// DevTools channel briefly congested.
    #[error("NIKA-CHRM-003 · failed to open a new page/tab")]
    NewPage {
        /// Underlying chromiumoxide new-page error.
        #[source]
        source: Source,
    },

    /// `Page::goto(url)` failed before the page loaded.
    ///
    /// Transient: wraps DNS failure · TLS handshake failure · navigation
    /// aborted. The `#[source]` chain preserves the upstream message.
    #[error("NIKA-CHRM-004 · navigation failed")]
    Navigation {
        /// Underlying chromiumoxide navigation error.
        #[source]
        source: Source,
    },

    /// Navigation did not complete within the per-request timeout.
    ///
    /// Transient: slow upstreams may succeed on a longer-timeout retry.
    #[error("NIKA-CHRM-005 · navigation timeout after {elapsed_ms}ms")]
    NavTimeout {
        /// Configured navigation timeout in milliseconds.
        elapsed_ms: u64,
    },

    /// `Page::content()` failed to serialize the rendered HTML.
    ///
    /// Structural: Chrome could not serialize the DOM (renderer crash,
    /// OOM, DevTools protocol error). Retrying unchanged is unlikely to help.
    #[error("NIKA-CHRM-006 · failed to extract page content")]
    Extract {
        /// Underlying chromiumoxide content-extraction error.
        #[source]
        source: Source,
    },

    /// Request was cancelled via the supplied `CancellationToken`.
    ///
    /// Structural: the caller asked us to stop · no retry semantics.
    #[error("NIKA-CHRM-007 · request cancelled before completion")]
    Cancelled,

    /// HTTP method not supported in v0.
    ///
    /// Structural: only `GET` is supported in Round 1. `POST` / `PUT` need
    /// explicit DevTools `Network` interception (Round 2+ · LOCK-031 gated).
    #[error("NIKA-CHRM-008 · HTTP method unsupported in v0: {method}")]
    UnsupportedMethod {
        /// The rejected HTTP method.
        method: String,
    },

    /// The concurrency semaphore was closed while waiting for a permit.
    ///
    /// Structural: the client is shutting down · do not retry.
    #[error("NIKA-CHRM-009 · render semaphore closed (client shutting down)")]
    SemaphoreClosed,
}

impl RenderError {
    /// Stable grep-anchor for logs · journal events · cockpit panels.
    ///
    /// MUST stay in sync with the `#[error]` Display prefix · the golden
    /// test below asserts the prefix parity for every variant.
    #[must_use]
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Launch { .. } => "NIKA-CHRM-001",
            Self::Config { .. } => "NIKA-CHRM-002",
            Self::NewPage { .. } => "NIKA-CHRM-003",
            Self::Navigation { .. } => "NIKA-CHRM-004",
            Self::NavTimeout { .. } => "NIKA-CHRM-005",
            Self::Extract { .. } => "NIKA-CHRM-006",
            Self::Cancelled => "NIKA-CHRM-007",
            Self::UnsupportedMethod { .. } => "NIKA-CHRM-008",
            Self::SemaphoreClosed => "NIKA-CHRM-009",
        }
    }

    /// `true` when the error is transient and retrying with backoff is the
    /// canonical mitigation. `false` for structural failures (caller must
    /// fix inputs · config · or stop).
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Launch { .. }
            | Self::NewPage { .. }
            | Self::Navigation { .. }
            | Self::NavTimeout { .. } => true,

            Self::Config { .. }
            | Self::Extract { .. }
            | Self::Cancelled
            | Self::UnsupportedMethod { .. }
            | Self::SemaphoreClosed => false,
        }
    }
}

/// Map `RenderError` onto the kernel `HttpError` trait surface.
///
/// Timeout maps to [`HttpError::Timeout`] · unsupported method to
/// [`HttpError::Unsupported`] · transient failures to [`HttpError::Connection`]
/// · everything else to [`HttpError::Other`]. Every mapped message embeds the
/// `NIKA-CHRM-NNN` code so callers dispatching through `dyn HttpClient` can
/// still grep the anchor.
impl From<RenderError> for HttpError {
    fn from(err: RenderError) -> Self {
        // Precompute the anchored reason before `err` is moved by the match.
        // (`error_code()` / `to_string()` borrow `&err`; both are released here.)
        let reason = format!("{} · {err}", err.error_code());

        // Exhaustive match (no wildcard) — `#[non_exhaustive]` only forces a
        // catch-all for downstream crates; within this crate we enumerate every
        // variant so adding one is a compile error here (canonical · forces the
        // mapping decision instead of silently falling through).
        match err {
            RenderError::NavTimeout { elapsed_ms } => HttpError::Timeout {
                duration_ms: elapsed_ms,
            },
            RenderError::UnsupportedMethod { method } => HttpError::Unsupported {
                feature: format!("chromium-render method={method}"),
            },
            // Transient failures → Connection (retry-eligible).
            RenderError::Launch { .. }
            | RenderError::NewPage { .. }
            | RenderError::Navigation { .. } => HttpError::Connection { reason },
            // Structural failures → Other.
            RenderError::Config { .. }
            | RenderError::Extract { .. }
            | RenderError::Cancelled
            | RenderError::SemaphoreClosed => HttpError::Other { reason },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_embeds_code_for_each_variant() {
        let cases: Vec<(RenderError, &str)> = vec![
            (
                RenderError::Launch {
                    source: "spawn fail".into(),
                },
                "NIKA-CHRM-001",
            ),
            (
                RenderError::Config {
                    detail: "bad flag".into(),
                },
                "NIKA-CHRM-002",
            ),
            (
                RenderError::NewPage {
                    source: "no tab".into(),
                },
                "NIKA-CHRM-003",
            ),
            (
                RenderError::Navigation {
                    source: "dns".into(),
                },
                "NIKA-CHRM-004",
            ),
            (
                RenderError::NavTimeout { elapsed_ms: 30_000 },
                "NIKA-CHRM-005",
            ),
            (
                RenderError::Extract {
                    source: "crash".into(),
                },
                "NIKA-CHRM-006",
            ),
            (RenderError::Cancelled, "NIKA-CHRM-007"),
            (
                RenderError::UnsupportedMethod {
                    method: "POST".into(),
                },
                "NIKA-CHRM-008",
            ),
            (RenderError::SemaphoreClosed, "NIKA-CHRM-009"),
        ];

        for (err, expected) in cases {
            assert_eq!(err.error_code(), expected, "error_code mismatch");
            assert!(
                err.to_string().starts_with(expected),
                "Display must start with {expected} · got `{err}`",
            );
        }
    }

    #[test]
    fn transient_classification_canonical() {
        assert!(RenderError::Launch { source: "x".into() }.is_transient());
        assert!(RenderError::NewPage { source: "x".into() }.is_transient());
        assert!(RenderError::Navigation { source: "x".into() }.is_transient());
        assert!(RenderError::NavTimeout { elapsed_ms: 1 }.is_transient());

        assert!(!RenderError::Config { detail: "x".into() }.is_transient());
        assert!(!RenderError::Extract { source: "x".into() }.is_transient());
        assert!(!RenderError::Cancelled.is_transient());
        assert!(!RenderError::UnsupportedMethod {
            method: "POST".into()
        }
        .is_transient());
        assert!(!RenderError::SemaphoreClosed.is_transient());
    }

    #[test]
    fn from_unsupported_method_maps_to_http_unsupported() {
        let http_err: HttpError = RenderError::UnsupportedMethod {
            method: "POST".into(),
        }
        .into();
        let dbg = format!("{http_err:?}");
        let HttpError::Unsupported { feature } = http_err else {
            panic!("expected Unsupported, got {dbg}");
        };
        assert!(feature.contains("chromium-render"));
        assert!(feature.contains("POST"));
    }

    #[test]
    fn from_nav_timeout_maps_to_http_timeout() {
        let http_err: HttpError = RenderError::NavTimeout { elapsed_ms: 30_000 }.into();
        assert!(matches!(
            http_err,
            HttpError::Timeout {
                duration_ms: 30_000
            }
        ));
    }

    #[test]
    fn from_transient_maps_to_http_connection_with_code() {
        let http_err: HttpError = RenderError::Launch {
            source: "oom".into(),
        }
        .into();
        let dbg = format!("{http_err:?}");
        let HttpError::Connection { reason } = http_err else {
            panic!("expected Connection, got {dbg}");
        };
        assert!(reason.contains("NIKA-CHRM-001"));
    }

    #[test]
    fn from_cancelled_maps_to_http_other_with_code() {
        let http_err: HttpError = RenderError::Cancelled.into();
        let dbg = format!("{http_err:?}");
        let HttpError::Other { reason } = http_err else {
            panic!("expected Other, got {dbg}");
        };
        assert!(reason.contains("NIKA-CHRM-007"));
    }

    #[test]
    fn from_semaphore_closed_maps_to_http_other() {
        let http_err: HttpError = RenderError::SemaphoreClosed.into();
        assert!(matches!(http_err, HttpError::Other { .. }));
    }

    #[test]
    fn error_source_chain_walks_through_navigation() {
        use std::error::Error;
        let inner = std::io::Error::other("connection refused");
        let err = RenderError::Navigation {
            source: Box::new(inner),
        };
        let src = err.source().expect("source should exist");
        assert!(src.to_string().contains("connection refused"));
    }

    #[test]
    fn debug_and_display_never_panic_for_any_variant() {
        let cases = vec![
            RenderError::Launch { source: "x".into() },
            RenderError::Config { detail: "x".into() },
            RenderError::NewPage { source: "x".into() },
            RenderError::Navigation { source: "x".into() },
            RenderError::NavTimeout { elapsed_ms: 0 },
            RenderError::Extract { source: "x".into() },
            RenderError::Cancelled,
            RenderError::UnsupportedMethod { method: "X".into() },
            RenderError::SemaphoreClosed,
        ];
        for err in cases {
            let _ = format!("{err:?}");
            let _ = format!("{err}");
        }
    }
}
