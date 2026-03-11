//! Native LLM inference module (v0.26)
//!
//! This module re-exports `NativeRuntime` from `spn_native` and provides
//! the interface for local GGUF model inference via mistral.rs.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  Nika Native Inference (v0.26)                                              │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                             │
//! │  spn_native::NativeRuntime (re-exported)                                    │
//! │  ├── load(path, config)        Load GGUF model into memory                  │
//! │  ├── unload()                  Unload model from memory                     │
//! │  ├── is_loaded()               Check if model is loaded                     │
//! │  ├── model_info()              Get metadata about loaded model              │
//! │  ├── infer(prompt, opts)       Generate response (non-streaming)            │
//! │  └── infer_stream(...)         Generate response (streaming, v0.26)         │
//! │                                                                             │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use nika::provider::native::NativeRuntime;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut runtime = NativeRuntime::new();
//!
//!     // Load a GGUF model
//!     runtime.load("~/.spn/models/qwen3-8b-q4_k_m.gguf".into(), None).await?;
//!
//!     // Non-streaming inference
//!     let response = runtime.infer("What is 2+2?", None).await?;
//!     println!("{}", response.message.content);
//!
//!     // Streaming inference (v0.26)
//!     let mut stream = runtime.infer_stream("Explain Rust", None).await?;
//!     while let Some(chunk) = stream.recv().await {
//!         print!("{}", chunk);
//!     }
//!
//!     Ok(())
//! }
//! ```

// Re-export NativeRuntime from spn_native
pub use spn_native::NativeRuntime;

// Re-export commonly used types from spn_native (via spn_core)
pub use spn_native::{
    ChatOptions, ChatResponse, LoadConfig, ModelInfo, NativeError,
};

// Backwards compatibility alias (deprecated in v0.26)
#[deprecated(since = "0.26.0", note = "Use NativeRuntime directly instead of NativeClient")]
pub type NativeClient = NativeRuntime;

/// Extract quantization level from model filename.
///
/// Parses common GGUF naming patterns:
/// - `model-q4_k_m.gguf` -> "Q4_K_M"
/// - `model-q8_0.gguf` -> "Q8_0"
/// - `model-f16.gguf` -> "F16"
#[must_use]
pub fn extract_quantization(filename: &str) -> Option<String> {
    // Delegate to spn_native's implementation
    spn_native::extract_quantization(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_quantization() {
        assert_eq!(extract_quantization("model-q4_k_m.gguf"), Some("Q4_K_M".to_string()));
        assert_eq!(extract_quantization("qwen-q8_0.gguf"), Some("Q8_0".to_string()));
        assert_eq!(extract_quantization("mistral-f16.gguf"), Some("F16".to_string()));
        assert_eq!(extract_quantization("model.gguf"), None);
    }
}
