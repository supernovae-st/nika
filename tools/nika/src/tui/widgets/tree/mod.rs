//! Enhanced Tree Widget for Nika TUI v0.20
//!
//! VS Code-like tree view with SuperNovae ecosystem detection.
//!
//! # Features
//!
//! - **Ecosystem Detection**: Premium visual treatment for .nika.yaml, .son, .spn/, etc.
//! - **Solarized Theme**: Consistent colors across light/dark modes
//! - **Glow Animations**: Subtle hover effects for ecosystem files
//! - **Keyboard Navigation**: Vim-style (hjkl) and arrow key support
//!
//! # Architecture
//!
//! ```text
//! tree/
//! ├── mod.rs      # Module root and public exports
//! ├── node.rs     # TreeNode, NodeKind, GitStatus
//! ├── state.rs    # TreeState (selection, expansion)
//! └── colors.rs   # TreeColors (Solarized palette)
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use nika::tui::widgets::tree::{TreeNode, TreeState, TreeColors};
//!
//! let root = TreeNode::from_path("./");
//! let mut state = TreeState::new();
//! let colors = TreeColors::solarized_dark();
//!
//! // Handle keyboard input
//! state.handle_key(KeyCode::Char('j')); // Down
//! state.handle_key(KeyCode::Enter);      // Toggle/Open
//! ```

mod colors;
mod node;
mod state;

pub use colors::TreeColors;
pub use node::{GitStatus, NodeId, NodeKind, TreeNode};
pub use state::{TreeAction, TreeState};
