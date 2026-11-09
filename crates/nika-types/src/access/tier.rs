// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The service tier a run asks of its route (`run.access.service_tier`):
//! the language's two words, the measured binding of those words to an
//! adapter's own speed control, and the facts one call kept about it.
//!
//! The words are the language's, never a route's. A route that offers a
//! speed control binds each word to its own native setting; a route with
//! no binding honors neither word. Omission is not `standard`: it keeps the
//! route's default tier, which may be either and may stay unknown.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// `run.access.service_tier` — how fast the route is asked to serve, apart
/// from WHICH route (`via`), the model and the reasoning effort. A closed
/// vocabulary: a route's own word for its speed control (`on`, `priority`,
/// a boolean) is never one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum ServiceTier {
    /// The route's regular serving tier.
    Standard,
    /// The route's faster serving tier, where it offers one for the selected model.
    Fast,
}

impl ServiceTier {
    /// Every tier, in the order the teaching lines list them.
    pub const ALL: [Self; 2] = [Self::Standard, Self::Fast];

    /// The `snake_case` wire form (the file's own word).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Fast => "fast",
        }
    }

    /// The tier an exact file word names; no other spelling is one.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tier| tier.as_str() == word)
    }
}

impl fmt::Display for ServiceTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One measured binding of the two words to an adapter's own speed control —
/// the ONE table the client (which selects and reads back the option) and the
/// access resolver (which refuses a tier no route can carry) both read.
///
/// A row exists only for an adapter release whose option was read from its
/// source and from a live `session/new` answer. An adapter without a row,
/// or another release of a measured adapter, cannot honor a declared tier,
/// whatever its sessions advertise: what a native value means on a turn
/// belongs to the release and to the agent it bundles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ServiceTierBinding {
    /// The seat token this adapter serves ([`super::HarnessRuntime::id`]).
    pub seat: &'static str,
    /// The `agentInfo.name` values the measurement holds for — the adapter's own report on
    /// `initialize`, never the command that spawned it.
    pub agents: &'static [&'static str],
    /// The exact `agentInfo.version` values the measurement holds for.
    pub versions: &'static [&'static str],
    /// The adapter release the measurement was made on, as a person reads it.
    pub measured_on: &'static str,
    /// The session configuration option that carries the tier, by its exact id.
    pub option: &'static str,
    /// The category that option is advertised under. Other options share it, so the client
    /// matches the id AND the category, never the category alone.
    pub category: &'static str,
    /// The native value that selects the fast tier.
    pub fast: &'static str,
    /// The native value that selects the standard tier.
    pub standard: &'static str,
}

impl ServiceTierBinding {
    /// codex-acp 1.13.1 (it bundles codex 0.156.1). Its `fast-mode` option, category
    /// `model_config`, is a select with the values `off` and `on` unless the client negotiates
    /// booleans, which this client does not. The adapter lists the option only when the
    /// session's current model supports a fast tier and recomputes it when the model changes,
    /// so the offer is read after the model is selected. Its setter stores `on` without
    /// checking that support: an accepted setter is not an offer, and the client requires the
    /// option in the configuration before it sends and in the answer after. On each turn the
    /// adapter then asks Codex for its `fast` service tier when the option is `on` and the
    /// model supports it, and sends an explicit null otherwise, which that Codex reads as
    /// standard routing; Codex applies either only while its own `fast_mode` feature is on.
    /// Nothing a client can read names the tier a turn carried or the tier that served it.
    pub const CODEX_ACP: Self = Self {
        seat: "codex",
        agents: &["@agentclientprotocol/codex-acp", "codex-acp"],
        versions: &["1.13.1"],
        measured_on: "codex-acp 1.13.1",
        option: "fast-mode",
        category: "model_config",
        fast: "on",
        standard: "off",
    };

    /// Every measured adapter.
    pub const MEASURED: [Self; 1] = [Self::CODEX_ACP];

    /// The measured binding of the adapter that reported `agent_name` on `initialize`,
    /// whatever its release: [`Self::measures`] then judges the release.
    #[must_use]
    pub fn for_agent(agent_name: &str) -> Option<Self> {
        Self::MEASURED
            .into_iter()
            .find(|row| row.agents.contains(&agent_name))
    }

    /// Whether the measurement holds for the release an adapter reported on `initialize`
    /// (`agentInfo.version`), compared exactly: no range, no prefix.
    #[must_use]
    pub fn measures(&self, version: &str) -> bool {
        self.versions.contains(&version)
    }

