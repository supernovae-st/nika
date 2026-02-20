//! Task Action Types - the 5 action verbs (v0.2)
//!
//! Defines the task action variants:
//! - `InferParams`: One-shot LLM call
//! - `ExecParams`: Shell command execution
//! - `FetchParams`: HTTP request
//! - `InvokeParams`: MCP tool call / resource read (v0.2)
//! - `AgentParams`: Agentic execution with tool calling (v0.2)
//!
//! ## Shorthand Syntax (v0.5.1)
//!
//! `infer:` and `exec:` support shorthand string syntax:
//! ```yaml
//! # Shorthand
//! infer: "Generate a headline"
//! exec: "echo hello"
//!
//! # Full form (equivalent)
//! infer:
//!   prompt: "Generate a headline"
//! exec:
//!   command: "echo hello"
//! ```

use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer};

use crate::ast::{AgentParams, InvokeParams};

/// Infer action - one-shot LLM call
///
/// Supports shorthand: `infer: "prompt"` or full form `infer: { prompt: "..." }`
#[derive(Debug, Clone)]
pub struct InferParams {
    pub prompt: String,
    /// Override provider for this task
    pub provider: Option<String>,
    /// Override model for this task
    pub model: Option<String>,
}

impl<'de> Deserialize<'de> for InferParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum InferParamsHelper {
            Short(String),
            Full {
                prompt: String,
                #[serde(default)]
                provider: Option<String>,
                #[serde(default)]
                model: Option<String>,
            },
        }

        match InferParamsHelper::deserialize(deserializer)? {
            InferParamsHelper::Short(prompt) => Ok(InferParams {
                prompt,
                provider: None,
                model: None,
            }),
            InferParamsHelper::Full {
                prompt,
                provider,
                model,
            } => Ok(InferParams {
                prompt,
                provider,
                model,
            }),
        }
    }
}

/// Exec action - shell command
///
/// Supports shorthand: `exec: "command"` or full form `exec: { command: "..." }`
#[derive(Debug, Clone)]
pub struct ExecParams {
    pub command: String,
}

impl<'de> Deserialize<'de> for ExecParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ExecParamsHelper {
            Short(String),
            Full { command: String },
        }

        match ExecParamsHelper::deserialize(deserializer)? {
            ExecParamsHelper::Short(command) => Ok(ExecParams { command }),
            ExecParamsHelper::Full { command } => Ok(ExecParams { command }),
        }
    }
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
