// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! App Routing and Action Dispatch
//!
//! Contains the apply_action method for routing user actions to appropriate handlers.
//!
//! # Chat Command Delegation
//!
//! Chat commands (ChatInfer, ChatExec, ChatFetch) are delegated to `ChatAgent` for
//! actual execution. Because ChatAgent methods are async, we spawn background tasks
//! via `spawn_tracked()` and communicate results back via the `stream_chunk_tx` channel.
//!
//! ## Architecture
//!
//! ```text
//! ViewAction::ChatInfer(prompt)
//!     │
//!     ▼
//! spawn_tracked(async {
//!     let mut agent = ChatAgent::new()?;
//!     agent.set_stream_chunk_tx(tx.clone());
//!     agent.infer(&prompt).await?;
//! })
//!     │
//!     ▼
//! poll_stream_chunks() in events.rs receives StreamChunk::Token events
//!     │
//!     ▼
//! ChatView displays real-time streaming response
//! ```
//!
//! ## Supported Commands
//!
//! - `/infer <prompt>` — LLM text generation via ChatAgent::infer()
//! - `/exec <command>` — Shell execution via ChatAgent::exec_command()
//! - `/fetch <url> [method]` — HTTP request via ChatAgent::fetch()
//!
//! ## Future Enhancements
//!
//! - ChatInvoke requires MCP client integration
//! - ChatAgent (multi-turn agentic loop) requires RigAgentLoop

use std::path::PathBuf;

use crate::chat_agent::ChatAgent;
use crate::InputMode;
// expand_includes removed — using parse_analyzed_with_includes instead
use nika_engine::event::EventLog;
use nika_engine::provider::rig::StreamChunk;
use nika_engine::runtime::Runner;

use super::super::views::{TuiView, View, ViewAction};
use super::types::Action;
use super::App;

impl App {
    /// Apply an action resulting from user input
    ///
    /// Routes actions to the appropriate handler method.
    pub(crate) fn apply_action(&mut self, action: Action) {
        match action {
            // ═══ Application Control ═══
            Action::Quit => {
                self.should_quit = true;
            }

            // ═══ View Navigation ═══
            Action::SwitchView(view) => {
                self.switch_to_view(view);
            }

            // ═══ Theme ═══
            Action::ToggleTheme => {
                self.toggle_theme();
            }

            // ═══ Scrolling ═══
            Action::ScrollUp => {
                self.handle_scroll_up();
            }
            Action::ScrollDown => {
                self.handle_scroll_down();
            }
            Action::ScrollToTop => {
                if self.current_view == TuiView::Command {
                    self.command_view.chat.scroll_to_top();
                }
            }
            Action::ScrollToBottom => {
                if self.current_view == TuiView::Command {
                    self.command_view.chat.scroll_to_bottom();
                }
            }

            // ═══ Pause/Step ═══
            Action::TogglePause => {
                self.state.toggle_pause();
                let label = if self.state.workflow.paused {
                    "Paused"
                } else {
                    "Resumed"
                };
                self.set_status(label);
            }
            Action::Step => {
                // Step one event while paused (single-step debugging)
                if self.state.workflow.paused {
                    self.state.workflow.step_requested = true;
                }
            }

            // ═══ Panel Focus ═══
            Action::FocusNext => {
                // Cycle to next panel
            }
            Action::FocusPrev => {
                // Cycle to previous panel
            }
            Action::FocusPanel(_n) => {
                // Focus specific panel by number
            }
            Action::CycleTab => {
                // Cycle tabs in focused panel
            }

            // ═══ Quick Actions ═══
            Action::CopyToClipboard => {
                // Copy last Nika (assistant) message content to clipboard
                let content = self.command_view.chat.last_nika_message_content();
                if let Some(text) = content {
                    match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&text)) {
                        Ok(()) => self.set_status("Copied to clipboard"),
                        Err(e) => self.set_status(&format!("Clipboard unavailable: {e}")),
                    }
                } else {
                    self.set_status("Nothing to copy");
                }
            }
            Action::RetryWorkflow => {
                self.retry_workflow();
            }
            Action::ExportTrace => match self.command_view.chat.export_session(None) {
                Ok(path) => self.set_status(&format!("Exported: {path}")),
                Err(e) => self.set_status(&format!("Export failed: {e}")),
            },

