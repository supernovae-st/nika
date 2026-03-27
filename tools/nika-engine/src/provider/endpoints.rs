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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
}

/// A resolved endpoint ready for use at runtime.
/// All env var overlays have been applied.
#[derive(Debug, Clone)]
pub struct ResolvedEndpoint {
    pub base_url: String,
    pub api_key: String,
    pub default_model: Option<String>,
    pub timeout_secs: u64,
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
        other => return Err(format!("Unsupported scheme '{}' -- only http/https allowed", other)),
    }

    // Host check — block metadata endpoints only
    if let Some(host) = parsed.host_str() {
        let h = host.to_lowercase();
        let h = h.trim_start_matches('[').trim_end_matches(']');

        // Block cloud metadata endpoints
        if h == "metadata.google.internal"
            || h == "metadata.google"
            || h == "169.254.169.254"
        {
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
    configs: &indexmap::IndexMap<String, CustomEndpointConfig>,
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
        validate_endpoint_url(&base_url)
            .map_err(|e| format!("Endpoint '{}': {}", name, e))?;

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
        assert!(validate_endpoint_url("http://localhost:8000/v1").is_ok());
    }

    #[test]
    fn test_validate_endpoint_url_valid_https() {
        assert!(validate_endpoint_url("https://h100.internal:8000/v1").is_ok());
    }

    #[test]
    fn test_validate_endpoint_url_valid_private_ip() {
        assert!(validate_endpoint_url("http://10.0.1.42:8000/v1").is_ok());
        assert!(validate_endpoint_url("http://192.168.1.100:8000/v1").is_ok());
        assert!(validate_endpoint_url("http://172.16.0.5:8000/v1").is_ok());
    }

    #[test]
    fn test_validate_endpoint_url_blocks_metadata() {
        assert!(validate_endpoint_url("http://169.254.169.254/latest").is_err());
        assert!(validate_endpoint_url("http://metadata.google.internal/").is_err());
    }

    #[test]
    fn test_validate_endpoint_url_blocks_link_local() {
        assert!(validate_endpoint_url("http://169.254.0.1:8000").is_err());
    }

    #[test]
    fn test_validate_endpoint_url_rejects_file_scheme() {
        assert!(validate_endpoint_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn test_validate_endpoint_url_rejects_ftp() {
        assert!(validate_endpoint_url("ftp://example.com").is_err());
    }

    #[test]
    fn test_validate_endpoint_url_rejects_no_host() {
        assert!(validate_endpoint_url("http://").is_err());
    }

    #[test]
    fn test_serde_roundtrip() {
        let cfg = CustomEndpointConfig {
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: None,
            model: Some("llama3.2".to_string()),
            timeout_secs: Some(60),
        };
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let parsed: CustomEndpointConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn test_resolve_endpoints_basic() {
        let mut configs = indexmap::IndexMap::new();
        configs.insert(
            "ollama".to_string(),
            CustomEndpointConfig {
                base_url: "http://localhost:11434/v1".to_string(),
                api_key: None,
                model: Some("llama3.2".to_string()),
                timeout_secs: None,
            },
        );
        let resolved = resolve_endpoints(&configs).unwrap();
        assert_eq!(resolved.len(), 1);
        let ep = &resolved["ollama"];
        assert_eq!(ep.base_url, "http://localhost:11434/v1");
        assert_eq!(ep.api_key, "ollama"); // default for no-auth servers
        assert_eq!(ep.default_model.as_deref(), Some("llama3.2"));
        assert_eq!(ep.timeout_secs, 300); // default
    }

    #[test]
    fn test_resolve_endpoints_with_key() {
        let mut configs = indexmap::IndexMap::new();
        configs.insert(
            "h100".to_string(),
            CustomEndpointConfig {
                base_url: "http://10.0.1.42:8000/v1".to_string(),
                api_key: Some("sk-internal".to_string()),
                model: None,
                timeout_secs: Some(60),
            },
        );
        let resolved = resolve_endpoints(&configs).unwrap();
        let ep = &resolved["h100"];
        assert_eq!(ep.api_key, "sk-internal");
        assert_eq!(ep.timeout_secs, 60);
    }

    #[test]
    fn test_resolve_endpoints_rejects_bad_url() {
        let mut configs = indexmap::IndexMap::new();
        configs.insert(
            "bad".to_string(),
            CustomEndpointConfig {
                base_url: "http://169.254.169.254/latest".to_string(),
                api_key: None,
                model: None,
                timeout_secs: None,
            },
        );
        assert!(resolve_endpoints(&configs).is_err());
    }
}
