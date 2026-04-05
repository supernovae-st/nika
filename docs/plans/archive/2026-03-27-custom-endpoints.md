# Custom Endpoints (OpenAI-Compatible) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Allow Nika workflows to connect to any OpenAI-compatible inference server (vLLM, TGI, Ollama, LiteLLM, SGLang) via configurable `base_url` endpoints — named in config, inline in YAML, or via environment variables.

**Architecture:** Add an `OpenAiCompat` variant to `RigProvider` backed by `openai::Client::from_url(key, url)` from rig-core 0.32. Provider resolution gains a new code path: check named endpoints in `NikaConfig.endpoints` first, then fall back to the existing catalog. Inline `base_url` on workflow/task creates transient (uncached) providers. The existing SSRF policy is relaxed for endpoints (localhost + RFC-1918 allowed, metadata IPs blocked).

**Tech Stack:** Rust, rig-core 0.32 (`openai::Client` builder), TOML config (`serde`), `marked_yaml` AST, thiserror error codes.

**Key insight:** rig-core 0.32 already supports `openai::Client::from_url(api_key, base_url)` and the OpenAI `from_env()` already reads `OPENAI_BASE_URL`. The feature is wiring this into Nika's AST/config/executor pipeline.

**Crate map:**
- `nika-core` (tools/nika-core/) — AST types, parser, analyzer. Zero I/O, zero engine deps.
- `nika-engine` (tools/nika-engine/) — Provider, executor, runner, config, errors.
- `nika-cli` (tools/nika-cli/) — CLI subcommands.
- `nika` (tools/nika/) — Binary entry point.

**Test command:** `cargo test --workspace --lib` (8400+ tests, safe — no keychain popups). Always use `--lib`.

---

## Phase 1: Data Model (no runtime changes)

### Task 1: Create `provider/endpoints.rs` — endpoint config structs

**Files:**
- Create: `tools/nika-engine/src/provider/endpoints.rs`
- Modify: `tools/nika-engine/src/provider/mod.rs`

**Step 1: Write the test file content**

In `tools/nika-engine/src/provider/endpoints.rs`, write the module with structs and tests:

```rust
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

/// Map of named endpoints (name → resolved config).
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
        other => return Err(format!("Unsupported scheme '{}' — only http/https allowed", other)),
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
                "Blocked metadata endpoint '{}' — SSRF protection",
                h
            ));
        }

        // Block link-local range (169.254.0.0/16) — metadata services hide here
        if let Ok(ip) = h.parse::<std::net::Ipv4Addr>() {
            let octets = ip.octets();
            if octets[0] == 169 && octets[1] == 254 {
                return Err(format!(
                    "Blocked link-local address '{}' — metadata SSRF protection",
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

        // Key: env override → config value → empty string (servers like Ollama need no key)
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
```

**Step 2: Add module to `provider/mod.rs`**

In `tools/nika-engine/src/provider/mod.rs`, add after `pub mod rig;`:

```rust
pub mod endpoints;

// Re-export endpoint types
pub use endpoints::{CustomEndpointConfig, CustomEndpointMap, ResolvedEndpoint};
```

**Step 3: Add `url` dependency to nika-engine**

Check if `url` crate is already a dependency. If not, add it:

Run: `grep 'url' tools/nika-engine/Cargo.toml`

If missing: `cargo add url -p nika-engine`

(Note: `url` is likely already a transitive dep via `reqwest`. Check if direct dep is needed.)

**Step 4: Run tests**

Run: `cargo test -p nika-engine --lib -- endpoints`
Expected: All tests PASS.

**Step 5: Commit**

