// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a conversation led by an intelligence holds of the person's request, typed for every
//! host (`nika/session-work@0`, additive): the values the candidate binds and where each comes
//! from, the choices the person delegated, and the questions asked together with their
//! identities. Provenance is a record, never an authorization: only the person's cited lines
//! authorize, and the session checks them itself.
//!
//! Values, provenance and delegations read back as they were written: a session keeps them
//! across a reopen as evidence. Questions are never kept, so their identities are renewed.

use serde::{Deserialize, Serialize};

use crate::outcome::QuestionId;

/// The part a value plays in the workflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ValueRole {
    /// A public source the workflow reads with GET.
    ReadSource,
    /// A file the workflow writes inside the project.
    OutputPath,
    /// The model a run uses.
    RunModel,
    /// Any other value.
    Value,
}

/// Where a bound value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProvenanceKind {
    /// A public source the person named.
    Named,
    /// Chosen within a choice the person delegated.
    Delegated,
    /// A routine new output the person asked for without naming it.
    Derived,
    /// A value of an offer the person accepted.
    Offered,
    /// A value the person typed.
    Answered,
    /// Kept from an earlier accepted revision.
    Retained,
}

/// Where one value comes from: its kind and the person's line (`u1`, `u2`, …) with their words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Provenance {
    /// The kind.
    pub kind: ProvenanceKind,
    /// The citation of the person's line it rests on.
    pub message: String,
    /// The person's words, verbatim, when the kind rests on words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    /// The question it answers or whose offer was accepted, by key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// The accepted option, by key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<String>,
}

impl Provenance {
    /// A provenance (INV-019).
    #[must_use]
    pub fn new(kind: ProvenanceKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            excerpt: None,
            question: None,
            option: None,
        }
    }

    /// The person's words it rests on.
    #[must_use]
    pub fn with_excerpt(mut self, excerpt: impl Into<String>) -> Self {
        self.excerpt = Some(excerpt.into());
        self
    }

    /// The question and option it comes from.
    #[must_use]
    pub fn with_offer(mut self, question: impl Into<String>, option: Option<String>) -> Self {
        self.question = Some(question.into());
        self.option = option;
        self
    }
}

/// One value the candidate binds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Binding {
    /// The question key it fills, when one asked for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Its part in the workflow.
    pub role: ValueRole,
    /// The value, exactly.
    pub value: String,
    /// Where it comes from.
    pub provenance: Provenance,
}

impl Binding {
    /// A binding (INV-019).
    #[must_use]
    pub fn new(role: ValueRole, value: impl Into<String>, provenance: Provenance) -> Self {
        Self {
            key: None,
            role,
            value: value.into(),
            provenance,
        }
    }

    /// The question key it fills.
    #[must_use]
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

/// A choice the person delegated, with their words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Delegation {
    /// The citation of the line that delegates.
    pub message: String,
    /// The person's words, verbatim.
    pub excerpt: String,
    /// What the delegation covers.
    pub scope: ValueRole,
}

impl Delegation {
    /// A delegation (INV-019).
    #[must_use]
    pub fn new(message: impl Into<String>, excerpt: impl Into<String>, scope: ValueRole) -> Self {
        Self {
            message: message.into(),
            excerpt: excerpt.into(),
            scope,
        }
    }
}

/// One concrete value an option carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct OfferValue {
    /// Its part in the workflow.
    pub role: ValueRole,
    /// The value, exactly.
    pub value: String,
    /// What the person reads for it, when it differs from the value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// For a model, what this machine's inventory says of it; absent when the inventory offers
    /// no such model (never invented).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choice: Option<ModelFacts>,
}

impl OfferValue {
    /// A value (INV-019).
    #[must_use]
    pub fn new(role: ValueRole, value: impl Into<String>, name: Option<String>) -> Self {
        Self {
            role,
            value: value.into(),
            name,
            choice: None,
        }
    }

    /// The same value with the inventory's facts about the model it names.
    #[must_use]
    pub fn with_choice(mut self, choice: Option<ModelFacts>) -> Self {
        self.choice = choice;
        self
    }
}

/// A list price in USD per million output tokens, compared by its total order so the offers
/// that carry it stay comparable.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UsdPerMillion(pub f64);

impl PartialEq for UsdPerMillion {
    fn eq(&self, other: &Self) -> bool {
        self.0.total_cmp(&other.0).is_eq()
    }
}

impl Eq for UsdPerMillion {}

