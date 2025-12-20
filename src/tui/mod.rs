//! TUI Module - Mission Control Dashboard
//!
//! Hyperspace-themed terminal interface for Nika workflow execution.
//!
//! Architecture:
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                        UI LAYER (widgets/)                          │
//! │  Pure rendering. No business logic. Receives ViewState.             │
//! └─────────────────────────────────────────────────────────────────────┘
//!                               ▲
//!                               │ ViewState (derived)
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                      DOMAIN LAYER (state.rs)                        │
//! │  AppState + Selectors. Transforms RuntimeEvents → ViewState.        │
//! └─────────────────────────────────────────────────────────────────────┘
//!                               ▲
//!                               │ RuntimeEvent stream
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                    CONNECTOR LAYER (runtime/)                       │
//! │  RuntimeBridge trait. Async IO. MockRuntime + NikaRuntime.          │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```

mod app;
mod events;
mod state;
mod theme;

pub mod runtime;
pub mod widgets;

pub use app::TuiApp;
pub use state::{AppState, WorkflowStatus};
pub use theme::HyperspaceTheme;

/// Run the TUI dashboard
pub async fn run(workflow_path: Option<&str>) -> anyhow::Result<()> {
    let app = TuiApp::new(workflow_path)?;
    app.run().await
}
