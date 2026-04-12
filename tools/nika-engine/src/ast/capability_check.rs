// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider-capability gate used by `nika check` (pre-run) and the runtime
//! (defense-in-depth). Replaces the pre-Track-2 pattern of `tracing::warn!()`
//! followed by a silent swap or silent drop.
//!
//! The single source of truth is
//! [`nika_core::catalogs::ProviderCapabilities`]; this module walks the AST
//! and converts a capability mismatch into
//! [`NikaError::UnsupportedProviderCapability`] (NIKA-120).

use nika_core::catalogs::ProviderCapabilities;

use crate::error::NikaError;

/// Reject `extended_thinking: true` on a provider that does not implement it.
///
/// `task_id` is only used to make the resulting error actionable — the caller
/// already has the value. `provider` must be the EFFECTIVE provider (task
/// override OR workflow default), already resolved.
pub fn check_extended_thinking(
    task_id: &str,
    provider: &str,
    extended_thinking: Option<bool>,
) -> Result<(), NikaError> {
    if extended_thinking != Some(true) {
        return Ok(());
    }
    let caps = ProviderCapabilities::for_provider(provider);
    if caps.extended_thinking {
        return Ok(());
    }
    Err(NikaError::UnsupportedProviderCapability {
        task_id: task_id.to_string(),
        provider: provider.to_string(),
        capability: "extended_thinking".to_string(),
        supported_providers: ProviderCapabilities::providers_supporting("extended_thinking")
            .into_iter()
            .map(String::from)
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_thinking_on_groq_returns_nika_120() {
        let err = check_extended_thinking("analyze", "groq", Some(true))
            .expect_err("groq must not silently swap or drop");
        match err {
            NikaError::UnsupportedProviderCapability {
                task_id,
                provider,
                capability,
                supported_providers,
            } => {
                assert_eq!(task_id, "analyze");
                assert_eq!(provider, "groq");
                assert_eq!(capability, "extended_thinking");
                assert!(
                    supported_providers.iter().any(|p| p == "anthropic"),
                    "help must list anthropic: {supported_providers:?}"
                );
                assert!(
                    supported_providers.iter().any(|p| p == "openai"),
                    "help must list openai: {supported_providers:?}"
                );
                assert!(
                    !supported_providers.iter().any(|p| p == "groq"),
                    "help must not list the rejected provider: {supported_providers:?}"
                );
            }
            other => panic!("expected NIKA-120, got {other:?}"),
        }
    }

    #[test]
    fn extended_thinking_on_anthropic_is_ok() {
        check_extended_thinking("analyze", "anthropic", Some(true))
            .expect("anthropic must accept extended_thinking");
    }

    #[test]
    fn extended_thinking_on_claude_alias_is_ok() {
        check_extended_thinking("analyze", "claude", Some(true))
            .expect("claude alias must accept extended_thinking");
    }

    #[test]
    fn extended_thinking_on_openai_is_ok() {
        check_extended_thinking("analyze", "openai", Some(true))
            .expect("openai reasoning models support extended_thinking");
    }

    #[test]
    fn extended_thinking_none_is_ok_on_any_provider() {
        check_extended_thinking("t", "groq", None).expect("absent capability must never fail");
        check_extended_thinking("t", "mistral", Some(false)).expect("false must never fail");
        check_extended_thinking("t", "xai", None).expect("absent capability must never fail");
    }

    #[test]
    fn extended_thinking_on_every_rejecting_provider_returns_nika_120() {
        for provider in ["groq", "mistral", "deepseek", "gemini", "xai", "native"] {
            let err = check_extended_thinking("analyze", provider, Some(true))
                .expect_err(&format!("{provider} must reject extended_thinking"));
            assert_eq!(err.code(), "NIKA-120", "provider={provider}");
        }
    }

    #[test]
    fn extended_thinking_on_unknown_provider_returns_nika_120() {
        let err = check_extended_thinking("t", "totally-fake", Some(true))
            .expect_err("unknown provider must be conservative");
        assert_eq!(err.code(), "NIKA-120");
    }
}
