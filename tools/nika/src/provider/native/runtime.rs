//! Native runtime implementation using mistral.rs.
//!
//! This module provides the `NativeRuntime` struct which implements
//! the `InferenceBackend` trait using the mistral.rs library.
//!
//! v0.27.0: Migrated from spn-native.

use crate::core::backend::{
    ChatMessage, ChatOptions, ChatResponse, ChatRole, LoadConfig, ModelInfo,
};
use crate::core::storage::extract_quantization;
use crate::provider::native::error::NativeError;
use crate::provider::native::traits::InferenceBackend;
use futures::Stream;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "native-inference")]
use crate::util::constants::INFER_TIMEOUT;
#[cfg(feature = "native-inference")]
use parking_lot::Mutex;
#[cfg(feature = "native-inference")]
use std::path::Path;
#[cfg(feature = "native-inference")]
use tracing::{debug, info};

#[cfg(feature = "native-inference")]
#[allow(unused_imports)] // Used in infer_stream for stream.next()
use futures::StreamExt as _;
#[cfg(feature = "native-inference")]
use mistralrs::{
    GgufModelBuilder, MemoryGpuConfig, Model, PagedAttentionMetaBuilder, RequestBuilder,
    TextMessageRole, TextMessages,
};
#[cfg(feature = "native-inference")]
use tokio::sync::RwLock;

/// A tracked spawned task with its cancellation token.
#[cfg(feature = "native-inference")]
struct TrackedTask {
    /// Handle to the spawned task.
    #[allow(dead_code)] // Used for abort on drop
    handle: JoinHandle<()>,
    /// Cancellation token for graceful shutdown.
    #[allow(dead_code)] // Stored for future cancellation use
    token: CancellationToken,
}

/// Native runtime for local LLM inference.
///
/// Uses mistral.rs for high-performance inference on GGUF models.
/// Supports CPU and GPU (Metal on macOS, CUDA on Linux) acceleration.
///
/// # Cancellation Support
///
/// Spawned inference tasks support graceful cancellation via `CancellationToken`.
/// Call `cancel_all()` to signal all in-flight tasks to stop, or `shutdown()`
/// to cancel and wait for tasks to complete.
///
/// # Example
///
/// ```ignore
/// use nika::provider::native::NativeRuntime;
/// use nika::core::backend::LoadConfig;
///
/// let mut runtime = NativeRuntime::new();
/// runtime.load("model.gguf".into(), LoadConfig::default()).await?;
/// let response = runtime.infer("Hello!", Default::default()).await?;
///
/// // Cancel all in-flight tasks
/// runtime.cancel_all();
/// ```
#[allow(dead_code)] // Fields used only with inference feature
pub struct NativeRuntime {
    /// The loaded model (None if no model is loaded).
    #[cfg(feature = "native-inference")]
    model: Option<Arc<RwLock<Model>>>,

    /// Metadata about the loaded model.
    model_info: Option<ModelInfo>,

    /// Path to the currently loaded model.
    model_path: Option<PathBuf>,

    /// Load configuration used for the current model.
    config: Option<LoadConfig>,

    /// Master cancellation token for all spawned tasks.
    cancellation_token: CancellationToken,

    /// Tracked spawned tasks for cleanup.
    #[cfg(feature = "native-inference")]
    tasks: Arc<Mutex<Vec<TrackedTask>>>,
}

// Manual Debug implementation (Model doesn't implement Debug)
impl std::fmt::Debug for NativeRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dbg = f.debug_struct("NativeRuntime");
        dbg.field("model_info", &self.model_info)
            .field("model_path", &self.model_path)
            .field("config", &self.config)
            .field("is_loaded", &self.is_loaded())
            .field("is_cancelled", &self.cancellation_token.is_cancelled());

        #[cfg(feature = "native-inference")]
        {
            let task_count = self.tasks.lock().len();
            dbg.field("active_tasks", &task_count);
        }

        dbg.finish()
    }
}

// Manual Clone implementation (clones the Arc, not the model itself)
// Note: Clone shares the same cancellation token and task list
impl Clone for NativeRuntime {
    fn clone(&self) -> Self {
        Self {
            #[cfg(feature = "native-inference")]
            model: self.model.clone(),
            model_info: self.model_info.clone(),
            model_path: self.model_path.clone(),
            config: self.config.clone(),
            cancellation_token: self.cancellation_token.clone(),
            #[cfg(feature = "native-inference")]
            tasks: Arc::clone(&self.tasks),
        }
    }
}

