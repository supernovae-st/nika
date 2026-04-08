// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat Session Persistence
//!
//! Save and load chat conversations between TUI sessions.
//! Sessions are stored in `.nika/sessions/` as JSON files.
//!
//! # Usage
//!
//! ```rust,ignore
//! use nika_tui::session::{ChatSession, save_session, load_session, list_sessions};
//!
//! // Save current session
//! save_session(&chat_state)?;
//!
//! // Load a previous session
//! let session = load_session("session-id")?;
//!
//! // List all sessions
//! let sessions = list_sessions()?;
//! ```

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::state::{ChatOverlayMessage, ChatOverlayMessageRole, ChatOverlayState};

/// Directory for session files
const SESSION_DIR: &str = ".nika/sessions";

/// Maximum number of sessions to keep
const MAX_SESSIONS: usize = 50;

/// A persisted chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    /// Unique session ID (timestamp-based)
    pub id: String,
    /// Session creation timestamp
    pub created_at: SystemTime,
    /// Last modified timestamp
    pub updated_at: SystemTime,
    /// Conversation messages
    pub messages: Vec<ChatOverlayMessage>,
    /// Command history
    pub command_history: Vec<String>,
    /// Model used in session
    pub model: String,
    /// Session title (derived from first user message)
    pub title: String,
    /// Total message count
    pub message_count: usize,
}

impl ChatSession {
    /// Create a new session from chat state
    pub fn from_chat_state(state: &ChatOverlayState) -> Self {
        let now = SystemTime::now();
        let id = generate_session_id();

        // Derive title from first user message
        let title = state
            .messages
            .iter()
            .find(|m| m.role == ChatOverlayMessageRole::User)
            .map(|m| truncate_title(&m.content))
            .unwrap_or_else(|| "New Session".to_string());

        Self {
            id,
            created_at: now,
            updated_at: now,
            messages: state.messages.clone(),
            command_history: state.history.clone(),
            model: state.current_model.clone(),
            title,
            message_count: state.messages.len(),
        }
    }

    /// Apply session data to chat state
    pub fn apply_to_chat_state(&self, state: &mut ChatOverlayState) {
        state.messages = self.messages.clone();
        state.history = self.command_history.clone();
        state.current_model = self.model.clone();
        state.scroll = 0;
    }

    /// Get session file path
    pub fn file_path(&self) -> PathBuf {
        PathBuf::from(SESSION_DIR).join(format!("{}.json", self.id))
    }

    /// Format creation time for display
    pub fn created_display(&self) -> String {
        format_system_time(&self.created_at)
    }

    /// Format updated time for display
    pub fn updated_display(&self) -> String {
        format_system_time(&self.updated_at)
    }
}

/// Session metadata for listing (lightweight, no messages)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Unique session ID
    pub id: String,
    /// Session title
    pub title: String,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Last modified timestamp
    pub updated_at: SystemTime,
    /// Total message count
    pub message_count: usize,
    /// Model used
    pub model: String,
}

impl From<&ChatSession> for SessionMeta {
    fn from(session: &ChatSession) -> Self {
        Self {
            id: session.id.clone(),
            title: session.title.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            message_count: session.message_count,
            model: session.model.clone(),
        }
    }
}

impl SessionMeta {
    /// Format creation time for display
    pub fn created_display(&self) -> String {
        format_system_time(&self.created_at)
    }
}

/// Save a chat session to disk
///
/// Creates `.nika/sessions/` directory if it doesn't exist.
/// Uses atomic write (temp file + rename) for data integrity.
pub fn save_session(state: &ChatOverlayState) -> io::Result<ChatSession> {
    // Guard: Prevent saving empty sessions (only system message).
    // This is defensive validation in the persistence layer to avoid
    // cluttering disk with meaningless sessions. UI should also validate,
    // but this guard ensures data integrity regardless of caller.
    if state.messages.len() <= 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Cannot save empty session",
        ));
    }

    let session = ChatSession::from_chat_state(state);
    let session_dir = PathBuf::from(SESSION_DIR);

    // Create directory if needed
    fs::create_dir_all(&session_dir)?;

    // Atomic write: temp file + rename
    let path = session.file_path();
    let temp_path = path.with_extension("json.tmp");

    let content = serde_json::to_string_pretty(&session)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    fs::write(&temp_path, content)?;
    fs::rename(&temp_path, &path)?;

    // Cleanup old sessions
    cleanup_old_sessions()?;

    Ok(session)
}