    /// The measured binding of the seat `id` (`codex`).
    #[must_use]
    pub fn for_seat(id: &str) -> Option<Self> {
        Self::MEASURED.into_iter().find(|row| row.seat == id)
    }

    /// The native value this adapter takes for `tier`.
    #[must_use]
    pub const fn native(&self, tier: ServiceTier) -> &'static str {
        match tier {
            ServiceTier::Standard => self.standard,
            ServiceTier::Fast => self.fast,
        }
    }

    /// The tier a native value of this adapter's option means, if it is one of the two.
    #[must_use]
    pub fn tier_of(&self, native: &str) -> Option<ServiceTier> {
        ServiceTier::ALL
            .into_iter()
            .find(|tier| self.native(*tier) == native)
    }
}

/// What ONE call's declared service tier became (the `service_tier` block of the
/// `access_selection` receipt), the facts kept apart. Present on a receipt only when the file
/// declared a tier: a run that asked for none records none.
///
/// `turn` and `effective` are the two facts a configuration cannot establish: the tier the
/// route carried on the turn itself, and the tier the provider reports having served. Each
/// stays unknown until a witness attests it. A confirmed configuration is never either.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct ServiceTierEvidence {
    /// The word the file declared.
    pub requested: Option<ServiceTier>,
    /// The route's option the tier was sent through, by its exact id.
    pub option: Option<String>,
    /// The native values the route listed for that option when the tier was judged, in the
    /// route's own order. `None`: no offer was observed.
    pub offered: Option<Vec<String>>,
    /// The exact native value sent.
    pub transmitted: Option<String>,
    /// The native value the route reported current after the selection (ACP read-back).
    pub configured: Option<String>,
    /// How `configured` was learned (`confirmed_selection`).
    pub configured_source: Option<String>,
    /// The tier carried on the turn itself, when a witness attests it.
    pub turn: Option<String>,
    /// Where `turn` came from; `Some` exactly when `turn` is.
    pub turn_source: Option<String>,
    /// The tier the provider reported serving, when it reports one.
    pub effective: Option<String>,
    /// Where `effective` came from; `Some` exactly when `effective` is.
    pub effective_source: Option<String>,
}

impl ServiceTierEvidence {
    /// Evidence for a call whose file declared `requested` (INV-019).
    #[must_use]
    pub fn new(requested: ServiceTier) -> Self {
        Self {
            requested: Some(requested),
            ..Self::default()
        }
    }

    /// The native values the route offered for `option` when the tier was judged.
    #[must_use]
    pub fn offered(mut self, option: Option<String>, values: Option<Vec<String>>) -> Self {
        self.option = option;
        self.offered = values;
        self
    }

    /// The exact native value sent.
    #[must_use]
    pub fn transmitted(mut self, value: Option<String>) -> Self {
        self.transmitted = value;
        self
    }

    /// The read-back value and how it was learned.
    #[must_use]
    pub fn configured(mut self, value: Option<String>, source: Option<String>) -> Self {
        self.configured = value;
        self.configured_source = source;
        self
    }

    /// The tier a witness saw on the turn, and where the witness came from. Without a value
    /// the fact stays unknown and the source is dropped.
    #[must_use]
    pub fn with_turn(mut self, value: Option<String>, source: &'static str) -> Self {
        self.turn_source = value.as_ref().map(|_| source.into());
        self.turn = value;
        self
    }

    /// The tier the provider reported, and where the report came from. Without a value the
    /// fact stays unknown and the source is dropped.
    #[must_use]
    pub fn with_effective(mut self, value: Option<String>, source: &'static str) -> Self {
        self.effective_source = value.as_ref().map(|_| source.into());
        self.effective = value;
        self
    }

