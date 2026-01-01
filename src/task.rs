//! Task verb definitions

use serde::Deserialize;
use std::collections::HashMap;

/// Infer verb - one-shot LLM call
#[derive(Debug, Clone, Deserialize)]
pub struct InferDef {
    pub prompt: String,
    /// Override provider for this task
    #[serde(default)]
    pub provider: Option<String>,
    /// Override model for this task
    #[serde(default)]
    pub model: Option<String>,
}

/// Exec verb - shell command
#[derive(Debug, Clone, Deserialize)]
pub struct ExecDef {
    pub command: String,
}

/// Fetch verb - HTTP request
#[derive(Debug, Clone, Deserialize)]
pub struct FetchDef {
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
