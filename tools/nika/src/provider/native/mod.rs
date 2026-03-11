//! Native LLM inference module (v0.27)
//!
//! This module provides local GGUF model inference via mistral.rs.
//! v0.27.0: Migrated from spn-native to nika directly.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Nika Native Inference (v0.27)                                              │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                             │
//! │  NativeRuntime (local implementation)                                       │
//! │  ├── load(path, config)        Load GGUF model into memory                  │
//! │  ├── unload()                  Unload model from memory                     │
//! │  ├── is_loaded()               Check if model is loaded                     │
//! │  ├── model_info()              Get metadata about loaded model              │
//! │  ├── infer(prompt, opts)       Generate response (non-streaming)            │
//! │  └── infer_stream(...)         Generate response (streaming)                │
//! │                                                                             │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use nika::provider::native::NativeRuntime;
//! use nika::core::backend::LoadConfig;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut runtime = NativeRuntime::new();
//!
//!     // Load a GGUF model
//!     runtime.load("~/.cache/huggingface/models/qwen3-8b-q4_k_m.gguf".into(), LoadConfig::default()).await?;
//!
//!     // Non-streaming inference
//!     let response = runtime.infer("What is 2+2?", Default::default()).await?;
//!     println!("{}", response.message.content);
//!
//!     // Streaming inference
//!     use futures::StreamExt;
//!     let mut stream = runtime.infer_stream("Explain Rust", Default::default()).await?;
//!     while let Some(chunk) = stream.next().await {
//!         print!("{}", chunk?);
//!     }
//!
//!     Ok(())
//! }
//! ```

// Local modules (v0.27 - migrated from spn-native)
pub mod error;
pub mod runtime;
pub mod traits;

// Re-export main types
pub use error::NativeError;
pub use runtime::NativeRuntime;
pub use traits::{DynInferenceBackend, InferenceBackend};

// Re-export backend types from core
pub use crate::core::backend::{
    ChatMessage, ChatOptions, ChatResponse, ChatRole, DownloadRequest, DownloadResult, LoadConfig,
    ModelInfo, PullProgress,
};

// Re-export storage types from core (v0.27)
pub use crate::core::storage::{
    default_model_dir, extract_quantization, HuggingFaceStorage, ModelStorage, StorageError,
};

// Backwards compatibility alias (deprecated in v0.26)
#[deprecated(
    since = "0.26.0",
    note = "Use NativeRuntime directly instead of NativeClient"
)]
pub type NativeClient = NativeRuntime;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_quantization() {
        assert_eq!(
            extract_quantization("model-q4_k_m.gguf"),
            Some("Q4_K_M".to_string())
        );
        assert_eq!(
            extract_quantization("qwen-q8_0.gguf"),
            Some("Q8_0".to_string())
        );
        assert_eq!(
            extract_quantization("mistral-f16.gguf"),
            Some("F16".to_string())
        );
        assert_eq!(extract_quantization("model.gguf"), None);
    }
}