            // ═══ Breakpoints ═══
            Action::ToggleBreakpoint => {
                // Toggle breakpoint on current task
            }

            // ═══ Mouse Actions ═══
            Action::MouseClickPanel(_panel_id) => {
                // Handle panel click
            }
            Action::MouseScrollUp => {
                self.handle_scroll_up();
            }
            Action::MouseScrollDown => {
                self.handle_scroll_down();
            }

            // ═══ Notifications ═══
            Action::DismissNotification => {
                self.state.dismiss_notification();
            }
            Action::DismissAllNotifications => {
                self.state.dismiss_all_notifications();
            }
            Action::DismissError => {
                self.state.workflow.error_message = None;
                self.status_message = None;
            }

            // ═══ Filter/Search ═══
            Action::EnterFilter => {
                self.input_mode = InputMode::Search;
            }
            Action::ExitFilter => {
                self.input_mode = InputMode::Normal;
                self.state.filter_query.clear();
            }
            Action::FilterInput(c) => {
                self.state.filter_query.push(c);
            }
            Action::FilterBackspace => {
                self.state.filter_query.pop();
            }
            Action::FilterDelete | Action::FilterCursorLeft | Action::FilterCursorRight => {
                // Cursor movement in filter - simplified
            }
            Action::FilterClear => {
                self.state.filter_query.clear();
            }

            // ═══ Settings Overlay ═══
            Action::SettingsNextField
            | Action::SettingsPrevField
            | Action::SettingsToggleEdit
            | Action::SettingsInput(_)
            | Action::SettingsBackspace
            | Action::SettingsDelete
            | Action::SettingsCancelEdit
            | Action::SettingsSave
            | Action::SettingsCursorLeft
            | Action::SettingsCursorRight => {
                // Settings overlay actions - handled by SettingsView
            }

            // ═══ Mode Change ═══
            Action::SetMode(mode) => {
                self.state.ui.mode = mode;
            }

            // ═══ Continue (no-op) ═══
            Action::Continue => {}

