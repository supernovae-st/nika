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

/// Ollama API base URL (default: http://localhost:11434)
pub fn ollama_base_url() -> String {
    std::env::var("OLLAMA_HOST")
        .or_else(|_| std::env::var("OLLAMA_API_BASE_URL"))
        .unwrap_or_else(|_| "http://localhost:11434".to_string())
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
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .pool_max_idle_per_host(2)
            .user_agent("nika-cli/0.8")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: ollama_base_url(),
        }
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
        std::env::remove_var("OLLAMA_HOST");
        std::env::remove_var("OLLAMA_API_BASE_URL");
        assert_eq!(ollama_base_url(), "http://localhost:11434");
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
}
