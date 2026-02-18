//! Task Action Types - the 5 action verbs (v0.2)
//!
//! Defines the task action variants:
//! - `InferParams`: One-shot LLM call
//! - `ExecParams`: Shell command execution
//! - `FetchParams`: HTTP request
//! - `InvokeParams`: MCP tool call / resource read (v0.2)
//! - `AgentParams`: Agentic execution with tool calling (v0.2)

use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::ast::{AgentParams, InvokeParams};

/// Infer action - one-shot LLM call
#[derive(Debug, Clone, Deserialize)]
pub struct InferParams {
    pub prompt: String,
    /// Override provider for this task
    #[serde(default)]
    pub provider: Option<String>,
    /// Override model for this task
    #[serde(default)]
    pub model: Option<String>,
}

/// Exec action - shell command
#[derive(Debug, Clone, Deserialize)]
pub struct ExecParams {
    pub command: String,
}

/// Fetch action - HTTP request
#[derive(Debug, Clone, Deserialize)]
pub struct FetchParams {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: FxHashMap<String, String>,
    pub body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

/// The 5 task action types (v0.2)
///
/// Each variant corresponds to a YAML verb:
/// - `infer:` - LLM inference (one-shot)
/// - `exec:` - Shell command execution
/// - `fetch:` - HTTP request
/// - `invoke:` - MCP tool call or resource read
/// - `agent:` - Agentic execution with tool calling loop
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TaskAction {
    Infer { infer: InferParams },
    Exec { exec: ExecParams },
    Fetch { fetch: FetchParams },
    Invoke { invoke: InvokeParams },
    Agent { agent: AgentParams },
}
