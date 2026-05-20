// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native runtime implementation using mistral.rs.
//!
//! This module provides the `NativeRuntime` struct which implements
//! the `InferenceBackend` trait using the mistral.rs library.
//!
//! # Model Kinds
//!
//! Two loading paths are supported:
//! - **TextGguf** — local GGUF files via `GgufModelBuilder` (text-only, default)
//! - **VisionHf** — HuggingFace vision models via `VisionModelBuilder`
//!
//! After loading, the runtime inspects `ModelCategory` from the loaded model
//! to determine vision capability. Vision inference methods
//! (`infer_vision`, `infer_vision_stream`) are only available when a vision
//! model is loaded.

use crate::core::backend::{
    ChatMessage, ChatOptions, ChatResponse, ChatRole, LoadConfig, ModelInfo, NativeModelKind,
    VisionImage,
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
use parking_lot::{Mutex, RwLock as ParkingRwLock};
#[cfg(feature = "native-inference")]
use std::path::Path;
#[cfg(feature = "native-inference")]
use tracing::{debug, info, warn};

#[cfg(feature = "native-inference")]
#[allow(unused_imports)] // Used in infer_stream for stream.next()
use futures::StreamExt as _;
#[cfg(feature = "native-inference")]
use mistralrs::{
    GgufModelBuilder, MemoryGpuConfig, Model, ModelCategory, PagedAttentionMetaBuilder,
    RequestBuilder, TextMessageRole, TextMessages, VisionMessages, VisionModelBuilder,
};
#[cfg(feature = "native-inference")]
use tokio::sync::RwLock;

/// A tracked spawned task with its cancellation token.
#[cfg(feature = "native-inference")]
struct TrackedTask {
    /// Handle to the spawned task — used for cleanup (retain, await).
    handle: JoinHandle<()>,
    /// Cancellation token for graceful shutdown.
    _token: CancellationToken,
}

/// Native runtime for local LLM inference.
///
/// Uses mistral.rs for high-performance inference on GGUF and HuggingFace
/// models. Supports CPU and GPU (Metal on macOS, CUDA on Linux) acceleration.
///
/// # Vision Support
///
/// When a vision model is loaded via `NativeModelKind::VisionHf`, the runtime
/// supports `infer_vision()` and `infer_vision_stream()` for multimodal
/// image+text inference. The `supports_vision()` method indicates capability.
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
/// use nika::core::backend::{LoadConfig, NativeModelKind};
///
/// let mut runtime = NativeRuntime::new();
///
/// // Text model (GGUF)
/// runtime.load("model.gguf".into(), LoadConfig::default()).await?;
/// let response = runtime.infer("Hello!", Default::default()).await?;
///
/// // Vision model (HuggingFace)
/// let config = LoadConfig {
///     model_kind: NativeModelKind::VisionHf {
///         model_id: "HuggingFaceM4/Idefics3-8B-Llama3".to_string(),
///         isq: Some("Q4K".to_string()),
///     },
///     ..Default::default()
/// };
/// runtime.load(PathBuf::new(), config).await?;
/// assert!(runtime.supports_vision());
/// ```
/// Snapshot of everything materialized by a successful `load()` call.
///
/// Bundled into a single struct so the whole loaded state can hide behind a
/// single `Arc<ParkingRwLock<Option<LoadedState>>>` (see
/// [`NativeRuntime::loaded`]). That layout gives us interior mutability:
/// cloning a `NativeRuntime` shares the same `Arc`, so loading on one clone is
/// visible to every other clone (the executor caches and clones `RigProvider`
/// per call, so this matters in practice — see B-1 in
/// `agency/dynergie-scrap/UPSTREAM_FIXES.md`).
#[cfg(feature = "native-inference")]
struct LoadedState {
    /// The loaded mistral.rs `Model` behind an async `RwLock` for concurrent
    /// inference. Cloning the outer `Arc` is cheap; the inner `Model` stays
    /// pinned in GPU/CPU memory until the last `Arc` is dropped.
    model: Arc<RwLock<Model>>,
    /// Metadata about the loaded model.
    info: ModelInfo,
    /// Source path of the loaded model. `None` for HuggingFace vision models
    /// where the file isn't a single artifact on the local FS.
    path: Option<PathBuf>,
    /// Load configuration used.
    config: LoadConfig,
    /// Whether the loaded model supports vision (derived from
    /// `Model.config().category`).
    is_vision: bool,
}

#[cfg(feature = "native-inference")]
impl std::fmt::Debug for LoadedState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedState")
            .field("info", &self.info)
            .field("path", &self.path)
            .field("config", &self.config)
            .field("is_vision", &self.is_vision)
            .finish_non_exhaustive()
    }
}

