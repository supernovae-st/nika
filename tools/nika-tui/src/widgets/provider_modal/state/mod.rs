//! Provider modal state types
//!
//! Split into focused submodules:
//! - `types` — Value types (NativeModelInfo, ProviderModalTab, ConnectionStatus, etc.)
//! - `modal` — Core struct, Default, Debug, visibility, animation, verification, tab labels
//! - `navigation` — Grid/list navigation, tab switching, model expand/collapse
//! - `providers` — Provider status tracking, latency history, session stats, loader events

mod modal;
mod navigation;
pub(crate) mod providers;
mod types;

#[cfg(test)]
mod tests;

// Re-export all public types (preserves existing `pub use state::*` in parent mod.rs)
pub use modal::ProviderModalState;
pub use types::{
    ApiKeyState, ConnectionStatus, DownloadState, NativeModelDetails, NativeModelInfo,
    ProviderModalTab,
};
