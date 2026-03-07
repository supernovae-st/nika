//! Terminal User Interface Module
//!
//! Feature-gated TUI with 5-view architecture (v0.22+).
//!
//! # Entry Points
//!
//! - `nika` → Studio view (3-panel: Browser | Editor | DAG)
//! - `nika chat` → Chat view (conversational agent)
//! - `nika studio` → Studio view (YAML editor)
//! - `nika workflow.yaml` → Runner view (run workflow)
//!
//! # 5-View Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  [1] Studio  │ [2] Runner │ [3] Chat │ [4] Scheduler │ [5] Settings │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                 │
//! │  Studio:    3-panel layout (Browser | Editor | DAG Preview)    │
//! │  Runner:    Real-time execution monitor (DAG, Reasoning)       │
//! │  Chat:      Conversational agent, 5-verb support, MCP tools    │
//! │  Scheduler: DAG visualization and scheduling                   │
//! │  Settings:  Configuration and preferences                      │
//! │                                                                 │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Navigation
//!
//! - `1-5` - Switch views (Normal mode)
//! - `?` - Show help
//! - `Ctrl+C` (2x) - Quit
//!
//! # Panic Recovery (v0.7.0+)
//!
//! The TUI installs a panic hook to restore terminal state on crashes.
//! Crash logs are written to `~/.nika/crash.log`.

#[cfg(feature = "tui")]
mod app;
#[cfg(feature = "tui")]
mod cache;
#[cfg(feature = "tui")]
pub mod chat_agent;
#[cfg(feature = "tui")]
pub mod command;
#[cfg(feature = "tui")]
pub mod config;
#[cfg(feature = "tui")]
mod cosmic_theme;
#[cfg(feature = "tui")]
mod edit_history;
#[cfg(feature = "tui")]
pub mod file_resolve;
#[cfg(feature = "tui")]
mod focus;
#[cfg(feature = "tui")]
pub mod icons;
#[cfg(feature = "tui")]
mod keybindings;
#[cfg(feature = "tui")]
mod layout;
#[cfg(feature = "tui")]
mod mode;
#[cfg(feature = "tui")]
pub mod session;
#[cfg(feature = "tui")]
mod standalone;
#[cfg(feature = "tui")]
pub mod startup;
#[cfg(feature = "tui")]
mod state;
#[cfg(feature = "tui")]
mod theme;
#[cfg(feature = "tui")]
pub mod tokens;
#[cfg(feature = "tui")]
mod unicode;
#[cfg(feature = "tui")]
mod utils;
#[cfg(feature = "tui")]
mod verification;
#[cfg(feature = "tui")]
mod views;
#[cfg(feature = "tui")]
pub mod widgets;

#[cfg(feature = "tui")]
pub use app::App;
#[cfg(feature = "tui")]
pub use cache::RenderCache;
#[cfg(feature = "tui")]
pub use chat_agent::{ChatAgent, ChatMessage, ChatRole, StreamingState};
#[cfg(feature = "tui")]
pub use command::{Command, HELP_TEXT};
#[cfg(feature = "tui")]
pub use config::{
    ChatSettings, ConfigError, PathSettings, StudioSettings, ThemeName, TuiConfig, TuiSettings,
};
#[cfg(feature = "tui")]
pub use cosmic_theme::CosmicTheme;
#[cfg(feature = "tui")]
pub use edit_history::EditHistory;
#[cfg(feature = "tui")]
pub use file_resolve::FileResolver;
#[cfg(feature = "tui")]
pub use focus::{FocusState, PanelId as NavPanelId};
#[cfg(feature = "tui")]
pub use icons::{IconMode, IconSet};
#[cfg(feature = "tui")]
pub use keybindings::{format_key, keybindings_for_context, KeyCategory, Keybinding};
#[cfg(feature = "tui")]
pub use layout::{LayoutMode, ResponsiveLayout};
#[cfg(feature = "tui")]
pub use mode::InputMode;
#[cfg(feature = "tui")]
pub use session::{
    delete_session, get_latest_session, list_sessions, load_session, save_session, ChatSession,
    SessionMeta,
};
#[cfg(feature = "tui")]
pub use standalone::{BrowserEntry, HistoryEntry, StandalonePanel, StandaloneState};
#[cfg(feature = "tui")]
pub use state::{
    // Animation frame constants (v0.9.x standard)
    AgentTurnState,
    PanelId,
    PanelScrollState,
    TuiMode,
    TuiState,
    FRAME_CYCLE,
    FRAME_DIV_GLACIAL,
    FRAME_DIV_NORMAL,
    TARGET_FPS,
};
#[cfg(feature = "tui")]
pub use theme::{ColorMode, MissionPhase, TaskStatus, Theme, VerbColor};
#[cfg(feature = "tui")]
pub use tokens::{ColorPalette, CosmicVariant, SemanticColors, TokenResolver};
#[cfg(feature = "tui")]
pub use unicode::{display_width, truncate_to_width};
#[cfg(feature = "tui")]
pub use utils::{format_number, format_number_compact, format_number_u64};
#[cfg(feature = "tui")]
pub use verification::{VerificationCache, VerificationEntry};
#[cfg(feature = "tui")]
pub use views::{
    ChatView, DagTab, HelpView, HomeView, MissionTab, MonitorView, NovanetTab, ReasoningTab,
    SchedulerView, SettingsView, StudioView, TuiView, View, ViewAction, YamlEditorPanel,
};

