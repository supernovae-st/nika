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

use crate::ast::expand_includes;
use crate::core::backend::DownloadRequest;
use crate::core::models::find_model;
use crate::core::storage::{default_model_dir, HuggingFaceStorage};
use crate::event::EventLog;
use crate::provider::rig::StreamChunk;
use crate::runtime::Runner;
use crate::tui::chat_agent::ChatAgent;
use crate::tui::InputMode;

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
                self.handle_scroll_up();
            }
            Action::ScrollToBottom => {
                self.handle_scroll_down();
            }

            // ═══ Pause/Step ═══
            Action::TogglePause => {
                self.state.workflow.paused = !self.state.workflow.paused;
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
                // Copy current panel content
            }
            Action::RetryWorkflow => {
                self.retry_workflow();
            }
            Action::ExportTrace => {
                // Export trace to file
            }

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
                self.state.notifications.pop();
            }
            Action::DismissAllNotifications => {
                self.state.notifications.clear();
            }
            Action::DismissError => {
                // Dismiss error message
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

            // ═══ Chat Overlay ═══
            Action::ChatOverlayInput(_)
            | Action::ChatOverlayBackspace
            | Action::ChatOverlayDelete
            | Action::ChatOverlayCursorLeft
            | Action::ChatOverlayCursorRight
            | Action::ChatOverlayHistoryUp
            | Action::ChatOverlayHistoryDown
            | Action::ChatOverlaySend
            | Action::ChatOverlayClear
            | Action::ChatOverlayScrollUp
            | Action::ChatOverlayScrollDown => {
                // Chat overlay actions - handled by ChatView
            }

            // ═══ Mode Change ═══
            Action::SetMode(mode) => {
                self.state.mode = mode;
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
                self.chat_view
                    .add_user_message(format!("/infer {}", prompt));
                self.set_status("Inferring...");
                tracing::debug!("ChatInfer: {}", prompt);

                // Clone what we need for the async task
                let tx = self.stream_chunk_tx.clone();
                let prompt_clone = prompt.clone();

                self.spawn_tracked(async move {
                    // Create ChatAgent and wire streaming
                    match ChatAgent::new() {
                        Ok(mut agent) => {
                            agent.set_stream_chunk_tx(tx.clone());
                            match agent.infer(&prompt_clone).await {
                                Ok(_response) => {
                                    // Response already streamed via stream_chunk_tx
                                    tracing::debug!("ChatInfer completed");
                                }
                                Err(e) => {
                                    let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                                }
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
            ViewAction::ChatExec(cmd) => {
                self.chat_view.add_user_message(format!("/exec {}", cmd));
                self.set_status("Executing...");
                tracing::debug!("ChatExec: {}", cmd);

                let tx = self.stream_chunk_tx.clone();
                let cmd_clone = cmd.clone();

                self.spawn_tracked(async move {
                    match ChatAgent::new() {
                        Ok(mut agent) => {
                            agent.set_stream_chunk_tx(tx.clone());
                            match agent.exec_command(&cmd_clone).await {
                                Ok(output) => {
                                    // Send result as a message (exec doesn't stream)
                                    let _ = tx.send(StreamChunk::Done(output)).await;
                                }
                                Err(e) => {
                                    let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                                }
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
            ViewAction::ChatFetch(url, method) => {
                self.chat_view
                    .add_user_message(format!("/fetch {} {}", method, url));
                self.set_status("Fetching...");
                tracing::debug!("ChatFetch: {} {}", method, url);

                let tx = self.stream_chunk_tx.clone();
                let url_clone = url.clone();
                let method_clone = method.clone();

                self.spawn_tracked(async move {
                    match ChatAgent::new() {
                        Ok(mut agent) => {
                            agent.set_stream_chunk_tx(tx.clone());
                            match agent.fetch(&url_clone, &method_clone).await {
                                Ok(response) => {
                                    // Send result (fetch doesn't stream)
                                    let _ = tx.send(StreamChunk::Done(response)).await;
                                }
                                Err(e) => {
                                    let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                                }
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
            ViewAction::ChatInvoke(tool, server, params) => {
                self.chat_view.add_user_message(format!(
                    "/invoke {} {}",
                    tool,
                    server.as_deref().unwrap_or("(auto)")
                ));
                self.set_status("Invoking MCP tool...");
                tracing::debug!("ChatInvoke: {} {:?}", tool, server);

                let tx = self.stream_chunk_tx.clone();
                let tool_clone = tool.clone();
                let server_clone = server.clone();
                let params_clone = params.clone();

                self.spawn_tracked(async move {
                    match ChatAgent::new() {
                        Ok(agent) => {
                            match agent
                                .invoke(&tool_clone, server_clone.as_deref(), params_clone)
                                .await
                            {
                                Ok(result) => {
                                    let _ = tx.send(StreamChunk::Done(result)).await;
                                }
                                Err(e) => {
                                    let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                                }
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
            // ChatAgent: Run agentic loop with MCP tools
            ViewAction::ChatAgent(goal, max_turns, extended, servers) => {
                self.chat_view.add_user_message(format!("/agent {}", goal));
                self.set_status("Running agent...");
                tracing::debug!(
                    "ChatAgent: {} (turns={:?}, ext={}, servers={:?})",
                    goal,
                    max_turns,
                    extended,
                    servers
                );

                let tx = self.stream_chunk_tx.clone();
                let goal_clone = goal.clone();
                let servers_clone = servers.clone();

                self.spawn_tracked(async move {
                    match ChatAgent::new() {
                        Ok(agent) => {
                            match agent
                                .run_agent(goal_clone, max_turns, extended, servers_clone)
                                .await
                            {
                                Ok(result) => {
                                    let _ = tx.send(StreamChunk::Done(result)).await;
                                }
                                Err(e) => {
                                    let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                                }
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
            ViewAction::ChatClear => {
                self.chat_view.messages.clear();
                self.set_status("Chat cleared");
            }
            ViewAction::ChatModelSwitch(provider) => {
                self.set_status(&format!("Switched to {:?}", provider));
                tracing::debug!("ChatModelSwitch: {:?}", provider);
            }
            ViewAction::ChatMcp(_mcp_action) => {
                self.set_status("MCP action received");
            }
            ViewAction::SendChatMessage(msg) => {
                self.chat_view.add_user_message(msg.clone());
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

            // Chat overlay
            ViewAction::ToggleChatOverlay => {
                // Note: Chat overlay visibility is controlled via input mode, not state toggle
                self.set_status("Chat overlay toggled");
            }

            // Error handling
            ViewAction::Error(msg) => {
                self.set_status(&format!("Error: {}", msg));
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
            | ViewAction::OpenSettings => {
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

        // Set appropriate input mode
        self.input_mode = match view {
            TuiView::Chat => InputMode::Insert,
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
            TuiView::Runner => self.monitor_view.on_enter(&mut self.state),
            TuiView::Chat => self.chat_view.on_enter(&mut self.state),
            TuiView::Settings => self.settings_view.on_enter(&mut self.state),
        }
    }

    /// Call on_leave lifecycle hook for a view
    fn call_view_on_leave(&mut self, view: TuiView) {
        match view {
            TuiView::Studio => self.studio_view.on_leave(&mut self.state),
            TuiView::Runner => self.monitor_view.on_leave(&mut self.state),
            TuiView::Chat => self.chat_view.on_leave(&mut self.state),
            TuiView::Settings => self.settings_view.on_leave(&mut self.state),
        }
    }

    /// Handle scroll up action for current view
    fn handle_scroll_up(&mut self) {
        // Delegate to current view's scroll handler
        self.state.chat_overlay.scroll_up();
    }

    /// Handle scroll down action for current view
    fn handle_scroll_down(&mut self) {
        // Delegate to current view's scroll handler
        self.state.chat_overlay.scroll_down();
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

        let workflow = match crate::ast::parse_workflow(&yaml_content) {
            Ok(w) => w,
            Err(e) => {
                self.set_status(&format!("Parse error: {}", e));
                tracing::error!("Failed to parse workflow: {}", e);
                return;
            }
        };

        // 2. Expand includes (DAG fusion)
        let base_path = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));

        let workflow = match expand_includes(workflow, base_path) {
            Ok(w) => w,
            Err(e) => {
                self.set_status(&format!("Include error: {}", e));
                tracing::error!("Failed to expand includes: {}", e);
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
        self.state.workflow = crate::tui::state::WorkflowState::new(workflow_name.clone());
        self.state.tasks.clear();
        self.workflow_done = false;

        // 6. Create Runner with quiet mode (no console output)
        let mut runner = Runner::with_event_log(workflow, event_log).quiet();

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

        // 8. Switch to Runner view and update status
        self.switch_to_view(TuiView::Runner);
        self.set_status(&format!("Running: {}", path.display()));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Native Model Management
    // ═══════════════════════════════════════════════════════════════════════════

    /// Pull a native model from HuggingFace.
    ///
    /// Spawns a background task to download the model with progress updates
    /// via the stream_chunk channel.
    fn pull_native_model(&mut self, model: String) {
        self.set_status(&format!("Starting pull: {}", model));

        // Look up the model in KNOWN_MODELS
        let known_model = match find_model(&model) {
            Some(m) => m,
            None => {
                self.set_status(&format!("Unknown model: {}", model));
                tracing::warn!("Unknown model ID: {}", model);
                return;
            }
        };

        // Clone data for the async task
        let tx = self.stream_chunk_tx.clone();
        let model_clone = model.clone();
        let repo = known_model.hf_repo.to_string();
        let filename = known_model.default_file.to_string();

        // Send start event
        let _ = tx.try_send(StreamChunk::NativeModelPullStarted {
            model: model.clone(),
        });

        self.spawn_tracked(async move {
            // Create storage
            let storage = match HuggingFaceStorage::new(default_model_dir()) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx
                        .send(StreamChunk::NativeModelPullFailed {
                            model: model_clone,
                            error: e.to_string(),
                        })
                        .await;
                    return;
                }
            };

            // Create download request
            let request = DownloadRequest::huggingface(&repo, &filename);

            // Clone tx for progress callback
            let progress_tx = tx.clone();
            let progress_model = model_clone.clone();

            // Download with progress
            let result = storage
                .download(&request, move |progress| {
                    let _ = progress_tx.try_send(StreamChunk::NativeModelPullProgress {
                        model: progress_model.clone(),
                        status: progress.status.clone(),
                        completed: progress.completed,
                        total: progress.total,
                    });
                })
                .await;

            match result {
                Ok(download_result) => {
                    tracing::info!(
                        "Model {} downloaded to {:?} ({} bytes)",
                        model_clone,
                        download_result.path,
                        download_result.size
                    );
                    let _ = tx
                        .send(StreamChunk::NativeModelPulled {
                            model: model_clone,
                            path: download_result.path.display().to_string(),
                            size: download_result.size,
                        })
                        .await;
                }
                Err(e) => {
                    tracing::error!("Failed to pull model {}: {}", model_clone, e);
                    let _ = tx
                        .send(StreamChunk::NativeModelPullFailed {
                            model: model_clone,
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        });
    }

    /// Delete a native model from local storage.
    fn delete_native_model(&mut self, model: String) {
        self.set_status(&format!("Deleting: {}", model));

        let tx = self.stream_chunk_tx.clone();
        let model_clone = model.clone();

        self.spawn_tracked(async move {
            // Create storage
            let storage = match HuggingFaceStorage::new(default_model_dir()) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx
                        .send(StreamChunk::NativeModelDeleteFailed {
                            model: model_clone,
                            error: e.to_string(),
                        })
                        .await;
                    return;
                }
            };

            // Delete the model
            match storage.delete(&model_clone) {
                Ok(()) => {
                    tracing::info!("Deleted model: {}", model_clone);
                    let _ = tx
                        .send(StreamChunk::NativeModelDeleted { model: model_clone })
                        .await;
                }
                Err(e) => {
                    tracing::error!("Failed to delete model {}: {}", model_clone, e);
                    let _ = tx
                        .send(StreamChunk::NativeModelDeleteFailed {
                            model: model_clone,
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        });
    }

    /// Refresh the list of native models.
    fn refresh_native_models(&mut self) {
        self.set_status("Refreshing native models...");

        let tx = self.stream_chunk_tx.clone();

        self.spawn_tracked(async move {
            // Create storage
            let storage = match HuggingFaceStorage::new(default_model_dir()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create storage for refresh: {}", e);
                    return;
                }
            };

            // List models
            match storage.list_models() {
                Ok(models) => {
                    let count = models.len();
                    tracing::info!("Found {} native models", count);
                    let _ = tx.send(StreamChunk::NativeModelsRefreshed { count }).await;
                }
                Err(e) => {
                    tracing::error!("Failed to list native models: {}", e);
                }
            }
        });
    }
}
