//! Native Ollama HTTP client for model management
//!
//! Based on:
//! - Context7: Ollama REST API documentation
//! - Perplexity: NDJSON streaming best practices
//! - Codebase: Existing reqwest patterns from executor.rs

use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::mpsc;

/// Maximum buffer size for NDJSON streaming (1MB) - prevents DoS
const MAX_NDJSON_BUFFER_SIZE: usize = 1024 * 1024;

/// Ollama API base URL (default: http://localhost:11434)
/// Validates URL scheme is HTTP or HTTPS to prevent SSRF
pub fn ollama_base_url() -> String {
    let url_str = std::env::var("OLLAMA_HOST")
        .or_else(|_| std::env::var("OLLAMA_API_BASE_URL"))
        .unwrap_or_else(|_| "http://localhost:11434".to_string());

    // Validate URL scheme (SEC-002: SSRF prevention)
    if let Ok(url) = url::Url::parse(&url_str) {
        if matches!(url.scheme(), "http" | "https") {
            return url_str;
        }
        tracing::warn!(
            "Invalid Ollama URL scheme '{}', using default",
            url.scheme()
        );
    } else {
        tracing::warn!("Invalid Ollama URL '{}', using default", url_str);
    }

    "http://localhost:11434".to_string()
}

/// Model info from /api/tags
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelInfo {
    pub name: String,
    pub size: u64,
    pub digest: String,
    pub modified_at: String,
    pub details: OllamaModelDetails,
}

impl OllamaModelInfo {
    /// Format size for display
    pub fn size_display(&self) -> String {
        if self.size >= 1_000_000_000 {
            format!("{:.1} GB", self.size as f64 / 1_000_000_000.0)
        } else if self.size >= 1_000_000 {
            format!("{:.1} MB", self.size as f64 / 1_000_000.0)
        } else {
            format!("{:.1} KB", self.size as f64 / 1_000.0)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelDetails {
    pub parameter_size: String,
    pub quantization_level: String,
    pub family: Option<String>,
}

/// Pull progress event (NDJSON streaming)
#[derive(Debug, Clone, Deserialize)]
pub struct PullProgressEvent {
    pub status: String,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
}

/// Pull progress for UI updates
#[derive(Debug, Clone)]
pub enum PullProgress {
    Starting,
    Downloading {
        digest: String,
        completed: u64,
        total: u64,
    },
    Verifying,
    Writing,
    Complete,
    Error(String),
}

/// Ollama client error types
#[derive(Debug, Error)]
pub enum OllamaError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Failed to delete model: {0}")]
    DeleteFailed(String),
    #[error("Ollama not running")]
    NotRunning,
}

/// Native Ollama HTTP client
pub struct OllamaClient {
    client: Client,
    base_url: String,
}

impl Default for OllamaClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OllamaClient {
    /// Create client with connection pooling
    pub fn new() -> Self {
        Self::with_base_url(ollama_base_url())
    }

    /// Create client with custom base URL (for testing)
    pub fn with_base_url(base_url: String) -> Self {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .pool_max_idle_per_host(2)
            .user_agent("nika-cli/0.8")
            .build()
            .expect("Failed to create HTTP client");

        Self { client, base_url }
    }

    /// Check if Ollama is running (2s timeout)
    pub async fn is_available(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// List installed models
    pub async fn list_models(&self) -> Result<Vec<OllamaModelInfo>, OllamaError> {
        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<OllamaModelInfo>,
        }

        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| OllamaError::ConnectionFailed(e.to_string()))?;

        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| OllamaError::ParseError(e.to_string()))?;

        Ok(tags.models)
    }

