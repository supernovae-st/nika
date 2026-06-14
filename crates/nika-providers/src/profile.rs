// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider profiles — the canonical 14, as data.
//!
//! A profile binds a provider id to a wire format, a default endpoint and a
//! key-loading recipe. Cloud rows are seeded from `nika-catalog` (codegen
//! from `data/llm-providers.toml` — id · env var · model nicknames); the 5
//! local servers are not catalog rows yet, so their profiles are const data
//! here (upstreaming them to the catalog TOML is a follow-up — the set and
//! its split are canon: `nika-spec/canon.yaml` `providers:` · 8 cloud + 5
//! local + 1 test = 14 per D-2026-06-10-N2).

use nika_catalog::types::Provider as CatalogRow;

/// Wire protocol family an adapter speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WireFormat {
    /// Anthropic Messages API.
    Anthropic,
    /// `OpenAI` Chat Completions — also every OpenAI-compatible server
    /// (cloud: `openai` · `deepseek` · `mistral` · `xai` · `groq` ·
    /// `openrouter` · local: `ollama` · `lmstudio` · `llamacpp` ·
    /// `localai` · `vllm`).
    OpenAiCompat,
    /// Google Gemini `generateContent` (wired s8.6). The profile
    /// `base_url` is a STEM — the adapter appends
    /// `/models/{model}:generateContent` per request.
    Gemini,
    /// Deterministic in-process mock (zero network · zero key) — the
    /// `hello.yaml` first-run surface.
    Mock,
}

impl WireFormat {
    /// Whether this wire family supports native `response_format:
    /// json_schema` (structured output).
    ///
    /// The SINGLE source of truth for the capability — both
    /// [`crate::ResolvedProvider::supports_response_format`] (per resolved
    /// provider) and [`crate::ProviderRegistry::supports_response_format`]
    /// (keyless · per model string) answer through here.
    ///
    /// v0.1 approximation, per wire family: `OpenAiCompat` (some local
    /// servers lack strict `json_schema`, but the family does) and
    /// `Gemini` (an OpenAPI-style subset via `responseSchema`) support it;
    /// `Anthropic` does NOT (the wire rejects `response_format` outright ·
    /// callers must fall back to a schema instruction). `Mock` answers
    /// `true` so structured-output tests need no live provider.
    #[must_use]
    pub fn supports_response_format(self) -> bool {
        matches!(self, Self::OpenAiCompat | Self::Mock | Self::Gemini)
    }
}

/// One canonical provider profile.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Profile {
    /// Canonical id (`"anthropic"` … always lowercase · the `model:`
    /// prefix before the first `/`).
    pub id: &'static str,
    /// Wire family.
    pub wire: WireFormat,
    /// Default endpoint URL (full path · operator-overridable via
    /// [`crate::ProvidersConfig`]).
    pub base_url: &'static str,
    /// Whether an API key is required (local servers + mock: no).
    pub requires_key: bool,
    /// Catalog row when one exists (env var · model nicknames · prefixes).
    pub catalog: Option<&'static CatalogRow>,
}

impl Profile {
    /// Environment variables consulted for this provider's API key, in
    /// priority order: `NIKA_<ID>_API_KEY` first, then the provider's
    /// conventional variable from the catalog row (e.g. `ANTHROPIC_API_KEY`).
    #[must_use]
    pub fn env_candidates(&self) -> Vec<String> {
        let mut v = vec![format!("NIKA_{}_API_KEY", self.id.to_uppercase())];
        if let Some(row) = self.catalog
            && !row.env_var.is_empty()
        {
            v.push(row.env_var.to_owned());
        }
        v
    }

    /// Resolve a model nickname through the catalog row (`"sonnet"` →
    /// `"claude-sonnet-4-20250514"`). Unknown names pass through verbatim —
    /// the wire model namespace is the provider's, not ours.
    #[must_use]
    pub fn resolve_model<'m>(&self, name: &'m str) -> &'m str {
        if let Some(row) = self.catalog {
            for m in row.models {
                if m.id == name {
                    return m.model;
                }
            }
        }
        name
    }
}

/// The canonical provider ids, in canon order (8 cloud · 5 local · 1 test).
pub const CANONICAL_IDS: [&str; 14] = [
    "anthropic",
    "openai",
    "gemini",
    "deepseek",
    "mistral",
    "xai",
    "groq",
    "openrouter",
    "ollama",
    "lmstudio",
    "llamacpp",
    "localai",
    "vllm",
    "mock",
];

/// The 8 cloud rows (catalog-backed) + the in-process mock.
///
/// gemini's `base_url` is a STEM (`…/v1beta`) — the s8.6 adapter appends
/// `/models/{model}:generateContent` per request (unlike the other wires,
/// whose `base_url` is the complete endpoint).
const CATALOG_WIRED: [(&str, WireFormat, &str); 9] = [
    (
        "anthropic",
        WireFormat::Anthropic,
        "https://api.anthropic.com/v1/messages",
    ),
    (
        "openai",
        WireFormat::OpenAiCompat,
        "https://api.openai.com/v1/chat/completions",
    ),
    (
        "gemini",
        WireFormat::Gemini,
        "https://generativelanguage.googleapis.com/v1beta",
    ),
    (
        "deepseek",
        WireFormat::OpenAiCompat,
        "https://api.deepseek.com/v1/chat/completions",
    ),
    (
        "mistral",
        WireFormat::OpenAiCompat,
        "https://api.mistral.ai/v1/chat/completions",
    ),
    (
        "xai",
        WireFormat::OpenAiCompat,
        "https://api.x.ai/v1/chat/completions",
    ),
    (
        "groq",
        WireFormat::OpenAiCompat,
        "https://api.groq.com/openai/v1/chat/completions",
    ),
    (
        "openrouter",
        WireFormat::OpenAiCompat,
        "https://openrouter.ai/api/v1/chat/completions",
    ),
    ("mock", WireFormat::Mock, ""),
];

