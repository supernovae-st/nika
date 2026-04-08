// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Backend types for model management.
//!
//! These types provide a unified interface for local model management,
//! including download progress, model info, and inference configuration.
//!

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use thiserror::Error;

// ============================================================================
// Progress Types
// ============================================================================

/// Progress information during model pull/download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullProgress {
    /// Current status message (e.g., "pulling manifest", "downloading").
    pub status: String,
    /// Bytes completed.
    pub completed: u64,
    /// Total bytes to download.
    pub total: u64,
}

impl PullProgress {
    /// Create a new progress update.
    #[must_use]
    pub fn new(status: impl Into<String>, completed: u64, total: u64) -> Self {
        Self {
            status: status.into(),
            completed,
            total,
        }
    }

    /// Get progress as a percentage (0.0 to 100.0).
    #[must_use]
    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.completed as f64 / self.total as f64) * 100.0
        }
    }

    /// Check if download is complete.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.total > 0 && self.completed >= self.total
    }
}

impl fmt::Display for PullProgress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:.1}%", self.status, self.percent())
    }
}

// ============================================================================
// Model Info
// ============================================================================

/// Information about an installed model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model name (e.g., "llama3.2:7b").
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Quantization level (e.g., "Q4_K_M", "Q8_0").
    pub quantization: Option<String>,
    /// Parameter count (e.g., "7B", "70B").
    pub parameters: Option<String>,
    /// Model digest/hash.
    pub digest: Option<String>,
}

impl ModelInfo {
    /// Get size in gigabytes.
    #[must_use]
    pub fn size_gb(&self) -> f64 {
        self.size as f64 / 1_000_000_000.0
    }

    /// Get size as human-readable string.
    #[must_use]
    pub fn size_human(&self) -> String {
        let gb = self.size_gb();
        if gb >= 1.0 {
            format!("{gb:.1} GB")
        } else {
            format!("{:.0} MB", self.size as f64 / 1_000_000.0)
        }
    }
}

impl fmt::Display for ModelInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.size_human())
    }
}

// ============================================================================
// Download Types
// ============================================================================

/// Request to download a model.
#[derive(Debug, Clone)]
pub struct DownloadRequest<'a> {
    /// The model to download (curated).
    pub model: Option<&'a super::models::KnownModel>,

    /// HuggingFace repo (for passthrough).
    pub hf_repo: Option<String>,

    /// Specific filename to download.
    pub filename: Option<String>,

    /// Quantization level (for curated models).
    pub quantization: Option<super::models::Quantization>,

    /// Force re-download even if exists.
    pub force: bool,
}

impl<'a> DownloadRequest<'a> {
    /// Create a request for a curated model.
    #[must_use]
    pub fn curated(model: &'a super::models::KnownModel) -> Self {
        Self {
            model: Some(model),
            hf_repo: None,
            filename: None,
            quantization: None,
            force: false,
        }
    }

    /// Create a request for a HuggingFace model.
    #[must_use]
    pub fn huggingface(repo: impl Into<String>, filename: impl Into<String>) -> Self {
        Self {
            model: None,
            hf_repo: Some(repo.into()),
            filename: Some(filename.into()),
            quantization: None,
            force: false,
        }
    }

    /// Set the quantization level.
    #[must_use]
    pub fn with_quantization(mut self, quant: super::models::Quantization) -> Self {
        self.quantization = Some(quant);
        self
    }

    /// Force re-download.
    #[must_use]
    pub fn force(mut self) -> Self {
        self.force = true;
        self
    }

    /// Get the target filename for this download.
    #[must_use]
    pub fn target_filename(&self) -> Option<String> {
        if let Some(filename) = &self.filename {
            return Some(filename.clone());
        }

        if let Some(model) = self.model {
            let quant = self
                .quantization
                .unwrap_or(super::models::Quantization::Q4_K_M);
            // Find the filename for this quantization
            return model
                .quantizations
                .iter()
                .find(|(q, _)| *q == quant)
                .map(|(_, f)| (*f).to_string());
        }

        None
    }
}

/// Result of a model download.
#[derive(Debug, Clone)]
pub struct DownloadResult {
    /// Local path to the downloaded model.
    pub path: PathBuf,

    /// Size of the downloaded file in bytes.
    pub size: u64,

    /// SHA256 checksum of the file.
    pub checksum: Option<String>,

    /// Whether the file was already cached.
    pub cached: bool,
}

// ============================================================================
// Native Model Kind
// ============================================================================

