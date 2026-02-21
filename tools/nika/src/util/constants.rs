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
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout for MCP tool calls (invoke: verb)
pub const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(30);

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
    }

    #[test]
    fn infer_timeout_is_longest() {
        // LLM calls need more time than other operations
        assert!(INFER_TIMEOUT > EXEC_TIMEOUT);
        assert!(INFER_TIMEOUT > FETCH_TIMEOUT);
    }

    #[test]
    fn connect_timeout_is_shortest() {
        // Connection establishment should be fast
        assert!(CONNECT_TIMEOUT < EXEC_TIMEOUT);
        assert!(CONNECT_TIMEOUT < FETCH_TIMEOUT);
        assert!(CONNECT_TIMEOUT < INFER_TIMEOUT);
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
