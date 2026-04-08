// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Invoke Action - MCP tool calls and resource reads
//!
//! Defines the invoke verb parameters for MCP integration:
//! - Tool calls: `mcp` + `tool` + optional `params`
//! - Resource reads: `mcp` + `resource`
//!
//! Tool and resource are mutually exclusive - exactly one must be specified.

use serde::Deserialize;

use crate::error::NikaError;

/// Invoke action - MCP integration
///
/// Used to call MCP server tools or read MCP resources.
/// Exactly one of `tool` or `resource` must be specified.
///
/// # Examples
///
/// Tool call:
/// ```yaml
/// invoke:
///   mcp: novanet
///   tool: novanet_context
///   params:
///     entity: qr-code
///     locale: fr-FR
/// ```
///
/// Resource read:
/// ```yaml
/// invoke:
///   mcp: novanet
///   resource: entity://qr-code/fr-FR
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct InvokeParams {
    /// MCP server name (must match a key in workflow's `mcp` config)
    ///
    /// Also accepts `server` as an alias.
    /// Optional for builtin tools (nika:* prefix) which don't need MCP.
    #[serde(alias = "server", default)]
    pub mcp: Option<String>,

    /// Tool name to call (mutually exclusive with `resource`)
    #[serde(default)]
    pub tool: Option<String>,

    /// Parameters to pass to the tool
    #[serde(default)]
    pub params: Option<serde_json::Value>,

    /// Resource URI to read (mutually exclusive with `tool`)
    #[serde(default)]
    pub resource: Option<String>,

    /// Timeout in seconds for tool execution
    ///
    /// Overrides the global `INVOKE_TASK_DEADLINE` (300s) for this task.
    /// If not set, falls back to the global default.
    #[serde(default)]
    pub timeout: Option<u64>,
}

impl InvokeParams {
    /// Returns true if this invoke targets a builtin tool (nika:* prefix).
    #[inline]
    pub fn is_builtin_tool(&self) -> bool {
        self.tool.as_ref().is_some_and(|t| t.starts_with("nika:"))
    }
}

impl InvokeParams {
    /// Validate invoke parameters.
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - `mcp` server name is empty (unless builtin tool with nika:* prefix)
    /// - Both `tool` and `resource` are `Some` (mutually exclusive)
    /// - Both `tool` and `resource` are `None` (one is required)
    /// - `tool` is Some but empty string
    /// - `resource` is Some but empty string
    ///
    pub fn validate(&self) -> Result<(), NikaError> {
        // Builtin tools don't require mcp
        // Validate MCP server name is not empty (for non-builtin tools)
        if !self.is_builtin_tool() {
            match &self.mcp {
                None => {
                    return Err(NikaError::ValidationError {
                        reason: "'mcp' server name is required for non-builtin tools".into(),
                    })
                }
                Some(mcp) if mcp.trim().is_empty() => {
                    return Err(NikaError::ValidationError {
                        reason: "'mcp' server name cannot be empty".into(),
                    });
                }
                _ => {}
            }
        }

        match (&self.tool, &self.resource) {
            (Some(tool), Some(_)) if !tool.trim().is_empty() => {
                return Err(NikaError::ValidationError {
                    reason: "'tool' and 'resource' are mutually exclusive - specify only one"
                        .into(),
                });
            }
            (Some(tool), None) if tool.trim().is_empty() => {
                return Err(NikaError::ValidationError {
                    reason: "'tool' name cannot be empty".into(),
                });
            }
            (None, Some(resource)) if resource.trim().is_empty() => {
                return Err(NikaError::ValidationError {
                    reason: "'resource' URI cannot be empty".into(),
                });
            }
            (Some(_), Some(_)) => {
                return Err(NikaError::ValidationError {
                    reason: "'tool' and 'resource' are mutually exclusive - specify only one"
                        .into(),
                });
            }
            (None, None) => {
                return Err(NikaError::ValidationError {
                    reason: "either 'tool' or 'resource' must be specified".into(),
                });
            }
            _ => {}
        }

        if self.timeout == Some(0) {
            return Err(NikaError::ValidationError {
                reason: "invoke timeout must be greater than 0 seconds".into(),
            });
        }

        Ok(())
    }

    /// Returns `true` if this is a tool call (has `tool` set).
    #[inline]
    pub fn is_tool_call(&self) -> bool {
        self.tool.is_some()
    }

    /// Returns `true` if this is a resource read (has `resource` set).
    #[inline]
    pub fn is_resource_read(&self) -> bool {
        self.resource.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;
    use serde_json::json;

    #[test]
    fn parse_tool_call() {
        let yaml = r#"
mcp: novanet
tool: novanet_context
params:
  entity: qr-code
"#;
        let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.mcp, Some("novanet".to_string()));
        assert_eq!(params.tool, Some("novanet_context".to_string()));
        assert_eq!(params.params, Some(json!({"entity": "qr-code"})));
        assert!(params.resource.is_none());
    }