/// A model choice as this machine's inventory states it: the role it would serve, the model, the
/// route that serves it (its id, class, whether it is ready, how it bills) and the catalogue's
/// output list price on a metered route — unknown or unmetered is `None`, never free.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModelFacts {
    /// The selection it serves: `run`, `author` or `decision`.
    pub role: String,
    /// The exact `provider/name`.
    pub model: String,
    /// The route's id (`run.access.via` names it).
    pub via: String,
    /// The route's class (`api` · `local` · `harness` …).
    pub class: String,
    /// Whether the route is ready here.
    pub configured: bool,
    /// How the route bills (`api_metered` · `included_quota` · `local` · `unknown` …).
    pub billing: String,
    /// The catalogue's output list price on a metered route.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_usd_per_million: Option<UsdPerMillion>,
}

impl ModelFacts {
    /// The facts of `model` for `role` over the route `via` (INV-019).
    #[must_use]
    pub fn new(
        role: impl Into<String>,
        model: impl Into<String>,
        (via, class, billing): (String, String, String),
        configured: bool,
        price: Option<f64>,
    ) -> Self {
        Self {
            role: role.into(),
            model: model.into(),
            via,
            class,
            configured,
            billing,
            output_usd_per_million: price.map(UsdPerMillion),
        }
    }
}

/// One option of a question.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Offer {
    /// Its key.
    pub key: String,
    /// What the person reads.
    pub label: String,
    /// Whether the author recommends it.
    pub recommended: bool,
    /// The concrete values accepting it binds.
    pub values: Vec<OfferValue>,
}

impl Offer {
    /// An option (INV-019).
    #[must_use]
    pub fn new(key: impl Into<String>, label: impl Into<String>, recommended: bool) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            recommended,
            values: Vec::new(),
        }
    }

    /// The values accepting it binds.
    #[must_use]
    pub fn with_values(mut self, values: Vec<OfferValue>) -> Self {
        self.values = values;
        self
    }
}

/// Whether a question can be answered now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AskedState {
    /// It waits for the person's answer.
    Open,
    /// It is asked after the questions it depends on are answered.
    After,
}

/// One question asked of the person, with the identity an answer names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct AskedQuestion {
    /// The identity: this question as this session asked it, at this revision of the request.
    #[serde(serialize_with = "witness")]
    pub id: QuestionId,
    /// The key of the value it asks for.
    pub key: String,
    /// The question, as the person reads it.
    pub question: String,
    /// Why it is asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// The part its answer plays, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<ValueRole>,
    /// Whether it can be answered now.
    pub state: AskedState,
    /// The questions it waits for, by key.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    /// Its options; none for a free answer.
    pub options: Vec<Offer>,
    /// Whether a free answer is accepted beside the options.
    pub free_text: bool,
    /// Whether several options may be chosen.
    pub multi_select: bool,
}

impl AskedQuestion {
    /// A question with no option, open (INV-019).
    #[must_use]
    pub fn new(id: QuestionId, key: impl Into<String>, question: impl Into<String>) -> Self {
        Self {
            id,
            key: key.into(),
            question: question.into(),
            why: None,
            role: None,
            state: AskedState::Open,
            after: Vec::new(),
            options: Vec::new(),
            free_text: true,
            multi_select: false,
        }
    }

    /// Why it is asked.
    #[must_use]
    pub fn with_why(mut self, why: impl Into<String>) -> Self {
        self.why = Some(why.into());
        self
    }

    /// The part its answer plays.
    #[must_use]
    pub fn with_role(mut self, role: ValueRole) -> Self {
        self.role = Some(role);
        self
    }

    /// Asked once the questions `after` are answered.
    #[must_use]
    pub fn after(mut self, after: Vec<String>) -> Self {
        self.state = if after.is_empty() {
            AskedState::Open
        } else {
            AskedState::After
        };
        self.after = after;
        self
    }

    /// Its options, and whether a free answer and several choices are accepted.
    #[must_use]
    pub fn with_options(
        mut self,
        options: Vec<Offer>,
        free_text: bool,
        multi_select: bool,
    ) -> Self {
        self.options = options;
        self.free_text = free_text;
        self.multi_select = multi_select;
        self
    }
}

/// A question's identity on the wire: its witness, never the session that asked it.
fn witness<S: serde::Serializer>(id: &QuestionId, out: S) -> Result<S::Ok, S::Error> {
    out.serialize_str(id.as_str())
}

/// The identities of questions on the wire: their witnesses.
pub(super) fn witnesses<S: serde::Serializer>(
    ids: &[QuestionId],
    out: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq as _;
    let mut seq = out.serialize_seq(Some(ids.len()))?;
    for id in ids {
        seq.serialize_element(id.as_str())?;
    }
    seq.end()
}
