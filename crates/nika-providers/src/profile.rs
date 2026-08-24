// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider profiles — the canonical 16, as data.
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
    /// `openrouter` · `huggingface` · `nvidia` · local: `ollama` · `lmstudio` · `llamacpp` ·
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

    /// The execution-access class this profile runs over TODAY
    /// (D-2026-08-04-N1 · `model:` picks the intelligence, access the
    /// path) — the ONE derivation, shared with the trace emitter via
    /// [`access_class_for`]. `Harness`/`Oauth` never come from a
    /// profile; those classes enter with the P3+ adapters, which carry
    /// their own descriptors.
    #[must_use]
    pub fn access_class(&self) -> nika_types::access::AccessClass {
        access_class_for(self.id)
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

/// The canonical provider ids, in canon order (10 cloud · 5 local · 1 test).
pub const CANONICAL_IDS: [&str; 16] = [
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
    "ollama",
    "lmstudio",
    "llamacpp",
    "localai",
    "vllm",
    "mock",
];

/// Whether the provider is one of the 5 server-backed KEYLESS engines
/// (ollama · lmstudio · llamacpp · localai · vllm) — the class the B-5
/// run gate probes, and the class the MODELS rung nuances: « resolves »
/// is never « reachable » for a server nothing dialed.
#[must_use]
pub fn server_backed_local(provider: &str) -> bool {
    LOCAL.iter().any(|(id, _)| *id == provider)
}

/// The execution-access class of a provider id (D-2026-08-04-N1) — the
/// ONE derivation every surface shares (probe rows · the trace
/// emitter's structured `access` field): `mock` is the test lane, the
/// 5 seed-keyed local servers are `Local` (override-proof — same key
/// as the B-5 gate), everything else reaches its vendor over an API
/// key. A non-canonical id classifies `Api` — post-resolve callers
/// never hold one, and an api-shaped guess is the honest default for
/// a keyed endpoint.
#[must_use]
pub fn access_class_for(provider: &str) -> nika_types::access::AccessClass {
    use nika_types::access::AccessClass;
    if provider == "mock" {
        AccessClass::Mock
    } else if server_backed_local(provider) {
        AccessClass::Local
    } else {
        AccessClass::Api
    }
}

/// Spec namespace for a `model:` that lacks a canonical provider prefix
/// (the FORM law · stdlib/providers-v0.1.md · #761). Numbered
/// `NIKA-PROVIDER-NNN` codes stay per-adapter runtime errors (spec 05).
pub const PREFIX_REFUSAL_CODE: &str = "NIKA-PROVIDER";

/// Why a `model:` cannot resolve in THIS binary (#320 / #761).
///
/// `code` is `Some(PREFIX_REFUSAL_CODE)` when the claim is a spec claim
/// (bare id · unknown prefix). `None` when the claim is engine-local
/// (a cataloged vendor this binary does not drive — the azure class).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolveRefusal {
    /// The resolver's own refusal reason.
    pub why: String,
    /// Spec code when the refusal is a spec claim.
    pub code: Option<&'static str>,
}

impl ResolveRefusal {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(why: String) -> Self {
        Self { why, code: None }
    }

    /// Stamp a spec code (consuming builder — `new()` stays frozen).
    #[must_use]
    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }
}

