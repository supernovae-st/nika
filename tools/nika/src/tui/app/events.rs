//! App Event Handling
//!
//! Contains event polling, keyboard handling, mouse handling, and stream processing.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyModifiers, MouseEvent};
use ratatui::layout::Rect;
use tokio::sync::broadcast;

use crate::event::EventKind;
use crate::provider::rig::StreamChunk;

use super::super::views::TuiView;
use super::types::Action;
use super::App;
use crate::tui::InputMode;

impl App {
    /// Poll for runtime events from the workflow execution
    ///
    /// Checks broadcast and legacy mpsc receivers for events,
    /// processes them, and updates state accordingly.
    pub(crate) fn poll_runtime_events(&mut self) {
        // Clear buffer for reuse
        self.event_buffer.clear();

        // Check broadcast receiver (v0.4.1 preferred)
        if let Some(ref mut rx) = self.broadcast_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => self.event_buffer.push(event),
                    Err(broadcast::error::TryRecvError::Empty) => break,
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        tracing::warn!("TUI lagged behind by {} events", n);
                    }
                    Err(broadcast::error::TryRecvError::Closed) => {
                        self.workflow_done = true;
                        break;
                    }
                }
            }
        }
        // Fallback to legacy mpsc receiver
        if let Some(ref mut rx) = self.event_rx {
            while let Ok(event) = rx.try_recv() {
                self.event_buffer.push(event);
            }
        }

        // Process events
        let events: Vec<_> = self.event_buffer.drain(..).collect();
        for event in events {
            match &event.kind {
                EventKind::WorkflowCompleted { .. } => {
                    self.workflow_done = true;
                }
                EventKind::WorkflowFailed { .. } => {
                    self.workflow_done = true;
                }
                _ => {}
            }
            self.state.handle_event(&event.kind, event.timestamp_ms);
        }

        // Poll LLM responses
        self.poll_llm_responses();

        // Poll streaming chunks
        self.poll_stream_chunks();
    }

    /// Poll for complete LLM responses
    fn poll_llm_responses(&mut self) {
        while let Ok(response) = self.llm_response_rx.try_recv() {
            // Update chat overlay
            if let Some(last) = self.state.chat_overlay.messages.last() {
                if last.content == "Thinking..." {
                    self.state.chat_overlay.messages.pop();
                }
            }
            self.state.chat_overlay.add_nika_message(response.clone());

            // Update chat view
            if let Some(last) = self.chat_view.messages.last() {
                if last.content == "Thinking..." || last.content.starts_with("$ ") {
                    self.chat_view.messages.pop();
                }
            }
            self.chat_view.add_nika_message(response, None);
        }
    }

    /// Poll for streaming chunks (real-time token display)
    fn poll_stream_chunks(&mut self) {
        while let Ok(chunk) = self.stream_chunk_rx.try_recv() {
            match chunk {
                StreamChunk::Token(token) => {
                    if !self.chat_view.is_streaming {
                        use crate::tui::widgets::DecryptVerb;
                        self.chat_view.start_streaming_with_verb(DecryptVerb::Infer);
                    }
                    self.chat_view.append_streaming(&token);
                }
                StreamChunk::Done(_) => {
                    self.chat_view.finalize_thinking();
                }
                StreamChunk::Error(err) => {
                    if self.chat_view.is_streaming {
                        self.chat_view.finish_streaming();
                    }
                    self.chat_view.show_error(&err);
                }
                _ => {}
            }
        }
    }

    /// Handle unified keyboard events across all views
    ///
    /// Processes key events and returns the appropriate Action.
    pub(crate) fn handle_unified_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Action {
        match (code, modifiers) {
            // Global shortcuts
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => self.handle_ctrl_c(),
            (KeyCode::Char('q'), KeyModifiers::NONE) => Action::Quit,

            // Mode switching
            (KeyCode::Esc, _) => {
                if self.input_mode != InputMode::Normal {
                    self.input_mode = InputMode::Normal;
                    Action::Continue
                } else {
                    Action::Continue
                }
            }

            // View switching (Normal mode only) - 4-view architecture v0.22
            (KeyCode::Char('1'), KeyModifiers::NONE) if self.input_mode == InputMode::Normal => {
                Action::SwitchView(TuiView::Studio)
            }
            (KeyCode::Char('2'), KeyModifiers::NONE) if self.input_mode == InputMode::Normal => {
                Action::SwitchView(TuiView::Runner)
            }
            (KeyCode::Char('3'), KeyModifiers::NONE) if self.input_mode == InputMode::Normal => {
                Action::SwitchView(TuiView::Chat)
            }
            (KeyCode::Char('4'), KeyModifiers::NONE) if self.input_mode == InputMode::Normal => {
                Action::SwitchView(TuiView::Settings)
            }

            // Default - no action
            _ => Action::Continue,
        }
    }

    /// Handle mouse events
    ///
    /// Processes mouse clicks and scrolling.
    #[allow(dead_code)]
    pub(crate) fn handle_mouse(
        &mut self,
        _mouse: MouseEvent,
        _terminal_size: Option<Rect>,
    ) -> Action {
        // TODO: Implement mouse handling for panel clicks, scrolling
        Action::Continue
    }

    /// Handle Ctrl+C with double-tap to quit (like Claude Code)
    fn handle_ctrl_c(&mut self) -> Action {
        let now = std::time::Instant::now();
        if let Some(last) = self.last_ctrl_c {
            if now.duration_since(last) < Duration::from_millis(500) {
                return Action::Quit;
            }
        }
        self.last_ctrl_c = Some(now);
        self.set_status("Press Ctrl+C again to quit");
        Action::Continue
    }
}