```bash
git add tools/nika-engine/src/provider/endpoints.rs tools/nika-engine/src/provider/mod.rs
# If Cargo.toml changed: git add tools/nika-engine/Cargo.toml
git commit -m "feat(provider): add CustomEndpointConfig and URL validation for OpenAI-compatible servers

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2: Add `endpoints` field to `NikaConfig`

**Files:**
- Modify: `tools/nika-engine/src/config.rs` (NikaConfig struct at ~line 20)

**Step 1: Add the `endpoints` field to `NikaConfig`**

In `tools/nika-engine/src/config.rs`, add to the `NikaConfig` struct:

```rust
use crate::provider::endpoints::CustomEndpointConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct NikaConfig {
    #[serde(default)]
    pub api_keys: ApiKeys,

    #[serde(default)]
    pub defaults: Defaults,

    /// Named custom endpoints for OpenAI-compatible servers (vLLM, TGI, Ollama, etc.)
    ///
    /// Example:
    /// ```toml
    /// [endpoints.h100]
    /// base_url = "http://10.0.1.42:8000/v1"
    /// api_key = "sk-internal"
    /// model = "Qwen/Qwen3-8B"
    /// ```
    #[serde(default)]
    pub endpoints: indexmap::IndexMap<String, CustomEndpointConfig>,
}
```

**Step 2: Add `resolve_endpoints()` method to `NikaConfig`**

Add this method to the `impl NikaConfig` block:

```rust
/// Resolve all configured endpoints into runtime-ready form.
///
/// Applies env var overrides (NIKA_ENDPOINT_<NAME>_URL, NIKA_ENDPOINT_<NAME>_KEY).
pub fn resolve_endpoints(
    &self,
) -> Result<crate::provider::endpoints::CustomEndpointMap, crate::error::NikaError> {
    crate::provider::endpoints::resolve_endpoints(&self.endpoints).map_err(|e| {
        crate::error::NikaError::ConfigError {
            reason: e,
        }
    })
}
```

**Step 3: Add `indexmap` import if not already present**

Check the existing imports in config.rs. `indexmap` should already be in Cargo.toml deps (used extensively in AST). If the import is missing, add `use indexmap::IndexMap;` or use the full path.

**Step 4: Write tests**

Add to the test module in `config.rs`:

```rust
#[test]
fn test_config_with_endpoints_toml_roundtrip() {
    let toml_str = r#"
[api_keys]
anthropic = "sk-ant-test"

[defaults]
provider = "anthropic"

[endpoints.h100]
base_url = "http://10.0.1.42:8000/v1"
api_key = "sk-internal"
model = "Qwen/Qwen3-8B"
timeout_secs = 60

[endpoints.ollama]
base_url = "http://localhost:11434/v1"
model = "llama3.2"
"#;
    let config: NikaConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.endpoints.len(), 2);
    assert_eq!(config.endpoints["h100"].base_url, "http://10.0.1.42:8000/v1");
    assert_eq!(config.endpoints["h100"].api_key.as_deref(), Some("sk-internal"));
    assert_eq!(config.endpoints["ollama"].api_key, None);
}

#[test]
fn test_config_without_endpoints_backward_compat() {
    let toml_str = r#"
[api_keys]
anthropic = "sk-ant-test"
"#;
    let config: NikaConfig = toml::from_str(toml_str).unwrap();
    assert!(config.endpoints.is_empty()); // #[serde(default)] = empty map
}

#[test]
fn test_resolve_endpoints_from_config() {
    let mut config = NikaConfig::default();
    config.endpoints.insert(
        "test".to_string(),
        crate::provider::endpoints::CustomEndpointConfig {
            base_url: "http://localhost:8000/v1".to_string(),
            api_key: Some("sk-test".to_string()),
            model: None,
            timeout_secs: None,
        },
    );
    let resolved = config.resolve_endpoints().unwrap();
    assert_eq!(resolved["test"].base_url, "http://localhost:8000/v1");
}
```

**Step 5: Run tests**

Run: `cargo test -p nika-engine --lib -- config`
Expected: All tests PASS (existing + new).

**Step 6: Commit**

```bash
git add tools/nika-engine/src/config.rs
git commit -m "feat(config): add endpoints field to NikaConfig for custom OpenAI-compatible servers

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Phase 2: Error Codes

### Task 3: Add NIKA-035 and NIKA-036 error codes

**Files:**
- Modify: `tools/nika-engine/src/error_domains.rs` (~line 40, ProviderError enum)
- Modify: `tools/nika-engine/src/error.rs` (add NikaError variants if missing)

**Step 1: Add variants to `ProviderError`**

In `tools/nika-engine/src/error_domains.rs`, add two new variants to the `ProviderError` enum (after `InvalidConfig`):

```rust
#[error("[NIKA-035] Custom endpoint '{name}' not found in config — add it to [endpoints.{name}] in ~/.config/nika/config.toml")]
EndpointNotFound { name: String },

#[error("[NIKA-036] Cannot connect to custom endpoint '{endpoint}': {reason}")]
EndpointConnectionFailed { endpoint: String, reason: String },
```

**Step 2: Update the `From<ProviderError> for NikaError` impl**

Add the two new match arms to the existing `From` impl:

```rust
ProviderError::EndpointNotFound { name } => NikaError::ProviderNotConfigured {
    provider: format!("custom endpoint '{}'", name),
},
ProviderError::EndpointConnectionFailed { endpoint, reason } => {
    NikaError::ProviderApiError {
        message: format!("Endpoint '{}': {}", endpoint, reason),
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p nika-engine --lib -- error`
Expected: All existing error tests PASS. No new tests needed — these variants are tested via integration in Phase 4.

**Step 4: Commit**