/// The MODELS-rung law (#320): why a `<provider>/<model>` string cannot
/// resolve in THIS binary — `None` when it can. Lives beside the resolver
/// (the set it interrogates is [`CANONICAL_IDS`]) and is shared by every
/// audit surface (CLI check · MCP `nika_check`): a hallucinated model
/// must red the audit on EVERY lane, never only one — the vendor catalog
/// advertising a provider does not make it runnable (the azure class).
///
/// The spec half (#761): a missing or unknown canonical prefix is the
/// FORM law and carries [`PREFIX_REFUSAL_CODE`]. A cataloged vendor this
/// binary cannot drive stays engine-local (`code` is `None`).
#[must_use]
pub fn resolve_refusal(model: &str) -> Option<ResolveRefusal> {
    match model.split_once('/') {
        None => Some(
            ResolveRefusal::new(format!(
                "`{model}` is a bare model id — the contract is `<provider>/<model>` \
                 (pick the provider that serves it; `nika doctor` names the \
                 {} runnable providers)",
                CANONICAL_IDS.len()
            ))
            .with_code(PREFIX_REFUSAL_CODE),
        ),
        Some((provider, _)) if !CANONICAL_IDS.contains(&provider) => {
            // The shared did-you-mean metric (nika-types::suggest — the
            // same threshold the parser/checker suggest with): `antropic`
            // is ONE edit from the most-used provider id, and the rename
            // is the whole fix. Silence past the threshold, as everywhere.
            let guess = nika_types::suggest::did_you_mean(provider, CANONICAL_IDS)
                .map(|p| format!(" — did you mean `{p}`?"))
                .unwrap_or_default();
            let why = format!(
                "provider `{provider}` does not resolve in THIS binary \
                 ({} runnable — `nika doctor` names them); a cataloged \
                 vendor is not a runnable one{guess}",
                CANONICAL_IDS.len()
            );
            let mut refusal = ResolveRefusal::new(why);
            // Spec claim: the prefix is not a known vendor at all.
            // Engine-local: the vendor is cataloged but this binary
            // cannot drive it (azure · moonshot-until-wired · aliases).
            if nika_catalog::find_provider(provider).is_none() {
                refusal = refusal.with_code(PREFIX_REFUSAL_CODE);
            }
            Some(refusal)
        }
        Some(_) => None,
    }
}

/// The MODELS-rung catalog cross-check (audit UX 2026-07-31 · the
/// two-strike class): `anthropic/claude-4-nonexistent` RESOLVES (the
/// provider is runnable) and the audit said `✔ MODELS` — the user buys
/// a key, and only THEN meets the typo at the provider.
///
/// `Some(warning)` when the model name matches NOTHING this binary
/// knows for a PRICED provider — neither of the two lanes the run
/// itself trusts: the catalog row's nicknames + wire ids (what
/// [`Profile::resolve_model`] maps at run, so `mistral/small` and
/// `anthropic/sonnet` are the binary's OWN teaching, never ghosts) and
/// the pricing snapshot's patterns. `None` otherwise. A warning, never
/// a refusal: the snapshot is a dated artifact and providers ship new
/// models weekly — the message says exactly that. Providers with no
/// priced rows (the local five · mock) are never judged: their models
/// are whatever the user pulled, and the catalog cannot know them.
///
/// Lives beside [`resolve_refusal`] for the same reason it does: ONE
/// law, consulted by every audit surface (CLI check · MCP `nika_check`).
#[must_use]
pub fn catalog_warning(model: &str) -> Option<String> {
    let (provider, name) = model.split_once('/')?;
    if !CANONICAL_IDS.contains(&provider) {
        return None; // resolve_refusal owns the unknown-provider class
    }
    // Lane 1 — the run lane's own names. The first cut of this law
    // consulted the pricing snapshot ONLY and warned on
    // `anthropic/sonnet` — a nickname the binary itself teaches and
    // maps to a wire id at run (binary probe, 2026-07-31).
    let catalog_row = nika_catalog::find_provider(provider);
    let row_models = catalog_row.map_or(&[][..], |row| row.models);
    if row_models.iter().any(|m| m.id == name || m.model == name) {
        return None;
    }
    // Lane 2 — the pricing snapshot (exact, then contains: the pass
    // that prices dated variants absorbs their near-typos too — the
    // conjured-price trade-off lives in the pricing layer, documented
    // at `find_pricing`, deliberately not re-judged here).
    let priced: Vec<&'static str> = nika_catalog::all_pricing()
        .iter()
        .filter(|p| p.provider.eq_ignore_ascii_case(provider))
        .map(|p| p.model_pattern)
        .collect();
    if priced.is_empty() {
        return None; // local/mock class — models are whatever was pulled
    }
    if nika_catalog::find_pricing_for(model).is_some() {
        return None;
    }
    // The same shared metric as the provider guess above — over the
    // UNION of what both lanes know (when the near miss is a nickname,
    // the nickname is the right suggestion).
    let guess = nika_types::suggest::did_you_mean(
        name,
        row_models
            .iter()
            .flat_map(|m| [m.id, m.model])
            .chain(priced.iter().copied()),
    )
    .map(|m| format!(" — did you mean `{provider}/{m}`?"))
    .unwrap_or_default();
    let snapshot = nika_catalog::pricing_snapshot();
    let known = priced.len()
        + row_models
            .iter()
            .filter(|m| !priced.contains(&m.model))
            .count();
    Some(format!(
        "`{model}` resolves (the provider is runnable) but matches none of \
         `{provider}`'s {known} known models — a typo, or newer than this \
         binary's snapshot ({}); a run would fail at the provider{guess}",
        snapshot.as_of,
    ))
}

