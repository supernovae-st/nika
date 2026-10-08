// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authored access selection — `run.access` (route · protocol ·
//! fallback) and `run.reasoning.effort` — and the receipt of how one call
//! carried it (requested · transmitted · configured · responder attested
//! or unknown). The requirement and the observation are separate types,
//! so a read-back is never mistaken for what the author asked.

use core::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::AccessClass;

/// `run.access.protocol` — HOW the engine talks to the route, apart from
/// WHICH route (`via`) and the model. `api`: a direct endpoint (a key · a
/// local server · the mock, on the same provider door); `acp`: an ACP
/// session with an agent application, whose adapter may launch its CLI.
/// A direct CLI one-shot is neither and never satisfies `acp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum AccessProtocol {
    /// A direct API endpoint.
    Api,
    /// An ACP session with an agent application.
    Acp,
}

impl AccessProtocol {
    /// Every protocol, in the order the teaching lines list them.
    pub const ALL: [Self; 2] = [Self::Api, Self::Acp];

    /// The `snake_case` wire form (the file's own word).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Acp => "acp",
        }
    }

    /// The protocol an exact file word names; no other spelling is one.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == word)
    }

    /// Whether a path class is reached over this protocol: `acp` only via
    /// an agent seat, `api` only via a key, local or mock path. OAuth has
    /// no adapter in this build, so neither claims it (fail closed).
    #[must_use]
    pub const fn admits(self, class: AccessClass) -> bool {
        match self {
            Self::Acp => matches!(class, AccessClass::Harness),
            Self::Api => matches!(
                class,
                AccessClass::Api | AccessClass::Local | AccessClass::Mock
            ),
        }
    }
}

impl fmt::Display for AccessProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `run.access.fallback` — only `none`: an explicit selection is exact and
/// a failure refuses, never another route, protocol, model or effort.
/// Alternatives are not in the language, so omitted behaves as `none`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum AccessFallback {
    /// No alternative, ever.
    None,
}

impl AccessFallback {
    /// The wire form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
        }
    }

    /// The fallback an exact file word names.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        (word == "none").then_some(Self::None)
    }
}

/// The authored requirement — `run.access` + `run.reasoning.effort`, the
/// file's words through resolution, dispatch, receipts and resume. Never
/// an observation (what a call sent and read back is
/// [`SelectionEvidence`]); the effort is the route's NATIVE value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct AccessRequirement {
    /// `run.access.via` — one route id (agent application or provider).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub via: Option<alloc::string::String>,
    /// `run.access.protocol`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub protocol: Option<AccessProtocol>,
    /// `run.access.fallback`, as authored (absent behaves as `none`).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub fallback: Option<AccessFallback>,
    /// `run.reasoning.effort`, verbatim.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub effort: Option<alloc::string::String>,
}

impl AccessRequirement {
    /// An empty requirement (INV-019).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The route id.
    #[must_use]
    pub fn with_via(mut self, via: Option<alloc::string::String>) -> Self {
        self.via = via;
        self
    }

    /// The protocol.
    #[must_use]
    pub const fn with_protocol(mut self, protocol: Option<AccessProtocol>) -> Self {
        self.protocol = protocol;
        self
    }

    /// The fallback, as authored.
    #[must_use]
    pub const fn with_fallback(mut self, fallback: Option<AccessFallback>) -> Self {
        self.fallback = fallback;
        self
    }

    /// The native reasoning effort.
    #[must_use]
    pub fn with_effort(mut self, effort: Option<alloc::string::String>) -> Self {
        self.effort = effort;
        self
    }

    /// Whether the requirement constrains the PATH (a route or a
    /// protocol); an effort-only requirement keeps today's resolution.
    #[must_use]
    pub const fn selects_path(&self) -> bool {
        self.via.is_some() || self.protocol.is_some()
    }

    /// The behavior-bearing identity a resume key folds: route, protocol
    /// and effort in a fixed order, absent dimensions omitted. `fallback`
    /// has one behavior today, so it changes nothing a task produces.
    #[must_use]
    pub fn identity(&self) -> alloc::string::String {
        let mut parts = alloc::vec::Vec::new();
        if let Some(via) = &self.via {
            parts.push(alloc::format!("via={via}"));
        }
        if let Some(protocol) = self.protocol {
            parts.push(alloc::format!("protocol={protocol}"));
        }
        if let Some(effort) = &self.effort {
            parts.push(alloc::format!("effort={effort}"));
        }
        parts.join(";")
    }

