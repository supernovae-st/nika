// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Null metrics exporter — discards all metrics.

use nika_kernel::metrics::{MetricTag, MetricsExporter};

/// No-op metrics exporter.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NullMetricsExporter;

impl NullMetricsExporter {
    /// Create a new null metrics exporter.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl MetricsExporter for NullMetricsExporter {
    fn counter(&self, _name: &str, _value: u64, _tags: &[MetricTag]) {}
    fn gauge(&self, _name: &str, _value: f64, _tags: &[MetricTag]) {}
    fn histogram(&self, _name: &str, _value: f64, _tags: &[MetricTag]) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_metrics_no_panic() {
        let m = NullMetricsExporter::new();
        m.counter("requests", 1, &[]);
        m.gauge("active", 5.0, &[("env".into(), "test".into())]);
        m.histogram("latency_ms", 42.0, &[]);
    }

    #[test]
    fn satisfies_metrics_exporter() {
        fn _accepts(_: &dyn MetricsExporter) {}
        _accepts(&NullMetricsExporter::new());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn null_metrics_is_send_sync() {
        _assert_send_sync::<NullMetricsExporter>();
    }
}
