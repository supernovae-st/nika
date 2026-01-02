//! Task Action Types - the 3 action verbs (v0.1)
//!
//! Defines the task action variants:
//! - `InferParams`: One-shot LLM call
//! - `ExecParams`: Shell command execution
//! - `FetchParams`: HTTP request

use rustc_hash::FxHashMap;
use serde::Deserialize;

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

/// The 3 task action types (v0.1)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TaskAction {
    Infer { infer: InferParams },
    Exec { exec: ExecParams },
    Fetch { fetch: FetchParams },
}
