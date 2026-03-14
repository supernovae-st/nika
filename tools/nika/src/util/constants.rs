//! Centralized constants for Nika runtime configuration
//!
//! All timeout and limit values in one place for easy tuning.

use std::time::Duration;

// ═══════════════════════════════════════════════════════════════
// Execution Timeouts
// ═══════════════════════════════════════════════════════════════

/// Timeout for shell command execution (exec: verb)
pub const EXEC_TIMEOUT: Duration = Duration::from_secs(60);

/// Timeout for HTTP requests (fetch: verb)
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for LLM inference calls (infer: verb, agent: verb)
pub const INFER_TIMEOUT: Duration = Duration::from_secs(120);

/// Timeout for establishing HTTP connections
/// Timeout increased for slow MCP server cold starts
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Timeout for MCP tool calls (invoke: verb)
/// Timeout increased for complex MCP operations
pub const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Total deadline for invoke task execution
/// Prevents N MCP calls × MCP_CALL_TIMEOUT from causing unbounded execution time.
/// A single invoke task with retries must complete within this deadline.
/// Set to 5 minutes to allow multiple retries while preventing runaway tasks.
pub const INVOKE_TASK_DEADLINE: Duration = Duration::from_secs(300);

/// Timeout for MCP reconnection attempts
/// Each reconnection attempt (disconnect + connect) has this timeout.
/// Prevents indefinite hanging when MCP servers become unresponsive.
pub const RECONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum number of MCP reconnection attempts
/// After this many failed reconnection attempts, the operation fails.
/// Prevents infinite retry loops on persistently broken connections.
#[allow(dead_code)] // API surface from spn fusion — not yet wired
pub const MAX_RECONNECT_ATTEMPTS: u32 = 3;

/// Timeout for decompose expansion (nested BFS traversal)
/// Prevents silent hangs during graph traversal
/// Set higher than MCP_CALL_TIMEOUT to allow multiple MCP calls in BFS loop
pub const DECOMPOSE_TIMEOUT: Duration = Duration::from_secs(120);

/// Timeout for complete MCP server initialization (connect + list_tools + overhead)
/// Prevents hanging on slow/unresponsive MCP servers during startup.
/// Should be > CONNECT_TIMEOUT + MCP_CALL_TIMEOUT to allow sequential operations.
/// Increased to match component timeouts
#[allow(dead_code)] // API surface from spn fusion — not yet wired
pub const MCP_INIT_TIMEOUT: Duration = Duration::from_secs(90);

/// Timeout for streaming chunk delivery (per-chunk, not total stream)
/// If no chunk arrives within this time, the stream is considered stalled.
pub const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(60);

/// Timeout for entire workflow execution (TUI mode)
#[allow(dead_code)] // API surface from spn fusion — not yet wired
pub const WORKFLOW_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

// ═══════════════════════════════════════════════════════════════
// MCP Cache TTL
// ═══════════════════════════════════════════════════════════════

/// TTL for cached MCP tool definitions.
/// After this duration, tool lists should be re-fetched from the server.
/// 5 minutes balances freshness with avoiding excessive list_tools() calls.
#[allow(dead_code)] // API surface from spn fusion — not yet wired
pub const TOOL_CACHE_TTL: Duration = Duration::from_secs(300);

// ═══════════════════════════════════════════════════════════════
// HTTP Client Limits
// ═══════════════════════════════════════════════════════════════

