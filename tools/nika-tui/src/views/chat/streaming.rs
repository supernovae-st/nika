// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Streaming Lifecycle for Chat View
//!
//! Contains streaming control, rain effects, agent phase handlers,
//! and thinking management.

use super::{
    AgentPhase, AgentPhaseIndicator, ChatView, DecryptVerb, MessageRole, StreamingDecrypt,
};

// ═══════════════════════════════════════════════════════════════════════════════
// Streaming Control Methods
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Start streaming mode
    pub fn start_streaming(&mut self) {
        self.is_streaming = true;
        self.partial_response.clear();
        // Reset and start matrix decrypt effect
        self.streaming_decrypt.clear();
    }

    /// Start streaming with a specific verb for theming
    pub fn start_streaming_with_verb(&mut self, verb: DecryptVerb) {
        self.is_streaming = true;
        self.partial_response.clear();
        // Reset and configure matrix streaming decrypt
        // Parameters tuned for visible chaos → reveal wave effect
        self.streaming_decrypt = StreamingDecrypt::new()
            .with_verb(verb) // Theme colors for revealed text
            .with_reveal_speed(0.4) // ~2.5 frames per char reveal
            .with_wave_factor(2.5) // wider wave for smoother reveal
            .with_initial_chaos(12); // frames of chaos before reveal starts
    }

    /// Append chunk to partial response during streaming
    pub fn append_streaming(&mut self, chunk: &str) {
        self.partial_response.push_str(chunk);
        // Push to matrix decrypt for reveal effect
        if self.matrix_effect_enabled {
            self.streaming_decrypt.push_text(chunk);
        }
        // Auto-scroll during streaming to follow new content
        self.auto_scroll_to_bottom();
    }

    /// Finish streaming and return the full response
    pub fn finish_streaming(&mut self) -> String {
        self.is_streaming = false;
        // WOW: Reveal all remaining text instantly
        self.streaming_decrypt.reveal_all();
        // Keep TaskBoxes visible after completion (Option A)
        // TaskBox::Infer now REPLACES the AI bubble - don't clear it
        // Old: self.inline_content.clear();
        // Reset agent phase
        self.on_agent_complete();
        // Final auto-scroll after streaming completes
        self.auto_scroll_to_bottom();
        std::mem::take(&mut self.partial_response)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Agent Phase Event Handlers
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Called when agent starts - sets Syncing phase
    pub fn on_agent_start(&mut self) {
        self.agent_phase = AgentPhase::Syncing;
        self.phase_indicator = AgentPhaseIndicator::new(AgentPhase::Syncing);
        self.agent_phase_tool = None;
    }

    /// Called when agent turn begins - sets Planning phase
    /// turn_index: Current turn number (0-indexed)
    pub fn on_agent_turn(&mut self, _turn_index: u32) {
        self.agent_phase = AgentPhase::Planning;
        self.phase_indicator.set_phase(AgentPhase::Planning);
        self.agent_phase_tool = None;
    }

    /// Called when MCP tool is invoked - sets Invoking phase
    pub fn on_mcp_invoke(&mut self, tool: &str, _server: &str) {
        self.agent_phase = AgentPhase::Invoking;
        self.agent_phase_tool = Some(tool.to_string());
        self.phase_indicator = AgentPhaseIndicator::new(AgentPhase::Invoking).with_tool(tool);
    }

    /// Called when MCP response received - sets Processing phase
    pub fn on_mcp_response(&mut self) {
        self.agent_phase = AgentPhase::Processing;
        self.phase_indicator.set_phase(AgentPhase::Processing);
        // Keep tool name for context
    }

    /// Called when provider/LLM called - sets Inferring phase
    pub fn on_provider_called(&mut self) {
        self.agent_phase = AgentPhase::Inferring;
        self.phase_indicator.set_phase(AgentPhase::Inferring);
        self.agent_phase_tool = None;
    }

    /// Called when first streaming token arrives - sets Streaming phase
    pub fn on_streaming_start(&mut self) {
        self.agent_phase = AgentPhase::Streaming;
        self.phase_indicator.set_phase(AgentPhase::Streaming);
    }

    /// Called when agent completes - resets to Idle
    pub fn on_agent_complete(&mut self) {
        self.agent_phase = AgentPhase::Idle;
        self.phase_indicator.set_phase(AgentPhase::Idle);
        self.agent_phase_tool = None;
    }

    /// Tick the phase indicator animation
    pub fn tick_phase_indicator(&mut self) {
        if self.agent_phase.is_active() {
            self.phase_indicator.tick();
        }
    }

    /// Get total tokens used in this session
    pub fn total_tokens(&self) -> u64 {
        self.session_context.tokens_used
    }

    /// Add tokens to session context (for status bar display)
    pub fn add_tokens(&mut self, input_tokens: u64, output_tokens: u64) {
        self.session_context.add_tokens(input_tokens, output_tokens);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Thinking Management
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Append thinking content during streaming
    pub fn append_thinking(&mut self, thinking: &str) {
        match &mut self.pending_thinking {
            Some(existing) => {
                existing.push('\n');
                existing.push_str(thinking);
            }
            None => {
                self.pending_thinking = Some(thinking.to_string());
            }
        }
    }

    /// Finalize thinking and attach to last message
    /// Call this when streaming completes
    pub fn finalize_thinking(&mut self) {
        if let Some(thinking) = self.pending_thinking.take() {
            // Attach thinking to the last Nika message
            if let Some(last) = self.messages.last_mut() {
                if last.role == MessageRole::Nika {
                    last.thinking = Some(thinking);
                }
            }
        }
    }
}
