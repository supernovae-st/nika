// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bounded transport backoff on a rate-limited or overloaded seat.
//!
//! A 429 (rate limited), a 503 (unavailable) or a 529 (Anthropic
//! `overloaded_error`) answers nothing and bills nothing: the seat asked
//! the caller to wait. Failing the task on the first one turned a busy
//! minute into a red run (gemini/gemini-2.5-flash under the product
//! matrix's parallel load · `NIKA-INFER-001 · rate limited (HTTP 429)` in
//! 200 ms), while the author's `retry:` is opt-in and silent by default.
//!
//! This is the layer-wide floor, the same on every wire: at most
//! [`MAX_RETRIES`] re-sends of the SAME request, `Retry-After` honoured
//! when the seat names a delay (bounded by [`MAX_RETRY_AFTER`] — a longer
//! wait is the human's decision, and the error prints the header), else
//! 1 s · 2 s · 4 s. Never on another 4xx (the request is wrong), never on
//! a quota exhaustion (`insufficient_quota` · waiting does not refill
//! credit), never on a connection drop or a 5xx other than 503 (the seat
//! may have sampled and billed — the author's `retry:` decides), never on
//! a schema failure (the verb's own repair loop). Every wait is recorded
//! in a [`TransportReport`] so the receipt says what happened, and the
//! sleep rides the kernel clock seam ([`Backoff`]) — a test injects a
//! recorder, production the system clock.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use nika_kernel::ai::provider::ProviderError;
use nika_kernel::clock::ClockDyn;

/// Re-sends after the first answer, at most (four round-trips in all).
pub const MAX_RETRIES: u32 = 3;

/// The longest `Retry-After` the layer waits on its own; past it the
/// error surfaces with the header for the human (or an authored `retry:`).
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

/// The first wait when the seat names no delay; doubles each retry.
const BASE_BACKOFF: Duration = Duration::from_secs(1);

/// What the transport did for one logical call: how many round-trips it
/// sent, how long it waited between them, and which statuses it waited on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct TransportReport {
    /// Round-trips sent (1 = the call answered first time).
    pub attempts: u32,
    /// Total backoff slept between round-trips.
    pub waited: Duration,
    /// The HTTP status of every answer that was retried, in order.
    pub statuses: Vec<u16>,
}

impl TransportReport {
    /// A report before any round-trip (INV-019 · `new()` on every
    /// `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether at least one round-trip was retried.
    #[must_use]
    pub fn retried(&self) -> bool {
        !self.statuses.is_empty()
    }

    /// Fold another logical call's report in (a verb that sends several
    /// round-trips per task sums them — the receipt reads the task total).
    pub fn absorb(&mut self, other: &Self) {
        self.attempts = self.attempts.saturating_add(other.attempts);
        self.waited = self.waited.saturating_add(other.waited);
        self.statuses.extend_from_slice(&other.statuses);
    }

    /// One line for a receipt, `None` when nothing was retried:
    /// `retried 2× on HTTP 429 · 429 (waited 3.0 s)`.
    #[must_use]
    pub fn summary(&self) -> Option<String> {
        if !self.retried() {
            return None;
        }
        let statuses = self
            .statuses
            .iter()
            .map(|s| format!("HTTP {s}"))
            .collect::<Vec<_>>()
            .join(" · ");
        Some(format!(
            "retried {}× on {statuses} (waited {:.1} s)",
            self.statuses.len(),
            self.waited.as_secs_f64()
        ))
    }
}

/// The sleep seam the backoff rides — object-safe over the kernel
/// [`ClockDyn`] so one registry can carry any clock behind one pointer.
pub trait Backoff: Send + Sync + fmt::Debug {
    /// Wait `duration` before the next round-trip.
    ///
    /// CANCEL SAFETY: cancel-safe — dropping the future abandons the wait;
    /// nothing was sent.
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// Any kernel clock as a [`Backoff`] (the production `SystemClock`, a
/// test's `MockClock`, the composer's declared clock).
#[derive(Debug, Clone)]
pub struct ClockBackoff<C>(pub Arc<C>);

impl<C: ClockDyn + fmt::Debug + 'static> Backoff for ClockBackoff<C> {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(self.0.sleep(duration))
    }
}

/// The production default: the system clock's `tokio::time::sleep`.
pub(crate) fn system_backoff() -> Arc<dyn Backoff> {
    Arc::new(ClockBackoff(Arc::new(nika_clock::SystemClock)))
}

/// The HTTP status a provider error carries, when it carries one.
pub(crate) fn status_of(err: &ProviderError) -> Option<u16> {
    match err {
        ProviderError::HttpResponse { details } => Some(details.status()),
        ProviderError::RateLimited { .. } => Some(429),
        ProviderError::Api { status, .. } => Some(*status),
        _ => None,
    }
}

