// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Message Operations for Chat View
//!
//! Contains message CRUD, export, and thinking toggle methods.

use std::path::Path;
use std::time::Instant;

use chrono::Local;
use serde::Serialize;

use nika_engine::util::fs::atomic_write;

use super::{categorize_error, ChatMessage, ChatView, ExecutionResult, MessageRole, WorkflowRole};

/// Maximum number of messages to retain in the chat view
const MAX_MESSAGES: usize = 500;
/// Maximum number of command history entries to retain
const MAX_HISTORY: usize = 200;

// ═══════════════════════════════════════════════════════════════════════════════
// Message Operations
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Enforce message cap, dropping oldest messages when over limit
    fn enforce_message_cap(&mut self) {
        if self.messages.len() > MAX_MESSAGES {
            let drain_count = self.messages.len() - MAX_MESSAGES;
            self.messages.drain(..drain_count);
            self.conversation_scroll.offset =
                self.conversation_scroll.offset.saturating_sub(drain_count);
            self.conversation_scroll.cursor =
                self.conversation_scroll.cursor.saturating_sub(drain_count);
        }
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: String) {
        let id = self.next_message_id();
        self.messages.push(ChatMessage {
            id,
            role: MessageRole::User,
            content: content.clone(),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution: None,
            thinking: None,
        });
        self.enforce_message_cap();
        self.history.push(content.clone());
        if self.history.len() > MAX_HISTORY {
            self.history.drain(..self.history.len() - MAX_HISTORY);
        }
        self.history_index = None;
        // When user sends a message, they want to see the response
        self.user_at_bottom = true;
        self.auto_scroll_to_bottom();
        // Sync DAG when messages change
        self.maybe_sync_dag();

        // Wire to ChatWorkflow DAG (unified execution)
        // Use add_message_with_mentions to handle @N references automatically
        let _ = self
            .workflow
            .add_message_with_mentions(&content, WorkflowRole::User);
    }

    /// Add a Nika response
    pub fn add_nika_message(&mut self, content: String, execution: Option<ExecutionResult>) {
        let id = self.next_message_id();
        self.messages.push(ChatMessage {
            id,
            role: MessageRole::Nika,
            content: content.clone(),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution,
            thinking: None,
        });
        self.enforce_message_cap();
        self.auto_scroll_to_bottom(); // Auto-scroll on new message
                                      // Sync DAG when messages change
        self.maybe_sync_dag();

        // Wire to ChatWorkflow DAG (unified execution)
        let _ = self
            .workflow
            .add_message_with_mentions(&content, WorkflowRole::Assistant);
    }

    /// Add a Nika response with thinking content
    pub fn add_nika_message_with_thinking(
        &mut self,
        content: String,
        thinking: Option<String>,
        execution: Option<ExecutionResult>,
    ) {
        let id = self.next_message_id();
        self.messages.push(ChatMessage {
            id,
            role: MessageRole::Nika,
            content: content.clone(),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution,
            thinking,
        });
        self.enforce_message_cap();
        self.auto_scroll_to_bottom(); // Auto-scroll on new message
                                      // Sync DAG when messages change
        self.maybe_sync_dag();

        // Wire to ChatWorkflow DAG (unified execution)
        let _ = self
            .workflow
            .add_message_with_mentions(&content, WorkflowRole::Assistant);
    }

    /// Add a system message (for mode changes, status updates)
    pub fn add_system_message(&mut self, content: impl Into<String>) {
        let id = self.next_message_id();
        let content_str = content.into();
        self.messages.push(ChatMessage {
            id,
            role: MessageRole::System,
            content: content_str.clone(),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution: None,
            thinking: None,
        });
        self.enforce_message_cap();
        self.auto_scroll_to_bottom(); // Auto-scroll on new message
                                      // Sync DAG when messages change
        self.maybe_sync_dag();

        // Wire to ChatWorkflow DAG (unified execution)
        let _ = self
            .workflow
            .add_message_with_mentions(&content_str, WorkflowRole::System);
    }

    /// Get the content of the last Nika (assistant) message, if any
    pub fn last_nika_message_content(&self) -> Option<String> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Nika)
            .map(|m| m.content.clone())
    }

    /// Export chat session to JSON file
    ///
    /// Exports all messages to a JSON file for later analysis or sharing.
    /// If no path is provided, generates a timestamped filename.
    ///
    /// # Security
    /// Path is validated to prevent directory traversal attacks:
    /// - Rejects paths with `..` components
    /// - Rejects absolute paths (must be relative to current directory)
    /// - Ensures `.json` extension
    ///
    /// # Returns
    /// The path to the exported file on success
    pub fn export_session(&self, path: Option<&str>) -> Result<String, String> {
        // Generate default filename with timestamp
        let filepath = match path {
            Some(p) => {
                // SECURITY: Validate user-provided path
                let path = Path::new(p);

                // Reject absolute paths (must be relative to current directory)
                if path.is_absolute() {
                    return Err("Security: Absolute paths not allowed. Use a relative path.".into());
                }

                // Reject paths with parent directory traversal
                for component in path.components() {
                    if matches!(component, std::path::Component::ParentDir) {
                        return Err(
                            "Security: Path traversal (..) not allowed. Use a simple filename."
                                .into(),
                        );
                    }
                }

                // Ensure .json extension (user-friendly, prevents accidental overwrites)
                let p_str = p.to_string();
                if p_str.ends_with(".json") {
                    p_str
                } else {
                    format!("{}.json", p_str)
                }
            }
            None => format!("nika-chat-{}.json", Local::now().format("%Y%m%d-%H%M%S")),
        };

        // Build exportable session data
        #[derive(Serialize)]
        struct ExportedSession {
            exported_at: String,
            model: String,
            message_count: usize,
            messages: Vec<ExportedMessage>,
        }

        #[derive(Serialize)]
        struct ExportedMessage {
            role: String,
            content: String,
            timestamp: String,
            thinking: Option<String>,
        }

        let messages: Vec<ExportedMessage> = self
            .messages
            .iter()
            .map(|m| ExportedMessage {
                role: format!("{:?}", m.role),
                content: m.content.clone(),
                timestamp: m.timestamp.to_rfc3339(),
                thinking: m.thinking.clone(),
            })
            .collect();

        let session = ExportedSession {
            exported_at: Local::now().to_rfc3339(),
            model: self.provider.model.clone(),
            message_count: messages.len(),
            messages,
        };

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&session)
            .map_err(|e| format!("JSON serialization failed: {}", e))?;

        // Write to file using atomic_write for safety
        atomic_write(Path::new(&filepath), json.as_bytes())
            .map_err(|e| format!("File write failed: {}", e))?;

        Ok(filepath)
    }

    /// Export chat session as a runnable Nika YAML workflow
    ///
    /// Exports the chat conversation as a YAML workflow file that can be re-executed
    /// with `nika run <file>`. User messages become `infer:` tasks, and @N mentions
    /// create DAG edges between tasks.
    pub fn export_session_yaml(&self, path: Option<&str>) -> Result<String, String> {
        // Generate default filename with timestamp
        let filepath = match path {
            Some(p) => {
                // SECURITY: Validate user-provided path (same as JSON export)
                let path = Path::new(p);

                // Reject absolute paths (must be relative to current directory)
                if path.is_absolute() {
                    return Err("Security: Absolute paths not allowed. Use a relative path.".into());
                }

                // Reject paths with parent directory traversal
                for component in path.components() {
                    if matches!(component, std::path::Component::ParentDir) {
                        return Err(
                            "Security: Path traversal (..) not allowed. Use a simple filename."
                                .into(),
                        );
                    }
                }

                // Ensure .nika.yaml extension
                let p_str = p.to_string();
                if p_str.ends_with(".nika.yaml") {
                    p_str
                } else if p_str.ends_with(".yaml") || p_str.ends_with(".yml") {
                    // Replace .yaml/.yml with .nika.yaml
                    let base = p_str.trim_end_matches(".yaml").trim_end_matches(".yml");
                    format!("{}.nika.yaml", base)
                } else {
                    format!("{}.nika.yaml", p_str)
                }
            }
            None => format!(
                "nika-chat-{}.nika.yaml",
                Local::now().format("%Y%m%d-%H%M%S")
            ),
        };

        // Generate YAML from ChatWorkflow
        let yaml = self.workflow.to_yaml();

        // Write to file using atomic_write for safety
        atomic_write(Path::new(&filepath), yaml.as_bytes())
            .map_err(|e| format!("File write failed: {}", e))?;

        Ok(filepath)
    }

    /// Generate a new unique message ID
    pub(super) fn next_message_id(&mut self) -> u64 {
        self.message_id_counter += 1;
        self.message_id_counter
    }

    /// Toggle thinking section visibility for a specific message
    ///
    /// Toggles the collapsed state of thinking content for the message at index.
    /// Used with 't' key to show/hide thinking for the cursor message.
    /// Uses stable message IDs instead of indices for stability.
    pub fn toggle_thinking(&mut self, idx: usize) {
        // Only toggle if this message has thinking content
        if let Some(msg) = self.messages.get(idx) {
            if msg.thinking.is_some() {
                let msg_id = msg.id;
                if self.thinking_collapsed.contains(&msg_id) {
                    self.thinking_collapsed.remove(&msg_id);
                } else {
                    self.thinking_collapsed.insert(msg_id);
                }
                self.cached_msg_dirty = true;
            }
        }
    }

    /// Toggle thinking visibility for all messages
    ///
    /// Toggles the default expanded state and clears individual overrides.
    /// Used with 'T' key to show/hide all thinking sections at once.
    pub fn toggle_all_thinking(&mut self) {
        self.thinking_expanded_default = !self.thinking_expanded_default;
        self.thinking_collapsed.clear();
        self.cached_msg_dirty = true;
        // Add system message to confirm the toggle
        let state = if self.thinking_expanded_default {
            "expanded"
        } else {
            "collapsed"
        };
        self.add_system_message(format!("🧠 Thinking sections now {} by default", state));
    }

    /// Check if thinking is visible for a specific message
    ///
    /// Returns true if thinking section should be shown for message at index.
    /// The `thinking_collapsed` set tracks messages that differ from the default.
    /// If default=false (collapsed), set contains messages to SHOW.
    /// If default=true (expanded), set contains messages to HIDE.
    /// Uses stable message IDs instead of indices for stability.
    pub fn is_thinking_visible(&self, idx: usize) -> bool {
        // Get the message ID from the index
        if let Some(msg) = self.messages.get(idx) {
            // If in the override set, return the OPPOSITE of default
            if self.thinking_collapsed.contains(&msg.id) {
                return !self.thinking_expanded_default;
            }
        }
        // Otherwise use default
        self.thinking_expanded_default
    }

    /// Auto-scroll to bottom of conversation (NovaNet pattern)
    /// Called when new messages are added to keep latest content visible
    /// Smart auto-scroll - only scrolls if user was at bottom
    /// This prevents jumping when user is reading history and new content arrives
    pub(super) fn auto_scroll_to_bottom(&mut self) {
        // Only auto-scroll if user was at the bottom
        // If they scrolled up to read history, don't interrupt them
        if !self.user_at_bottom {
            return;
        }

        // Update total immediately (don't wait for render)
        // Use estimated lines per message until render computes exact count
        let estimated_lines_per_message = 4; // header + content + spacing
        let estimated_total = self.messages.len() * estimated_lines_per_message;

        // Update scroll state total (will be refined during render)
        self.conversation_scroll.total = estimated_total;

        // Scroll to bottom using the estimated total
        let visible = self.conversation_scroll.visible.max(1);
        self.conversation_scroll.offset = estimated_total.saturating_sub(visible);
        self.conversation_scroll.cursor = estimated_total.saturating_sub(1);
    }

    /// Append text to the last message (for streaming tokens)
    ///
    /// Used for Claude Code-like streaming where tokens appear in real-time.
    /// If the last message is "Thinking...", it will be replaced.
    /// Auto-scrolls to keep streaming content visible
    pub fn append_to_last_message(&mut self, token: &str) {
        if let Some(last) = self.messages.last_mut() {
            // If it's "Thinking...", replace it with the first token
            if last.content == "Thinking..." {
                last.content = token.to_string();
            } else {
                // Append token to existing content
                last.content.push_str(token);
            }
        }
        // Auto-scroll during streaming to follow new content
        self.auto_scroll_to_bottom();
    }

    /// Replace the last message content (for error display)
    pub fn replace_last_message(&mut self, content: String) {
        if let Some(last) = self.messages.last_mut() {
            last.content = content;
        }
    }

    /// Display an error with recovery suggestions
    /// Categorizes errors and provides actionable hints
    pub fn show_error(&mut self, error: &str) {
        let (category, suggestion) = categorize_error(error);
        let formatted = format!(
            "❌ {} Error: {}\n💡 {}\n\nUse /help for commands or /clear to restart.",
            category, error, suggestion
        );
        self.add_system_message(formatted);
    }
}
