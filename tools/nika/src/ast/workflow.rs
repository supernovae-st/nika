//! Workflow Types - main workflow structure (v0.1, v0.2)
//!
//! Contains the core YAML-parsed types:
//! - `Workflow`: Root workflow with tasks and flows
//! - `Task`: Individual task definition
//! - `Flow`: DAG edge between tasks
//! - `FlowEndpoint`: Single or multiple task references
//! - `McpConfigInline`: Inline MCP server configuration (v0.2)

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::binding::UseWiring;
use crate::error::NikaError;

use super::action::TaskAction;
use super::output::OutputPolicy;

/// Expected schema version for v0.1 workflows
pub const SCHEMA_V01: &str = "nika/workflow@0.1";

/// Expected schema version for v0.2 workflows
#[allow(dead_code)]
pub const SCHEMA_V02: &str = "nika/workflow@0.2";

/// Inline MCP server configuration (v0.2)
///
/// Allows workflows to define MCP servers directly in YAML.
/// The server name is the map key in the `mcp` field.
///
/// # Example
///
/// ```yaml
/// mcp:
///   novanet:
///     command: cargo
///     args: [run, -p, novanet-mcp]
///     env:
///       NEO4J_URI: bolt://localhost:7687
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct McpConfigInline {
    /// Command to spawn the MCP server
    pub command: String,
    /// Arguments to pass to the command
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables for the server process
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Working directory for the server process
    pub cwd: Option<String>,
}

/// Workflow parsed from YAML (raw)
#[derive(Debug, Deserialize)]
struct WorkflowRaw {
    pub schema: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub model: Option<String>,
    /// MCP server configurations (v0.2)
    #[serde(default)]
    pub mcp: Option<HashMap<String, McpConfigInline>>,
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
    /// MCP server configurations (v0.2)
    ///
    /// Allows workflows to define MCP servers inline rather than
    /// referencing external configuration. The map key is the server
    /// name used in `invoke.mcp` fields.
    pub mcp: Option<HashMap<String, McpConfigInline>>,
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
            mcp: raw.mcp,
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
            FlowEndpoint::Single(s) => vec![s],
            FlowEndpoint::Multiple(v) => v.iter().map(String::as_str).collect(),
        }
    }
}
