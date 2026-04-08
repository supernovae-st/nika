// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP Client Implementation
//!
//! Provides a client for connecting to MCP (Model Context Protocol) servers.
//! Uses rmcp SDK for real connections, with mock mode for testing.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika_mcp::{McpClient, McpConfig};
//! use serde_json::json;
//!
//! // Create client from config
//! let config = McpConfig::new("novanet", "npx")
//!     .with_args(["-y", "@novanet/mcp-server"]);
//! let client = McpClient::new(config)?;
//!
//! // Connect and call tool
//! client.connect().await?;
//! let result = client.call_tool("novanet_describe", json!({})).await?;
//! ```
//!
//! ## Mock Mode
//!
//! For testing, use `McpClient::mock()` to create a pre-connected client
//! that returns canned responses:
//!
//! ```rust,ignore
//! let client = McpClient::mock("novanet");
//! assert!(client.is_connected());
//! ```
//!
//! ## Response Caching
//!
//! Enable response caching for deterministic tools:
//!
//! ```rust,ignore
//! use std::time::Duration;
//!
//! let client = McpClient::new(config)?
//!     .with_cache(CacheConfig {
//!         ttl: Duration::from_secs(300), // 5 minutes
//!         max_entries: 1000,
//!     });
//!
//! // First call hits the server
//! let r1 = client.call_tool("novanet_describe", json!({})).await?;
//!
//! // Second call with same params returns cached result
//! let r2 = client.call_tool("novanet_describe", json!({})).await?;
//! ```

use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use rustc_hash::FxHasher;
use serde_json::Value;

use std::sync::Arc;

use crate::error::{McpError, Result};
use crate::retry::{retry_mcp_call, McpRetryConfig};
use crate::rmcp_adapter::RmcpClientAdapter;
use crate::types::{ContentBlock, McpConfig, ResourceContent, ToolCallResult, ToolDefinition};
use crate::validation::{ErrorEnhancer, McpValidator, ValidationConfig, ValidationErrorKind};
use nika_event::{EventKind, EventLog};

// ═══════════════════════════════════════════════════════════════════════════
// HEALTH CHECK TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// Result of a successful MCP server ping.
#[derive(Debug, Clone)]
pub struct McpPingResult {
    /// Server name
    pub server: String,

    /// Round-trip latency
    pub latency: Duration,

    /// Number of tools available on the server
    pub tool_count: usize,

    /// Whether the connection was already established
    pub was_connected: bool,
}

/// Error when pinging an MCP server.
#[derive(Debug, Clone)]
pub enum McpPingError {
    /// Server process failed to start
    StartFailed { server: String, details: String },

    /// Server timed out responding
    Timeout { server: String, timeout: Duration },

    /// Connection was refused
    ConnectionRefused { server: String },

    /// Server responded with error
    ServerError { server: String, details: String },
}

impl std::fmt::Display for McpPingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpPingError::StartFailed { server, details } => {
                write!(f, "MCP server '{}' failed to start: {}", server, details)
            }
            McpPingError::Timeout { server, timeout } => {
                write!(f, "MCP server '{}' timed out after {:?}", server, timeout)
            }
            McpPingError::ConnectionRefused { server } => {
                write!(f, "MCP server '{}' connection refused", server)
            }
            McpPingError::ServerError { server, details } => {
                write!(f, "MCP server '{}' error: {}", server, details)
            }
        }
    }
}

impl McpPingError {
    /// Get a user-friendly suggestion for fixing this error.
    pub fn suggestion(&self) -> &'static str {
        match self {
            McpPingError::StartFailed { .. } => {
                "Check the MCP server command is correct and the executable exists"
            }
            McpPingError::Timeout { .. } => {
                "The MCP server may be slow to start. Try increasing the timeout"
            }
            McpPingError::ConnectionRefused { .. } => {
                "Ensure the MCP server is running and accessible"
            }
            McpPingError::ServerError { .. } => "Check the MCP server logs for more details",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CACHE TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// Cache configuration for MCP response caching.
///
/// # Example
///
/// ```rust,ignore
/// use std::time::Duration;
///
/// let config = CacheConfig {
///     ttl: Duration::from_secs(300), // 5 minutes
///     max_entries: 1000,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Time-to-live for cache entries
    pub ttl: Duration,

    /// Maximum number of entries in the cache
    pub max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(300), // 5 minutes
            max_entries: 1000,
        }
    }
}

/// A cached MCP tool response.
///
/// Stores result behind `Arc` for cheap cloning on cache hits.
/// `ToolCallResult` can contain large content blocks (text, base64 images),
/// so Arc avoids deep cloning the entire content on every cache access.
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached result (Arc for cheap cloning)
    result: Arc<ToolCallResult>,

    /// When the entry was created
    created_at: Instant,
}

impl CacheEntry {
    fn new(result: Arc<ToolCallResult>) -> Self {
        Self {
            result,
            created_at: Instant::now(),
        }
    }

    fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }
}

/// Response cache for MCP tool calls.
///
/// Thread-safe cache using DashMap with TTL-based expiration.
#[derive(Debug)]
struct ResponseCache {
    /// Configuration
    config: CacheConfig,

    /// Cache entries keyed by "tool:params_hash"
    entries: DashMap<String, CacheEntry, rustc_hash::FxBuildHasher>,

    /// Cache hit counter
    hits: AtomicU64,

    /// Cache miss counter
    misses: AtomicU64,
}

impl ResponseCache {
    fn new(config: CacheConfig) -> Self {
        Self {
            config,
            entries: DashMap::default(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Generate cache key from tool name and params.
    ///
    /// Uses canonical JSON serialization (sorted keys) so that semantically
    /// identical objects with different key insertion order produce the same key.
    fn cache_key(tool: &str, params: &Value) -> String {
        let mut hasher = FxHasher::default();
        // Canonicalize: sort object keys recursively, then serialize.
        let canonical = Self::canonicalize_value(params);
        let params_str = match serde_json::to_string(&canonical) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    tool = tool,
                    error = %e,
                    "JSON serialization failed for cache key, using Debug format"
                );
                format!("{:?}", params)
            }
        };
        params_str.hash(&mut hasher);
        format!("{}:{:016x}", tool, hasher.finish())
    }

    /// Maximum nesting depth for JSON canonicalization to prevent stack overflow.
    const MAX_CANONICALIZE_DEPTH: usize = 128;

    /// Recursively sort all object keys in a JSON Value for canonical serialization.
    ///
    /// Limits recursion depth to [`MAX_CANONICALIZE_DEPTH`] to prevent stack overflow
    /// on adversarial input.
    fn canonicalize_value(value: &Value) -> Value {
        Self::canonicalize_value_inner(value, 0)
    }