```bash
git add tools/nika-engine/src/error_domains.rs
git commit -m "feat(error): add NIKA-035 (endpoint not found) and NIKA-036 (endpoint connection failed)

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Phase 3: AST Threading (nika-core)

### Task 4: Add `base_url` to Raw AST

**Files:**
- Modify: `tools/nika-core/src/ast/raw/workflow.rs` (~line 14, RawWorkflow struct)
- Modify: `tools/nika-core/src/ast/raw/task.rs` (~line 16, RawTask struct)

**Step 1: Add `base_url` field to `RawWorkflow`**

In `tools/nika-core/src/ast/raw/workflow.rs`, add after the `model` field (~line 26):

```rust
    /// Base URL for OpenAI-compatible endpoint (e.g., vLLM, Ollama).
    /// When set, `provider: openai` will point to this URL instead of api.openai.com.
    pub base_url: Option<Spanned<String>>,
```

**Step 2: Add `base_url` field to `RawTask`**

In `tools/nika-core/src/ast/raw/task.rs`, add after the `model` field (~line 30):

```rust
    /// Task-level base URL override for OpenAI-compatible endpoint.
    /// Takes precedence over workflow-level base_url.
    pub base_url: Option<Spanned<String>>,
```

**Step 3: Run tests (nika-core only)**

Run: `cargo test -p nika-core --lib`
Expected: Some tests may fail due to Default trait or struct initialization. Fix any compilation errors — add `base_url: None,` wherever `RawWorkflow` or `RawTask` is constructed in tests.

**Step 4: Commit**

```bash
git add tools/nika-core/src/ast/raw/workflow.rs tools/nika-core/src/ast/raw/task.rs
git commit -m "feat(ast): add base_url field to RawWorkflow and RawTask

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 5: Parse `base_url` from YAML

**Files:**
- Modify: `tools/nika-core/src/ast/raw/parser.rs`

**Step 1: Find the workflow parsing section**

Search for where `provider` and `model` are parsed from the YAML map. You'll see:
```rust
provider: get_string_field(file, map, "provider")?,
model: get_string_field(file, map, "model")?,
```

Add immediately after `model`:
```rust
base_url: get_string_field(file, map, "base_url")?,
```

**Step 2: Find the task parsing section**

Search for where task `provider` and `model` are parsed. You'll see:
```rust
provider: get_string_field(file, task_map, "provider")?,
model: get_string_field(file, task_map, "model")?,
```

Add immediately after `model`:
```rust
base_url: get_string_field(file, task_map, "base_url")?,
```

**Step 3: Update known-field validation**

The parser has an unknown-field checker. Search for `"provider"` in the known fields list and add `"base_url"` next to it. There should be something like:
```rust
let known_fields = &["id", "description", "provider", "model", ...];
```
Add `"base_url"` to this list for BOTH workflow-level and task-level known fields.

**Step 4: Run tests**

Run: `cargo test -p nika-core --lib -- parser`
Expected: PASS. The parser already handles `Option<Spanned<String>>` gracefully — missing YAML fields produce `None`.

**Step 5: Commit**

```bash
git add tools/nika-core/src/ast/raw/parser.rs
git commit -m "feat(parser): parse base_url field from workflow and task YAML

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 6: Add `base_url` to Analyzed AST + Analyzer

**Files:**
- Modify: `tools/nika-core/src/ast/analyzed/workflow.rs` (~line 25, AnalyzedWorkflow)
- Modify: `tools/nika-core/src/ast/analyzed/task.rs` (~line 21, AnalyzedTask)
- Modify: `tools/nika-core/src/ast/analyzer/analyze.rs`

**Step 1: Add to `AnalyzedWorkflow`**

After `pub model: Option<String>`:
```rust
    /// Base URL for OpenAI-compatible endpoint override
    pub base_url: Option<String>,
```

**Step 2: Add to `AnalyzedTask`**

After `pub model: Option<String>`:
```rust
    /// Task-level base URL override for OpenAI-compatible endpoint
    pub base_url: Option<String>,
```

**Step 3: Thread through analyzer**

In `analyze.rs`, find where `AnalyzedWorkflow` is constructed (search for `provider: raw.provider`). Add:
```rust
base_url: raw.base_url.as_ref().map(|s| s.value.clone()),
```

Find where `AnalyzedTask` is constructed (search for `provider: raw_task.provider`). Add:
```rust
base_url: raw_task.base_url.as_ref().map(|s| s.value.clone()),
```

**Step 4: Fix any compilation errors**

Search for any place that constructs `AnalyzedWorkflow` or `AnalyzedTask` in test code and add `base_url: None,`.

Run: `cargo test -p nika-core --lib`
Expected: PASS.

**Step 5: Commit**

```bash
git add tools/nika-core/src/ast/analyzed/workflow.rs tools/nika-core/src/ast/analyzed/task.rs tools/nika-core/src/ast/analyzer/analyze.rs
git commit -m "feat(analyzer): thread base_url from Raw to Analyzed AST

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 7: Add `base_url` to Lower phase (InferParams + AgentParams)