    /// Pull model with streaming progress
    pub async fn pull_model(&self, model: &str) -> mpsc::Receiver<PullProgress> {
        let (tx, rx) = mpsc::channel(32);
        let url = format!("{}/api/pull", self.base_url);
        let client = self.client.clone();
        let model = model.to_string();

        tokio::spawn(async move {
            let body = serde_json::json!({
                "name": model,
                "stream": true
            });

            let response = match client.post(&url).json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(PullProgress::Error(e.to_string())).await;
                    return;
                }
            };

            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            let _ = tx.send(PullProgress::Starting).await;

            while let Some(chunk) = stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(PullProgress::Error(e.to_string())).await;
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // SEC-003: Prevent DoS via unbounded buffer growth
                if buffer.len() > MAX_NDJSON_BUFFER_SIZE {
                    let _ = tx
                        .send(PullProgress::Error("Response too large".to_string()))
                        .await;
                    break;
                }

                // Process complete NDJSON lines
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.trim().is_empty() {
                        continue;
                    }

                    if let Ok(event) = serde_json::from_str::<PullProgressEvent>(&line) {
                        let progress = match event.status.as_str() {
                            s if s.contains("pulling") => {
                                if let (Some(digest), Some(total), Some(completed)) =
                                    (event.digest, event.total, event.completed)
                                {
                                    PullProgress::Downloading {
                                        digest,
                                        completed,
                                        total,
                                    }
                                } else {
                                    PullProgress::Starting
                                }
                            }
                            s if s.contains("verifying") => PullProgress::Verifying,
                            s if s.contains("writing") => PullProgress::Writing,
                            "success" => PullProgress::Complete,
                            _ => continue,
                        };
                        let _ = tx.send(progress).await;
                    }
                }
            }
        });

        rx
    }

    /// Delete installed model
    pub async fn delete_model(&self, model: &str) -> Result<(), OllamaError> {
        let body = serde_json::json!({ "name": model });

        let response = self
            .client
            .delete(format!("{}/api/delete", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| OllamaError::ConnectionFailed(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(OllamaError::DeleteFailed(model.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_base_url_default() {
        // Use unique env var name to avoid test pollution
        let prev = std::env::var("OLLAMA_HOST").ok();
        std::env::remove_var("OLLAMA_HOST");
        std::env::remove_var("OLLAMA_API_BASE_URL");
        let result = ollama_base_url();
        // Restore
        if let Some(v) = prev {
            std::env::set_var("OLLAMA_HOST", v);
        }
        assert_eq!(result, "http://localhost:11434");
    }

    #[test]
    fn test_pull_progress_event_parse() {
        let json =
            r#"{"status":"pulling sha256:abc","digest":"sha256:abc","total":1000,"completed":500}"#;
        let event: PullProgressEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.status, "pulling sha256:abc");
        assert_eq!(event.total, Some(1000));
        assert_eq!(event.completed, Some(500));
    }

    #[test]
    fn test_pull_progress_event_minimal() {
        let json = r#"{"status":"success"}"#;
        let event: PullProgressEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.status, "success");
        assert!(event.digest.is_none());
    }

    #[test]
    fn test_model_info_parse() {
        let json = r#"{
            "name": "llama3.2:latest",
            "size": 4700000000,
            "digest": "sha256:abc123",
            "modified_at": "2026-02-24T10:00:00Z",
            "details": {
                "parameter_size": "8B",
                "quantization_level": "Q4_0"
            }
        }"#;
        let model: OllamaModelInfo = serde_json::from_str(json).unwrap();
        assert_eq!(model.name, "llama3.2:latest");
        assert_eq!(model.size, 4700000000);
        assert_eq!(model.details.parameter_size, "8B");
    }

    #[test]
    fn test_model_info_size_display() {
        let model = OllamaModelInfo {
            name: "test".into(),
            size: 4_700_000_000,
            digest: "sha256:abc".into(),
            modified_at: "2026-01-01".into(),
            details: OllamaModelDetails {
                parameter_size: "8B".into(),
                quantization_level: "Q4_0".into(),
                family: None,
            },
        };
        assert_eq!(model.size_display(), "4.7 GB");
    }

    // SEC-002: URL validation tests
    // NOTE: These tests must clear both env vars to avoid parallel test pollution

    #[test]
    fn test_ollama_base_url_valid_http() {
        std::env::remove_var("OLLAMA_API_BASE_URL");
        std::env::set_var("OLLAMA_HOST", "http://192.168.1.100:11434");
        let url = ollama_base_url();
        std::env::remove_var("OLLAMA_HOST");
        assert_eq!(url, "http://192.168.1.100:11434");
    }

    #[test]
    fn test_ollama_base_url_valid_https() {
        std::env::remove_var("OLLAMA_API_BASE_URL");
        std::env::set_var("OLLAMA_HOST", "https://ollama.example.com");
        let url = ollama_base_url();
        std::env::remove_var("OLLAMA_HOST");
        assert_eq!(url, "https://ollama.example.com");
    }

    #[test]
    fn test_ollama_base_url_invalid_scheme_uses_default() {
        std::env::remove_var("OLLAMA_API_BASE_URL");
        std::env::set_var("OLLAMA_HOST", "ftp://malicious.com");
        let url = ollama_base_url();
        std::env::remove_var("OLLAMA_HOST");
        assert_eq!(url, "http://localhost:11434");
    }

    #[test]
    fn test_ollama_base_url_invalid_url_uses_default() {
        // Clear env first to avoid pollution from parallel tests
        std::env::remove_var("OLLAMA_API_BASE_URL");
        std::env::set_var("OLLAMA_HOST", "not-a-valid-url");
        let url = ollama_base_url();
        std::env::remove_var("OLLAMA_HOST");
        // "not-a-valid-url" has no scheme, so url::Url::parse fails -> returns default
        assert_eq!(url, "http://localhost:11434");
    }

    // =========== Wiremock Integration Tests ===========

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_is_available_returns_true_when_running() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": []
            })))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::with_base_url(mock_server.uri());
        assert!(client.is_available().await);
    }

    #[tokio::test]
    async fn test_is_available_returns_false_when_not_running() {
        // Use a port that's definitely not running anything
        let client = OllamaClient::with_base_url("http://127.0.0.1:19999".to_string());
        assert!(!client.is_available().await);
    }

    #[tokio::test]
    async fn test_list_models_returns_empty_list() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": []
            })))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::with_base_url(mock_server.uri());
        let models = client.list_models().await.unwrap();
        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn test_list_models_returns_models() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": [
                    {
                        "name": "llama3.2:latest",
                        "size": 4700000000_u64,
                        "digest": "sha256:abc123",
                        "modified_at": "2026-02-24T10:00:00Z",
                        "details": {
                            "parameter_size": "8B",
                            "quantization_level": "Q4_0"
                        }
                    },
                    {
                        "name": "mistral:latest",
                        "size": 7000000000_u64,
                        "digest": "sha256:def456",
                        "modified_at": "2026-02-23T10:00:00Z",
                        "details": {
                            "parameter_size": "7B",
                            "quantization_level": "Q4_K_M"
                        }
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::with_base_url(mock_server.uri());
        let models = client.list_models().await.unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "llama3.2:latest");
        assert_eq!(models[0].size, 4700000000);
        assert_eq!(models[1].name, "mistral:latest");
    }

    #[tokio::test]
    async fn test_list_models_connection_error() {
        let client = OllamaClient::with_base_url("http://127.0.0.1:19999".to_string());
        let result = client.list_models().await;
        assert!(matches!(result, Err(OllamaError::ConnectionFailed(_))));
    }

    #[tokio::test]
    async fn test_list_models_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::with_base_url(mock_server.uri());
        let result = client.list_models().await;
        assert!(matches!(result, Err(OllamaError::ParseError(_))));
    }

    #[tokio::test]
    async fn test_delete_model_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/api/delete"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::with_base_url(mock_server.uri());
        let result = client.delete_model("llama3.2").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_model_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/api/delete"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::with_base_url(mock_server.uri());
        let result = client.delete_model("nonexistent").await;
        assert!(matches!(result, Err(OllamaError::DeleteFailed(_))));
    }

    #[tokio::test]
    async fn test_pull_model_emits_starting() {
        let mock_server = MockServer::start().await;

        // Mock returns a simple NDJSON response
        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"status":"pulling manifest"}
{"status":"success"}
"#,
            ))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::with_base_url(mock_server.uri());
        let mut rx = client.pull_model("llama3.2").await;

        // First event should be Starting
        let first = rx.recv().await;
        assert!(matches!(first, Some(PullProgress::Starting)));
    }

    #[tokio::test]
    async fn test_pull_model_complete_flow() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"status":"pulling manifest"}
{"status":"pulling sha256:abc","digest":"sha256:abc","total":1000,"completed":500}
{"status":"pulling sha256:abc","digest":"sha256:abc","total":1000,"completed":1000}
{"status":"verifying sha256:abc"}
{"status":"writing manifest"}
{"status":"success"}
"#,
            ))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::with_base_url(mock_server.uri());
        let mut rx = client.pull_model("llama3.2").await;

        let mut events = vec![];
        while let Some(event) = rx.recv().await {
            events.push(event);
        }

        // Should have Starting, then downloading events, verifying, writing, complete
        assert!(!events.is_empty());
        assert!(matches!(events[0], PullProgress::Starting));
        assert!(events.iter().any(|e| matches!(e, PullProgress::Complete)));
    }

    #[tokio::test]
    async fn test_with_base_url_creates_client() {
        let client = OllamaClient::with_base_url("http://custom:1234".to_string());
        // Just verify it creates successfully (internal state)
        assert_eq!(client.base_url, "http://custom:1234");
    }
}
