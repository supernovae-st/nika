// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Inference backend traits.
//!
//! Defines the interface for local model inference backends.
//!

use crate::core::backend::{ChatOptions, ChatResponse, LoadConfig, ModelInfo, VisionImage};
use crate::provider::native::NativeError;
use futures::Stream;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

/// Trait for any inference backend (mistral.rs, llama.cpp, etc.).
///
/// This trait provides a unified interface for loading and running
/// local LLM inference. Implementations can use different backends
/// while presenting the same API to consumers.
pub trait InferenceBackend: Send + Sync {
    /// Load a model from disk.
    ///
    /// # Arguments
    /// * `model_path` - Path to the GGUF model file
    /// * `config` - Load configuration (context size, GPU layers, etc.)
    ///
    /// # Returns
    /// `Ok(())` if the model was loaded successfully.
    fn load(
        &mut self,
        model_path: PathBuf,
        config: LoadConfig,
    ) -> impl Future<Output = Result<(), NativeError>> + Send;

    /// Unload the model from memory.
    ///
    /// Frees GPU/CPU memory used by the model.
    fn unload(&mut self) -> impl Future<Output = Result<(), NativeError>> + Send;

    /// Check if a model is currently loaded.
    #[must_use]
    fn is_loaded(&self) -> bool;

    /// Get metadata about the loaded model.
    ///
    /// Returns `None` if no model is loaded.
    fn model_info(&self) -> Option<&ModelInfo>;

    /// Generate a response (non-streaming).
    ///
    /// # Arguments
    /// * `prompt` - The input prompt
    /// * `options` - Generation options (temperature, max_tokens, etc.)
    ///
    /// # Returns
    /// The complete chat response.
    fn infer(
        &self,
        prompt: &str,
        options: ChatOptions,
    ) -> impl Future<Output = Result<ChatResponse, NativeError>> + Send;

    /// Generate a response (streaming).
    ///
    /// Returns a stream of token strings as they are generated.
    ///
    /// # Arguments
    /// * `prompt` - The input prompt
    /// * `options` - Generation options (temperature, max_tokens, etc.)
    fn infer_stream(
        &self,
        prompt: &str,
        options: ChatOptions,
    ) -> impl Future<
        Output = Result<impl Stream<Item = Result<String, NativeError>> + Send, NativeError>,
    > + Send;

    /// Check if the loaded model supports vision (multimodal image input).
    ///
    /// Returns `false` if no model is loaded or the model is text-only.
    #[must_use]
    fn supports_vision(&self) -> bool;

    /// Generate a vision response (non-streaming).
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to accompany the images
    /// * `images` - Images to send to the vision model
    /// * `options` - Generation options (temperature, max_tokens, etc.)
    ///
    /// # Errors
    /// Returns `NativeError::InvalidConfig` if the model does not support vision.
    fn infer_vision(
        &self,
        prompt: &str,
        images: Vec<VisionImage>,
        options: ChatOptions,
    ) -> impl Future<Output = Result<ChatResponse, NativeError>> + Send;

    /// Generate a vision response (streaming).
    ///
    /// Returns a stream of token strings as they are generated.
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to accompany the images
    /// * `images` - Images to send to the vision model
    /// * `options` - Generation options (temperature, max_tokens, etc.)
    ///
    /// # Errors
    /// Returns `NativeError::InvalidConfig` if the model does not support vision.
    fn infer_vision_stream(
        &self,
        prompt: &str,
        images: Vec<VisionImage>,
        options: ChatOptions,
    ) -> impl Future<
        Output = Result<impl Stream<Item = Result<String, NativeError>> + Send, NativeError>,
    > + Send;
}

