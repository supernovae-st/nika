// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The obligation ledger: every demand the request states, typed, with the state the
//! compiler gives it. A duty is REALIZED only when a named element of the emitted
//! workflow carries it; a duty nobody carries stays UNRESOLVED and a candidate with an
//! unresolved duty is never READY. A model changes HOW a duty is realized, never whether
//! it is: the ledger is derived from the private plan (and, on the deterministic door,
//! from the reading's own unresolved and ambiguous clauses), exported beside the plan in
//! provenance, and read by the assembler's READY law.

use serde_json::{Value, json};

use super::lexicon::Reading;
use super::plan::{EffectPolicy, Op, Plan};
use super::shape;

/// What kind of demand a duty is (closed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DutyKind {
    /// Content produced from material: a draft, an extraction, a classification, a
    /// computation, a validation, an exploration.
    Transformation,
    /// A row selection stated as a rule (a computation whose typed rule keeps or drops rows).
    Filter,
    /// A change to the outside world: a write, a send, a publish, a payment…
    Effect,
    /// A human gate that must dominate an effect.
    Gate,
    /// A verbatim instruction shaping how content is produced (tone, exclusions, style).
    Format,
    /// A count or a bound the produced content must honour (N bullets, N lines max, once per item).
    Cardinality,
    /// An identity the structure must preserve (order, one heading per file named after it).
    Identity,
    /// A cross-cutting safeguard: no second action for one identifier, a retry bound, a
    /// revision recheck before the final action.
    Safeguard,
    /// Requested work the compiler cannot type: it is named, never dropped.
    Work,
    /// A cadence or an outside event the request wants to start on; stated beside the
    /// candidate as a requirement, never baked into the program bytes.
    Trigger,
    /// A sentence that describes the material (what a file holds); realized by the material
    /// itself, it binds no operation.
    Context,
    /// A bound on the shape of the workflow (nothing else, no other file, no language model,
    /// one request); realized by the emitted shape, unresolved when the shape breaks it.
    Structure,
}

impl DutyKind {
    pub(super) const fn word(self) -> &'static str {
        match self {
            Self::Transformation => "transformation",
            Self::Filter => "filter",
            Self::Effect => "effect",
            Self::Gate => "gate",
            Self::Format => "format",
            Self::Cardinality => "cardinality",
            Self::Identity => "identity",
            Self::Safeguard => "safeguard",
            Self::Work => "work",
            Self::Trigger => "trigger",
            Self::Context => "context",
            Self::Structure => "structure",
        }
    }
}

/// Where a duty stands (closed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DutyState {
    /// Stated, not yet carried by any element of the workflow.
    Unresolved,
    /// Carried by a named element of the emitted workflow.
    Realized,
    /// Only a human can settle it (an undecided effect, a value the request withholds).
    NeedsHuman,
    /// Requested and prohibited at once; visible, never resolved by a model.
    Contradicted,
    /// Stated work the compiler cannot build; named in a diagnostic, never dropped.
    Unsupported,
    /// A demand the compiler refuses to carry (a prohibited effect is refused by omission).
    Refused,
}

impl DutyState {
    pub(super) const fn word(self) -> &'static str {
        match self {
            Self::Unresolved => "unresolved",
            Self::Realized => "realized",
            Self::NeedsHuman => "needs_human",
            Self::Contradicted => "contradicted",
            Self::Unsupported => "unsupported",
            Self::Refused => "refused",
        }
    }
}

/// One demand of the request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Duty {
    pub kind: DutyKind,
    /// The verbatim excerpt (or the plan element's text) the duty was read from.
    pub evidence: String,
    pub state: DutyState,
    /// The task, binding or structural element that carries the duty, once realized.
    pub realized_by: Option<String>,
    /// Why the duty is in its state when the state is not the plain unresolved one.
    pub note: Option<String>,
}

impl Duty {
    fn new(kind: DutyKind, evidence: &str) -> Self {
        Self {
            kind,
            evidence: evidence.trim().to_owned(),
            state: DutyState::Unresolved,
            realized_by: None,
            note: None,
        }
    }
    fn with_state(mut self, state: DutyState, note: &str) -> Self {
        self.state = state;
        self.note = Some(note.to_owned());
        self
    }
    /// The duty is carried by a named element of the emitted workflow.
    pub(super) fn realize(&mut self, by: &str, note: Option<&str>) {
        self.state = DutyState::Realized;
        self.realized_by = Some(by.to_owned());
        self.note = note.map(str::to_owned);
    }
    pub(super) fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.word(),
            "evidence": self.evidence,
            "state": self.state.word(),
            "realized_by": self.realized_by,
            "note": self.note,
        })
    }
}

