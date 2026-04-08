// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Enhanced Tree Widget for Nika TUI
//!
//! VS Code-like tree view with SuperNovae ecosystem detection.
//!
//! # Features
//!
//! - **Ecosystem Detection**: Premium visual treatment for .nika.yaml, .son, .nika/, .novanet/, etc.
//! - **Solarized Theme**: Consistent colors across light/dark modes
//! - **Glow Animations**: Pulse effects for ecosystem files (60fps ticker)
//! - **Keyboard Navigation**: Vim-style (hjkl) and arrow key support
//! - **Filtering**: Show workflows only (W), agents only (A), ecosystem (E), or all (0)
//!
//! # Architecture
//!
//! ```text
//! tree/
//! ├── mod.rs       # Module root and public exports
//! ├── node.rs      # TreeNode, NodeKind, GitStatus
//! ├── state.rs     # TreeState (selection, expansion, navigation)
//! ├── colors.rs    # TreeColors (Solarized palette)
//! ├── animation.rs # GlowAnimation, AnimationTicker (60fps)
//! ├── filter.rs    # TreeFilter, FilterConfig (W/A/E/0 keys)
//! └── render.rs    # TreeWidget (StatefulWidget implementation)
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use nika_tui::widgets::tree::{
//!     TreeNode, TreeState, TreeColors, TreeWidget,
//!     AnimationTicker, FilterConfig, TreeFilter,
//! };
//!
//! let root = TreeNode::from_path("./");
//! let mut state = TreeState::new();
//! let colors = TreeColors::solarized_dark();
//! let mut ticker = AnimationTicker::new();
//! let filter = FilterConfig::new();
//!
//! // Render tree with animations and filtering
//! let widget = TreeWidget::new(&root)
//!     .colors(colors)
//!     .ticker(&ticker)
//!     .filter(filter);
//!
//! // Handle keyboard input
//! state.apply_action(TreeAction::Down); // Navigation
//! state.apply_action(TreeAction::Toggle); // Expand/collapse
//! ```

mod animation;
mod colors;
mod filter;
mod node;
mod render;
mod state;

// Animation system
pub use animation::{AnimationKind, AnimationTicker, GlowAnimation};

// Color palette
pub use colors::TreeColors;

// Filtering
pub use filter::{filter_from_key, FilterConfig, TreeFilter};

// Core types
pub use node::{
    build_git_status_cache, GitStatus, GitStatusCache, NodeId, NodeKind, TreeNode, MAX_TREE_DEPTH,
};

// State management
pub use state::{TreeAction, TreeState};

// Rendering
pub use render::TreeWidget;