impl NativeRuntime {
    /// Create a new native runtime.
    ///
    /// The runtime is created without a model loaded. Call `load()` to
    /// load a model before running inference.
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "native-inference")]
            model: None,
            model_info: None,
            model_path: None,
            config: None,
            cancellation_token: CancellationToken::new(),
            #[cfg(feature = "native-inference")]
            tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Cancel all in-flight inference tasks.
    ///
    /// This signals all spawned tasks to stop gracefully. Tasks that are
    /// currently running will return `NativeError::Cancelled` on their next
    /// cancellation check point.
    ///
    /// Note: This does not wait for tasks to complete. Use `shutdown()` if
    /// you need to wait for all tasks to finish.
    pub fn cancel_all(&self) {
        tracing::debug!("Cancelling all native inference tasks");
        self.cancellation_token.cancel();
    }

    /// Check if cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }

    /// Get a child cancellation token for a new task.
    ///
    /// The returned token will be cancelled when either:
    /// - The parent token is cancelled (via `cancel_all()`)
    /// - The child token is cancelled directly
    #[must_use]
    pub fn child_token(&self) -> CancellationToken {
        self.cancellation_token.child_token()
    }

    /// Clean up completed tasks from the task list.
    #[cfg(feature = "native-inference")]
    fn cleanup_completed_tasks(&self) {
        let mut tasks = self.tasks.lock();
        tasks.retain(|task| !task.handle.is_finished());
    }

    /// Get the number of active (non-completed) tasks.
    #[cfg(feature = "native-inference")]
    #[must_use]
    pub fn active_task_count(&self) -> usize {
        self.cleanup_completed_tasks();
        self.tasks.lock().len()
    }

    /// Shutdown the runtime, cancelling all tasks and waiting for completion.
    ///
    /// This method:
    /// 1. Cancels all in-flight tasks via the cancellation token
    /// 2. Waits for all tasks to complete (with timeout)
    /// 3. Unloads the model if loaded
    ///
    /// # Arguments
    ///
    /// * `timeout` - Maximum time to wait for tasks to complete
    #[cfg(feature = "native-inference")]
    pub async fn shutdown(&mut self, timeout: std::time::Duration) -> Result<(), NativeError> {
        use tokio::time::timeout as tokio_timeout;

        tracing::info!("Shutting down native runtime");

        // Cancel all tasks
        self.cancel_all();

        // Wait for tasks to complete (with timeout)
        let tasks: Vec<_> = {
            let mut guard = self.tasks.lock();
            std::mem::take(&mut *guard)
        };

        if !tasks.is_empty() {
            tracing::debug!(task_count = tasks.len(), "Waiting for tasks to complete");

            let wait_future = async {
                for task in tasks {
                    // Ignore join errors (task may have panicked)
                    let _ = task.handle.await;
                }
            };

            if tokio_timeout(timeout, wait_future).await.is_err() {
                tracing::warn!("Timeout waiting for tasks to complete during shutdown");
            }
        }

        // Unload model
        self.unload().await?;

        tracing::info!("Native runtime shutdown complete");
        Ok(())
    }

    /// Get the path to the currently loaded model.
    #[must_use]
    pub fn model_path(&self) -> Option<&PathBuf> {
        self.model_path.as_ref()
    }

    /// Get the load configuration for the current model.
    #[must_use]
    pub fn config(&self) -> Option<&LoadConfig> {
        self.config.as_ref()
    }

    /// Convert ChatRole to mistral.rs TextMessageRole.
    #[cfg(feature = "native-inference")]
    #[allow(dead_code)] // Will be used for streaming support
    fn convert_role(role: ChatRole) -> TextMessageRole {
        match role {
            ChatRole::System => TextMessageRole::System,
            ChatRole::User => TextMessageRole::User,
            ChatRole::Assistant => TextMessageRole::Assistant,
        }
    }
}

