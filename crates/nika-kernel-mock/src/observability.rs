// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NullObservabilitySink` — no-op sink for tests.

use nika_kernel::observability::{MetricEvent, ObservabilityError, ObservabilitySink, SpanEvent};

/// No-op observability sink that discards all events.
///
/// Used when tests don't exercise observability but need a value for type
/// requirements.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NullObservabilitySink;

impl NullObservabilitySink {
    /// Create a new null observability sink.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ObservabilitySink for NullObservabilitySink {
    async fn emit_span(&self, _span: SpanEvent) -> Result<(), ObservabilityError> {
        Ok(())
    }

    async fn record_metric(&self, _metric: MetricEvent) -> Result<(), ObservabilityError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_emit_span_returns_ok() {
        let sink = NullObservabilitySink::new();
        let span = SpanEvent::new("test", 100);
        assert!(sink.emit_span(span).await.is_ok());
    }

    #[tokio::test]
    async fn null_record_metric_returns_ok() {
        let sink = NullObservabilitySink::new();
        let metric = MetricEvent::new("counter", 1.0);
        assert!(sink.record_metric(metric).await.is_ok());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn null_observability_sink_is_send_sync() {
        _assert_send_sync::<NullObservabilitySink>();
    }
}
