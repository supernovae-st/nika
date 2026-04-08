// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP Client Pool
//!
//! Centralized lifecycle manager for MCP client connections.
//! Handles lazy initialization, deduplication, and graceful shutdown.
//!
//! ## Why not a traditional connection pool (bb8/deadpool)?
//!
//! MCP is 1-connection-per-server, not N-connections-per-server.
//! We need per-server lazy init + coordinated shutdown, not pool sizing.
//!
//! ## Thread Safety
//!
//! `McpClientPool` is `Clone + Send + Sync`. Cloning is cheap (Arc inner).
//! Multiple components (TaskExecutor, App, ChatAgent) share the same pool.
//!
//! ## Initialization Pattern
//!
//! Uses `DashMap<String, Arc<OnceCell<Arc<McpClient>>>>`:
//!
//! - **DashMap**: Concurrent access to different servers (shard-level locking)
//! - **OnceCell**: Per-server async init serialization
//!   - Only one task spawns the server process; others wait
//!   - If init fails, cell stays uninitialized for retry on next call
//! - **Arc wrapping**: Releases DashMap shard lock before awaiting

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use rustc_hash::FxHashMap;
use tokio::sync::OnceCell;

use crate::error::McpError;
use crate::types::McpConfig;
use crate::validation::ValidationConfig;
use crate::{McpClient, McpConfigInline};
use nika_event::{EventKind, EventLog};

/// Centralized MCP client lifecycle manager.
///
/// Provides lazy connection establishment, per-server deduplication,
/// and coordinated shutdown of all MCP server processes.
///
/// # Clone semantics
///
/// Cloning is cheap (Arc inner). All clones share the same pool state.
/// This enables sharing across TaskExecutor, TUI App, and ChatAgent.
///
/// # Example
///
/// ```rust,ignore
/// let pool = McpClientPool::with_configs(event_log, mcp_configs);
///
/// // Lazy connect (first call spawns server, subsequent return cached)
/// let client = pool.get_or_connect("neo4j").await?;
/// client.call_tool("novanet_search", params).await?;
///
/// // Graceful shutdown (disconnects all servers)
/// pool.shutdown_all().await;
/// ```
#[derive(Clone)]
pub struct McpClientPool {
    inner: Arc<PoolInner>,
}

struct PoolInner {
    /// Per-server lazy-initialized clients.
    ///
    /// ## Why `Arc<OnceCell<Arc<McpClient>>>`?
    ///
    /// 1. **DashMap shard lock**: `entry().or_insert_with()` holds shard lock.
    ///    We `.clone()` the `Arc<OnceCell>` to release it before awaiting.
    /// 2. **OnceCell serialization**: Only one task runs the init closure;
    ///    concurrent callers wait on the same future.
    /// 3. **Retry on failure**: If init fails, cell stays uninitialized
    ///    and next call retries (documented tokio::OnceCell behavior).
    clients: DashMap<String, Arc<OnceCell<Arc<McpClient>>>>,

    /// Server configurations (workflow-level or global).
    ///
    /// parking_lot::RwLock because configs are rarely mutated but read on every get_or_connect().
    configs: parking_lot::RwLock<FxHashMap<String, McpConfigInline>>,

    /// Event log for McpConnected/McpError observability events.
    event_log: EventLog,

    /// Shutdown flag. Once true, get_or_connect() returns Err immediately.
    is_shutdown: AtomicBool,
}

impl McpClientPool {
    /// Create an empty pool (no server configurations loaded yet).
    pub fn new(event_log: EventLog) -> Self {
        Self {
            inner: Arc::new(PoolInner {
                clients: DashMap::new(),
                configs: parking_lot::RwLock::new(FxHashMap::default()),
                event_log,
                is_shutdown: AtomicBool::new(false),
            }),
        }
    }