/// Install panic hook to restore terminal state on crashes.
///
/// This function saves the original panic hook and wraps it with
/// terminal restoration logic. Crash logs are written to `~/.nika/crash.log`.
///
/// # Safety
///
/// This function should be called BEFORE entering raw mode.
/// It is safe to call multiple times (only installs once via std::sync::Once).
#[cfg(feature = "tui")]
fn install_panic_hook() {
    use std::io::Write;
    use std::panic;
    use std::sync::Once;

    use crossterm::{execute, terminal::LeaveAlternateScreen};

    static HOOK_INSTALLED: Once = Once::new();

    HOOK_INSTALLED.call_once(|| {
        let original_hook = panic::take_hook();

        panic::set_hook(Box::new(move |panic_info| {
            // 1. Restore terminal state FIRST (before any output)
            let _ = crossterm::terminal::disable_raw_mode();
            let _ = execute!(std::io::stdout(), LeaveAlternateScreen);

            // 2. Write crash log
            let crash_log_path = dirs::config_dir()
                .map(|d| d.join("nika").join("crash.log"))
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp/nika-crash.log"));

            // Ensure parent directory exists
            if let Some(parent) = crash_log_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&crash_log_path)
            {
                let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                let _ = writeln!(
                    f,
                    "\n══════════════════════════════════════════════════════════════"
                );
                let _ = writeln!(f, "NIKA CRASH: {}", timestamp);
                let _ = writeln!(
                    f,
                    "══════════════════════════════════════════════════════════════"
                );
                let _ = writeln!(f, "{}", panic_info);

                // Try to capture backtrace
                let backtrace = std::backtrace::Backtrace::capture();
                let _ = writeln!(f, "\nBacktrace:\n{}", backtrace);
            }

            // 3. Print user-friendly message to stderr
            eprintln!(
                "\n\x1b[31m╔══════════════════════════════════════════════════════════════╗\x1b[0m"
            );
            eprintln!(
                "\x1b[31m║  NIKA CRASHED - Terminal has been restored                    ║\x1b[0m"
            );
            eprintln!(
                "\x1b[31m╠══════════════════════════════════════════════════════════════╣\x1b[0m"
            );
            eprintln!(
                "\x1b[31m║  Crash log: {:49} ║\x1b[0m",
                crash_log_path.display()
            );
            eprintln!(
                "\x1b[31m║  Please report: https://github.com/SuperNovae-studio/nika    ║\x1b[0m"
            );
            eprintln!(
                "\x1b[31m╚══════════════════════════════════════════════════════════════╝\x1b[0m"
            );

            // 4. Call original hook
            original_hook(panic_info);
        }));
    });
}

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
    use crate::serde_yaml;

    // Install panic hook for terminal recovery
    install_panic_hook();

    // 1. Parse and validate workflow (use tokio::fs for non-blocking I/O)
    let yaml_content = tokio::fs::read_to_string(workflow_path)
        .await
        .map_err(|e| crate::error::NikaError::WorkflowNotFound {
            path: format!("{}: {}", workflow_path.display(), e),
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
    let mut runner = Runner::with_event_log(workflow, event_log).quiet();

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
    // Use run_unified() for the 5-view architecture (Studio/Runner/Chat/Scheduler/Settings)
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
    // Install panic hook for terminal recovery
    install_panic_hook();

    // Find project root (look for Cargo.toml or .git)
    let root = find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Create standalone state
    let state = StandaloneState::new(root);

    // Create and run standalone app with unified 5-view architecture
    // Starts in Studio view (3-panel: Browser | Editor | DAG)
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
/// * `model` - Optional model override (e.g., "claude-sonnet-4-6")
#[cfg(feature = "tui")]
pub async fn run_tui_chat(
    provider: Option<String>,
    model: Option<String>,
) -> crate::error::Result<()> {
    use views::TuiView;

    // Install panic hook for terminal recovery
    install_panic_hook();

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

    // Install panic hook for terminal recovery
    install_panic_hook();

    // Find project root
    let root = find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Create standalone state
    let state = StandaloneState::new(root.clone());

    // Create app and set initial view to Studio
    let mut app = App::new_standalone(state)?.with_initial_view(TuiView::Studio);

    // If workflow provided, load it into Editor view
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

/// Run the TUI with customizable options (view and workflow)
///
/// This is the entry point for `nika ui [--view <view>] [workflow]` command.
/// Supports all 5 views: Studio, Runner, Chat, Scheduler, Settings.
///
/// # Arguments
///
/// * `workflow` - Optional workflow file to load
/// * `initial_view` - Optional initial view (defaults to Explorer)
#[cfg(feature = "tui")]
pub async fn run_tui_with_options(
    workflow: Option<std::path::PathBuf>,
    initial_view: Option<views::TuiView>,
) -> crate::error::Result<()> {
    // Install panic hook for terminal recovery
    install_panic_hook();

    // Find project root
    let root = find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Create standalone state
    let state = StandaloneState::new(root.clone());

    // Create app with initial view (default to Explorer)
    let mut app = App::new_standalone(state)?;

    if let Some(view) = initial_view {
        app = app.with_initial_view(view);
    }

    // If workflow provided and view is Editor/Runner, load it
    if let Some(path) = workflow {
        let full_path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };

        // Load file into Studio view
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

#[cfg(not(feature = "tui"))]
pub async fn run_tui_with_options(
    _workflow: Option<std::path::PathBuf>,
    _initial_view: Option<views::TuiView>,
) -> crate::error::Result<()> {
    Err(crate::error::NikaError::ValidationError {
        reason: "TUI feature not enabled. Rebuild with --features tui".to_string(),
    })
}
