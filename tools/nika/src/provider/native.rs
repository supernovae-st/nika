//! Native inference provider using spn-native (mistral.rs backend).
//!
//! Provides local GGUF model inference without requiring Ollama.
//! Requires the `native-inference` feature to be enabled.
//!
//! ## Usage
//!
//! ```ignore
//! use nika::provider::native::NativeClient;
//!
//! let mut client = NativeClient::new();
//!
//! // Load a model first
//! client.load("~/.spn/models/qwen3-8b-q4_k_m.gguf", None).await?;
//!
//! // Run inference
//! let response = client.infer("Hello!", None).await?;
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

// Re-export the InferenceBackend trait for consumers
pub use spn_native::inference::InferenceBackend;
pub use spn_native::{ChatOptions, ChatResponse, LoadConfig, NativeRuntime};

/// Error type for native inference operations.
#[derive(Debug, thiserror::Error)]
pub enum NativeClientError {
    /// Model not loaded
    #[error("No model loaded. Call load() first.")]
    ModelNotLoaded,

    /// Model load failed
    #[error("Failed to load model: {0}")]
    LoadFailed(String),

    /// Inference failed
    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    /// Invalid model path
    #[error("Invalid model path: {0}")]
    InvalidPath(String),
}

/// Native inference client wrapping spn-native's NativeRuntime.
///
/// Provides a high-level interface for local GGUF model inference.
/// Thread-safe via `Arc<RwLock<>>`.
#[derive(Clone)]
pub struct NativeClient {
    /// The underlying inference runtime.
    runtime: Arc<RwLock<NativeRuntime>>,
    /// Name/path of the currently loaded model (for display).
    loaded_model: Arc<RwLock<Option<String>>>,
}

impl std::fmt::Debug for NativeClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeClient")
            .field("loaded", &"<runtime>")
            .finish()
    }
}

impl Default for NativeClient {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeClient {
    /// Create a new native inference client.
    ///
    /// The client is created without a model loaded. Call `load()` before inference.
    #[must_use]
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(RwLock::new(NativeRuntime::new())),
            loaded_model: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if a model is currently loaded.
    pub async fn is_loaded(&self) -> bool {
        let runtime = self.runtime.read().await;
        runtime.is_loaded()
    }

    /// Get the name of the currently loaded model.
    pub async fn loaded_model(&self) -> Option<String> {
        self.loaded_model.read().await.clone()
    }

    /// Load a GGUF model from the given path.
    ///
    /// # Arguments
    /// * `model_path` - Path to the GGUF model file
    /// * `config` - Optional load configuration (context size, GPU layers, etc.)
    ///
    /// # Example
    /// ```ignore
    /// client.load("~/.spn/models/qwen3-8b-q4_k_m.gguf", None).await?;
    /// ```
    pub async fn load(
        &self,
        model_path: impl Into<PathBuf>,
        config: Option<LoadConfig>,
    ) -> Result<(), NativeClientError> {
        let path = model_path.into();
        let config = config.unwrap_or_default();

        // Validate path exists
        if !path.exists() {
            return Err(NativeClientError::InvalidPath(format!(
                "Model file not found: {}",
                path.display()
            )));
        }

        // Extract model name for display
        let model_name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Load the model
        let mut runtime = self.runtime.write().await;
        runtime
            .load(path, config)
            .await
            .map_err(|e| NativeClientError::LoadFailed(e.to_string()))?;

        // Store the loaded model name
        *self.loaded_model.write().await = Some(model_name);

        Ok(())
    }

    /// Unload the current model from memory.
    pub async fn unload(&self) -> Result<(), NativeClientError> {
        let mut runtime = self.runtime.write().await;
        runtime
            .unload()
            .await
            .map_err(|e| NativeClientError::InferenceFailed(e.to_string()))?;

        *self.loaded_model.write().await = None;
        Ok(())
    }

    /// Run inference on the loaded model.
    ///
    /// # Arguments
    /// * `prompt` - The input prompt
    /// * `options` - Optional inference options (temperature, max_tokens, etc.)
    ///
    /// # Returns
    /// The complete chat response.
    ///
    /// # Errors
    /// Returns `ModelNotLoaded` if no model is loaded.
    pub async fn infer(
        &self,
        prompt: &str,
        options: Option<ChatOptions>,
    ) -> Result<ChatResponse, NativeClientError> {
        let runtime = self.runtime.read().await;

        if !runtime.is_loaded() {
            return Err(NativeClientError::ModelNotLoaded);
        }

        let opts = options.unwrap_or_default();
        runtime
            .infer(prompt, opts)
            .await
            .map_err(|e| NativeClientError::InferenceFailed(e.to_string()))
    }

    /// Get information about the loaded model.
    pub async fn model_info(&self) -> Option<spn_native::ModelInfo> {
        let runtime = self.runtime.read().await;
        runtime.model_info().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_client_creation() {
        let client = NativeClient::new();
        // Just verify it can be created
        assert!(std::mem::size_of_val(&client) > 0);
    }

    #[tokio::test]
    async fn test_native_client_not_loaded() {
        let client = NativeClient::new();
        assert!(!client.is_loaded().await);
        assert!(client.loaded_model().await.is_none());
    }

    #[tokio::test]
    async fn test_infer_without_model_fails() {
        let client = NativeClient::new();
        let result = client.infer("test", None).await;
        assert!(matches!(result, Err(NativeClientError::ModelNotLoaded)));
    }

    #[tokio::test]
    async fn test_load_nonexistent_path_fails() {
        let client = NativeClient::new();
        let result = client.load("/nonexistent/path/model.gguf", None).await;
        assert!(matches!(result, Err(NativeClientError::InvalidPath(_))));
    }
}
