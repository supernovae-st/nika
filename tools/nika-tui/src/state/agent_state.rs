// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent tracking state slice
//!
//! Extracted from TuiState to isolate agent execution tracking
//! (turns, streaming, spawned sub-agents, template resolutions).

use std::collections::VecDeque;

use super::types::{AgentTurnState, SpawnedAgent, TemplateResolution};

/// Agent-related state extracted from TuiState
///
/// Tracks agent turns, streaming buffers, spawned sub-agents,
/// and recent template resolutions.
#[derive(Debug, Default)]
pub struct AgentState {
    /// Agent turns for current agent task
    pub turns: Vec<AgentTurnState>,
    /// Streaming buffer for live output
    pub streaming_buffer: String,
    /// Max turns for current agent
    pub max_turns: Option<u32>,
    /// Spawned sub-agents
    pub spawned_agents: Vec<SpawnedAgent>,
    /// Recent template resolutions (for observability)
    pub recent_templates: VecDeque<TemplateResolution>,
}

impl AgentState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new agent turn
    pub fn add_turn(&mut self, turn: AgentTurnState) {
        self.turns.push(turn);
    }

    /// Append text to streaming buffer
    pub fn append_stream(&mut self, text: &str) {
        self.streaming_buffer.push_str(text);
    }

    /// Clear streaming buffer and return its contents
    pub fn take_stream(&mut self) -> String {
        std::mem::take(&mut self.streaming_buffer)
    }

    /// Record a spawned sub-agent
    pub fn add_spawned(&mut self, agent: SpawnedAgent) {
        self.spawned_agents.push(agent);
    }

    /// Add a template resolution record (capped at 50)
    pub fn add_template_resolution(&mut self, resolution: TemplateResolution) {
        if self.recent_templates.len() >= 50 {
            self.recent_templates.pop_front();
        }
        self.recent_templates.push_back(resolution);
    }

    /// Reset agent state for a new agent task
    pub fn reset(&mut self) {
        self.turns.clear();
        self.streaming_buffer.clear();
        self.max_turns = None;
        self.spawned_agents.clear();
        // Keep template resolutions across runs for observability
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_state_default() {
        let agent = AgentState::new();
        assert!(agent.turns.is_empty());
        assert!(agent.streaming_buffer.is_empty());
        assert!(agent.max_turns.is_none());
        assert!(agent.spawned_agents.is_empty());
        assert!(agent.recent_templates.is_empty());
    }

    #[test]
    fn test_agent_state_add_turn() {
        let mut agent = AgentState::new();
        agent.add_turn(AgentTurnState {
            index: 0,
            status: "completed".to_string(),
            tokens: Some(100),
            tool_calls: vec!["novanet_search".to_string()],
            thinking: None,
            response_text: Some("Result".to_string()),
        });
        assert_eq!(agent.turns.len(), 1);
        assert_eq!(agent.turns[0].index, 0);
    }

    #[test]
    fn test_agent_state_streaming() {
        let mut agent = AgentState::new();
        agent.append_stream("Hello ");
        agent.append_stream("World");
        assert_eq!(agent.streaming_buffer, "Hello World");

        let taken = agent.take_stream();
        assert_eq!(taken, "Hello World");
        assert!(agent.streaming_buffer.is_empty());
    }

    #[test]
    fn test_agent_state_spawned_agents() {
        let mut agent = AgentState::new();
        agent.add_spawned(SpawnedAgent {
            parent_task_id: "parent".to_string(),
            child_task_id: "child".to_string(),
            depth: 1,
        });
        assert_eq!(agent.spawned_agents.len(), 1);
        assert_eq!(agent.spawned_agents[0].depth, 1);
    }

    #[test]
    fn test_agent_state_template_cap() {
        let mut agent = AgentState::new();
        for i in 0..55 {
            agent.add_template_resolution(TemplateResolution {
                task_id: format!("task_{}", i),
                template: "{{with.x}}".to_string(),
                result: "value".to_string(),
                timestamp_ms: i as u64,
            });
        }
        // Capped at 50
        assert_eq!(agent.recent_templates.len(), 50);
        // Oldest entries were evicted
        assert_eq!(agent.recent_templates[0].task_id, "task_5");
    }

    #[test]
    fn test_agent_state_reset() {
        let mut agent = AgentState::new();
        agent.add_turn(AgentTurnState {
            index: 0,
            status: "done".to_string(),
            tokens: None,
            tool_calls: vec![],
            thinking: None,
            response_text: None,
        });
        agent.append_stream("partial");
        agent.max_turns = Some(5);
        agent.add_spawned(SpawnedAgent {
            parent_task_id: "p".to_string(),
            child_task_id: "c".to_string(),
            depth: 1,
        });
        agent.add_template_resolution(TemplateResolution {
            task_id: "t".to_string(),
            template: "{{x}}".to_string(),
            result: "v".to_string(),
            timestamp_ms: 0,
        });

        agent.reset();
        assert!(agent.turns.is_empty());
        assert!(agent.streaming_buffer.is_empty());
        assert!(agent.max_turns.is_none());
        assert!(agent.spawned_agents.is_empty());
        // Templates preserved across reset
        assert_eq!(agent.recent_templates.len(), 1);
    }
}