    /// The receipt block. An absent fact is a visible `null`, never omitted nor copied from
    /// another; `evidence` reads `unknown` exactly when its `value` is `null`.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "requested": self.requested.map(ServiceTier::as_str),
            "option": self.option,
            "offered": self.offered,
            "transmitted": self.transmitted,
            "configured": self.configured,
            "configured_source": self.configured_source,
            "turn": {
                "value": self.turn,
                "evidence": self.turn_source.as_deref().unwrap_or("unknown"),
            },
            "effective": {
                "value": self.effective,
                "evidence": self.effective_source.as_deref().unwrap_or("unknown"),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tier_vocabulary_is_two_exact_words() {
        assert_eq!(ServiceTier::parse("standard"), Some(ServiceTier::Standard));
        assert_eq!(ServiceTier::parse("fast"), Some(ServiceTier::Fast));
        // No case folding, no padding, and no route-native word is a tier.
        for word in [
            "Fast", "FAST", " fast", "fast ", "on", "off", "true", "priority", "flex", "default",
            "auto", "",
        ] {
            assert_eq!(ServiceTier::parse(word), None, "{word:?}");
        }
        assert_eq!(ServiceTier::Fast.to_string(), "fast");
        assert_eq!(ServiceTier::Standard.to_string(), "standard");
        assert_eq!(
            ServiceTier::ALL.map(ServiceTier::as_str),
            ["standard", "fast"]
        );
    }

    #[test]
    fn the_codex_binding_names_its_option_and_both_native_values() {
        let binding = ServiceTierBinding::for_seat("codex").expect("measured");
        assert_eq!(binding, ServiceTierBinding::CODEX_ACP);
        assert_eq!(binding.option, "fast-mode");
        assert_eq!(binding.category, "model_config");
        assert_eq!(binding.native(ServiceTier::Fast), "on");
        assert_eq!(binding.native(ServiceTier::Standard), "off");
        assert_eq!(binding.tier_of("on"), Some(ServiceTier::Fast));
        assert_eq!(binding.tier_of("off"), Some(ServiceTier::Standard));
        // The language's own words are not this adapter's values.
        for native in ["fast", "standard", "true", "priority", ""] {
            assert_eq!(binding.tier_of(native), None, "{native:?}");
        }
        for name in ["@agentclientprotocol/codex-acp", "codex-acp"] {
            assert_eq!(ServiceTierBinding::for_agent(name), Some(binding), "{name}");
        }
    }

    #[test]
    fn the_measurement_holds_for_its_exact_release_only() {
        let binding = ServiceTierBinding::CODEX_ACP;
        assert_eq!(binding.versions, ["1.13.1"]);
        assert!(binding.measures("1.13.1"));
        // No range and no prefix: a later release bundles another agent, whose reading of the
        // same native value is unmeasured.
        for version in [
            "1.13.0", "1.13.10", "1.13", "1.14.0", "2.6.0", "v1.13.1", "",
        ] {
            assert!(!binding.measures(version), "{version:?}");
        }
    }

    #[test]
    fn an_adapter_without_a_measured_row_binds_nothing() {
        for seat in ["claude-code", "gemini-cli", "kimi-code", "openai", ""] {
            assert_eq!(ServiceTierBinding::for_seat(seat), None, "{seat:?}");
        }
        for name in [
            "@agentclientprotocol/claude-agent-acp",
            "gemini-cli",
            "Codex-ACP",
            "",
        ] {
            assert_eq!(ServiceTierBinding::for_agent(name), None, "{name:?}");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn the_tier_block_keeps_every_fact_apart_and_unknown_facts_visible() {
        let configured = ServiceTierEvidence::new(ServiceTier::Fast)
            .offered(
                Some("fast-mode".into()),
                Some(vec!["off".into(), "on".into()]),
            )
            .transmitted(Some("on".into()))
            .configured(Some("on".into()), Some("confirmed_selection".into()));
        assert_eq!(
            configured.to_json(),
            serde_json::json!({
                "requested": "fast",
                "option": "fast-mode",
                "offered": ["off", "on"],
                "transmitted": "on",
                "configured": "on",
                "configured_source": "confirmed_selection",
                "turn": {"value": null, "evidence": "unknown"},
                "effective": {"value": null, "evidence": "unknown"},
            })
        );
        // Nothing sent or read: the request alone, every other fact a visible null.
        assert_eq!(
            ServiceTierEvidence::new(ServiceTier::Standard).to_json(),
            serde_json::json!({
                "requested": "standard",
                "option": null,
                "offered": null,
                "transmitted": null,
                "configured": null,
                "configured_source": null,
                "turn": {"value": null, "evidence": "unknown"},
                "effective": {"value": null, "evidence": "unknown"},
            })
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn a_witnessed_fact_names_its_source_and_an_absent_one_stays_unknown() {
        let witnessed = ServiceTierEvidence::new(ServiceTier::Fast)
            .with_turn(Some("fast".into()), "adapter_witness")
            .with_effective(None, "provider_response");
        let json = witnessed.to_json();
        assert_eq!(
            json["turn"],
            serde_json::json!({"value": "fast", "evidence": "adapter_witness"})
        );
        // A source without a value is dropped: evidence never names a witness that saw nothing.
        assert_eq!(
            json["effective"],
            serde_json::json!({"value": null, "evidence": "unknown"})
        );
        assert_eq!(witnessed.effective_source, None);
    }
}
