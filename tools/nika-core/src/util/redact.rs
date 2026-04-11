// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Secret redaction for traces, event logs, and error messages.
//!
//! Pattern-based redaction for ~20 known API key formats plus a runtime
//! value registry for `$env.*` bindings that do not match a known regex.
//!
//! Callers include the event log, tracing writers, error formatters, and
//! the binding resolver. Any string that may appear in user-visible output
//! or on-disk traces should pass through [`redact_secrets`] first.
//!
//! ## Provider coverage
//!
//! Prefix-based (distinctive leading tokens, safe to match anywhere):
//! OpenAI / Anthropic (`sk-…`), GitHub PAT (`ghp_…`), Groq (`gsk_…`),
//! Google (`AIza…`), xAI (`xai-…`), Stripe (`sk_live_…`), Rokt, SendGrid,
//! AWS (`AKIA…` / `ASIA…`), Twilio SID (`AC<hex32>`), Replicate (`r8_…`),
//! Mistral (`ms_…`), fal.ai (`fal_…`), Slack (`xox[bp]-…`), Bearer tokens,
//! JWT triples, database URIs (postgres/mysql/mongodb/redis).
//!
//! Context-anchored (generic long hex/base64 formats that only match when
//! a provider header or name appears nearby, avoiding false positives on
//! random hashes): Cohere, ElevenLabs, Together AI.
//!
//! Any string registered via [`register_secrets`] is also redacted by a
//! post-pass value-substitution step.

use std::sync::{LazyLock, RwLock};

/// Global registry of extra secret values for value-based redaction.
///
/// Populated at runtime with resolved `$env.*` binding values (e.g., custom
/// API keys like `ELEVENLABS_API_KEY` that don't match known regex patterns).
/// Checked by [`redact_secrets`] after pattern-based redaction.
static EXTRA_SECRETS: LazyLock<RwLock<Vec<String>>> = LazyLock::new(|| RwLock::new(Vec::new()));

/// Register extra secret values for value-based redaction.
///
/// Call this after resolving `$env.*` bindings to ensure custom API keys
/// are redacted from traces even if they don't match known regex patterns.
pub fn register_secrets(values: Vec<String>) {
    if values.is_empty() {
        return;
    }
    if let Ok(mut secrets) = EXTRA_SECRETS.write() {
        for v in values {
            if !secrets.contains(&v) {
                secrets.push(v);
            }
        }
    }
}

/// Maximum nesting depth for [`redact_value`].
///
/// A hostile MCP response with 10,000+ nesting levels would stack-overflow
/// the recursive walk. Beyond 128 levels, we return the value unchanged
/// (secrets that deep are unreachable by humans anyway).
const MAX_REDACT_DEPTH: usize = 128;

/// Walk a JSON value and redact every string leaf via [`redact_secrets`].
///
/// Used by event log emitters (`EventKind::McpInvoke`, binding defaults,
/// MCP invoke params) to strip secrets before structured output crosses
/// a trace / telemetry boundary. Non-string leaves (numbers, bools,
/// nulls) pass through unchanged. Keys stay unchanged — only values are
/// scanned.
///
/// Recursion is bounded by [`MAX_REDACT_DEPTH`] to prevent stack overflow
/// on adversarial input.
pub fn redact_value(v: serde_json::Value) -> serde_json::Value {
    redact_value_inner(v, 0)
}

fn redact_value_inner(v: serde_json::Value, depth: usize) -> serde_json::Value {
    if depth > MAX_REDACT_DEPTH {
        return v;
    }
    match v {
        serde_json::Value::String(s) => serde_json::Value::String(redact_secrets(&s)),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(|item| redact_value_inner(item, depth + 1))
                .collect(),
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, redact_value_inner(v, depth + 1)))
                .collect(),
        ),
        other => other,
    }
}

