//! IPC protocol — message types and wire format.
//!
//! Wire format: `[4-byte big-endian length][JSON payload]`
//!
//! All messages are length-prefixed JSON. The length prefix is a u32 in
//! big-endian byte order, giving a maximum message size of ~4 GB (but we
//! cap at 16 MB for safety).

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{DaemonError, DaemonResult};

/// Maximum message size: 16 MB.
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Length prefix size in bytes (u32 big-endian).
pub const LENGTH_PREFIX_SIZE: usize = 4;

// ═══════════════════════════════════════════════════════════════════════════
// REQUEST
// ═══════════════════════════════════════════════════════════════════════════

/// A request from a client to the daemon.
///
/// # Debug
/// The `Debug` impl redacts `SetSecret.key` and `auth_token` fields to prevent
/// API key leakage in tracing output.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum DaemonRequest {
    // ── Health ───────────────────────────────────────────────────────────
    /// Ping the daemon (health check).
    Ping,

    /// Get daemon status (PID, uptime, services).
    Status,

    // ── Secrets ──────────────────────────────────────────────────────────
    /// Get a secret (API key) for a provider.
    /// Requires auth token (same as SetSecret) to prevent same-UID exfiltration.
    GetSecret {
        provider: String,
        auth_token: Option<String>,
    },

    /// Check if a secret exists for a provider.
    HasSecret { provider: String },

    /// List all provider secret status.
    ListSecrets,

    /// Store a secret (API key) for a provider in the encrypted vault.
    SetSecret {
        provider: String,
        key: String,
        /// Auth token for write operations (read from `~/.nika/daemon/.token`).
        auth_token: Option<String>,
    },

    /// Delete a secret (API key) for a provider from the encrypted vault.
    DeleteSecret {
        provider: String,
        /// Auth token for write operations.
        auth_token: Option<String>,
    },

    // ── Jobs ──────────────────────────────────────────────────────────────
    /// Submit a new job.
    JobSubmit {
        workflow: String,
        name: Option<String>,
        args: Option<String>,
        cron: Option<String>,
        max_retries: Option<u32>,
    },

    /// List jobs, optionally filtered by state.
    JobList { state: Option<String> },

    /// Get job status/details.
    JobStatus { id: String },

    /// Cancel a running job.
    JobCancel { id: String },

    /// Retry a failed job.
    JobRetry { id: String },

    /// Get job history events.
    JobHistory { id: String },
    // ── Watch ─────────────────────────────────────────────────────────────
    /// Start watching a directory for workflow changes.
    WatchStart { dir: String, patterns: Vec<String> },

    /// Stop watching.
    WatchStop,

    /// Get watch status.
    WatchStatus,

    // ── Cache ────────────────────────────────────────────────────────────
    /// Get a cached LLM response.
    CacheGet { key: String },

    /// Store a response in the cache.
    CacheSet {
        key: String,
        provider: String,
        model: String,
        response: String,
        tokens_in: u64,
        tokens_out: u64,
        cost: f64,
        ttl_secs: Option<u64>,
    },

    /// Clear all cache entries.
    CacheClear,

    /// Get cache statistics.
    CacheStats,

    // ── Events ───────────────────────────────────────────────────────────
    /// Subscribe to daemon events (streaming).
    EventSubscribe,

    // ── LSP Queries ─────────────────────────────────────────────────────
    /// List all known providers with API key status (for LSP completions).
    ListProviderStatus,

    /// Estimate cost for a model invocation (for LSP inlay hints).
    EstimateCost {
        provider: String,
        model: String,
        input_tokens: u64,
        output_tokens: u64,
    },

    /// Get recent runs for a workflow file (for LSP hover/code lens).
    GetWorkflowHistory { workflow: String },

    /// Get daemon capabilities and stats (for LSP status bar).
    GetDaemonCapabilities,

    // ── Lifecycle ─────────────────────────────────────────────────────────
    /// Request graceful daemon shutdown.
    /// Requires auth token to prevent any local process from killing the daemon.
    Shutdown {
        /// Auth token for shutdown (read from `~/.nika/daemon/.token`).
        auth_token: Option<String>,
    },
}