**Files:**
- Modify: `tools/nika-engine/src/ast/lower.rs` (InferParams construction)
- Search for: `InferParams` and `AgentParams` struct definitions (could be in `nika-core` or `nika-engine`)

**Step 1: Find the InferParams struct**

Run: `grep -rn "pub struct InferParams" tools/` to locate it. Add:
```rust
    /// Base URL for OpenAI-compatible endpoint override.
    /// Resolved precedence: task base_url > workflow base_url > config endpoint > env var.
    pub base_url: Option<String>,
```

**Step 2: Find the AgentParams struct**

Run: `grep -rn "pub struct AgentParams" tools/` to locate it. Add:
```rust
    /// Base URL for OpenAI-compatible endpoint override.
    pub base_url: Option<String>,
```

**Step 3: Thread in `lower.rs`**

In the `lower_infer()` function, find where `InferParams` is constructed. Add the `base_url` field:
```rust
base_url: analyzed_task.base_url.clone(),
```

Same for the agent lowering function — thread `base_url` from `AnalyzedTask` into `AgentParams`.

**Important:** The lower function receives the `AnalyzedTask` which has both task-level and workflow-level context. The task's `base_url` should take precedence. If the lower function only sees action-level data, you may need to pass the workflow's `base_url` as a fallback parameter. Check how `provider` is threaded — follow the same pattern.

**Step 4: Fix compilation errors**

Search for all places that construct `InferParams` and `AgentParams` in tests. Add `base_url: None,`.

Run: `cargo test --workspace --lib`
Expected: All 8400+ tests PASS.

**Step 5: Commit**

```bash
git add -A  # Multiple files likely touched
git commit -m "feat(ast): thread base_url through lower phase into InferParams and AgentParams

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Phase 4: Provider Variant + Executor Wiring

### Task 8: Add `OpenAiCompat` variant to `RigProvider`

**Files:**
- Modify: `tools/nika-engine/src/provider/rig.rs`

**Step 1: Add the variant to the enum** (~line 164)

After the `XAi(xai::Client)` variant:

```rust
    /// OpenAI-compatible endpoint (vLLM, TGI, Ollama, LiteLLM, SGLang).
    /// Uses openai::Client pointed at a custom base URL.
    OpenAiCompat {
        client: openai::Client,
        /// Display name for events/errors (e.g., "h100", "ollama")
        endpoint_name: String,
        /// Default model for this endpoint
        default_model: Option<String>,
    },