/// Specifies which kind of native model to load.
///
/// The native runtime supports two loading paths:
/// - `TextGguf` — local GGUF files via `GgufModelBuilder` (text-only)
/// - `VisionHf` — HuggingFace vision models via `VisionModelBuilder`
///
/// The default is `TextGguf` for backward compatibility. When `VisionHf`
/// is used, `model_path` in `InferenceBackend::load()` is ignored -- the
/// model is fetched from HuggingFace by `model_id`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeModelKind {
    /// Load a local GGUF file (text-only, existing behavior).
    #[default]
    TextGguf,
    /// Load a HuggingFace vision model by model ID.
    VisionHf {
        /// HuggingFace model ID (e.g., "HuggingFaceM4/Idefics3-8B-Llama3").
        model_id: String,
        /// Optional ISQ quantization type string (e.g., "Q4K", "Q8_0").
        /// Parsed at load time via `mistralrs::parse_isq_value`.
        isq: Option<String>,
    },
}

impl NativeModelKind {
    /// Returns true if this is a vision model kind.
    #[must_use]
    pub fn is_vision(&self) -> bool {
        matches!(self, Self::VisionHf { .. })
    }
}

// ============================================================================
// Vision Image
// ============================================================================

/// An image to send to a vision model for inference.
///
/// Contains raw image bytes and the MIME media type. The native runtime
/// will decode these bytes into a `DynamicImage` before passing them to
/// the mistral.rs `VisionMessages` / `RequestBuilder` API.
#[derive(Debug, Clone)]
pub struct VisionImage {
    /// Raw image bytes (PNG, JPEG, WebP, etc.).
    pub bytes: Vec<u8>,
    /// MIME type (e.g., "image/png", "image/jpeg").
    pub media_type: String,
}

impl VisionImage {
    /// Create a new vision image from bytes and media type.
    #[must_use]
    pub fn new(bytes: Vec<u8>, media_type: impl Into<String>) -> Self {
        Self {
            bytes,
            media_type: media_type.into(),
        }
    }
}

// ============================================================================
// Load Configuration
// ============================================================================

/// Configuration for loading a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadConfig {
    /// GPU IDs to use for this model (empty = auto).
    pub gpu_ids: Vec<u32>,
    /// Number of layers to offload to GPU (-1 = all, 0 = none).
    pub gpu_layers: i32,
    /// Context size (token window).
    pub context_size: Option<u32>,
    /// Keep model loaded in memory (prevent unload).
    pub keep_alive: bool,
    /// Which kind of native model to load.
    /// Defaults to `TextGguf` for backward compatibility.
    #[serde(default)]
    pub model_kind: NativeModelKind,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            gpu_ids: Vec::new(),
            gpu_layers: -1, // All layers on GPU by default
            context_size: None,
            keep_alive: false,
            model_kind: NativeModelKind::default(),
        }
    }
}

impl LoadConfig {
    /// Create a new load configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set specific GPU IDs.
    #[must_use]
    pub fn with_gpus(mut self, gpu_ids: Vec<u32>) -> Self {
        self.gpu_ids = gpu_ids;
        self
    }

    /// Set GPU layers (-1 = all, 0 = CPU only).
    #[must_use]
    pub fn with_gpu_layers(mut self, layers: i32) -> Self {
        self.gpu_layers = layers;
        self
    }

    /// Set context size.
    #[must_use]
    pub fn with_context_size(mut self, size: u32) -> Self {
        self.context_size = Some(size);
        self
    }

    /// Set keep alive.
    #[must_use]
    pub fn with_keep_alive(mut self, keep: bool) -> Self {
        self.keep_alive = keep;
        self
    }

    /// Check if this is a CPU-only configuration.
    #[must_use]
    pub fn is_cpu_only(&self) -> bool {
        self.gpu_layers == 0
    }

    /// Check if using all GPU layers.
    #[must_use]
    pub fn is_full_gpu(&self) -> bool {
        self.gpu_layers < 0
    }
}

// ============================================================================
// Chat Types
// ============================================================================

/// Role in a chat conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// System message (instructions).
    System,
    /// User message.
    User,
    /// Assistant response.
    Assistant,
}

impl fmt::Display for ChatRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
        }
    }
}

/// A message in a chat conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role of the message sender.
    pub role: ChatRole,
    /// Content of the message.
    pub content: String,
}

impl ChatMessage {
    /// Create a new system message.
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
        }
    }

    /// Create a new user message.
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }

    /// Create a new assistant message.
    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
        }
    }
}

/// Options for chat completion.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ChatOptions {
    /// Temperature for sampling (0.0 to 2.0).
    pub temperature: Option<f32>,
    /// Top-p (nucleus) sampling.
    pub top_p: Option<f32>,
    /// Top-k sampling.
    pub top_k: Option<u32>,
    /// Maximum tokens to generate.
    pub max_tokens: Option<u32>,
    /// Stop sequences.
    pub stop: Vec<String>,
    /// Seed for reproducibility.
    pub seed: Option<u64>,
}

