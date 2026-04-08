// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Custom endpoint configuration for OpenAI-compatible servers (vLLM, TGI, Ollama, etc.)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for a named custom endpoint (stored in config.toml).
///
/// Example TOML:
/// ```toml
/// [endpoints.h100]
/// base_url = "http://10.0.1.42:8000/v1"
/// api_key = "sk-internal-token"
/// model = "meta-llama/Llama-3.1-70B-Instruct"
/// ```
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomEndpointConfig {
    /// Base URL of the OpenAI-compatible API (required).
    /// Must include the `/v1` path if the server expects it.
    pub base_url: String,

    /// API key for authentication (optional — some servers like Ollama need no auth).
    /// Can be overridden by env var `NIKA_ENDPOINT_<NAME>_KEY`.
    #[serde(default)]
    pub api_key: Option<String>,

    /// Default model for this endpoint (optional).
    /// Used when no `model:` is specified on the task.
    #[serde(default)]
    pub model: Option<String>,

    /// Request timeout in seconds (optional, default: 300s).
    #[serde(default)]
    pub timeout_secs: Option<u64>,

    /// Hourly rate for this endpoint in the specified currency (for `nika bench` cost estimation).
    /// Local/self-hosted endpoints don't have per-token pricing, so cost is estimated as:
    /// `estimated_cost = (workflow_duration_secs / 3600) × hourly_rate`
    #[serde(default)]
    pub hourly_rate: Option<f64>,

    /// Currency for `hourly_rate` (default: "USD"). Display only — not converted.
    #[serde(default)]
    pub currency: Option<String>,
}

impl std::fmt::Debug for CustomEndpointConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomEndpointConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("model", &self.model)
            .field("timeout_secs", &self.timeout_secs)
            .field("hourly_rate", &self.hourly_rate)
            .field("currency", &self.currency)
            .finish()
    }
}

/// A resolved endpoint ready for use at runtime.
/// All env var overlays have been applied.
#[derive(Clone)]
pub struct ResolvedEndpoint {
    pub base_url: String,
    pub api_key: String,
    pub default_model: Option<String>,
    pub timeout_secs: u64,
    /// Hourly rate for cost estimation (None = no cost tracking).
    pub hourly_rate: Option<f64>,
    /// Currency label (e.g. "USD", "EUR").
    pub currency: String,
}

impl std::fmt::Debug for ResolvedEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedEndpoint")
            .field("base_url", &self.base_url)
            .field("api_key", &"***")
            .field("default_model", &self.default_model)
            .field("timeout_secs", &self.timeout_secs)
            .field("hourly_rate", &self.hourly_rate)
            .field("currency", &self.currency)
            .finish()
    }
}

/// Map of named endpoints (name -> resolved config).
pub type CustomEndpointMap = HashMap<String, ResolvedEndpoint>;

/// Validate that an endpoint URL is safe to use.
///
/// Rules:
/// - Must parse as valid URL with http or https scheme.
/// - Must NOT point to cloud metadata services (169.254.x.x, metadata.google.internal).
/// - Localhost (127.0.0.1, ::1) and private IPs (10.x, 172.16-31.x, 192.168.x) ARE allowed
///   because the primary use case is local/datacenter inference servers.
pub fn validate_endpoint_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL '{}': {}", url, e))?;

    // Scheme check
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "Unsupported scheme '{}' -- only http/https allowed",
                other
            ))
        }
    }

    // Host check — block metadata endpoints only
    if let Some(host) = parsed.host_str() {
        let h = host.to_lowercase();
        let h = h.trim_start_matches('[').trim_end_matches(']');

        // Block cloud metadata endpoints
        if h == "metadata.google.internal" || h == "metadata.google" || h == "169.254.169.254" {
            return Err(format!(
                "Blocked metadata endpoint '{}' -- SSRF protection",
                h
            ));
        }

        // Block link-local range (169.254.0.0/16) — metadata services hide here
        if let Ok(ip) = h.parse::<std::net::Ipv4Addr>() {
            let octets = ip.octets();
            if octets[0] == 169 && octets[1] == 254 {
                return Err(format!(
                    "Blocked link-local address '{}' -- metadata SSRF protection",
                    h
                ));
            }
        }

        // Block IPv4-mapped IPv6 addresses (::ffff:169.254.x.x)
        if let Ok(ip6) = h.parse::<std::net::Ipv6Addr>() {
            if let Some(ip4) = ip6.to_ipv4_mapped() {
                let octets = ip4.octets();
                if octets[0] == 169 && octets[1] == 254 {
                    return Err(format!(
                        "Blocked IPv4-mapped link-local address '{}' -- metadata SSRF protection",
                        h
                    ));
                }
            }
        }
    } else {
        return Err(format!("URL '{}' has no host", url));
    }

    Ok(())
}

