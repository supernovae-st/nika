//! Canary token system for Nika Shield.
//!
//! Injects random tokens into system prompts that the LLM must never output.
//! If a canary appears in the output, it indicates:
//! 1. The system prompt leaked (direct exfiltration)
//! 2. The LLM was persuaded to repeat internal data (injection attack)
//!
//! C6 CORRECTION: Pure random tokens, no NIKA-CANARY- prefix (attacker-visible).

use std::sync::atomic::{AtomicU32, Ordering};
use uuid::Uuid;

// ── CanarySystem ────────────────────────────────────────────────────────────

/// Canary token system — 3 random tokens injected at different positions.
pub struct CanarySystem {
    /// 3 random 16-char alphanumeric tokens.
    tokens: [String; 3],
    /// Detection counter (atomic for concurrent access).
    detections: AtomicU32,
}

impl std::fmt::Debug for CanarySystem {
    /// Sprint 2 P0-1 hardening: redact tokens. Otherwise tracing logs and
    /// `dbg!()` from anywhere in the stack would leak the per-run secrets
    /// the canary check relies on for adversarial detection.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanarySystem")
            .field("tokens", &"<3 tokens redacted>")
            .field("detection_count", &self.detection_count())
            .finish()
    }
}

impl CanarySystem {
    /// Create a new canary system with 3 random tokens.
    ///
    /// Tokens are derived from UUIDs (cryptographically random) — no prefix
    /// pattern (C6: attacker-invisible).
    pub fn new() -> Self {
        let tokens = std::array::from_fn(|_| {
            // Generate 16 lowercase alphanumeric chars from a UUID
            let uuid = Uuid::new_v4();
            let hex = format!("{:032x}", uuid.as_u128());
            // Take first 16 hex chars (0-9, a-f = alphanumeric)
            hex[..16].to_string()
        });
        Self {
            tokens,
            detections: AtomicU32::new(0),
        }
    }

    /// Append canary tokens to the END of a system prompt as metadata footer.
    ///
    /// **Sprint 2 P0-1 hardening:** suffix-only injection preserves the
    /// provider's prefix-cache hit rate. Anthropic / OpenAI / Mistral cache
    /// exact-match prefixes — placing per-run random tokens at the START
    /// would invalidate the cache on every call (50–90 % cost regression
    /// on cached prompts). The trade-off is that the canary now appears
    /// AFTER any injection payload that flips the model's instruction
    /// hierarchy, but the suffix-cached `[trace_id=…]` block still gets
    /// echoed when the model is fully exfiltrating its system prompt —
    /// the only adversarial scenario where canary detection actually fires.
    #[must_use]
    pub fn inject_into_system_prompt(&self, system_prompt: &str) -> String {
        let mut out = String::with_capacity(system_prompt.len() + 192);
        out.push_str(system_prompt);
        if !system_prompt.is_empty() && !system_prompt.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("\n[trace_id=");
        out.push_str(&self.tokens[0]);
        out.push_str("]\n[session=");
        out.push_str(&self.tokens[1]);
        out.push_str("]\n[verify=");
        out.push_str(&self.tokens[2]);
        out.push(']');
        out
    }

    /// Test-only: peek at a token so tests can craft mock LLM responses
    /// that contain the token verbatim (Sprint 2 Item 2).
    #[cfg(test)]
    pub fn peek_token(&self, idx: usize) -> &str {
        &self.tokens[idx]
    }

    /// Check LLM output for canary leakage (exact + substring + char-spaced).
    pub fn check_output(&self, output: &str) -> Option<CanaryDetection> {
        for (idx, token) in self.tokens.iter().enumerate() {
            // Exact match
            if output.contains(token) {
                self.detections.fetch_add(1, Ordering::Relaxed);
                return Some(CanaryDetection {
                    token_index: idx,
                    match_type: CanaryMatchType::Exact,
                });
            }
            // Substring match (8+ consecutive chars from the token)
            if token.len() >= 8 {
                let windows = token.len() - 7;
                for start in 0..windows {
                    let substr = &token[start..start + 8];
                    if output.contains(substr) {
                        self.detections.fetch_add(1, Ordering::Relaxed);
                        return Some(CanaryDetection {
                            token_index: idx,
                            match_type: CanaryMatchType::Substring,
                        });
                    }
                }
            }
            // Character-spaced variant: t.o.k.e.n or t o k e n
            let spaced_dots: String = token
                .chars()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(".");
            if output.contains(&spaced_dots) {
                self.detections.fetch_add(1, Ordering::Relaxed);
                return Some(CanaryDetection {
                    token_index: idx,
                    match_type: CanaryMatchType::CharSpaced,
                });
            }
        }
        None
    }