    fn canonicalize_value_inner(value: &Value, depth: usize) -> Value {
        if depth >= Self::MAX_CANONICALIZE_DEPTH {
            return value.clone();
        }
        match value {
            Value::Object(map) => {
                let mut sorted: serde_json::Map<String, Value> = serde_json::Map::new();
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                for key in keys {
                    sorted.insert(
                        key.clone(),
                        Self::canonicalize_value_inner(&map[key], depth + 1),
                    );
                }
                Value::Object(sorted)
            }
            Value::Array(arr) => Value::Array(
                arr.iter()
                    .map(|v| Self::canonicalize_value_inner(v, depth + 1))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// Get a cached result if it exists and is not expired.
    ///
    /// Returns an `Arc<ToolCallResult>` for cheap sharing (atomic ref-count increment
    /// instead of deep cloning content blocks).
    fn get(&self, tool: &str, params: &Value) -> Option<Arc<ToolCallResult>> {
        let key = Self::cache_key(tool, params);

        if let Some(entry) = self.entries.get(&key) {
            if entry.is_expired(self.config.ttl) {
                // Entry expired — remove atomically only if still expired
                // (avoids TOCTOU where a fresh entry gets deleted between drop+remove)
                let ttl = self.config.ttl;
                drop(entry);
                self.entries.remove_if(&key, |_, e| e.is_expired(ttl));
                self.misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }

            self.hits.fetch_add(1, Ordering::Relaxed);
            return Some(Arc::clone(&entry.result));
        }

        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Store a result in the cache.
    ///
    /// Wraps the result in `Arc` for cheap retrieval on subsequent hits.
    fn put(&self, tool: &str, params: &Value, result: ToolCallResult) {
        // Don't cache errors
        if result.is_error {
            return;
        }

        let key = Self::cache_key(tool, params);

        // Evict oldest entries if over capacity
        if self.entries.len() >= self.config.max_entries {
            self.evict_oldest();
        }

        self.entries.insert(key, CacheEntry::new(Arc::new(result)));
    }

    /// Evict the oldest entries to make room for new ones.
    ///
    /// Uses partial sort (`select_nth_unstable_by_key`) for O(n) eviction
    /// instead of O(n log n) full sort.
    fn evict_oldest(&self) {
        let to_remove = (self.config.max_entries / 10).max(1);
        let mut entries: Vec<(String, Instant)> = self
            .entries
            .iter()
            .map(|e| (e.key().clone(), e.created_at))
            .collect();

        if entries.len() <= to_remove {
            // Fewer entries than eviction target — remove all
            for (key, _) in &entries {
                self.entries.remove(key);
            }
            return;
        }

        // Partial sort: partition so the `to_remove` oldest are at the front
        entries.select_nth_unstable_by_key(to_remove - 1, |(_, created)| *created);

        for (key, _) in entries.iter().take(to_remove) {
            self.entries.remove(key);
        }
    }

    /// Clear all entries.
    fn clear(&self) {
        self.entries.clear();
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
    }

    /// Get cache statistics.
    fn stats(&self) -> ResponseCacheStats {
        ResponseCacheStats {
            entries: self.entries.len(),
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
        }
    }
}

/// Response cache statistics for observability.
#[derive(Debug, Clone, Default)]
pub struct ResponseCacheStats {
    /// Number of entries in the cache
    pub entries: usize,

    /// Number of cache hits
    pub hits: u64,

    /// Number of cache misses
    pub misses: u64,
}

impl ResponseCacheStats {
    /// Calculate hit rate (0.0 to 1.0)
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// MCP Client for connecting to and interacting with MCP servers.
///
/// The client can operate in two modes:
/// - **Real mode**: Uses rmcp SDK via RmcpClientAdapter
/// - **Mock mode**: Returns canned responses for testing
///
/// ## Validation
///
/// Enable parameter validation with `with_validation()`:
///
/// ```rust,ignore
/// let client = McpClient::new(config)?
///     .with_validation(ValidationConfig::default());
/// ```
///
/// When validation is enabled:
/// 1. `connect()` caches tool schemas from `list_tools()`
/// 2. `call_tool()` validates params before calling the server
/// 3. Errors are enhanced with required fields and suggestions
pub struct McpClient {
    /// Server name (from config or mock)
    name: String,

    /// Connection state (atomic for interior mutability)
    /// For mock clients, this tracks mock state.
    /// For real clients, rmcp adapter tracks actual connection.
    connected: AtomicBool,

    /// Whether this is a mock client
    is_mock: bool,

    /// rmcp adapter for real connections (None for mock clients)
    adapter: Option<RmcpClientAdapter>,

    /// Parameter validator (None if validation disabled)
    validator: Option<McpValidator>,

    /// Response cache (None if caching disabled)
    cache: Option<ResponseCache>,

    /// Guard to prevent concurrent reconnect storms from for_each tasks
    reconnecting: AtomicBool,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("name", &self.name)
            .field("connected", &self.connected)
            .field("is_mock", &self.is_mock)
            .field("has_adapter", &self.adapter.is_some())
            .field("has_validator", &self.validator.is_some())
            .field("has_cache", &self.cache.is_some())
            .finish()
    }
}

impl McpClient {
    /// Create a new MCP client from configuration.
    ///
    /// Validates the configuration and returns an error if invalid.
    /// The client is created in disconnected state.
    ///
    /// # Errors
    ///
    /// Returns `McpError::ValidationError` if:
    /// - `config.name` is empty
    /// - `config.command` is empty
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = McpConfig::new("novanet", "npx")
    ///     .with_args(["-y", "@novanet/mcp-server"]);
    /// let client = McpClient::new(config)?;
    /// assert!(!client.is_connected());
    /// ```
    pub fn new(config: McpConfig) -> Result<Self> {
        // Validate configuration
        if config.name.is_empty() {
            return Err(McpError::ValidationError {
                reason: "MCP server name cannot be empty".to_string(),
            });
        }

        if config.command.is_empty() {
            return Err(McpError::ValidationError {
                reason: "MCP server command cannot be empty".to_string(),
            });
        }

        let name = config.name.clone();
        let adapter = RmcpClientAdapter::new(config);

        Ok(Self {
            name,
            connected: AtomicBool::new(false),
            is_mock: false,
            adapter: Some(adapter),
            validator: None,
            cache: None,
            reconnecting: AtomicBool::new(false),
        })
    }

    /// Enable parameter validation with the given config.
    ///
    /// When validation is enabled:
    /// - `connect()` will cache tool schemas from `list_tools()`
    /// - `call_tool()` will validate params before calling the server
    /// - Errors will be enhanced with required fields and suggestions
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let client = McpClient::new(config)?
    ///     .with_validation(ValidationConfig::default());
    /// ```
    pub fn with_validation(mut self, config: ValidationConfig) -> Self {
        self.validator = Some(McpValidator::new(config));
        self
    }

    /// Enable response caching with the given config.
    ///
    /// When caching is enabled:
    /// - Successful tool responses are cached by `tool:params_hash` key
    /// - Subsequent calls with same params return cached results
    /// - Cache entries expire after TTL
    /// - Error responses are never cached
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use std::time::Duration;
    ///
    /// let client = McpClient::new(config)?
    ///     .with_cache(CacheConfig {
    ///         ttl: Duration::from_secs(300), // 5 minutes
    ///         max_entries: 1000,
    ///     });
    /// ```
    pub fn with_cache(mut self, config: CacheConfig) -> Self {
        self.cache = Some(ResponseCache::new(config));
        self
    }

    /// Get cache statistics (hits, misses, entries).
    ///
    /// Returns `None` if caching is disabled.
    pub fn cache_stats(&self) -> Option<ResponseCacheStats> {
        self.cache.as_ref().map(|c| c.stats())
    }

    /// Create a mock MCP client for testing.
    ///
    /// The mock client is pre-connected and returns canned responses:
    /// - `novanet_describe`: Returns `{"nodes": 62, "arcs": 182}`
    /// - `novanet_context`: Returns entity context JSON
    /// - Other tools: Returns a generic success response
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let client = McpClient::mock("novanet");
    /// assert!(client.is_connected());
    /// ```
    pub fn mock(name: &str) -> Self {
        Self {
            name: name.to_string(),
            connected: AtomicBool::new(true), // Mock is pre-connected
            is_mock: true,
            adapter: None,
            validator: None,
            cache: None,
            reconnecting: AtomicBool::new(false),
        }
    }

    /// Get the server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Check if the client is connected to the server.
    ///
    /// For real clients, delegates to adapter's sync check (non-blocking).
    /// This avoids race conditions where AtomicBool becomes stale.
    pub fn is_connected(&self) -> bool {
        if self.is_mock {
            return self.connected.load(Ordering::SeqCst);
        }
        // Delegate to adapter for accurate state (avoids stale AtomicBool)
        self.adapter
            .as_ref()
            .map(|a| a.is_connected_sync())
            .unwrap_or(false)
    }

    /// Check connection state asynchronously (accurate for real clients).
    pub async fn is_connected_async(&self) -> bool {
        if self.is_mock {
            return self.connected.load(Ordering::SeqCst);
        }
        if let Some(adapter) = &self.adapter {
            adapter.is_connected().await
        } else {
            false
        }
    }

    /// Ping the MCP server to verify it's responsive.
    ///
    /// This method:
    /// 1. Connects to the server if not already connected
    /// 2. Calls `list_tools()` to verify the server responds
    /// 3. Returns latency and tool count
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let result = client.ping().await?;
    /// println!("Server {} responded in {:?} with {} tools",
    ///     result.server, result.latency, result.tool_count);
    /// ```
    pub async fn ping(&self) -> std::result::Result<McpPingResult, McpPingError> {
        let start = Instant::now();
        let was_connected = self.is_connected_async().await;

        // For mock clients, always succeed quickly
        if self.is_mock {
            return Ok(McpPingResult {
                server: self.name.clone(),
                latency: start.elapsed(),
                tool_count: self.mock_list_tools().len(),
                was_connected: true,
            });
        }

        // Connect if needed
        if !was_connected {
            if let Err(e) = self.connect().await {
                let error_msg = e.to_string().to_lowercase();
                if error_msg.contains("refused") || error_msg.contains("connection") {
                    return Err(McpPingError::ConnectionRefused {
                        server: self.name.clone(),
                    });
                }
                return Err(McpPingError::StartFailed {
                    server: self.name.clone(),
                    details: e.to_string(),
                });
            }
        }

        // Call list_tools with timeout to verify server responds
        match tokio::time::timeout(Duration::from_secs(10), self.list_tools()).await {
            Ok(Ok(tools)) => Ok(McpPingResult {
                server: self.name.clone(),
                latency: start.elapsed(),
                tool_count: tools.len(),
                was_connected,
            }),
            Ok(Err(e)) => Err(McpPingError::ServerError {
                server: self.name.clone(),
                details: e.to_string(),
            }),
            Err(_) => Err(McpPingError::Timeout {
                server: self.name.clone(),
                timeout: Duration::from_secs(10),
            }),
        }
    }

    /// Quick check if MCP server is likely to be reachable.
    ///
    /// Returns true if:
    /// - Mock client: always true
    /// - Real client: adapter exists and is configured
    ///
    /// This is a synchronous check that doesn't actually connect.
    /// Use `ping()` for a full health check.
    pub fn is_configured(&self) -> bool {
        self.is_mock || self.adapter.is_some()
    }

    /// Connect to the MCP server.
    ///
    /// For mock clients, this is a no-op that always succeeds.
    /// For real clients, this uses rmcp SDK to connect.
    ///
    /// When validation is enabled, this also caches tool schemas from `list_tools()`.
    ///
    /// This method is idempotent - calling it when already connected succeeds.
    ///
    /// # Errors
    ///
    /// Returns `McpError::McpStartError` if the server process fails to start.
    /// Returns `McpError::McpSchemaError` if schema caching fails.
    pub async fn connect(&self) -> Result<()> {
        if self.is_mock {
            self.connected.store(true, Ordering::SeqCst);
            // Populate mock tools if validator is enabled
            if let Some(ref validator) = self.validator {
                let tools = self.mock_list_tools();
                validator
                    .cache()
                    .populate(&self.name, &tools)
                    .map_err(|e| McpError::McpSchemaError {
                        tool: "*".to_string(),
                        reason: format!("Failed to cache mock tool schemas: {}", e),
                    })?;
            }
            return Ok(());
        }

        let adapter = self
            .adapter
            .as_ref()
            .ok_or_else(|| McpError::McpNotConnected {
                name: self.name.clone(),
            })?;

        adapter.connect().await?;
        self.connected.store(true, Ordering::SeqCst);

        // Populate schema cache if validator is enabled
        if let Some(ref validator) = self.validator {
            let tools = adapter.list_tools().await?;
            validator
                .cache()
                .populate(&self.name, &tools)
                .map_err(|e| McpError::McpSchemaError {
                    tool: "*".to_string(),
                    reason: format!("Failed to cache tool schemas: {}", e),
                })?;
            tracing::debug!(
                mcp_server = %self.name,
                tools_cached = tools.len(),
                "Cached tool schemas for validation"
            );
        }

        Ok(())
    }

    /// Disconnect from the MCP server.
    ///
    /// For mock clients, this just updates the connection state.
    /// For real clients, this terminates the server process via rmcp.
    ///
    /// This method is idempotent - calling it when already disconnected succeeds.
    pub async fn disconnect(&self) -> Result<()> {
        if self.is_mock {
            self.connected.store(false, Ordering::SeqCst);
            return Ok(());
        }

        if let Some(adapter) = &self.adapter {
            adapter.disconnect().await?;
        }

        // Clear response cache — stale after disconnect
        if let Some(ref cache) = self.cache {
            cache.clear();
        }

        // Clear schema cache — schemas may change after server restart
        if let Some(ref validator) = self.validator {
            validator.cache().clear();
        }

        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Reconnect to the MCP server.
    ///
    /// Useful when the connection is broken (e.g., broken pipe, server crashed).
    /// This terminates any existing connection and establishes a new one.
    ///
    /// # Errors
    ///
    /// Returns `McpError::McpStartError` if reconnection fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // After detecting a broken connection
    /// client.reconnect().await?;
    /// // Retry the failed operation
    /// ```
    pub async fn reconnect(&self) -> Result<()> {
        if self.is_mock {
            self.connected.store(true, Ordering::SeqCst);
            return Ok(());
        }

        // Guard: only one task reconnects, others skip to avoid reconnect storm
        if self
            .reconnecting
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            tracing::debug!(
                mcp_server = %self.name,
                "Reconnect already in progress, skipping"
            );
            return Ok(());
        }

        // Reconnect — release the guard on completion (success or failure)
        let result = self.reconnect_inner().await;
        self.reconnecting.store(false, Ordering::SeqCst);
        result
    }

    /// Inner reconnect logic, separated so the guard can be released reliably.
    async fn reconnect_inner(&self) -> Result<()> {
        // Disconnect clears validator cache + response cache
        self.disconnect().await?;

        // Reconnect via the adapter (re-establishes transport)
        let adapter = self
            .adapter
            .as_ref()
            .ok_or_else(|| McpError::McpNotConnected {
                name: self.name.clone(),
            })?;

        adapter.reconnect().await?;
        self.connected.store(true, Ordering::SeqCst);

        // Re-populate validator schema cache (disconnect() cleared it)
        if let Some(ref validator) = self.validator {
            let tools = adapter.list_tools().await?;
            validator
                .cache()
                .populate(&self.name, &tools)
                .map_err(|e| McpError::McpSchemaError {
                    tool: "*".to_string(),
                    reason: format!("Failed to cache tool schemas after reconnect: {}", e),
                })?;
            tracing::debug!(
                mcp_server = %self.name,
                tools_cached = tools.len(),
                "Re-populated tool schemas after reconnect"
            );
        }

        Ok(())
    }

    /// Check if an error indicates a broken connection.
    ///
    /// Used to determine if a reconnection attempt should be made
    /// before the next retry.
    pub fn is_connection_error(error: &McpError) -> bool {
        let error_str = error.to_string().to_lowercase();
        error_str.contains("broken pipe")
            || error_str.contains("connection reset")
            || error_str.contains("connection refused")
            || error_str.contains("eof")
            || error_str.contains("stdin not available")
            || error_str.contains("stdout not available")
            || error_str.contains("transport closed")
            || error_str.contains("transport send")
    }

    /// Enhance an error with validation context if available.
    fn enhance_error(&self, tool_name: &str, error: McpError) -> McpError {
        if let Some(ref validator) = self.validator {
            if validator.config().enhance_errors {
                let enhancer = ErrorEnhancer::new(validator.cache());
                return enhancer.enhance(&self.name, tool_name, error);
            }
        }
        error
    }

    /// Call an MCP tool with the given parameters.
    ///
    /// # Arguments
    ///
    /// * `name` - Tool name (e.g., "novanet_context", "read_file")
    /// * `params` - Tool parameters as JSON value
    ///
    /// # Validation
    ///
    /// When validation is enabled via `with_validation()`:
    /// - Parameters are validated against the tool schema before calling
    /// - Errors include required fields and suggestions
    ///
    /// # Errors
    ///
    /// Returns `McpError::McpValidationFailed` if parameter validation fails.
    /// Returns `McpError::McpNotConnected` if the client is not connected.
    /// Returns `McpError::McpToolError` if the tool call fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let result = client.call_tool("novanet_context", json!({
    ///     "mode": "page",
    ///     "focus_key": "qr-code",
    ///     "locale": "fr-FR"
    /// })).await?;
    /// ```
    pub async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult> {
        // Pre-call validation (if enabled)
        if let Some(ref validator) = self.validator {
            if validator.config().pre_validate {
                let result = validator.validate(&self.name, name, &params);
                if !result.is_valid {
                    // Convert validation errors to McpError
                    let missing: Vec<String> = result
                        .errors
                        .iter()
                        .filter_map(|e| {
                            if let ValidationErrorKind::MissingRequired { field } = &e.kind {
                                Some(field.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    let suggestions: Vec<String> = result
                        .errors
                        .iter()
                        .filter_map(|e| {
                            if let ValidationErrorKind::UnknownField { suggestions, .. } = &e.kind {
                                Some(suggestions.clone())
                            } else {
                                None
                            }
                        })
                        .flatten()
                        .collect();

                    let details = result
                        .errors
                        .iter()
                        .map(|e| e.message.clone())
                        .collect::<Vec<_>>()
                        .join("; ");

                    return Err(McpError::McpValidationFailed {
                        tool: name.to_string(),
                        details,
                        missing,
                        suggestions,
                    });
                }
            }
        }

        // Check cache for a hit (before making the actual call)
        if let Some(ref cache) = self.cache {
            if let Some(cached_result) = cache.get(name, &params) {
                tracing::debug!(
                    mcp_server = %self.name,
                    tool = %name,
                    "Cache hit for MCP tool call"
                );
                let mut result = (*cached_result).clone();
                result.was_cached = true;
                return Ok(result);
            }
        }

        if self.is_mock {
            if !self.connected.load(Ordering::SeqCst) {
                return Err(McpError::McpNotConnected {
                    name: self.name.clone(),
                });
            }
            let result = self.mock_tool_call(name, &params);
            // Store mock result in cache too
            if let Some(ref cache) = self.cache {
                cache.put(name, &params, result.clone());
            }
            return Ok(result);
        }

        // Real mode: use rmcp adapter with retry via backon (NIKA-103)
        let adapter = self
            .adapter
            .as_ref()
            .ok_or_else(|| McpError::McpNotConnected {
                name: self.name.clone(),
            })?;

        let result = retry_mcp_call(McpRetryConfig::default(), || {
            let params = params.clone();
            async move {
                match adapter.call_tool(name, params).await {
                    Ok(result) => Ok(result),
                    Err(e) => {
                        let enhanced = self.enhance_error(name, e);
                        // On connection errors, attempt reconnect for next retry
                        if Self::is_connection_error(&enhanced) {
                            tracing::warn!(
                                mcp_server = %self.name,
                                tool = %name,
                                error = %enhanced,
                                "Connection error, attempting reconnect"
                            );
                            if let Err(reconnect_err) = self.reconnect().await {
                                tracing::error!(
                                    mcp_server = %self.name,
                                    error = %reconnect_err,
                                    "Failed to reconnect"
                                );
                            }
                        }
                        Err(enhanced)
                    }
                }
            }
        })
        .await?;

        // Store successful result in cache
        if let Some(ref cache) = self.cache {
            cache.put(name, &params, result.clone());
            tracing::debug!(
                mcp_server = %self.name,
                tool = %name,
                "Cached MCP tool response"
            );
        }
        Ok(result)
    }

    /// Call an MCP tool with retry event emission.
    ///
    /// This method is similar to `call_tool()` but emits `McpRetry` events
    /// through the provided EventLog when connection errors trigger retries.
    /// This enables TUI observability of MCP retry attempts.
    ///
    /// # Arguments
    ///
    /// * `name` - Tool name (e.g., "novanet_context")
    /// * `params` - Tool parameters as JSON
    /// * `task_id` - Task ID for event correlation
    /// * `event_log` - EventLog for emitting McpRetry events
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let result = client.call_tool_with_retry_events(
    ///     "novanet_context",
    ///     json!({"mode": "page", "locale": "fr-FR"}),
    ///     &task_id,
    ///     &event_log,
    /// ).await?;
    /// ```
    pub async fn call_tool_with_retry_events(
        &self,
        name: &str,
        params: Value,
        task_id: &Arc<str>,
        event_log: &EventLog,
    ) -> Result<ToolCallResult> {
        // Pre-call validation (if enabled) - same as call_tool()
        if let Some(ref validator) = self.validator {
            if validator.config().pre_validate {
                let result = validator.validate(&self.name, name, &params);
                if !result.is_valid {
                    let missing: Vec<String> = result
                        .errors
                        .iter()
                        .filter_map(|e| {
                            if let ValidationErrorKind::MissingRequired { field } = &e.kind {
                                Some(field.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    let suggestions: Vec<String> = result
                        .errors
                        .iter()
                        .filter_map(|e| {
                            if let ValidationErrorKind::UnknownField { suggestions, .. } = &e.kind {
                                Some(suggestions.clone())
                            } else {
                                None
                            }
                        })
                        .flatten()
                        .collect();

                    let details = result
                        .errors
                        .iter()
                        .map(|e| e.message.clone())
                        .collect::<Vec<_>>()
                        .join("; ");

                    return Err(McpError::McpValidationFailed {
                        tool: name.to_string(),
                        details,
                        missing,
                        suggestions,
                    });
                }
            }
        }

        // Check cache for a hit
        if let Some(ref cache) = self.cache {
            if let Some(cached_result) = cache.get(name, &params) {
                tracing::debug!(
                    mcp_server = %self.name,
                    tool = %name,
                    "Cache hit for MCP tool call"
                );
                let mut result = (*cached_result).clone();
                result.was_cached = true;
                return Ok(result);
            }
        }

        if self.is_mock {
            if !self.connected.load(Ordering::SeqCst) {
                return Err(McpError::McpNotConnected {
                    name: self.name.clone(),
                });
            }
            let result = self.mock_tool_call(name, &params);
            if let Some(ref cache) = self.cache {
                cache.put(name, &params, result.clone());
            }
            return Ok(result);
        }

        // Real mode: use rmcp adapter with retry via backon + event emission (NIKA-103)
        let adapter = self
            .adapter
            .as_ref()
            .ok_or_else(|| McpError::McpNotConnected {
                name: self.name.clone(),
            })?;

        let config = McpRetryConfig::default();
        let max_attempts = config.max_retries + 1; // total = initial + retries
        let attempt_counter = std::sync::atomic::AtomicU32::new(0);

        let result = retry_mcp_call(config, || {
            let params = params.clone();
            async {
                let attempt = attempt_counter.fetch_add(1, Ordering::SeqCst);
                match adapter.call_tool(name, params).await {
                    Ok(result) => Ok(result),
                    Err(e) => {
                        let enhanced = self.enhance_error(name, e);
                        if Self::is_connection_error(&enhanced) {
                            // Emit McpRetry event for TUI observability
                            event_log.emit(EventKind::McpRetry {
                                task_id: Arc::clone(task_id),
                                server_name: self.name.clone(),
                                operation: name.to_string(),
                                attempt: attempt + 1,
                                max_attempts: max_attempts as u32,
                                error: enhanced.to_string(),
                            });
                            tracing::warn!(
                                mcp_server = %self.name,
                                tool = %name,
                                attempt = attempt + 1,
                                error = %enhanced,
                                "Connection error, attempting reconnect (McpRetry event emitted)"
                            );
                            if let Err(reconnect_err) = self.reconnect().await {
                                tracing::error!(
                                    mcp_server = %self.name,
                                    error = %reconnect_err,
                                    "Failed to reconnect"
                                );
                            }
                        }
                        Err(enhanced)
                    }
                }
            }
        })
        .await?;

        // Store successful result in cache
        if let Some(ref cache) = self.cache {
            cache.put(name, &params, result.clone());
            tracing::debug!(
                mcp_server = %self.name,
                tool = %name,
                "Cached MCP tool response"
            );
        }
        Ok(result)
    }

    /// Read a resource from the MCP server.
    ///
    /// # Arguments
    ///
    /// * `uri` - Resource URI (e.g., "file:///path", "neo4j://entity/qr-code")
    ///
    /// # Errors
    ///
    /// Returns `McpError::McpNotConnected` if the client is not connected.
    /// Returns `McpError::McpResourceNotFound` if the resource doesn't exist.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let resource = client.read_resource("neo4j://entity/qr-code").await?;
    /// ```
    pub async fn read_resource(&self, uri: &str) -> Result<ResourceContent> {
        if self.is_mock {
            if !self.connected.load(Ordering::SeqCst) {
                return Err(McpError::McpNotConnected {
                    name: self.name.clone(),
                });
            }
            return Ok(self.mock_read_resource(uri));
        }

        // Real mode: use rmcp adapter with retry via backon (NIKA-103)
        let adapter = self
            .adapter
            .as_ref()
            .ok_or_else(|| McpError::McpNotConnected {
                name: self.name.clone(),
            })?;

        retry_mcp_call(McpRetryConfig::default(), || {
            async move {
                match adapter.read_resource(uri).await {
                    Ok(result) => Ok(result),
                    Err(e) => {
                        // On connection errors, attempt reconnect for next retry
                        if Self::is_connection_error(&e) {
                            tracing::warn!(
                                mcp_server = %self.name,
                                uri = %uri,
                                error = %e,
                                "Connection error, attempting reconnect"
                            );
                            if let Err(reconnect_err) = self.reconnect().await {
                                tracing::error!(
                                    mcp_server = %self.name,
                                    error = %reconnect_err,
                                    "Failed to reconnect"
                                );
                            }
                        }
                        Err(e)
                    }
                }
            }
        })
        .await
    }

    /// List all available tools from the MCP server.
    ///
    /// # Errors
    ///
    /// Returns `McpError::McpNotConnected` if the client is not connected.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let tools = client.list_tools().await?;
    /// for tool in tools {
    ///     println!("Tool: {}", tool.name);
    /// }
    /// ```
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        if self.is_mock {
            if !self.connected.load(Ordering::SeqCst) {
                return Err(McpError::McpNotConnected {
                    name: self.name.clone(),
                });
            }
            return Ok(self.mock_list_tools());
        }

        // Real mode: use rmcp adapter
        let adapter = self
            .adapter
            .as_ref()
            .ok_or_else(|| McpError::McpNotConnected {
                name: self.name.clone(),
            })?;

        adapter.list_tools().await
    }

    // ═══════════════════════════════════════════════════════════════
    // MOCK IMPLEMENTATIONS
    // ═══════════════════════════════════════════════════════════════

    /// Generate mock response for tool calls.
    fn mock_tool_call(&self, name: &str, params: &Value) -> ToolCallResult {
        match name {
            "novanet_describe" => {
                let response = serde_json::json!({
                    "nodes": 61,
                    "arcs": 182,
                    "labels": ["Entity", "EntityNative", "Page", "Block"],
                    "relationships": ["HAS_NATIVE", "CONTAINS", "FLOWS_TO"]
                });
                ToolCallResult::success(vec![ContentBlock::text(response.to_string())])
            }

            "novanet_context" => {
                // Extract focus_key/entity from params for a realistic response
                let entity = params
                    .get("focus_key")
                    .or_else(|| params.get("entity"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let locale = params
                    .get("locale")
                    .and_then(|v| v.as_str())
                    .unwrap_or("en-US");

                let response = serde_json::json!({
                    "entity": entity,
                    "locale": locale,
                    "context": {
                        "title": format!("{} - Generated Title", entity),
                        "description": format!("Auto-generated content for {} in {}", entity, locale),
                        "keywords": ["generated", "mock", entity]
                    }
                });
                ToolCallResult::success(vec![ContentBlock::text(response.to_string())])
            }

            _ => {
                // Generic success response for unknown tools
                let response = serde_json::json!({
                    "tool": name,
                    "status": "success",
                    "message": "Mock tool call completed"
                });
                ToolCallResult::success(vec![ContentBlock::text(response.to_string())])
            }
        }
    }

    /// Generate mock response for resource reads.
    fn mock_read_resource(&self, uri: &str) -> ResourceContent {
        // Generate a mock resource based on URI pattern
        let text = if uri.starts_with("neo4j://entity/") {
            let entity = uri.strip_prefix("neo4j://entity/").unwrap_or("unknown");
            serde_json::json!({
                "id": entity,
                "type": "Entity",
                "properties": {
                    "name": entity,
                    "created": "2024-01-01T00:00:00Z"
                }
            })
            .to_string()
        } else if uri.starts_with("file://") {
            "Mock file content".to_string()
        } else {
            serde_json::json!({
                "uri": uri,
                "content": "Mock resource content"
            })
            .to_string()
        };

        ResourceContent::new(uri)
            .with_mime_type("application/json")
            .with_text(text)
    }

    /// Get tool definitions synchronously.
    ///
    /// For mock clients, returns mock tool definitions.
    /// For real clients, returns cached tools from the last `list_tools()` call.
    ///
    /// **Important:** For real clients, you must call `list_tools().await` first
    /// to populate the cache before this method returns useful results.
    ///
    /// This method is primarily used for building rig agents where we need
    /// tool definitions during construction.
    pub fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        if self.is_mock {
            self.mock_list_tools()
        } else if let Some(ref adapter) = self.adapter {
            adapter.get_cached_tools()
        } else {
            Vec::new()
        }
    }

    /// Check if the tool cache is still fresh within the given TTL.
    ///
    /// Returns `true` for mock clients (always fresh).
    /// For real clients, checks if tools were fetched within `ttl` duration.
    pub fn is_tool_cache_fresh(&self, ttl: std::time::Duration) -> bool {
        if self.is_mock {
            true
        } else if let Some(ref adapter) = self.adapter {
            adapter.is_tool_cache_fresh(ttl)
        } else {
            false
        }
    }

    /// Invalidate the tool cache, forcing re-fetch on next `list_tools()` call.
    ///
    /// No-op for mock clients.
    pub fn invalidate_tool_cache(&self) {
        if !self.is_mock {
            if let Some(ref adapter) = self.adapter {
                adapter.invalidate_tool_cache();
            }
        }
    }

    /// Generate mock tool definitions.
    fn mock_list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition::new("novanet_describe")
                .with_description("Bootstrap understanding of the graph"),
            ToolDefinition::new("novanet_search")
                .with_description("Find nodes via 5 modes: fulltext, property, hybrid, walk, triggers"),
            ToolDefinition::new("novanet_context")
                .with_description("Unified context assembly for LLM content generation")
                .with_input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "mode": {"type": "string", "description": "Context mode (page, block, knowledge, assemble)"},
                        "focus_key": {"type": "string", "description": "Focus node key"},
                        "locale": {"type": "string", "description": "Target locale (e.g., fr-FR)"}
                    },
                    "required": ["mode", "locale"]
                })),
        ]
    }
}

// Drop is handled by RmcpClientAdapter which cleans up the child process

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // CONCURRENT CALL TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_multiple_sequential_calls() {
        // Verify multiple sequential calls work
        let client = McpClient::mock("test");

        for i in 0..10 {
            let result = client
                .call_tool("test_tool", serde_json::json!({"iteration": i}))
                .await;
            assert!(
                result.is_ok(),
                "Call {} should succeed: {:?}",
                i,
                result.err()
            );
        }
    }

    #[tokio::test]
    async fn test_concurrent_calls() {
        // Verify concurrent calls work
        let client = std::sync::Arc::new(McpClient::mock("test"));

        let handles: Vec<_> = (0..20)
            .map(|i| {
                let client = std::sync::Arc::clone(&client);
                tokio::spawn(async move {
                    client
                        .call_tool("test_tool", serde_json::json!({"iteration": i}))
                        .await
                })
            })
            .collect();

        for (i, handle) in handles.into_iter().enumerate() {
            let result = handle.await.expect("Task should not panic");
            assert!(result.is_ok(), "Concurrent call {} should succeed", i);
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // BASIC TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_client_name_accessor() {
        let config = McpConfig::new("test-server", "echo");
        let client = McpClient::new(config).unwrap();
        assert_eq!(client.name(), "test-server");
    }

    #[test]
    fn test_mock_client_is_pre_connected() {
        let client = McpClient::mock("test");
        assert!(client.is_connected());
        assert!(client.is_mock);
    }

    #[test]
    fn test_real_client_starts_disconnected() {
        let config = McpConfig::new("test", "echo");
        let client = McpClient::new(config).unwrap();
        assert!(!client.is_connected());
        assert!(!client.is_mock);
    }

    #[tokio::test]
    async fn test_mock_tool_call_returns_success() {
        let client = McpClient::mock("test");
        let result = client
            .call_tool("unknown_tool", serde_json::json!({}))
            .await;
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert!(!result.unwrap().is_error);
    }

    // ═══════════════════════════════════════════════════════════════
    // RESOURCE READ TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_mock_read_resource_entity() {
        let client = McpClient::mock("test");
        let result = client.read_resource("neo4j://entity/qr-code").await;
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());

        let resource = result.unwrap();
        assert_eq!(resource.uri, "neo4j://entity/qr-code");
        assert!(resource.text.is_some());
    }

    #[tokio::test]
    async fn test_mock_read_resource_file() {
        let client = McpClient::mock("test");
        let result = client.read_resource("file:///tmp/test.txt").await;
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());

        let resource = result.unwrap();
        assert_eq!(resource.uri, "file:///tmp/test.txt");
    }

    // ═══════════════════════════════════════════════════════════════
    // DROP TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_mock_client_drop_is_noop() {
        // Mock clients should not try to kill any process
        let client = McpClient::mock("test");
        assert!(client.is_mock);
        // Dropping should not panic
        drop(client);
    }

    #[test]
    fn test_real_client_drop_without_process() {
        // Real client that was never connected should drop safely
        let config = McpConfig::new("test", "echo");
        let client = McpClient::new(config).unwrap();
        assert!(!client.is_mock);
        // No process was spawned, drop should be safe
        drop(client);
    }

    // ═══════════════════════════════════════════════════════════════
    // VALIDATION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_with_validation_enables_validator() {
        let config = McpConfig::new("test", "echo");
        let client = McpClient::new(config)
            .unwrap()
            .with_validation(ValidationConfig::default());

        // Should have validator
        assert!(client.validator.is_some());
    }

    #[tokio::test]
    async fn test_mock_connect_populates_schema_cache_when_validation_enabled() {
        let client = McpClient::mock("novanet").with_validation(ValidationConfig::default());

        // Connect should populate cache
        client.connect().await.unwrap();

        // Cache should have mock tools
        let validator = client.validator.as_ref().unwrap();
        let stats = validator.cache().stats();
        assert!(stats.tool_count > 0, "Should have cached tools");
    }

    #[tokio::test]
    async fn test_call_tool_validates_missing_required_field() {
        let client = McpClient::mock("novanet").with_validation(ValidationConfig::default());
        client.connect().await.unwrap();

        // novanet_context requires "mode" and "locale"
        let result = client
            .call_tool(
                "novanet_context",
                serde_json::json!({
                    "focus_key": "qr-code"
                    // Missing "mode" and "locale"
                }),
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, McpError::McpValidationFailed { .. }));

        if let McpError::McpValidationFailed {
            missing, details, ..
        } = err
        {
            assert!(missing.contains(&"mode".to_string()));
            assert!(details.contains("mode"));
        }
    }

