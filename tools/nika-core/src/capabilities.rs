//! Capability inference and enforcement for Nika Shield.
//!
//! Each task has a minimum capability set inferred from its YAML. Runtime
//! enforcement limits blast radius if a task is compromised.
//!
//! Based on CaMeL (Google DeepMind, arxiv:2503.18813).

use crate::ast::analyzed::{AnalyzedTask, AnalyzedTaskAction};

/// Capabilities inferred from a task's YAML definition.
#[derive(Debug, Clone, Default)]
pub struct TaskCapabilities {
    /// Can read files (exec with cat/read, nika:read, nika:glob, nika:grep)
    pub can_read_files: bool,
    /// Can write files (nika:write, nika:edit)
    pub can_write_files: bool,
    /// Can execute shell commands
    pub can_exec: bool,
    /// Can make HTTP requests
    pub can_fetch: bool,
    /// Can call MCP tools (list of server::tool names)
    pub mcp_tools: Vec<String>,
    /// Can call builtin tools (list of nika:tool names)
    pub builtin_tools: Vec<String>,
    /// Has agent capabilities (multi-turn, dynamic tool calls)
    pub is_agent: bool,
}

impl TaskCapabilities {
    /// Check if this task has any dangerous capabilities that require trust.
    pub fn has_dangerous_capabilities(&self) -> bool {
        self.can_write_files || self.can_exec
    }
}

/// Infer capabilities from an analyzed task.
pub fn infer_capabilities(task: &AnalyzedTask) -> TaskCapabilities {
    let mut caps = TaskCapabilities::default();
    match &task.action {
        AnalyzedTaskAction::Infer(_) => {
            // Pure inference — no capabilities needed
        }
        AnalyzedTaskAction::Exec(_) => {
            caps.can_exec = true;
        }
        AnalyzedTaskAction::Fetch(_) => {
            caps.can_fetch = true;
        }
        AnalyzedTaskAction::Invoke(inv) => {
            if inv.tool.starts_with("nika:") {
                caps.builtin_tools.push(inv.tool.clone());
                // Infer file capabilities from tool name
                match inv.tool.as_str() {
                    "nika:read" | "nika:glob" | "nika:grep" => caps.can_read_files = true,
                    "nika:write" | "nika:edit" => caps.can_write_files = true,
                    _ => {}
                }
            } else {
                caps.mcp_tools.push(inv.tool.clone());
            }
        }
        AnalyzedTaskAction::Agent(agent) => {
            caps.is_agent = true;
            for tool in &agent.tools {
                if tool.starts_with("nika:") {
                    caps.builtin_tools.push(tool.clone());
                    match tool.as_str() {
                        "nika:read" | "nika:glob" | "nika:grep" => caps.can_read_files = true,
                        "nika:write" | "nika:edit" => caps.can_write_files = true,
                        _ => {}
                    }
                } else {
                    caps.mcp_tools.push(tool.clone());
                }
            }
            // Agents can transitively fetch if they have HTTP-like tools
            caps.can_fetch = agent
                .tools
                .iter()
                .any(|t| !t.starts_with("nika:") && t.contains("fetch"));
        }
    }
    caps
}

/// Filter dangerous tools from an agent's tool list based on trust chain.
///
/// If the agent has untrusted inputs and is not elevated, removes tools
/// from the `dangerous_tools` list.
pub fn restrict_agent_tools(
    tools: Vec<String>,
    has_untrusted_inputs: bool,
    trust_elevated: bool,
    dangerous_tools: &[String],
) -> (Vec<String>, Vec<String>) {
    if !has_untrusted_inputs || trust_elevated {
        return (tools, vec![]);
    }

    let mut kept = Vec::new();
    let mut removed = Vec::new();
    for tool in tools {
        if dangerous_tools.contains(&tool) {
            removed.push(tool);
        } else {
            kept.push(tool);
        }
    }
    (kept, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_restrict_tools_removes_dangerous_when_untrusted() {
        let tools = vec![
            "nika:write".to_string(),
            "novanet::search".to_string(),
            "nika:exec".to_string(),
        ];
        let dangerous = vec!["nika:write".to_string(), "nika:exec".to_string()];
        let (kept, removed) = restrict_agent_tools(tools, true, false, &dangerous);
        assert_eq!(kept, vec!["novanet::search".to_string()]);
        assert_eq!(
            removed,
            vec!["nika:write".to_string(), "nika:exec".to_string()]
        );
    }

    #[test]
    fn test_restrict_tools_keeps_all_when_elevated() {
        let tools = vec!["nika:write".to_string(), "novanet::search".to_string()];
        let dangerous = vec!["nika:write".to_string()];
        let (kept, removed) = restrict_agent_tools(tools, true, true, &dangerous);
        assert_eq!(kept.len(), 2);
        assert!(removed.is_empty());
    }

    #[test]
    fn test_restrict_tools_keeps_all_when_trusted() {
        let tools = vec!["nika:write".to_string(), "novanet::search".to_string()];
        let dangerous = vec!["nika:write".to_string()];
        let (kept, removed) = restrict_agent_tools(tools, false, false, &dangerous);
        assert_eq!(kept.len(), 2);
        assert!(removed.is_empty());
    }
}
