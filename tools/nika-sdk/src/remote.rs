//! Remote transport — HTTP/SSE client for `nika serve`.
//!
//! Gated behind `#[cfg(feature = "remote")]`.
//! Uses reqwest for HTTP and streams SSE events from `/v1/events/{id}`.

#![cfg(feature = "remote")]

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use tracing::debug;

use crate::error::SdkError;
use crate::sse;
use crate::transport::Transport;
use crate::types::{
    ArtifactInfo, ArtifactListResponse, EventStream, JobInfo, RunRequest, RunResponse,
};

pub(crate) struct RemoteTransport {
    client: reqwest::Client,
    base_url: String,
}

impl RemoteTransport {
    pub fn new(url: impl Into<String>, token: impl Into<String>) -> Result<Self, SdkError> {
        let base_url = url.into().trim_end_matches('/').to_string();
        let token = token.into();

        // Validate URL
        if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
            return Err(SdkError::InvalidUrl(format!(
                "URL must start with http:// or https://, got: {base_url}"
            )));
        }

        let mut headers = HeaderMap::new();
        let auth_value = format!("Bearer {token}");
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value)
                .map_err(|_| SdkError::InvalidUrl("invalid token characters".into()))?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| SdkError::Connection(e.to_string()))?;

        Ok(Self { client, base_url })
    }
}

/// Validate a path segment is safe for URL interpolation (no traversal).
fn validate_path_segment(value: &str, label: &str) -> Result<(), SdkError> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
    {
        return Err(SdkError::InvalidUrl(format!(
            "unsafe {label}: {value:?}"
        )));
    }
    Ok(())
}

#[async_trait]
impl Transport for RemoteTransport {
    async fn submit(&self, req: &RunRequest) -> Result<String, SdkError> {
        let url = format!("{}/v1/run", self.base_url);
        let resp = self.client.post(&url).json(req).send().await.map_err(map_reqwest_error)?;

        let status = resp.status();
        if status.is_success() {
            let body: RunResponse = resp.json().await.map_err(map_reqwest_error)?;
            Ok(body.job_id)
        } else {
            Err(map_http_error(status.as_u16(), resp).await)
        }
    }

    async fn status(&self, job_id: &str) -> Result<JobInfo, SdkError> {
        validate_path_segment(job_id, "job_id")?;
        let url = format!("{}/v1/status/{job_id}", self.base_url);
        let resp = self.client.get(&url).send().await.map_err(map_reqwest_error)?;

        let status = resp.status();
        if status.is_success() {
            resp.json().await.map_err(map_reqwest_error)
        } else {
            Err(map_http_error(status.as_u16(), resp).await)
        }
    }

    async fn cancel(&self, job_id: &str) -> Result<JobInfo, SdkError> {
        validate_path_segment(job_id, "job_id")?;
        let url = format!("{}/v1/cancel/{job_id}", self.base_url);
        let resp = self.client.post(&url).send().await.map_err(map_reqwest_error)?;

        let status = resp.status();
        if status.is_success() {
            resp.json().await.map_err(map_reqwest_error)
        } else {
            Err(map_http_error(status.as_u16(), resp).await)
        }
    }