    /// Create a pool with pre-loaded server configurations.
    pub fn with_configs(event_log: EventLog, configs: FxHashMap<String, McpConfigInline>) -> Self {
        Self {
            inner: Arc::new(PoolInner {
                clients: DashMap::new(),
                configs: parking_lot::RwLock::new(configs),
                event_log,
                is_shutdown: AtomicBool::new(false),
            }),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // CONFIG MANAGEMENT
    // ═══════════════════════════════════════════════════════════════════════

    /// Replace all server configurations.
    ///
    /// Does NOT disconnect existing clients. Call `shutdown_all()` first
    /// if you need to force reconnection with new configs.
    pub fn set_configs(&self, configs: FxHashMap<String, McpConfigInline>) {
        *self.inner.configs.write() = configs;
    }

    /// Get a read reference to the current configs.
    pub fn configs(&self) -> parking_lot::RwLockReadGuard<'_, FxHashMap<String, McpConfigInline>> {
        self.inner.configs.read()
    }

    /// Check if a configuration exists for the given server name.
    pub fn has_config(&self, name: &str) -> bool {
        self.inner.configs.read().contains_key(name)
    }

    /// Return the number of configured MCP servers.
    pub fn config_count(&self) -> usize {
        self.inner.configs.read().len()
    }

    /// Get the event log.
    pub fn event_log(&self) -> &EventLog {
        &self.inner.event_log
    }

    // ═══════════════════════════════════════════════════════════════════════
    // CLIENT ACCESS (THE MAIN API)
    // ═══════════════════════════════════════════════════════════════════════

    /// Get an existing client or establish a new connection.
    ///
    /// This is the primary API. It:
    /// 1. Returns a cached client if already connected
    /// 2. Spawns the server process and connects if this is the first call
    /// 3. Serializes concurrent init attempts per server (OnceCell)
    /// 4. Retries automatically if a previous init attempt failed
    ///
    /// # Errors
    ///
    /// - `McpError::McpNotConfigured` if no config exists for this server
    /// - `McpError::McpStartError` if the server process fails to spawn
    /// - `McpError::McpStartError` if the pool is shut down
    pub async fn get_or_connect(&self, name: &str) -> Result<Arc<McpClient>, McpError> {
        // Fast path: reject if pool is shutting down
        if self.inner.is_shutdown.load(Ordering::SeqCst) {
            return Err(McpError::McpStartError {
                name: name.to_string(),
                reason: "MCP client pool is shut down".to_string(),
            });
        }

        // Single allocation reused for DashMap entry key and init closure.
        let name_owned = name.to_string();

        // Get or create the OnceCell for this server.
        // SAFETY: entry() holds a shard lock. The .clone() immediately releases it.
        // NEVER access self.inner.clients from within the OnceCell init closure.
        let cell = self
            .inner
            .clients
            .entry(name_owned.clone())
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone();

        // Capture Arc<PoolInner> for the async init closure.
        // We clone the Arc (cheap) to avoid borrowing self across await.
        let pool_inner = Arc::clone(&self.inner);

        // OnceCell::get_or_try_init ensures:
        // - Only one task runs the init closure
        // - Concurrent callers wait for the result
        // - If init fails, the cell stays empty for retry
        let client = cell
            .get_or_try_init(|| async {
                // Double-check shutdown inside init closure to close TOCTOU race:
                // A task could pass the outer check, then shutdown_all() runs,
                // then this closure starts — we must reject here too.
                if pool_inner.is_shutdown.load(Ordering::SeqCst) {
                    return Err(McpError::McpStartError {
                        name: name_owned.clone(),
                        reason: "MCP client pool is shut down".to_string(),
                    });
                }
                Self::connect_server(&pool_inner.configs, &pool_inner.event_log, &name_owned).await
            })
            .await?;

        Ok(Arc::clone(client))
    }

    /// Internal: spawn and connect to an MCP server.
    async fn connect_server(
        configs: &parking_lot::RwLock<FxHashMap<String, McpConfigInline>>,
        event_log: &EventLog,
        name: &str,
    ) -> Result<Arc<McpClient>, McpError> {
        // Read config (hold read lock only briefly)
        let config = {
            let guard = configs.read();
            guard.get(name).cloned()
        };

        let config = config.ok_or_else(|| McpError::McpNotConfigured {
            name: name.to_string(),
        })?;

        // Build McpConfig from inline config
        let mut mcp_config = McpConfig::new(name, &config.command);
        for arg in &config.args {
            mcp_config = mcp_config.with_arg(arg);
        }
        for (key, value) in &config.env {
            mcp_config = mcp_config.with_env(key, value);
        }
        if let Some(cwd) = &config.cwd {
            mcp_config = mcp_config.with_cwd(cwd);
        }

        // Expand environment variables ($VAR, ${VAR}, ~) in command/args/env/cwd
        let mcp_config = mcp_config
            .expand_env_vars()
            .map_err(|e| McpError::McpStartError {
                name: name.to_string(),
                reason: format!("Environment variable expansion failed: {}", e),
            })?;

        // Create with validation enabled and connect
        let client = McpClient::new(mcp_config)
            .map_err(|e| McpError::McpStartError {
                name: name.to_string(),
                reason: e.to_string(),
            })?
            .with_validation(ValidationConfig::default());

        match client.connect().await {
            Ok(()) => {
                // Cache tools for synchronous get_tool_definitions() access
                if let Err(e) = client.list_tools().await {
                    tracing::warn!(mcp_server = %name, error = %e, "Failed to cache tools");
                }

                tracing::info!(mcp_server = %name, "Connected to MCP server");
                event_log.emit(EventKind::McpConnected {
                    server_name: name.to_string(),
                });

                Ok(Arc::new(client))
            }
            Err(e) => {
                let error_msg = e.to_string();
                event_log.emit(EventKind::McpError {
                    server_name: name.to_string(),
                    error: error_msg.clone(),
                });

                Err(McpError::McpStartError {
                    name: name.to_string(),
                    reason: error_msg,
                })
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // INSPECTION
    // ═══════════════════════════════════════════════════════════════════════

    /// Check if a server has an active (initialized) connection.
    pub fn is_connected(&self, name: &str) -> bool {
        self.inner
            .clients
            .get(name)
            .and_then(|cell| cell.get().map(|_| true))
            .unwrap_or(false)
    }

    /// Count of servers with active connections.
    pub fn connected_count(&self) -> usize {
        self.inner
            .clients
            .iter()
            .filter(|entry| entry.value().get().is_some())
            .count()
    }

    /// Check if the pool has been shut down.
    pub fn is_shutdown(&self) -> bool {
        self.inner.is_shutdown.load(Ordering::SeqCst)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // LIFECYCLE
    // ═══════════════════════════════════════════════════════════════════════

    /// Disconnect a specific server and remove it from the pool.
    ///
    /// Always removes the entry to prevent dangling OnceCell references,
    /// even if disconnect fails. The next call to `get_or_connect()` will re-initialize.
    pub async fn disconnect(&self, name: &str) -> Result<(), McpError> {
        // Attempt disconnect, capturing any error
        let disconnect_err = if let Some(cell) = self.inner.clients.get(name) {
            if let Some(client) = cell.get() {
                client.disconnect().await.err()
            } else {
                None
            }
        } else {
            None
        };

        // Always remove to prevent dangling entries with spent OnceCell
        self.inner.clients.remove(name);

        if let Some(e) = disconnect_err {
            return Err(e);
        }
        Ok(())
    }

    /// Gracefully shut down all MCP server connections.
    ///
    /// After this call:
    /// - All server processes are terminated
    /// - The pool is marked as shut down
    /// - `get_or_connect()` will return Err for all subsequent calls
    ///
    /// This method is idempotent.
    pub async fn shutdown_all(&self) {
        // 1. Set shutdown flag to reject new connections
        self.inner.is_shutdown.store(true, Ordering::SeqCst);

        // 2. Drain all clients from the map
        let entries: Vec<(String, Arc<OnceCell<Arc<McpClient>>>)> = self
            .inner
            .clients
            .iter()
            .map(|entry| (entry.key().clone(), Arc::clone(entry.value())))
            .collect();

        self.inner.clients.clear();

        // 3. Disconnect each initialized client with timeout
        for (name, cell) in entries {
            if let Some(client) = cell.get() {
                let disconnect_result =
                    tokio::time::timeout(Duration::from_secs(5), client.disconnect()).await;

                match disconnect_result {
                    Ok(Ok(())) => {
                        tracing::debug!(server = %name, "MCP server disconnected");
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(server = %name, error = %e, "Error disconnecting MCP server");
                    }
                    Err(_) => {
                        tracing::warn!(server = %name, "MCP server disconnect timed out (5s)");
                    }
                }
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TESTING
    // ═══════════════════════════════════════════════════════════════════════

    /// Inject a pre-built client for testing.
    ///
    /// The client is inserted as already-initialized, bypassing connect_server().
    /// Only intended for test code (production callers use connect_server).
    pub fn inject_mock(&self, name: &str, client: Arc<McpClient>) {
        let cell = Arc::new(OnceCell::new());
        // OnceCell is freshly created so set() always succeeds
        let _ = cell.set(client);
        self.inner.clients.insert(name.to_string(), cell);
    }
}

impl std::fmt::Debug for McpClientPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClientPool")
            .field("connected", &self.connected_count())
            .field("configured", &self.inner.configs.read().len())
            .field("is_shutdown", &self.is_shutdown())
            .finish()
    }
}

// Compile-time assertion: McpClientPool must be Send + Sync + Clone.
// If a future change introduces a !Send or !Sync field, this fails at definition site.
const _: () = {
    fn _assert_send_sync_clone<T: Send + Sync + Clone>() {}
    fn _check() {
        _assert_send_sync_clone::<McpClientPool>();
    }
};

#[cfg(test)]
mod tests {
    use super::*;
    use nika_event::EventLog;

    #[test]
    fn test_pool_new_is_empty() {
        let pool = McpClientPool::new(EventLog::new());
        assert_eq!(pool.connected_count(), 0);
        assert!(!pool.is_shutdown());
    }

    #[test]
    fn test_pool_with_configs() {
        let mut configs = FxHashMap::default();
        configs.insert(
            "test".to_string(),
            McpConfigInline {
                command: "echo".to_string(),
                args: vec![],
                env: FxHashMap::default(),
                cwd: None,
            },
        );

        let pool = McpClientPool::with_configs(EventLog::new(), configs);
        assert!(pool.has_config("test"));
        assert!(!pool.has_config("missing"));
    }

    #[test]
    fn test_pool_clone_shares_state() {
        let pool1 = McpClientPool::new(EventLog::new());
        let pool2 = pool1.clone();

        let mock = Arc::new(McpClient::mock("test"));
        pool1.inject_mock("test", mock);

        // pool2 should see the same client
        assert!(pool2.is_connected("test"));
    }

    #[test]
    fn test_pool_is_connected_false_when_empty() {
        let pool = McpClientPool::new(EventLog::new());
        assert!(!pool.is_connected("neo4j"));
    }

    #[test]
    fn test_pool_inject_mock() {
        let pool = McpClientPool::new(EventLog::new());
        let mock = Arc::new(McpClient::mock("novanet"));
        pool.inject_mock("novanet", mock);

        assert!(pool.is_connected("novanet"));
        assert_eq!(pool.connected_count(), 1);
    }

    #[tokio::test]
    async fn test_pool_get_or_connect_with_mock() {
        let pool = McpClientPool::new(EventLog::new());
        let mock = Arc::new(McpClient::mock("novanet"));
        pool.inject_mock("novanet", mock);

        let client = pool.get_or_connect("novanet").await.unwrap();
        assert!(client.is_connected());
        assert_eq!(client.name(), "novanet");
    }

    #[tokio::test]
    async fn test_pool_get_or_connect_not_configured() {
        let pool = McpClientPool::new(EventLog::new());
        let result = pool.get_or_connect("missing").await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("not configured"),
            "Expected McpNotConfigured error"
        );
    }

    #[tokio::test]
    async fn test_pool_shutdown_rejects_new_connections() {
        let pool = McpClientPool::new(EventLog::new());
        pool.shutdown_all().await;

        assert!(pool.is_shutdown());
        let result = pool.get_or_connect("test").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("shut down"));
    }

    #[tokio::test]
    async fn test_pool_disconnect_single_server() {
        let pool = McpClientPool::new(EventLog::new());
        let mock = Arc::new(McpClient::mock("test"));
        pool.inject_mock("test", mock);

        assert!(pool.is_connected("test"));
        pool.disconnect("test").await.unwrap();
        assert!(!pool.is_connected("test"));
    }

    #[tokio::test]
    async fn test_pool_shutdown_clears_all() {
        let pool = McpClientPool::new(EventLog::new());
        pool.inject_mock("a", Arc::new(McpClient::mock("a")));
        pool.inject_mock("b", Arc::new(McpClient::mock("b")));
        assert_eq!(pool.connected_count(), 2);

        pool.shutdown_all().await;
        assert_eq!(pool.connected_count(), 0);
        assert!(pool.is_shutdown());
    }

    #[test]
    fn test_pool_set_configs() {
        let pool = McpClientPool::new(EventLog::new());
        assert!(!pool.has_config("neo4j"));

        let mut configs = FxHashMap::default();
        configs.insert(
            "neo4j".to_string(),
            McpConfigInline {
                command: "npx".to_string(),
                args: vec![],
                env: FxHashMap::default(),
                cwd: None,
            },
        );
        pool.set_configs(configs);
        assert!(pool.has_config("neo4j"));
    }

    #[test]
    fn test_pool_debug_format() {
        let pool = McpClientPool::new(EventLog::new());
        let debug = format!("{:?}", pool);
        assert!(debug.contains("McpClientPool"));
        assert!(debug.contains("connected: 0"));
    }
}