impl std::fmt::Debug for DaemonRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redact sensitive fields (API keys, auth tokens) to prevent log leakage.
        match self {
            Self::SetSecret { provider, .. } => {
                write!(f, "DaemonRequest::SetSecret {{ provider: {provider:?}, key: <redacted>, auth_token: <redacted> }}")
            }
            Self::DeleteSecret { provider, .. } => {
                write!(f, "DaemonRequest::DeleteSecret {{ provider: {provider:?}, auth_token: <redacted> }}")
            }
            Self::GetSecret { provider, .. } => {
                write!(
                    f,
                    "DaemonRequest::GetSecret {{ provider: {provider:?}, auth_token: <redacted> }}"
                )
            }
            Self::Ping => write!(f, "DaemonRequest::Ping"),
            Self::Status => write!(f, "DaemonRequest::Status"),
            Self::HasSecret { provider } => {
                write!(f, "DaemonRequest::HasSecret {{ provider: {provider:?} }}")
            }
            Self::ListSecrets => write!(f, "DaemonRequest::ListSecrets"),
            Self::Shutdown { .. } => {
                write!(f, "DaemonRequest::Shutdown {{ auth_token: <redacted> }}")
            }
            // All other variants use type-tag only to keep debug output manageable
            Self::JobSubmit { workflow, .. } => write!(
                f,
                "DaemonRequest::JobSubmit {{ workflow: {workflow:?}, .. }}"
            ),
            Self::JobList { .. } => write!(f, "DaemonRequest::JobList"),
            Self::JobStatus { id } => write!(f, "DaemonRequest::JobStatus {{ id: {id:?} }}"),
            Self::JobCancel { id } => write!(f, "DaemonRequest::JobCancel {{ id: {id:?} }}"),
            Self::JobRetry { id } => write!(f, "DaemonRequest::JobRetry {{ id: {id:?} }}"),
            Self::JobHistory { id } => write!(f, "DaemonRequest::JobHistory {{ id: {id:?} }}"),
            Self::WatchStart { dir, .. } => {
                write!(f, "DaemonRequest::WatchStart {{ dir: {dir:?}, .. }}")
            }
            Self::WatchStop => write!(f, "DaemonRequest::WatchStop"),
            Self::WatchStatus => write!(f, "DaemonRequest::WatchStatus"),
            Self::CacheGet { .. } => write!(f, "DaemonRequest::CacheGet"),
            Self::CacheSet { .. } => write!(f, "DaemonRequest::CacheSet"),
            Self::CacheClear => write!(f, "DaemonRequest::CacheClear"),
            Self::CacheStats => write!(f, "DaemonRequest::CacheStats"),
            Self::EventSubscribe => write!(f, "DaemonRequest::EventSubscribe"),
            Self::ListProviderStatus => write!(f, "DaemonRequest::ListProviderStatus"),
            Self::EstimateCost { model, .. } => {
                write!(f, "DaemonRequest::EstimateCost {{ model: {model:?}, .. }}")
            }
            Self::GetWorkflowHistory { workflow } => {
                write!(
                    f,
                    "DaemonRequest::GetWorkflowHistory {{ workflow: {workflow:?} }}"
                )
            }
            Self::GetDaemonCapabilities => write!(f, "DaemonRequest::GetDaemonCapabilities"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RESPONSE
// ═══════════════════════════════════════════════════════════════════════════

/// A response from the daemon to a client.
///
/// # Debug
/// The `Debug` impl redacts `Secret { value }` to prevent API key leakage
/// in tracing output. All other variants print their type tag only.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum DaemonResponse {
    /// Generic success (no payload).
    Ok,

    /// Error response with code and message.
    Error {
        code: String,
        message: String,
    },

    /// Pong response with daemon info.
    Pong {
        version: String,
        uptime_secs: u64,
    },

    /// Daemon status info.
    StatusInfo {
        pid: u32,
        uptime_secs: u64,
        services: Vec<String>,
    },

    /// Secret value (None if not found).
    Secret {
        value: Option<String>,
    },

    /// Whether a secret exists.
    SecretExists {
        exists: bool,
    },

    /// List of provider secret info.
    SecretList {
        providers: Vec<ProviderSecretInfo>,
    },

    /// Secret stored successfully.
    SecretStored,

    /// Secret deleted successfully.
    SecretDeleted,

    /// Authentication required for this operation.
    AuthRequired,

    // ── Jobs ──────────────────────────────────────────────────────────────
    /// Job created successfully.
    JobCreated {
        id: String,
    },

    /// List of jobs.
    JobList {
        jobs: Vec<serde_json::Value>,
    },

    /// Single job details.
    JobDetail {
        job: serde_json::Value,
    },

    /// Job history events.
    JobHistoryList {
        events: Vec<serde_json::Value>,
    },

    // ── Watch ──────────────────────────────────────────────────────────────
    WatchActive {
        dir: String,
        patterns: Vec<String>,
    },
    WatchInactive,

    // ── Cache ──────────────────────────────────────────────────────────────
    CacheHit {
        response: String,
    },
    CacheMiss,
    CacheStatsResult {
        entries: usize,
        hits: u64,
        misses: u64,
        evictions: u64,
        total_tokens_saved: u64,
        total_cost_saved: f64,
    },

    // ── Events ─────────────────────────────────────────────────────────────
    Event {
        event: serde_json::Value,
    },

    // ── LSP Queries ─────────────────────────────────────────────────────
    /// List of provider statuses with key availability.
    ProviderStatusList {
        providers: Vec<nika_core::catalogs::ProviderStatusInfo>,
    },

    /// Cost estimate result.
    CostEstimateResult {
        estimate: nika_core::catalogs::CostEstimate,
    },

    /// Workflow run history.
    WorkflowHistoryResult {
        runs: Vec<nika_core::catalogs::WorkflowRunInfo>,
    },

    /// Daemon capabilities and stats.
    DaemonCapabilitiesResult {
        capabilities: nika_core::catalogs::DaemonCapabilities,
    },

    // ── Lifecycle ─────────────────────────────────────────────────────────
    /// Acknowledgement that daemon is shutting down.
    ShuttingDown,
}

impl std::fmt::Debug for DaemonResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Type-tag only to prevent any future debug!(?response) from leaking API keys.
        // Secret variant is explicitly redacted.
        let tag = match self {
            Self::Ok => "Ok",
            Self::Error { .. } => "Error",
            Self::Pong { .. } => "Pong",
            Self::StatusInfo { .. } => "StatusInfo",
            Self::Secret { .. } => "Secret(<redacted>)",
            Self::SecretExists { .. } => "SecretExists",
            Self::SecretList { .. } => "SecretList",
            Self::SecretStored => "SecretStored",
            Self::SecretDeleted => "SecretDeleted",
            Self::AuthRequired => "AuthRequired",
            Self::JobCreated { .. } => "JobCreated",
            Self::JobList { .. } => "JobList",
            Self::JobDetail { .. } => "JobDetail",
            Self::JobHistoryList { .. } => "JobHistoryList",
            Self::WatchActive { .. } => "WatchActive",
            Self::WatchInactive => "WatchInactive",
            Self::CacheHit { .. } => "CacheHit",
            Self::CacheMiss => "CacheMiss",
            Self::CacheStatsResult { .. } => "CacheStatsResult",
            Self::Event { .. } => "Event",
            Self::ProviderStatusList { .. } => "ProviderStatusList",
            Self::CostEstimateResult { .. } => "CostEstimateResult",
            Self::WorkflowHistoryResult { .. } => "WorkflowHistoryResult",
            Self::DaemonCapabilitiesResult { .. } => "DaemonCapabilitiesResult",
            Self::ShuttingDown => "ShuttingDown",
        };
        write!(f, "DaemonResponse::{tag}")
    }
}