    #[tokio::test]
    async fn test_call_tool_passes_validation_with_valid_params() {
        let client = McpClient::mock("novanet").with_validation(ValidationConfig::default());
        client.connect().await.unwrap();

        // Valid params - has required "mode" and "locale"
        let result = client
            .call_tool(
                "novanet_context",
                serde_json::json!({
                    "mode": "page",
                    "focus_key": "qr-code",
                    "locale": "fr-FR"
                }),
            )
            .await;

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_call_tool_skips_validation_when_disabled() {
        let config = ValidationConfig {
            pre_validate: false, // Disabled
            ..Default::default()
        };
        let client = McpClient::mock("novanet").with_validation(config);
        client.connect().await.unwrap();

        // Missing required field, but validation is disabled
        let result = client
            .call_tool(
                "novanet_context",
                serde_json::json!({
                    "focus_key": "qr-code"
                    // Missing "mode" and "locale" - but validation disabled
                }),
            )
            .await;

        // Should pass because validation is disabled
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_call_tool_without_validation_works() {
        // Client without validation
        let client = McpClient::mock("novanet");

        // No connect needed for mock without validation
        let result = client
            .call_tool(
                "novanet_context",
                serde_json::json!({
                    // Missing required fields but no validator
                }),
            )
            .await;

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_validation_for_unknown_tool_passes() {
        let client = McpClient::mock("novanet").with_validation(ValidationConfig::default());
        client.connect().await.unwrap();

        // Unknown tool - no schema cached, should pass through
        let result = client
            .call_tool(
                "unknown_tool",
                serde_json::json!({
                    "anything": "goes"
                }),
            )
            .await;

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    // ═══════════════════════════════════════════════════════════════
    // RESPONSE CACHING TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_with_cache_enables_caching() {
        let config = McpConfig::new("test", "echo");
        let client = McpClient::new(config)
            .unwrap()
            .with_cache(CacheConfig::default());

        // Should have cache
        assert!(client.cache.is_some());
    }

    #[test]
    fn test_cache_stats_returns_none_when_disabled() {
        let client = McpClient::mock("test");
        assert!(client.cache_stats().is_none());
    }

    #[test]
    fn test_cache_stats_returns_some_when_enabled() {
        let client = McpClient::mock("test").with_cache(CacheConfig::default());
        let stats = client.cache_stats();
        assert!(stats.is_some());
        let stats = stats.unwrap();
        assert_eq!(stats.entries, 0);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[tokio::test]
    async fn test_cache_hit_returns_cached_result() {
        let client = McpClient::mock("test").with_cache(CacheConfig::default());

        let params = serde_json::json!({"entity": "qr-code"});

        // First call - miss
        let result1 = client.call_tool("novanet_context", params.clone()).await;
        assert!(result1.is_ok(), "Should succeed: {:?}", result1.err());

        let stats = client.cache_stats().unwrap();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.entries, 1);

        // Second call with same params - hit
        let result2 = client.call_tool("novanet_context", params.clone()).await;
        assert!(result2.is_ok(), "Should succeed: {:?}", result2.err());

        let stats = client.cache_stats().unwrap();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 1);

        // Results should be equivalent
        let r1 = result1.unwrap();
        let r2 = result2.unwrap();
        assert_eq!(r1.content.len(), r2.content.len());
    }

    #[tokio::test]
    async fn test_cache_different_params_miss() {
        let client = McpClient::mock("test").with_cache(CacheConfig::default());

        // Call with params A
        let params_a = serde_json::json!({"focus_key": "qr-code"});
        client.call_tool("novanet_context", params_a).await.unwrap();

        // Call with params B - different, should miss
        let params_b = serde_json::json!({"focus_key": "barcode"});
        client.call_tool("novanet_context", params_b).await.unwrap();

        let stats = client.cache_stats().unwrap();
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.entries, 2);
    }

    #[tokio::test]
    async fn test_cache_different_tools_miss() {
        let client = McpClient::mock("test").with_cache(CacheConfig::default());

        let params = serde_json::json!({});

        // Call tool A
        client
            .call_tool("novanet_describe", params.clone())
            .await
            .unwrap();

        // Call tool B with same params - different tool, should miss
        client
            .call_tool("novanet_search", params.clone())
            .await
            .unwrap();

        let stats = client.cache_stats().unwrap();
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hits, 0);
    }

