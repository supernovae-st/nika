// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Distributed tracing trait for W3C Trace Context.
//!
//! Span hierarchy: `parent_span_id` threads parent→child links so
//! Olympus `/trace` and any OTLP exporter can reconstruct the call
//! graph without synthesising spans on the consumer side. `links`
//! carries W3C span-link tuples so fan-in (retry/batch/scatter-gather)
//! stays representable in trace export. See ADR-035 seed (Wave 4B-#1).

use std::collections::BTreeMap;

use nika_error::id::{SpanId, TraceId};

/// W3C span-link target — names a span in another trace that shares
/// causal lineage with the current span. Used for batch/fanout patterns
/// where one span "includes" several independent spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct SpanRef {
    /// Trace ID of the linked span.
    pub trace_id: TraceId,
    /// Span ID of the linked span.
    pub span_id: SpanId,
}

impl SpanRef {
    /// New span reference.
    #[must_use]
    pub const fn new(trace_id: TraceId, span_id: SpanId) -> Self {
        Self { trace_id, span_id }
    }
}

/// RAII span guard — span ends when dropped.
///
/// `parent_span_id` carries the W3C Trace Context parent link; `None`
/// marks a root span (trace entry point). `links` allows multi-parent
/// fan-in for batch processors per the OTLP `span.links` field.
#[non_exhaustive]
pub struct SpanGuard {
    /// Span ID for this span.
    pub span_id: SpanId,
    /// Trace ID this span belongs to.
    pub trace_id: TraceId,
    /// Parent span within the same trace. `None` = root span.
    /// Reserved at v0.81 for full W3C Trace Context (Wave 4B-#1).
    pub parent_span_id: Option<SpanId>,
    /// W3C span links (multi-parent causality for fan-in patterns).
    /// Reserved at v0.81; empty by default.
    pub links: Vec<SpanRef>,
}

impl SpanGuard {
    /// Create a new root span guard (no parent, no links).
    #[must_use]
    pub fn new(trace_id: TraceId, span_id: SpanId) -> Self {
        Self {
            span_id,
            trace_id,
            parent_span_id: None,
            links: Vec::new(),
        }
    }

    /// Create a child span guard under an explicit parent.
    #[must_use]
    pub fn child_of(trace_id: TraceId, span_id: SpanId, parent_span_id: SpanId) -> Self {
        Self {
            span_id,
            trace_id,
            parent_span_id: Some(parent_span_id),
            links: Vec::new(),
        }
    }

    /// Create a span guard with explicit links (fan-in from batch/retry).
    #[must_use]
    pub fn with_links(
        trace_id: TraceId,
        span_id: SpanId,
        parent_span_id: Option<SpanId>,
        links: Vec<SpanRef>,
    ) -> Self {
        Self {
            span_id,
            trace_id,
            parent_span_id,
            links,
        }
    }
}

impl std::fmt::Debug for SpanGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpanGuard")
            .field("trace_id", &self.trace_id.to_string())
            .field("span_id", &self.span_id.to_string())
            .field(
                "parent_span_id",
                &self
                    .parent_span_id
                    .as_ref()
                    .map(std::string::ToString::to_string),
            )
            .field("links", &self.links.len())
            .finish()
    }
}

/// Tracer provider for distributed tracing.
///
/// Production: `OpenTelemetry` SDK, Jaeger, etc.
/// Tests: `NullTracerProvider`.
pub trait TracerProvider: Send + Sync {
    /// Start a new span under the given trace.
    ///
    /// Prefer [`Self::start_child_span`] to thread parent linkage.
    /// Impls that don't care about hierarchy just implement this one
    /// and the default `start_child_span` backfills `parent_span_id`.
    fn start_span(&self, trace_id: TraceId, name: &str) -> SpanGuard;

    /// Start a span with an explicit parent span id.
    ///
    /// Default: calls `start_span` and sets `parent_span_id` on the
    /// returned guard. Impls that care about W3C span hierarchy at
    /// their SDK level should override to pass the parent through
    /// natively (some SDKs require the parent at span-start time to
    /// set sampling/baggage correctly).
    fn start_child_span(
        &self,
        trace_id: TraceId,
        parent_span_id: Option<SpanId>,
        name: &str,
    ) -> SpanGuard {
        let mut g = self.start_span(trace_id, name);
        g.parent_span_id = parent_span_id;
        g
    }

    /// Inject trace context into outgoing HTTP headers.
    fn inject(&self, headers: &mut BTreeMap<String, String>);

    /// Extract trace context from incoming HTTP headers.
    fn extract(&self, headers: &BTreeMap<String, String>) -> Option<TraceId>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_guard_new_is_root() {
        let guard = SpanGuard::new(TraceId::nil(), SpanId::nil());
        assert!(guard.trace_id.is_nil());
        assert!(guard.span_id.is_nil());
        assert!(guard.parent_span_id.is_none());
        assert!(guard.links.is_empty());
    }

    #[test]
    fn span_guard_child_of_sets_parent() {
        let parent = SpanId::nil();
        let guard = SpanGuard::child_of(TraceId::nil(), SpanId::nil(), parent);
        assert_eq!(guard.parent_span_id, Some(parent));
        assert!(guard.links.is_empty());
    }

    #[test]
    fn span_guard_with_links_populates_both() {
        let link = SpanRef::new(TraceId::nil(), SpanId::nil());
        let guard = SpanGuard::with_links(
            TraceId::nil(),
            SpanId::nil(),
            Some(SpanId::nil()),
            vec![link, link],
        );
        assert_eq!(guard.links.len(), 2);
        assert!(guard.parent_span_id.is_some());
    }

    #[test]
    fn span_guard_debug_surfaces_fields() {
        let guard = SpanGuard::child_of(TraceId::nil(), SpanId::nil(), SpanId::nil());
        let debug = format!("{guard:?}");
        assert!(debug.contains("SpanGuard"));
        assert!(debug.contains("parent_span_id"));
        assert!(debug.contains("links"));
    }

    #[test]
    fn span_ref_new_stores_both_ids() {
        let r = SpanRef::new(TraceId::nil(), SpanId::nil());
        assert!(r.trace_id.is_nil());
        assert!(r.span_id.is_nil());
    }

    /// Minimal `TracerProvider` that only implements `start_span` —
    /// proves the `start_child_span` default correctly fills the
    /// parent field from the fallback path.
    struct MinimalTracer;
    impl TracerProvider for MinimalTracer {
        fn start_span(&self, trace_id: TraceId, _name: &str) -> SpanGuard {
            SpanGuard::new(trace_id, SpanId::nil())
        }
        fn inject(&self, _headers: &mut BTreeMap<String, String>) {}
        fn extract(&self, _headers: &BTreeMap<String, String>) -> Option<TraceId> {
            None
        }
    }

    #[test]
    fn tracer_provider_start_child_span_default_preserves_parent() {
        let t = MinimalTracer;
        let parent = SpanId::nil();
        let guard = t.start_child_span(TraceId::nil(), Some(parent), "op");
        assert_eq!(guard.parent_span_id, Some(parent));
    }

    #[test]
    fn tracer_provider_start_child_span_default_root_when_none() {
        let t = MinimalTracer;
        let guard = t.start_child_span(TraceId::nil(), None, "op");
        assert!(guard.parent_span_id.is_none());
    }

    fn _assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn tracer_provider_is_send_sync() {
        _assert_send_sync::<dyn TracerProvider>();
    }

    #[test]
    fn span_ref_is_send_sync() {
        _assert_send_sync::<SpanRef>();
    }
}