/// Information about a provider's secret status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderSecretInfo {
    pub provider: String,
    pub source: SecretSource,
}

/// Where a secret was found.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecretSource {
    /// From an environment variable.
    Env,
    /// From the encrypted vault.
    Vault,
    /// Not found.
    NotFound,
}

// ═══════════════════════════════════════════════════════════════════════════
// WIRE FORMAT
// ═══════════════════════════════════════════════════════════════════════════

/// Encode a message to wire format: `[4-byte BE length][JSON]`.
pub fn encode_message(msg: &impl Serialize) -> DaemonResult<Vec<u8>> {
    let json = serde_json::to_vec(msg)?;
    if json.len() > MAX_MESSAGE_SIZE {
        return Err(DaemonError::MessageTooLarge {
            size: json.len(),
            max: MAX_MESSAGE_SIZE,
        });
    }
    let len = json.len() as u32;
    let mut buf = Vec::with_capacity(LENGTH_PREFIX_SIZE + json.len());
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(&json);
    Ok(buf)
}

/// Decode a message from a reader (async).
///
/// Reads a 4-byte big-endian length prefix, then reads that many bytes
/// of JSON payload and deserializes.
pub async fn decode_message<T: DeserializeOwned>(
    reader: &mut (impl AsyncRead + Unpin),
) -> DaemonResult<T> {
    // Read length prefix
    let mut len_buf = [0u8; LENGTH_PREFIX_SIZE];
    reader.read_exact(&mut len_buf).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            DaemonError::Protocol("connection closed before length prefix".into())
        } else {
            DaemonError::Connection(e)
        }
    })?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > MAX_MESSAGE_SIZE {
        return Err(DaemonError::MessageTooLarge {
            size: len,
            max: MAX_MESSAGE_SIZE,
        });
    }

    // Read JSON payload
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            DaemonError::Protocol("connection closed before full payload".into())
        } else {
            DaemonError::Connection(e)
        }
    })?;

    serde_json::from_slice(&payload).map_err(|e| DaemonError::Protocol(e.to_string()))
}