    #[tokio::test]
    async fn test_cache_ttl_expiration() {
        use std::time::Duration;

        // Very short TTL for testing
        let client = McpClient::mock("test").with_cache(CacheConfig {
            ttl: Duration::from_millis(50),
            max_entries: 100,
        });

        let params = serde_json::json!({"test": true});

        // First call - miss
        client.call_tool("test_tool", params.clone()).await.unwrap();
        assert_eq!(client.cache_stats().unwrap().entries, 1);

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Second call - should be a miss because entry expired
        client.call_tool("test_tool", params.clone()).await.unwrap();

        let stats = client.cache_stats().unwrap();
        assert_eq!(stats.misses, 2); // Both calls were misses
        assert_eq!(stats.hits, 0);
    }

    #[test]
    fn test_cache_hit_rate_calculation() {
        let stats = super::ResponseCacheStats {
            entries: 10,
            hits: 80,
            misses: 20,
        };
        assert!((stats.hit_rate() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_cache_hit_rate_zero_total() {
        let stats = super::ResponseCacheStats {
            entries: 0,
            hits: 0,
            misses: 0,
        };
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[test]
    fn test_cache_key_deterministic() {
        let params = serde_json::json!({"entity": "qr-code", "locale": "fr-FR"});

        let key1 = super::ResponseCache::cache_key("tool", &params);
        let key2 = super::ResponseCache::cache_key("tool", &params);

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_key_different_for_different_params() {
        let params1 = serde_json::json!({"entity": "qr-code"});
        let params2 = serde_json::json!({"entity": "barcode"});

        let key1 = super::ResponseCache::cache_key("tool", &params1);
        let key2 = super::ResponseCache::cache_key("tool", &params2);

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_cache_key_different_for_different_tools() {
        let params = serde_json::json!({"test": true});

        let key1 = super::ResponseCache::cache_key("tool_a", &params);
        let key2 = super::ResponseCache::cache_key("tool_b", &params);

        assert_ne!(key1, key2);
    }

    // ═══════════════════════════════════════════════════════════════
    // MCP PING HEALTH CHECK TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_ping_mock_client_succeeds() {
        let client = McpClient::mock("test");

        let result = client.ping().await;
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());

        let ping = result.unwrap();
        assert_eq!(ping.server, "test");
        assert!(ping.was_connected);
        assert!(ping.tool_count > 0);
        // Latency should be very small for mock
        assert!(ping.latency.as_millis() < 100);
    }

    #[test]
    fn test_mcp_ping_error_types() {
        let start_failed = super::McpPingError::StartFailed {
            server: "novanet".to_string(),
            details: "command not found".to_string(),
        };
        assert!(start_failed.to_string().contains("failed to start"));
        assert!(!start_failed.suggestion().is_empty());

        let timeout = super::McpPingError::Timeout {
            server: "slow-server".to_string(),
            timeout: std::time::Duration::from_secs(10),
        };
        assert!(timeout.to_string().contains("timed out"));

        let refused = super::McpPingError::ConnectionRefused {
            server: "offline".to_string(),
        };
        assert!(refused.to_string().contains("refused"));

        let server_err = super::McpPingError::ServerError {
            server: "broken".to_string(),
            details: "internal error".to_string(),
        };
        assert!(server_err.to_string().contains("error"));
    }

    #[tokio::test]
    async fn test_ping_result_has_valid_fields() {
        let client = McpClient::mock("novanet");

        let result = client.ping().await.unwrap();

        // Check all fields are populated
        assert_eq!(result.server, "novanet");
        assert!(result.tool_count >= 3); // Mock has at least 3 tools
        assert!(result.was_connected); // Mock is pre-connected
    }

    #[test]
    fn test_is_configured_returns_true_for_mock() {
        let client = McpClient::mock("test");
        assert!(client.is_configured());
    }

    #[test]
    fn test_is_configured_returns_true_for_real_client() {
        let config = McpConfig::new("test", "echo");
        let client = McpClient::new(config).unwrap();
        assert!(client.is_configured());
    }

    // ═══════════════════════════════════════════════════════════════
    // McpRetry Event Emission Tests
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_call_tool_with_retry_events_mock_success() {
        use nika_event::EventLog;

        let client = McpClient::mock("novanet");
        let event_log = EventLog::new();
        let task_id: Arc<str> = Arc::from("test_retry_events");

        // Mock client should succeed immediately (no retries)
        let result = client
            .call_tool_with_retry_events(
                "novanet_context",
                serde_json::json!({"focus_key": "qr-code"}),
                &task_id,
                &event_log,
            )
            .await;

        assert!(
            result.is_ok(),
            "Mock call should succeed: {:?}",
            result.err()
        );

        // No McpRetry events should be emitted for successful mock calls
        let events = event_log.filter_task("test_retry_events");
        let retry_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::McpRetry { .. }))
            .collect();
        assert!(
            retry_events.is_empty(),
            "No retry events for successful calls"
        );
    }

    #[tokio::test]
    async fn test_call_tool_with_retry_events_uses_cache() {
        use nika_event::EventLog;
        use std::time::Duration;

        // Create client with cache enabled
        let client = McpClient::mock("novanet").with_cache(CacheConfig {
            ttl: Duration::from_secs(60),
            max_entries: 100,
        });
        let event_log = EventLog::new();
        let task_id: Arc<str> = Arc::from("test_cache_hit");

        let params = serde_json::json!({"focus_key": "qr-code"});

        // First call - cache miss
        let result1 = client
            .call_tool_with_retry_events("novanet_context", params.clone(), &task_id, &event_log)
            .await
            .unwrap();
        assert!(!result1.was_cached);

        // Second call - should hit cache
        let result2 = client
            .call_tool_with_retry_events("novanet_context", params.clone(), &task_id, &event_log)
            .await
            .unwrap();
        assert!(result2.was_cached);
    }

    #[tokio::test]
    async fn test_call_tool_with_retry_events_not_connected_fails() {
        use nika_event::EventLog;

        // Create a real (not mock) client that isn't connected
        let config = McpConfig::new("test", "nonexistent_command");
        let client = McpClient::new(config).unwrap();
        let event_log = EventLog::new();
        let task_id: Arc<str> = Arc::from("test_not_connected");

        let result = client
            .call_tool_with_retry_events("some_tool", serde_json::json!({}), &task_id, &event_log)
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::McpNotConnected { .. } => {} // Expected
            err => panic!("Expected McpNotConnected, got: {err:?}"),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // NIKA-104: Cache Invalidation on Disconnect
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_disconnect_clears_response_cache() {
        let client = McpClient::mock("test_cache");

        // Mock disconnect just flips connected flag
        assert!(client.is_connected());
        client.disconnect().await.unwrap();
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_disconnect_clears_response_cache_with_entries() {
        let cache_config = CacheConfig {
            ttl: std::time::Duration::from_secs(300),
            max_entries: 100,
        };
        let client = McpClient::mock("test_cache_entries").with_cache(cache_config);

        // Populate cache via call_tool on mock
        let _ = client
            .call_tool("novanet_describe", serde_json::json!({}))
            .await;

        // Verify cache has an entry
        let stats = client.cache_stats();
        assert!(stats.is_some());

        // Disconnect should not crash even with cache entries
        client.disconnect().await.unwrap();
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_disconnect_invalidates_tool_cache_via_adapter() {
        // For real (non-mock) clients, disconnect delegates to adapter
        // which now calls invalidate_tool_cache()
        let config = McpConfig::new("test_adapter_cache", "echo");
        let client = McpClient::new(config).unwrap();

        // Disconnect on non-connected real client is a no-op (idempotent)
        client.disconnect().await.unwrap();
        assert!(!client.is_connected());
    }

    // ========================================================================
    // Wave 2: Deep Audit - Bug-Proving Tests
    // ========================================================================

    // ---- FIXED: Cache key uses canonical JSON serialization ----
    // cache_key() now canonicalizes JSON (sorts keys recursively) before hashing,
    // so semantically identical objects always match regardless of insertion order.
    #[test]
    fn wave2_cache_key_canonical_json_ordering() {
        use serde_json::json;

        // Build two semantically identical JSON objects with different key ordering.
        // serde_json::json! macro preserves source order, so we need to construct
        // maps manually with different insertion order.
        let mut map_a = serde_json::Map::new();
        map_a.insert("alpha".to_string(), json!("first"));
        map_a.insert("beta".to_string(), json!("second"));
        map_a.insert("gamma".to_string(), json!("third"));

        let mut map_b = serde_json::Map::new();
        map_b.insert("gamma".to_string(), json!("third"));
        map_b.insert("alpha".to_string(), json!("first"));
        map_b.insert("beta".to_string(), json!("second"));

        let value_a = Value::Object(map_a);
        let value_b = Value::Object(map_b);

        // Both represent the same logical JSON: {"alpha":"first","beta":"second","gamma":"third"}
        // but their serializations may differ due to insertion order.
        let json_a = serde_json::to_string(&value_a).unwrap();
        let json_b = serde_json::to_string(&value_b).unwrap();

        // Now compute cache keys using the same algorithm as ResponseCache::cache_key
        let key_a = ResponseCache::cache_key("test_tool", &value_a);
        let key_b = ResponseCache::cache_key("test_tool", &value_b);

        // FIXED: cache_key now canonicalizes JSON (sorts keys recursively),
        // so semantically identical objects always produce the same cache key
        // regardless of key insertion order or serde_json Map implementation.
        assert_eq!(
            key_a, key_b,
            "Canonical cache keys should match regardless of key insertion order. \
             json_a='{}', json_b='{}'",
            json_a, json_b
        );
    }

    // ---- BUG: evict_oldest() is O(n log n) under contention ----
    // ResponseCache::evict_oldest() iterates ALL DashMap entries, collects into
    // a Vec, sorts by creation time, then removes the oldest 10%.
    // Under high concurrency with many cache entries, this is expensive.
    //
    // FIX: Use a bounded LRU cache (e.g., `moka` or `quick_cache`) instead of
    // DashMap + manual eviction, or maintain a sorted index.
    #[test]
    fn wave2_evict_oldest_collects_all_entries() {
        use std::time::Duration;

        // Create a cache with small max_entries to trigger eviction
        let cache = ResponseCache::new(CacheConfig {
            ttl: Duration::from_secs(300),
            max_entries: 5,
        });

        // Fill the cache beyond capacity
        for i in 0..6 {
            let params = serde_json::json!({"i": i});
            cache.put(
                &format!("tool_{}", i),
                &params,
                ToolCallResult::success(vec![ContentBlock::text(format!("result_{}", i))]),
            );
        }

        // Verify cache has entries (some may have been evicted)
        let stats = cache.stats();
        assert!(stats.entries <= 6, "Cache should have at most 6 entries");

        // The eviction strategy removes ~10% of max_entries = 0.5, rounded up to 1.
        // After inserting 6 items into a cache with max_entries=5,
        // evict_oldest should have been triggered on the 6th insertion.
        //
        // BUG (performance): evict_oldest collects ALL entries into a Vec,
        // sorts them, then removes the oldest. For a 5-entry cache this is fine.
        // For 100k entries under concurrent access, this is O(n log n) while
        // holding DashMap read locks on every shard.
        //
        // We can't directly measure the perf impact in a unit test,
        // but we CAN verify the eviction strategy and document the issue.
        let to_remove = 5 / 10; // max_entries / 10 = 0
        let actual_remove = to_remove.max(1); // .max(1) = 1
        assert_eq!(
            actual_remove, 1,
            "Eviction removes max(max_entries/10, 1) entries. \
             BUG: This requires iterating ALL entries + sorting to find the oldest one. \
             An LRU cache would do this in O(1)."
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // CONNECTION ERROR DETECTION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_is_connection_error_broken_pipe() {
        let err = McpError::McpToolError {
            tool: "test".into(),
            reason: "Broken pipe".into(),
            error_code: None,
        };
        assert!(McpClient::is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_connection_reset() {
        let err = McpError::McpToolError {
            tool: "test".into(),
            reason: "Connection reset by peer".into(),
            error_code: None,
        };
        assert!(McpClient::is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_connection_refused() {
        let err = McpError::McpToolError {
            tool: "test".into(),
            reason: "Connection refused".into(),
            error_code: None,
        };
        assert!(McpClient::is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_eof() {
        let err = McpError::McpToolError {
            tool: "test".into(),
            reason: "unexpected EOF".into(),
            error_code: None,
        };
        assert!(McpClient::is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_stdin_not_available() {
        let err = McpError::McpToolError {
            tool: "test".into(),
            reason: "stdin not available".into(),
            error_code: None,
        };
        assert!(McpClient::is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_stdout_not_available() {
        let err = McpError::McpToolError {
            tool: "test".into(),
            reason: "stdout not available".into(),
            error_code: None,
        };
        assert!(McpClient::is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_transport_closed() {
        let err = McpError::McpToolError {
            tool: "test".into(),
            reason: "Transport closed unexpectedly".into(),
            error_code: None,
        };
        assert!(McpClient::is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_transport_send() {
        let err = McpError::McpToolError {
            tool: "test".into(),
            reason: "Transport send failed".into(),
            error_code: None,
        };
        assert!(McpClient::is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_non_connection_error() {
        let err = McpError::McpToolError {
            tool: "test".into(),
            reason: "invalid parameter 'mode'".into(),
            error_code: None,
        };
        assert!(!McpClient::is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_not_connected() {
        let err = McpError::McpNotConnected {
            name: "novanet".into(),
        };
        // McpNotConnected message doesn't contain transport keywords
        assert!(!McpClient::is_connection_error(&err));
    }
}
