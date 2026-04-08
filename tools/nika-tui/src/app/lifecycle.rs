// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! App Lifecycle Methods
//!
//! Contains constructors, initialization, cleanup, and background task management.
//!
//! # Functions
//!
//! - `spawn_tracked()` - Spawn a background task with abort tracking
//! - `spawn_provider_verification()` - Verify LLM providers async
//! - `spawn_provider_verification_timeout()` - Timeout watcher for provider verification
//! - `spawn_mcp_verification()` - Verify MCP servers async
//! - `save_current_session()` - Save chat session to disk
//! - `cancel_background_tasks()` - Abort all tracked background tasks
//! - `cleanup()` - Restore terminal state

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};

use nika_engine::error::{NikaError, Result};
use nika_engine::mcp::{McpClient, McpConfig};
use nika_engine::provider::rig::RigProvider;

use super::super::session::save_session;
use super::super::verification::VerificationEntry;
use super::App;
use nika_engine::provider::rig::StreamChunk;

/// Global guard to prevent concurrent provider verification spawns
static PROVIDER_VERIFICATION_RUNNING: AtomicBool = AtomicBool::new(false);

impl App {
    /// Spawn a tracked background task that will be cancelled on cleanup
    ///
    /// Use this instead of raw `tokio::spawn()` for tasks that should be
    /// cleaned up when the TUI exits. The task's AbortHandle is stored for
    /// later cancellation via `cancel_background_tasks()`.
    pub(crate) fn spawn_tracked<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let handle = tokio::spawn(future);
        // PERF: parking_lot::Mutex doesn't poison, so this is infallible
        self.background_handles.lock().push(handle.abort_handle());
    }

    /// Spawn async provider verification tasks
    ///
    /// Verifies all configured LLM providers in parallel, sending StreamChunk events
    /// to update the session context display in real-time.
    ///
    /// Uses TTL-based caching to avoid redundant API calls (30s default).
    pub(crate) fn spawn_provider_verification(&self) {
        // Guard against concurrent spawns (e.g., rapid refresh button clicks)
        if PROVIDER_VERIFICATION_RUNNING.swap(true, Ordering::SeqCst) {
            tracing::debug!("Provider verification already in progress, skipping");
            return;
        }

        let tx = self.stream_chunk_tx.clone();
        use nika_core::catalogs::default_model_for_provider;
        use nika_core::provider_name::ProviderName;

        /// Cloud providers that require API key verification.
        const CLOUD_PROVIDERS: &[ProviderName] = &[
            ProviderName::Anthropic,
            ProviderName::OpenAI,
            ProviderName::Mistral,
            ProviderName::Groq,
            ProviderName::DeepSeek,
            ProviderName::Gemini,
            ProviderName::XAi,
        ];

        let provider_ids: Vec<(&str, String)> = CLOUD_PROVIDERS
            .iter()
            .map(|p| {
                let id = p.as_str();
                let model = default_model_for_provider(id)
                    .unwrap_or("unknown")
                    .to_string();
                (id, model)
            })
            .collect();
        let cache = Arc::clone(&self.verification_cache);

        // Check cache and send events for cached providers, mark uncached as verifying
        for (provider_id, default_model) in &provider_ids {
            let model = default_model.to_string();

            // Check if we have a valid cached result
            let cached = {
                let cache_guard = cache.lock();
                cache_guard.get_provider(provider_id).and_then(|entry| {
                    if cache_guard.is_valid(entry) {
                        Some(entry.clone())
                    } else {
                        None
                    }
                })
            };

            if let Some(entry) = cached {
                // Send cached result directly
                match entry.status {
                    super::super::widgets::VerifyStatus::Verified => {
                        let _ = tx.try_send(StreamChunk::ProviderVerified {
                            provider: provider_id.to_string(),
                            model: entry.model.unwrap_or(model),
                            latency_ms: entry.latency.map(|d| d.as_millis() as u64).unwrap_or(0),
                        });
                    }
                    super::super::widgets::VerifyStatus::Failed => {
                        let _ = tx.try_send(StreamChunk::ProviderVerifyFailed {
                            provider: provider_id.to_string(),
                            error: entry.error.unwrap_or_else(|| "Unknown error".to_string()),
                        });
                    }
                    _ => {
                        // Verifying or Unknown - re-verify
                        let _ = tx.try_send(StreamChunk::ProviderVerifying {
                            provider: provider_id.to_string(),
                            model,
                        });
                    }
                }
            } else {
                // Mark as verifying - will spawn task below
                let _ = tx.try_send(StreamChunk::ProviderVerifying {
                    provider: provider_id.to_string(),
                    model,
                });
            }
        }

        // Spawn verification tasks only for providers NOT in valid cache
        for (provider_id_str, _) in provider_ids {
            let provider_id = provider_id_str.to_string();

            // Skip if cached
            {
                let cache_guard = cache.lock();
                if cache_guard.has_valid_provider(&provider_id) {
                    continue;
                }
            }

            let tx = tx.clone();
            let cache = Arc::clone(&cache);

            self.spawn_tracked(async move {
                // Create provider and check if configured
                let provider_opt: Option<RigProvider> = match provider_id.as_str() {
                    "claude" => {
                        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                            let p = RigProvider::claude();
                            if p.is_configured() {
                                Some(p)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    "openai" => {
                        if std::env::var("OPENAI_API_KEY").is_ok() {
                            let p = RigProvider::openai();
                            if p.is_configured() {
                                Some(p)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    "mistral" => {
                        if std::env::var("MISTRAL_API_KEY").is_ok() {
                            let p = RigProvider::mistral();
                            if p.is_configured() {
                                Some(p)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    "groq" => {
                        if std::env::var("GROQ_API_KEY").is_ok() {
                            let p = RigProvider::groq();
                            if p.is_configured() {
                                Some(p)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    "deepseek" => {
                        if std::env::var("DEEPSEEK_API_KEY").is_ok() {
                            let p = RigProvider::deepseek();
                            if p.is_configured() {
                                Some(p)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    "native" => {
                        #[cfg(feature = "native-inference")]
                        {
                            Some(RigProvider::native())
                        }
                        #[cfg(not(feature = "native-inference"))]
                        {
                            None
                        }
                    }
                    _ => None,
                };

                const SINGLE_PROVIDER_TIMEOUT: Duration = Duration::from_secs(10);

                match provider_opt {
                    Some(provider) => {
                        match tokio::time::timeout(SINGLE_PROVIDER_TIMEOUT, provider.verify()).await
                        {
                            Ok(Ok(result)) => {
                                // Cache the successful result
                                {
                                    let mut cache_guard = cache.lock();
                                    cache_guard.set_provider(
                                        provider_id.clone(),
                                        VerificationEntry::verified(
                                            result.latency,
                                            Some(result.model.clone()),
                                        ),
                                    );
                                }
                                let _ = tx
                                    .send(StreamChunk::ProviderVerified {
                                        provider: provider_id,
                                        model: result.model,
                                        latency_ms: result.latency.as_millis() as u64,
                                    })
                                    .await;
                            }
                            Ok(Err(e)) => {
                                // Cache the failure
                                {
                                    let mut cache_guard = cache.lock();
                                    cache_guard.set_provider(
                                        provider_id.clone(),
                                        VerificationEntry::failed(e.to_string()),
                                    );
                                }
                                let _ = tx
                                    .send(StreamChunk::ProviderVerifyFailed {
                                        provider: provider_id,
                                        error: e.to_string(),
                                    })
                                    .await;
                            }
                            Err(_timeout) => {
                                tracing::warn!(
                                    provider = %provider_id,
                                    "Provider verification timed out after 10s"
                                );
                                {
                                    let mut cache_guard = cache.lock();
                                    cache_guard.set_provider(
                                        provider_id.clone(),
                                        VerificationEntry::failed(
                                            "Verification timeout (10s)".to_string(),
                                        ),
                                    );
                                }
                                let _ = tx
                                    .send(StreamChunk::ProviderVerifyFailed {
                                        provider: provider_id,
                                        error: "Verification timeout (10s)".to_string(),
                                    })
                                    .await;
                            }
                        }
                    }
                    None => {
                        tracing::debug!(provider = %provider_id, "Provider not configured");
                        let _ = tx
                            .send(StreamChunk::ProviderNotConfigured {
                                provider: provider_id,
                            })
                            .await;
                    }
                }
            });
        }

        // Release the guard after all tasks had time to complete (12s > 10s timeout)
        self.spawn_tracked(async {
            tokio::time::sleep(Duration::from_secs(12)).await;
            PROVIDER_VERIFICATION_RUNNING.store(false, Ordering::SeqCst);
        });
    }

    /// Spawn provider verification timeout watcher
    ///
    /// After 5 seconds, checks if ANY provider has been verified.
    /// If not, sends ProviderVerificationTimeout event to show fallback UI.
    pub(crate) fn spawn_provider_verification_timeout(&self) {
        const PROVIDER_VERIFICATION_TIMEOUT: Duration = Duration::from_secs(5);

        let tx = self.stream_chunk_tx.clone();
        let cache = Arc::clone(&self.verification_cache);

        self.spawn_tracked(async move {
            tokio::time::sleep(PROVIDER_VERIFICATION_TIMEOUT).await;

            // Check if ANY provider is verified
            let has_verified = {
                let cache_guard = cache.lock();
                cache_guard.has_any_verified_provider()
            };

            if !has_verified {
                // Send timeout event
                let _ = tx.try_send(StreamChunk::ProviderVerificationTimeout);
            }
        });
    }

    /// Spawn async MCP server verification tasks
    ///
    /// Pings all configured MCP servers in parallel, sending StreamChunk events
    /// to update the session context display in real-time.
    ///
    /// Uses TTL-based caching to avoid redundant MCP pings (30s default).
    pub(crate) fn spawn_mcp_verification(&self) {
        let pool_configs = self.mcp_pool.configs();
        if pool_configs.is_empty() {
            tracing::debug!("No MCP servers configured, skipping verification");
            return;
        }

        let tx = self.stream_chunk_tx.clone();
        let cache = Arc::clone(&self.verification_cache);
        let configs: Vec<_> = pool_configs
            .iter()
            .map(|(name, config)| (name.clone(), config.clone()))
            .collect();
        // Drop the read guard before spawning async work
        drop(pool_configs);

        // Check cache and send events for cached servers, mark uncached as pinging
        for (server_name, _config) in &configs {
            // Check if we have a valid cached result
            let cached = {
                let cache_guard = cache.lock();
                cache_guard.get_mcp(server_name).and_then(|entry| {
                    if cache_guard.is_valid(entry) {
                        Some(entry.clone())
                    } else {
                        None
                    }
                })
            };

            if let Some(entry) = cached {
                // Send cached result directly
                match entry.status {
                    super::super::widgets::VerifyStatus::Verified => {
                        let _ = tx.try_send(StreamChunk::McpPinged {
                            server: server_name.clone(),
                            latency_ms: entry.latency.map(|d| d.as_millis() as u64).unwrap_or(0),
                            tool_count: entry.tool_count.unwrap_or(0),
                        });
                    }
                    super::super::widgets::VerifyStatus::Failed => {
                        let _ = tx.try_send(StreamChunk::McpError {
                            server_name: server_name.clone(),
                            error: entry.error.unwrap_or_else(|| "Unknown error".to_string()),
                        });
                    }
                    _ => {
                        // Verifying or Unknown - re-ping
                        let _ = tx.try_send(StreamChunk::McpPinging {
                            server: server_name.clone(),
                        });
                    }
                }
            } else {
                // Mark as pinging - will spawn task below
                let _ = tx.try_send(StreamChunk::McpPinging {
                    server: server_name.clone(),
                });
            }
        }

        // Spawn ping tasks only for servers NOT in valid cache
        for (server_name, config) in configs {
            // Skip if cached
            {
                let cache_guard = cache.lock();
                if cache_guard.has_valid_mcp(&server_name) {
                    continue;
                }
            }

            let tx = tx.clone();
            let cache = Arc::clone(&cache);
            let server_name_for_config = server_name.clone();

            self.spawn_tracked(async move {
                // Build MCP config from inline config
                let mut mcp_config = McpConfig::new(&server_name_for_config, &config.command);
                for arg in &config.args {
                    mcp_config = mcp_config.with_arg(arg);
                }
                for (key, value) in &config.env {
                    mcp_config = mcp_config.with_env(key, value);
                }
                if let Some(cwd) = &config.cwd {
                    mcp_config = mcp_config.with_cwd(cwd);
                }

                // Create client and ping
                match McpClient::new(mcp_config) {
                    Ok(client) => match client.ping().await {
                        Ok(result) => {
                            // Cache the successful result
                            {
                                let mut cache_guard = cache.lock();
                                cache_guard.set_mcp(
                                    server_name.clone(),
                                    VerificationEntry::verified_mcp(
                                        result.latency,
                                        result.tool_count,
                                    ),
                                );
                            }
                            let _ = tx
                                .send(StreamChunk::McpPinged {
                                    server: server_name,
                                    latency_ms: result.latency.as_millis() as u64,
                                    tool_count: result.tool_count,
                                })
                                .await;
                        }
                        Err(e) => {
                            // Cache the failure
                            {
                                let mut cache_guard = cache.lock();
                                cache_guard.set_mcp(
                                    server_name.clone(),
                                    VerificationEntry::failed(e.to_string()),
                                );
                            }
                            let _ = tx
                                .send(StreamChunk::McpError {
                                    server_name,
                                    error: e.to_string(),
                                })
                                .await;
                        }
                    },
                    Err(e) => {
                        // Cache the failure
                        {
                            let mut cache_guard = cache.lock();
                            cache_guard.set_mcp(
                                server_name.clone(),
                                VerificationEntry::failed(e.to_string()),
                            );
                        }
                        let _ = tx
                            .send(StreamChunk::McpError {
                                server_name,
                                error: e.to_string(),
                            })
                            .await;
                    }
                }
            });
        }
    }

    /// Save current chat session to disk
    ///
    /// Only saves in standalone mode (chat mode). Skips empty sessions.
    pub(crate) fn save_current_session(&mut self) {
        // Only save in standalone mode (chat mode)
        if self.standalone_state.is_none() {
            return;
        }

        // Get chat state from the chat view
        let chat_state = self.command_view.chat.get_chat_state();

        // Save session (guard: won't save empty sessions)
        match save_session(&chat_state) {
            Ok(session) => {
                self.session_id = Some(session.id.clone());
                tracing::info!("Chat session saved: {}", session.id);
            }
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => {
                // Empty session - not an error, just skip
                tracing::debug!("Skipping save: {}", e);
            }
            Err(e) => {
                tracing::warn!("Failed to save session: {}", e);
            }
        }
    }

    /// Request workflow retry (TIER 1.2)
    ///
    /// Resets failed tasks and signals that caller should re-run the workflow.
    /// Only works when workflow is in failed state.
    ///
    /// Note: Will be called from routing.rs when Action::RetryWorkflow is handled.
    pub(crate) fn retry_workflow(&mut self) {
        if self.state.is_running() {
            self.set_status("⚠ Cannot retry: workflow is still running");
            return;
        }

        if self.state.is_success() {
            self.set_status("⚠ Cannot retry: workflow completed successfully");
            return;
        }

        if !self.state.is_failed() {
            self.set_status("⚠ Nothing to retry");
            return;
        }

        // Reset state for retry
        let reset_tasks = self.state.reset_for_retry();
        self.retry_requested = true;
        self.workflow_done = false;

        if reset_tasks.is_empty() {
            self.set_status("✓ Ready to retry (no failed tasks found)");
        } else {
            self.set_status(&format!(
                "✓ Ready to retry: {} task(s) reset ({})",
                reset_tasks.len(),
                reset_tasks.join(", ")
            ));
        }
    }

    /// Cancel all background tasks
    ///
    /// Should be called during cleanup to ensure graceful shutdown.
    /// Tasks are aborted immediately; no waiting for completion.
    pub(crate) fn cancel_background_tasks(&self) {
        // PERF: parking_lot::Mutex doesn't poison, so this is simple
        let handles = self.background_handles.lock();
        let count = handles.len();
        for handle in handles.iter() {
            handle.abort();
        }
        // Reset global guards that background tasks may have held
        PROVIDER_VERIFICATION_RUNNING.store(false, Ordering::SeqCst);
        tracing::debug!("Aborted {} background tasks", count);
    }

    /// Cleanup terminal state
    ///
    /// Restores terminal to normal mode, disables raw mode, and shows cursor.
    pub(crate) fn cleanup(&mut self) -> Result<()> {
        // Note: Background tasks are cancelled in run_unified() after this,
        // because cleanup() is not async. Use cancel_background_tasks() separately.

        if let Some(ref mut terminal) = self.terminal {
            disable_raw_mode().map_err(|e| NikaError::TuiError {
                reason: format!("Failed to disable raw mode: {}", e),
            })?;

            execute!(
                terminal.backend_mut(),
                crossterm::event::DisableMouseCapture,
                LeaveAlternateScreen
            )
            .map_err(|e| NikaError::TuiError {
                reason: format!("Failed to leave alternate screen: {}", e),
            })?;

            terminal.show_cursor().map_err(|e| NikaError::TuiError {
                reason: format!("Failed to show cursor: {}", e),
            })?;
        }

        tracing::info!("TUI cleanup complete");
        Ok(())
    }

    /// Toggle theme (cycle through Dark → Light → Solarized)
    pub(crate) fn toggle_theme(&mut self) {
        use super::super::tokens::CosmicVariant;
        let next = match self.cosmic_theme.variant() {
            CosmicVariant::CosmicDark => CosmicVariant::CosmicLight,
            CosmicVariant::CosmicLight => CosmicVariant::CosmicViolet,
            CosmicVariant::CosmicViolet => CosmicVariant::CosmicDark,
        };
        self.cosmic_theme = super::super::cosmic_theme::CosmicTheme::new(next);
        self.theme = self.cosmic_theme.as_theme();
    }

    /// Set status message (shown in status bar)
    pub(crate) fn set_status(&mut self, message: &str) {
        self.status_message = Some((message.to_string(), std::time::Instant::now()));
    }
}
