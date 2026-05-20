// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! EngineRunExecutor — the production impl of [`RunExecutor`].
//!
//! Called by `RunTool` in nika-builtin after Shield checks. Handles:
//! - YAML parsing via `parse_analyzed()`
//! - `Runner` construction and configuration
//! - `WORKFLOW_DEPTH` + `PARENT_CHAIN` scoping
//! - Timeout wrapping

use crate::ast::parse_analyzed;
use crate::runtime::Runner;
use async_trait::async_trait;
use nika_core::trust::InvocationSource;
use nika_kernel::builtin::BuiltinError;
use nika_kernel::scope::{RunExecutor, RunSpec};
use nika_kernel::task_local::{PARENT_CHAIN, WORKFLOW_DEPTH};
use std::cell::Cell;
use std::time::Duration;

/// Production `RunExecutor` — wraps the nika-engine `Runner`.
///
/// Constructed once at startup and stored in `BuiltinToolRouter`.
#[derive(Default)]
pub struct EngineRunExecutor;

impl EngineRunExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RunExecutor for EngineRunExecutor {
    async fn run_workflow(&self, spec: RunSpec) -> Result<serde_json::Value, BuiltinError> {
        // Get YAML content from file or inline
        let yaml_content = get_yaml_content(&spec).await?;

        // Parse the workflow
        let workflow = parse_analyzed(&yaml_content).map_err(|e| BuiltinError::Other {
            tool: "nika:run".into(),
            reason: format!("Failed to parse workflow YAML: {e}"),
        })?;

        // Build runner
        let mut runner = Runner::new(workflow).map_err(|e| BuiltinError::Other {
            tool: "nika:run".into(),
            reason: format!("Failed to create runner: {e}"),
        })?;
        runner = runner.quiet();

        // Propagate base_path from workflow file location
        if let Some(ref path) = spec.path {
            if let Ok(canonical) = path.canonicalize() {
                if let Some(dir) = canonical.parent() {
                    runner = runner.with_base_path(dir.to_path_buf());
                }
            }
        }

        // Trust ceiling propagation (Shield Item 4)
        runner = runner.with_invocation_source(InvocationSource::NestedRun {
            ceiling: spec.caller_trust,
        });

        // Inject parent context — keyed by the canonical reserved binding so
        // the child can read it via `with: { x: $__parent_context__ }`.
        if let Some(context) = spec.parent_context {
            runner =
                runner.with_initial_context(nika_core::ast::PARENT_CONTEXT_BINDING, context);
        }

        let timeout = spec.timeout;
        let depth = spec.depth;
        let parent_chain = spec.parent_chain;

        // Execute within task_local scopes for depth tracking and cycle detection
        let execution_result = WORKFLOW_DEPTH
            .scope(Cell::new(depth), async {
                PARENT_CHAIN
                    .scope(parent_chain, async {
                        tokio::time::timeout(timeout, runner.run()).await
                    })
                    .await
            })
            .await;

        let timeout_secs = timeout.as_secs();
        match execution_result {
            Ok(Ok(output_str)) => {
                // Runner::run() returns a JSON string — parse it
                serde_json::from_str::<serde_json::Value>(&output_str).map_err(|e| {
                    BuiltinError::Other {
                        tool: "nika:run".into(),
                        reason: format!("Failed to parse workflow output as JSON: {e}"),
                    }
                })
            }
            Ok(Err(e)) => Err(BuiltinError::Other {
                tool: "nika:run".into(),
                reason: format!("Workflow execution failed: {e}"),
            }),
            Err(_) => Err(BuiltinError::Timeout {
                tool: format!("nika:run (timed out after {timeout_secs}s)"),
            }),
        }
    }
}

/// Load YAML content from file (with 30s read timeout) or return inline content.
async fn get_yaml_content(spec: &RunSpec) -> Result<String, BuiltinError> {
    if let Some(ref inline) = spec.yaml_content {
        if !inline.contains("schema:") {
            return Err(BuiltinError::InvalidArgs {
                tool: "nika:run".into(),
                reason: "Inline YAML must contain 'schema:' field".into(),
            });
        }
        return Ok(inline.clone());
    }

    let path = spec.path.as_ref().ok_or_else(|| BuiltinError::InvalidArgs {
        tool: "nika:run".into(),
        reason: "Either path or yaml_content must be set in RunSpec".into(),
    })?;

    let canonical = path.canonicalize().map_err(|e| BuiltinError::Other {
        tool: "nika:run".into(),
        reason: format!(
            "Failed to resolve workflow path '{}': {e}",
            path.display()
        ),
    })?;

    tokio::time::timeout(Duration::from_secs(30), tokio::fs::read_to_string(&canonical))
        .await
        .map_err(|_| BuiltinError::Timeout {
            tool: format!(
                "nika:run (reading {} timed out after 30s)",
                canonical.display()
            ),
        })?
        .map_err(|e| BuiltinError::Io {
            tool: "nika:run".into(),
            reason: format!("Failed to read workflow file '{}': {e}", canonical.display()),
        })
}