    /// The `access_requirement` receipt (absent fields omitted).
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// One dimension (model or effort) as one call carried it, the facts kept
/// apart; `None` is not applicable or not observed, never a copied fact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct SelectedValue {
    /// What the author or caller asked, verbatim.
    pub requested: Option<alloc::string::String>,
    /// The route's option or field it was sent through (ACP config id · API key).
    pub option: Option<alloc::string::String>,
    /// The exact value sent.
    pub transmitted: Option<alloc::string::String>,
    /// What the route reported current after the selection (ACP read-back).
    pub configured: Option<alloc::string::String>,
    /// How `configured` was learned (`session_config` ·
    /// `confirmed_selection` · `accepted_request`).
    pub configured_source: Option<alloc::string::String>,
}

impl SelectedValue {
    /// Nothing asked, sent or read (INV-019).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The requested value.
    #[must_use]
    pub fn requested(mut self, value: Option<alloc::string::String>) -> Self {
        self.requested = value;
        self
    }

    /// The option id and the exact value sent through it.
    #[must_use]
    pub fn transmitted(
        mut self,
        option: Option<alloc::string::String>,
        value: Option<alloc::string::String>,
    ) -> Self {
        self.option = option;
        self.transmitted = value;
        self
    }

    /// The read-back value and how it was learned.
    #[must_use]
    pub fn configured(
        mut self,
        value: Option<alloc::string::String>,
        source: Option<alloc::string::String>,
    ) -> Self {
        self.configured = value;
        self.configured_source = source;
        self
    }

    #[cfg(feature = "serde")]
    fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// What ONE call's selection became (the `access_selection` receipt): the
/// protocol travelled, model and effort facts, responder attested or unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct SelectionEvidence {
    /// The protocol the call travelled; `None` for a direct CLI one-shot.
    pub protocol: Option<AccessProtocol>,
    /// The model dimension.
    pub model: SelectedValue,
    /// The reasoning-effort dimension.
    pub effort: SelectedValue,
    /// The responder as the route named it in its answer; `None` is unknown
    /// (ACP names none), never the requested or configured model.
    pub responder: Option<alloc::string::String>,
    /// Where `responder` came from (`api_response` · `cli_reported`).
    pub responder_source: Option<alloc::string::String>,
    /// What the route moved mid-call (`model=<v>` · `effort=<v>`, in order).
    pub changed_mid_turn: alloc::vec::Vec<alloc::string::String>,
}

impl SelectionEvidence {
    /// The schema the receipt names.
    pub const SCHEMA: &'static str = "nika/access-selection@1";

    /// Evidence for a call over `protocol` (INV-019).
    #[must_use]
    pub fn new(protocol: Option<AccessProtocol>) -> Self {
        Self {
            protocol,
            ..Self::default()
        }
    }

    /// The model dimension.
    #[must_use]
    pub fn with_model(mut self, model: SelectedValue) -> Self {
        self.model = model;
        self
    }

    /// The effort dimension.
    #[must_use]
    pub fn with_effort(mut self, effort: SelectedValue) -> Self {
        self.effort = effort;
        self
    }

    /// The responder the route named, and where it came from.
    #[must_use]
    pub fn with_responder(
        mut self,
        model: Option<alloc::string::String>,
        source: &'static str,
    ) -> Self {
        self.responder_source = model.as_ref().map(|_| source.into());
        self.responder = model;
        self
    }

    /// Record what the route moved mid-call.
    #[must_use]
    pub fn with_changes(mut self, changes: alloc::vec::Vec<alloc::string::String>) -> Self {
        self.changed_mid_turn = changes;
        self
    }

    /// The receipt (`responder.evidence` is `unknown` when absent;
    /// `changed_mid_turn` rides only when the route moved something).
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let mut row = serde_json::json!({
            "schema": Self::SCHEMA,
            "protocol": self.protocol.map(AccessProtocol::as_str),
            "model": self.model.to_json(),
            "effort": self.effort.to_json(),
            "responder": {
                "model": self.responder,
                "evidence": self.responder_source.as_deref().unwrap_or("unknown"),
            },
        });
        if !self.changed_mid_turn.is_empty()
            && let Some(object) = row.as_object_mut()
        {
            object.insert(
                "changed_mid_turn".into(),
                serde_json::json!(self.changed_mid_turn),
            );
        }
        row
    }
}