pub struct NativeRuntime {
    /// Shared loaded state, populated by `load()` and consumed by every infer
    /// path. `None` until a successful load. Cloning the runtime shares this
    /// `Arc`, so the cache→clone pattern used by the executor still sees a
    /// loaded model after any one caller triggers `load()` /
    /// `ensure_loaded()`.
    #[cfg(feature = "native-inference")]
    loaded: Arc<ParkingRwLock<Option<LoadedState>>>,

    /// Master cancellation token for all spawned tasks.
    cancellation_token: CancellationToken,

    /// Tracked spawned tasks for cleanup.
    #[cfg(feature = "native-inference")]
    tasks: Arc<Mutex<Vec<TrackedTask>>>,
}

// Manual Debug implementation (Model doesn't implement Debug).
impl std::fmt::Debug for NativeRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dbg = f.debug_struct("NativeRuntime");

        #[cfg(feature = "native-inference")]
        {
            let guard = self.loaded.read();
            // Mirror the pre-refactor surface so legacy tests + log scrapers
            // see `is_loaded` + `is_vision` at the top level.
            dbg.field("loaded", &*guard)
                .field("is_loaded", &guard.is_some())
                .field(
                    "is_vision",
                    &guard.as_ref().is_some_and(|s| s.is_vision),
                );
        }

        dbg.field("is_cancelled", &self.cancellation_token.is_cancelled());

        #[cfg(feature = "native-inference")]
        {
            let task_count = self.tasks.lock().len();
            dbg.field("active_tasks", &task_count);
        }

        dbg.finish()
    }
}