/// The 5 local OpenAI-compatible servers (not catalog rows yet — const
/// profiles · zero key · loopback defaults · operator-overridable).
///
/// `llamacpp` and `localai` both ship 8080 because that IS each upstream's
/// own default; running both at once needs one `with_base_url` override
/// (`nika doctor` will flag the collision · s19).
const LOCAL: [(&str, &str); 5] = [
    ("ollama", "http://127.0.0.1:11434/v1/chat/completions"),
    ("lmstudio", "http://127.0.0.1:1234/v1/chat/completions"),
    ("llamacpp", "http://127.0.0.1:8080/v1/chat/completions"),
    ("localai", "http://127.0.0.1:8080/v1/chat/completions"),
    ("vllm", "http://127.0.0.1:8000/v1/chat/completions"),
];

/// Build the canonical 14 profiles (catalog-joined where rows exist).
#[must_use]
pub fn seed() -> Vec<Profile> {
    let mut out = Vec::with_capacity(14);
    for (id, wire, base_url) in CATALOG_WIRED {
        let catalog = nika_catalog::find_provider(id);
        out.push(Profile {
            id,
            wire,
            base_url,
            requires_key: catalog.is_some_and(|c| c.requires_key),
            catalog,
        });
    }
    for (id, base_url) in LOCAL {
        out.push(Profile {
            id,
            wire: WireFormat::OpenAiCompat,
            base_url,
            requires_key: false,
            catalog: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_yields_the_canonical_fourteen() {
        let profiles = seed();
        assert_eq!(profiles.len(), 14);
        let mut ids: Vec<&str> = profiles.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        let mut canon = CANONICAL_IDS.to_vec();
        canon.sort_unstable();
        assert_eq!(ids, canon);
    }

    #[test]
    fn cloud_rows_join_their_catalog_entry() {
        let profiles = seed();
        let anthropic = profiles.iter().find(|p| p.id == "anthropic").unwrap();
        let row = anthropic.catalog.expect("anthropic is a catalog row");
        assert_eq!(row.env_var, "ANTHROPIC_API_KEY");
        assert!(anthropic.requires_key);
        assert_eq!(anthropic.wire, WireFormat::Anthropic);
    }

    #[test]
    fn every_cloud_row_is_catalog_joined_and_keyed() {
        // Drift guard: if a cloud id ever drops from the catalog TOML,
        // requires_key silently flips false and the fail-fast auth gate
        // disables itself (401 at the wire instead of guided AuthFailed).
        for p in seed() {
            if matches!(p.wire, WireFormat::Mock) || !p.base_url.starts_with("https://") {
                continue;
            }
            assert!(p.catalog.is_some(), "{} must stay a catalog row", p.id);
            assert!(p.requires_key, "{} must require a key", p.id);
        }
    }

    #[test]
    fn locals_need_no_key_and_speak_openai_compat() {
        for p in seed() {
            if ["ollama", "lmstudio", "llamacpp", "localai", "vllm"].contains(&p.id) {
                assert!(!p.requires_key, "{} must not require a key", p.id);
                assert_eq!(p.wire, WireFormat::OpenAiCompat);
                assert!(p.base_url.starts_with("http://127.0.0.1"), "{}", p.id);
            }
        }
    }

    #[test]
    fn mock_needs_no_key() {
        let profiles = seed();
        let mock = profiles.iter().find(|p| p.id == "mock").unwrap();
        assert!(!mock.requires_key);
        assert_eq!(mock.wire, WireFormat::Mock);
    }

    #[test]
    fn env_ladder_puts_nika_prefix_first() {
        let profiles = seed();
        let openai = profiles.iter().find(|p| p.id == "openai").unwrap();
        let ladder = openai.env_candidates();
        assert_eq!(ladder[0], "NIKA_OPENAI_API_KEY");
        assert_eq!(ladder[1], "OPENAI_API_KEY");
    }

    #[test]
    fn wire_response_format_capability_is_the_single_source() {
        // The one matrix both the resolved provider and the keyless
        // registry query answer through.
        assert!(WireFormat::OpenAiCompat.supports_response_format());
        assert!(WireFormat::Gemini.supports_response_format());
        assert!(WireFormat::Mock.supports_response_format());
        assert!(
            !WireFormat::Anthropic.supports_response_format(),
            "anthropic rejects response_format · instruction fallback required"
        );
    }

    #[test]
    fn nickname_resolves_through_catalog_unknown_passes_through() {
        let profiles = seed();
        let anthropic = profiles.iter().find(|p| p.id == "anthropic").unwrap();
        let wire = anthropic.resolve_model("sonnet");
        assert!(wire.starts_with("claude-"), "nickname mapped: {wire}");
        assert_eq!(
            anthropic.resolve_model("claude-future-9"),
            "claude-future-9"
        );
        // local profiles have no catalog: verbatim passthrough
        let ollama = profiles.iter().find(|p| p.id == "ollama").unwrap();
        assert_eq!(ollama.resolve_model("llama3.2"), "llama3.2");
    }
}