    /// Total number of canary detections across all checks.
    pub fn detection_count(&self) -> u32 {
        self.detections.load(Ordering::Relaxed)
    }

    /// Get token count (for observability).
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }
}

impl Default for CanarySystem {
    fn default() -> Self {
        Self::new()
    }
}

// ── CanaryDetection ─────────────────────────────────────────────────────────

/// Details about a detected canary leak.
#[derive(Debug, Clone)]
pub struct CanaryDetection {
    /// Which token was detected (0, 1, or 2).
    pub token_index: usize,
    /// How the token was detected.
    pub match_type: CanaryMatchType,
}

/// Canary match type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanaryMatchType {
    /// Exact token match in output.
    Exact,
    /// 8+ consecutive chars from the token found in output.
    Substring,
    /// Character-spaced variant (t.o.k.e.n) found in output.
    CharSpaced,
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canary_generation() {
        let canary = CanarySystem::new();
        assert_eq!(canary.token_count(), 3);
        for token in &canary.tokens {
            assert_eq!(token.len(), 16);
            assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
            assert!(token
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        }
    }

    #[test]
    fn test_canary_no_prefix_pattern() {
        let canary = CanarySystem::new();
        for token in &canary.tokens {
            // C6: no NIKA or CANARY prefix
            assert!(!token.starts_with("nika"));
            assert!(!token.starts_with("canary"));
            assert!(!token.to_lowercase().contains("canary"));
        }
    }

    #[test]
    fn test_canary_detection_exact() {
        let canary = CanarySystem::new();
        let token = canary.tokens[1].clone();
        let output = format!("Here is the leaked token: {}", token);
        let detection = canary.check_output(&output);
        assert!(detection.is_some());
        let d = detection.unwrap();
        assert_eq!(d.token_index, 1);
        assert_eq!(d.match_type, CanaryMatchType::Exact);
    }

    #[test]
    fn test_canary_detection_substring() {
        let canary = CanarySystem::new();
        // Use first 8 chars of token 0
        let substr = &canary.tokens[0][..8];
        let output = format!("Partial leak: {} end", substr);
        let detection = canary.check_output(&output);
        assert!(detection.is_some());
        assert_eq!(detection.unwrap().match_type, CanaryMatchType::Substring);
    }

    #[test]
    fn test_canary_no_false_positive() {
        let canary = CanarySystem::new();
        let normal = "This is a normal response about cats and dogs.";
        assert!(canary.check_output(normal).is_none());
    }

    #[test]
    fn test_canary_injection_contains_tokens() {
        let canary = CanarySystem::new();
        let prompt = "You are a helpful assistant.";
        let injected = canary.inject_into_system_prompt(prompt);
        assert!(injected.contains(prompt));
        for token in &canary.tokens {
            assert!(injected.contains(token));
        }
    }

    #[test]
    fn test_canary_detection_counter() {
        let canary = CanarySystem::new();
        let output = format!("Leaked: {}", canary.tokens[0]);
        assert_eq!(canary.detection_count(), 0);
        canary.check_output(&output);
        assert_eq!(canary.detection_count(), 1);
    }

    /// Sprint 2 P0-1 regression: the prompt prefix MUST be preserved
    /// verbatim so the provider's prefix-cache hit rate stays high. A
    /// 50–90 % cost regression on cached prompts is the alternative.
    #[test]
    fn test_canary_inject_preserves_prefix_for_provider_cache() {
        let canary = CanarySystem::new();
        let base = "You are a helpful assistant. Always respond in JSON.";
        let injected = canary.inject_into_system_prompt(base);
        assert!(
            injected.starts_with(base),
            "prefix must be preserved verbatim for provider cache"
        );
        assert!(
            injected.contains("[trace_id="),
            "canary must appear in suffix"
        );
        assert!(injected.contains("[session="), "session token must appear");
        assert!(injected.contains("[verify="), "verify token must appear");
        assert!(injected.len() > base.len(), "suffix added");
        assert!(injected.len() < base.len() + 256, "suffix is bounded");
    }

    /// Sprint 2 P0-1 hardening: Debug must NOT leak the canary tokens.
    #[test]
    fn test_canary_debug_redacts_tokens() {
        let canary = CanarySystem::new();
        let dbg = format!("{canary:?}");
        for token in &canary.tokens {
            assert!(
                !dbg.contains(token),
                "Debug must not leak token {token}: {dbg}"
            );
        }
        assert!(dbg.contains("redacted"));
        assert!(dbg.contains("CanarySystem"));
    }

    #[test]
    fn test_canary_uniqueness() {
        let c1 = CanarySystem::new();
        let c2 = CanarySystem::new();
        assert_ne!(
            c1.tokens, c2.tokens,
            "Canary tokens should be unique per instance"
        );
    }
}