/// Resolve a set of endpoint configs into runtime-ready endpoints.
///
/// Applies env var overrides:
/// - `NIKA_ENDPOINT_<NAME>_URL` overrides `base_url`
/// - `NIKA_ENDPOINT_<NAME>_KEY` overrides `api_key`
pub fn resolve_endpoints(
    configs: &HashMap<String, CustomEndpointConfig>,
) -> Result<CustomEndpointMap, String> {
    let mut map = CustomEndpointMap::new();

    for (name, cfg) in configs {
        let env_prefix = format!("NIKA_ENDPOINT_{}", name.to_uppercase().replace('-', "_"));

        // URL: env override or config value
        let base_url = std::env::var(format!("{}_URL", env_prefix))
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| cfg.base_url.clone());

        // Validate URL
        validate_endpoint_url(&base_url).map_err(|e| format!("Endpoint '{}': {}", name, e))?;

        // Key: env override -> config value -> empty string (servers like Ollama need no key)
        let api_key = std::env::var(format!("{}_KEY", env_prefix))
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| cfg.api_key.clone())
            .unwrap_or_else(|| "ollama".to_string()); // Ollama convention: any non-empty string

        let resolved = ResolvedEndpoint {
            base_url,
            api_key,
            default_model: cfg.model.clone(),
            timeout_secs: cfg.timeout_secs.unwrap_or(300),
            hourly_rate: cfg.hourly_rate,
            currency: cfg.currency.clone().unwrap_or_else(|| "USD".to_string()),
        };

        map.insert(name.clone(), resolved);
    }

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_endpoint_url_valid_http() {
        let result = validate_endpoint_url("http://localhost:8000/v1");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn test_validate_endpoint_url_valid_https() {
        let result = validate_endpoint_url("https://h100.internal:8000/v1");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn test_validate_endpoint_url_valid_private_ip() {
        let result = validate_endpoint_url("http://10.0.1.42:8000/v1");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_endpoint_url("http://192.168.1.100:8000/v1");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_endpoint_url("http://172.16.0.5:8000/v1");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn test_validate_endpoint_url_blocks_metadata() {
        let result = validate_endpoint_url("http://169.254.169.254/latest");
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        let result = validate_endpoint_url("http://metadata.google.internal/");
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    }

    #[test]
    fn test_validate_endpoint_url_blocks_link_local() {
        let result = validate_endpoint_url("http://169.254.0.1:8000");
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    }

    #[test]
    fn test_validate_endpoint_url_blocks_ipv4_mapped_ipv6_link_local() {
        let result = validate_endpoint_url("http://[::ffff:169.254.169.254]/v1");
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        let result = validate_endpoint_url("http://[::ffff:169.254.0.1]:8000");
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    }

    #[test]
    fn test_validate_endpoint_url_allows_ipv4_mapped_ipv6_private() {
        // Private IPs are allowed for local inference servers
        let result = validate_endpoint_url("http://[::ffff:10.0.1.42]:8000/v1");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = validate_endpoint_url("http://[::ffff:192.168.1.1]:8000/v1");
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn test_validate_endpoint_url_rejects_file_scheme() {
        let result = validate_endpoint_url("file:///etc/passwd");
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    }

    #[test]
    fn test_validate_endpoint_url_rejects_ftp() {
        let result = validate_endpoint_url("ftp://example.com");
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    }

    #[test]
    fn test_validate_endpoint_url_rejects_no_host() {
        let result = validate_endpoint_url("http://");
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    }

    #[test]
    fn test_serde_roundtrip() {
        let cfg = CustomEndpointConfig {
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: None,
            model: Some("llama3.2".to_string()),
            timeout_secs: Some(60),
            hourly_rate: None,
            currency: None,
        };
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let parsed: CustomEndpointConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn test_resolve_endpoints_basic() {
        let mut configs = HashMap::new();
        configs.insert(
            "ollama".to_string(),
            CustomEndpointConfig {
                base_url: "http://localhost:11434/v1".to_string(),
                api_key: None,
                model: Some("llama3.2".to_string()),
                timeout_secs: None,
                hourly_rate: None,
                currency: None,
            },
        );
        let resolved = resolve_endpoints(&configs).unwrap();
        assert_eq!(resolved.len(), 1);
        let ep = &resolved["ollama"];
        assert_eq!(ep.base_url, "http://localhost:11434/v1");
        assert_eq!(ep.api_key, "ollama"); // default for no-auth servers
        assert_eq!(ep.default_model.as_deref(), Some("llama3.2"));
        assert_eq!(ep.timeout_secs, 300); // default
        assert!(ep.hourly_rate.is_none());
        assert_eq!(ep.currency, "USD"); // default
    }

    #[test]
    fn test_resolve_endpoints_with_hourly_rate() {
        let mut configs = HashMap::new();
        configs.insert(
            "h100".to_string(),
            CustomEndpointConfig {
                base_url: "http://10.0.1.42:8000/v1".to_string(),
                api_key: Some("sk-internal".to_string()),
                model: None,
                timeout_secs: None,
                hourly_rate: Some(3.0),
                currency: Some("EUR".to_string()),
            },
        );
        let resolved = resolve_endpoints(&configs).unwrap();
        let ep = &resolved["h100"];
        assert_eq!(ep.hourly_rate, Some(3.0));
        assert_eq!(ep.currency, "EUR");
    }

    #[test]
    fn test_hourly_rate_serde_roundtrip() {
        let cfg = CustomEndpointConfig {
            base_url: "http://10.0.1.42:8000/v1".to_string(),
            api_key: Some("sk-internal".to_string()),
            model: Some("Qwen/Qwen3-8B".to_string()),
            timeout_secs: Some(60),
            hourly_rate: Some(3.0),
            currency: Some("EUR".to_string()),
        };
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let parsed: CustomEndpointConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn test_resolve_endpoints_with_key() {
        let mut configs = HashMap::new();
        configs.insert(
            "h100".to_string(),
            CustomEndpointConfig {
                base_url: "http://10.0.1.42:8000/v1".to_string(),
                api_key: Some("sk-internal".to_string()),
                model: None,
                timeout_secs: Some(60),
                hourly_rate: None,
                currency: None,
            },
        );
        let resolved = resolve_endpoints(&configs).unwrap();
        let ep = &resolved["h100"];
        assert_eq!(ep.api_key, "sk-internal");
        assert_eq!(ep.timeout_secs, 60);
    }

    #[test]
    fn test_resolve_endpoints_rejects_bad_url() {
        let mut configs = HashMap::new();
        configs.insert(
            "bad".to_string(),
            CustomEndpointConfig {
                base_url: "http://169.254.169.254/latest".to_string(),
                api_key: None,
                model: None,
                timeout_secs: None,
                hourly_rate: None,
                currency: None,
            },
        );
        let result = resolve_endpoints(&configs);
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    }

    #[test]
    fn test_full_endpoint_config_to_provider_resolution() {
        // Simulate config.toml with a custom endpoint
        let mut configs = HashMap::new();
        configs.insert(
            "vllm".to_string(),
            CustomEndpointConfig {
                base_url: "http://localhost:8000/v1".to_string(),
                api_key: Some("sk-test-key".to_string()),
                model: Some("Qwen/Qwen3-8B".to_string()),
                timeout_secs: Some(60),
                hourly_rate: None,
                currency: None,
            },
        );

        // Resolve endpoints
        let resolved = resolve_endpoints(&configs).unwrap();
        assert_eq!(resolved.len(), 1);

        // Create provider from resolved endpoint
        let provider =
            crate::provider::rig::RigProvider::from_name_with_endpoints("vllm", &resolved).unwrap();

        // Verify it's an OpenAiCompat variant
        assert!(matches!(
            provider,
            crate::provider::rig::RigProvider::OpenAiCompat { .. }
        ));

        // Verify unknown name falls back to catalog (and fails due to missing API key)
        let result =
            crate::provider::rig::RigProvider::from_name_with_endpoints("anthropic", &resolved);
        // May or may not fail depending on env — just verify it doesn't match as endpoint
        if let Ok(p) = &result {
            assert!(!matches!(
                p,
                crate::provider::rig::RigProvider::OpenAiCompat { .. }
            ));
        }
    }

    #[test]
    fn test_debug_masks_api_key_custom_endpoint_config() {
        let cfg = CustomEndpointConfig {
            base_url: "http://localhost:8000/v1".to_string(),
            api_key: Some("sk-secret-key-12345678".to_string()),
            model: None,
            timeout_secs: None,
            hourly_rate: None,
            currency: None,
        };
        let debug = format!("{:?}", cfg);
        assert!(
            !debug.contains("sk-secret-key-12345678"),
            "Debug must not leak api_key"
        );
        assert!(debug.contains("***"), "Debug should show masked key");
    }

    #[test]
    fn test_debug_masks_api_key_resolved_endpoint() {
        let ep = ResolvedEndpoint {
            base_url: "http://localhost:8000/v1".to_string(),
            api_key: "sk-secret-key-12345678".to_string(),
            default_model: None,
            timeout_secs: 300,
            hourly_rate: None,
            currency: "USD".to_string(),
        };
        let debug = format!("{:?}", ep);
        assert!(
            !debug.contains("sk-secret-key-12345678"),
            "Debug must not leak api_key"
        );
        assert!(debug.contains("***"), "Debug should show masked key");
    }
}