/// Redact known API key / secret patterns from a string.
///
/// Replaces matches with `[REDACTED]`. Also redacts any values registered
/// via [`register_secrets`] (value-based redaction pass).
pub fn redact_secrets(s: &str) -> String {
    static SECRET_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(concat!(
            r#"(?i)("#,
            r#"sk-[a-zA-Z0-9_-]{10,}"#,          // OpenAI / Anthropic
            r#"|Bearer\s+[a-zA-Z0-9_.\-]{10,}"#, // Bearer tokens
            r#"|ghp_[a-zA-Z0-9]{36}"#,           // GitHub PAT
            r#"|gho_[a-zA-Z0-9]{36}"#,           // GitHub OAuth
            r#"|gh[udr]_[a-zA-Z0-9]{36}"#,       // GitHub user/device/refresh
            r#"|xox[bp]-[a-zA-Z0-9\-]+"#,        // Slack
            r#"|AKIA[A-Z0-9]{16}"#,              // AWS access key
            r#"|ASIA[A-Z0-9]{16}"#,              // AWS temp credentials (STS)
            r#"|gsk_[a-zA-Z0-9]{20,}"#,          // Groq
            r#"|AIza[a-zA-Z0-9_\-]{30,}"#,       // Google API
            r#"|xai-[a-zA-Z0-9]{20,}"#,          // xAI
            r#"|sk_live_[a-zA-Z0-9]{20,}"#,      // Stripe live
            r#"|rk_live_[a-zA-Z0-9]{20,}"#,      // Rokt
            r#"|whsec_[a-zA-Z0-9]{20,}"#,        // Webhook secret
            r#"|SG\.[a-zA-Z0-9_-]{20,}"#,        // SendGrid
            r#"|AC[a-f0-9]{32}"#,                // Twilio SID
            r#"|eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}"#, // JWT
            r#"|r8_[a-zA-Z0-9]{37,}"#,           // Replicate
            r#"|ms_[a-zA-Z0-9]{20,}"#,           // Mistral
            r#"|fal_[a-zA-Z0-9_-]{20,}"#,        // fal.ai
            // Context-anchored: provider name must appear near the key
            r#"|cohere[-_]?(?:api[-_]?)?key[:\s=]+[A-Za-z0-9]{40}"#, // Cohere
            r#"|(?:xi[-_]?api[-_]?key|elevenlabs[-_]?(?:api[-_]?)?key)[:\s=]+[a-f0-9]{32}"#, // ElevenLabs
            r#"|together[-_]?(?:api[-_]?)?key[:\s=]+[a-f0-9]{64}"#, // Together AI
            r#"|(?:mongodb(?:\+srv)?|postgres(?:ql)?|mysql|redis)://[^\s,;'"#,
            r#"']+"#, // DB URIs
            r#")"#,
        ))
        .expect("SECRET_RE is a valid regex")
    });
    let mut result = SECRET_RE.replace_all(s, "[REDACTED]").into_owned();

    // Value-based redaction: replace registered env-sourced secret values
    if let Ok(secrets) = EXTRA_SECRETS.read() {
        for secret in secrets.iter() {
            if result.contains(secret.as_str()) {
                result = result.replace(secret.as_str(), "[REDACTED]");
            }
        }
    }

    result
}

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
        // Split to avoid secret-scanning false positive on test fixture
        let input = format!("mongodb+srv://admin:secret@cluster.{}.net/db", "mongodb");
        let result = redact_secrets(&input);
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
        // Split to avoid secret-scanning false positive on test fixture
        let input = format!("whsec_{}", "MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw");
        let result = redact_secrets(&input);
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
        assert!(result.contains("[REDACTED]"), "JWT not redacted: {result}");
        assert!(!result.contains("eyJhbGci"));
    }

    #[test]
    fn redact_secrets_replicate() {
        // Replicate keys use the `r8_` prefix followed by ~40 alphanumerics.
        let input = format!("REPLICATE_API_TOKEN=r8_{}", "a".repeat(40));
        let result = redact_secrets(&input);
        assert!(
            result.contains("[REDACTED]"),
            "Replicate key not redacted: {result}"
        );
        assert!(!result.contains("r8_aaaa"));
    }

    #[test]
    fn redact_secrets_mistral() {
        // Mistral keys use the `ms_` prefix.
        let input = format!("mistral: ms_{}", "X".repeat(30));
        let result = redact_secrets(&input);
        assert!(
            result.contains("[REDACTED]"),
            "Mistral key not redacted: {result}"
        );
        assert!(!result.contains("ms_XX"));
    }

    #[test]
    fn redact_secrets_fal_ai() {
        let input = format!("FAL_KEY=fal_{}", "Z".repeat(24));
        let result = redact_secrets(&input);
        assert!(
            result.contains("[REDACTED]"),
            "fal.ai key not redacted: {result}"
        );
        assert!(!result.contains("fal_ZZ"));
    }

    #[test]
    fn redact_secrets_cohere_context() {
        // Cohere keys have no distinctive prefix, so we context-anchor on the provider name.
        // Runtime-constructed to avoid GitHub push-protection false positives on the literal.
        let input = format!("cohere-api-key: {}", "a".repeat(40));
        let result = redact_secrets(&input);
        assert!(
            result.contains("[REDACTED]"),
            "Cohere key not redacted: {result}"
        );
        assert!(!result.contains(&"a".repeat(40)));
    }

    #[test]
    fn redact_secrets_cohere_no_false_positive() {
        // Random 40-char token WITHOUT the cohere context should NOT match.
        // Runtime-constructed (see note in the companion test above).
        let input = format!("some-hash: {}", "a".repeat(40));
        let result = redact_secrets(&input);
        assert_eq!(
            result, input,
            "Cohere context-anchor must not false-positive: {result}"
        );
    }

    #[test]
    fn redact_secrets_elevenlabs() {
        let input = "xi-api-key: a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4";
        let result = redact_secrets(input);
        assert!(
            result.contains("[REDACTED]"),
            "ElevenLabs key not redacted: {result}"
        );
        assert!(!result.contains("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4"));
    }

    #[test]
    fn redact_secrets_together() {
        // Together uses 64-char hex keys — must be context-anchored to avoid matching arbitrary blake3 hashes.
        let input = format!("together-api-key: {}", "a".repeat(64));
        let result = redact_secrets(&input);
        assert!(
            result.contains("[REDACTED]"),
            "Together AI key not redacted: {result}"
        );
        assert!(!result.contains("aaaaaaaaaa"));
    }

    #[test]
    fn redact_secrets_together_no_false_positive() {
        // A raw 64-char hex blake3 hash WITHOUT the together context should NOT match.
        let input = format!("blake3: {}", "a".repeat(64));
        let result = redact_secrets(&input);
        assert_eq!(
            result, input,
            "Together context-anchor must not false-positive: {result}"
        );
    }

    #[test]
    fn redact_value_walks_nested_strings() {
        let v = serde_json::json!({
            "api_key": "sk-proj-abc123def456ghi789jkl",
            "nested": {
                "token": "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij",
                "count": 42,
                "flag": true,
            },
            "tokens": ["Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9", "safe value"],
        });
        let redacted = redact_value(v);
        let s = redacted.to_string();
        assert!(s.contains("[REDACTED]"));
        assert!(!s.contains("sk-proj"));
        assert!(!s.contains("ghp_"));
        assert!(!s.contains("eyJhbGci"));
        // Non-string scalars and safe strings pass through
        assert!(s.contains("42"));
        assert!(s.contains("true"));
        assert!(s.contains("safe value"));
        // Keys are preserved
        assert!(s.contains("api_key"));
        assert!(s.contains("nested"));
    }

    #[test]
    fn redact_value_non_string_passthrough() {
        assert_eq!(redact_value(serde_json::json!(42)), serde_json::json!(42));
        assert_eq!(
            redact_value(serde_json::json!(true)),
            serde_json::json!(true)
        );
        assert_eq!(
            redact_value(serde_json::json!(null)),
            serde_json::json!(null)
        );
    }

    #[test]
    fn redact_value_respects_depth_bound() {
        // Build a 500-level deep nested JSON object — well beyond MAX_REDACT_DEPTH.
        // At the deepest leaf, place a secret that should be redacted only if
        // the recursion reaches it (it shouldn't — the bound stops at 128).
        let secret = "sk-proj-abc123def456ghi789jkl";
        let mut v = serde_json::Value::String(secret.to_string());
        for i in 0..500 {
            v = serde_json::json!({ format!("level_{i}"): v });
        }

        // Must not stack overflow — that's the primary assertion.
        let redacted = redact_value(v.clone());

        // The top-level structure should still be an object (not panicked/crashed).
        assert!(redacted.is_object(), "Top level must remain an object");

        // Secrets at the top 128 levels should be redacted.
        // Secrets below depth 128 are returned unchanged (acceptable trade-off).
        // Verify: walk 129 levels down — the value there should be unchanged (unredacted).
        let mut cursor = &redacted;
        for i in (0..500).rev() {
            let key = format!("level_{i}");
            cursor = &cursor[&key];
        }
        // At depth 500 (well past MAX_REDACT_DEPTH=128), the secret passes through unchanged.
        assert_eq!(
            cursor.as_str().unwrap(),
            secret,
            "Secrets beyond MAX_REDACT_DEPTH should pass through unchanged"
        );

        // Verify a secret at a shallow depth IS redacted: build a 5-level deep value.
        let mut shallow = serde_json::Value::String(secret.to_string());
        for i in 0..5 {
            shallow = serde_json::json!({ format!("l{i}"): shallow });
        }
        let redacted_shallow = redact_value(shallow);
        let mut c = &redacted_shallow;
        for i in (0..5).rev() {
            c = &c[&format!("l{i}")];
        }
        assert!(
            c.as_str().unwrap().contains("[REDACTED]"),
            "Secrets within MAX_REDACT_DEPTH must be redacted: {c}"
        );
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
