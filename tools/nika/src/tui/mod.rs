//! Terminal User Interface Module
//!
//! Feature-gated TUI with 4-view architecture (v0.5.2+).
//!
//! # Entry Points
//!
//! - `nika` → Home view (browse workflows)
//! - `nika chat` → Chat view (conversational agent)
//! - `nika studio` → Studio view (YAML editor)
//! - `nika workflow.yaml` → Monitor view (run workflow)
//!
//! # 4-View Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  [a] Chat  │  [h] Home  │  [s] Studio  │  [m] Monitor          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                 │
//! │  Chat:    Conversational agent, 5-verb support, MCP tools      │
//! │  Home:    File browser for .nika.yaml, execution history       │
//! │  Studio:  YAML editor with live validation, schema hints       │
//! │  Monitor: Real-time 4-panel observer (DAG, Reasoning, NovaNet) │
//! │                                                                 │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Navigation
//!
//! - `Tab` / `a/h/s/m` - Switch views
//! - `?` - Show help
//! - `q` - Quit

#[cfg(feature = "tui")]
mod app;
#[cfg(feature = "tui")]
pub mod chat_agent;
#[cfg(feature = "tui")]
pub mod command;
#[cfg(feature = "tui")]
pub mod file_resolve;
#[cfg(feature = "tui")]
mod focus;
#[cfg(feature = "tui")]
mod keybindings;
#[cfg(feature = "tui")]
mod mode;
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
mod watcher;
#[cfg(feature = "tui")]
pub mod widgets;

#[cfg(feature = "tui")]
pub use app::App;
#[cfg(feature = "tui")]
pub use chat_agent::{ChatAgent, ChatMessage, ChatRole, StreamingState};
#[cfg(feature = "tui")]
pub use command::{Command, HELP_TEXT};
#[cfg(feature = "tui")]
pub use file_resolve::FileResolver;
#[cfg(feature = "tui")]
pub use focus::{FocusState, PanelId as NavPanelId};
#[cfg(feature = "tui")]
pub use keybindings::{format_key, keybindings_for_context, KeyCategory, Keybinding};
#[cfg(feature = "tui")]
pub use mode::InputMode;
#[cfg(feature = "tui")]
pub use standalone::{BrowserEntry, HistoryEntry, StandalonePanel, StandaloneState};
#[cfg(feature = "tui")]
pub use state::{AgentTurnState, PanelId, TuiMode, TuiState};
#[cfg(feature = "tui")]
pub use theme::{MissionPhase, TaskStatus, Theme};
#[cfg(feature = "tui")]
pub use views::{DagTab, MissionTab, NovanetTab, ReasoningTab, TuiView, ViewAction};
#[cfg(feature = "tui")]
pub use watcher::{FileEvent, FileWatcher};

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
    // Use run_unified() for the 4-view architecture (Chat/Home/Studio/Monitor)
    let app = App::new(workflow_path)?.with_broadcast_receiver(event_rx);
    let tui_result = app.run_unified().await;

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

    // Create and run standalone app with unified 4-view architecture
    // Starts in Home view (file browser) with Chat/Studio/Monitor available
    let app = App::new_standalone(state)?;
    app.run_unified().await
}

/// Run the TUI in Chat mode (conversational agent)
///
/// This is the entry point for `nika chat` command.
/// Starts directly in Chat view for conversational interactions.
///
/// # Arguments
///
/// * `provider` - Optional provider override ("claude" or "openai")
/// * `model` - Optional model override (e.g., "claude-sonnet-4-20250514")
#[cfg(feature = "tui")]
pub async fn run_tui_chat(
    provider: Option<String>,
    model: Option<String>,
) -> crate::error::Result<()> {
    use views::TuiView;

    // Find project root
    let root = find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Create standalone state (Chat mode needs file context)
    let state = StandaloneState::new(root);

    // Create app with provider/model overrides
    let app = App::new_standalone(state)?
        .with_initial_view(TuiView::Chat)
        .with_chat_overrides(provider, model);

    app.run_unified().await
}

/// Run the TUI in Studio mode (workflow editor)
///
/// This is the entry point for `nika studio [workflow]` command.
/// Starts directly in Studio view for YAML editing with live validation.
#[cfg(feature = "tui")]
pub async fn run_tui_studio(workflow: Option<std::path::PathBuf>) -> crate::error::Result<()> {
    use views::TuiView;

    // Find project root
    let root = find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Create standalone state
    let state = StandaloneState::new(root.clone());

    // Create app and set initial view to Studio
    let mut app = App::new_standalone(state)?.with_initial_view(TuiView::Studio);

    // If workflow provided, load it into Studio view
    if let Some(path) = workflow {
        let full_path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        app = app.with_studio_file(full_path);
    }

    app.run_unified().await
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

#[cfg(not(feature = "tui"))]
pub async fn run_tui_chat(
    _provider: Option<String>,
    _model: Option<String>,
) -> crate::error::Result<()> {
    Err(crate::error::NikaError::ValidationError {
        reason: "TUI feature not enabled. Rebuild with --features tui".to_string(),
    })
}

#[cfg(not(feature = "tui"))]
pub async fn run_tui_studio(_workflow: Option<std::path::PathBuf>) -> crate::error::Result<()> {
    Err(crate::error::NikaError::ValidationError {
        reason: "TUI feature not enabled. Rebuild with --features tui".to_string(),
    })
}
