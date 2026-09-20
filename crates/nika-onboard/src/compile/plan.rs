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
    /// An open-world region the request explicitly delegates to agents; bounded by turns.
    Explore,
}

impl Op {
    pub(super) const ALL: [Self; 10] = [
        Self::Read,
        Self::Fetch,
        Self::Lookup,
        Self::Search,
        Self::Extract,
        Self::Classify,
        Self::Draft,
        Self::Compute,
        Self::Validate,
        Self::Explore,
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
            Self::Explore => "explore",
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
            Self::Explore => {
                "explore: an open-ended region the request explicitly hands to agents, bounded by a number of turns"
            }
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
    pub(super) fn parse(word: &str) -> Option<Self> {
        [
            Self::Automatic,
            Self::HumanFirst,
            Self::Forbidden,
            Self::Undecided,
            Self::Conflict,
        ]
        .into_iter()
        .find(|policy| policy.word() == word)
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

/// The closed set of binding roles the reader and the composer emit. A recorded plan may
/// only name one of these: the assembler matches roles by identity.
const BINDING_ROLES: [&str; 5] = ["url", "email", "path", "timezone", "money_policy"];

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
    /// The faithful inverse of [`Self::to_json`]: a recorded plan (the `provenance.plan`
    /// value of an earlier outcome) becomes the same private plan, element for element.
    /// Any element outside the closed vocabulary, any missing field and any wrong type is
    /// an error naming the offending path; nothing is guessed or dropped. A `strategy`
    /// word riding the record is the caller's to read; it is not a plan element.
    ///
    /// # Errors
    /// The path and reason the record cannot be read back.
    pub(super) fn from_json(record: &Value) -> Result<Self, String> {
        let object = record
            .as_object()
            .ok_or_else(|| "the plan record is not an object".to_owned())?;
        let list = |key: &str| -> Result<&Vec<Value>, String> {
            object
                .get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| format!("`{key}` is missing or not an array"))
        };
        let mut plan = Self::default();
        for (k, item) in list("operations")?.iter().enumerate() {
            plan.steps
                .push(step_from(item, &format!("operations[{k}]"))?);
        }
        for (k, item) in list("effects")?.iter().enumerate() {
            plan.effects
                .push(effect_from(item, &format!("effects[{k}]"))?);
        }
        for (k, item) in list("obligations")?.iter().enumerate() {
            plan.obligations
                .push(obligation_from(item, &format!("obligations[{k}]"))?);
        }
        for (k, item) in list("bindings")?.iter().enumerate() {
            plan.bindings
                .push(binding_from(item, &format!("bindings[{k}]"))?);
        }
        for key in ["constraints", "unknowns"] {
            if object.get(key).is_none() {
                return Err(format!("`{key}` is missing"));
            }
        }
        plan.constraints = words(record, "plan", "constraints")?;
        plan.unknowns = words(record, "plan", "unknowns")?;
        plan.trigger = optional_text(record, "plan", "trigger")?;
        Ok(plan)
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

/// A required string field of one recorded element.
fn text(item: &Value, path: &str, key: &str) -> Result<String, String> {
    item.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("`{path}.{key}` is missing or not a string"))
}

/// An optional string field: absent or null reads as none, anything else must be a string.
fn optional_text(item: &Value, path: &str, key: &str) -> Result<Option<String>, String> {
    match item.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("`{path}.{key}` is not a string")),
    }
}

/// The nonempty verbatim excerpt every operation, effect and obligation carries.
fn excerpt(item: &Value, path: &str) -> Result<String, String> {
    let evidence = text(item, path, "evidence")?;
    if evidence.trim().is_empty() {
        return Err(format!("`{path}.evidence` is empty"));
    }
    Ok(evidence)
}

/// A list of strings; absent or null reads as empty.
fn words(item: &Value, path: &str, key: &str) -> Result<Vec<String>, String> {
    match item.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(k, v)| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("`{path}.{key}[{k}]` is not a string"))
            })
            .collect(),
        Some(_) => Err(format!("`{path}.{key}` is not an array")),
    }
}

fn step_from(item: &Value, path: &str) -> Result<Step, String> {
    let word = text(item, path, "op")?;
    let op = Op::parse(&word).ok_or_else(|| format!("`{path}.op` is unknown: {word}"))?;
    Ok(Step {
        op,
        evidence: excerpt(item, path)?,
        detail: text(item, path, "detail")?,
        categories: words(item, path, "categories")?,
    })
}

fn effect_from(item: &Value, path: &str) -> Result<Effect, String> {
    let word = text(item, path, "verb")?;
    let verb =
        EffectVerb::parse(&word).ok_or_else(|| format!("`{path}.verb` is unknown: {word}"))?;
    let word = text(item, path, "policy")?;
    let policy =
        EffectPolicy::parse(&word).ok_or_else(|| format!("`{path}.policy` is unknown: {word}"))?;
    Ok(Effect {
        verb,
        target: text(item, path, "target")?,
        evidence: excerpt(item, path)?,
        policy,
        policy_literal: optional_text(item, path, "policy_literal")?,
    })
}

