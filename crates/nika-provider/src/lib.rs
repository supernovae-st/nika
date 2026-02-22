//! nika-provider — LLM provider integration for Nika workflow engine
//!
//! ## Provider Strategy (v0.4+)
//!
//! Nika uses [rig-core](https://github.com/0xPlaygrounds/rig) for LLM providers.
//!
//! | Component | Implementation |
//! |-----------|----------------|
//! | `agent:` verb | `RigAgentLoop` + rig-core |
//! | `infer:` verb | `RigProvider` + rig-core |
//! | Tool calling | `NikaMcpTool` (rig `ToolDyn`) |
//!
//! ## Supported Providers
//!
//! | Provider | Constructor | Env Var |
//! |----------|-------------|---------|
//! | Claude | `RigProvider::claude()` | `ANTHROPIC_API_KEY` |
//! | OpenAI | `RigProvider::openai()` | `OPENAI_API_KEY` |
//! | Mistral | `RigProvider::mistral()` | `MISTRAL_API_KEY` |
//! | Ollama | `RigProvider::ollama()` | `OLLAMA_API_BASE_URL` |
//! | Groq | `RigProvider::groq()` | `GROQ_API_KEY` |
//! | DeepSeek | `RigProvider::deepseek()` | `DEEPSEEK_API_KEY` |
//!
//! ## Example
//!
//! ```rust,ignore
//! use nika_provider::{RigProvider, NikaMcpTool};
//!
//! // Auto-detect from env (checks all 6 providers)
//! let provider = RigProvider::auto().expect("No API key found");
//!
//! // Or explicit
//! let provider = RigProvider::claude();
//!
//! // Simple inference
//! let result = provider.infer("Summarize this", None).await?;
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod rig;

// Re-export main types for convenience
pub use rig::{NikaMcpTool, RigProvider, StreamChunk, StreamResult};
