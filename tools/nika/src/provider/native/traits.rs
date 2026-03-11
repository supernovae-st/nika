//! Inference backend traits.
//!
//! Defines the interface for local model inference backends.
//!
//! v0.27.0: Migrated from spn-native.

use crate::core::backend::{ChatOptions, ChatResponse, LoadConfig, ModelInfo};
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

    // Verify the trait is object-safe
    fn _assert_object_safe(_: &dyn DynInferenceBackend) {}
}
