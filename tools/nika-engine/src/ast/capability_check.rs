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

use crate::ast::agent::ToolChoice;
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

/// Reject `stop_sequences: [...]` on a provider that does not honour it.
///
/// Empty / absent `stop_sequences` is always OK — the matrix only gates
/// user-supplied values.
pub fn check_stop_sequences(
    task_id: &str,
    provider: &str,
    stop_sequences: &[String],
) -> Result<(), NikaError> {
    if stop_sequences.is_empty() {
        return Ok(());
    }
    let caps = ProviderCapabilities::for_provider(provider);
    if caps.stop_sequences {
        return Ok(());
    }
    Err(NikaError::UnsupportedProviderCapability {
        task_id: task_id.to_string(),
        provider: provider.to_string(),
        capability: "stop_sequences".to_string(),
        supported_providers: ProviderCapabilities::providers_supporting("stop_sequences")
            .into_iter()
            .map(String::from)
            .collect(),
    })
}

/// Reject `tool_choice: required` on a provider that does not honour it.
///
/// Only `Required` is gated — `Auto` and `None` are universal.
pub fn check_tool_choice(
    task_id: &str,
    provider: &str,
    tool_choice: Option<&ToolChoice>,
) -> Result<(), NikaError> {
    let is_required = matches!(tool_choice, Some(ToolChoice::Required));
    if !is_required {
        return Ok(());
    }
    let caps = ProviderCapabilities::for_provider(provider);
    if caps.tool_choice_required {
        return Ok(());
    }
    Err(NikaError::UnsupportedProviderCapability {
        task_id: task_id.to_string(),
        provider: provider.to_string(),
        capability: "tool_choice_required".to_string(),
        supported_providers: ProviderCapabilities::providers_supporting("tool_choice_required")
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

    // ---- stop_sequences ----

    #[test]
    fn stop_sequences_on_groq_returns_nika_120() {
        let err =
            check_stop_sequences("analyze", "groq", &["END".to_string(), "DONE".to_string()])
                .expect_err("groq must not silently drop stop_sequences");
        match err {
            NikaError::UnsupportedProviderCapability {
                provider,
                capability,
                supported_providers,
                ..
            } => {
                assert_eq!(provider, "groq");
                assert_eq!(capability, "stop_sequences");
                assert!(supported_providers.iter().any(|p| p == "anthropic"));
                assert!(supported_providers.iter().any(|p| p == "openai"));
                assert!(!supported_providers.iter().any(|p| p == "groq"));
            }
            other => panic!("expected NIKA-120, got {other:?}"),
        }
    }

    #[test]
    fn stop_sequences_empty_is_ok_on_any_provider() {
        check_stop_sequences("t", "groq", &[]).expect("empty stop must never fail");
        check_stop_sequences("t", "xai", &[]).expect("empty stop must never fail");
    }

    #[test]
    fn stop_sequences_on_anthropic_is_ok() {
        check_stop_sequences("t", "anthropic", &["STOP".to_string()]).expect("anthropic supports");
    }

    #[test]
    fn stop_sequences_on_every_rejecting_provider_returns_nika_120() {
        for provider in ["groq", "mistral", "xai"] {
            let err = check_stop_sequences("t", provider, &["STOP".to_string()])
                .expect_err(&format!("{provider} must reject stop_sequences"));
            assert_eq!(err.code(), "NIKA-120", "provider={provider}");
        }
    }

    // ---- tool_choice ----

    #[test]
    fn tool_choice_required_on_mistral_returns_nika_120() {
        let err = check_tool_choice("assistant", "mistral", Some(&ToolChoice::Required))
            .expect_err("mistral must not silently no-op tool_choice: required");
        match err {
            NikaError::UnsupportedProviderCapability {
                provider,
                capability,
                supported_providers,
                ..
            } => {
                assert_eq!(provider, "mistral");
                assert_eq!(capability, "tool_choice_required");
                assert!(supported_providers.iter().any(|p| p == "anthropic"));
                assert!(supported_providers.iter().any(|p| p == "openai"));
                assert!(!supported_providers.iter().any(|p| p == "mistral"));
            }
            other => panic!("expected NIKA-120, got {other:?}"),
        }
    }

    #[test]
    fn tool_choice_auto_and_none_are_universal() {
        check_tool_choice("t", "groq", Some(&ToolChoice::Auto)).expect("auto is universal");
        check_tool_choice("t", "xai", Some(&ToolChoice::None)).expect("none is universal");
        check_tool_choice("t", "mistral", None).expect("absent is universal");
    }

    #[test]
    fn tool_choice_required_on_anthropic_is_ok() {
        check_tool_choice("t", "anthropic", Some(&ToolChoice::Required)).expect("anthropic");
    }

    #[test]
    fn tool_choice_required_on_every_rejecting_provider_returns_nika_120() {
        for provider in ["groq", "mistral", "deepseek", "gemini", "xai", "native"] {
            let err = check_tool_choice("t", provider, Some(&ToolChoice::Required))
                .expect_err(&format!("{provider} must reject tool_choice: required"));
            assert_eq!(err.code(), "NIKA-120", "provider={provider}");
        }
    }
}
