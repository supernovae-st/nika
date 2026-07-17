// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider profiles — the canonical 17, as data.
//!
//! A profile binds a provider id to a wire format, a default endpoint and a
//! key-loading recipe. Every canonical id joins its `nika-catalog` row
//! (codegen from `data/llm-providers.toml` — description · tags · model
//! nicknames): cloud rows carry env var + key prefixes, the 5 local
//! servers' rows (the 2026-07-06 fill) carry the catalog face while their
//! endpoints and keyless-ness stay const HERE — the loopback defaults and
//! the `is_local` classification are runtime facts, never data.

use nika_catalog::types::Provider as CatalogRow;

/// Wire protocol family an adapter speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WireFormat {
    /// Anthropic Messages API.
    Anthropic,
    /// `OpenAI` Chat Completions — also every OpenAI-compatible server
    /// (cloud: `openai` · `deepseek` · `mistral` · `xai` · `groq` ·
    /// `openrouter` · `huggingface` · `nvidia` · `moonshot` · local: `ollama` · `lmstudio` · `llamacpp` ·
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
    /// `ResolvedProvider::supports_response_format` (per resolved
    /// provider) and `ProviderRegistry::supports_response_format`
    /// (keyless · per model string) answer through here.
    ///
    /// Per wire family: `OpenAiCompat` (some local servers lack strict
    /// `json_schema`, but the family does), `Gemini` (an OpenAPI-style
    /// subset via `responseSchema`) and `Anthropic` (grammar-constrained
    /// `output_config.format` · GA 2026-01-29 · the wire normalizes to
    /// its narrower dialect) all carry a native mode. `Mock` answers
    /// `true` so structured-output tests need no live provider. Every
    /// family being native today, the matches! stays — a future wire
    /// variant defaults to the instruction fallback until proven.
    #[must_use]
    pub fn supports_response_format(self) -> bool {
        matches!(
            self,
            Self::OpenAiCompat | Self::Mock | Self::Gemini | Self::Anthropic
        )
    }

    /// Whether this wire family's STRICT structured mode rejects an
    /// UNDERSPECIFIED schema — an object without `properties` or an
    /// array without `items` anywhere in the tree (F2 · ADR-098).
    ///
    /// The real strict modes do: `OpenAI`'s `json_schema`+`strict` 400s
    /// on exactly this class (the whole family is treated as its peer),
    /// gemini's `responseSchema` `OpenAPI` subset carries its own
    /// rejection surface, and `Anthropic`'s grammar compiler rejects a
    /// free-form object (`additionalProperties` must be `false`) — all
    /// three fall back to the native JSON mode + LOCAL validation (on
    /// anthropic the JSON mode is a wire no-op: the schema instruction
    /// rides the prompt). `Mock` SYNTHESIZES a conformant instance from
    /// ANY schema (F3 · the offline-CI base) — never rejects.
    #[must_use]
    pub fn strict_rejects_underspecified(self) -> bool {
        matches!(self, Self::OpenAiCompat | Self::Gemini | Self::Anthropic)
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

    /// Whether this profile is one of the 5 LOCAL servers (`ollama` ·
    /// `lmstudio` · `llamacpp` · `localai` · `vllm`) — keyed on the
    /// canonical id (the [`LOCAL`] seed rows), so an operator
    /// `with_base_url` override never flips the classification. Local
    /// servers get the generous transport-deadline default (a local
    /// model routinely needs minutes for one completion — see
    /// `wire::transport_deadline`).
    pub(crate) fn is_local(&self) -> bool {
        LOCAL.iter().any(|(id, _)| *id == self.id)
    }

    /// Whether THIS provider supports native `response_format:
    /// json_schema` — the wire-family answer with one per-provider
    /// correction: deepseek's API accepts only `text | json_object`
    /// (api-docs.deepseek.com create-chat-completion · fetched
    /// 2026-07-08 · `json_schema` is out-of-enum → 4xx), so it takes
    /// the instruction fallback + local validation like a non-native
    /// wire. Capability is a PROVIDER fact, not only a wire fact —
    /// every consumer (resolved provider · keyless registry query)
    /// answers through here.
    #[must_use]
    pub fn supports_response_format(&self) -> bool {
        self.id != "deepseek" && self.wire.supports_response_format()
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

/// The canonical provider ids, in canon order (11 cloud · 5 local · 1 test).
pub const CANONICAL_IDS: [&str; 17] = [
    "anthropic",
    "openai",
    "gemini",
    "deepseek",
    "mistral",
    "xai",
    "groq",
    "openrouter",
    "huggingface",
    "nvidia",
    "moonshot",
    "ollama",
    "lmstudio",
    "llamacpp",
    "localai",
    "vllm",
    "mock",
];

/// The MODELS-rung law (#320): why a `<provider>/<model>` string cannot
/// resolve in THIS binary — `None` when it can. Lives beside the resolver
/// (the set it interrogates is [`CANONICAL_IDS`]) and is shared by every
/// audit surface (CLI check · MCP `nika_check`): a hallucinated model
/// must red the audit on EVERY lane, never only one — the vendor catalog
/// advertising a provider does not make it runnable (the azure class).
#[must_use]
pub fn resolve_refusal(model: &str) -> Option<String> {
    match model.split_once('/') {
        None => Some(format!(
            "`{model}` is a bare model id — the contract is `<provider>/<model>` \
             (pick the provider that serves it; `nika doctor` names the \
             {} runnable providers)",
            CANONICAL_IDS.len()
        )),
        Some((provider, _)) if !CANONICAL_IDS.contains(&provider) => {
            // The shared did-you-mean metric (nika-types::suggest — the
            // same threshold the parser/checker suggest with): `antropic`
            // is ONE edit from the most-used provider id, and the rename
            // is the whole fix. Silence past the threshold, as everywhere.
            let guess = nika_types::suggest::did_you_mean(provider, CANONICAL_IDS)
                .map(|p| format!(" — did you mean `{p}`?"))
                .unwrap_or_default();
            Some(format!(
                "provider `{provider}` does not resolve in THIS binary \
                 ({} runnable — `nika doctor` names them); a cataloged \
                 vendor is not a runnable one{guess}",
                CANONICAL_IDS.len()
            ))
        }
        Some(_) => None,
    }
}

/// The 11 cloud rows (catalog-backed) + the in-process mock.
///
/// gemini's `base_url` is a STEM (`…/v1beta`) — the s8.6 adapter appends
/// `/models/{model}:generateContent` per request (unlike the other wires,
/// whose `base_url` is the complete endpoint).
const CATALOG_WIRED: [(&str, WireFormat, &str); 12] = [
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
    // huggingface · the Inference Providers router (chat-only surface ·
    // 100+ open-weights across 18 backend providers · zero markup) ·
    // model names carry an INNER slash + optional :provider/:policy
    // suffix (`Qwen/Qwen3.5-9B:groq`) — resolve() split_once already
    // hands the whole rest through untouched.
    (
        "huggingface",
        WireFormat::OpenAiCompat,
        "https://router.huggingface.co/v1/chat/completions",
    ),
    // nvidia · integrate.api.nvidia.com (NIM cloud) · Nemotron 3 family
    // (Open Model License) + hosted open models · self-hosted NIM
    // containers expose the same surface (override base_url).
    (
        "nvidia",
        WireFormat::OpenAiCompat,
        "https://integrate.api.nvidia.com/v1/chat/completions",
    ),
    // moonshot · api.moonshot.ai (international endpoint) · Kimi K3
    // (1M context · thinking model — reasoning spends output tokens,
    // budget max_tokens accordingly) + the K2.x line · weights announced
    // open 2026-07-27 · promoted per the ADR-104 precedent (ADR-105).
    (
        "moonshot",
        WireFormat::OpenAiCompat,
        "https://api.moonshot.ai/v1/chat/completions",
    ),
    ("mock", WireFormat::Mock, ""),
];

/// The 5 local OpenAI-compatible servers — endpoints and keyless-ness are
/// const RUNTIME facts (loopback defaults · operator-overridable); their
/// catalog rows (description · tags · seed models) join in [`seed`].
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

/// Build the canonical 17 profiles (catalog-joined where rows exist).
#[must_use]
pub fn seed() -> Vec<Profile> {
    let mut out = Vec::with_capacity(17);
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
            // Keyless by CONSTRUCTION, never by data: a catalog edit
            // flipping requires_key on a local row must not invent a
            // key gate the servers don't have.
            requires_key: false,
            catalog: nika_catalog::find_provider(id),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn resolve_refusal_names_the_two_classes_and_clears_the_runnable() {
        // bare id — teaches the contract
        let bare = resolve_refusal("gpt-5-turbo").expect("bare id refused");
        assert!(bare.contains("bare model id") && bare.contains("17 runnable"));
        // cataloged-but-unresolvable provider — the azure class
        let azure = resolve_refusal("azure/gpt-4o").expect("azure refused");
        assert!(azure.contains("`azure`") && azure.contains("not a runnable one"));
        // azure is far from every canonical id — no guess appended
        assert!(!azure.contains("did you mean"), "{azure}");
        // every canonical provider clears, inner slashes included
        assert!(resolve_refusal("mock/echo").is_none());
        assert!(resolve_refusal("huggingface/Qwen/Qwen3.5-9B:groq").is_none());
    }

    #[test]
    fn provider_typo_gets_the_rename() {
        // The sweep's mute surface (2026-07-11): `antropic` is ONE edit
        // from the most-used provider id — the refusal now carries the
        // rename, through the SAME shared metric as the parser/checker.
        let typo = resolve_refusal("antropic/claude-sonnet-4-6").expect("typo refused");
        assert!(typo.contains("did you mean `anthropic`?"), "{typo}");
        let gemni = resolve_refusal("gemni/gemini-2.5-flash").expect("typo refused");
        assert!(gemni.contains("did you mean `gemini`?"), "{gemni}");
    }

    use super::*;

    #[test]
    fn seed_yields_the_canonical_fourteen() {
        let profiles = seed();
        assert_eq!(profiles.len(), 17);
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
    fn locals_join_their_catalog_rows_but_stay_keyless_by_construction() {
        // The 2026-07-06 fill: local rows carry the catalog FACE
        // (description · tags · seed models — `nika catalog` and the
        // editor picker see them), while requires_key stays a literal
        // false in seed(): a catalog edit can never invent a key gate.
        for p in seed() {
            if ["ollama", "lmstudio", "llamacpp", "localai", "vllm"].contains(&p.id) {
                let row = p
                    .catalog
                    .unwrap_or_else(|| panic!("{} must join its row", p.id));
                assert!(!row.models.is_empty(), "{} row needs a seed model", p.id);
                assert!(!p.requires_key, "{} keyless by construction", p.id);
            }
        }
    }

    #[test]
    fn is_local_classifies_exactly_the_five_local_servers() {
        // F1: the classification drives the transport-deadline default
        // (local ≫ cloud) — keyed on the id so a base_url override can
        // never flip it.
        for p in seed() {
            let expected = ["ollama", "lmstudio", "llamacpp", "localai", "vllm"].contains(&p.id);
            assert_eq!(p.is_local(), expected, "{} classification", p.id);
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
        // registry query answer through. Anthropic joined 2026-07-07
        // (output_config.format · GA 2026-01-29) — the instruction
        // fallback is no longer its schema path.
        assert!(WireFormat::OpenAiCompat.supports_response_format());
        assert!(WireFormat::Gemini.supports_response_format());
        assert!(WireFormat::Mock.supports_response_format());
        assert!(WireFormat::Anthropic.supports_response_format());
        // The strict-reject nuance rides with it: anthropic's grammar
        // compiler rejects free-form objects → ADR-098 fallback applies.
        assert!(WireFormat::Anthropic.strict_rejects_underspecified());
        assert!(!WireFormat::Mock.strict_rejects_underspecified());
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
