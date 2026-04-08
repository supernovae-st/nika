// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! App Event Handling
//!
//! Contains event polling, keyboard handling, and stream processing.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::broadcast;

use nika_engine::event::EventKind;
use nika_engine::provider::rig::StreamChunk;

use super::super::views::{TuiView, View, ViewAction};
use super::types::Action;
use super::App;
use crate::InputMode;

impl App {
    /// Poll for runtime events from the workflow execution
    ///
    /// Checks broadcast and mpsc receivers for events,
    /// processes them, and updates state accordingly.
    pub(crate) fn poll_runtime_events(&mut self) {
        // Clear buffer for reuse
        self.event_buffer.clear();

        // Check broadcast receiver
        if let Some(ref mut rx) = self.broadcast_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => self.event_buffer.push(event),
                    Err(broadcast::error::TryRecvError::Empty) => break,
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        tracing::warn!("TUI lagged behind by {} events — forcing full refresh", n);
                        self.state.dirty.mark_all();
                    }
                    Err(broadcast::error::TryRecvError::Closed) => {
                        self.workflow_done = true;
                        break;
                    }
                }
            }
        }
        // Fallback to mpsc receiver
        if let Some(ref mut rx) = self.event_rx {
            while let Ok(event) = rx.try_recv() {
                self.event_buffer.push(event);
            }
        }

        // Process events
        // PERF: swap-and-drain to avoid Vec allocation every tick
        let mut events = std::mem::take(&mut self.event_buffer);
        for event in events.drain(..) {
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
        // Give back the (now empty but heap-allocated) buffer for reuse
        self.event_buffer = events;

        // Poll LLM responses
        self.poll_llm_responses();

        // Poll streaming chunks
        self.poll_stream_chunks();
    }

    /// Poll for complete LLM responses
    fn poll_llm_responses(&mut self) {
        while let Ok(response) = self.llm_response_rx.try_recv() {
            // Update chat view (ChatView is the single source of truth)
            if let Some(last) = self.command_view.chat.messages.last() {
                if last.content == "Thinking..." || last.content.starts_with("$ ") {
                    self.command_view.chat.messages.pop();
                }
            }
            self.command_view.chat.add_nika_message(response, None);
        }
    }

    /// Poll for streaming chunks (real-time token display)
    fn poll_stream_chunks(&mut self) {
        while let Ok(chunk) = self.stream_chunk_rx.try_recv() {
            match chunk {
                StreamChunk::Token(token) => {
                    if !self.command_view.chat.is_streaming {
                        use crate::widgets::DecryptVerb;
                        self.command_view
                            .chat
                            .start_streaming_with_verb(DecryptVerb::Infer);
                    }
                    self.command_view.chat.append_streaming(&token);
                }
                StreamChunk::Done(_) => {
                    if self.command_view.chat.is_streaming {
                        let response = self.command_view.chat.finish_streaming();
                        if !response.is_empty() {
                            self.command_view.chat.add_nika_message(response, None);
                        }
                    }
                    self.command_view.chat.finalize_thinking();
                }
                StreamChunk::Error(err) => {
                    if self.command_view.chat.is_streaming {
                        self.command_view.chat.finish_streaming();
                    }
                    self.command_view.chat.show_error(&err);
                }
                StreamChunk::Thinking(text) => {
                    self.command_view.chat.append_thinking(&text);
                }
                _ => {}
            }
        }
    }

    /// Handle unified keyboard events across all views
    ///
    /// Processes key events and returns the appropriate Action.
    pub(crate) fn handle_unified_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Action {
        // Dismiss first-launch welcome hint on any keypress
        self.dismiss_welcome_hint();

        // 1. Ctrl+C is always global (never delegate — prevents copy from stealing quit)
        if let (KeyCode::Char('c'), KeyModifiers::CONTROL) = (code, modifiers) {
            if !(self.current_view == TuiView::Studio && self.input_mode == InputMode::Insert) {
                return self.handle_ctrl_c();
            }
        }

        // 2. Dispatch to view FIRST — views must be able to close their own overlays
        //    (provider modal, command palette, help) before global shortcuts fire.
        let view_action = self.dispatch_to_current_view(code, modifiers);
        if view_action != ViewAction::None {
            return Action::from_view_action(view_action);
        }

        // 3. Global Escape: exit Insert/Search mode (only if view didn't handle it)
        if code == KeyCode::Esc && self.input_mode != InputMode::Normal {
            self.input_mode = InputMode::Normal;
            return Action::Continue;
        }

        // 4. Quit (Normal mode only, only if view didn't consume the key)
        if code == KeyCode::Char('q')
            && modifiers.is_empty()
            && self.input_mode == InputMode::Normal
        {
            return Action::Quit;
        }

        // 5. View switching - 3-view architecture
        // Allow Alt+1-3 in ANY mode for view switching.
        let alt_pressed = modifiers.contains(KeyModifiers::ALT);
        let is_normal = self.input_mode == InputMode::Normal;

        // In Control view, 1/2/3 are theme shortcuts — don't steal them for view switching.
        // In Command view, 1-4 are Monitor panel focus keys — let the view handle them.
        // Alt+1-3 always works for view switching regardless of current view.
        let on_control = self.current_view == TuiView::Control;
        let on_command = self.current_view == TuiView::Command;
        if alt_pressed || (is_normal && modifiers.is_empty() && !on_control && !on_command) {
            if let Some(view) = match code {
                KeyCode::Char('1') => Some(TuiView::Studio),
                KeyCode::Char('2') => Some(TuiView::Command),
                KeyCode::Char('3') => Some(TuiView::Control),
                _ => None,
            } {
                return Action::SwitchView(view);
            }
        }

        // Letter shortcuts for view switching (Normal mode only, not in Command where they're text input)
        if is_normal && modifiers.is_empty() && !on_command {
            if let Some(view) = match code {
                KeyCode::Char('s') => Some(TuiView::Studio),
                KeyCode::Char('c') => Some(TuiView::Command),
                KeyCode::Char('x') => Some(TuiView::Control),
                _ => None,
            } {
                return Action::SwitchView(view);
            }
        }

        Action::Continue
    }

    /// Dispatch key event to the current view's handle_key method
    ///
    /// Bridges the gap between the app event loop
    /// and view-specific keyboard handling.
    fn dispatch_to_current_view(&mut self, code: KeyCode, modifiers: KeyModifiers) -> ViewAction {
        // Convert KeyCode + modifiers to KeyEvent
        let key = KeyEvent::new(code, modifiers);

        match self.current_view {
            TuiView::Studio => self.studio_view.handle_key(key, &mut self.state),
            TuiView::Command => self.command_view.handle_key(key, &mut self.state),
            TuiView::Control => self.control_view.handle_key(key, &mut self.state),
        }
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