/// Write a message to a writer (async).
pub async fn write_message(
    writer: &mut (impl AsyncWrite + Unpin),
    msg: &impl Serialize,
) -> DaemonResult<()> {
    let buf = encode_message(msg)?;
    writer.write_all(&buf).await?;
    writer.flush().await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Serialization tests ─────────────────────────────────────────────

    #[test]
    fn request_serialize_ping() {
        let req = DaemonRequest::Ping;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"type":"Ping"}"#);
    }

    #[test]
    fn request_serialize_get_secret() {
        let req = DaemonRequest::GetSecret {
            provider: "anthropic".into(),
            auth_token: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"GetSecret""#));
        assert!(json.contains(r#""provider":"anthropic""#));
    }

    #[test]
    fn request_serialize_has_secret() {
        let req = DaemonRequest::HasSecret {
            provider: "openai".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"HasSecret""#));
        assert!(json.contains(r#""provider":"openai""#));
    }

    #[test]
    fn request_serialize_list_secrets() {
        let req = DaemonRequest::ListSecrets;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"type":"ListSecrets"}"#);
    }

    #[test]
    fn request_serialize_status() {
        let req = DaemonRequest::Status;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"type":"Status"}"#);
    }

    #[test]
    fn request_serialize_shutdown() {
        let req = DaemonRequest::Shutdown {
            auth_token: Some("tok-abc".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"Shutdown""#));
        assert!(json.contains(r#""auth_token":"tok-abc""#));
    }

    #[test]
    fn request_serialize_shutdown_no_token() {
        let req = DaemonRequest::Shutdown { auth_token: None };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"Shutdown""#));
        assert!(json.contains(r#""auth_token":null"#));
    }

    #[test]
    fn response_serialize_shutting_down() {
        let resp = DaemonResponse::ShuttingDown;
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"type":"ShuttingDown"}"#);
    }

    #[test]
    fn response_serialize_pong() {
        let resp = DaemonResponse::Pong {
            version: "0.46.1".into(),
            uptime_secs: 120,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""type":"Pong""#));
        assert!(json.contains(r#""version":"0.46.1""#));
        assert!(json.contains(r#""uptime_secs":120"#));
    }

    #[test]
    fn response_serialize_error() {
        let resp = DaemonResponse::Error {
            code: "NIKA-500".into(),
            message: "internal error".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""type":"Error""#));
        assert!(json.contains(r#""code":"NIKA-500""#));
    }

    #[test]
    fn response_serialize_secret() {
        let resp = DaemonResponse::Secret {
            value: Some("sk-ant-123".into()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""type":"Secret""#));
        assert!(json.contains(r#""value":"sk-ant-123""#));
    }

    #[test]
    fn response_serialize_secret_none() {
        let resp = DaemonResponse::Secret { value: None };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""value":null"#));
    }

    #[test]
    fn response_serialize_secret_exists() {
        let resp = DaemonResponse::SecretExists { exists: true };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""exists":true"#));
    }

    #[test]
    fn response_serialize_secret_list() {
        let resp = DaemonResponse::SecretList {
            providers: vec![
                ProviderSecretInfo {
                    provider: "anthropic".into(),
                    source: SecretSource::Env,
                },
                ProviderSecretInfo {
                    provider: "openai".into(),
                    source: SecretSource::NotFound,
                },
            ],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""source":"Env""#));
        assert!(json.contains(r#""source":"NotFound""#));
    }

    #[test]
    fn response_serialize_status_info() {
        let resp = DaemonResponse::StatusInfo {
            pid: 1234,
            uptime_secs: 3600,
            services: vec!["secrets".into(), "jobs".into()],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""pid":1234"#));
        assert!(json.contains(r#""secrets""#));
    }

    // ── Roundtrip tests ─────────────────────────────────────────────────

    // ── SetSecret / DeleteSecret tests ─────────────────────────────────

    #[test]
    fn request_serialize_set_secret() {
        let req = DaemonRequest::SetSecret {
            provider: "anthropic".into(),
            key: "sk-ant-test".into(),
            auth_token: Some("tok-abc".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"SetSecret""#));
        assert!(json.contains(r#""provider":"anthropic""#));
        assert!(json.contains(r#""key":"sk-ant-test""#));
        assert!(json.contains(r#""auth_token":"tok-abc""#));
    }

    #[test]
    fn request_serialize_set_secret_no_token() {
        let req = DaemonRequest::SetSecret {
            provider: "openai".into(),
            key: "sk-proj-test".into(),
            auth_token: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""auth_token":null"#));
    }

    #[test]
    fn request_serialize_delete_secret() {
        let req = DaemonRequest::DeleteSecret {
            provider: "mistral".into(),
            auth_token: Some("tok-xyz".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"DeleteSecret""#));
        assert!(json.contains(r#""provider":"mistral""#));
    }

    #[test]
    fn response_serialize_secret_stored() {
        let resp = DaemonResponse::SecretStored;
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"type":"SecretStored"}"#);
    }

    #[test]
    fn response_serialize_secret_deleted() {
        let resp = DaemonResponse::SecretDeleted;
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"type":"SecretDeleted"}"#);
    }

    #[test]
    fn response_serialize_auth_required() {
        let resp = DaemonResponse::AuthRequired;
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"type":"AuthRequired"}"#);
    }

    #[test]
    fn roundtrip_request_all_variants() {
        let requests = vec![
            DaemonRequest::Ping,
            DaemonRequest::Status,
            DaemonRequest::GetSecret {
                provider: "anthropic".into(),
                auth_token: None,
            },
            DaemonRequest::HasSecret {
                provider: "openai".into(),
            },
            DaemonRequest::ListSecrets,
            DaemonRequest::SetSecret {
                provider: "anthropic".into(),
                key: "sk-ant-test".into(),
                auth_token: Some("tok-123".into()),
            },
            DaemonRequest::DeleteSecret {
                provider: "mistral".into(),
                auth_token: None,
            },
            DaemonRequest::ListProviderStatus,
            DaemonRequest::EstimateCost {
                provider: "anthropic".into(),
                model: "claude-sonnet-4-20250514".into(),
                input_tokens: 1000,
                output_tokens: 500,
            },
            DaemonRequest::GetWorkflowHistory {
                workflow: "test.nika.yaml".into(),
            },
            DaemonRequest::GetDaemonCapabilities,
            DaemonRequest::Shutdown {
                auth_token: Some("tok-shutdown".into()),
            },
        ];
        for req in requests {
            let json = serde_json::to_string(&req).unwrap();
            let deserialized: DaemonRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(req, deserialized);
        }
    }

    #[test]
    fn roundtrip_response_all_variants() {
        use nika_core::catalogs::{
            CostEstimate, DaemonCapabilities, KeySource, ProviderCategory, ProviderStatusInfo,
            WorkflowRunInfo,
        };
        let responses = vec![
            DaemonResponse::Ok,
            DaemonResponse::Error {
                code: "E001".into(),
                message: "test".into(),
            },
            DaemonResponse::Pong {
                version: "0.46.1".into(),
                uptime_secs: 0,
            },
            DaemonResponse::StatusInfo {
                pid: 1,
                uptime_secs: 0,
                services: vec![],
            },
            DaemonResponse::Secret {
                value: Some("key".into()),
            },
            DaemonResponse::Secret { value: None },
            DaemonResponse::SecretExists { exists: true },
            DaemonResponse::SecretList { providers: vec![] },
            DaemonResponse::SecretStored,
            DaemonResponse::SecretDeleted,
            DaemonResponse::AuthRequired,
            DaemonResponse::ProviderStatusList {
                providers: vec![ProviderStatusInfo {
                    id: "anthropic".into(),
                    name: "Anthropic Claude".into(),
                    has_key: true,
                    source: KeySource::Env,
                    category: ProviderCategory::Llm,
                    env_var: "ANTHROPIC_API_KEY".into(),
                }],
            },
            DaemonResponse::CostEstimateResult {
                estimate: CostEstimate {
                    usd: 0.01,
                    input_rate_per_million: 3.0,
                    output_rate_per_million: 15.0,
                    model: "test".into(),
                    provider: "anthropic".into(),
                },
            },
            DaemonResponse::WorkflowHistoryResult {
                runs: vec![WorkflowRunInfo {
                    job_id: "j1".into(),
                    state: "completed".into(),
                    workflow: "w.nika.yaml".into(),
                    created_at: "2026-01-01T00:00:00Z".into(),
                    started_at: None,
                    completed_at: None,
                    exit_code: None,
                }],
            },
            DaemonResponse::DaemonCapabilitiesResult {
                capabilities: DaemonCapabilities {
                    version: "0.50.0".into(),
                    uptime_secs: 0,
                    cache_entries: 0,
                    cache_hit_rate: 0.0,
                    active_jobs: 0,
                    watch_active: false,
                    total_cost_saved: 0.0,
                },
            },
            DaemonResponse::ShuttingDown,
        ];
        for resp in responses {
            let json = serde_json::to_string(&resp).unwrap();
            let deserialized: DaemonResponse = serde_json::from_str(&json).unwrap();
            assert_eq!(resp, deserialized);
        }
    }

    // ── Wire format tests ───────────────────────────────────────────────

    #[test]
    fn wire_format_length_prefix() {
        let req = DaemonRequest::Ping;
        let buf = encode_message(&req).unwrap();

        // First 4 bytes = big-endian length
        let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        assert_eq!(len, buf.len() - LENGTH_PREFIX_SIZE);

        // Remaining bytes = valid JSON
        let json: DaemonRequest = serde_json::from_slice(&buf[LENGTH_PREFIX_SIZE..]).unwrap();
        assert_eq!(json, req);
    }

    #[test]
    fn wire_format_encode_decode_consistency() {
        let req = DaemonRequest::GetSecret {
            provider: "anthropic".into(),
            auth_token: None,
        };
        let buf = encode_message(&req).unwrap();

        // Verify length
        let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        let payload = &buf[LENGTH_PREFIX_SIZE..];
        assert_eq!(payload.len(), len);

        // Verify JSON
        let decoded: DaemonRequest = serde_json::from_slice(payload).unwrap();
        assert_eq!(decoded, req);
    }

    #[tokio::test]
    async fn wire_format_async_roundtrip() {
        let req = DaemonRequest::GetSecret {
            provider: "mistral".into(),
            auth_token: None,
        };

        // Encode
        let buf = encode_message(&req).unwrap();

        // Decode from cursor (simulates reading from socket)
        let mut cursor = std::io::Cursor::new(buf);
        let decoded: DaemonRequest = decode_message(&mut cursor).await.unwrap();
        assert_eq!(decoded, req);
    }

    #[tokio::test]
    async fn wire_format_response_async_roundtrip() {
        let resp = DaemonResponse::Pong {
            version: "0.46.1".into(),
            uptime_secs: 42,
        };

        let buf = encode_message(&resp).unwrap();
        let mut cursor = std::io::Cursor::new(buf);
        let decoded: DaemonResponse = decode_message(&mut cursor).await.unwrap();
        assert_eq!(decoded, resp);
    }

    #[tokio::test]
    async fn wire_format_multiple_messages() {
        let messages = vec![
            DaemonRequest::Ping,
            DaemonRequest::Status,
            DaemonRequest::ListSecrets,
        ];

        // Encode all into one buffer
        let mut buf = Vec::new();
        for msg in &messages {
            buf.extend(encode_message(msg).unwrap());
        }

        // Decode all from one reader
        let mut cursor = std::io::Cursor::new(buf);
        for expected in &messages {
            let decoded: DaemonRequest = decode_message(&mut cursor).await.unwrap();
            assert_eq!(&decoded, expected);
        }
    }

    #[tokio::test]
    async fn wire_format_write_then_read() {
        let req = DaemonRequest::HasSecret {
            provider: "groq".into(),
        };

        // Write to vec
        let mut buf = Vec::new();
        write_message(&mut buf, &req).await.unwrap();

        // Read back
        let mut cursor = std::io::Cursor::new(buf);
        let decoded: DaemonRequest = decode_message(&mut cursor).await.unwrap();
        assert_eq!(decoded, req);
    }

    #[tokio::test]
    async fn wire_format_empty_reader_returns_error() {
        let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
        let result: DaemonResult<DaemonRequest> = decode_message(&mut cursor).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn wire_format_truncated_payload_returns_error() {
        // Claim 100 bytes but only provide 5
        let mut buf = Vec::new();
        buf.extend_from_slice(&100u32.to_be_bytes());
        buf.extend_from_slice(b"hello");

        let mut cursor = std::io::Cursor::new(buf);
        let result: DaemonResult<DaemonRequest> = decode_message(&mut cursor).await;
        assert!(result.is_err());
    }

    #[test]
    fn wire_format_too_large_message() {
        // Create a message that would exceed the limit
        let huge = "x".repeat(MAX_MESSAGE_SIZE + 1);
        let resp = DaemonResponse::Secret { value: Some(huge) };
        let result = encode_message(&resp);
        assert!(result.is_err());
        if let Err(DaemonError::MessageTooLarge { .. }) = result {
            // Expected
        } else {
            panic!("Expected MessageTooLarge error");
        }
    }

    #[test]
    fn provider_secret_info_roundtrip() {
        let info = ProviderSecretInfo {
            provider: "anthropic".into(),
            source: SecretSource::Vault,
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: ProviderSecretInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, decoded);
    }

    // ── LSP query tests ────────────────────────────────────────────────

    #[test]
    fn request_serialize_list_provider_status() {
        let req = DaemonRequest::ListProviderStatus;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"type":"ListProviderStatus"}"#);
        let back: DaemonRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn request_serialize_estimate_cost() {
        let req = DaemonRequest::EstimateCost {
            provider: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            input_tokens: 1000,
            output_tokens: 500,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"EstimateCost"#));
        assert!(json.contains(r#""input_tokens":1000"#));
        let back: DaemonRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn request_serialize_get_workflow_history() {
        let req = DaemonRequest::GetWorkflowHistory {
            workflow: "test.nika.yaml".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""workflow":"test.nika.yaml"#));
        let back: DaemonRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn request_serialize_get_daemon_capabilities() {
        let req = DaemonRequest::GetDaemonCapabilities;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"type":"GetDaemonCapabilities"}"#);
    }

    #[test]
    fn response_serialize_provider_status_list() {
        use nika_core::catalogs::{KeySource, ProviderCategory, ProviderStatusInfo};
        let resp = DaemonResponse::ProviderStatusList {
            providers: vec![ProviderStatusInfo {
                id: "anthropic".into(),
                name: "Anthropic Claude".into(),
                has_key: true,
                source: KeySource::Env,
                category: ProviderCategory::Llm,
                env_var: "ANTHROPIC_API_KEY".into(),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""type":"ProviderStatusList"#));
        let back: DaemonResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn response_serialize_cost_estimate_result() {
        use nika_core::catalogs::CostEstimate;
        let resp = DaemonResponse::CostEstimateResult {
            estimate: CostEstimate {
                usd: 0.018,
                input_rate_per_million: 3.0,
                output_rate_per_million: 15.0,
                model: "claude-sonnet-4-20250514".into(),
                provider: "anthropic".into(),
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""type":"CostEstimateResult"#));
        let back: DaemonResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn response_serialize_workflow_history_result() {
        use nika_core::catalogs::WorkflowRunInfo;
        let resp = DaemonResponse::WorkflowHistoryResult {
            runs: vec![WorkflowRunInfo {
                job_id: "j-1".into(),
                state: "completed".into(),
                workflow: "test.nika.yaml".into(),
                created_at: "2026-03-27T12:00:00Z".into(),
                started_at: Some("2026-03-27T12:00:01Z".into()),
                completed_at: Some("2026-03-27T12:00:03Z".into()),
                exit_code: Some(0),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: DaemonResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn response_serialize_daemon_capabilities_result() {
        use nika_core::catalogs::DaemonCapabilities;
        let resp = DaemonResponse::DaemonCapabilitiesResult {
            capabilities: DaemonCapabilities {
                version: "0.50.0".into(),
                uptime_secs: 3600,
                cache_entries: 42,
                cache_hit_rate: 0.87,
                active_jobs: 2,
                watch_active: true,
                total_cost_saved: 1.23,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""type":"DaemonCapabilitiesResult"#));
        let back: DaemonResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn secret_source_all_variants_roundtrip() {
        for source in [
            SecretSource::Env,
            SecretSource::Vault,
            SecretSource::NotFound,
        ] {
            let json = serde_json::to_string(&source).unwrap();
            let decoded: SecretSource = serde_json::from_str(&json).unwrap();
            assert_eq!(source, decoded);
        }
    }
}