/// How long to wait before re-sending, or `None` when the error is not
/// the seat asking for patience (or the patience is spent).
pub(crate) fn retry_delay(err: &ProviderError, retries_so_far: u32) -> Option<Duration> {
    if retries_so_far >= MAX_RETRIES {
        return None;
    }
    let named_delay = match err {
        // The sanitized wire error: exhausted credit reads non-transient.
        ProviderError::HttpResponse { details } if details.is_transient() => {
            details.retry_after_ms()
        }
        ProviderError::RateLimited { retry_after_ms } => *retry_after_ms,
        ProviderError::Api { .. } => None,
        _ => return None,
    };
    if !matches!(status_of(err), Some(429 | 503 | 529)) {
        return None;
    }
    match named_delay {
        Some(ms) => {
            let delay = Duration::from_millis(ms);
            (delay <= MAX_RETRY_AFTER).then_some(delay)
        }
        None => Some(BASE_BACKOFF.saturating_mul(1u32 << retries_so_far.min(16))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::ai::provider::ProviderHttpError;

    fn http(status: u16, code: Option<&str>, retry_after: Option<&str>) -> ProviderError {
        ProviderError::HttpResponse {
            details: ProviderHttpError::new(status, code, None, retry_after),
        }
    }

    #[test]
    fn a_bare_429_backs_off_one_two_four_then_stops() {
        let err = http(429, None, None);
        assert_eq!(retry_delay(&err, 0), Some(Duration::from_secs(1)));
        assert_eq!(retry_delay(&err, 1), Some(Duration::from_secs(2)));
        assert_eq!(retry_delay(&err, 2), Some(Duration::from_secs(4)));
        assert_eq!(retry_delay(&err, 3), None, "MAX_RETRIES spent");
        assert_eq!(retry_delay(&err, 99), None);
    }

    #[test]
    fn retry_after_is_honoured_up_to_the_cap() {
        assert_eq!(
            retry_delay(&http(429, None, Some("2")), 0),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            retry_delay(&http(429, None, Some("2.5")), 2),
            Some(Duration::from_millis(2500)),
            "the header wins over the exponential schedule"
        );
        assert_eq!(
            retry_delay(&http(429, None, Some("30")), 0),
            Some(MAX_RETRY_AFTER)
        );
        assert_eq!(
            retry_delay(&http(429, None, Some("45")), 0),
            None,
            "past the cap the wait is the human's decision"
        );
        // A date-form header carries no delay the layer can compute — the
        // exponential schedule stands.
        assert_eq!(
            retry_delay(&http(429, None, Some("Sun, 06 Nov 1994 08:49:37 GMT")), 0),
            Some(Duration::from_secs(1))
        );
    }

    #[test]
    fn overloaded_and_unavailable_back_off_the_rest_of_5xx_does_not() {
        assert!(retry_delay(&http(503, None, None), 0).is_some());
        assert!(retry_delay(&http(529, Some("overloaded_error"), None), 0).is_some());
        for status in [500, 502, 504, 520] {
            assert_eq!(
                retry_delay(&http(status, None, None), 0),
                None,
                "{status}: the seat may have sampled — the author's retry decides"
            );
        }
    }

    #[test]
    fn a_wrong_request_or_a_dead_key_is_never_retried() {
        for status in [400, 401, 403, 404, 408, 422] {
            assert_eq!(retry_delay(&http(status, None, None), 0), None, "{status}");
        }
    }

    #[test]
    fn exhausted_quota_is_terminal_even_with_a_retry_after() {
        for code in ["insufficient_quota", "credit_balance_exhausted"] {
            assert_eq!(
                retry_delay(&http(429, Some(code), Some("1")), 0),
                None,
                "{code}: waiting does not refill credit"
            );
        }
    }

    #[test]
    fn the_typed_variants_follow_the_same_table() {
        assert_eq!(
            retry_delay(
                &ProviderError::RateLimited {
                    retry_after_ms: Some(1500)
                },
                0
            ),
            Some(Duration::from_millis(1500))
        );
        assert_eq!(
            retry_delay(
                &ProviderError::RateLimited {
                    retry_after_ms: None
                },
                1
            ),
            Some(Duration::from_secs(2))
        );
        let api = |status| ProviderError::Api {
            status,
            message: "m".to_owned(),
        };
        assert!(retry_delay(&api(503), 0).is_some());
        assert_eq!(
            retry_delay(&api(408), 0),
            None,
            "a timeout keeps its verdict"
        );
        assert_eq!(retry_delay(&api(500), 0), None);
        assert_eq!(
            retry_delay(
                &ProviderError::Connection {
                    reason: "reset".to_owned()
                },
                0
            ),
            None,
            "a dropped connection may have been billed"
        );
        assert_eq!(
            retry_delay(
                &ProviderError::Other {
                    reason: "x".to_owned()
                },
                0
            ),
            None
        );
    }

    #[test]
    fn the_report_sums_and_summarizes() {
        let mut report = TransportReport::new();
        assert!(!report.retried());
        assert_eq!(report.summary(), None);
        report.attempts = 3;
        report.waited = Duration::from_millis(3000);
        report.statuses = vec![429, 429];
        let mut other = TransportReport::new();
        other.attempts = 1;
        report.absorb(&other);
        assert_eq!(report.attempts, 4);
        assert_eq!(
            report.summary().as_deref(),
            Some("retried 2× on HTTP 429 · HTTP 429 (waited 3.0 s)")
        );
    }

    #[test]
    fn status_of_reads_every_shape() {
        assert_eq!(status_of(&http(503, None, None)), Some(503));
        assert_eq!(
            status_of(&ProviderError::RateLimited {
                retry_after_ms: None
            }),
            Some(429)
        );
        assert_eq!(
            status_of(&ProviderError::Other {
                reason: "x".to_owned()
            }),
            None
        );
    }
}
