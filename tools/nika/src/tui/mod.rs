//! Terminal User Interface Module
//!
//! Feature-gated TUI for workflow observability.
//!
//! # Two Modes
//!
//! 1. **Standalone Mode** (`nika tui` without args):
//!    - File browser for `.nika.yaml` files
//!    - Execution history
//!    - YAML preview
//!
//! 2. **Execution Mode** (`nika tui workflow.yaml`):
//!    - 4-panel workflow observer
//!    - Real-time event streaming
//!
//! # Architecture
//!
//! ```text
//! STANDALONE MODE (Home)                    EXECUTION MODE (Run)
//! ┌───────────────┬───────────────┐        ┌─────────────────────────────┐
//! │ [1] BROWSER   │ [2] HISTORY   │        │ [1] MISSION CONTROL         │
//! ├───────────────┴───────────────┤        ├──────────────┬──────────────┤
//! │ [3] PREVIEW                   │        │ [2] DAG      │ [3] NOVANET  │
//! └───────────────────────────────┘        ├──────────────┴──────────────┤
//!                                          │ [4] AGENT REASONING         │
//!                                          └─────────────────────────────┘
//! ```

#[cfg(feature = "tui")]
mod app;
#[cfg(feature = "tui")]
mod panels;
#[cfg(feature = "tui")]
mod standalone;
#[cfg(feature = "tui")]
mod state;
#[cfg(feature = "tui")]
mod theme;
#[cfg(feature = "tui")]
mod views;
#[cfg(feature = "tui")]
mod widgets;

#[cfg(feature = "tui")]
pub use app::App;
#[cfg(feature = "tui")]
pub use standalone::{BrowserEntry, HistoryEntry, StandalonePanel, StandaloneState};
#[cfg(feature = "tui")]
pub use state::{AgentTurnState, PanelId, TuiMode, TuiState};
#[cfg(feature = "tui")]
pub use theme::{MissionPhase, TaskStatus, Theme};
#[cfg(feature = "tui")]
pub use views::{
    BrowserView, DagTab, MissionTab, MonitorView, NovanetTab, ReasoningTab, TuiView, ViewAction,
};

/// Run the TUI for a workflow
///
/// This function:
/// 1. Parses and validates the workflow
/// 2. Creates an EventLog with broadcast channel
/// 3. Spawns the Runner in a background task
/// 4. Runs the TUI with real-time event updates
#[cfg(feature = "tui")]
pub async fn run_tui(workflow_path: &std::path::Path) -> crate::error::Result<()> {
    use crate::ast::Workflow;
    use crate::event::EventLog;
    use crate::runtime::Runner;

    // 1. Parse and validate workflow
    let yaml_content = std::fs::read_to_string(workflow_path).map_err(|e| {
        crate::error::NikaError::WorkflowNotFound {
            path: format!("{}: {}", workflow_path.display(), e),
        }
    })?;

    let workflow: Workflow = serde_yaml::from_str(&yaml_content).map_err(|e| {
        let line_info = e
            .location()
            .map(|l| format!(" (line {})", l.line()))
            .unwrap_or_default();
        crate::error::NikaError::ParseError {
            details: format!("{}{}", e, line_info),
        }
    })?;

    workflow.validate_schema()?;

    // 2. Create EventLog with broadcast channel for TUI
    let (event_log, event_rx) = EventLog::new_with_broadcast();

    // 3. Create Runner with the broadcast-enabled EventLog and quiet mode
    // quiet() suppresses console output that would interfere with the TUI
    let runner = Runner::with_event_log(workflow, event_log).quiet();

    // 4. Spawn Runner in background task
    let runner_handle = tokio::spawn(async move {
        match runner.run().await {
            Ok(output) => {
                tracing::info!("Workflow completed: {} bytes output", output.len());
            }
            Err(e) => {
                tracing::error!("Workflow failed: {}", e);
            }
        }
    });

    // 5. Create and run TUI with event receiver
    let app = App::new(workflow_path)?.with_broadcast_receiver(event_rx);
    let tui_result = app.run().await;

    // 6. Abort runner if TUI exits early (user pressed q)
    runner_handle.abort();

    tui_result
}

/// Run the TUI in standalone mode (file browser + history)
///
/// This function:
/// 1. Scans for .nika.yaml files in the project
/// 2. Shows file browser, history, and preview
/// 3. Allows user to select and run workflows
#[cfg(feature = "tui")]
pub async fn run_tui_standalone() -> crate::error::Result<()> {
    // Find project root (look for Cargo.toml or .git)
    let root = find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Create standalone state
    let state = StandaloneState::new(root);

    // Create and run standalone app
    let app = App::new_standalone(state)?;
    app.run_standalone().await
}

/// Find project root by looking for Cargo.toml or .git
#[cfg(feature = "tui")]
fn find_project_root() -> Option<std::path::PathBuf> {
    let mut current = std::env::current_dir().ok()?;

    loop {
        if current.join("Cargo.toml").exists() || current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[cfg(not(feature = "tui"))]
pub async fn run_tui(_workflow_path: &std::path::Path) -> crate::error::Result<()> {
    Err(crate::error::NikaError::ValidationError {
        reason: "TUI feature not enabled. Rebuild with --features tui".to_string(),
    })
}

#[cfg(not(feature = "tui"))]
pub async fn run_tui_standalone() -> crate::error::Result<()> {
    Err(crate::error::NikaError::ValidationError {
        reason: "TUI feature not enabled. Rebuild with --features tui".to_string(),
    })
}
