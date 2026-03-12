//! Daemon IPC Protocol
//!
//! JSON-based request/response protocol over Unix socket.
//! Each message is newline-delimited JSON.

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════════
// REQUESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Request from client to daemon
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum DaemonRequest {
    /// Ping to check if daemon is alive
    Ping,

    /// Graceful shutdown
    Shutdown,

    /// Get a secret from keychain
    GetSecret {
        /// Provider name (e.g., "anthropic", "openai")
        provider: String,
    },

    /// Set a secret in keychain
    SetSecret {
        /// Provider name
        provider: String,
        /// Secret value
        value: String,
    },

    /// Delete a secret from keychain
    DeleteSecret {
        /// Provider name
        provider: String,
    },

    /// List all stored providers
    ListProviders,

    /// Get daemon status information
    Status,
}

// ═══════════════════════════════════════════════════════════════════════════════
// RESPONSES
// ═══════════════════════════════════════════════════════════════════════════════

/// Response from daemon to client
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum DaemonResponse {
    /// Pong response to Ping
    Pong,

    /// Shutdown acknowledged
    ShuttingDown,

    /// Secret value retrieved
    Secret {
        /// The secret value (will be masked in logs)
        value: String,
    },

    /// Secret was set successfully
    SecretSet,

    /// Secret was deleted successfully
    SecretDeleted,

    /// List of providers with secrets
    ProviderList {
        /// Provider names that have stored secrets
        providers: Vec<String>,
    },

    /// Daemon status information
    StatusInfo {
        /// Daemon version
        version: String,
        /// Uptime in seconds
        uptime_secs: u64,
        /// Number of requests handled
        requests_handled: u64,
        /// PID of daemon process
        pid: u32,
    },

    /// Error response
    Error {
        /// Error code
        code: String,
        /// Error message
        message: String,
    },
}

impl DaemonResponse {
    /// Create an error response
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }

    /// Create a "not found" error
    pub fn not_found(provider: &str) -> Self {
        Self::error(
            "NOT_FOUND",
            format!("Secret not found for provider: {}", provider),
        )
    }

    /// Create a "keychain error" response
    pub fn keychain_error(message: impl Into<String>) -> Self {
        Self::error("KEYCHAIN_ERROR", message)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// SERIALIZATION HELPERS
// ═══════════════════════════════════════════════════════════════════════════════

impl DaemonRequest {
    /// Serialize to JSON with newline
    pub fn to_json_line(&self) -> serde_json::Result<String> {
        let mut json = serde_json::to_string(self)?;
        json.push('\n');
        Ok(json)
    }

    /// Deserialize from JSON (with or without newline)
    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s.trim())
    }
}

impl DaemonResponse {
    /// Serialize to JSON with newline
    pub fn to_json_line(&self) -> serde_json::Result<String> {
        let mut json = serde_json::to_string(self)?;
        json.push('\n');
        Ok(json)
    }

