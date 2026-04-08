// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Terminal User Interface Module
//!
//! Standalone TUI crate with 3-view architecture.
//!
//! # Entry Points
//!
//! - `nika` -> Studio view (3-panel: Browser | Editor | DAG)
//! - `nika chat` -> Command view (Chat mode)
//! - `nika studio` -> Studio view (YAML editor)
//! - `nika workflow.yaml` -> Command view (Monitor mode)
//!
//! # 3-View Architecture
//!
//! ```text
//! +------------------------------------------------------------------+
//! |  [1] Studio  | [2] Command | [3] Control                         |
//! +------------------------------------------------------------------+
//! |                                                                    |
//! |  Studio:    3-panel layout (Browser | Editor | DAG Preview)       |
//! |  Command:   Chat + Monitor modes (Ctrl+M to toggle)              |
//! |  Control:   Configuration and preferences                         |
//! |                                                                    |
//! +------------------------------------------------------------------+
//! ```
//!
//! # Navigation
//!
//! - `1-3` - Switch views (Normal mode)
//! - `?` - Show help
//! - `Ctrl+C` (2x) - Quit
//!
//! # Panic Recovery
//!
//! The TUI installs a panic hook to restore terminal state on crashes.
//! Crash logs are written to `~/.nika/crash.log`.

#[cfg(test)]
mod test_helpers;

mod app;
mod cache;
pub mod chat_agent;
pub mod command;
pub mod config;
mod cosmic_theme;
pub mod diagnostics;
mod edit_history;
pub mod file_resolve;
mod focus;
pub mod git; // Git integration for gutter + file status
pub mod highlight;
pub mod icons;
mod keybindings;
mod layout;
mod mode;
pub mod providers;
pub mod selection;
pub mod session;
mod standalone;
pub mod startup;
mod state;
mod theme;
pub mod tokens;
mod unicode;
mod utils;
mod verification;
pub mod views;
pub mod widgets;
pub mod wizard;

pub use app::App;
pub use cache::RenderCache;
pub use chat_agent::{ChatAgent, ChatMessage, ChatRole, StreamingState};
pub use command::{Command, HELP_TEXT};
pub use config::{
    ChatSettings, ConfigError, PathSettings, StudioSettings, ThemeName, TuiConfig, TuiSettings,
};
pub use cosmic_theme::CosmicTheme;
pub use edit_history::EditHistory;
pub use file_resolve::FileResolver;
pub use focus::PanelId as NavPanelId;
pub use highlight::{
    HighlightCapture, HighlightTheme, Highlighter, SolarizedTheme, TreeSitterHighlighter,
};
pub use icons::{IconMode, IconSet};
pub use keybindings::{format_key, keybindings_for_context, KeyCategory, Keybinding};
pub use layout::{LayoutMode, ResponsiveLayout};
pub use mode::InputMode;
pub use selection::{Position, Selection, SelectionMode, SelectionSet};
pub use session::{
    delete_session, get_latest_session, list_sessions, load_session, save_session, ChatSession,
    SessionMeta,
};
pub use standalone::StandaloneState;
pub use state::{
    // Animation frame constants
    AgentTurnState,
    MonitorPanel,
    PanelScrollState,
    TuiMode,
    TuiState,
    FRAME_CYCLE,
    FRAME_DIV_GLACIAL,
    FRAME_DIV_NORMAL,
};
pub use theme::{ColorMode, MissionPhase, TaskStatus, Theme, VerbColor};
pub use tokens::{ColorPalette, CosmicVariant, SemanticColors, TokenResolver};
pub use unicode::{display_width, truncate_to_width};
pub use utils::{format_number, format_number_compact, format_number_u64};
pub use verification::{VerificationCache, VerificationEntry};
pub use views::{
    ChatView, DagTab, MissionTab, MonitorView, NovanetTab, ReasoningTab, SettingsView, StudioView,
    TuiView, View, ViewAction, YamlEditorPanel,
};
pub use wizard::{WizardConfig, WizardState, WizardStep};

/// Install panic hook to restore terminal state on crashes.
///
/// This function saves the original panic hook and wraps it with
/// terminal restoration logic. Crash logs are written to `~/.nika/crash.log`.
///
/// # Safety
///
/// This function should be called BEFORE entering raw mode.
/// It is safe to call multiple times (only installs once via std::sync::Once).
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
                "\x1b[31m║  Please report: https://github.com/supernovae-st/nika    ║\x1b[0m"
            );
            eprintln!(
                "\x1b[31m╚══════════════════════════════════════════════════════════════╝\x1b[0m"
            );

            // 4. Call original hook
            original_hook(panic_info);
        }));
    });
}

