// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The private semantic plan: what the human asked, before any structure exists.
//!
//! A plan is compiler-private metadata. It is not a workflow key, a fifth verb,
//! a public IR or an SDK noun. Deterministic reading, a bounded decision seat and
//! a generative proposal all produce THIS shape; one deterministic assembler
//! turns it into ordinary four-verb source, and the ordinary Check judges that.
//! Every element carries a verbatim excerpt of the intent as evidence: nothing
//! in a plan may be invented, and nothing recognized may be dropped.

use serde_json::{Value, json};

/// Closed operation vocabulary. Names are private; the assembler owns their structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Op {
    /// Consume the document supplied at invocation (never an external retrieval).
    Read,
    /// Retrieve one page by an explicit URL.
    Fetch,
    /// Retrieve existing records from an external source, directory, database or knowledge base.
    Lookup,
    /// Search a corpus of documents by a query.
    Search,
    /// Extract structured fields from free text.
    Extract,
    /// Categorize or route into named categories.
    Classify,
    /// Write, summarize, translate or draft text without sending it.
    Draft,
    /// A deterministic numeric or comparison rule written in code, not by a model.
    Compute,
    /// Validate or verify data against explicit criteria.
    Validate,
}

impl Op {
    pub(super) const ALL: [Self; 9] = [
        Self::Read,
        Self::Fetch,
        Self::Lookup,
        Self::Search,
        Self::Extract,
        Self::Classify,
        Self::Draft,
        Self::Compute,
        Self::Validate,
    ];
    pub(super) const fn word(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Fetch => "fetch",
            Self::Lookup => "lookup",
            Self::Search => "search",
            Self::Extract => "extract",
            Self::Classify => "classify",
            Self::Draft => "draft",
            Self::Compute => "compute",
            Self::Validate => "validate",
        }
    }
    pub(super) fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.word() == word)
    }
    /// One-line definition used both in decision seats and generative instructions.
    pub(super) const fn definition(self) -> &'static str {
        match self {
            Self::Read => {
                "read: consume the document or text supplied with each invocation; not an external retrieval"
            }
            Self::Fetch => "fetch: retrieve one web page by an explicit URL present in the request",
            Self::Lookup => {
                "lookup: retrieve existing records from an external source, database, directory, catalog, calendar, history, registry or knowledge base"
            }
            Self::Search => {
                "search: find relevant passages or files in a corpus of documents by a query"
            }
            Self::Extract => {
                "extract: pull structured fields or facts out of free text, a form, a PDF or a transcript"
            }
            Self::Classify => "classify: categorize or route into named categories",
            Self::Draft => {
                "draft: write, summarize, translate, propose in writing or draft text without sending it"
            }
            Self::Compute => {
                "compute: a numeric or comparison rule that must run as code, not as model judgement"
            }
            Self::Validate => "validate: verify data against explicit criteria",
        }
    }
}

/// One requested operation with its verbatim evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Step {
    pub op: Op,
    /// Exact substring of the intent that requested it.
    pub evidence: String,
    /// The object of the operation, verbatim (what to look up, extract, draft…).
    pub detail: String,
    /// Named categories when the intent names them verbatim (classify only).
    pub categories: Vec<String>,
}

/// How an external effect may reach the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EffectPolicy {
    /// Requested without a prior human requirement.
    Automatic,
    /// Requested only after a fresh explicit human approval of that exact proposal.
    HumanFirst,
    /// Explicitly prohibited; never emitted.
    Forbidden,
    /// The human has explicitly not decided whether to include it.
    Undecided,
    /// Requested and prohibited at once; must stay visible, never resolved by a model.
    Conflict,
}

impl EffectPolicy {
    pub(super) const fn word(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::HumanFirst => "human_first",
            Self::Forbidden => "forbidden",
            Self::Undecided => "unspecified",
            Self::Conflict => "conflict",
        }
    }
}

/// Closed effect verbs; `Other` keeps the verbatim target as its only identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EffectVerb {
    Create,
    Send,
    Publish,
    Update,
    Notify,
    Refund,
    Pay,
    Order,
    Merge,
    Delete,
    /// Write a local file at an explicit path named in the request.
    Write,
    Other,
}

impl EffectVerb {
    pub(super) const fn word(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Send => "send",
            Self::Publish => "publish",
            Self::Update => "update",
            Self::Notify => "notify",
            Self::Refund => "refund",
            Self::Pay => "pay",
            Self::Order => "order",
            Self::Merge => "merge",
            Self::Delete => "delete",
            Self::Write => "write",
            Self::Other => "effect",
        }
    }
    pub(super) fn parse(word: &str) -> Option<Self> {
        [
            Self::Create,
            Self::Send,
            Self::Publish,
            Self::Update,
            Self::Notify,
            Self::Refund,
            Self::Pay,
            Self::Order,
            Self::Merge,
            Self::Delete,
            Self::Write,
            Self::Other,
        ]
        .into_iter()
        .find(|verb| verb.word() == word)
    }
    /// Money-moving effects need explicit literal policy data before any gate.
    pub(super) const fn moves_money(self) -> bool {
        matches!(self, Self::Refund | Self::Pay | Self::Order)
    }
}

