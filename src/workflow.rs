//! Workflow parsing structures

use std::sync::Arc;
use serde::Deserialize;
use crate::task::{InferDef, ExecDef, FetchDef};
use crate::use_block::UseBlock;
use crate::output_policy::OutputPolicy;

/// Workflow parsed from YAML (raw)
#[derive(Debug, Deserialize)]
struct WorkflowRaw {
    pub schema: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub model: Option<String>,
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub flows: Vec<Flow>,
}

/// Workflow with Arc-wrapped tasks for efficient cloning
#[derive(Debug)]
pub struct Workflow {
    pub schema: String,
    pub provider: String,
    pub model: Option<String>,
    pub tasks: Vec<Arc<Task>>,
    pub flows: Vec<Flow>,
}

impl<'de> Deserialize<'de> for Workflow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = WorkflowRaw::deserialize(deserializer)?;
        Ok(Workflow {
            schema: raw.schema,
            provider: raw.provider,
            model: raw.model,
            tasks: raw.tasks.into_iter().map(Arc::new).collect(),
            flows: raw.flows,
        })
    }
}

fn default_provider() -> String {
    "claude".to_string()
}

#[derive(Debug, Deserialize)]
pub struct Task {
    pub id: String,
    /// Explicit data wiring (v0.1)
    #[serde(default, rename = "use")]
    pub use_block: Option<UseBlock>,
    /// Output format and validation (v0.1)
    #[serde(default)]
    pub output: Option<OutputPolicy>,
    #[serde(flatten)]
    pub action: TaskAction,
}

/// The 3 MVP verbs - serde auto-detects which one
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TaskAction {
    Infer { infer: InferDef },
    Exec { exec: ExecDef },
    Fetch { fetch: FetchDef },
}

#[derive(Debug, Deserialize)]
pub struct Flow {
    pub source: FlowEndpoint,
    pub target: FlowEndpoint,
}

/// Handles string OR array for source/target
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FlowEndpoint {
    Single(String),
    Multiple(Vec<String>),
}

impl FlowEndpoint {
    pub fn as_vec(&self) -> Vec<&str> {
        match self {
            FlowEndpoint::Single(s) => vec![s.as_str()],
            FlowEndpoint::Multiple(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}
