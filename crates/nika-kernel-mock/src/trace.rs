// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Null tracer provider — no-op spans.

use std::collections::BTreeMap;

use nika_error::id::{SpanId, TraceId};
use nika_kernel::trace::{SpanGuard, TracerProvider};

/// No-op tracer provider.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NullTracerProvider;

impl NullTracerProvider {
    /// Create a new null tracer provider.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl TracerProvider for NullTracerProvider {
    fn start_span(&self, trace_id: TraceId, _name: &str) -> SpanGuard {
        SpanGuard::new(trace_id, SpanId::nil())
    }

    fn inject(&self, _headers: &mut BTreeMap<String, String>) {}

    fn extract(&self, _headers: &BTreeMap<String, String>) -> Option<TraceId> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_span_returns_nil_span() {
        let tp = NullTracerProvider::new();
        let guard = tp.start_span(TraceId::nil(), "test-span");
        assert!(guard.span_id.is_nil());
    }

    #[test]
    fn inject_is_noop() {
        let tp = NullTracerProvider::new();
        let mut headers = BTreeMap::new();
        tp.inject(&mut headers);
        assert!(headers.is_empty());
    }

    #[test]
    fn extract_returns_none() {
        let tp = NullTracerProvider::new();
        assert!(tp.extract(&BTreeMap::new()).is_none());
    }

    #[test]
    fn satisfies_tracer_provider() {
        fn _accepts(_: &dyn TracerProvider) {}
        _accepts(&NullTracerProvider::new());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn null_tracer_is_send_sync() {
        _assert_send_sync::<NullTracerProvider>();
    }
}
