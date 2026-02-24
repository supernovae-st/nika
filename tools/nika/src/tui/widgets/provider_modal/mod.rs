//! Provider Modal v2
//!
//! Rich tabbed modal for provider management:
//! - Cloud providers with cards and sparklines
//! - Ollama local models with install/pull
//! - API key management via system keychain
//! - Configuration preferences

mod components;
mod keyring;
mod ollama_client;
mod state;
mod tabs;

pub use components::*;
pub use keyring::*;
pub use ollama_client::*;
pub use state::*;
pub use tabs::*;
