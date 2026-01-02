//! Workflow parsing structures

use std::sync::Arc;

use serde::Deserialize;

use crate::error::NikaError;
use crate::output_policy::OutputPolicy;
use crate::task_action::TaskAction;
use crate::use_wiring::UseWiring;

/// Expected schema version for v0.1 workflows
pub const SCHEMA_V01: &str = "nika/workflow@0.1";

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

impl Workflow {
    /// Validate the workflow schema version
    ///
    /// Returns error if schema doesn't match expected version.
    pub fn validate_schema(&self) -> Result<(), NikaError> {
        if self.schema != SCHEMA_V01 {
            return Err(NikaError::InvalidSchema {
                expected: SCHEMA_V01.to_string(),
                actual: self.schema.clone(),
            });
        }
        Ok(())
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
    pub use_wiring: Option<UseWiring>,
    /// Output format and validation (v0.1)
    #[serde(default)]
    pub output: Option<OutputPolicy>,
    #[serde(flatten)]
    pub action: TaskAction,
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