    #[test]
    fn parse_resource_read() {
        let yaml = r#"
mcp: novanet
resource: entity://qr-code/fr-FR
"#;
        let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.mcp, Some("novanet".to_string()));
        assert!(params.tool.is_none());
        assert_eq!(params.resource, Some("entity://qr-code/fr-FR".to_string()));
    }

    #[test]
    fn validate_ok_tool() {
        let params = InvokeParams {
            mcp: Some("test".to_string()),
            tool: Some("test_tool".to_string()),
            params: None,
            resource: None,
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert!(params.is_tool_call());
        assert!(!params.is_resource_read());
    }

    #[test]
    fn validate_ok_resource() {
        let params = InvokeParams {
            mcp: Some("test".to_string()),
            tool: None,
            params: None,
            resource: Some("test://resource".to_string()),
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert!(!params.is_tool_call());
        assert!(params.is_resource_read());
    }

    #[test]
    fn validate_err_both() {
        let params = InvokeParams {
            mcp: Some("test".to_string()),
            tool: Some("test_tool".to_string()),
            params: None,
            resource: Some("test://resource".to_string()),
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("mutually exclusive"));
    }

    #[test]
    fn validate_err_neither() {
        let params = InvokeParams {
            mcp: Some("test".to_string()),
            tool: None,
            params: None,
            resource: None,
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be specified"));
    }

    // =========================================================================
    // Empty String Validation Tests
    // =========================================================================

    #[test]
    fn validate_err_empty_mcp() {
        let params = InvokeParams {
            mcp: Some("".to_string()),
            tool: Some("test_tool".to_string()),
            params: None,
            resource: None,
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        assert!(result.unwrap_err().to_string().contains("mcp"));
    }

    #[test]
    fn validate_err_whitespace_mcp() {
        let params = InvokeParams {
            mcp: Some("   ".to_string()),
            tool: Some("test_tool".to_string()),
            params: None,
            resource: None,
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        assert!(result.unwrap_err().to_string().contains("mcp"));
    }

    #[test]
    fn validate_err_empty_tool() {
        let params = InvokeParams {
            mcp: Some("test".to_string()),
            tool: Some("".to_string()),
            params: None,
            resource: None,
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        assert!(result.unwrap_err().to_string().contains("tool"));
    }

    #[test]
    fn validate_err_whitespace_tool() {
        let params = InvokeParams {
            mcp: Some("test".to_string()),
            tool: Some("  \t  ".to_string()),
            params: None,
            resource: None,
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        assert!(result.unwrap_err().to_string().contains("tool"));
    }

    #[test]
    fn validate_err_empty_resource() {
        let params = InvokeParams {
            mcp: Some("test".to_string()),
            tool: None,
            params: None,
            resource: Some("".to_string()),
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        assert!(result.unwrap_err().to_string().contains("resource"));
    }

    #[test]
    fn validate_err_whitespace_resource() {
        let params = InvokeParams {
            mcp: Some("test".to_string()),
            tool: None,
            params: None,
            resource: Some("   ".to_string()),
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        assert!(result.unwrap_err().to_string().contains("resource"));
    }

    // =========================================================================
    // Builtin Tools Tests (nika:* prefix)
    // =========================================================================

    #[test]
    fn validate_ok_builtin_tool_without_mcp() {
        // Builtin tools (nika:* prefix) don't require mcp
        let params = InvokeParams {
            mcp: None,
            tool: Some("nika:sleep".to_string()),
            params: Some(json!({"duration": "1s"})),
            resource: None,
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert!(params.is_builtin_tool());
    }

    #[test]
    fn validate_ok_builtin_tool_with_mcp() {
        // Builtin tools can optionally specify mcp (ignored)
        let params = InvokeParams {
            mcp: Some("ignored".to_string()),
            tool: Some("nika:log".to_string()),
            params: Some(json!({"level": "info", "message": "test"})),
            resource: None,
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert!(params.is_builtin_tool());
    }

    #[test]
    fn validate_err_non_builtin_without_mcp() {
        // Non-builtin tools (no nika:* prefix) require mcp
        let params = InvokeParams {
            mcp: None,
            tool: Some("novanet_context".to_string()),
            params: None,
            resource: None,
            timeout: None,
        };
        let result = params.validate();
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        assert!(result.unwrap_err().to_string().contains("mcp"));
    }

    #[test]
    fn is_builtin_tool_detects_nika_prefix() {
        let params = InvokeParams {
            mcp: None,
            tool: Some("nika:sleep".to_string()),
            params: None,
            resource: None,
            timeout: None,
        };
        assert!(params.is_builtin_tool());
    }

    #[test]
    fn is_builtin_tool_rejects_non_nika() {
        let params = InvokeParams {
            mcp: Some("test".to_string()),
            tool: Some("novanet_context".to_string()),
            params: None,
            resource: None,
            timeout: None,
        };
        assert!(!params.is_builtin_tool());
    }

    #[test]
    fn parse_builtin_tool_without_mcp() {
        // YAML without mcp field should parse successfully for builtin tools
        let yaml = r#"
tool: nika:sleep
params:
  duration: "1s"
"#;
        let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();
        assert!(params.mcp.is_none());
        assert_eq!(params.tool, Some("nika:sleep".to_string()));
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    // =========================================================================
    // Timeout Field Tests
    // =========================================================================

    #[test]
    fn parse_tool_call_with_timeout() {
        let yaml = r#"
mcp: novanet
tool: novanet_context
timeout: 60
params:
  entity: qr-code
"#;
        let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.timeout, Some(60));
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn parse_tool_call_without_timeout() {
        let yaml = r#"
mcp: novanet
tool: novanet_context
"#;
        let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.timeout, None);
    }

    #[test]
    fn validate_err_timeout_zero() {
        let params = InvokeParams {
            mcp: None,
            tool: Some("nika:sleep".to_string()),
            params: None,
            resource: None,
            timeout: Some(0),
        };
        let err = params.validate().unwrap_err();
        assert!(
            err.to_string().contains("timeout"),
            "should reject timeout=0: {err}"
        );
    }

    #[test]
    fn validate_ok_timeout_positive() {
        let params = InvokeParams {
            mcp: Some("server".to_string()),
            tool: Some("server::tool".to_string()),
            params: None,
            resource: None,
            timeout: Some(5),
        };
        assert!(params.validate().is_ok());
    }
}
