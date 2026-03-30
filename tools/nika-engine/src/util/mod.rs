//! Utilities Module - shared infrastructure
//!
//! Contains helper functions and data structures used across the codebase:
//! - `constants`: Centralized timeouts and limits
//! - `fs`: Atomic file write operations
//! - `interner`: String interning for recurring task IDs (`Arc<str>` deduplication)
//! - `system`: Platform-specific system information (RAM detection, etc.)

pub mod constants;
pub mod fs;
mod interner;
pub mod system;

// Re-export actively used types
pub use constants::{
    CONNECT_TIMEOUT, DECOMPOSE_TIMEOUT, EXEC_TIMEOUT, FETCH_TIMEOUT, INVOKE_TASK_DEADLINE,
    REDIRECT_LIMIT, STREAM_CHUNK_TIMEOUT,
};
// MCP_CALL_TIMEOUT and RECONNECT_TIMEOUT moved to nika-mcp crate
pub use fs::{atomic_write, check_preview_size, format_size};
pub use interner::intern;

/// Redact known API key / secret patterns from a string.
///
/// Matches common secret formats (OpenAI, Anthropic, GitHub, Slack, AWS, Groq,
/// Google, xAI, Stripe, Twilio, database URIs) and replaces with `[REDACTED]`.
/// Used by tracing, event logging, and security modules to avoid leaking secrets.
pub fn redact_secrets(s: &str) -> String {
    use std::sync::LazyLock;
    static SECRET_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r#"(?i)(sk-[a-zA-Z0-9_-]{10,}|Bearer\s+[a-zA-Z0-9_.\-]{10,}|ghp_[a-zA-Z0-9]{36}|gho_[a-zA-Z0-9]{36}|xox[bp]-[a-zA-Z0-9\-]+|AKIA[A-Z0-9]{16}|gsk_[a-zA-Z0-9]{20,}|AIza[a-zA-Z0-9_\-]{30,}|xai-[a-zA-Z0-9]{20,}|sk_live_[a-zA-Z0-9]{20,}|rk_live_[a-zA-Z0-9]{20,}|whsec_[a-zA-Z0-9]{20,}|AC[a-f0-9]{32}|(?:mongodb(?:\+srv)?|postgres(?:ql)?|mysql|redis)://[^\s,;'"]+)"#
        ).expect("SECRET_RE is a valid regex")
    });
    SECRET_RE.replace_all(s, "[REDACTED]").into_owned()
}

/// Truncate a string at a valid UTF-8 char boundary.
///
/// Returns a slice of at most `max_bytes` bytes, ending at a char boundary.
/// Avoids panics from byte-slicing multi-byte UTF-8 sequences (CJK, emoji, etc.).
///
/// # Example
/// ```ignore
/// let s = "こんにちは世界"; // 21 bytes
/// assert_eq!(truncate_str(s, 10), "こんに"); // 9 bytes, safe boundary
/// ```
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_secrets_openai_key() {
        let input = "key=sk-proj-abc123def456ghi789jkl";
        let result = redact_secrets(input);
        assert!(
            result.contains("[REDACTED]"),
            "OpenAI key not redacted: {result}"
        );
        assert!(!result.contains("sk-proj"), "Key prefix leaked: {result}");
    }

    #[test]
    fn redact_secrets_anthropic_key() {
        let input = "Authorization: sk-ant-api03-abcdefghij1234567890";
        let result = redact_secrets(input);
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("sk-ant"));
    }

    #[test]
    fn redact_secrets_bearer_token() {
        let input = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let result = redact_secrets(input);
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("eyJhbGci"));
    }

    #[test]
    fn redact_secrets_stripe_key() {
        // Construct at runtime to avoid GitHub push protection false positive
        let input = format!("Stripe key: sk_live_{}", "a".repeat(24));
        let result = redact_secrets(&input);
        assert!(
            result.contains("[REDACTED]"),
            "Stripe key not redacted: {result}"
        );
        assert!(!result.contains("sk_live_"));
    }

    #[test]
    fn redact_secrets_twilio_sid() {
        // Construct at runtime to avoid GitHub push protection false positive
        let input = format!("SID: AC{}", "0".repeat(32));
        let result = redact_secrets(&input);
        assert!(
            result.contains("[REDACTED]"),
            "Twilio SID not redacted: {result}"
        );
        assert!(!result.contains("AC00000"));
    }

    #[test]
    fn redact_secrets_database_uri() {
        let input = "DSN: postgres://user:pass@host:5432/db";
        let result = redact_secrets(input);
        assert!(
            result.contains("[REDACTED]"),
            "DB URI not redacted: {result}"
        );
        assert!(!result.contains("postgres://"));
    }

    #[test]
    fn redact_secrets_mongodb_srv() {
        let input = "mongodb+srv://admin:secret@cluster.mongodb.net/db";
        let result = redact_secrets(input);
        assert!(
            result.contains("[REDACTED]"),
            "MongoDB URI not redacted: {result}"
        );
    }

    #[test]
    fn redact_secrets_preserves_safe_strings() {
        let input = "echo hello world";
        assert_eq!(redact_secrets(input), input);
    }

    #[test]
    fn redact_secrets_github_pat() {
        let input = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let result = redact_secrets(input);
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("ghp_"));
    }

    #[test]
    fn redact_secrets_aws_access_key() {
        let input = "AKIAIOSFODNN7EXAMPLE";
        let result = redact_secrets(input);
        assert!(
            result.contains("[REDACTED]"),
            "AWS key not redacted: {result}"
        );
    }

    #[test]
    fn redact_secrets_webhook_secret() {
        let input = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
        let result = redact_secrets(input);
        assert!(
            result.contains("[REDACTED]"),
            "Webhook secret not redacted: {result}"
        );
    }
}

pub fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last char boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