fn obligation_from(item: &Value, path: &str) -> Result<Obligation, String> {
    let word = text(item, path, "kind")?;
    let value = item.get("value").and_then(Value::as_u64);
    let kind = match (word.as_str(), value) {
        ("dedup", _) => ObligationKind::Dedup,
        ("revision_check", _) => ObligationKind::RevisionCheck,
        ("retry_bound", Some(n)) if n > 0 => ObligationKind::RetryBound(
            u32::try_from(n)
                .map_err(|_| format!("`{path}.value` exceeds the retry bound range"))?,
        ),
        ("retry_bound", _) => return Err(format!("`{path}.value` must be a positive integer")),
        _ => return Err(format!("`{path}.kind` is unknown: {word}")),
    };
    Ok(Obligation {
        kind,
        evidence: excerpt(item, path)?,
    })
}

fn binding_from(item: &Value, path: &str) -> Result<Binding, String> {
    let word = text(item, path, "role")?;
    let role = BINDING_ROLES
        .into_iter()
        .find(|role| *role == word)
        .ok_or_else(|| format!("`{path}.role` is unknown: {word}"))?;
    Ok(Binding {
        role,
        literal: text(item, path, "literal")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_plan_round_trips_through_its_record() {
        let plan = Plan {
            steps: vec![
                Step {
                    op: Op::Classify,
                    evidence: "classify it as urgent or routine".to_owned(),
                    detail: "it".to_owned(),
                    categories: vec!["urgent".to_owned(), "routine".to_owned()],
                },
                Step {
                    op: Op::Read,
                    evidence: "Read ./a.md".to_owned(),
                    detail: "./a.md".to_owned(),
                    categories: Vec::new(),
                },
            ],
            effects: vec![Effect {
                verb: EffectVerb::Refund,
                target: "le remboursement".to_owned(),
                evidence: "avant le remboursement".to_owned(),
                policy: EffectPolicy::HumanFirst,
                policy_literal: Some("100 EUR max".to_owned()),
            }],
            obligations: vec![
                Obligation {
                    kind: ObligationKind::RetryBound(3),
                    evidence: "at most 3 attempts".to_owned(),
                },
                Obligation {
                    kind: ObligationKind::Dedup,
                    evidence: "never twice".to_owned(),
                },
                Obligation {
                    kind: ObligationKind::RevisionCheck,
                    evidence: "recheck".to_owned(),
                },
            ],
            bindings: vec![
                Binding {
                    role: "url",
                    literal: "https://example.invalid/x".to_owned(),
                },
                Binding {
                    role: "money_policy",
                    literal: "100 EUR max".to_owned(),
                },
            ],
            constraints: vec!["never infer".to_owned()],
            unknowns: vec!["something else".to_owned()],
            trigger: Some("every morning".to_owned()),
        };
        let record = plan.to_json();
        let back = Plan::from_json(&record).expect("round trip");
        assert_eq!(back, plan);
        assert_eq!(back.to_json(), record);
        let mut with_strategy = record.clone();
        with_strategy["strategy"] = json!("cold");
        assert_eq!(
            Plan::from_json(&with_strategy).expect("strategy rides"),
            plan
        );
    }

    #[test]
    fn a_defective_record_names_its_path() {
        let record = Plan::default().to_json();
        let mut bad = record.clone();
        bad["operations"] = json!([{"op":"read","detail":"x","evidence":""}]);
        assert_eq!(
            Plan::from_json(&bad).unwrap_err(),
            "`operations[0].evidence` is empty"
        );
        let mut bad = record.clone();
        bad["effects"] = json!([{"verb":"send","target":"t","policy":"later","evidence":"e"}]);
        assert_eq!(
            Plan::from_json(&bad).unwrap_err(),
            "`effects[0].policy` is unknown: later"
        );
        let mut bad = record.clone();
        bad["obligations"] = json!([{"kind":"retry_bound","value":0,"evidence":"e"}]);
        assert_eq!(
            Plan::from_json(&bad).unwrap_err(),
            "`obligations[0].value` must be a positive integer"
        );
        let mut bad = record.clone();
        bad["bindings"] = json!([{"role":"secret","literal":"x"}]);
        assert_eq!(
            Plan::from_json(&bad).unwrap_err(),
            "`bindings[0].role` is unknown: secret"
        );
        let mut bad = record;
        bad["trigger"] = json!(7);
        assert_eq!(
            Plan::from_json(&bad).unwrap_err(),
            "`plan.trigger` is not a string"
        );
        assert_eq!(
            Plan::from_json(&json!([])).unwrap_err(),
            "the plan record is not an object"
        );
        assert_eq!(
            Plan::from_json(&json!({"operations":[]})).unwrap_err(),
            "`effects` is missing or not an array"
        );
    }
}
