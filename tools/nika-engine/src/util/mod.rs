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
        regex::Regex::new(concat!(
            r#"(?i)("#,
            r#"sk-[a-zA-Z0-9_-]{10,}"#,                       // OpenAI / Anthropic
            r#"|Bearer\s+[a-zA-Z0-9_.\-]{10,}"#,              // Bearer tokens
            r#"|ghp_[a-zA-Z0-9]{36}"#,                         // GitHub PAT
            r#"|gho_[a-zA-Z0-9]{36}"#,                         // GitHub OAuth
            r#"|gh[udr]_[a-zA-Z0-9]{36}"#,                     // GitHub user/device/refresh
            r#"|xox[bp]-[a-zA-Z0-9\-]+"#,                      // Slack
            r#"|AKIA[A-Z0-9]{16}"#,                             // AWS access key
            r#"|ASIA[A-Z0-9]{16}"#,                             // AWS temp credentials (STS)
            r#"|gsk_[a-zA-Z0-9]{20,}"#,                         // Groq
            r#"|AIza[a-zA-Z0-9_\-]{30,}"#,                      // Google API
            r#"|xai-[a-zA-Z0-9]{20,}"#,                         // xAI
            r#"|sk_live_[a-zA-Z0-9]{20,}"#,                     // Stripe live
            r#"|rk_live_[a-zA-Z0-9]{20,}"#,                     // Rokt
            r#"|whsec_[a-zA-Z0-9]{20,}"#,                       // Webhook secret
            r#"|SG\.[a-zA-Z0-9_-]{20,}"#,                       // SendGrid
            r#"|AC[a-f0-9]{32}"#,                                // Twilio SID
            r#"|eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}"#, // JWT
            r#"|(?:mongodb(?:\+srv)?|postgres(?:ql)?|mysql|redis)://[^\s,;'"#, r#"']+"#, // DB URIs
            r#")"#,
        )).expect("SECRET_RE is a valid regex")
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

    #[test]
    fn redact_secrets_aws_temp_creds() {
        let input = "ASIAVYXYZEXAMPLE12345";
        let result = redact_secrets(input);
        assert!(
            result.contains("[REDACTED]"),
            "AWS temp creds not redacted: {result}"
        );
        assert!(!result.contains("ASIAVYXYZ"));
    }

    #[test]
    fn redact_secrets_github_user_token() {
        let input = format!("token: ghu_{}", "A".repeat(36));
        let result = redact_secrets(&input);
        assert!(
            result.contains("[REDACTED]"),
            "GitHub user token not redacted: {result}"
        );
    }

    #[test]
    fn redact_secrets_github_device_token() {
        let input = format!("token: ghd_{}", "B".repeat(36));
        let result = redact_secrets(&input);
        assert!(
            result.contains("[REDACTED]"),
            "GitHub device token not redacted: {result}"
        );
    }

    #[test]
    fn redact_secrets_sendgrid() {
        let input = "SG.abcdefghij1234567890_-ABCD";
        let result = redact_secrets(input);
        assert!(
            result.contains("[REDACTED]"),
            "SendGrid key not redacted: {result}"
        );
    }

    #[test]
    fn redact_secrets_jwt() {
        let input = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.abc123def456";
        let result = redact_secrets(input);
        assert!(
            result.contains("[REDACTED]"),
            "JWT not redacted: {result}"
        );
        assert!(!result.contains("eyJhbGci"));
    }

    #[test]
    fn redact_secrets_is_idempotent() {
        let inputs = [
            "key=sk-proj-abc123def456ghi789jkl",
            "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij",
            "postgres://user:pass@host:5432/db",
            "AKIAIOSFODNN7EXAMPLE",
        ];
        for input in inputs {
            let once = redact_secrets(input);
            let twice = redact_secrets(&once);
            assert_eq!(once, twice, "NOT idempotent for: {input:?}");
        }
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
