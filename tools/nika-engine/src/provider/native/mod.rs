// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native LLM inference module
//!
//! This module provides local GGUF model inference via mistral.rs.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Nika Native Inference                                              │
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

// Local modules
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
    ModelInfo, NativeModelKind, PullProgress, VisionImage,
};

// Re-export storage types from core
pub use crate::core::storage::{
    default_model_dir, extract_quantization, HuggingFaceStorage, ModelStorage, StorageError,
};

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