/// Object-safe version of InferenceBackend for dynamic dispatch.
///
/// Use this when you need runtime polymorphism (e.g., `Box<dyn DynInferenceBackend>`).
///
/// # Limitations
///
/// **Streaming is NOT supported via this trait.** The `infer_stream_dyn` method
/// always returns an error because Rust's type system cannot express a boxed
/// stream that borrows from `self` (required for object safety).
///
/// For streaming inference, use the concrete [`InferenceBackend`] trait directly.
///
/// # Notes
///
/// This trait takes owned `String` instead of `&str` for prompts
/// to enable object-safe async methods.
#[allow(clippy::type_complexity)]
pub trait DynInferenceBackend: Send + Sync {
    /// Load a model from disk (boxed future for object safety).
    fn load_dyn(
        &mut self,
        model_path: PathBuf,
        config: LoadConfig,
    ) -> Pin<Box<dyn Future<Output = Result<(), NativeError>> + Send + '_>>;

    /// Unload the model from memory (boxed future for object safety).
    fn unload_dyn(&mut self) -> Pin<Box<dyn Future<Output = Result<(), NativeError>> + Send + '_>>;

    /// Check if a model is currently loaded.
    #[must_use]
    fn is_loaded_dyn(&self) -> bool;

    /// Get metadata about the loaded model (cloned for object safety).
    fn model_info_dyn(&self) -> Option<ModelInfo>;

    /// Generate a response (boxed future for object safety).
    ///
    /// Takes owned `String` instead of `&str` for object safety.
    fn infer_dyn(
        &self,
        prompt: String,
        options: ChatOptions,
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, NativeError>> + Send + '_>>;

    /// Check if the loaded model supports vision.
    #[must_use]
    fn supports_vision_dyn(&self) -> bool;

    /// Generate a vision response (boxed future for object safety).
    ///
    /// Takes owned `String` instead of `&str` for object safety.
    fn infer_vision_dyn(
        &self,
        prompt: String,
        images: Vec<VisionImage>,
        options: ChatOptions,
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, NativeError>> + Send + '_>>;

    /// Generate a streaming response (boxed stream for object safety).
    ///
    /// # Important: Streaming Not Supported via Dynamic Dispatch
    ///
    /// **This method always returns `Err(NativeError::InvalidConfig(...))`.**
    ///
    /// The streaming API cannot be made object-safe because the stream type
    /// returned by `InferenceBackend::infer_stream()` borrows from `self`,
    /// which cannot be expressed in a `Pin<Box<dyn Stream + 'static>>` return type.
    ///
    /// ## Workaround
    ///
    /// Use `InferenceBackend` directly instead of `dyn DynInferenceBackend`:
    ///
    /// ```ignore
    /// // Instead of:
    /// let backend: Box<dyn DynInferenceBackend> = Box::new(NativeRuntime::new());
    /// let stream = backend.infer_stream_dyn("prompt".into(), opts).await?;  // Always fails!
    ///
    /// // Use concrete type:
    /// let mut backend = NativeRuntime::new();
    /// backend.load(path, config).await?;
    /// let stream = backend.infer_stream("prompt", opts).await?;
    /// ```
    ///
    /// Or collect streaming results to a Vec before using dynamic dispatch.
    ///
    /// Takes owned `String` instead of `&str` for object safety.
    fn infer_stream_dyn(
        &self,
        prompt: String,
        options: ChatOptions,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Pin<Box<dyn Stream<Item = Result<String, NativeError>> + Send + 'static>>,
                        NativeError,
                    >,
                > + Send
                + '_,
        >,
    >;
}

/// Blanket implementation of DynInferenceBackend for any InferenceBackend.
impl<T: InferenceBackend + 'static> DynInferenceBackend for T {
    fn load_dyn(
        &mut self,
        model_path: PathBuf,
        config: LoadConfig,
    ) -> Pin<Box<dyn Future<Output = Result<(), NativeError>> + Send + '_>> {
        Box::pin(self.load(model_path, config))
    }

    fn unload_dyn(&mut self) -> Pin<Box<dyn Future<Output = Result<(), NativeError>> + Send + '_>> {
        Box::pin(self.unload())
    }

    fn is_loaded_dyn(&self) -> bool {
        InferenceBackend::is_loaded(self)
    }

    fn model_info_dyn(&self) -> Option<ModelInfo> {
        InferenceBackend::model_info(self).cloned()
    }