impl Default for NativeRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native-inference")]
impl InferenceBackend for NativeRuntime {
    async fn load(&mut self, model_path: PathBuf, config: LoadConfig) -> Result<(), NativeError> {
        info!(?model_path, "Loading GGUF model");

        // Unload any existing model
        if self.model.is_some() {
            self.unload().await?;
        }

        // Validate path exists
        if !model_path.exists() {
            return Err(NativeError::ModelNotFound {
                repo: "local".to_string(),
                filename: model_path.to_string_lossy().to_string(),
            });
        }

        // Build the model using GgufModelBuilder
        // API: GgufModelBuilder::new(directory, vec![filename])
        let parent = model_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let filename = model_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .ok_or_else(|| {
                NativeError::InvalidConfig("Invalid model path: no filename".to_string())
            })?;

        debug!(gpu_layers = config.gpu_layers, %parent, %filename, "Building model");

        // Build model with PagedAttention for better memory management.
        // PagedAttention enables efficient KV cache handling for longer contexts.
        // Use context_size from LoadConfig, defaulting to 2048 if not specified.
        let context_size = config.context_size.unwrap_or(2048);
        let model = GgufModelBuilder::new(parent, vec![filename])
            .with_logging()
            .with_paged_attn(|| {
                PagedAttentionMetaBuilder::default()
                    .with_block_size(32)
                    .with_gpu_memory(MemoryGpuConfig::ContextSize(context_size as usize))
                    .build()
            })
            .map_err(|e| NativeError::InvalidConfig(format!("PagedAttention config error: {e}")))?
            .build()
            .await
            .map_err(|e| NativeError::InvalidConfig(format!("Failed to build model: {e}")))?;

        // Extract model info from the loaded model
        let info = ModelInfo {
            name: model_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            size: tokio::fs::metadata(&model_path)
                .await
                .map(|m| m.len())
                .unwrap_or(0),
            quantization: extract_quantization_from_path(&model_path),
            parameters: None,
            digest: None,
        };

        self.model = Some(Arc::new(RwLock::new(model)));
        self.model_info = Some(info);
        self.model_path = Some(model_path);
        self.config = Some(config);

        info!("Model loaded successfully");
        Ok(())
    }

    async fn unload(&mut self) -> Result<(), NativeError> {
        if self.model.is_some() {
            info!("Unloading model");
            self.model = None;
            self.model_info = None;
            self.model_path = None;
            self.config = None;
        }
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.model.is_some()
    }

    fn model_info(&self) -> Option<&ModelInfo> {
        self.model_info.as_ref()
    }

    async fn infer(&self, prompt: &str, options: ChatOptions) -> Result<ChatResponse, NativeError> {
        let model = self.model.as_ref().ok_or(NativeError::ModelNotLoaded)?;

        let model = model.read().await;

        // Build messages - just user prompt for now
        // System messages should be passed as part of the prompt or via messages API
        let messages = TextMessages::new().add_message(TextMessageRole::User, prompt);

        debug!(
            temperature = options.temperature,
            max_tokens = options.max_tokens,
            "Running inference"
        );

        // Build request with sampling parameters
        let mut request = RequestBuilder::from(messages);

        // Apply temperature if provided (convert f32 to f64)
        if let Some(temp) = options.temperature {
            request = request.set_sampler_temperature(f64::from(temp));
        }

        // Apply max_tokens if provided
        if let Some(max_tokens) = options.max_tokens {
            request = request.set_sampler_max_len(max_tokens as usize);
        }

        // Send request with sampling parameters and timeout
        let response = tokio::time::timeout(INFER_TIMEOUT, model.send_chat_request(request))
            .await
            .map_err(|_| NativeError::InferenceTimeout {
                timeout_secs: INFER_TIMEOUT.as_secs(),
            })?
            .map_err(|e| NativeError::InferenceFailed(format!("Inference failed: {e}")))?;

        // Extract response content - fail if no content returned
        let content = response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .ok_or_else(|| {
                NativeError::InferenceFailed(
                    "Model returned empty response (no choices)".to_string(),
                )
            })?;

        // Log performance metrics for debugging and optimization
        debug!(
            prompt_tokens = response.usage.prompt_tokens,
            completion_tokens = response.usage.completion_tokens,
            avg_prompt_tok_per_sec = ?response.usage.avg_prompt_tok_per_sec,
            avg_compl_tok_per_sec = ?response.usage.avg_compl_tok_per_sec,
            "Inference completed"
        );

        Ok(ChatResponse {
            message: ChatMessage {
                role: ChatRole::Assistant,
                content,
            },
            done: true,
            total_duration: None,
            prompt_eval_count: Some(response.usage.prompt_tokens as u32),
            eval_count: Some(response.usage.completion_tokens as u32),
        })
    }

