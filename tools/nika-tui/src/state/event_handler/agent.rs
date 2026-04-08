// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent event handlers
//!
//! AgentStart, AgentTurn, AgentComplete, AgentSpawned,
//! ContextAssembled, TemplateResolved

use nika_engine::event::{AgentTurnKind, AgentTurnMetadata, ContextSource, ExcludedItem};

use super::TuiState;
use crate::state::notification::Notification;
use crate::state::types::{AgentTurnState, ContextAssembly, SpawnedAgent, TemplateResolution};

impl TuiState {
    pub(super) fn on_agent_start(&mut self, max_turns: u32) {
        // Use reset() to clear all fields including spawned_agents (was leaked before)
        self.agent.reset();
        self.agent.max_turns = Some(max_turns);
        // TIER 4.1: Mark reasoning panel dirty
        self.dirty.reasoning = true;
    }

    pub(super) fn on_agent_turn(
        &mut self,
        turn_index: u32,
        kind: &AgentTurnKind,
        metadata: &Option<AgentTurnMetadata>,
    ) {
        // Extract tokens from metadata if present
        let tokens = metadata.as_ref().map(|m| m.total_tokens());
        // Extract thinking and response_text from metadata
        let thinking = metadata.as_ref().and_then(|m| m.thinking.clone());
        let response_text = metadata.as_ref().map(|m| m.response_text.clone());

        let turn = AgentTurnState {
            index: turn_index,
            status: kind.to_string(),
            tokens,
            tool_calls: Vec::new(),
            thinking,
            response_text,
        };
        // Update or add turn
        if let Some(existing) = self.agent.turns.iter_mut().find(|t| t.index == turn_index) {
            existing.status = kind.to_string();
            existing.tokens = tokens;
            existing.thinking = turn.thinking;
            existing.response_text = turn.response_text;
        } else {
            self.agent.turns.push(turn);
        }
        // TIER 4.1: Mark reasoning panel dirty
        self.dirty.reasoning = true;
    }

    pub(super) fn on_agent_complete(&mut self) {
        // token_history is already populated per-turn by on_provider_responded
        // TIER 4.1: Mark reasoning panel dirty
        self.dirty.reasoning = true;
    }

    pub(super) fn on_agent_spawned(
        &mut self,
        parent_task_id: &str,
        child_task_id: &str,
        depth: u32,
        timestamp_ms: u64,
    ) {
        // Track spawned sub-agent
        self.agent.spawned_agents.push(SpawnedAgent {
            parent_task_id: parent_task_id.to_string(),
            child_task_id: child_task_id.to_string(),
            depth,
        });

        // Add notification for nested agent spawn
        self.add_notification(Notification::info(
            format!(
                "Hatching '{}' at depth {} - fly little one!",
                child_task_id, depth
            ),
            timestamp_ms,
        ));

        // TIER 4.1: Mark reasoning and notifications dirty
        self.dirty.reasoning = true;
        self.dirty.notifications = true;
    }

    pub(super) fn on_context_assembled(
        &mut self,
        sources: &[ContextSource],
        excluded: &[ExcludedItem],
        total_tokens: u64,
        budget_used_pct: f32,
        truncated: bool,
    ) {
        self.mcp.context_assembly = ContextAssembly {
            sources: sources.to_vec(),
            excluded: excluded.to_vec(),
            total_tokens,
            budget_used_pct,
            truncated,
        };
        // TIER 4.1: Mark novanet panel dirty
        self.dirty.novanet = true;
    }

    pub(super) fn on_template_resolved(
        &mut self,
        task_id: &str,
        template: &str,
        result: &str,
        timestamp_ms: u64,
    ) {
        // Keep last 10 resolutions
        if self.agent.recent_templates.len() >= 10 {
            self.agent.recent_templates.pop_front();
        }
        self.agent.recent_templates.push_back(TemplateResolution {
            task_id: task_id.to_string(),
            template: template.to_string(),
            result: result.to_string(),
            timestamp_ms,
        });
        // Mark context panel dirty (template bindings are context-related)
        self.dirty.novanet = true;
    }
}