/// Maximum number of HTTP redirects to follow
pub const REDIRECT_LIMIT: usize = 5;

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeouts_are_positive() {
        assert!(EXEC_TIMEOUT.as_secs() > 0);
        assert!(FETCH_TIMEOUT.as_secs() > 0);
        assert!(INFER_TIMEOUT.as_secs() > 0);
        assert!(CONNECT_TIMEOUT.as_secs() > 0);
        assert!(MCP_CALL_TIMEOUT.as_secs() > 0);
        assert!(MCP_INIT_TIMEOUT.as_secs() > 0);
        assert!(STREAM_CHUNK_TIMEOUT.as_secs() > 0);
        assert!(WORKFLOW_TIMEOUT.as_secs() > 0);
        assert!(DECOMPOSE_TIMEOUT.as_secs() > 0);
        assert!(INVOKE_TASK_DEADLINE.as_secs() > 0);
    }

    #[test]
    fn mcp_init_timeout_is_longer_than_call_timeout() {
        // Init timeout should be > call timeout to allow for connect + list_tools
        assert!(MCP_INIT_TIMEOUT > MCP_CALL_TIMEOUT);
        assert!(MCP_INIT_TIMEOUT > CONNECT_TIMEOUT);
    }

    #[test]
    fn workflow_timeout_is_longest() {
        // Workflow execution can run for a long time
        assert!(WORKFLOW_TIMEOUT > INFER_TIMEOUT);
        assert!(WORKFLOW_TIMEOUT > EXEC_TIMEOUT);
        assert!(WORKFLOW_TIMEOUT > FETCH_TIMEOUT);
    }

    #[test]
    fn infer_timeout_is_longer_than_exec() {
        // LLM calls need more time than shell commands
        assert!(INFER_TIMEOUT > EXEC_TIMEOUT);
        assert!(INFER_TIMEOUT > FETCH_TIMEOUT);
    }

    #[test]
    fn connect_timeout_is_reasonable() {
        // Connection timeout increased for MCP cold starts
        // Should still be shorter than inference timeout
        assert!(CONNECT_TIMEOUT < INFER_TIMEOUT);
        assert!(CONNECT_TIMEOUT <= EXEC_TIMEOUT);
    }

    #[test]
    fn redirect_limit_is_reasonable() {
        // Not too many, not too few
        // Compile-time assertion via const block
        const _: () = {
            assert!(REDIRECT_LIMIT >= 3);
            assert!(REDIRECT_LIMIT <= 10);
        };
        // Runtime assertion for test visibility
        assert_eq!(REDIRECT_LIMIT, 5);
    }

    #[test]
    fn decompose_timeout_allows_multiple_mcp_calls() {
        // Decompose can make multiple MCP calls in BFS traversal
        // The overall timeout should be > single MCP call timeout
        assert!(DECOMPOSE_TIMEOUT > MCP_CALL_TIMEOUT);
    }

    // ═══════════════════════════════════════════════════════════════
    // Reconnection timeout tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn reconnect_timeout_is_30_seconds() {
        assert_eq!(RECONNECT_TIMEOUT.as_secs(), 30);
    }

    #[test]
    fn reconnect_timeout_exceeds_connect_timeout() {
        // Reconnect timeout should be >= connect timeout since reconnect
        // involves disconnect + connect operations
        assert!(RECONNECT_TIMEOUT >= CONNECT_TIMEOUT);
    }

    #[test]
    fn max_reconnect_attempts_is_3() {
        assert_eq!(MAX_RECONNECT_ATTEMPTS, 3);
    }

    // ═══════════════════════════════════════════════════════════════
    // Invoke task deadline tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn invoke_task_deadline_is_5_minutes() {
        assert_eq!(INVOKE_TASK_DEADLINE.as_secs(), 300);
    }

    #[test]
    fn invoke_task_deadline_exceeds_mcp_call_timeout() {
        // Task deadline should allow multiple MCP call retries
        // 5 minutes > 60 seconds means at least 4 retries possible
        assert!(INVOKE_TASK_DEADLINE > MCP_CALL_TIMEOUT);
        assert!(INVOKE_TASK_DEADLINE.as_secs() >= MCP_CALL_TIMEOUT.as_secs() * 4);
    }

    #[test]
    fn invoke_task_deadline_equals_workflow_timeout() {
        // Task deadline matches workflow timeout for consistency
        assert_eq!(INVOKE_TASK_DEADLINE, WORKFLOW_TIMEOUT);
    }
}
