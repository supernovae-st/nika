//! Workflow Types - main workflow structure (v0.1, v0.2)
//!
//! Contains the core YAML-parsed types:
//! - `Workflow`: Root workflow with tasks and flows
//! - `Task`: Individual task definition
//! - `Flow`: DAG edge between tasks
//! - `FlowEndpoint`: Single or multiple task references
//! - `McpConfigInline`: Inline MCP server configuration (v0.2)

use rustc_hash::FxHashMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::binding::WiringSpec;
use crate::error::NikaError;

use super::action::TaskAction;
use super::output::OutputPolicy;

/// Expected schema version for v0.1 workflows
pub const SCHEMA_V01: &str = "nika/workflow@0.1";

/// Expected schema version for v0.2 workflows
pub const SCHEMA_V02: &str = "nika/workflow@0.2";

/// Expected schema version for v0.3 workflows (for_each parallelism)
pub const SCHEMA_V03: &str = "nika/workflow@0.3";

/// Expected schema version for v0.4 workflows (extended thinking)
pub const SCHEMA_V04: &str = "nika/workflow@0.4";

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
    pub env: FxHashMap<String, String>,
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
    pub mcp: Option<FxHashMap<String, McpConfigInline>>,
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
    pub mcp: Option<FxHashMap<String, McpConfigInline>>,
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
    /// Compute a hash of the workflow for cache invalidation (v0.4.1)
    ///
    /// Uses xxhash3 for fast hashing. The hash is computed from:
    /// - Schema version
    /// - Provider + model
    /// - Task count and IDs
    ///
    /// Returns a 16-character hex string (64-bit hash).
    pub fn compute_hash(&self) -> String {
        use xxhash_rust::xxh3::xxh3_64;

        let mut hasher_input = String::new();
        hasher_input.push_str(&self.schema);
        hasher_input.push_str(&self.provider);
        if let Some(ref model) = self.model {
            hasher_input.push_str(model);
        }
        hasher_input.push_str(&self.tasks.len().to_string());
        for task in &self.tasks {
            hasher_input.push_str(&task.id);
        }

        let hash = xxh3_64(hasher_input.as_bytes());
        format!("{:016x}", hash)
    }

    /// Validate the workflow schema version and task configuration
    ///
    /// Returns error if:
    /// - Schema doesn't match expected version (v0.1, v0.2, or v0.3)
    /// - Any task has invalid for_each configuration (non-array or empty)
    pub fn validate_schema(&self) -> Result<(), NikaError> {
        // Validate schema version
        if self.schema != SCHEMA_V01 && self.schema != SCHEMA_V02 && self.schema != SCHEMA_V03 {
            return Err(NikaError::InvalidSchema {
                expected: format!("{} or {} or {}", SCHEMA_V01, SCHEMA_V02, SCHEMA_V03),
                actual: self.schema.clone(),
            });
        }

        // Validate for_each on all tasks
        for task in &self.tasks {
            task.validate_for_each()?;
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
    pub use_wiring: Option<WiringSpec>,
    /// Output format and validation (v0.1)
    #[serde(default)]
    pub output: Option<OutputPolicy>,
    /// Parallel iteration over array values (v0.3)
    ///
    /// When specified, the task will be executed once for each value in the array.
    /// Each iteration runs in parallel with its own bindings.
    ///
    /// # Example
    ///
    /// ```yaml
    /// tasks:
    ///   - id: process_locales
    ///     for_each: ["en-US", "fr-FR", "de-DE"]
    ///     as: locale
    ///     exec:
    ///       command: "echo {{use.locale}}"
    /// ```
    #[serde(default)]
    pub for_each: Option<serde_json::Value>,
    /// Variable name for current iteration value (v0.3)
    ///
    /// Defaults to "item" if not specified.
    /// The value is accessible as `{{use.<as>}}` in templates.
    #[serde(default, rename = "as")]
    pub for_each_as: Option<String>,
    /// Maximum parallel executions for for_each (v0.3)
    ///
    /// Controls how many iterations run concurrently.
    /// Defaults to 1 (sequential). Set higher for parallel execution.
    ///
    /// # Example
    ///
    /// ```yaml
    /// for_each: ["a", "b", "c", "d", "e"]
    /// concurrency: 3  # Run at most 3 at a time
    /// ```
    #[serde(default)]
    pub concurrency: Option<usize>,
    /// Stop all iterations on first error (v0.3)
    ///
    /// When true (default), aborts remaining iterations if any fails.
    /// When false, continues executing remaining iterations.
    ///
    /// # Example
    ///
    /// ```yaml
    /// for_each: $items
    /// fail_fast: false  # Continue even if some fail
    /// ```
    #[serde(default)]
    pub fail_fast: Option<bool>,
    #[serde(flatten)]
    pub action: TaskAction,
}

impl Task {
    /// Validate for_each configuration (v0.3)
    ///
    /// Returns error if:
    /// - for_each is not an array
    /// - for_each array is empty
    pub fn validate_for_each(&self) -> Result<(), NikaError> {
        if let Some(for_each) = &self.for_each {
            if !for_each.is_array() {
                return Err(NikaError::ValidationError {
                    reason: format!("for_each must be an array, got {}", for_each),
                });
            }
            if let Some(arr) = for_each.as_array() {
                if arr.is_empty() {
                    return Err(NikaError::ValidationError {
                        reason: "for_each array cannot be empty".to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Check if this task has for_each iteration
    pub fn has_for_each(&self) -> bool {
        self.for_each.is_some()
    }

    /// Get the iteration variable name (defaults to "item")
    pub fn for_each_var(&self) -> &str {
        self.for_each_as.as_deref().unwrap_or("item")
    }

    /// Get the concurrency limit for for_each (defaults to 1 = sequential)
    pub fn for_each_concurrency(&self) -> usize {
        self.concurrency.unwrap_or(1).max(1) // At least 1
    }

    /// Get the fail_fast setting for for_each (defaults to true)
    pub fn for_each_fail_fast(&self) -> bool {
        self.fail_fast.unwrap_or(true)
    }
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
