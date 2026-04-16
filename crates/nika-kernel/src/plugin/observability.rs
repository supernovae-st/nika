// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Observability sink traits — unified before the v0.100 split.
//!
//! This is a single trait now. v0.100 will split into `MetricsExporter`
//! and `TracerProvider` as separate traits (ADR-034 plans this).
//! See `docs/architecture/forward-compat-invariants.md` Pattern 1.

use serde::{Deserialize, Serialize};

/// Unified observability sink.
///
/// Reserved for v0.95. Real implementations will live in
/// `nika-observability-otel` (OpenTelemetry) and `nika-observability-stderr`
/// (development fallback).
///
/// v0.100 splits this into `MetricsExporter` + `TracerProvider`.
#[trait_variant::make(ObservabilitySinkDyn: Send)]
pub trait ObservabilitySink: Send + Sync {
    /// Emit a span event.
    async fn emit_span(&self, span: SpanEvent) -> Result<(), ObservabilityError>;

    /// Record a metric value.
    async fn record_metric(&self, metric: MetricEvent) -> Result<(), ObservabilityError>;
}

/// A span event for tracing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SpanEvent {
    /// Span name (e.g., `"infer"`, `"tool_call"`).
    pub name: String,
    /// Duration in microseconds.
    pub duration_us: u64,
    /// Key-value attributes.
    pub attributes: Vec<(String, String)>,
}

impl SpanEvent {
    /// Create a new span event.
    #[must_use]
    pub fn new(name: impl Into<String>, duration_us: u64) -> Self {
        Self {
            name: name.into(),
            duration_us,
            attributes: Vec::new(),
        }
    }
}

/// A metric event for recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MetricEvent {
    /// Metric name (e.g., `"tokens_used"`, `"latency_ms"`).
    pub name: String,
    /// Metric value.
    pub value: f64,
    /// Key-value labels.
    pub labels: Vec<(String, String)>,
}

impl MetricEvent {
    /// Create a new metric event.
    #[must_use]
    pub fn new(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            value,
            labels: Vec::new(),
        }
    }
}

/// Observability errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ObservabilityError {
    /// Observability sink not configured.
    #[error("observability sink not configured: {reason}")]
    NotConfigured {
        /// Why the sink is not available.
        reason: String,
    },

    /// Export failed.
    #[error("observability export failed: {reason}")]
    ExportFailed {
        /// What went wrong during export.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn observability_types_are_send_sync() {
        _assert_send_sync::<SpanEvent>();
        _assert_send_sync::<MetricEvent>();
        _assert_send_sync::<ObservabilityError>();
    }

    #[test]
    fn span_event_new() {
        let span = SpanEvent::new("infer", 1234);
        assert_eq!(span.name, "infer");
        assert_eq!(span.duration_us, 1234);
        assert!(span.attributes.is_empty());
    }

    #[test]
    fn metric_event_new() {
        let metric = MetricEvent::new("tokens_used", 150.0);
        assert_eq!(metric.name, "tokens_used");
        assert!((metric.value - 150.0).abs() < f64::EPSILON);
        assert!(metric.labels.is_empty());
    }

    #[test]
    fn span_event_serde_roundtrip() {
        let span = SpanEvent::new("test", 500);
        let json = serde_json::to_string(&span).expect("serialize");
        let back: SpanEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, "test");
        assert_eq!(back.duration_us, 500);
    }

    #[test]
    fn metric_event_serde_roundtrip() {
        let metric = MetricEvent::new("latency_ms", 42.5);
        let json = serde_json::to_string(&metric).expect("serialize");
        let back: MetricEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, "latency_ms");
        assert!((back.value - 42.5).abs() < f64::EPSILON);
    }

    #[test]
    fn observability_error_display() {
        let err = ObservabilityError::NotConfigured {
            reason: "no sink".into(),
        };
        assert!(err.to_string().contains("not configured"));

        let err = ObservabilityError::ExportFailed {
            reason: "network down".into(),
        };
        assert!(err.to_string().contains("export failed"));
    }
}
