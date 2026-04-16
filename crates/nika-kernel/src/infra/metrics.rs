// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Metrics exporter trait for observability.
//!
//! Deliberately sync (not async) — metric emission must never block.
//! `OpenTelemetry` v0 merged metrics+traces; v1 split them — hard-learned lesson.

/// Tag for metric attribution (key, value).
pub type MetricTag = (String, String);

/// Exporter for counter, gauge, and histogram metrics.
///
/// Production: Prometheus, `OpenTelemetry`, `StatsD`, etc.
/// Tests: `NullMetricsExporter` (no-op).
pub trait MetricsExporter: Send + Sync {
    /// Increment a counter.
    fn counter(&self, name: &str, value: u64, tags: &[MetricTag]);
    /// Set a gauge value.
    fn gauge(&self, name: &str, value: f64, tags: &[MetricTag]);
    /// Record a histogram observation.
    fn histogram(&self, name: &str, value: f64, tags: &[MetricTag]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn metrics_exporter_is_send_sync() {
        _assert_send_sync::<dyn MetricsExporter>();
    }
}