    async fn infer_stream(
        &self,
        prompt: &str,
        options: ChatOptions,
    ) -> Result<impl Stream<Item = Result<String, NativeError>> + Send, NativeError> {
        use crate::util::constants::STREAM_CHUNK_TIMEOUT;
        use async_stream::stream;
        use mistralrs::Response;
        use tokio::sync::mpsc;

        // Check for cancellation before starting
        if self.cancellation_token.is_cancelled() {
            return Err(NativeError::Cancelled);
        }

        let model = self.model.as_ref().ok_or(NativeError::ModelNotLoaded)?;
        let model_arc = Arc::clone(model);
        let prompt_owned = prompt.to_string();

        // Create channel for streaming chunks
        let (tx, mut rx) = mpsc::channel::<Result<String, NativeError>>(32);

        // Get a child cancellation token for this task
        let task_token = self.child_token();
        let task_token_clone = task_token.clone();

        // Spawn streaming task that holds the model lock
        // Task will be cancelled when: receiver dropped, parent cancelled, or task_token cancelled
        let handle = tokio::spawn(async move {
            let model = model_arc.read().await;

            // Build messages
            let messages = TextMessages::new().add_message(TextMessageRole::User, &prompt_owned);

            // Build request with sampling parameters
            let mut request = RequestBuilder::from(messages);

            if let Some(temp) = options.temperature {
                request = request.set_sampler_temperature(f64::from(temp));
            }

            if let Some(max_tokens) = options.max_tokens {
                request = request.set_sampler_max_len(max_tokens as usize);
            }

            // Stream chat request with per-chunk timeout
            match model.stream_chat_request(request).await {
                Ok(mut stream) => {
                    loop {
                        // Check for cancellation at each iteration
                        if task_token_clone.is_cancelled() {
                            debug!("Streaming task cancelled");
                            let _ = tx.send(Err(NativeError::Cancelled)).await;
                            break;
                        }

                        // Race between: cancellation, timeout, and next chunk
                        let chunk_result = tokio::select! {
                            biased;

                            _ = task_token_clone.cancelled() => {
                                debug!("Streaming task cancelled during chunk wait");
                                let _ = tx.send(Err(NativeError::Cancelled)).await;
                                break;
                            }

                            result = tokio::time::timeout(STREAM_CHUNK_TIMEOUT, stream.next()) => {
                                result
                            }
                        };

                        let chunk = match chunk_result {
                            Ok(Some(c)) => c,
                            Ok(None) => break, // Stream ended normally
                            Err(_) => {
                                // Chunk timeout - send error and stop
                                let _ = tx
                                    .send(Err(NativeError::InferenceTimeout {
                                        timeout_secs: STREAM_CHUNK_TIMEOUT.as_secs(),
                                    }))
                                    .await;
                                break;
                            }
                        };

                        match chunk {
                            Response::Chunk(chunk_response) => {
                                if let Some(choice) = chunk_response.choices.first() {
                                    if let Some(text) = &choice.delta.content {
                                        if tx.send(Ok(text.clone())).await.is_err() {
                                            // Receiver dropped, stop streaming
                                            break;
                                        }
                                    }
                                }
                            }
                            Response::Done(_) => {
                                debug!("Streaming completed");
                                break;
                            }
                            Response::ModelError(msg, _) => {
                                let _ = tx
                                    .send(Err(NativeError::InferenceFailed(format!(
                                        "Model error: {}",
                                        msg
                                    ))))
                                    .await;
                                break;
                            }
                            Response::ValidationError(err) => {
                                let _ = tx
                                    .send(Err(NativeError::InferenceFailed(format!(
                                        "Validation error: {:?}",
                                        err
                                    ))))
                                    .await;
                                break;
                            }
                            Response::InternalError(err) => {
                                let _ = tx
                                    .send(Err(NativeError::InferenceFailed(format!(
                                        "Internal error: {:?}",
                                        err
                                    ))))
                                    .await;
                                break;
                            }
                            _ => {
                                // Other response types, continue
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(Err(NativeError::InferenceFailed(format!(
                            "Failed to start streaming: {}",
                            e
                        ))))
                        .await;
                }
            }
        });

        // Track the spawned task for cleanup
        {
            let mut tasks = self.tasks.lock();
            tasks.push(TrackedTask {
                handle,
                token: task_token,
            });
        }

        // Clean up completed tasks periodically
        self.cleanup_completed_tasks();

        // Convert mpsc receiver to Stream
        Ok(stream! {
            while let Some(result) = rx.recv().await {
                yield result;
            }
        })
    }
}

/// Extract quantization from file path.
#[cfg(feature = "native-inference")]
fn extract_quantization_from_path(path: &Path) -> Option<String> {
    let filename = path.file_name()?.to_string_lossy();
    extract_quantization(&filename)
}

// Stub implementation when inference feature is not enabled
#[cfg(not(feature = "native-inference"))]
impl InferenceBackend for NativeRuntime {
    async fn load(&mut self, _model_path: PathBuf, _config: LoadConfig) -> Result<(), NativeError> {
        Err(NativeError::InvalidConfig(
            "Inference feature not enabled. Rebuild with --features native-inference".to_string(),
        ))
    }

    async fn unload(&mut self) -> Result<(), NativeError> {
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        false
    }

    fn model_info(&self) -> Option<&ModelInfo> {
        None
    }

    async fn infer(
        &self,
        _prompt: &str,
        _options: ChatOptions,
    ) -> Result<ChatResponse, NativeError> {
        Err(NativeError::InvalidConfig(
            "Inference feature not enabled. Rebuild with --features native-inference".to_string(),
        ))
    }

    async fn infer_stream(
        &self,
        _prompt: &str,
        _options: ChatOptions,
    ) -> Result<impl Stream<Item = Result<String, NativeError>> + Send, NativeError> {
        Err::<futures::stream::Empty<Result<String, NativeError>>, _>(NativeError::InvalidConfig(
            "Inference feature not enabled. Rebuild with --features native-inference".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let runtime = NativeRuntime::new();
        assert!(!runtime.is_loaded());
        assert!(runtime.model_info().is_none());
        assert!(runtime.model_path().is_none());
        assert!(!runtime.is_cancelled());
    }

    #[test]
    fn test_runtime_default() {
        let runtime = NativeRuntime::default();
        assert!(!runtime.is_loaded());
        assert!(!runtime.is_cancelled());
    }

    #[test]
    fn test_cancel_all() {
        let runtime = NativeRuntime::new();
        assert!(!runtime.is_cancelled());

        runtime.cancel_all();
        assert!(runtime.is_cancelled());
    }

    #[test]
    fn test_child_token_cancelled_with_parent() {
        let runtime = NativeRuntime::new();
        let child = runtime.child_token();

        assert!(!child.is_cancelled());

        runtime.cancel_all();

        assert!(child.is_cancelled());
    }

    #[test]
    fn test_child_token_independent() {
        let runtime = NativeRuntime::new();
        let child1 = runtime.child_token();
        let child2 = runtime.child_token();

        // Cancelling child1 should not affect child2 or parent
        child1.cancel();

        assert!(child1.is_cancelled());
        assert!(!child2.is_cancelled());
        assert!(!runtime.is_cancelled());
    }

    #[test]
    fn test_clone_shares_cancellation_token() {
        let runtime1 = NativeRuntime::new();
        let runtime2 = runtime1.clone();

        assert!(!runtime1.is_cancelled());
        assert!(!runtime2.is_cancelled());

        runtime1.cancel_all();

        assert!(runtime1.is_cancelled());
        assert!(runtime2.is_cancelled());
    }

    #[test]
    fn test_debug_includes_cancellation_state() {
        let runtime = NativeRuntime::new();
        let debug_str = format!("{:?}", runtime);
        assert!(debug_str.contains("is_cancelled"));
    }

    #[tokio::test]
    #[cfg(not(feature = "native-inference"))]
    async fn test_load_without_feature() {
        let mut runtime = NativeRuntime::new();
        let result = runtime
            .load(PathBuf::from("test.gguf"), LoadConfig::default())
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Inference feature not enabled"));
    }

    #[tokio::test]
    #[cfg(not(feature = "native-inference"))]
    async fn test_infer_without_feature() {
        let runtime = NativeRuntime::new();
        let result = runtime.infer("test", ChatOptions::default()).await;
        assert!(result.is_err());
    }

    #[cfg(feature = "native-inference")]
    mod native_inference_tests {
        use super::*;

        #[tokio::test]
        async fn test_active_task_count_starts_at_zero() {
            let runtime = NativeRuntime::new();
            assert_eq!(runtime.active_task_count(), 0);
        }

        #[tokio::test]
        async fn test_infer_stream_returns_cancelled_when_already_cancelled() {
            let runtime = NativeRuntime::new();
            runtime.cancel_all();

            let result = runtime
                .infer_stream("test prompt", ChatOptions::default())
                .await;

            // Use match instead of unwrap_err() because Stream doesn't impl Debug
            match result {
                Err(NativeError::Cancelled) => {} // Expected
                Err(other) => panic!("Expected NativeError::Cancelled, got {:?}", other),
                Ok(_) => panic!("Expected Err(Cancelled), got Ok(stream)"),
            }
        }

        #[tokio::test]
        async fn test_shutdown_cancels_token() {
            let mut runtime = NativeRuntime::new();
            assert!(!runtime.is_cancelled());

            // Shutdown should cancel the token
            let result = runtime
                .shutdown(std::time::Duration::from_millis(100))
                .await;
            assert!(result.is_ok());
            assert!(runtime.is_cancelled());
        }
    }
}