impl ChatOptions {
    /// Create new chat options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set temperature.
    #[must_use]
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set top-p sampling.
    #[must_use]
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// Set top-k sampling.
    #[must_use]
    pub fn with_top_k(mut self, top_k: u32) -> Self {
        self.top_k = Some(top_k);
        self
    }

    /// Set maximum tokens.
    #[must_use]
    pub fn with_max_tokens(mut self, max: u32) -> Self {
        self.max_tokens = Some(max);
        self
    }

    /// Add a stop sequence.
    #[must_use]
    pub fn with_stop(mut self, stop: impl Into<String>) -> Self {
        self.stop.push(stop.into());
        self
    }

    /// Set seed for reproducibility.
    #[must_use]
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }
}

/// Response from a chat completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    /// The assistant's response message.
    pub message: ChatMessage,
    /// Whether the response is complete (not streaming).
    pub done: bool,
    /// Total duration in nanoseconds.
    pub total_duration: Option<u64>,
    /// Tokens generated.
    pub eval_count: Option<u64>,
    /// Prompt tokens.
    pub prompt_eval_count: Option<u64>,
}

impl ChatResponse {
    /// Get the response content.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.message.content
    }

    /// Get tokens per second (if metrics available).
    #[must_use]
    pub fn tokens_per_second(&self) -> Option<f64> {
        match (self.eval_count, self.total_duration) {
            (Some(count), Some(duration)) if duration > 0 => {
                Some(count as f64 / (duration as f64 / 1_000_000_000.0))
            }
            _ => None,
        }
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Error types for backend operations.
#[derive(Error, Debug, Clone)]
pub enum BackendError {
    /// Backend server is not running.
    #[error("Backend server is not running")]
    NotRunning,

    /// Model not found in registry or locally.
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    /// Model is already loaded.
    #[error("Model already loaded: {0}")]
    AlreadyLoaded(String),

    /// Insufficient GPU/system memory.
    #[error("Insufficient memory to load model")]
    InsufficientMemory,

    /// Network error during pull/API call.
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Process management error.
    #[error("Process error: {0}")]
    ProcessError(String),

    /// Backend-specific error.
    #[error("Backend error: {0}")]
    BackendSpecific(String),

    /// Missing API key for cloud provider.
    #[error("Missing API key for provider: {0}")]
    MissingApiKey(String),

    /// API returned an error response.
    #[error("API error (HTTP {status}): {message}")]
    ApiError {
        /// HTTP status code.
        status: u16,
        /// Error message from API.
        message: String,
    },

    /// Failed to parse API response.
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Model loading failed.
    #[error("Model load error: {0}")]
    LoadError(String),

    /// Inference failed.
    #[error("Inference error: {0}")]
    InferenceError(String),

    /// Invalid model configuration.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Storage/filesystem error.
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Download failed.
    #[error("Download error: {0}")]
    DownloadError(String),

    /// Checksum verification failed.
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumError {
        /// Expected checksum.
        expected: String,
        /// Actual checksum.
        actual: String,
    },

    /// Path traversal attack detected.
    #[error("Path traversal detected: '{path}' escapes storage directory")]
    PathTraversal {
        /// The invalid path.
        path: String,
    },
}

impl BackendError {
    /// Returns `true` if this error is transient and the operation should be retried.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::NetworkError(_) | Self::NotRunning | Self::DownloadError(_)
        )
    }

    /// Returns `true` if this is an authentication/authorization error.
    #[must_use]
    pub fn is_auth_error(&self) -> bool {
        match self {
            Self::MissingApiKey(_) => true,
            Self::ApiError { status, .. } => *status == 401 || *status == 403,
            _ => false,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pull_progress() {
        let progress = PullProgress::new("downloading", 500, 1000);
        assert_eq!(progress.percent(), 50.0);
        assert!(!progress.is_complete());

        let complete = PullProgress::new("complete", 1000, 1000);
        assert!(complete.is_complete());
    }

    #[test]
    fn test_pull_progress_display() {
        let progress = PullProgress::new("pulling", 750, 1000);
        assert_eq!(progress.to_string(), "pulling: 75.0%");
    }

    #[test]
    fn test_model_info_size() {
        let info = ModelInfo {
            name: "llama3.2:7b".to_string(),
            size: 4_500_000_000,
            quantization: Some("Q4_K_M".to_string()),
            parameters: Some("7B".to_string()),
            digest: None,
        };

        assert!((info.size_gb() - 4.5).abs() < 0.01);
        assert_eq!(info.size_human(), "4.5 GB");
    }

    #[test]
    fn test_load_config_default() {
        let config = LoadConfig::default();
        assert!(config.gpu_ids.is_empty());
        assert_eq!(config.gpu_layers, -1);
        assert!(config.is_full_gpu());
        assert!(!config.is_cpu_only());
    }

    #[test]
    fn test_load_config_builder() {
        let config = LoadConfig::new()
            .with_gpus(vec![0, 1])
            .with_gpu_layers(32)
            .with_context_size(8192)
            .with_keep_alive(true);

        assert_eq!(config.gpu_ids, vec![0, 1]);
        assert_eq!(config.gpu_layers, 32);
        assert_eq!(config.context_size, Some(8192));
        assert!(config.keep_alive);
    }

    #[test]
    fn test_chat_message_constructors() {
        let system = ChatMessage::system("You are helpful");
        assert_eq!(system.role, ChatRole::System);
        assert_eq!(system.content, "You are helpful");

        let user = ChatMessage::user("Hello");
        assert_eq!(user.role, ChatRole::User);

        let assistant = ChatMessage::assistant("Hi there!");
        assert_eq!(assistant.role, ChatRole::Assistant);
    }

    #[test]
    fn test_chat_options_builder() {
        let options = ChatOptions::new()
            .with_temperature(0.7)
            .with_top_p(0.9)
            .with_max_tokens(100);

        assert_eq!(options.temperature, Some(0.7));
        assert_eq!(options.top_p, Some(0.9));
        assert_eq!(options.max_tokens, Some(100));
    }

    #[test]
    fn test_backend_error_is_retryable() {
        assert!(BackendError::NetworkError("timeout".to_string()).is_retryable());
        assert!(BackendError::NotRunning.is_retryable());
        assert!(!BackendError::ModelNotFound("model".to_string()).is_retryable());
        assert!(!BackendError::InsufficientMemory.is_retryable());
    }

    // ========================================================================
    // NativeModelKind serde tests
    // ========================================================================

    #[test]
    fn test_native_model_kind_serde_text_gguf() {
        let kind = NativeModelKind::TextGguf;
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("text_gguf"));
        let roundtrip: NativeModelKind = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, NativeModelKind::TextGguf);
    }

    #[test]
    fn test_native_model_kind_serde_vision_hf() {
        let kind = NativeModelKind::VisionHf {
            model_id: "Qwen/Qwen2.5-VL-7B-Instruct".to_string(),
            isq: Some("Q4K".to_string()),
        };
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("vision_hf"));
        assert!(json.contains("Qwen/Qwen2.5-VL-7B-Instruct"));
        assert!(json.contains("Q4K"));
        let roundtrip: NativeModelKind = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, kind);
    }

    #[test]
    fn test_native_model_kind_serde_vision_hf_no_isq() {
        let kind = NativeModelKind::VisionHf {
            model_id: "google/gemma-3-4b-it".to_string(),
            isq: None,
        };
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("vision_hf"));
        assert!(json.contains("google/gemma-3-4b-it"));
        // isq serializes as null (no skip_serializing_if), roundtrip still works
        let roundtrip: NativeModelKind = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, kind);
        assert_eq!(
            roundtrip,
            NativeModelKind::VisionHf {
                model_id: "google/gemma-3-4b-it".to_string(),
                isq: None,
            }
        );
    }

    // ========================================================================
    // LoadConfig serde tests (vision model kind)
    // ========================================================================

    #[test]
    fn test_load_config_serde_default_model_kind() {
        // Deserialize without model_kind → defaults to TextGguf
        let json = r#"{"gpu_ids":[],"gpu_layers":-1,"context_size":null,"keep_alive":false}"#;
        let config: LoadConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.model_kind, NativeModelKind::TextGguf);
    }

    #[test]
    fn test_load_config_serde_with_vision_hf() {
        let json = r#"{"gpu_ids":[],"gpu_layers":-1,"context_size":4096,"keep_alive":false,"model_kind":{"kind":"vision_hf","model_id":"Qwen/Qwen2.5-VL-7B","isq":"Q4K"}}"#;
        let config: LoadConfig = serde_json::from_str(json).unwrap();
        assert!(config.model_kind.is_vision());
        match &config.model_kind {
            NativeModelKind::VisionHf { model_id, isq } => {
                assert_eq!(model_id, "Qwen/Qwen2.5-VL-7B");
                assert_eq!(isq.as_deref(), Some("Q4K"));
            }
            _ => panic!("Expected VisionHf"),
        }
    }

    // ========================================================================
    // VisionImage tests
    // ========================================================================

    #[test]
    fn test_vision_image_construction() {
        let img = VisionImage::new(vec![0x89, 0x50, 0x4E, 0x47], "image/png");
        assert_eq!(img.bytes.len(), 4);
        assert_eq!(img.media_type, "image/png");
    }

    #[test]
    fn test_vision_image_empty_bytes() {
        let img = VisionImage::new(vec![], "image/jpeg");
        assert_eq!(img.bytes.len(), 0);
        assert_eq!(img.media_type, "image/jpeg");
    }
}
