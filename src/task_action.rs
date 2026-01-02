//! Task action definitions (v0.1)
//!
//! Defines the 3 action types: Infer, Exec, Fetch

use serde::Deserialize;
use std::collections::HashMap;

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
    pub headers: HashMap<String, String>,
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
