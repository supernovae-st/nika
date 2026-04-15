// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Distributed tracing trait for W3C Trace Context.

use std::collections::BTreeMap;

use nika_error::id::{SpanId, TraceId};

/// RAII span guard — span ends when dropped.
#[non_exhaustive]
pub struct SpanGuard {
    /// Span ID for this span.
    pub span_id: SpanId,
    /// Trace ID this span belongs to.
    pub trace_id: TraceId,
}

impl SpanGuard {
    /// Create a new span guard.
    #[must_use]
    pub fn new(trace_id: TraceId, span_id: SpanId) -> Self {
        Self { span_id, trace_id }
    }
}

impl std::fmt::Debug for SpanGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpanGuard")
            .field("trace_id", &self.trace_id.to_string())
            .field("span_id", &self.span_id.to_string())
            .finish()
    }
}

/// Tracer provider for distributed tracing.
///
/// Production: `OpenTelemetry` SDK, Jaeger, etc.
/// Tests: `NullTracerProvider`.
pub trait TracerProvider: Send + Sync {
    /// Start a new span under the given trace.
    fn start_span(&self, trace_id: TraceId, name: &str) -> SpanGuard;

    /// Inject trace context into outgoing HTTP headers.
    fn inject(&self, headers: &mut BTreeMap<String, String>);

    /// Extract trace context from incoming HTTP headers.
    fn extract(&self, headers: &BTreeMap<String, String>) -> Option<TraceId>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_guard_new() {
        let guard = SpanGuard::new(TraceId::nil(), SpanId::nil());
        assert!(guard.trace_id.is_nil());
        assert!(guard.span_id.is_nil());
    }

    #[test]
    fn span_guard_debug() {
        let guard = SpanGuard::new(TraceId::nil(), SpanId::nil());
        let debug = format!("{guard:?}");
        assert!(debug.contains("SpanGuard"));
    }

    fn _assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn tracer_provider_is_send_sync() {
        _assert_send_sync::<dyn TracerProvider>();
    }
}