/// The duty a constraint states: a structure law, a context sentence (realized by the
/// material at once), an identity, a bound or a format instruction.
fn constraint_duty(constraint: &str) -> Duty {
    if !super::structure::laws(constraint).is_empty() {
        return Duty::new(DutyKind::Structure, constraint);
    }
    if super::structure::context_statement(constraint) {
        let mut duty = Duty::new(DutyKind::Context, constraint);
        duty.realize(
            "the material",
            Some("describes what the material holds; shapes prompts, binds no operation"),
        );
        return duty;
    }
    let kind = if shape::structural(constraint) {
        DutyKind::Identity
    } else if bounded(constraint) {
        DutyKind::Cardinality
    } else {
        DutyKind::Format
    };
    Duty::new(kind, constraint)
}

/// The ledger of one request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Ledger {
    pub duties: Vec<Duty>,
}

impl Ledger {
    /// Every duty the plan states, in plan order: transformations (and the filter a typed
    /// rule states), effects with their gates, formats, cardinalities and identities from
    /// the constraints, safeguards from the obligations, a distributive trigger as a
    /// cardinality, and every unknown as untyped work. Nothing is invented and nothing
    /// recognized is left out.
    pub(super) fn extract(plan: &Plan) -> Self {
        let mut duties = Vec::new();
        for step in &plan.steps {
            if !step.op.carries_constraints() {
                continue;
            }
            let evidence = step_evidence(step);
            duties.push(Duty::new(DutyKind::Transformation, evidence));
            if step.op == Op::Compute && plan.rules.iter().any(super::rules::Rule::filters) {
                duties.push(Duty::new(DutyKind::Filter, evidence));
            }
        }
        for effect in &plan.effects {
            let duty = Duty::new(DutyKind::Effect, &effect.evidence);
            match effect.policy {
                EffectPolicy::HumanFirst => {
                    duties.push(duty);
                    duties.push(Duty::new(DutyKind::Gate, &effect.evidence));
                }
                EffectPolicy::Forbidden => duties.push(duty.with_state(
                    DutyState::Refused,
                    "prohibited by the request; never emitted",
                )),
                EffectPolicy::Undecided => duties.push(duty.with_state(
                    DutyState::NeedsHuman,
                    "the request leaves this effect undecided",
                )),
                EffectPolicy::Conflict => duties.push(
                    duty.with_state(DutyState::Contradicted, "requested and prohibited at once"),
                ),
                _ => duties.push(duty),
            }
        }
        for constraint in &plan.constraints {
            duties.push(constraint_duty(constraint));
        }
        // Two bounds on one unit that cannot both hold: both duties are contradicted, and the
        // request is refused rather than run on a prompt that obeys one of them.
        if let Some((i, j)) = super::cardinality::contradiction(&plan.constraints) {
            let (a, b) = (plan.constraints[i].trim(), plan.constraints[j].trim());
            for duty in duties
                .iter_mut()
                .filter(|d| d.kind == DutyKind::Cardinality && (d.evidence == a || d.evidence == b))
            {
                let other = if duty.evidence == a { b } else { a };
                duty.state = DutyState::Contradicted;
                duty.note = Some(format!("cannot hold together with `{other}`"));
            }
        }
        for obligation in &plan.obligations {
            duties.push(Duty::new(DutyKind::Safeguard, &obligation.evidence));
        }
        if let Some(trigger) = plan.trigger.as_deref() {
            match super::trigger::classify(trigger) {
                super::trigger::TriggerForm::Distributive if shape::led_by_quantifier(trigger) => {
                    duties.push(Duty::new(DutyKind::Cardinality, trigger));
                }
                super::trigger::TriggerForm::Schedule | super::trigger::TriggerForm::Event => {
                    duties.push(Duty::new(DutyKind::Trigger, trigger));
                }
                super::trigger::TriggerForm::Distributive
                | super::trigger::TriggerForm::Sequence => {}
            }
        }
        for unknown in &plan.unknowns {
            duties.push(Duty::new(DutyKind::Work, unknown).with_state(
                DutyState::Unsupported,
                "requested work outside the compiler's vocabulary",
            ));
        }
        Self { duties }
    }

