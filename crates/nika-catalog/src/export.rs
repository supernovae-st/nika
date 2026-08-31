// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The catalog wire projection — what `nika catalog --json` and the MCP
//! `nika_catalog` tool emit.
//!
//! The embedded provider catalog (providers · models · capabilities) is
//! typed and build-time generated, but until this module it left the
//! binary on no wire — every IDE/agent consumer re-bundled its own copy
//! and drifted generationally. This projection is the single machine
//! surface; clients paint it, never re-derive it.
//!
//! # Wire contract
//!
//! Versioned envelope: `catalog_version: 1`. Evolution is ADDITIVE-ONLY —
//! new fields may appear, existing fields never change meaning or
//! disappear within a version. Consumers must ignore unknown fields.
//!
//! Pricing is deliberately NOT part of this payload — rates live in the
//! feature-gated pricing catalog (TOML-externalised) and are never
//! hardcoded into client bundles.
//!
//! The structs serialize with `serde` only (no `Deserialize`): this is an
//! OUTPUT-ONLY projection. Wire consumers parse JSON; Rust consumers
//! (Olympus cockpit · future cortex overlays) read the typed views.

use serde::Serialize;

use crate::types::{JsonMode, Modality, Provider, ProviderModel, Tag};

/// Version marker of the `nika catalog --json` envelope. Additive-only.
pub const CATALOG_EXPORT_VERSION: u32 = 1;

/// The full catalog projection (`catalog_version` + every provider).
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct CatalogExport {
    /// Envelope version (see [`CATALOG_EXPORT_VERSION`]).
    pub catalog_version: u32,
    /// Every embedded provider, in catalog order.
    pub providers: Vec<ProviderExport>,
}

/// One provider entry of the wire projection.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct ProviderExport {
    /// Canonical id (`"anthropic"`, always lowercase).
    pub id: &'static str,
    /// Human-readable name (`"Anthropic Claude"`).
    pub name: &'static str,
    /// Alternative names resolving to this provider (`["claude"]`).
    pub aliases: &'static [&'static str],
    /// Environment variable carrying the API key (`"ANTHROPIC_API_KEY"`).
    /// The NAME only — no surface here ever reads or emits the value.
    pub env_var: &'static str,
    /// Whether the provider needs an API key at all.
    pub requires_key: bool,
    /// Whether this provider runs locally (mirrors the `local` tag —
    /// sovereignty-relevant: local providers cost zero keys, zero network).
    pub local: bool,
    /// Default model (wire identifier) when the caller names none.
    pub default_model: &'static str,
    /// Cheap/fast model (wire identifier) for repair and cost-sensitive passes.
    pub cheap_model: &'static str,
    /// Short human description.
    pub description: &'static str,
    /// Wire-protocol dialect family (`"openai-chat"` · `"anthropic"` · …),
    /// `None` for bespoke protocols.
    pub api_dialect: Option<&'static str>,
    /// Whether a model of this provider RESOLVES in this build — `false`
    /// means the catalog knows the vendor and the engine carries no
    /// adapter, so `nika check` refuses at the MODELS rung with the same
    /// verb: *"provider `x` does not resolve in THIS binary"*.
    ///
    /// The word is the refusal rung's, on purpose. « wired » already
    /// names a DIFFERENT set one screen away — the run registry behind
    /// `nika_cli_host::machine_truth::MachineTruth::wired`, which
    /// excludes the `mock` test lane. Two sets, two words; a synonym
    /// here would re-create the very A-06 confusion that module exists
    /// to cure.
    ///
    /// Never typed and never readable here: the catalog is L0 data, the
    /// adapters are L1.5 code, and `nika_catalog::export` is gated off
    /// inside `nika-providers` — so the fact can only be STATED by the
    /// L4 surface that holds both. [`CatalogExport::with_resolvable`] is
    /// the ONLY path to `true`, and its one honest argument is
    /// `nika_providers::CANONICAL_IDS`.
    ///
    /// A projection built by [`catalog_export`] alone claims nothing —
    /// every row reads `false`. The two surfaces that emit this
    /// projection to a human or an agent (`nika catalog` · the MCP
    /// `nika_catalog` tool) are pinned to the adapter set by
    /// `nika-cli/tests/catalog_family.rs`.
    pub resolves: bool,
    /// Typed capability/deployment/economics tags, kebab-case.
    pub tags: Vec<&'static str>,
    /// Known models (nickname → wire id) with resolved capabilities.
    pub models: Vec<ModelExport>,
}