/// The 10 cloud rows (catalog-backed) + the in-process mock.
///
/// gemini's `base_url` is a STEM (`…/v1beta`) — the s8.6 adapter appends
/// `/models/{model}:generateContent` per request (unlike the other wires,
/// whose `base_url` is the complete endpoint).
const CATALOG_WIRED: [(&str, WireFormat, &str); 11] = [
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

/// Build the canonical 16 profiles (catalog-joined where rows exist).
#[must_use]
pub fn seed() -> Vec<Profile> {
    let mut out = Vec::with_capacity(16);
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
        assert!(bare.why.contains("bare model id") && bare.why.contains("16 runnable"));
        // cataloged-but-unresolvable provider — the azure class
        let azure = resolve_refusal("azure/gpt-4o").expect("azure refused");
        assert!(azure.why.contains("`azure`") && azure.why.contains("not a runnable one"));
        // azure is far from every canonical id — no guess appended
        assert!(!azure.why.contains("did you mean"), "{}", azure.why);
        // every canonical provider clears, inner slashes included
        assert!(resolve_refusal("mock/echo").is_none());
        assert!(resolve_refusal("huggingface/Qwen/Qwen3.5-9B:groq").is_none());
    }

    /// #761: the prefix half is a spec claim; a cataloged vendor this
    /// binary cannot drive stays engine-local.
    #[test]
    fn resolve_refusal_stamps_the_prefix_code_and_spares_the_azure_class() {
        let bare = resolve_refusal("gpt-5-turbo").expect("bare id refused");
        assert_eq!(bare.code, Some(PREFIX_REFUSAL_CODE));
        let unknown = resolve_refusal("not-a-provider/gpt-4").expect("unknown prefix refused");
        assert_eq!(unknown.code, Some(PREFIX_REFUSAL_CODE));
        let azure = resolve_refusal("azure/gpt-4o").expect("azure refused");
        assert_eq!(azure.code, None, "cataloged-but-unresolvable stays local");
        let typo = resolve_refusal("antropic/claude-sonnet-4-6").expect("typo refused");
        assert_eq!(typo.code, Some(PREFIX_REFUSAL_CODE));
        assert!(resolve_refusal("mock/echo").is_none());
    }

    #[test]
    fn provider_typo_gets_the_rename() {
        // The sweep's mute surface (2026-07-11): `antropic` is ONE edit
        // from the most-used provider id — the refusal now carries the
        // rename, through the SAME shared metric as the parser/checker.
        let typo = resolve_refusal("antropic/claude-sonnet-4-6").expect("typo refused");
        assert!(
            typo.why.contains("did you mean `anthropic`?"),
            "{}",
            typo.why
        );
        let gemni = resolve_refusal("gemni/gemini-2.5-flash").expect("typo refused");
        assert!(
            gemni.why.contains("did you mean `gemini`?"),
            "{}",
            gemni.why
        );
    }

    /// The two-strike class (audit UX 2026-07-31): a ghost model on a
    /// RUNNABLE provider resolved green, the user bought a key, and only
    /// then met the typo. The catalog cross-check warns at audit time —
    /// and stays honestly a WARNING (the snapshot is dated; providers
    /// ship new models weekly).
    #[test]
    fn catalog_warning_catches_the_ghost_and_spares_the_living() {
        // The exact audit specimen: provider runnable, model nowhere in
        // the snapshot — warned, with the snapshot date named.
        let ghost = catalog_warning("anthropic/claude-4-nonexistent").expect("ghost warned");
        assert!(
            ghost.contains("matches none of `anthropic`'s")
                && ghost.contains("newer than this binary's snapshot"),
            "{ghost}"
        );
        // A cataloged model never warns (exact then contains — the same
        // one resolution the COST rung prices with).
        assert!(catalog_warning("openai/gpt-4o-mini").is_none());
        // The run lane's OWN names never warn: catalog-row nicknames
        // and wire ids are what `resolve_model` maps at run — the first
        // cut of this law warned on `anthropic/sonnet`, a nickname the
        // binary itself teaches (binary probe, 2026-07-31).
        assert!(catalog_warning("anthropic/sonnet").is_none());
        assert!(catalog_warning("mistral/small").is_none());
        assert!(catalog_warning("mistral/mistral-small-latest").is_none());
        assert!(catalog_warning("anthropic/claude-sonnet-4-20250514").is_none());
        // A nickname near-miss warns AND suggests the nickname.
        let sonett = catalog_warning("anthropic/sonett").expect("nickname typo warned");
        assert!(
            sonett.contains("did you mean `anthropic/sonnet`?"),
            "{sonett}"
        );
        // Local providers carry whatever was pulled: never judged.
        assert!(catalog_warning("ollama/qwen3.5:4b").is_none());
        assert!(catalog_warning("llamacpp/anything-at-all").is_none());
        // mock and the not-our-class shapes stay silent (resolve_refusal
        // owns bare ids and unknown providers).
        assert!(catalog_warning("mock/echo").is_none());
        assert!(catalog_warning("gpt-4o-mini").is_none());
        assert!(catalog_warning("azure/gpt-4o").is_none());
    }

    use super::*;

    #[test]
    fn seed_yields_the_canonical_fourteen() {
        let profiles = seed();
        assert_eq!(profiles.len(), 16);
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

#[cfg(test)]
mod access_tests {
    use crate::registry::ProviderRegistry;
    use nika_types::access::{AccessClass, BillingClass};

    #[test]
    fn every_canonical_profile_partitions_into_local_api_mock() {
        let reg = ProviderRegistry::without_http(crate::ProvidersConfig::default());
        let (mut local, mut api, mut mock) = (0u32, 0u32, 0u32);
        for p in reg.profiles() {
            match p.access_class() {
                AccessClass::Local => {
                    local += 1;
                    // every Local profile is a keyless server-backed engine
                    assert!(!p.requires_key, "{} local but requires a key", p.id);
                }
                AccessClass::Api => {
                    api += 1;
                    assert!(p.requires_key, "{} api but keyless", p.id);
                }
                AccessClass::Mock => {
                    mock += 1;
                    assert_eq!(p.id, "mock");
                }
                other => panic!("unexpected access class {other} for {}", p.id),
            }
        }
        // The canonical partition (5 local servers · 1 mock · the rest api)
        // — id-swap mutants die on the exact counts.
        assert_eq!(local, 5);
        assert_eq!(mock, 1);
        assert!(api >= 10, "cloud rows collapsed: {api}");
    }

    #[test]
    fn named_spot_checks_pin_the_classification() {
        let reg = ProviderRegistry::without_http(crate::ProvidersConfig::default());
        let class_of = |id: &str| {
            reg.profiles()
                .iter()
                .find(|p| p.id == id)
                .unwrap_or_else(|| panic!("{id} missing"))
                .access_class()
        };
        assert_eq!(class_of("ollama"), AccessClass::Local);
        assert_eq!(class_of("vllm"), AccessClass::Local);
        assert_eq!(class_of("mistral"), AccessClass::Api);
        assert_eq!(class_of("openrouter"), AccessClass::Api);
        assert_eq!(class_of("mock"), AccessClass::Mock);
    }

    #[test]
    fn default_billing_rides_the_class_honestly() {
        let reg = ProviderRegistry::without_http(crate::ProvidersConfig::default());
        let billing_of = |id: &str| {
            reg.profiles()
                .iter()
                .find(|p| p.id == id)
                .unwrap_or_else(|| panic!("{id} missing"))
                .access_class()
                .default_billing()
        };
        // local compute is unpriced-not-free · api keys are metered USD
        assert_eq!(billing_of("llamacpp"), BillingClass::Local);
        assert_eq!(billing_of("mock"), BillingClass::Local);
        assert_eq!(billing_of("groq"), BillingClass::ApiMetered);
    }
}
