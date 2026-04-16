// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent definition — the `agents:` section at workflow root.
//!
//! Named agent definitions that can be referenced by agent tasks
//! via `from: agent_name`. Promotes reuse of agent configurations
//! across multiple tasks in a workflow.

use serde::{Deserialize, Serialize};

use super::limits::LimitsConfig;

/// A named agent definition that can be referenced by agent tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AgentDef {
    /// Agent name (used as `from: <name>` in tasks).
    pub name: String,
    /// System prompt for the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Provider to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Model to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Tools available to this agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Skills available to this agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// MCP servers available to this agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
    /// Temperature for LLM calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Maximum agent turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    /// Depth limit for nested agent calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth_limit: Option<u32>,
    /// Resource limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<LimitsConfig>,
}

impl AgentDef {
    /// Create a new agent definition with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            system: None,
            provider: None,
            model: None,
            tools: Vec::new(),
            skills: Vec::new(),
            mcp: Vec::new(),
            temperature: None,
            max_turns: None,
            depth_limit: None,
            limits: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_name() {
        let agent = AgentDef::new("researcher");
        assert_eq!(agent.name, "researcher");
        assert!(agent.tools.is_empty());
        assert!(agent.model.is_none());
    }

    #[test]
    fn serde_roundtrip() {
        let agent = AgentDef {
            name: "coder".into(),
            system: Some("You are an expert programmer.".into()),
            model: Some("claude-sonnet-4-5-20250514".into()),
            tools: vec!["read_file".into(), "write_file".into()],
            max_turns: Some(20),
            ..AgentDef::new("coder")
        };
        let json = serde_json::to_string(&agent).expect("serialize");
        let back: AgentDef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, "coder");
        assert_eq!(back.tools.len(), 2);
        assert_eq!(back.max_turns, Some(20));
    }

    #[test]
    fn serde_empty_vecs_omitted() {
        let agent = AgentDef::new("minimal");
        let json = serde_json::to_string(&agent).expect("serialize");
        assert!(!json.contains("tools"));
        assert!(!json.contains("skills"));
    }
}