/// Install signal handlers for SIGTERM and SIGHUP (Unix only).
///
/// These signals are sent when the terminal is closed (SIGHUP) or when the
/// process is killed (SIGTERM). Without handlers, the terminal is left in raw
/// mode, making the shell unusable until `reset` is run.
///
/// The handler restores terminal state (disable raw mode, leave alternate
/// screen) and exits with the conventional 128+signal code.
#[cfg(unix)]
fn install_signal_handlers() {
    use signal_hook::consts::{SIGHUP, SIGTERM};
    use signal_hook::iterator::Signals;

    let mut signals: Signals = match Signals::new([SIGTERM, SIGHUP]) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to install signal handlers: {e}");
            return;
        }
    };

    std::thread::spawn(move || {
        // Blocks on epoll/kqueue — no CPU-wasting poll loop
        if let Some(sig) = signals.forever().next() {
            let _ = crossterm::terminal::disable_raw_mode();
            let _ =
                crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
            std::process::exit(128 + sig);
        }
    });
}

/// No-op on Windows — signal_hook::iterator is Unix-only.
#[cfg(not(unix))]
fn install_signal_handlers() {}

/// Run the TUI for a workflow
///
/// This function:
/// 1. Parses and validates the workflow
/// 2. Creates an EventLog with broadcast channel
/// 3. Spawns the Runner in a background task
/// 4. Runs the TUI with real-time event updates
pub async fn run_tui(workflow_path: &std::path::Path) -> nika_engine::error::Result<()> {
    use nika_engine::ast::parse_analyzed;
    use nika_engine::event::EventLog;
    use nika_engine::runtime::Runner;

    // Install panic hook for terminal recovery
    install_panic_hook();
    install_signal_handlers();

    // 1. Parse and validate workflow (use tokio::fs for non-blocking I/O)
    let yaml_content = tokio::fs::read_to_string(workflow_path)
        .await
        .map_err(|e| nika_engine::error::NikaError::WorkflowNotFound {
            path: format!("{}: {}", workflow_path.display(), e),
        })?;

    let workflow = parse_analyzed(&yaml_content)?;

    // 2. Create EventLog with broadcast channel for TUI
    let (event_log, event_rx) = EventLog::new_with_broadcast();

    // 3. Create Runner with the broadcast-enabled EventLog and quiet mode
    // quiet() suppresses console output that would interfere with the TUI
    let mut runner = Runner::with_event_log(workflow, event_log)?.quiet();

    // 3b. Wire custom endpoints from config.toml
    let config = nika_engine::config::NikaConfig::load()
        .unwrap_or_default()
        .with_env();
    if !config.endpoints.is_empty() {
        if let Ok(resolved) = nika_engine::provider::endpoints::resolve_endpoints(&config.endpoints)
        {
            runner.with_custom_endpoints(resolved);
        }
    }

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
    // Use run_unified() for the 3-view architecture (Studio/Command/Control)
    let app = App::new(workflow_path)?.with_broadcast_receiver(event_rx);
    let tui_result = app.run_unified().map(|_| ());

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
pub async fn run_tui_standalone() -> nika_engine::error::Result<()> {
    use views::WizardView;

    // First-run detection: if wizard hasn't been completed, offer to run it
    if !WizardView::was_completed() {
        // Check if we're in an interactive terminal
        use std::io::IsTerminal;
        if std::io::stdin().is_terminal() {
            use colored::Colorize;
            use std::io::{self, Write};

            println!();
            println!(
                "{}",
                "╔═══════════════════════════════════════════════════════════════╗".cyan()
            );
            println!(
                "{}",
                "║  🦋 Welcome to Nika!                                          ║".cyan()
            );
            println!(
                "{}",
                "║                                                               ║".cyan()
            );
            println!(
                "{}",
                "║  It looks like this is your first time running Nika.         ║".cyan()
            );
            println!(
                "{}",
                "║  Would you like to run the setup wizard?                      ║".cyan()
            );
            println!(
                "{}",
                "╚═══════════════════════════════════════════════════════════════╝".cyan()
            );
            println!();
            print!("{}", "Run setup wizard? [Y/n]: ".bold());
            io::stdout().flush().ok();

            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_ok() {
                let input = input.trim().to_lowercase();
                if input.is_empty() || input == "y" || input == "yes" {
                    println!();
                    println!("{}", "Starting setup wizard...".dimmed());
                    println!();
                    // Run the wizard
                    run_tui_wizard()?;
                    println!();
                    println!("{}", "Setup complete! Starting Nika...".green());
                    println!();
                }
            }
        }
    }

    // Install panic hook for terminal recovery
    install_panic_hook();
    install_signal_handlers();

    // Find project root (look for Cargo.toml or .git)
    let root = find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Create standalone state
    let state = StandaloneState::new(root);

    // Create and run standalone app with unified 3-view architecture
    // Starts in Studio view (3-panel: Browser | Editor | DAG)
    let app = App::new_standalone(state)?;
    let launch_wizard = app.run_unified()?;

    // Check if wizard was requested from Settings view
    if launch_wizard {
        println!();
        run_tui_wizard()?;
    }

    Ok(())
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
pub async fn run_tui_chat(
    provider: Option<String>,
    model: Option<String>,
) -> nika_engine::error::Result<()> {
    use views::TuiView;

    // Install panic hook for terminal recovery
    install_panic_hook();
    install_signal_handlers();

    // Find project root
    let root = find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Create standalone state (Chat mode needs file context)
    let state = StandaloneState::new(root);

    // Create app with provider/model overrides
    let app = App::new_standalone(state)?
        .with_initial_view(TuiView::Command)
        .with_chat_overrides(provider, model);

    app.run_unified().map(|_| ())
}

/// Run the TUI in Studio mode (workflow editor)
///
/// This is the entry point for `nika studio [workflow]` command.
/// Starts directly in Studio view for YAML editing with live validation.
pub async fn run_tui_studio(
    workflow: Option<std::path::PathBuf>,
) -> nika_engine::error::Result<()> {
    use views::TuiView;

    // Install panic hook for terminal recovery
    install_panic_hook();
    install_signal_handlers();

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

    app.run_unified().map(|_| ())
}

/// Run the TUI with customizable options (view and workflow)
///
/// This is the entry point for `nika ui [--view <view>] [workflow]` command.
/// Supports all 3 views: Studio, Command, Control.
///
/// # Arguments
///
/// * `workflow` - Optional workflow file to load
/// * `initial_view` - Optional initial view (defaults to Studio)
pub async fn run_tui_with_options(
    workflow: Option<std::path::PathBuf>,
    initial_view: Option<views::TuiView>,
) -> nika_engine::error::Result<()> {
    // Install panic hook for terminal recovery
    install_panic_hook();
    install_signal_handlers();

    // Find project root
    let root = find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Create standalone state
    let state = StandaloneState::new(root.clone());

    // Create app with initial view (default to Studio)
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

    app.run_unified().map(|_| ())
}

/// Find project root by looking for Cargo.toml or .git
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

/// Run the TUI Setup Wizard (standalone)
///
/// Full-screen setup wizard, separate from the 3-view TUI.
/// Launched by `nika setup` (deprecated) or Settings → "Re-run wizard".
pub fn run_tui_wizard() -> nika_engine::error::Result<()> {
    use crossterm::{
        event::{self, Event, KeyEventKind},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};
    use std::io::stdout;

    use crate::state::TuiState;
    use crate::theme::Theme;
    use crate::views::{View, ViewAction, WizardView};

    // Install panic hook for terminal recovery
    install_panic_hook();
    install_signal_handlers();

    // Setup terminal
    enable_raw_mode().map_err(nika_engine::error::NikaError::IoError)?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).map_err(nika_engine::error::NikaError::IoError)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(nika_engine::error::NikaError::IoError)?;

    // Create wizard view and state
    let mut wizard = WizardView::new();
    let mut tui_state = TuiState::new("wizard");
    let theme = Theme::default();

    // Main loop
    let result = loop {
        // Draw
        terminal
            .draw(|frame| {
                wizard.render(frame, frame.area(), &tui_state, &theme);
            })
            .map_err(nika_engine::error::NikaError::IoError)?;

        // Handle events
        if event::poll(std::time::Duration::from_millis(100))
            .map_err(nika_engine::error::NikaError::IoError)?
        {
            if let Event::Key(key) =
                event::read().map_err(nika_engine::error::NikaError::IoError)?
            {
                // Only handle key press events
                if key.kind == KeyEventKind::Press {
                    match wizard.handle_key(key, &mut tui_state) {
                        ViewAction::Quit => break Ok(()),
                        ViewAction::Error(msg) => {
                            break Err(nika_engine::error::NikaError::ValidationError {
                                reason: msg,
                            })
                        }
                        _ => {}
                    }
                }
            }
        }
    };

    // Restore terminal
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}
