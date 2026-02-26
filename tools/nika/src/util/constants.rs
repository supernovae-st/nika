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
/// v0.12.1: Increased from 10s to 20s for slow MCP server cold starts
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Timeout for MCP tool calls (invoke: verb)
/// v0.12.1: Increased from 30s to 60s for complex MCP operations
pub const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// v0.8.5: Timeout for complete MCP server initialization (connect + list_tools + overhead)
/// Prevents hanging on slow/unresponsive MCP servers during startup.
/// Should be > CONNECT_TIMEOUT + MCP_CALL_TIMEOUT to allow sequential operations.
/// v0.12.1: Increased from 45s to 90s to match increased component timeouts
pub const MCP_INIT_TIMEOUT: Duration = Duration::from_secs(90);

/// Timeout for streaming chunk delivery (per-chunk, not total stream)
/// If no chunk arrives within this time, the stream is considered stalled.
pub const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(60);

/// Timeout for entire workflow execution (TUI mode)
pub const WORKFLOW_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

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
    }

    #[test]
    fn mcp_init_timeout_is_longer_than_call_timeout() {
        // v0.8.5: Init timeout should be > call timeout to allow for connect + list_tools
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
        // v0.12.1: Connection timeout increased for MCP cold starts
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
}
