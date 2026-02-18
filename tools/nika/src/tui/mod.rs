//! Terminal User Interface Module
//!
//! Feature-gated TUI for workflow observability.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │ [1] WORKFLOW PROGRESS                                               │
//! ├─────────────────────────────┬───────────────────────────────────────┤
//! │ [2] GRAPH TRAVERSAL         │ [3] CONTEXT ASSEMBLED                 │
//! ├─────────────────────────────┴───────────────────────────────────────┤
//! │ [4] AGENT REASONING                                                 │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```

#[cfg(feature = "tui")]
mod app;
#[cfg(feature = "tui")]
mod event;
#[cfg(feature = "tui")]
mod panels;
#[cfg(feature = "tui")]
mod ui;

#[cfg(feature = "tui")]
pub use app::App;

/// Run the TUI for a workflow
#[cfg(feature = "tui")]
pub async fn run_tui(workflow_path: &std::path::Path) -> crate::error::Result<()> {
    let app = App::new(workflow_path)?;
    app.run().await
}

#[cfg(not(feature = "tui"))]
pub async fn run_tui(_workflow_path: &std::path::Path) -> crate::error::Result<()> {
    Err(crate::error::NikaError::ValidationError {
        reason: "TUI feature not enabled. Rebuild with --features tui".to_string(),
    })
}