    async fn events(&self, job_id: &str) -> Result<EventStream, SdkError> {
        validate_path_segment(job_id, "job_id")?;
        let url = format!("{}/v1/events/{job_id}", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("Accept", "text/event-stream")
            .timeout(std::time::Duration::from_secs(3600)) // SSE streams are long-lived
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = resp.status();
        if !status.is_success() {
            return Err(map_http_error(status.as_u16(), resp).await);
        }

        let byte_stream = resp.bytes_stream();

        let event_stream = async_stream::stream! {
            let mut buffer = String::new();
            let mut byte_stream = std::pin::pin!(byte_stream);
            const MAX_BUFFER: usize = 1024 * 1024; // 1 MiB

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        yield Err(SdkError::Connection(e.to_string()));
                        return;
                    }
                };

                let text = match std::str::from_utf8(&chunk) {
                    Ok(t) => t,
                    Err(e) => {
                        yield Err(SdkError::EventParse(e.to_string()));
                        return;
                    }
                };

                buffer.push_str(text);

                // Guard against unbounded buffer growth from malformed streams
                if buffer.len() > MAX_BUFFER {
                    yield Err(SdkError::EventParse(
                        format!("SSE buffer exceeded {} bytes", MAX_BUFFER),
                    ));
                    return;
                }

                // Process complete SSE frames (delimited by \n\n or \r\n\r\n)
                while let Some((pos, delim_len)) = find_frame_boundary(&buffer) {
                    let frame_text = buffer[..pos + delim_len].to_string();
                    buffer = buffer[pos + delim_len..].to_string();

                    let frames = sse::parse_sse_block(&frame_text);
                    for frame in frames {
                        match frame {
                            sse::SseFrame::Comment(_) => {
                                debug!("SSE keepalive");
                            }
                            sse::SseFrame::Event { data, .. } => {
                                match sse::parse_event(&data) {
                                    Ok(event) => {
                                        let terminal = event.is_terminal();
                                        yield Ok(event);
                                        if terminal {
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        yield Err(e);
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Stream ended without terminal event
            yield Err(SdkError::StreamClosed);
        };

        Ok(Box::pin(event_stream))
    }

    async fn list_artifacts(&self, job_id: &str) -> Result<Vec<ArtifactInfo>, SdkError> {
        validate_path_segment(job_id, "job_id")?;
        let url = format!("{}/v1/jobs/{job_id}/artifacts", self.base_url);
        let resp = self.client.get(&url).send().await.map_err(map_reqwest_error)?;

        let status = resp.status();
        if status.is_success() {
            let body: ArtifactListResponse = resp.json().await.map_err(map_reqwest_error)?;
            Ok(body.artifacts)
        } else {
            Err(map_http_error(status.as_u16(), resp).await)
        }
    }

    async fn download_artifact(&self, job_id: &str, name: &str) -> Result<Bytes, SdkError> {
        validate_path_segment(job_id, "job_id")?;
        validate_path_segment(name, "artifact name")?;
        let url = format!("{}/v1/jobs/{job_id}/artifacts/{name}", self.base_url);
        let resp = self.client.get(&url).send().await.map_err(map_reqwest_error)?;

        let status = resp.status();
        if status.is_success() {
            resp.bytes().await.map_err(map_reqwest_error)
        } else {
            Err(map_http_error(status.as_u16(), resp).await)
        }
    }

    async fn health(&self) -> Result<bool, SdkError> {
        let url = format!("{}/health", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) if e.is_connect() || e.is_timeout() => Ok(false),
            Err(e) => Err(map_reqwest_error(e)),
        }
    }
}

/// Find the next SSE frame boundary in the buffer.
///
/// Returns `(position, delimiter_length)` for either `\n\n` (2) or `\r\n\r\n` (4).
fn find_frame_boundary(buf: &str) -> Option<(usize, usize)> {
    // Check \r\n\r\n first (longer delimiter, so it won't be partially matched by \n\n)
    if let Some(pos) = buf.find("\r\n\r\n") {
        // Only prefer CRLF if it comes before or at the same position as LF
        if let Some(lf_pos) = buf.find("\n\n") {
            if lf_pos <= pos {
                return Some((lf_pos, 2));
            }
        }
        return Some((pos, 4));
    }
    buf.find("\n\n").map(|pos| (pos, 2))
}

/// Map reqwest errors to SdkError with appropriate variants.
fn map_reqwest_error(e: reqwest::Error) -> SdkError {
    if e.is_timeout() {
        // Duration not available from reqwest error — report what we know
        SdkError::Timeout(
            e.url()
                .and_then(|u| {
                    if u.path().contains("/events/") {
                        Some(std::time::Duration::from_secs(3600))
                    } else {
                        Some(std::time::Duration::from_secs(30))
                    }
                })
                .unwrap_or(std::time::Duration::from_secs(30)),
        )
    } else if e.is_connect() {
        SdkError::Connection(e.to_string())
    } else {
        SdkError::Http {
            status: e.status().map(|s| s.as_u16()).unwrap_or(0),
            message: e.to_string(),
            body: None,
        }
    }
}

/// Extract error info from HTTP response body.
async fn map_http_error(status: u16, resp: reqwest::Response) -> SdkError {
    let body = resp.text().await.ok();
    let message = body
        .as_deref()
        .and_then(|b| serde_json::from_str::<serde_json::Value>(b).ok())
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| format!("HTTP {status}"));

    match status {
        401 => SdkError::Unauthorized,
        404 => SdkError::NotFound(message),
        429 => SdkError::QueueFull,
        _ => SdkError::Http {
            status,
            message,
            body,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_url() {
        let result = RemoteTransport::new("ftp://localhost:3000", "tok");
        assert!(matches!(result, Err(SdkError::InvalidUrl(_))));
    }

    #[test]
    fn accepts_http_url() {
        let result = RemoteTransport::new("http://localhost:3000", "tok");
        assert!(result.is_ok());
    }

    #[test]
    fn accepts_https_url() {
        let result = RemoteTransport::new("https://api.example.com", "tok");
        assert!(result.is_ok());
    }

    #[test]
    fn strips_trailing_slash() {
        let transport = RemoteTransport::new("http://localhost:3000/", "tok").unwrap();
        assert_eq!(transport.base_url, "http://localhost:3000");
    }

    #[test]
    fn path_traversal_rejected() {
        assert!(validate_path_segment("../etc/passwd", "test").is_err());
        assert!(validate_path_segment("foo/bar", "test").is_err());
        assert!(validate_path_segment("foo\\bar", "test").is_err());
        assert!(validate_path_segment("", "test").is_err());
        assert!(validate_path_segment("..foo", "test").is_err());
    }

    #[test]
    fn safe_path_segments_accepted() {
        assert!(validate_path_segment("job-abc-123", "test").is_ok());
        assert!(validate_path_segment("report.md", "test").is_ok());
        assert!(validate_path_segment("abc123", "test").is_ok());
    }

    #[test]
    fn frame_boundary_lf() {
        let buf = "event: started\ndata: {}\n\nrest";
        let (pos, len) = find_frame_boundary(buf).unwrap();
        assert_eq!(len, 2);
        assert_eq!(&buf[..pos], "event: started\ndata: {}");
    }

    #[test]
    fn frame_boundary_crlf() {
        let buf = "event: started\r\ndata: {}\r\n\r\nrest";
        let (pos, len) = find_frame_boundary(buf).unwrap();
        assert_eq!(len, 4);
        assert_eq!(&buf[pos + len..], "rest");
    }

    #[test]
    fn frame_boundary_lf_before_crlf() {
        // When LF boundary comes first, prefer it
        let buf = "data: a\n\ndata: b\r\n\r\n";
        let (pos, len) = find_frame_boundary(buf).unwrap();
        assert_eq!(len, 2);
        assert_eq!(&buf[..pos], "data: a");
    }

    #[test]
    fn frame_boundary_none() {
        assert!(find_frame_boundary("no boundary here\n").is_none());
    }
}