    /// The deterministic door's ledger: the plan's duties plus every clause the reader
    /// could not settle (unresolved work, an ambiguous head, prose it cannot parse).
    pub(super) fn extract_reading(reading: &Reading) -> Self {
        let mut ledger = Self::extract(&reading.plan);
        for clause in &reading.unresolved {
            ledger.duties.push(Duty::new(DutyKind::Work, clause));
        }
        for ambiguity in &reading.ambiguous {
            ledger
                .duties
                .push(Duty::new(DutyKind::Work, &ambiguity.clause).with_state(
                DutyState::NeedsHuman,
                "a small set of operations fits this clause; a bounded seat or a human settles it",
            ));
        }
        for prose in &reading.soft_constraints {
            // The declarative reader files a clause both as a constraint and as prose; one duty.
            if reading
                .plan
                .constraints
                .iter()
                .any(|c| c.trim() == prose.trim())
            {
                continue;
            }
            ledger.duties.push(constraint_duty(prose));
        }
        ledger
    }

    /// The duties nothing carries yet: a READY candidate has none.
    pub(super) fn silent(&self) -> impl Iterator<Item = &Duty> {
        self.duties
            .iter()
            .filter(|d| d.state == DutyState::Unresolved)
    }

    /// The duties the request itself makes impossible to honour together.
    pub(super) fn contradicted(&self) -> impl Iterator<Item = &Duty> {
        self.duties
            .iter()
            .filter(|d| d.state == DutyState::Contradicted)
    }

    /// The provenance projection: observational, never authority.
    pub(super) fn to_json(&self) -> Value {
        json!(self.duties.iter().map(Duty::to_json).collect::<Vec<_>>())
    }
}

/// The excerpt a step's duty is read from: its evidence, or its detail when a recorded
/// step carries no excerpt.
pub(super) fn step_evidence(step: &super::plan::Step) -> &str {
    if step.evidence.trim().is_empty() {
        step.detail.as_str()
    } else {
        step.evidence.as_str()
    }
}