/// One model entry of a provider, with its resolved capabilities.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct ModelExport {
    /// Short nickname used by workflows (`"sonnet"`, `"mini"`).
    pub id: &'static str,
    /// Wire identifier sent to the provider API.
    pub model: &'static str,
    /// Maximum context window in tokens (input + output combined).
    pub context_window_tokens: u32,
    /// Maximum tokens the model can emit in one response.
    pub max_output_tokens: u32,
    /// Capabilities resolved through the TOML rule table
    /// ([`crate::model_capabilities`]) against the WIRE identifier.
    pub capabilities: CapabilitiesExport,
}

/// The IDE-relevant capability slice of one model.
///
/// A deliberate SUBSET of [`crate::types::ModelCapabilities`] — the three
/// answers an authoring surface needs for hover/completion (can it see
/// images? does it reason? how strong is its JSON discipline?). More
/// fields arrive additively as consumers need them.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct CapabilitiesExport {
    /// Model exposes a reasoning / extended-thinking mode.
    pub reasoning: bool,
    /// Model accepts image input (vision).
    pub vision: bool,
    /// Structured-output discipline: `"unavailable"` · `"object"` ·
    /// `"schema"` · `null` when the rule table does not know.
    pub json_mode: Option<&'static str>,
}

impl ProviderExport {
    /// The human listing's model cell — wire ids, joined, so `nika catalog`
    /// prints `grok-3` instead of only `2 models` (B18 / issue 1306).
    #[must_use]
    pub fn human_model_ids(&self) -> String {
        self.models
            .iter()
            .map(|m| m.model)
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

impl CatalogExport {
    /// State which provider ids resolve in THIS build.
    ///
    /// The ONE setter of [`ProviderExport::resolves`] — there is no
    /// other path to `true`. The set arrives as a parameter because the
    /// fact lives one layer up (`nika_providers::CANONICAL_IDS`) and an
    /// L0 catalog cannot read it; a hand-maintained mirror of that list
    /// inside the catalog data would be the next drift.
    ///
    /// Ids absent from the catalog are ignored: the runtime carries
    /// engines the catalog data does not, and this projection speaks
    /// only about rows it has.
    #[must_use]
    pub fn with_resolvable(mut self, resolvable_ids: &[&str]) -> Self {
        for provider in &mut self.providers {
            provider.resolves = resolvable_ids.contains(&provider.id);
        }
        self
    }
}

/// Build the full catalog projection from the embedded catalogs.
///
/// Pure over compile-time data: zero I/O, zero network, deterministic
/// for a given binary. Capabilities are resolved per model through
/// [`crate::model_capabilities`] using the provider id + WIRE model id
/// (the rule table matches wire names, not nicknames).
///
/// Every row leaves here `resolves: false` — this function makes no
/// claim about what the engine can reach. A surface a human or an agent
/// reads chains [`CatalogExport::with_resolvable`] over the adapter
/// layer's own id list (`nika_providers::CANONICAL_IDS`) first.
#[must_use]
pub fn catalog_export() -> CatalogExport {
    CatalogExport {
        catalog_version: CATALOG_EXPORT_VERSION,
        providers: crate::all_providers().iter().map(provider_export).collect(),
    }
}

/// Project one provider entry (models resolved through the rule table).
fn provider_export(p: &Provider) -> ProviderExport {
    ProviderExport {
        id: p.id,
        name: p.name,
        aliases: p.aliases,
        env_var: p.env_var,
        requires_key: p.requires_key,
        local: p.tags.iter().any(|t| matches!(t, Tag::Local)),
        default_model: p.default_model,
        cheap_model: p.cheap_model,
        description: p.description,
        api_dialect: p.api_dialect,
        // No claim here — `with_resolvable` is the only setter.
        resolves: false,
        tags: p.tags.iter().map(|t| t.as_str()).collect(),
        models: p.models.iter().map(|m| model_export(p.id, m)).collect(),
    }
}

/// Project one model entry — capabilities resolved against the WIRE id
/// (the rule table's matchers are written for wire names).
fn model_export(provider_id: &str, m: &ProviderModel) -> ModelExport {
    let caps = crate::model_capabilities(provider_id, m.model);
    ModelExport {
        id: m.id,
        model: m.model,
        context_window_tokens: m.context_window_tokens,
        max_output_tokens: m.max_output_tokens,
        capabilities: CapabilitiesExport {
            reasoning: caps.reasoning,
            vision: caps
                .input_modalities
                .iter()
                .any(|m| matches!(m, Modality::Image)),
            json_mode: caps.json_mode.map(JsonMode::as_str),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_is_versioned_and_total() {
        let export = catalog_export();
        assert_eq!(export.catalog_version, CATALOG_EXPORT_VERSION);
        assert_eq!(export.catalog_version, 1, "the locked v1 wire marker");
        assert_eq!(
            export.providers.len(),
            crate::all_providers().len(),
            "every embedded provider is projected — none filtered, none invented",
        );
        assert!(!export.providers.is_empty());
    }

    #[test]
    fn local_flag_mirrors_the_local_tag() {
        let export = catalog_export();
        assert!(
            export.providers.iter().any(|p| p.local),
            "the catalog ships local providers (ollama · lm studio · …)",
        );
        for p in &export.providers {
            let tagged = p.tags.contains(&"local");
            assert_eq!(
                p.local, tagged,
                "provider `{}`: the local flag must mirror the `local` tag",
                p.id,
            );
        }
    }

    #[test]
    fn every_model_carries_resolved_capabilities() {
        let export = catalog_export();
        let mut models_seen = 0usize;
        for p in &export.providers {
            for m in &p.models {
                models_seen += 1;
                assert!(
                    !m.id.is_empty(),
                    "provider `{}`: empty model nickname",
                    p.id
                );
                assert!(!m.model.is_empty(), "provider `{}`: empty wire id", p.id);
                if let Some(mode) = m.capabilities.json_mode {
                    assert!(
                        matches!(mode, "unavailable" | "object" | "schema"),
                        "provider `{}` model `{}`: json_mode `{mode}` outside the closed set",
                        p.id,
                        m.id,
                    );
                }
            }
        }
        assert!(models_seen > 0, "the catalog ships model entries");
        let all_models = || export.providers.iter().flat_map(|p| &p.models);
        assert!(
            all_models().any(|m| m.capabilities.vision),
            "at least one vision-capable model exists in the embedded catalog",
        );
        assert!(
            all_models().any(|m| m.capabilities.reasoning),
            "at least one reasoning model exists in the embedded catalog",
        );
    }

    /// B18 / issue 1306: the human cell prints wire ids (`grok-3`), not
    /// only a count, and gpt-4o-mini has entered the openai row.
    #[test]
    fn human_model_ids_print_the_wire_names() {
        let export = catalog_export();
        let xai = export
            .providers
            .iter()
            .find(|p| p.id == "xai")
            .expect("xai");
        let ids = xai.human_model_ids();
        assert!(
            ids.contains("grok-3"),
            "xai prints grok-3, not only a count: {ids}"
        );
        let openai = export
            .providers
            .iter()
            .find(|p| p.id == "openai")
            .expect("openai");
        assert!(
            openai.models.iter().any(|m| m.model == "gpt-4o-mini"),
            "gpt-4o-mini entered the openai catalog: {:?}",
            openai.models.iter().map(|m| m.model).collect::<Vec<_>>()
        );
        assert!(
            openai.human_model_ids().contains("gpt-4o-mini"),
            "{}",
            openai.human_model_ids()
        );
    }

    #[test]
    fn wire_shape_is_the_locked_contract() {
        let export = catalog_export();
        let value = serde_json::to_value(&export).expect("projection serializes");
        let top = value.as_object().expect("top level is an object");
        assert_eq!(top.len(), 2, "v1 envelope carries exactly two keys");
        assert!(top.contains_key("catalog_version"));
        assert!(top.contains_key("providers"));

        let first = value["providers"][0]
            .as_object()
            .expect("provider entries are objects");
        for key in [
            "id",
            "name",
            "aliases",
            "env_var",
            "requires_key",
            "local",
            "default_model",
            "cheap_model",
            "description",
            "api_dialect",
            "resolves",
            "tags",
            "models",
        ] {
            assert!(first.contains_key(key), "provider entry missing `{key}`");
        }
    }

    #[test]
    fn the_bare_projection_claims_nothing() {
        let export = catalog_export();
        assert!(
            export.providers.iter().all(|p| !p.resolves),
            "catalog_export() alone knows nothing about the adapter set",
        );
    }

    #[test]
    fn with_resolvable_marks_exactly_the_named_ids() {
        let all: Vec<&str> = catalog_export().providers.iter().map(|p| p.id).collect();
        let named = ["anthropic", "mock"];
        let export = catalog_export().with_resolvable(&named);
        for p in &export.providers {
            assert_eq!(
                p.resolves,
                named.contains(&p.id),
                "provider `{}`: resolves must mirror the named set",
                p.id,
            );
        }
        assert_eq!(
            export.providers.iter().filter(|p| p.resolves).count(),
            named.len(),
            "the resolving rows are exactly the named ones, in a catalog of {}",
            all.len(),
        );
    }

    #[test]
    fn an_id_the_catalog_does_not_carry_marks_nothing() {
        let export = catalog_export().with_resolvable(&["a-provider-no-catalog-row-names"]);
        assert!(
            export.providers.iter().all(|p| !p.resolves),
            "the projection speaks only about rows it has",
        );
    }

    #[test]
    fn a_second_call_replaces_the_claim_rather_than_accumulating() {
        // Chaining must not leave a stale `true` behind: the last call
        // IS the build's answer, or a re-annotation would silently keep
        // a provider marked runnable after it left the wire layer.
        let export = catalog_export()
            .with_resolvable(&["anthropic", "mock"])
            .with_resolvable(&["mock"]);
        let resolving: Vec<&str> = export
            .providers
            .iter()
            .filter(|p| p.resolves)
            .map(|p| p.id)
            .collect();
        assert_eq!(resolving, vec!["mock"]);
    }

    #[test]
    fn aliases_resolve_back_to_their_provider() {
        for p in catalog_export().providers {
            for alias in p.aliases {
                let resolved = crate::find_provider(alias).map(|r| r.id);
                assert_eq!(
                    resolved,
                    Some(p.id),
                    "alias `{alias}` must round-trip to provider `{}`",
                    p.id,
                );
            }
        }
    }

    #[test]
    fn mock_catalog_names_the_taught_echo_seat() {
        let export = catalog_export();
        let mock = export
            .providers
            .iter()
            .find(|provider| provider.id == "mock")
            .expect("the embedded catalog carries the mock provider");
        assert!(
            mock.models
                .iter()
                .any(|model| model.id == "echo" && model.model == "echo"),
            "mock/echo is taught by examples and must be discoverable in the catalog",
        );
    }
}