/// One external effect the intent names, with its policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Effect {
    pub verb: EffectVerb,
    /// The verbatim target phrase ("créer une fiche prospect dans le CRM").
    pub target: String,
    pub evidence: String,
    pub policy: EffectPolicy,
    /// Verbatim policy data found in the intent (caps, eligibility), if any.
    pub policy_literal: Option<String>,
}

/// Cross-cutting obligations the program must structurally honour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ObligationKind {
    /// No second effect for the same incoming identifier.
    Dedup,
    /// A numeric maximum of attempts, cycles or iterations.
    RetryBound(u32),
    /// Recheck the current version immediately before the final effect.
    RevisionCheck,
}

impl ObligationKind {
    pub(super) const fn word(&self) -> &'static str {
        match self {
            Self::Dedup => "dedup",
            Self::RetryBound(_) => "retry_bound",
            Self::RevisionCheck => "revision_check",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Obligation {
    pub kind: ObligationKind,
    pub evidence: String,
}

/// A literal copied from the intent, never reproduced by a model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Binding {
    pub role: &'static str,
    pub literal: String,
}

/// The whole private plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Plan {
    pub steps: Vec<Step>,
    pub effects: Vec<Effect>,
    pub obligations: Vec<Obligation>,
    pub bindings: Vec<Binding>,
    /// Verbatim instructions that shape prompts but are not operations.
    pub constraints: Vec<String>,
    /// Requested work the compiler cannot construct; never dropped silently.
    pub unknowns: Vec<String>,
    /// Trigger or cadence context the program does not implement (one invocation per item).
    pub trigger: Option<String>,
}

impl Plan {
    pub(super) fn has(&self, op: Op) -> bool {
        self.steps.iter().any(|s| s.op == op)
    }
    pub(super) fn step(&self, op: Op) -> Option<&Step> {
        self.steps.iter().find(|s| s.op == op)
    }
    pub(super) fn retry_bound(&self) -> Option<u32> {
        self.obligations.iter().find_map(|o| match o.kind {
            ObligationKind::RetryBound(n) => Some(n),
            _ => None,
        })
    }
    pub(super) fn obligation(&self, word: &str) -> bool {
        self.obligations.iter().any(|o| o.kind.word() == word)
    }
    /// Merge a step; the same operation twice keeps the first evidence and joins details.
    pub(super) fn push_step(&mut self, step: Step) {
        if let Some(existing) = self.steps.iter_mut().find(|s| s.op == step.op) {
            if !step.detail.is_empty() && !existing.detail.contains(&step.detail) {
                if !existing.detail.is_empty() {
                    existing.detail.push_str(" ; ");
                }
                existing.detail.push_str(&step.detail);
            }
            for category in step.categories {
                if !existing.categories.contains(&category) {
                    existing.categories.push(category);
                }
            }
        } else {
            self.steps.push(step);
        }
    }
    /// The provenance projection: private, observational, never authority.
    pub(super) fn to_json(&self) -> Value {
        json!({
            "operations": self.steps.iter().map(|s| json!({
                "op": s.op.word(), "detail": s.detail, "evidence": s.evidence,
                "categories": s.categories,
            })).collect::<Vec<_>>(),
            "effects": self.effects.iter().map(|e| json!({
                "verb": e.verb.word(), "target": e.target, "policy": e.policy.word(),
                "evidence": e.evidence, "policy_literal": e.policy_literal,
            })).collect::<Vec<_>>(),
            "obligations": self.obligations.iter().map(|o| json!({
                "kind": o.kind.word(),
                "value": match o.kind { ObligationKind::RetryBound(n) => Some(n), _ => None },
                "evidence": o.evidence,
            })).collect::<Vec<_>>(),
            "bindings": self.bindings.iter().map(|b| json!({"role": b.role, "literal": b.literal})).collect::<Vec<_>>(),
            "constraints": self.constraints,
            "unknowns": self.unknowns,
            "trigger": self.trigger,
        })
    }
    /// Every evidence excerpt must be a verbatim substring of the intent.
    pub(super) fn anchored(&self, intent: &str) -> bool {
        self.steps
            .iter()
            .all(|s| !s.evidence.trim().is_empty() && intent.contains(&s.evidence))
            && self
                .effects
                .iter()
                .all(|e| !e.evidence.trim().is_empty() && intent.contains(&e.evidence))
            && self
                .obligations
                .iter()
                .all(|o| !o.evidence.trim().is_empty() && intent.contains(&o.evidence))
    }
}