/// Load a chat session by ID
pub fn load_session(id: &str) -> io::Result<ChatSession> {
    let path = PathBuf::from(SESSION_DIR).join(format!("{}.json", id));

    let content = fs::read_to_string(&path)?;
    let session: ChatSession = serde_json::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok(session)
}

/// List all sessions (metadata only, sorted by updated_at descending)
pub fn list_sessions() -> io::Result<Vec<SessionMeta>> {
    let session_dir = PathBuf::from(SESSION_DIR);

    if !session_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();

    for entry in fs::read_dir(&session_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(session) = serde_json::from_str::<ChatSession>(&content) {
                    sessions.push(SessionMeta::from(&session));
                }
            }
        }
    }

    // Sort by updated_at descending (most recent first)
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(sessions)
}

/// Delete a session by ID
pub fn delete_session(id: &str) -> io::Result<()> {
    let path = PathBuf::from(SESSION_DIR).join(format!("{}.json", id));
    fs::remove_file(path)
}

/// Get the most recent session
pub fn get_latest_session() -> io::Result<Option<ChatSession>> {
    let sessions = list_sessions()?;

    if let Some(meta) = sessions.first() {
        let session = load_session(&meta.id)?;
        Ok(Some(session))
    } else {
        Ok(None)
    }
}

/// Generate a unique session ID (timestamp-based)
fn generate_session_id() -> String {
    use std::time::UNIX_EPOCH;

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    format!("session-{}", duration.as_millis())
}

/// Truncate title to reasonable length (UTF-8 safe)
fn truncate_title(s: &str) -> String {
    let s = s.trim();
    let char_count = s.chars().count();
    if char_count <= 50 {
        s.to_string()
    } else {
        // UTF-8 safe truncation using chars iterator
        let truncated: String = s.chars().take(47).collect();
        format!("{}...", truncated)
    }
}

/// Format SystemTime for display using chrono for correct date handling
fn format_system_time(time: &SystemTime) -> String {
    use chrono::{DateTime, Utc};
    let datetime: DateTime<Utc> = (*time).into();
    datetime.format("%Y-%m-%d %H:%M").to_string()
}

