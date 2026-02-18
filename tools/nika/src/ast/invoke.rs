//! Invoke Action - MCP tool calls and resource reads (v0.2)
//!
//! Defines the invoke verb parameters for MCP integration:
//! - Tool calls: `mcp` + `tool` + optional `params`
//! - Resource reads: `mcp` + `resource`
//!
//! Tool and resource are mutually exclusive - exactly one must be specified.

use serde::Deserialize;

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
///   tool: novanet_generate
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
    pub mcp: String,

    /// Tool name to call (mutually exclusive with `resource`)
    #[serde(default)]
    pub tool: Option<String>,

    /// Parameters to pass to the tool
    #[serde(default)]
    pub params: Option<serde_json::Value>,

    /// Resource URI to read (mutually exclusive with `tool`)
    #[serde(default)]
    pub resource: Option<String>,
}

impl InvokeParams {
    /// Validate that exactly one of `tool` or `resource` is specified.
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - Both `tool` and `resource` are `Some` (mutually exclusive)
    /// - Both `tool` and `resource` are `None` (one is required)
    pub fn validate(&self) -> Result<(), String> {
        match (&self.tool, &self.resource) {
            (Some(_), Some(_)) => {
                Err("'tool' and 'resource' are mutually exclusive - specify only one".to_string())
            }
            (None, None) => Err("either 'tool' or 'resource' must be specified".to_string()),
            _ => Ok(()),
        }
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
    use serde_json::json;

    #[test]
    fn parse_tool_call() {
        let yaml = r#"
mcp: novanet
tool: novanet_generate
params:
  entity: qr-code
"#;
        let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.mcp, "novanet");
        assert_eq!(params.tool, Some("novanet_generate".to_string()));
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
        assert_eq!(params.mcp, "novanet");
        assert!(params.tool.is_none());
        assert_eq!(params.resource, Some("entity://qr-code/fr-FR".to_string()));
    }

    #[test]
    fn validate_ok_tool() {
        let params = InvokeParams {
            mcp: "test".to_string(),
            tool: Some("test_tool".to_string()),
            params: None,
            resource: None,
        };
        assert!(params.validate().is_ok());
        assert!(params.is_tool_call());
        assert!(!params.is_resource_read());
    }

    #[test]
    fn validate_ok_resource() {
        let params = InvokeParams {
            mcp: "test".to_string(),
            tool: None,
            params: None,
            resource: Some("test://resource".to_string()),
        };
        assert!(params.validate().is_ok());
        assert!(!params.is_tool_call());
        assert!(params.is_resource_read());
    }

    #[test]
    fn validate_err_both() {
        let params = InvokeParams {
            mcp: "test".to_string(),
            tool: Some("test_tool".to_string()),
            params: None,
            resource: Some("test://resource".to_string()),
        };
        let result = params.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mutually exclusive"));
    }

    #[test]
    fn validate_err_neither() {
        let params = InvokeParams {
            mcp: "test".to_string(),
            tool: None,
            params: None,
            resource: None,
        };
        let result = params.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be specified"));
    }
}