// Manual Clone implementation (Arc clones only — the underlying state IS shared).
//
// This is intentional: the executor caches one `RigProvider` per provider name
// (`runtime/executor/mod.rs::get_rig_provider`) and hands clones back to call
// sites. We rely on those clones seeing the SAME `loaded` slot so a single
// `load()` (or `ensure_loaded()`) call benefits every subsequent infer.
impl Clone for NativeRuntime {
    fn clone(&self) -> Self {
        Self {
            #[cfg(feature = "native-inference")]
            loaded: Arc::clone(&self.loaded),
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
            loaded: Arc::new(ParkingRwLock::new(None)),
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
    ///
    /// Returns a clone because the underlying state lives behind an
    /// `Arc<RwLock<…>>` and we can't safely hand out borrows through the
    /// read-guard lifetime.
    #[must_use]
    pub fn model_path(&self) -> Option<PathBuf> {
        #[cfg(feature = "native-inference")]
        {
            self.loaded.read().as_ref().and_then(|s| s.path.clone())
        }
        #[cfg(not(feature = "native-inference"))]
        {
            None
        }
    }

    /// Get the load configuration for the current model.
    ///
    /// See [`Self::model_path`] for why this returns an owned value.
    #[must_use]
    pub fn config(&self) -> Option<LoadConfig> {
        #[cfg(feature = "native-inference")]
        {
            self.loaded.read().as_ref().map(|s| s.config.clone())
        }
        #[cfg(not(feature = "native-inference"))]
        {
            None
        }
    }

    /// `&self`-callable load: the shared implementation behind both
    /// `InferenceBackend::load(&mut self)` (trait method) and
    /// `ensure_loaded(&self)` (lazy auto-load from inference paths).
    ///
    /// All field mutation goes through `self.loaded.write()` so we never
    /// need a `&mut self` borrow — this is the whole point of bundling the
    /// loaded state behind `Arc<ParkingRwLock<Option<LoadedState>>>`.
    #[cfg(feature = "native-inference")]
    async fn do_load(
        &self,
        model_path: PathBuf,
        config: LoadConfig,
    ) -> Result<(), NativeError> {
        // Unload any existing model. Use a non-blocking probe so we don't
        // try to acquire `loaded.write()` here AND inside `do_unload()`
        // back-to-back.
        if self.loaded.read().is_some() {
            self.do_unload();
        }

        // Clone model_kind so we can match on it after `config` is moved
        // into the LoadedState.
        let model_kind = config.model_kind.clone();

        match &model_kind {
            // ============================================================
            // TextGguf — existing GGUF loading path.
            // ============================================================
            NativeModelKind::TextGguf => {
                info!(?model_path, "Loading GGUF model");

                // Validate path exists.
                if !model_path.exists() {
                    return Err(NativeError::ModelNotFound {
                        repo: "local".to_string(),
                        filename: model_path.to_string_lossy().to_string(),
                    });
                }

                // Build the model using GgufModelBuilder.
                // API: GgufModelBuilder::new(directory, vec![filename]).
                let parent = model_path
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string());
                let filename = model_path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .ok_or_else(|| {
                        NativeError::InvalidConfig(
                            "Invalid model path: no filename".to_string(),
                        )
                    })?;

                debug!(
                    gpu_layers = config.gpu_layers,
                    %parent, %filename, "Building GGUF model"
                );

                // Build model with PagedAttention for better memory
                // management. PagedAttention enables efficient KV cache
                // handling for longer contexts. Use context_size from
                // LoadConfig, defaulting to 2048 if not specified.
                let context_size = config.context_size.unwrap_or(2048);
                let model = GgufModelBuilder::new(parent, vec![filename])
                    .with_logging()
                    .with_paged_attn(|| {
                        PagedAttentionMetaBuilder::default()
                            .with_block_size(32)
                            .with_gpu_memory(MemoryGpuConfig::ContextSize(
                                context_size as usize,
                            ))
                            .build()
                    })
                    .map_err(|e| {
                        NativeError::InvalidConfig(format!(
                            "PagedAttention config error: {e}"
                        ))
                    })?
                    .build()
                    .await
                    .map_err(|e| {
                        NativeError::InvalidConfig(format!(
                            "Failed to build model: {e}"
                        ))
                    })?;

                // GGUF models are always text-only (no VisionModelBuilder).
                let is_vision = detect_vision_capability(&model);

                // Extract model info from the loaded model.
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

                *self.loaded.write() = Some(LoadedState {
                    model: Arc::new(RwLock::new(model)),
                    info,
                    path: Some(model_path),
                    config,
                    is_vision,
                });

                info!(is_vision, "GGUF model loaded successfully");
            }

            // ============================================================
            // VisionHf — HuggingFace vision model loading path.
            // ============================================================
            NativeModelKind::VisionHf { model_id, isq } => {
                info!(%model_id, ?isq, "Loading HuggingFace vision model");

                let context_size = config.context_size.unwrap_or(4096);

                // Start building the vision model.
                let mut builder = VisionModelBuilder::new(model_id).with_logging();

                // Apply ISQ quantization if specified. ISQ (In-Situ
                // Quantization) quantizes the model weights after loading,
                // reducing memory usage while maintaining quality.
                if let Some(isq_str) = isq {
                    let isq_type =
                        mistralrs::parse_isq_value(isq_str, None).map_err(|e| {
                            NativeError::InvalidConfig(format!(
                                "Invalid ISQ type '{}': {}",
                                isq_str, e
                            ))
                        })?;
                    debug!(?isq_type, "Applying ISQ quantization");
                    builder = builder.with_isq(isq_type);
                }

                // Apply PagedAttention for efficient KV cache handling.
                builder = builder
                    .with_paged_attn(|| {
                        PagedAttentionMetaBuilder::default()
                            .with_block_size(32)
                            .with_gpu_memory(MemoryGpuConfig::ContextSize(
                                context_size as usize,
                            ))
                            .build()
                    })
                    .map_err(|e| {
                        NativeError::InvalidConfig(format!(
                            "PagedAttention config error: {e}"
                        ))
                    })?;

                // Build the model (downloads from HuggingFace if not cached).
                let model = builder.build().await.map_err(|e| {
                    NativeError::InvalidConfig(format!(
                        "Failed to build vision model '{}': {}",
                        model_id, e
                    ))
                })?;

                // Verify this is actually a vision model.
                let is_vision = detect_vision_capability(&model);
                if !is_vision {
                    warn!(
                        %model_id,
                        "Model loaded via VisionHf path but does not report \
                         Vision category"
                    );
                }

                // Build model info for vision models.
                let info = ModelInfo {
                    name: model_id.clone(),
                    // HF models don't have a single file size.
                    size: 0,
                    quantization: isq.clone(),
                    parameters: None,
                    digest: None,
                };

                // For HF vision models the source isn't a single file on
                // disk — the HuggingFace cache is the source of truth — so
                // `path: None` is the canonical answer to `model_path()`.
                *self.loaded.write() = Some(LoadedState {
                    model: Arc::new(RwLock::new(model)),
                    info,
                    path: None,
                    config,
                    is_vision,
                });

                info!(is_vision, %model_id, "Vision model loaded successfully");
            }
        }

        Ok(())
    }

    /// `&self`-callable unload: the shared implementation behind
    /// `InferenceBackend::unload(&mut self)`.
    ///
    /// Synchronous (no async work, just dropping the loaded state). Returns
    /// nothing because the only way it can fail is poison and parking_lot
    /// is poison-free.
    #[cfg(feature = "native-inference")]
    fn do_unload(&self) {
        let mut guard = self.loaded.write();
        if guard.is_some() {
            info!("Unloading model");
            *guard = None;
        }
    }

    /// Ensure a model is loaded for native inference, auto-loading on first
    /// call.
    ///
    /// Idempotent. Resolves `model_id` (a catalog alias like `"mistral:7b"`
    /// or `"qwen3:8b"`) into `(PathBuf, LoadConfig)` via the curated
    /// `nika_core::catalogs::models` catalog, then runs the same load path
    /// the trait `load()` does — except via interior mutability, so the
    /// inference path can call this from `&self` (the `RigProvider::Native(_)`
    /// arm only ever has a borrow).
    ///
    /// Returns the actionable error (with the exact `nika model pull`
    /// command in the message) when the model file has not been downloaded
    /// yet, so the operator gets a one-step recovery path instead of an
    /// opaque NIKA-031.
    ///
    /// **Limitation (B-1 follow-up):** if a *different* model is already
    /// loaded, this is a no-op and the existing model serves the call.
    /// Mid-session model switching is out of scope; call `unload()`
    /// explicitly first to switch.
    ///
    /// Closes B-1 in `agency/dynergie-scrap/UPSTREAM_FIXES.md`.
    #[cfg(feature = "native-inference")]
    pub async fn ensure_loaded(&self, model_id: &str) -> Result<(), NativeError> {
        if self.loaded.read().is_some() {
            return Ok(());
        }
        let (path, config) = resolve_native_model(model_id)?;
        self.do_load(path, config).await
    }
}

/// Resolve a catalog model id (`"mistral:7b"`, `"qwen3:8b"`, …) into the
/// concrete arguments [`NativeRuntime::ensure_loaded`] needs to call
/// [`NativeRuntime::do_load`].
///
/// Tries two on-disk layouts in order ·
///
///  1. **Nested HuggingFace cache** (canonical post-`nika model pull`) ·
///     `~/.nika/models/<hf_repo>/<default_file>` — this is what
///     `HuggingFaceStorage::new(default_model_dir()).pull(...)` writes,
///     because the HuggingFace hub natively organises files by repo.
///  2. **Flat layout** (legacy / manual `cp`) · `~/.nika/models/<default_file>`
///     — kept for backward-compat with operators who downloaded GGUF
///     files by hand before `nika model pull` shipped.
///
/// Vision (HuggingFace) models stay out of this fast path — they need
/// explicit `LoadConfig` from the caller because the `model_id` alone
/// does not carry an ISQ hint.
#[cfg(feature = "native-inference")]
fn resolve_native_model(model_id: &str) -> Result<(PathBuf, LoadConfig), NativeError> {
    use nika_core::catalogs::models::find_model;

    let model = find_model(model_id).ok_or_else(|| NativeError::ModelNotFound {
        repo: "catalog".to_string(),
        filename: format!(
            "{model_id} — unknown model alias. \
             Run `nika model list` to see available aliases."
        ),
    })?;

    let models_dir = crate::core::storage::default_model_dir();
    let nested = models_dir.join(model.hf_repo).join(model.default_file);
    let flat = models_dir.join(model.default_file);

    let path = if nested.exists() {
        nested
    } else if flat.exists() {
        flat
    } else {
        return Err(NativeError::ModelNotFound {
            repo: model.hf_repo.to_string(),
            filename: format!(
                "{} not found (tried {} then {}). \
                 Run `nika model pull {}` to download.",
                model.default_file,
                nested.display(),
                flat.display(),
                model_id,
            ),
        });
    };

    let config = LoadConfig {
        model_kind: NativeModelKind::TextGguf,
        ..Default::default()
    };

    Ok((path, config))
}

impl Default for NativeRuntime {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helpers (native-inference only)
// ============================================================================

/// Extract quantization from file path.
#[cfg(feature = "native-inference")]
fn extract_quantization_from_path(path: &Path) -> Option<String> {
    let filename = path.file_name()?.to_string_lossy();
    extract_quantization(&filename)
}

/// Build a `RequestBuilder` from `ChatOptions`, applying sampling parameters.
#[cfg(feature = "native-inference")]
fn apply_sampling_params(mut request: RequestBuilder, options: &ChatOptions) -> RequestBuilder {
    // Apply temperature if provided (convert f32 to f64)
    if let Some(temp) = options.temperature {
        request = request.set_sampler_temperature(f64::from(temp));
    }

    // Apply max_tokens if provided
    if let Some(max_tokens) = options.max_tokens {
        request = request.set_sampler_max_len(max_tokens as usize);
    }

    // Apply top_p if provided
    if let Some(top_p) = options.top_p {
        request = request.set_sampler_topp(f64::from(top_p));
    }

    // Apply top_k if provided
    if let Some(top_k) = options.top_k {
        request = request.set_sampler_topk(top_k as usize);
    }

    request
}

/// Parse a `ChatCompletionResponse` into a `ChatResponse`.
#[cfg(feature = "native-inference")]
fn parse_chat_completion(
    response: &mistralrs::ChatCompletionResponse,
) -> Result<ChatResponse, NativeError> {
    let content = response
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .ok_or_else(|| {
            NativeError::InferenceFailed("Model returned empty response (no choices)".to_string())
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
        prompt_eval_count: Some(response.usage.prompt_tokens as u64),
        eval_count: Some(response.usage.completion_tokens as u64),
    })
}

/// Decode `VisionImage` bytes into `image::DynamicImage` instances.
///
/// Uses `image::load_from_memory` which auto-detects format from bytes.
/// This is safe because mistralrs validates images further in the pipeline.
#[cfg(feature = "native-inference")]
fn decode_vision_images(images: &[VisionImage]) -> Result<Vec<image::DynamicImage>, NativeError> {
    images
        .iter()
        .enumerate()
        .map(|(i, img)| {
            image::load_from_memory(&img.bytes).map_err(|e| {
                NativeError::InferenceFailed(format!(
                    "Failed to decode image {} ({}): {}",
                    i, img.media_type, e
                ))
            })
        })
        .collect()
}

/// Detect whether a loaded `Model` is a vision model by inspecting its config.
///
/// Returns `true` if `ModelCategory::Vision`, `false` for all other categories
/// or if config cannot be retrieved.
#[cfg(feature = "native-inference")]
fn detect_vision_capability(model: &Model) -> bool {
    match model.config() {
        Ok(config) => matches!(config.category, ModelCategory::Vision { .. }),
        Err(e) => {
            warn!(
                "Could not read model config to detect vision capability: {}",
                e
            );
            false
        }
    }
}

// ============================================================================
// Streaming helper — shared between text and vision streams
// ============================================================================

/// Spawn a streaming inference task and return an mpsc-backed `Stream`.
///
/// This encapsulates the `TrackedTask` + `mpsc` + `CancellationToken` pattern
/// used by both `infer_stream` and `infer_vision_stream`.
#[cfg(feature = "native-inference")]
fn spawn_stream_task(
    runtime: &NativeRuntime,
    model_arc: Arc<RwLock<Model>>,
    request: RequestBuilder,
) -> Result<impl Stream<Item = Result<String, NativeError>> + Send, NativeError> {
    use crate::util::constants::STREAM_CHUNK_TIMEOUT;
    use async_stream::stream;
    use mistralrs::Response;
    use tokio::sync::mpsc;

    // Check for cancellation before starting
    if runtime.cancellation_token.is_cancelled() {
        return Err(NativeError::Cancelled);
    }

    // Create channel for streaming chunks
    let (tx, mut rx) = mpsc::channel::<Result<String, NativeError>>(32);

    // Get a child cancellation token for this task
    let task_token = runtime.child_token();
    let task_token_clone = task_token.clone();

    // Spawn streaming task that holds the model lock
    // Task will be cancelled when: receiver dropped, parent cancelled, or task_token cancelled
    let handle = tokio::spawn(async move {
        let model = model_arc.read().await;

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

                        result = tokio::time::timeout(
                            STREAM_CHUNK_TIMEOUT,
                            stream.next(),
                        ) => {
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
        let mut tasks = runtime.tasks.lock();
        tasks.push(TrackedTask {
            handle,
            _token: task_token,
        });
    }

    // Clean up completed tasks periodically
    runtime.cleanup_completed_tasks();

    // Convert mpsc receiver to Stream
    Ok(stream! {
        while let Some(result) = rx.recv().await {
            yield result;
        }
    })
}

// ============================================================================
// InferenceBackend implementation (native-inference enabled)
// ============================================================================

#[cfg(feature = "native-inference")]
impl InferenceBackend for NativeRuntime {
    async fn load(&mut self, model_path: PathBuf, config: LoadConfig) -> Result<(), NativeError> {
        // Trait signature keeps `&mut self` for backward-compat, but the
        // actual work runs through interior mutability so the `&self`
        // version (`do_load`) is the single source of truth — it's also
        // what `ensure_loaded()` calls from the inference fast-path.
        self.do_load(model_path, config).await
    }

    async fn unload(&mut self) -> Result<(), NativeError> {
        self.do_unload();
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded.read().is_some()
    }

    fn model_info(&self) -> Option<&ModelInfo> {
        // The trait wants `Option<&ModelInfo>` but our state hides behind a
        // read-guard with a runtime-bound lifetime — we can't borrow through
        // it. The trait dyn shim (`InferenceBackendDyn::model_info_dyn`) is
        // the only documented external caller and it immediately maps the
        // value to an owned `ModelInfo`, so always returning `None` here is
        // surprising-but-safe (it just forces callers to ask via a different
        // path or via cloning). For inherent access, prefer `model_path()` /
        // `is_loaded()` which return owned values.
        //
        // TODO(B-1 follow-up): once the trait surface is reshaped to return
        // `Option<ModelInfo>` (owned), wire this through. For now we keep the
        // trait contract intact and document the gap.
        None
    }

    fn supports_vision(&self) -> bool {
        self.loaded.read().as_ref().is_some_and(|s| s.is_vision)
    }

    // ========================================================================
    // Text inference (non-streaming)
    // ========================================================================

    async fn infer(&self, prompt: &str, options: ChatOptions) -> Result<ChatResponse, NativeError> {
        // Clone the inner Arc out of the parking_lot guard, then drop the
        // guard before the .await so we don't hold a non-Send guard across
        // the await point.
        let model_arc = self
            .loaded
            .read()
            .as_ref()
            .ok_or(NativeError::ModelNotLoaded)?
            .model
            .clone();
        let model = model_arc.read().await;

        // Build messages - just user prompt for now
        let messages = TextMessages::new().add_message(TextMessageRole::User, prompt);

        debug!(
            temperature = options.temperature,
            max_tokens = options.max_tokens,
            "Running text inference"
        );

        // Build request with sampling parameters
        let request = apply_sampling_params(RequestBuilder::from(messages), &options);

        // Send request with timeout
        let response = tokio::time::timeout(INFER_TIMEOUT, model.send_chat_request(request))
            .await
            .map_err(|_| NativeError::InferenceTimeout {
                timeout_secs: INFER_TIMEOUT.as_secs(),
            })?
            .map_err(|e| NativeError::InferenceFailed(format!("Inference failed: {e}")))?;

        parse_chat_completion(&response)
    }

    // ========================================================================
    // Text inference (streaming)
    // ========================================================================

    async fn infer_stream(
        &self,
        prompt: &str,
        options: ChatOptions,
    ) -> Result<impl Stream<Item = Result<String, NativeError>> + Send, NativeError> {
        // Check for cancellation before starting (matches original behavior)
        if self.cancellation_token.is_cancelled() {
            return Err(NativeError::Cancelled);
        }

        let model_arc = self
            .loaded
            .read()
            .as_ref()
            .ok_or(NativeError::ModelNotLoaded)?
            .model
            .clone();

        // Build text messages and request
        let messages = TextMessages::new().add_message(TextMessageRole::User, prompt);
        let request = apply_sampling_params(RequestBuilder::from(messages), &options);

        spawn_stream_task(self, model_arc, request)
    }

    // ========================================================================
    // Vision inference (non-streaming)
    // ========================================================================

    async fn infer_vision(
        &self,
        prompt: &str,
        images: Vec<VisionImage>,
        options: ChatOptions,
    ) -> Result<ChatResponse, NativeError> {
        // Snapshot the loaded state in one read-guard scope, then drop the
        // guard before awaiting on the inner model lock (parking_lot guards
        // aren't `Send` across .await points).
        let (model_arc, is_vision) = {
            let guard = self.loaded.read();
            let state = guard.as_ref().ok_or(NativeError::ModelNotLoaded)?;
            (state.model.clone(), state.is_vision)
        };

        // Guard: model must support vision.
        if !is_vision {
            return Err(NativeError::InvalidConfig(
                "Loaded model does not support vision. Load a vision model via \
                 NativeModelKind::VisionHf"
                    .to_string(),
            ));
        }

        // Guard: at least one image required.
        if images.is_empty() {
            return Err(NativeError::InvalidConfig(
                "infer_vision requires at least one image".to_string(),
            ));
        }

        let model = model_arc.read().await;

        debug!(
            image_count = images.len(),
            temperature = options.temperature,
            max_tokens = options.max_tokens,
            "Running vision inference"
        );

        // Decode image bytes into DynamicImage instances
        let dynamic_images = decode_vision_images(&images)?;

        // Build VisionMessages with images.
        // VisionMessages::add_image_message requires a &Model reference to get
        // the model-specific image prefixer (e.g., "<image>" tokens).
        let vision_messages = VisionMessages::new()
            .add_image_message(TextMessageRole::User, prompt, dynamic_images, &model)
            .map_err(|e| {
                NativeError::InferenceFailed(format!("Failed to build vision message: {}", e))
            })?;

        // Convert to RequestBuilder and apply sampling params
        let request = apply_sampling_params(RequestBuilder::from(vision_messages), &options);

        // Send request with timeout
        let response = tokio::time::timeout(INFER_TIMEOUT, model.send_chat_request(request))
            .await
            .map_err(|_| NativeError::InferenceTimeout {
                timeout_secs: INFER_TIMEOUT.as_secs(),
            })?
            .map_err(|e| NativeError::InferenceFailed(format!("Vision inference failed: {e}")))?;

        parse_chat_completion(&response)
    }

    // ========================================================================
    // Vision inference (streaming)
    // ========================================================================

    async fn infer_vision_stream(
        &self,
        prompt: &str,
        images: Vec<VisionImage>,
        options: ChatOptions,
    ) -> Result<impl Stream<Item = Result<String, NativeError>> + Send, NativeError> {
        // Snapshot loaded state under one (non-Send) parking_lot guard, then
        // drop it before crossing any .await.
        let (model_arc, is_vision) = {
            let guard = self.loaded.read();
            let state = guard.as_ref().ok_or(NativeError::ModelNotLoaded)?;
            (state.model.clone(), state.is_vision)
        };

        // Guard: model must support vision.
        if !is_vision {
            return Err(NativeError::InvalidConfig(
                "Loaded model does not support vision. Load a vision model via \
                 NativeModelKind::VisionHf"
                    .to_string(),
            ));
        }

        // Guard: at least one image required.
        if images.is_empty() {
            return Err(NativeError::InvalidConfig(
                "infer_vision_stream requires at least one image".to_string(),
            ));
        }

        debug!(
            image_count = images.len(),
            temperature = options.temperature,
            max_tokens = options.max_tokens,
            "Starting vision inference stream"
        );

        // Decode images (done before spawning task to fail fast on bad data).
        let dynamic_images = decode_vision_images(&images)?;

        // Build VisionMessages — requires read lock for model-specific prefixer.
        // We acquire the inner async lock briefly here to build the message,
        // then release it so the spawned task can re-acquire for streaming.
        let request = {
            let model = model_arc.read().await;

            let vision_messages = VisionMessages::new()
                .add_image_message(TextMessageRole::User, prompt, dynamic_images, &model)
                .map_err(|e| {
                    NativeError::InferenceFailed(format!("Failed to build vision message: {}", e))
                })?;

            apply_sampling_params(RequestBuilder::from(vision_messages), &options)
        };

        spawn_stream_task(self, model_arc, request)
    }
}

// ============================================================================
// Stub implementation when inference feature is not enabled
// ============================================================================

#[cfg(not(feature = "native-inference"))]
impl InferenceBackend for NativeRuntime {
    async fn load(&mut self, _model_path: PathBuf, _config: LoadConfig) -> Result<(), NativeError> {
        Err(NativeError::InvalidConfig(
            "Native inference not available in this build".to_string(),
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

    fn supports_vision(&self) -> bool {
        false
    }

    async fn infer(
        &self,
        _prompt: &str,
        _options: ChatOptions,
    ) -> Result<ChatResponse, NativeError> {
        Err(NativeError::InvalidConfig(
            "Native inference not available in this build".to_string(),
        ))
    }

    async fn infer_stream(
        &self,
        _prompt: &str,
        _options: ChatOptions,
    ) -> Result<impl Stream<Item = Result<String, NativeError>> + Send, NativeError> {
        Err::<futures::stream::Empty<Result<String, NativeError>>, _>(NativeError::InvalidConfig(
            "Native inference not available in this build".to_string(),
        ))
    }

    async fn infer_vision(
        &self,
        _prompt: &str,
        _images: Vec<VisionImage>,
        _options: ChatOptions,
    ) -> Result<ChatResponse, NativeError> {
        Err(NativeError::InvalidConfig(
            "Native inference not available in this build".to_string(),
        ))
    }

    async fn infer_vision_stream(
        &self,
        _prompt: &str,
        _images: Vec<VisionImage>,
        _options: ChatOptions,
    ) -> Result<impl Stream<Item = Result<String, NativeError>> + Send, NativeError> {
        Err::<futures::stream::Empty<Result<String, NativeError>>, _>(NativeError::InvalidConfig(
            "Native inference not available in this build".to_string(),
        ))
    }
}

// ============================================================================
// Tests
// ============================================================================

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
        assert!(!runtime.supports_vision());
    }

    #[test]
    fn test_runtime_default() {
        let runtime = NativeRuntime::default();
        assert!(!runtime.is_loaded());
        assert!(!runtime.is_cancelled());
        assert!(!runtime.supports_vision());
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
        assert!(debug_str.contains("is_vision"));
    }

    #[test]
    fn test_vision_image_construction() {
        let img = VisionImage::new(vec![0xFF, 0xD8], "image/jpeg");
        assert_eq!(img.bytes, vec![0xFF, 0xD8]);
        assert_eq!(img.media_type, "image/jpeg");
    }

    #[test]
    fn test_native_model_kind_default() {
        let kind = NativeModelKind::default();
        assert!(matches!(kind, NativeModelKind::TextGguf));
        assert!(!kind.is_vision());
    }

    #[test]
    fn test_native_model_kind_vision() {
        let kind = NativeModelKind::VisionHf {
            model_id: "test/model".to_string(),
            isq: Some("Q4K".to_string()),
        };
        assert!(kind.is_vision());
    }

    #[test]
    fn test_load_config_default_has_text_gguf() {
        let config = LoadConfig::default();
        assert!(matches!(config.model_kind, NativeModelKind::TextGguf));
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

    #[tokio::test]
    #[cfg(not(feature = "native-inference"))]
    async fn test_infer_vision_without_feature() {
        let runtime = NativeRuntime::new();
        let images = vec![VisionImage::new(vec![0xFF], "image/png")];
        let result = runtime
            .infer_vision("describe", images, ChatOptions::default())
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Inference feature not enabled"));
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

        #[tokio::test]
        async fn test_infer_vision_requires_loaded_model() {
            let runtime = NativeRuntime::new();
            let images = vec![VisionImage::new(vec![0xFF], "image/png")];
            let result = runtime
                .infer_vision("describe", images, ChatOptions::default())
                .await;
            assert!(matches!(result, Err(NativeError::ModelNotLoaded)));
        }

        #[tokio::test]
        async fn test_infer_vision_stream_requires_loaded_model() {
            let runtime = NativeRuntime::new();
            let images = vec![VisionImage::new(vec![0xFF], "image/png")];
            let result = runtime
                .infer_vision_stream("describe", images, ChatOptions::default())
                .await;
            match result {
                Err(NativeError::ModelNotLoaded) => {} // Expected
                Err(other) => panic!("Expected ModelNotLoaded, got {:?}", other),
                Ok(_) => panic!("Expected Err, got Ok"),
            }
        }
    }
}