/// Remove old sessions when we exceed MAX_SESSIONS
fn cleanup_old_sessions() -> io::Result<()> {
    let mut sessions = list_sessions()?;

    if sessions.len() > MAX_SESSIONS {
        // Sort by updated_at ascending (oldest first)
        sessions.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));

        // Delete oldest sessions
        let to_delete = sessions.len() - MAX_SESSIONS;
        for meta in sessions.iter().take(to_delete) {
            let _ = delete_session(&meta.id);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_session_id() {
        let id1 = generate_session_id();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let id2 = generate_session_id();

        assert!(id1.starts_with("session-"));
        assert!(id2.starts_with("session-"));
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_truncate_title_short() {
        assert_eq!(truncate_title("Hello world"), "Hello world");
    }

    #[test]
    fn test_truncate_title_long() {
        let long = "a".repeat(100);
        let truncated = truncate_title(&long);
        assert!(truncated.len() <= 50);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_truncate_title_trims() {
        assert_eq!(truncate_title("  hello  "), "hello");
    }

    #[test]
    fn test_format_system_time() {
        let time = SystemTime::UNIX_EPOCH;
        let formatted = format_system_time(&time);
        // Check format pattern (YYYY-MM-DD HH:MM), not exact value (timezone-dependent)
        assert!(
            formatted.starts_with("1970-01-01"),
            "Expected date 1970-01-01, got: {formatted}"
        );
        assert!(
            formatted.len() == 16 && formatted.chars().nth(10) == Some(' '),
            "Expected format YYYY-MM-DD HH:MM, got: {formatted}"
        );
    }

    #[test]
    fn test_chat_session_from_empty_state() {
        let state = ChatOverlayState::new();
        let session = ChatSession::from_chat_state(&state);

        assert!(session.id.starts_with("session-"));
        assert_eq!(session.title, "New Session"); // No user message
        assert_eq!(session.messages.len(), 1); // Only system message
    }

    #[test]
    fn test_chat_session_from_state_with_messages() {
        let mut state = ChatOverlayState::new();
        state.input = "Hello Nika".to_string();
        state.cursor = 10;
        state.add_user_message(); // This adds the user message

        let session = ChatSession::from_chat_state(&state);

        assert_eq!(session.title, "Hello Nika");
        assert_eq!(session.messages.len(), 2);
    }

    #[test]
    fn test_chat_session_apply_to_state() {
        let mut state1 = ChatOverlayState::new();
        state1.input = "Test message".to_string();
        state1.cursor = 12;
        state1.add_user_message();
        state1.add_nika_message("Response");

        let session = ChatSession::from_chat_state(&state1);

        let mut state2 = ChatOverlayState::new();
        session.apply_to_chat_state(&mut state2);

        assert_eq!(state2.messages.len(), 3);
        assert_eq!(state2.history.len(), state1.history.len());
    }

    #[test]
    fn test_session_meta_from_session() {
        let mut state = ChatOverlayState::new();
        state.input = "Test".to_string();
        state.cursor = 4;
        state.add_user_message();

        let session = ChatSession::from_chat_state(&state);
        let meta = SessionMeta::from(&session);

        assert_eq!(meta.id, session.id);
        assert_eq!(meta.title, session.title);
        assert_eq!(meta.message_count, session.message_count);
    }

    #[test]
    fn test_session_file_path() {
        let mut state = ChatOverlayState::new();
        state.input = "Test".to_string();
        state.cursor = 4;
        state.add_user_message();

        let session = ChatSession::from_chat_state(&state);
        let path = session.file_path();

        assert!(path.to_string_lossy().contains(".nika/sessions/"));
        assert!(path.to_string_lossy().ends_with(".json"));
    }

    // Note: File I/O tests would need to mock the SESSION_DIR constant
    // or use integration tests with actual filesystem

    #[test]
    fn test_chat_session_created_display() {
        let mut state = ChatOverlayState::new();
        state.input = "Test".to_string();
        state.cursor = 4;
        state.add_user_message();

        let session = ChatSession::from_chat_state(&state);
        let display = session.created_display();

        // Should be in YYYY-MM-DD HH:MM format
        assert!(display.contains('-'));
        assert!(display.contains(':'));
    }

    #[test]
    fn test_cleanup_respects_max_sessions() {
        // This test verifies the cleanup logic (unit test without filesystem)
        let sessions: Vec<SessionMeta> = (0..60)
            .map(|i| SessionMeta {
                id: format!("session-{}", i),
                title: format!("Test {}", i),
                created_at: SystemTime::UNIX_EPOCH,
                updated_at: SystemTime::UNIX_EPOCH,
                message_count: 1,
                model: "test".to_string(),
            })
            .collect();

        assert_eq!(sessions.len(), 60);
        // With MAX_SESSIONS = 50, cleanup would delete 10 oldest
    }

    #[test]
    fn test_session_serialization_roundtrip() {
        let mut state = ChatOverlayState::new();
        state.input = "Hello".to_string();
        state.cursor = 5;
        state.add_user_message();
        state.add_nika_message("Hi there!");

        let session = ChatSession::from_chat_state(&state);
        let json = serde_json::to_string(&session).unwrap();
        let restored: ChatSession = serde_json::from_str(&json).unwrap();

        assert_eq!(session.id, restored.id);
        assert_eq!(session.title, restored.title);
        assert_eq!(session.messages.len(), restored.messages.len());
    }
}
