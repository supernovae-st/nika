// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! BuiltinTool trait definition for nika:* tools.

use crate::error::NikaError;
use std::future::Future;
use std::pin::Pin;

/// Trait for builtin nika:* tools.
///
/// All builtin tools must be Send + Sync to support concurrent execution
/// in the DAG runner.
pub trait BuiltinTool: Send + Sync {
    /// Tool name (without nika: prefix).
    ///
    /// # Example
    /// ```ignore
    /// fn name(&self) -> &'static str { "sleep" }  // for nika:sleep
    /// ```
    fn name(&self) -> &'static str;

    /// Tool description for LLM tool discovery.
    ///
    /// Defaults to empty string if not overridden.
    fn description(&self) -> &'static str {
        ""
    }

    /// JSON schema for tool parameters.
    ///
    /// Used for tool discovery and validation.
    /// Defaults to empty object if not overridden.
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    /// Execute the tool with JSON-encoded arguments.
    ///
    /// Returns the tool result as a JSON string.
    ///
    /// # Arguments
    /// * `args` - JSON-encoded parameters for the tool
    ///
    /// # Returns
    /// * `Ok(String)` - JSON-encoded result on success
    /// * `Err(NikaError)` - Tool execution error
    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>>;
}