    fn infer_dyn(
        &self,
        prompt: String,
        options: ChatOptions,
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, NativeError>> + Send + '_>> {
        Box::pin(async move { self.infer(&prompt, options).await })
    }

    fn supports_vision_dyn(&self) -> bool {
        InferenceBackend::supports_vision(self)
    }

    fn infer_vision_dyn(
        &self,
        prompt: String,
        images: Vec<VisionImage>,
        options: ChatOptions,
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, NativeError>> + Send + '_>> {
        Box::pin(async move { self.infer_vision(&prompt, images, options).await })
    }

    fn infer_stream_dyn(
        &self,
        _prompt: String,
        _options: ChatOptions,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Pin<Box<dyn Stream<Item = Result<String, NativeError>> + Send + 'static>>,
                        NativeError,
                    >,
                > + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            // We cannot easily box a stream that borrows from self,
            // so for streaming, callers should use InferenceBackend directly
            // or collect results into a Vec first
            Err(NativeError::InvalidConfig(
                "Streaming not supported via DynInferenceBackend. Use InferenceBackend directly."
                    .to_string(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::backend::{ChatMessage, ChatRole};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Mock backend for testing the DynInferenceBackend blanket impl.
    struct MockBackend {
        loaded: AtomicBool,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                loaded: AtomicBool::new(false),
            }
        }

        fn new_loaded() -> Self {
            Self {
                loaded: AtomicBool::new(true),
            }
        }
    }

    impl InferenceBackend for MockBackend {
        async fn load(
            &mut self,
            _model_path: PathBuf,
            _config: LoadConfig,
        ) -> Result<(), NativeError> {
            self.loaded.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn unload(&mut self) -> Result<(), NativeError> {
            self.loaded.store(false, Ordering::SeqCst);
            Ok(())
        }

        fn is_loaded(&self) -> bool {
            self.loaded.load(Ordering::SeqCst)
        }

        fn model_info(&self) -> Option<&ModelInfo> {
            None
        }

        async fn infer(
            &self,
            prompt: &str,
            _options: ChatOptions,
        ) -> Result<ChatResponse, NativeError> {
            if !self.is_loaded() {
                return Err(NativeError::ModelNotLoaded);
            }
            Ok(ChatResponse {
                message: ChatMessage {
                    role: ChatRole::Assistant,
                    content: format!("echo: {prompt}"),
                },
                done: true,
                total_duration: Some(100),
                eval_count: Some(10),
                prompt_eval_count: Some(5),
            })
        }

        async fn infer_stream(
            &self,
            _prompt: &str,
            _options: ChatOptions,
        ) -> Result<impl Stream<Item = Result<String, NativeError>> + Send, NativeError> {
            Ok(futures::stream::once(async { Ok("token".to_string()) }))
        }

        fn supports_vision(&self) -> bool {
            false
        }

        async fn infer_vision(
            &self,
            _prompt: &str,
            _images: Vec<VisionImage>,
            _options: ChatOptions,
        ) -> Result<ChatResponse, NativeError> {
            Err(NativeError::InvalidConfig(
                "MockBackend does not support vision".to_string(),
            ))
        }

        async fn infer_vision_stream(
            &self,
            _prompt: &str,
            _images: Vec<VisionImage>,
            _options: ChatOptions,
        ) -> Result<impl Stream<Item = Result<String, NativeError>> + Send, NativeError> {
            Err::<futures::stream::Empty<Result<String, NativeError>>, _>(
                NativeError::InvalidConfig("MockBackend does not support vision".to_string()),
            )
        }
    }

    fn _assert_object_safe(_: &dyn DynInferenceBackend) {}

    #[test]
    fn is_loaded_dyn_delegates_false() {
        let backend = MockBackend::new();
        let dyn_backend: &dyn DynInferenceBackend = &backend;
        assert!(!dyn_backend.is_loaded_dyn());
    }

    #[test]
    fn is_loaded_dyn_delegates_true() {
        let backend = MockBackend::new_loaded();
        let dyn_backend: &dyn DynInferenceBackend = &backend;
        assert!(dyn_backend.is_loaded_dyn());
    }

    #[test]
    fn model_info_dyn_returns_none_when_not_loaded() {
        let backend = MockBackend::new();
        let dyn_backend: &dyn DynInferenceBackend = &backend;
        assert!(dyn_backend.model_info_dyn().is_none());
    }

    #[tokio::test]
    async fn load_dyn_delegates_to_load() {
        let mut backend = MockBackend::new();
        assert!(!backend.is_loaded());

        let result = backend
            .load_dyn(PathBuf::from("/tmp/model.gguf"), LoadConfig::default())
            .await;
        assert!(
            result.is_ok(),
            "load_dyn should succeed: {:?}",
            result.as_ref().err()
        );
        assert!(backend.is_loaded_dyn());
    }

    #[tokio::test]
    async fn unload_dyn_delegates_to_unload() {
        let mut backend = MockBackend::new_loaded();
        assert!(backend.is_loaded());

        let result = backend.unload_dyn().await;
        assert!(
            result.is_ok(),
            "unload_dyn should succeed: {:?}",
            result.as_ref().err()
        );
        assert!(!backend.is_loaded_dyn());
    }

    #[tokio::test]
    async fn infer_dyn_delegates_with_owned_string() {
        let backend = MockBackend::new_loaded();
        let dyn_backend: &dyn DynInferenceBackend = &backend;

        let result = dyn_backend
            .infer_dyn("hello world".to_string(), ChatOptions::default())
            .await;
        assert!(
            result.is_ok(),
            "infer_dyn should succeed: {:?}",
            result.as_ref().err()
        );
        let response = result.unwrap();
        assert_eq!(response.message.content, "echo: hello world");
        assert!(response.done);
    }

    #[tokio::test]
    async fn infer_dyn_propagates_error_when_not_loaded() {
        let backend = MockBackend::new();
        let dyn_backend: &dyn DynInferenceBackend = &backend;

        let result = dyn_backend
            .infer_dyn("hello".to_string(), ChatOptions::default())
            .await;
        assert!(
            result.is_err(),
            "infer_dyn should fail when not loaded but got: {:?}",
            result.as_ref().ok()
        );
        assert!(
            matches!(result.unwrap_err(), NativeError::ModelNotLoaded),
            "expected ModelNotLoaded error"
        );
    }

    #[tokio::test]
    async fn infer_stream_dyn_always_errors() {
        let backend = MockBackend::new_loaded();
        let dyn_backend: &dyn DynInferenceBackend = &backend;

        let result = dyn_backend
            .infer_stream_dyn("hello".to_string(), ChatOptions::default())
            .await;
        assert!(
            result.is_err(),
            "infer_stream_dyn should always fail but got Ok(_)"
        );
        match result {
            Err(NativeError::InvalidConfig(msg)) => {
                assert!(
                    msg.contains("Streaming not supported"),
                    "expected streaming error, got: {msg}"
                );
            }
            Err(other) => panic!("expected InvalidConfig, got: {other}"),
            Ok(_) => panic!("expected error, got Ok"),
        }
    }

    #[tokio::test]
    async fn boxed_dyn_backend_load_infer_unload() {
        let mut backend: Box<dyn DynInferenceBackend> = Box::new(MockBackend::new());

        let load_result = backend
            .load_dyn(PathBuf::from("/tmp/model.gguf"), LoadConfig::default())
            .await;
        assert!(
            load_result.is_ok(),
            "load_dyn should succeed: {:?}",
            load_result.as_ref().err()
        );
        assert!(backend.is_loaded_dyn());

        let infer_result = backend
            .infer_dyn("test prompt".to_string(), ChatOptions::default())
            .await;
        assert!(
            infer_result.is_ok(),
            "infer_dyn should succeed: {:?}",
            infer_result.as_ref().err()
        );
        assert_eq!(infer_result.unwrap().message.content, "echo: test prompt");

        let unload_result = backend.unload_dyn().await;
        assert!(
            unload_result.is_ok(),
            "unload_dyn should succeed: {:?}",
            unload_result.as_ref().err()
        );
        assert!(!backend.is_loaded_dyn());
    }
}
