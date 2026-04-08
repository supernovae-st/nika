// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Typed provider name enum replacing stringly-typed provider references.
//!
//! Supports 9 built-in providers + Custom(String) for user-defined endpoints.

use std::fmt;

/// Typed provider name with known variants and custom support.
///
/// Replaces `Option<String>` throughout the AST and engine.
/// Manual Serde with alias support for common alternative names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProviderName {
    Anthropic,
    OpenAI,
    Mistral,
    Groq,
    DeepSeek,
    Gemini,
    XAi,
    Native,
    Mock,
    Custom(String),
}

impl ProviderName {
    /// Parse a provider name string, matching known providers case-insensitively.
    ///
    /// Supports aliases:
    /// - `claude` → Anthropic
    /// - `gpt` → OpenAI
    /// - `local`, `gguf` → Native
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "anthropic" | "claude" => Self::Anthropic,
            "openai" | "gpt" => Self::OpenAI,
            "mistral" => Self::Mistral,
            "groq" => Self::Groq,
            "deepseek" => Self::DeepSeek,
            "gemini" | "google" => Self::Gemini,
            "xai" | "grok" => Self::XAi,
            "native" | "local" | "gguf" => Self::Native,
            "mock" | "test" => Self::Mock,
            _ => Self::Custom(s.to_string()),
        }
    }

    /// Canonical string representation.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAI => "openai",
            Self::Mistral => "mistral",
            Self::Groq => "groq",
            Self::DeepSeek => "deepseek",
            Self::Gemini => "gemini",
            Self::XAi => "xai",
            Self::Native => "native",
            Self::Mock => "mock",
            Self::Custom(s) => s,
        }
    }

    /// Whether this is a cloud provider that requires an API key.
    pub fn requires_api_key(&self) -> bool {
        !matches!(self, Self::Native | Self::Mock | Self::Custom(_))
    }

    /// Whether this provider supports vision/multimodal.
    pub fn supports_vision(&self) -> bool {
        !matches!(self, Self::Native | Self::DeepSeek)
    }
}

impl fmt::Display for ProviderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for ProviderName {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for ProviderName {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl serde::Serialize for ProviderName {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for ProviderName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::parse(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_known_providers() {
        assert_eq!(ProviderName::parse("anthropic"), ProviderName::Anthropic);
        assert_eq!(ProviderName::parse("claude"), ProviderName::Anthropic);
        assert_eq!(ProviderName::parse("openai"), ProviderName::OpenAI);
        assert_eq!(ProviderName::parse("gpt"), ProviderName::OpenAI);
        assert_eq!(ProviderName::parse("mock"), ProviderName::Mock);
        assert_eq!(ProviderName::parse("native"), ProviderName::Native);
        assert_eq!(ProviderName::parse("local"), ProviderName::Native);
        assert_eq!(ProviderName::parse("gguf"), ProviderName::Native);
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(ProviderName::parse("ANTHROPIC"), ProviderName::Anthropic);
        assert_eq!(ProviderName::parse("OpenAI"), ProviderName::OpenAI);
        assert_eq!(ProviderName::parse("Mock"), ProviderName::Mock);
    }

    #[test]
    fn test_parse_custom() {
        assert_eq!(
            ProviderName::parse("my-custom-provider"),
            ProviderName::Custom("my-custom-provider".to_string())
        );
    }

    #[test]
    fn test_as_str_roundtrip() {
        for name in [
            ProviderName::Anthropic,
            ProviderName::OpenAI,
            ProviderName::Mistral,
            ProviderName::Groq,
            ProviderName::DeepSeek,
            ProviderName::Gemini,
            ProviderName::XAi,
            ProviderName::Native,
            ProviderName::Mock,
        ] {
            assert_eq!(ProviderName::parse(name.as_str()), name);
        }
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", ProviderName::Anthropic), "anthropic");
        assert_eq!(
            format!("{}", ProviderName::Custom("vllm".to_string())),
            "vllm"
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let name = ProviderName::Anthropic;
        let json = serde_json::to_string(&name).unwrap();
        assert_eq!(json, r#""anthropic""#);
        let round: ProviderName = serde_json::from_str(&json).unwrap();
        assert_eq!(round, name);
    }

    #[test]
    fn test_serde_alias_deserialization() {
        let round: ProviderName = serde_json::from_str(r#""claude""#).unwrap();
        assert_eq!(round, ProviderName::Anthropic);
        let round: ProviderName = serde_json::from_str(r#""gpt""#).unwrap();
        assert_eq!(round, ProviderName::OpenAI);
    }

    #[test]
    fn test_requires_api_key() {
        assert!(ProviderName::Anthropic.requires_api_key());
        assert!(ProviderName::OpenAI.requires_api_key());
        assert!(!ProviderName::Native.requires_api_key());
        assert!(!ProviderName::Mock.requires_api_key());
    }

    #[test]
    fn test_supports_vision() {
        assert!(ProviderName::Anthropic.supports_vision());
        assert!(ProviderName::OpenAI.supports_vision());
        assert!(!ProviderName::Native.supports_vision());
        assert!(!ProviderName::DeepSeek.supports_vision());
    }

    #[test]
    fn test_from_string() {
        let name: ProviderName = "anthropic".into();
        assert_eq!(name, ProviderName::Anthropic);
        let name: ProviderName = String::from("openai").into();
        assert_eq!(name, ProviderName::OpenAI);
    }
}