/// Whether a constraint states a count or a bound on produced content: a number (a digit
/// run or a number word) beside a size unit ("3 bullets", "12 lignes max", "under 150
/// words"). A concurrency bound is a structure the assembler consumes, not a cardinality
/// of the content, and is left to the format duties.
fn bounded(constraint: &str) -> bool {
    super::cardinality::bound(constraint).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Effect, EffectVerb, Obligation, ObligationKind, Step};

    fn step(op: Op, evidence: &str) -> Step {
        Step::new(op, evidence, evidence, Vec::new())
    }

    fn effect(verb: EffectVerb, evidence: &str, policy: EffectPolicy) -> Effect {
        Effect::new(verb, evidence, evidence, policy)
    }

    fn kinds(ledger: &Ledger) -> Vec<(DutyKind, DutyState)> {
        ledger.duties.iter().map(|d| (d.kind, d.state)).collect()
    }

    #[test]
    fn a_plan_states_its_duties_in_order_and_a_read_states_none() {
        let mut plan = Plan::default();
        plan.steps = vec![
            step(Op::Read, "Read ./notes/brief.md"),
            step(Op::Draft, "write a 3-bullet summary"),
        ];
        plan.effects = vec![effect(
            EffectVerb::Write,
            "write a 3-bullet summary to ./out/summary.md only after my approval",
            EffectPolicy::HumanFirst,
        )];
        plan.constraints = vec![
            "3 bullets".to_owned(),
            "in a warm tone".to_owned(),
            "with one heading per file named after the file".to_owned(),
        ];
        plan.obligations = vec![Obligation::new(ObligationKind::Dedup, "never twice")];
        let ledger = Ledger::extract(&plan);
        assert_eq!(
            kinds(&ledger),
            [
                (DutyKind::Transformation, DutyState::Unresolved),
                (DutyKind::Effect, DutyState::Unresolved),
                (DutyKind::Gate, DutyState::Unresolved),
                (DutyKind::Cardinality, DutyState::Unresolved),
                (DutyKind::Format, DutyState::Unresolved),
                (DutyKind::Identity, DutyState::Unresolved),
                (DutyKind::Safeguard, DutyState::Unresolved),
            ]
        );
        assert_eq!(ledger.silent().count(), 7);
        assert_eq!(ledger.duties[0].evidence, "write a 3-bullet summary");
        assert_eq!(ledger.duties[2].evidence, plan.effects[0].evidence);
        // Metamorphic: one more constraint is exactly one more duty; a reordered plan keeps
        // the same multiset of duties.
        let mut more = plan.clone();
        more.constraints.push("no more than 12 lines".to_owned());
        assert_eq!(Ledger::extract(&more).duties.len(), 8);
        assert_eq!(
            Ledger::extract(&more).duties[6].kind,
            DutyKind::Cardinality,
            "the constraints precede the safeguards"
        );
        let mut reordered = plan;
        reordered.steps.reverse();
        let mut a = kinds(&ledger);
        let mut b = kinds(&Ledger::extract(&reordered));
        a.sort_by_key(|(k, s)| (k.word(), s.word()));
        b.sort_by_key(|(k, s)| (k.word(), s.word()));
        assert_eq!(a, b);
    }

    #[test]
    fn policies_and_unknowns_set_their_states_never_a_model() {
        let mut plan = Plan::default();
        plan.effects = vec![
            effect(EffectVerb::Send, "never send it", EffectPolicy::Forbidden),
            effect(
                EffectVerb::Notify,
                "maybe notify ops",
                EffectPolicy::Undecided,
            ),
            effect(
                EffectVerb::Refund,
                "refund and never refund",
                EffectPolicy::Conflict,
            ),
            effect(EffectVerb::Write, "write ./x.md", EffectPolicy::Automatic),
        ];
        plan.unknowns = vec!["the URL from last time".to_owned()];
        plan.trigger = Some("for each critical row".to_owned());
        let ledger = Ledger::extract(&plan);
        assert_eq!(
            kinds(&ledger),
            [
                (DutyKind::Effect, DutyState::Refused),
                (DutyKind::Effect, DutyState::NeedsHuman),
                (DutyKind::Effect, DutyState::Contradicted),
                (DutyKind::Effect, DutyState::Unresolved),
                (DutyKind::Cardinality, DutyState::Unresolved),
                (DutyKind::Work, DutyState::Unsupported),
            ]
        );
        assert_eq!(ledger.silent().count(), 2);
        assert!(
            ledger.duties[0]
                .note
                .as_deref()
                .unwrap()
                .contains("prohibited")
        );
        // A sequencing trigger is not a cardinality.
        let mut sequenced = plan;
        sequenced.trigger = Some("once all three are done".to_owned());
        assert!(
            Ledger::extract(&sequenced)
                .duties
                .iter()
                .all(|d| d.kind != DutyKind::Cardinality)
        );
    }

    #[test]
    fn a_typed_row_rule_is_a_filter_duty_beside_its_transformation() {
        let rule = super::super::rules::synthesize(
            "keep only the rows whose status is refunded",
            &["status".to_owned()],
        )
        .expect("the closed grammar reads this rule");
        let mut plan = Plan::default();
        plan.steps = vec![
            step(Op::Read, "Read ./orders.csv"),
            step(Op::Compute, "keep only the rows whose status is refunded"),
        ];
        plan.rules = vec![rule];
        assert_eq!(
            kinds(&Ledger::extract(&plan)),
            [
                (DutyKind::Transformation, DutyState::Unresolved),
                (DutyKind::Filter, DutyState::Unresolved),
            ]
        );
        // Totals over every row are a transformation, not a filter.
        let totals = super::super::rules::synthesize(
            "the number of tickets and the sum of amount_cents",
            &["ticket".to_owned(), "amount_cents".to_owned()],
        );
        let mut plan = Plan::default();
        plan.steps = vec![step(
            Op::Compute,
            "the number of tickets and the sum of amount_cents",
        )];
        plan.rules = totals.into_iter().collect();
        assert!(
            Ledger::extract(&plan)
                .duties
                .iter()
                .all(|d| d.kind != DutyKind::Filter)
        );
    }

    #[test]
    fn the_readers_unsettled_clauses_are_untyped_work() {
        let mut reading = Reading::default();
        reading.unresolved.push("do the thing".to_owned());
        reading.soft_constraints.push("pas de blabla".to_owned());
        let ledger = Ledger::extract_reading(&reading);
        assert_eq!(
            kinds(&ledger),
            [
                (DutyKind::Work, DutyState::Unresolved),
                (DutyKind::Format, DutyState::Unresolved),
            ]
        );
        let json = ledger.to_json();
        assert_eq!(json[0]["kind"], "work");
        assert_eq!(json[0]["state"], "unresolved");
        assert_eq!(json[0]["evidence"], "do the thing");
        assert!(json[0]["realized_by"].is_null());
    }

    #[test]
    fn a_count_beside_a_size_unit_is_bounded_and_a_concurrency_bound_is_not() {
        for text in [
            "3 bullets",
            "12 lignes max",
            "a brief of under 150 words",
            "as five bullets",
            "résumé de chaque en 3 lignes max",
            "at most 2 pages",
        ] {
            assert!(bounded(text), "{text}");
        }
        for text in [
            "in a warm tone",
            "Process at most 2 products at a time",
            "the top 3 countries",
            "never infer amounts",
        ] {
            assert!(!bounded(text), "{text}");
        }
    }
}