            // ═══ View-Specific Actions ═══
            Action::ViewSpecific(view_action) => {
                self.apply_view_action(view_action);
            }
        }
    }

    /// Apply view-specific actions that need App-level orchestration
    ///
    /// and need access to App-level state (spawning tasks, MCP clients, etc.)
    fn apply_view_action(&mut self, action: ViewAction) {
        match action {
            // ═══════════════════════════════════════════════════════════════════
            // Chat Commands — Delegated to ChatAgent
            //
            // Because ChatAgent methods are async, we spawn background tasks via
            // spawn_tracked() and communicate results back via stream_chunk_tx.
            // The poll_stream_chunks() method in events.rs handles the responses.
            // ═══════════════════════════════════════════════════════════════════
            ViewAction::ChatInfer(prompt) => {
                self.command_view
                    .chat
                    .add_user_message(format!("/infer {}", prompt));
                self.set_status("Inferring...");
                self.spawn_chat_command(move |mut agent, tx| async move {
                    agent.set_stream_chunk_tx(tx.clone());
                    let response = agent.infer(&prompt).await.map_err(|e| e.to_string())?;
                    let _ = tx.send(StreamChunk::Done(response)).await;
                    Ok(())
                });
            }
            ViewAction::ChatExec(cmd) => {
                self.command_view
                    .chat
                    .add_user_message(format!("/exec {}", cmd));
                self.set_status("Executing...");
                self.spawn_chat_command(move |mut agent, tx| async move {
                    agent.set_stream_chunk_tx(tx.clone());
                    let output = agent.exec_command(&cmd).await.map_err(|e| e.to_string())?;
                    let _ = tx.send(StreamChunk::Done(output)).await;
                    Ok(())
                });
            }
            ViewAction::ChatFetch(url, method) => {
                self.command_view
                    .chat
                    .add_user_message(format!("/fetch {} {}", method, url));
                self.set_status("Fetching...");
                self.spawn_chat_command(move |mut agent, tx| async move {
                    agent.set_stream_chunk_tx(tx.clone());
                    let response = agent
                        .fetch(&url, &method)
                        .await
                        .map_err(|e| e.to_string())?;
                    let _ = tx.send(StreamChunk::Done(response)).await;
                    Ok(())
                });
            }
            ViewAction::ChatInvoke(tool, server, params) => {
                self.command_view.chat.add_user_message(format!(
                    "/invoke {} {}",
                    tool,
                    server.as_deref().unwrap_or("(auto)")
                ));
                self.set_status("Invoking MCP tool...");
                self.spawn_chat_command(move |agent, tx| async move {
                    let result = agent
                        .invoke(&tool, server.as_deref(), params)
                        .await
                        .map_err(|e| e.to_string())?;
                    let _ = tx.send(StreamChunk::Done(result)).await;
                    Ok(())
                });
            }
            ViewAction::ChatAgent(goal, max_turns, extended, servers) => {
                self.command_view
                    .chat
                    .add_user_message(format!("/agent {}", goal));
                self.set_status("Running agent...");
                self.spawn_chat_command(move |agent, tx| async move {
                    let result = agent
                        .run_agent(goal, max_turns, extended, servers)
                        .await
                        .map_err(|e| e.to_string())?;
                    let _ = tx.send(StreamChunk::Done(result)).await;
                    Ok(())
                });
            }
            ViewAction::ChatClear => {
                self.command_view.chat.messages.clear();
                self.set_status("Chat cleared");
            }
            ViewAction::ChatModelSwitch(provider) => {
                // Update ChatView state so warning bar + status bar reflect the new provider
                let provider_id = provider.command_name().to_string();
                let default_model =
                    nika_core::catalogs::default_model_for_provider(provider.command_name())
                        .unwrap_or(provider.command_name());
                self.command_view.chat.set_model(default_model);
                self.command_view.chat.set_provider(provider.name());
                self.command_view.chat.provider.id = provider_id;

                // Recreate ChatAgent with new provider
                self.chat_agent = match ChatAgent::new() {
                    Ok(agent) => Some(agent),
                    Err(e) => {
                        tracing::warn!("Chat agent unavailable after provider switch: {e}");
                        None
                    }
                };

                self.set_status(&format!("Switched to {}", provider.name()));
                tracing::info!("ChatModelSwitch: {:?}", provider);
            }
            ViewAction::ChatMcp(mcp_action) => {
                use crate::command::McpAction;
                match mcp_action {
                    McpAction::List => {
                        let servers = &self.command_view.chat.session_context.mcp_servers;
                        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
                        let msg = if names.is_empty() {
                            "No MCP servers configured".to_string()
                        } else {
                            format!("MCP servers: {}", names.join(", "))
                        };
                        self.command_view.chat.add_system_message(msg);
                    }
                    McpAction::Select(servers) => {
                        self.command_view
                            .chat
                            .set_mcp_servers(servers.iter().cloned());
                        self.set_status("MCP servers updated");
                    }
                    McpAction::Toggle(server) => {
                        let servers = &mut self.command_view.chat.session_context.mcp_servers;
                        if let Some(s) = servers.iter_mut().find(|s| s.name == server) {
                            use crate::widgets::McpStatus;
                            let was_cold =
                                s.status == McpStatus::Cold || s.status == McpStatus::Error;
                            s.status = if was_cold {
                                McpStatus::Connected
                            } else {
                                McpStatus::Cold
                            };
                            let label = if was_cold { "enabled" } else { "disabled" };
                            self.set_status(&format!("MCP server '{}' {}", server, label));
                        } else {
                            self.set_status(&format!("MCP server '{}' not found", server));
                        }
                    }
                }
            }
            ViewAction::SendChatMessage(msg) => {
                self.command_view.chat.add_user_message(msg.clone());
                self.set_status("Message sent");
                tracing::debug!("SendChatMessage: {}", msg);
            }

            // Workflow actions
            ViewAction::RunWorkflow(path) => {
                self.run_workflow(path);
            }
            ViewAction::OpenInStudio(path) => match self.studio_view.load_file(path.clone()) {
                Ok(()) => {
                    self.switch_to_view(TuiView::Studio);
                    self.set_status(&format!("Opened: {}", path.display()));
                }
                Err(e) => {
                    self.set_status(&format!("Failed to open: {}", e));
                    tracing::error!("OpenInStudio failed: {}", e);
                }
            },
            ViewAction::ValidateWorkflow(path) => {
                self.set_status(&format!("Validating: {}", path.display()));
                tracing::debug!("ValidateWorkflow: {}", path.display());
            }

            // Theme actions
            ViewAction::SetTheme(variant) => {
                self.cosmic_theme = super::super::cosmic_theme::CosmicTheme::new(variant);
                self.theme = self.cosmic_theme.as_theme();
                self.set_status(&format!("Theme set to: {:?}", variant));
            }
            ViewAction::ToggleTheme => {
                self.toggle_theme();
            }

            // Provider actions - triggers async verification via lifecycle.rs
            ViewAction::VerifyProviders => {
                self.set_status("Verifying providers...");
                // Spawn async provider verification tasks (uses cache for TTL)
                self.spawn_provider_verification();
                self.spawn_mcp_verification();
            }
            ViewAction::RefreshVerification => {
                self.set_status("Refreshing verification...");
                // Invalidate cache before re-verification
                {
                    let mut cache = self.verification_cache.lock();
                    cache.invalidate_all();
                }
                // Re-spawn verification with fresh cache
                self.spawn_provider_verification();
                self.spawn_provider_verification_timeout();
                self.spawn_mcp_verification();
            }
            ViewAction::ProviderSelectorConfirm { provider_id, model } => {
                self.set_status(&format!("Selected: {} / {}", provider_id, model));
                tracing::debug!("ProviderSelectorConfirm: {} / {}", provider_id, model);
            }

            // Native model actions
            ViewAction::PullNativeModel(model) => {
                self.pull_native_model(model);
            }
            ViewAction::DeleteNativeModel(model) => {
                self.delete_native_model(model);
            }
            ViewAction::RefreshNativeModels => {
                self.refresh_native_models();
            }

            // Error handling
            ViewAction::Error(msg) => {
                self.set_status(&format!("Error: {}", msg));
            }

            // Status message (success, info, warning, error) with auto-dismiss
            ViewAction::StatusMessage(msg) => {
                self.state.status_messages.push(msg);
            }

            // Launch wizard
            ViewAction::LaunchWizard => {
                self.should_launch_wizard = true;
                self.should_quit = true;
                self.set_status("Launching setup wizard...");
            }

            // Navigation - already handled by from_view_action conversion
            ViewAction::None
            | ViewAction::Quit
            | ViewAction::SwitchView(_)
            | ViewAction::OpenControl => {
                // These are converted to Action variants directly
            }
        }
    }

    /// Switch to a specific view with appropriate mode changes
    ///
    /// Calls lifecycle hooks (on_leave, on_enter)
    fn switch_to_view(&mut self, view: TuiView) {
        // Skip if already on this view
        if self.current_view == view {
            return;
        }

        // Call on_leave for the current view
        self.call_view_on_leave(self.current_view);

        // Switch view
        let old_view = self.current_view;
        self.current_view = view;
        self.state.dirty.mark_all(); // Force full redraw on view switch

        // Set appropriate input mode
        self.input_mode = match view {
            TuiView::Command => InputMode::Insert,
            _ => InputMode::Normal,
        };

        // Call on_enter for the new view
        self.call_view_on_enter(view);

        tracing::debug!("Switched view: {:?} -> {:?}", old_view, view);
    }

    /// Call on_enter lifecycle hook for a view
    pub(super) fn call_view_on_enter(&mut self, view: TuiView) {
        match view {
            TuiView::Studio => self.studio_view.on_enter(&mut self.state),
            TuiView::Command => self.command_view.on_enter(&mut self.state),
            TuiView::Control => self.control_view.on_enter(&mut self.state),
        }
    }

    /// Call on_leave lifecycle hook for a view
    fn call_view_on_leave(&mut self, view: TuiView) {
        match view {
            TuiView::Studio => self.studio_view.on_leave(&mut self.state),
            TuiView::Command => self.command_view.on_leave(&mut self.state),
            TuiView::Control => self.control_view.on_leave(&mut self.state),
        }
    }

    /// Handle scroll up action for current view
    fn handle_scroll_up(&mut self) {
        if self.current_view == TuiView::Command {
            self.command_view.chat.scroll_up();
        }
    }

    /// Handle scroll down action for current view
    fn handle_scroll_down(&mut self) {
        if self.current_view == TuiView::Command {
            self.command_view.chat.scroll_down();
        }
    }

    /// Run a workflow from a file path
    ///
    /// Executes the workflow and displays results in Runner view.
    /// This is the glue code that connects:
    /// - Studio view (F5 to run)
    /// - Workflow parsing (ast module)
    /// - Runner execution (runtime module)
    /// - TaskBox widgets (MonitorView)
    fn run_workflow(&mut self, path: PathBuf) {
        tracing::info!("Running workflow: {}", path.display());

        // 1. Read and parse workflow file
        let yaml_content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) => {
                self.set_status(&format!("Failed to read file: {}", e));
                tracing::error!("Failed to read workflow file: {}", e);
                return;
            }
        };

        // 2. Parse with include expansion (raw → expand_raw_include → analyze)
        let base_path = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));

        let workflow =
            match nika_engine::ast::parse_analyzed_with_includes(&yaml_content, base_path) {
                Ok(w) => w,
                Err(e) => {
                    self.set_status(&format!("Parse error: {}", e));
                    tracing::error!("Failed to parse workflow: {}", e);
                    return;
                }
            };

        // 3. Create EventLog with broadcast channel for TUI
        let (event_log, event_rx) = EventLog::new_with_broadcast();

        // 4. Wire broadcast receiver to App (must be before spawn)
        self.broadcast_rx = Some(event_rx);

        // 5. Reset TUI state for new workflow execution
        let workflow_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workflow")
            .to_string();
        self.state.workflow = crate::state::WorkflowState::new(workflow_name.clone());
        self.state.tasks.clear();
        self.workflow_done = false;

        // 6. Create Runner with quiet mode (no console output)
        let mut runner = match Runner::with_event_log(workflow, event_log) {
            Ok(r) => r.with_base_path(base_path.to_path_buf()).quiet(),
            Err(e) => {
                self.set_status(&format!("DAG error: {}", e));
                tracing::error!("Failed to construct Runner DAG: {}", e);
                return;
            }
        };

        // 6b. Wire custom endpoints from config.toml
        let config = nika_engine::config::NikaConfig::load()
            .unwrap_or_default()
            .with_env();
        if !config.endpoints.is_empty() {
            if let Ok(resolved) =
                nika_engine::provider::endpoints::resolve_endpoints(&config.endpoints)
            {
                runner.with_custom_endpoints(resolved);
            }
        }

        // 7. Spawn Runner in background task
        self.spawn_tracked(async move {
            tracing::info!("Starting workflow execution: {}", workflow_name);
            match runner.run().await {
                Ok(output) => {
                    tracing::info!(
                        "Workflow '{}' completed: {} bytes output",
                        workflow_name,
                        output.len()
                    );
                }
                Err(e) => {
                    tracing::error!("Workflow '{}' failed: {}", workflow_name, e);
                }
            }
        });

        // 8. Switch to Command view in Monitor mode and update status
        self.command_view.switch_to_monitor();
        self.switch_to_view(TuiView::Command);
        self.set_status(&format!("Running: {}", path.display()));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Chat Command Helper
    // ═══════════════════════════════════════════════════════════════════════════

    /// Spawn a background chat command with ChatAgent creation boilerplate.
    ///
    /// Handles ChatAgent creation, error reporting, and task spawning.
    /// The closure receives a ready ChatAgent and the stream_chunk sender.
    fn spawn_chat_command<F, Fut>(&mut self, op: F)
    where
        F: FnOnce(ChatAgent, tokio::sync::mpsc::Sender<StreamChunk>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = std::result::Result<(), String>> + Send,
    {
        let tx = self.stream_chunk_tx.clone();
        self.spawn_tracked(async move {
            match ChatAgent::new() {
                Ok(agent) => {
                    if let Err(msg) = op(agent, tx.clone()).await {
                        let _ = tx.send(StreamChunk::Error(msg)).await;
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(StreamChunk::Error(format!(
                            "Failed to create ChatAgent: {}",
                            e
                        )))
                        .await;
                }
            }
        });
    }
}