    /// Deserialize from JSON (with or without newline)
    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s.trim())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_ping_serialization() {
        let req = DaemonRequest::Ping;
        let json = req.to_json_line().unwrap();
        assert_eq!(json, "{\"type\":\"Ping\"}\n");

        let parsed = DaemonRequest::from_json(&json).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn test_request_get_secret_serialization() {
        let req = DaemonRequest::GetSecret {
            provider: "anthropic".to_string(),
        };
        let json = req.to_json_line().unwrap();
        assert!(json.contains("GetSecret"));
        assert!(json.contains("anthropic"));

        let parsed = DaemonRequest::from_json(&json).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn test_request_set_secret_serialization() {
        let req = DaemonRequest::SetSecret {
            provider: "openai".to_string(),
            value: "sk-test-key".to_string(),
        };
        let json = req.to_json_line().unwrap();
        assert!(json.contains("SetSecret"));
        assert!(json.contains("openai"));
        // Note: value is serialized but should be masked in logs

        let parsed = DaemonRequest::from_json(&json).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn test_request_shutdown_serialization() {
        let req = DaemonRequest::Shutdown;
        let json = req.to_json_line().unwrap();
        assert_eq!(json, "{\"type\":\"Shutdown\"}\n");
    }

    #[test]
    fn test_response_pong_serialization() {
        let resp = DaemonResponse::Pong;
        let json = resp.to_json_line().unwrap();
        assert_eq!(json, "{\"type\":\"Pong\"}\n");

        let parsed = DaemonResponse::from_json(&json).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn test_response_secret_serialization() {
        let resp = DaemonResponse::Secret {
            value: "sk-ant-api-key".to_string(),
        };
        let json = resp.to_json_line().unwrap();
        assert!(json.contains("Secret"));
        assert!(json.contains("sk-ant-api-key"));

        let parsed = DaemonResponse::from_json(&json).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn test_response_error_serialization() {
        let resp = DaemonResponse::error("TEST_ERROR", "Something went wrong");
        let json = resp.to_json_line().unwrap();
        assert!(json.contains("Error"));
        assert!(json.contains("TEST_ERROR"));
        assert!(json.contains("Something went wrong"));

        let parsed = DaemonResponse::from_json(&json).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn test_response_status_info_serialization() {
        let resp = DaemonResponse::StatusInfo {
            version: "0.27.0".to_string(),
            uptime_secs: 3600,
            requests_handled: 42,
            pid: 12345,
        };
        let json = resp.to_json_line().unwrap();
        assert!(json.contains("StatusInfo"));
        assert!(json.contains("0.27.0"));
        assert!(json.contains("3600"));
        assert!(json.contains("42"));
        assert!(json.contains("12345"));

        let parsed = DaemonResponse::from_json(&json).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn test_response_provider_list_serialization() {
        let resp = DaemonResponse::ProviderList {
            providers: vec!["anthropic".to_string(), "openai".to_string()],
        };
        let json = resp.to_json_line().unwrap();
        assert!(json.contains("ProviderList"));
        assert!(json.contains("anthropic"));
        assert!(json.contains("openai"));

        let parsed = DaemonResponse::from_json(&json).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn test_error_helpers() {
        let not_found = DaemonResponse::not_found("gemini");
        match not_found {
            DaemonResponse::Error { code, message } => {
                assert_eq!(code, "NOT_FOUND");
                assert!(message.contains("gemini"));
            }
            _ => panic!("Expected Error variant"),
        }

        let keychain_err = DaemonResponse::keychain_error("Access denied");
        match keychain_err {
            DaemonResponse::Error { code, message } => {
                assert_eq!(code, "KEYCHAIN_ERROR");
                assert_eq!(message, "Access denied");
            }
            _ => panic!("Expected Error variant"),
        }
    }

    #[test]
    fn test_all_request_variants() {
        // Ensure all variants can be serialized and deserialized
        let variants = vec![
            DaemonRequest::Ping,
            DaemonRequest::Shutdown,
            DaemonRequest::GetSecret {
                provider: "test".to_string(),
            },
            DaemonRequest::SetSecret {
                provider: "test".to_string(),
                value: "secret".to_string(),
            },
            DaemonRequest::DeleteSecret {
                provider: "test".to_string(),
            },
            DaemonRequest::ListProviders,
            DaemonRequest::Status,
        ];

        for req in variants {
            let json = req.to_json_line().unwrap();
            let parsed = DaemonRequest::from_json(&json).unwrap();
            assert_eq!(parsed, req);
        }
    }

    #[test]
    fn test_all_response_variants() {
        // Ensure all variants can be serialized and deserialized
        let variants = vec![
            DaemonResponse::Pong,
            DaemonResponse::ShuttingDown,
            DaemonResponse::Secret {
                value: "test".to_string(),
            },
            DaemonResponse::SecretSet,
            DaemonResponse::SecretDeleted,
            DaemonResponse::ProviderList {
                providers: vec!["a".to_string()],
            },
            DaemonResponse::StatusInfo {
                version: "1.0.0".to_string(),
                uptime_secs: 0,
                requests_handled: 0,
                pid: 1,
            },
            DaemonResponse::Error {
                code: "ERR".to_string(),
                message: "msg".to_string(),
            },
        ];

        for resp in variants {
            let json = resp.to_json_line().unwrap();
            let parsed = DaemonResponse::from_json(&json).unwrap();
            assert_eq!(parsed, resp);
        }
    }
}
