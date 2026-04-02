//! Prometheus metrics for nika serve.
//!
//! Exposes `GET /metrics` in Prometheus text format.
//!
//! ## Metrics
//!
//! - `nika_jobs_total{status}`: Counter of completed jobs by status
//! - `nika_jobs_active`: Gauge of currently active jobs
//! - `nika_job_duration_seconds`: Histogram of job durations
//! - `nika_http_requests_total{method,path,status}`: HTTP request counter

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Install the Prometheus recorder and return a handle for rendering metrics.
///
/// Must be called once at server startup (before any metrics are recorded).
/// Returns `None` if the recorder is already installed (e.g., in tests).
pub fn install_recorder() -> Option<PrometheusHandle> {
    PrometheusBuilder::new().install_recorder().ok()
}

/// `GET /metrics` — render all recorded metrics in Prometheus text format.
pub async fn metrics_handler(handle: Option<axum::extract::State<PrometheusHandle>>) -> Response {
    match handle {
        Some(axum::extract::State(h)) => {
            let output = h.render();
            (
                StatusCode::OK,
                [("content-type", "text/plain; version=0.0.4")],
                output,
            )
                .into_response()
        }
        None => (StatusCode::SERVICE_UNAVAILABLE, "metrics not initialized").into_response(),
    }
}

/// Record a completed job metric.
pub fn record_job_completed(status: &str, duration_secs: f64) {
    metrics::counter!("nika_jobs_total", "status" => status.to_string()).increment(1);
    metrics::histogram!("nika_job_duration_seconds").record(duration_secs);
}

/// Record the current number of active jobs.
pub fn record_active_jobs(count: usize) {
    metrics::gauge!("nika_jobs_active").set(count as f64);
}

/// Axum middleware that records HTTP request metrics.
pub async fn http_metrics_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let response = next.run(req).await;
    record_http_request(&method, &path, response.status().as_u16());
    response
}

/// Record an HTTP request.
pub fn record_http_request(method: &str, path: &str, status: u16) {
    metrics::counter!(
        "nika_http_requests_total",
        "method" => method.to_string(),
        "path" => path.to_string(),
        "status" => status.to_string(),
    )
    .increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_functions_dont_panic() {
        // These should not panic even without a recorder installed
        record_job_completed("completed", 1.5);
        record_active_jobs(3);
        record_http_request("GET", "/health", 200);
    }
}