```

**Step 2: Update `name()` method** (~line 364)

Add match arm:
```rust
RigProvider::OpenAiCompat { endpoint_name, .. } => {
    // SAFETY: This leaks a string per unique endpoint name.
    // Acceptable: bounded by number of config entries (typically 1-5).
    Box::leak(endpoint_name.clone().into_boxed_str())
}
```

Note: The return type is `&'static str`. For a non-static name, you have two options:
1. Leak the string (simple, bounded by # of endpoints)
2. Change the return type to `Cow<'static, str>` (cleaner but touches more code)

Check the call sites of `name()`. If they only use it for display/logging, leaking is fine. If they need `&'static str` for lifetime reasons, leaking is the pragmatic choice.

**Step 3: Update `default_model()` method** (~line 389)

Add match arm:
```rust
RigProvider::OpenAiCompat { default_model, .. } => {
    default_model.as_deref().unwrap_or("gpt-3.5-turbo")
}
```

Wait — `default_model()` returns `&'static str`. If `default_model` is `Option<String>`, we can't return a reference to it. Options:
1. Leak the string (same as name)
2. Change return type to `Cow<'_, str>`
3. Store as `Option<&'static str>` (requires leaking at construction)

Simplest: leak at construction time. In the `OpenAiCompat` variant, store `default_model: &'static str`. When constructing, use `Box::leak()`.

Alternatively — and better for this case — just return the fallback `"gpt-3.5-turbo"` since the model is always specified explicitly in the workflow YAML when using custom endpoints. The default is only a safety net.

**Step 4: Update ALL match blocks in `RigProvider`**

Every `match self { ... }` in rig.rs needs an `OpenAiCompat` arm. Search for `RigProvider::XAi` to find all match blocks. For each one, add an arm that delegates to the `openai::Client` inside:

For `infer()`:
```rust
RigProvider::OpenAiCompat { client, .. } => {
    let agent = client.agent(model_id).max_tokens(8192).build();
    timeout(INFER_TIMEOUT, agent.prompt(prompt))
        .await
        .map_err(|_| RigInferError::Timeout { duration_ms: INFER_TIMEOUT.as_millis() as u64 })?
        .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
}
```

This is identical to the `OpenAI` arm. For all methods (`infer`, `infer_with_options`, `infer_vision`, `stream_infer`, `stream_vision`, etc.), copy the `OpenAI` arm's code.

**Tip:** There may be 8-15 match blocks. Use search `RigProvider::XAi` to find them all. Each OpenAiCompat arm is a copy of the OpenAI arm but destructures `client` from the struct variant instead.

**Step 5: Add constructor**

Add a new method to `impl RigProvider`:

```rust
/// Create an OpenAI-compatible provider pointed at a custom base URL.
///
/// Used for vLLM, TGI, Ollama, LiteLLM, SGLang, and any OpenAI-compatible server.
pub fn openai_compat(
    endpoint_name: &str,
    base_url: &str,
    api_key: &str,
    default_model: Option<&str>,
) -> Result<Self, crate::error::NikaError> {
    use crate::provider::endpoints::validate_endpoint_url;
    validate_endpoint_url(base_url).map_err(|e| {
        crate::error_domains::ProviderError::InvalidConfig { message: e }
    })?;

    let client = openai::Client::from_url(api_key, base_url);
    Ok(RigProvider::OpenAiCompat {
        client,
        endpoint_name: endpoint_name.to_string(),
        default_model: default_model.map(|s| s.to_string()),
    })
}
```

**Step 6: Run tests**

Run: `cargo test -p nika-engine --lib -- provider`
Expected: PASS. Fix any missing match arms.

**Step 7: Commit**

```bash
git add tools/nika-engine/src/provider/rig.rs
git commit -m "feat(provider): add OpenAiCompat variant to RigProvider for custom endpoints

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 9: Add `from_name_with_endpoints()` provider resolution

**Files:**
- Modify: `tools/nika-engine/src/provider/rig.rs`

**Step 1: Add the new constructor**

Add to `impl RigProvider`, after `from_name()`:

```rust
/// Resolve a provider name, checking custom endpoints first, then falling back to catalog.
///
/// Resolution order:
/// 1. Named custom endpoint from config (e.g., "h100" → endpoints["h100"])
/// 2. Catalog provider (e.g., "openai" → standard OpenAI API)
///
/// Special case: if name is "openai" and OPENAI_BASE_URL is set,
/// rig-core's `from_env()` already handles it — no extra logic needed.
pub fn from_name_with_endpoints(
    name: &str,
    endpoints: &crate::provider::endpoints::CustomEndpointMap,
) -> Result<Self, crate::error::NikaError> {
    // 1. Check custom endpoints first
    if let Some(ep) = endpoints.get(name) {
        return Self::openai_compat(
            name,
            &ep.base_url,
            &ep.api_key,
            ep.default_model.as_deref(),
        );
    }

    // 2. Fall back to catalog provider
    Self::from_name(name)
}
```

**Step 2: Write tests**

```rust
#[cfg(test)]
mod endpoint_resolution_tests {
    use super::*;
    use crate::provider::endpoints::{CustomEndpointMap, ResolvedEndpoint};

    #[test]
    fn test_from_name_with_endpoints_custom() {
        let mut endpoints = CustomEndpointMap::new();
        endpoints.insert("local".to_string(), ResolvedEndpoint {
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: "ollama".to_string(),
            default_model: Some("llama3.2".to_string()),
            timeout_secs: 300,
        });

        let provider = RigProvider::from_name_with_endpoints("local", &endpoints).unwrap();
        assert!(matches!(provider, RigProvider::OpenAiCompat { .. }));
    }

    #[test]
    fn test_from_name_with_endpoints_fallback_to_catalog() {
        let endpoints = CustomEndpointMap::new(); // empty
        // This will fail because no ANTHROPIC_API_KEY is set in test env
        // But it should NOT return EndpointNotFound — it should hit the catalog path
        let result = RigProvider::from_name_with_endpoints("anthropic", &endpoints);
        // Expect MissingApiKey (NIKA-032), NOT EndpointNotFound (NIKA-035)
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("NIKA-032") || err_msg.contains("API key"));
    }

    #[test]
    fn test_from_name_with_endpoints_unknown() {
        let endpoints = CustomEndpointMap::new();
        let result = RigProvider::from_name_with_endpoints("nonexistent", &endpoints);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("NIKA-030")); // Not configured
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p nika-engine --lib -- endpoint_resolution`
Expected: PASS.

**Step 4: Commit**

```bash
git add tools/nika-engine/src/provider/rig.rs
git commit -m "feat(provider): add from_name_with_endpoints() for custom endpoint resolution

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 10: Wire endpoints into `TaskExecutor`

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/mod.rs`

**Step 1: Add endpoint map field to `TaskExecutor`** (~line 59)

Add to the struct:
```rust
    /// Custom endpoints for OpenAI-compatible servers (vLLM, TGI, Ollama)
    custom_endpoints: Arc<crate::provider::endpoints::CustomEndpointMap>,
```

**Step 2: Update constructors**

In `with_policy()` (~line 112), add a parameter:

```rust
pub fn with_policy(
    provider: &str,
    model: Option<&str>,
    mcp_configs: Option<FxHashMap<String, McpConfigInline>>,
    event_log: EventLog,
    policy_config: Option<PolicyConfig>,
    permission_mode: Option<PermissionMode>,
    custom_endpoints: Option<crate::provider::endpoints::CustomEndpointMap>,  // NEW
) -> Result<Self, NikaError> {
```

In the `Ok(Self { ... })` block (~line 176), add:
```rust
custom_endpoints: Arc::new(custom_endpoints.unwrap_or_default()),
```

Update `new()` to pass `None` for the new parameter:
```rust
pub fn new(...) -> Result<Self, NikaError> {
    Self::with_policy(provider, model, mcp_configs, event_log, None, None, None)
}
```

**Step 3: Update `get_rig_provider()`** (~line 379)

Replace the existing `get_rig_provider()`:

```rust
pub(super) fn get_rig_provider(&self, name: &str) -> Result<RigProvider, NikaError> {
    use dashmap::mapref::entry::Entry;

    // Check custom endpoints first — they don't alias through the catalog
    if self.custom_endpoints.contains_key(name) {
        // Custom endpoints are cached under their exact name
        match self.rig_provider_cache.entry(name.to_string()) {
            Entry::Occupied(e) => return Ok(e.get().clone()),
            Entry::Vacant(e) => {
                let provider =
                    RigProvider::from_name_with_endpoints(name, &self.custom_endpoints)?;
                e.insert(provider.clone());
                self.event_log.emit(EventKind::ProviderInitialized {
                    provider: name.to_string(),
                    model: provider.default_model().to_string(),
                    cached: false,
                });
                return Ok(provider);
            }
        }
    }

    // Catalog providers — normalize alias to canonical name for cache key
    let canonical = crate::core::find_provider(name)
        .map(|p| p.id)
        .unwrap_or(name);

    match self.rig_provider_cache.entry(canonical.to_string()) {
        Entry::Occupied(e) => Ok(e.get().clone()),
        Entry::Vacant(e) => {
            let provider = RigProvider::from_name(name)?;
            e.insert(provider.clone());
            self.event_log.emit(EventKind::ProviderInitialized {
                provider: canonical.to_string(),
                model: provider.default_model().to_string(),
                cached: false,
            });
            Ok(provider)
        }
    }
}
```

**Step 4: Fix all call sites of `with_policy()`**

Search for `with_policy(` in the codebase. Every call site needs the new `custom_endpoints` parameter (pass `None` for now — wired in Task 12).

Run: `grep -rn "with_policy(" tools/`

**Step 5: Run tests**

Run: `cargo test --workspace --lib`
Expected: PASS (all 8400+). The new parameter defaults to `None`/empty map, so behavior is unchanged.

**Step 6: Commit**

```bash
git add tools/nika-engine/src/runtime/executor/mod.rs
# Add any other files touched (call sites of with_policy)
git commit -m "feat(executor): wire CustomEndpointMap into TaskExecutor and get_rig_provider

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 11: Handle inline `base_url` in infer executor

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs` (~line 129)

**Step 1: Add inline base_url resolution before provider lookup**

After `let provider_name = infer.provider.as_deref().unwrap_or(&self.default_provider);` (line 129) and BEFORE the mock check (line 133), add:

```rust
// If task has an inline base_url, create a transient OpenAI-compat provider
// (not cached — inline URLs are one-off overrides)
if let Some(ref base_url) = infer.base_url {
    if provider_name != "mock" {
        let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "ollama".to_string());
        let provider = RigProvider::openai_compat(
            &format!("inline:{}", provider_name),
            base_url,
            &api_key,
            infer.model.as_deref(),
        )?;

        let model = infer.model.as_deref().or(self.default_model.as_deref());

        self.event_log.emit(EventKind::ProviderCalled {
            task_id: Arc::clone(task_id),
            provider: format!("{}@{}", provider_name, base_url),
            model: model.unwrap_or_else(|| provider.default_model()).to_string(),
            prompt_len: prompt.len(),
        });

        // Skip the normal provider resolution path — use inline provider directly
        // (rest of the infer logic continues with this provider)
        // Note: This means vision dispatch, structured output, and streaming
        // all work with the inline provider. Delegate to a helper or restructure
        // the flow so `provider` is set early and the rest of the function uses it.
    }
}
```

**Important architectural note:** The `run_infer` method is long (~400 lines) with vision dispatch, structured output layers, and streaming. Rather than duplicating all that logic, the cleanest approach is to resolve the `provider` variable early (before line 213 where `get_rig_provider` is called) and let the rest of the function use it:

```rust
// Resolve provider: inline base_url → cached endpoint → catalog
let provider = if let Some(ref base_url) = infer.base_url {
    // Transient provider — not cached
    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "ollama".to_string());
    RigProvider::openai_compat(
        &format!("{}@inline", provider_name),
        base_url,
        &api_key,
        infer.model.as_deref(),
    )?
} else {
    self.get_rig_provider(provider_name)?
};
```

Replace line 213 (`let provider = self.get_rig_provider(provider_name)?;`) with this block.

**Step 2: Run tests**

Run: `cargo test -p nika-engine --lib -- infer`
Expected: PASS. No behavior change for existing workflows (base_url is None).

**Step 3: Commit**

```bash
git add tools/nika-engine/src/runtime/executor/infer.rs
git commit -m "feat(infer): resolve inline base_url to transient OpenAiCompat provider

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 12: Handle inline `base_url` in agent executor + wire runner

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/agent.rs` (~line 118)
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs` (~line 443)
- Modify: `tools/nika-engine/src/runtime/runner.rs`

**Step 1: Update agent executor**

In `agent.rs`, where `provider_name` is resolved (~line 118), add inline base_url handling. The agent loop creates its own client internally (`openai::Client::from_env()`), so the approach is different:

If `resolved_agent.base_url` is `Some`, set the `OPENAI_BASE_URL` env var temporarily before creating the agent loop. Or better — pass the base_url into `RigAgentLoop` so it can use `openai::Client::from_url()` instead of `from_env()`.

Add `base_url: Option<String>` field to `RigAgentLoop` struct:

In `rig_agent_loop/mod.rs` (or wherever the struct is defined):
```rust
pub struct RigAgentLoop {
    // ... existing fields ...
    /// Custom base URL for OpenAI-compatible endpoints
    pub base_url: Option<String>,
}
```

Thread it from `AgentParams.base_url` through the `RigAgentLoop::new()` constructor.

**Step 2: Update `run_openai()` in providers.rs** (~line 443)

Replace:
```rust
let client = openai::Client::from_env();
```

With:
```rust
let client = if let Some(ref url) = self.base_url {
    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "ollama".to_string());
    openai::Client::from_url(&api_key, url)
} else {
    openai::Client::from_env()
};
```

**Step 3: Wire config loading in runner.rs**

In `runner.rs`, find where `TaskExecutor::with_policy()` is called. Before that call, load the config and resolve endpoints:

```rust
// Load custom endpoints from config
let config = crate::config::NikaConfig::load()
    .unwrap_or_default()
    .with_env();
let custom_endpoints = config.resolve_endpoints().ok();
```

Pass `custom_endpoints` as the new parameter to `TaskExecutor::with_policy()`.

**Step 4: Run full test suite**

Run: `cargo test --workspace --lib`
Expected: All 8400+ tests PASS.

**Step 5: Commit**

```bash
git add tools/nika-engine/src/runtime/executor/agent.rs tools/nika-engine/src/runtime/rig_agent_loop/ tools/nika-engine/src/runtime/runner.rs
git commit -m "feat(agent): support base_url in agent loop + wire config endpoints into runner

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Phase 5: CLI Surface

### Task 13: Update `nika provider list` to show custom endpoints

**Files:**
- Modify: `tools/nika-cli/src/provider.rs` (or wherever `ProviderAction::List` is handled)

**Step 1: After displaying LLM providers, add an endpoints section**

After the existing provider list loop, add:

```rust
// Show custom endpoints from config
let config = nika_engine::config::NikaConfig::load()
    .unwrap_or_default()
    .with_env();
if !config.endpoints.is_empty() {
    println!();
    println!(
        "  {} ({})",
        "Custom Endpoints".bold(),
        format!("{} configured", config.endpoints.len()).cyan()
    );
    println!("{}", nika::display::separator(50));
    println!();
    for (i, (name, ep)) in config.endpoints.iter().enumerate() {
        let is_last = i == config.endpoints.len() - 1;
        let connector = tree_connector(is_last).dimmed();
        let model_info = ep
            .model
            .as_deref()
            .map(|m| format!(" model={}", m))
            .unwrap_or_default();
        let key_info = if ep.api_key.is_some() {
            "[key set]"
        } else {
            "[no auth]"
        };
        println!(
            "  {} {} {:12} {} {}{}",
            connector,
            StatusIcon::Ok,
            name,
            ep.base_url.dimmed(),
            key_info.dimmed(),
            model_info.dimmed(),
        );
    }
    println!();
    println!(
        "{}",
        hint("Add endpoints in ~/.config/nika/config.toml under [endpoints.<name>]")
    );
}
```

**Step 2: Run manually to verify**

Run: `cargo run -- provider list`
Expected: Shows existing providers + any configured endpoints.

**Step 3: Commit**

```bash
git add tools/nika-cli/src/provider.rs
git commit -m "feat(cli): show custom endpoints in nika provider list

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Phase 6: Integration Test

### Task 14: Write an end-to-end test with mock endpoint

**Files:**
- Create: a test in `tools/nika-engine/src/provider/endpoints.rs` (or a new integration test)

**Step 1: Write a test that wires the full pipeline**

Add to the test module in `endpoints.rs`:

```rust
#[test]
fn test_full_endpoint_config_to_provider_resolution() {
    // Simulate config.toml with a custom endpoint
    let mut configs = indexmap::IndexMap::new();
    configs.insert(
        "vllm".to_string(),
        CustomEndpointConfig {
            base_url: "http://localhost:8000/v1".to_string(),
            api_key: Some("sk-test-key".to_string()),
            model: Some("Qwen/Qwen3-8B".to_string()),
            timeout_secs: Some(60),
        },
    );

    // Resolve endpoints
    let resolved = resolve_endpoints(&configs).unwrap();
    assert_eq!(resolved.len(), 1);

    // Create provider from resolved endpoint
    let provider = crate::provider::rig::RigProvider::from_name_with_endpoints(
        "vllm",
        &resolved,
    )
    .unwrap();

    // Verify it's an OpenAiCompat variant
    assert!(matches!(provider, crate::provider::rig::RigProvider::OpenAiCompat { .. }));

    // Verify unknown name falls back to catalog (and fails due to missing API key)
    let result = crate::provider::rig::RigProvider::from_name_with_endpoints(
        "anthropic",
        &resolved,
    );
    assert!(result.is_err()); // Missing API key in test env
}
```

**Step 2: Run tests**

Run: `cargo test --workspace --lib`
Expected: All PASS.

**Step 3: Commit**

```bash
git add tools/nika-engine/src/provider/endpoints.rs
git commit -m "test(provider): add end-to-end test for custom endpoint → provider resolution

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Phase 7: Documentation

### Task 15: Update CLAUDE.md and rules

**Files:**
- Modify: `tools/nika/CLAUDE.md`

**Step 1: Add custom endpoints section**

Add a new section after the "Conventions" section:

```markdown
## Custom Endpoints (OpenAI-Compatible)

Nika supports connecting to any OpenAI-compatible inference server (vLLM, TGI, Ollama, LiteLLM, SGLang).

### Configuration (config.toml)

```toml
# ~/.config/nika/config.toml
[endpoints.h100]
base_url = "http://10.0.1.42:8000/v1"
api_key = "sk-internal-token"
model = "Qwen/Qwen3-8B"
timeout_secs = 60

[endpoints.ollama]
base_url = "http://localhost:11434/v1"
model = "llama3.2"
```

### Usage in workflows

```yaml
schema: "nika/workflow@0.12"
provider: h100          # Named endpoint from config.toml
model: Qwen/Qwen3-8B

tasks:
  - id: generate
    infer: "Hello from vLLM"
```

### Inline base_url (one-off)

```yaml
- id: local
  provider: openai
  base_url: "http://localhost:11434/v1"
  model: llama3.2
  infer: "Hello from Ollama"
```

### Environment variable

```bash
export OPENAI_BASE_URL="http://localhost:8000/v1"
# All provider: openai tasks now hit this URL
```

### Env var overrides for named endpoints

```bash
export NIKA_ENDPOINT_H100_URL="http://new-server:8000/v1"
export NIKA_ENDPOINT_H100_KEY="sk-new-key"
```
```

**Step 2: Commit**

```bash
git add tools/nika/CLAUDE.md
git commit -m "docs(nika): add custom endpoints documentation

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Verification Checklist

After all tasks are complete:

- [ ] `cargo test --workspace --lib` — all 8400+ tests pass
- [ ] `cargo clippy --workspace -- -D warnings` — zero warnings
- [ ] `cargo run -- provider list` — shows custom endpoints section (if any configured)
- [ ] Existing workflows unchanged — no behavior change when no `base_url` or `endpoints` are configured
- [ ] TOML config backward compatible — old config.toml without `[endpoints]` still loads

## YAML Spec Summary (user-facing)

```yaml
# Option 1: Named endpoint (config.toml)
provider: h100

# Option 2: Inline base_url (workflow header)
provider: openai
base_url: "http://localhost:11434/v1"

# Option 3: Inline base_url (per task)
- id: local
  provider: openai
  base_url: "http://localhost:11434/v1"
  infer: "..."

# Option 4: ENV only (no YAML change)
# OPENAI_BASE_URL=http://localhost:11434/v1
provider: openai
```
