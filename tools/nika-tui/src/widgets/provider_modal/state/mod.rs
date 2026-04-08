// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
