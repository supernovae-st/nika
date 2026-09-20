// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candidate composer: a finite, deterministically ordered set of private plans
//! between the COLD proposals, the recalled candidates and the assembler, judged by a
//! deterministic feasibility filter before any seat sees them.
//!
//! One [`Candidate`] is one plan with its source, its structural signature and its
//! feasibility verdict. The set is the distinct admissible COLD plans (first-seen
//! order, capped at [`CAP`]) plus the pattern-informed variants the recalled
//! candidates suggest in a dimension the request leaves open — and ONLY when the
//! assembler can express that dimension. Today the assembler emits one linear chain
//! per invocation and the private plan carries no topology, so the fan-out and fan-in
//! dimensions the pattern words name are recorded as suggestions
//! (`expressible: false`) and yield no variant: a variant nobody can assemble is not
//! composed, and a source nothing produces is not declared.
//!
//! Feasibility is a hard filter against the deterministic reading (the floor) and the
//! request text. Every failure is a recorded reason; an infeasible candidate stays in
//! provenance and is never offered to a seat. Nothing here calls a provider, writes
//! source or grants authority: the assembler and Check judge whatever is selected.

use std::cmp::Reverse;

use super::lexicon::Reading;
use super::plan::{Binding, EffectPolicy, EffectVerb, Op, Plan};
use super::retrieve::Hit;
use serde_json::{Value, json};

/// The finite cap on composed candidates.
pub(super) const CAP: usize = 8;

/// Topologies the assembler expresses from a private plan today: one linear chain per
/// invocation. A pattern dimension outside this set is recorded, never composed.
const EXPRESSIBLE: &[&str] = &["sequential"];

/// A whole-word phrase of the request.
type Phrase = &'static [&'static str];
/// One pattern dimension: its word, the recalled pattern words that name it, and the
/// request phrases that already fix it (a variant there would contradict the request).
type DimensionSpec = (&'static str, &'static [&'static str], &'static [Phrase]);

/// The pattern dimensions the recalled candidates may suggest.
const DIMENSIONS: &[DimensionSpec] = &[
    (
        "fanout",
        &["fanout", "batch"],
        &[
            &["each"],
            &["every"],
            &["chaque"],
            &["chacun"],
            &["chacune"],
            &["parallel"],
            &["parallele"],
            &["batch"],
            &["one", "by", "one"],
            &["un", "par", "un"],
        ],
    ),
    (
        "fanin",
        &["fanin", "join"],
        &[
            &["combine"],
            &["merge"],
            &["fusionne"],
            &["regroupe"],
            &["aggregate"],
            &["agrege"],
            &["consolidate"],
            &["consolide"],
            &["join"],
        ],
    ),
];

/// Where one candidate came from. Pattern variants and ambiguity readings would be
/// further sources; neither exists today (see the module doc), so neither is declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CandidateSource {
    /// The admissible plan first proposed by this COLD sample.
    ColdSample(usize),
}

/// One composed candidate.
#[derive(Clone, Debug)]
pub(super) struct Candidate {
    pub plan: Plan,
    pub source: CandidateSource,
    /// Sorted operation words, `effect:verb:policy` words and obligation words.
    pub signature: Vec<String>,
    /// `Ok` when every hard filter holds; otherwise every reason, in filter order.
    pub feasibility: Result<(), Vec<String>>,
}

impl Candidate {
    pub(super) fn feasible(&self) -> bool {
        self.feasibility.is_ok()
    }
    /// The COLD sample this candidate came from.
    pub(super) fn sample(&self) -> usize {
        match self.source {
            CandidateSource::ColdSample(index) => index,
        }
    }
    /// The provenance projection: private, observational, never authority. The plan is
    /// recorded so an oracle can judge every candidate, not only the selected one.
    pub(super) fn to_json(&self, index: usize) -> Value {
        json!({
            "index": index,
            "source": match self.source {
                CandidateSource::ColdSample(sample) => json!({"kind": "cold_sample", "sample": sample}),
            },
            "signature": self.signature,
            "feasible": self.feasible(),
            "reasons": self.feasibility.as_ref().err().cloned().unwrap_or_default(),
            "plan": self.plan.to_json(),
        })
    }
}

/// A topology dimension the recalled candidates suggest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Dimension {
    pub name: &'static str,
    /// Recalled candidates whose pattern words name it.
    pub hits: Vec<String>,
    /// The request already fixes this dimension itself.
    pub fixed_by_request: bool,
    /// The assembler can express a variant in this dimension.
    pub expressible: bool,
}

impl Dimension {
    pub(super) fn to_json(&self) -> Value {
        json!({
            "dimension": self.name,
            "hits": self.hits,
            "fixed_by_request": self.fixed_by_request,
            "expressible": self.expressible,
        })
    }
}

/// What [`compose`] produced: the candidates and the dimension record.
pub(super) struct Composition {
    pub candidates: Vec<Candidate>,
    pub dimensions: Vec<Dimension>,
}

/// Compose the finite candidate set from the admissible COLD plans (`samples`, in
/// sample order), the deterministic reading (the floor) and the recalled candidates.
/// Two samples with the same signature and the same feasibility verdict are one
/// candidate; a feasible twin of an infeasible signature stays visible.
pub(super) fn compose(
    samples: &[(usize, Plan)],
    reading: &Reading,
    hits: &[Hit],
    intent: &str,
) -> Composition {
    let mut candidates: Vec<Candidate> = Vec::new();
    for (index, plan) in samples {
        if candidates.len() >= CAP {
            break;
        }
        let signature = signature(plan);
        let feasibility = feasibility(plan, &reading.plan, intent);
        if candidates
            .iter()
            .any(|c| c.signature == signature && c.feasibility.is_ok() == feasibility.is_ok())
        {
            continue;
        }
        candidates.push(Candidate {
            plan: plan.clone(),
            source: CandidateSource::ColdSample(*index),
            signature,
            feasibility,
        });
    }
    // A variant is composed only in a dimension the request leaves open AND the
    // assembler expresses. That set is empty today, so the dimensions are the record.
    Composition {
        candidates,
        dimensions: dimensions(hits, intent),
    }
}

fn dimensions(hits: &[Hit], intent: &str) -> Vec<Dimension> {
    let text = fold(intent);
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    DIMENSIONS
        .iter()
        .filter_map(|(name, patterns, fixers)| {
            let hits: Vec<String> = hits
                .iter()
                .filter(|hit| {
                    hit.patterns
                        .iter()
                        .any(|pattern| patterns.contains(&pattern.as_str()))
                })
                .map(|hit| hit.id.clone())
                .collect();
            if hits.is_empty() {
                return None;
            }
            Some(Dimension {
                name,
                hits,
                fixed_by_request: fixers
                    .iter()
                    .any(|phrase| words.windows(phrase.len()).any(|window| window == *phrase)),
                expressible: EXPRESSIBLE.contains(name),
            })
        })
        .collect()
}

/// Lowercase with French diacritics folded, for whole-word phrase matching.
fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            other => other,
        })
        .collect()
}

/// The deterministic hard filters, in a fixed order so reasons are stable:
///
/// 1. anchored — every operation, effect and obligation carries a nonempty verbatim
///    excerpt of the request;
/// 2. nonempty — at least one operation or effect;
/// 3. settled — no unresolved requested work;
/// 4. floor operations — every operation the reading recognized is accounted for by a
///    candidate operation or effect whose excerpt overlaps its clause, or by a
///    candidate operation of the same kind (a plan merges same-kind operations and
///    keeps one excerpt): the model may re-read a clause as another operation; it may
///    not drop the operation;
/// 5. floor effects and policy floor — every effect the reading recognized is present
///    with its policy, or with `human_first` where the reading said `automatic`
///    (strengthening only): a human gate is never removed, a prohibition, an
///    indecision or a contradiction is never resolved by a model, and a policy
///    literal the reading found is kept;
/// 6. money gate — an effect that moves money is never automatic;
/// 7. floor obligations — every obligation the reading recognized is present;
/// 8. literals carried — every literal the reading bound (URL, path, email, timezone)
///    is carried verbatim by some candidate operation or effect;
/// 9. no invented literal — every URL, path, email or number in a candidate detail or
///    target appears verbatim in the request.
pub(super) fn feasibility(candidate: &Plan, floor: &Plan, intent: &str) -> Result<(), Vec<String>> {
    let mut why: Vec<String> = Vec::new();
    if !candidate.anchored(intent) {
        why.push(
            "an operation, effect or obligation lacks an exact nonempty excerpt of the request"
                .to_owned(),
        );
    }
    if candidate.steps.is_empty() && candidate.effects.is_empty() {
        why.push("names no operation and no effect".to_owned());
    }
    for unknown in &candidate.unknowns {
        why.push(format!("unresolved requested work: {unknown}"));
    }
    floor_operations(candidate, floor, &mut why);
    floor_effects(candidate, floor, &mut why);
    for effect in &candidate.effects {
        if effect.verb.moves_money() && effect.policy == EffectPolicy::Automatic {
            why.push(format!(
                "`{}` moves money without a prior human approval",
                effect.verb.word()
            ));
        }
    }
    for obligation in &floor.obligations {
        if !candidate
            .obligations
            .iter()
            .any(|o| o.kind == obligation.kind)
        {
            why.push(format!(
                "dropped the obligation `{}` ({})",
                obligation.kind.word(),
                obligation.evidence.trim()
            ));
        }
    }
    literals(candidate, floor, intent, &mut why);
    why.dedup();
    if why.is_empty() { Ok(()) } else { Err(why) }
}

/// Rule 4: a recognized operation may be re-read, never dropped. The reader is reliable on
/// the operations its cue table names unambiguously; a validation, a computation or an
/// exploration it guessed from an instruction ("fais relire", "compare") is advisory and
/// never vetoes a proposal on its own.
fn floor_operations(candidate: &Plan, floor: &Plan, why: &mut Vec<String>) {
    for step in floor
        .steps
        .iter()
        .filter(|s| !matches!(s.op, Op::Validate | Op::Compute | Op::Explore))
    {
        let accounted = candidate
            .steps
            .iter()
            .any(|s| s.op == step.op || overlaps(&s.evidence, &step.evidence))
            || candidate
                .effects
                .iter()
                .any(|e| overlaps(&e.evidence, &step.evidence));
        if !accounted {
            why.push(format!(
                "dropped the recognized operation `{}` ({})",
                step.op.word(),
                step.evidence.trim()
            ));
        }
    }
}

/// Rule 5: every recognized effect stays, with its policy floor and its policy literal.
fn floor_effects(candidate: &Plan, floor: &Plan, why: &mut Vec<String>) {
    for effect in &floor.effects {
        let Some(found) = candidate.effects.iter().find(|e| e.verb == effect.verb) else {
            why.push(format!(
                "dropped the recognized effect `{}` ({})",
                effect.verb.word(),
                effect.evidence.trim()
            ));
            continue;
        };
        let strengthened =
            effect.policy == EffectPolicy::Automatic && found.policy == EffectPolicy::HumanFirst;
        if found.policy != effect.policy && !strengthened {
            if effect.policy == EffectPolicy::HumanFirst {
                why.push(format!(
                    "removed the human gate before `{}`",
                    effect.verb.word()
                ));
            } else {
                why.push(format!(
                    "changed the policy of `{}` from {} to {}",
                    effect.verb.word(),
                    effect.policy.word(),
                    found.policy.word()
                ));
            }
        }
        if let Some(literal) = &effect.policy_literal
            && found.policy_literal.as_ref() != Some(literal)
        {
            why.push(format!(
                "dropped the policy literal `{literal}` of `{}`",
                effect.verb.word()
            ));
        }
    }
}

/// Rules 8 and 9: every bound literal carried, no literal invented.
fn literals(candidate: &Plan, floor: &Plan, intent: &str, why: &mut Vec<String>) {
    for binding in &floor.bindings {
        if !carries(candidate, binding) {
            why.push(format!(
                "the {} `{}` is no longer carried by any operation or effect",
                binding.role, binding.literal
            ));
        }
    }
    let intent_runs = digit_runs(intent);
    for text in candidate
        .steps
        .iter()
        .map(|s| s.detail.as_str())
        .chain(candidate.effects.iter().map(|e| e.target.as_str()))
    {
        for token in literal_tokens(text) {
            let present = if token.bytes().all(|b| b.is_ascii_digit()) {
                intent_runs.contains(&token)
            } else {
                intent.contains(token.as_str())
            };
            if !present {
                why.push(format!("the literal `{token}` is not in the request"));
            }
        }
    }
}

fn overlaps(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    !a.is_empty() && !b.is_empty() && (a.contains(b) || b.contains(a))
}

/// A bound literal is carried when a candidate names it, or when the element that consumes
/// it in the assembler is present: a URL by a fetch, a path by a read or a write, an email
/// by an effect that addresses someone, a money policy by the effect that keeps the literal,
/// a timezone by any step. The reading seeds the bindings themselves, so listing a binding
/// is never enough on its own.
fn carries(plan: &Plan, binding: &Binding) -> bool {
    let literal = binding.literal.as_str();
    let named = plan
        .steps
        .iter()
        .any(|s| s.detail.contains(literal) || s.evidence.contains(literal))
        || plan
            .effects
            .iter()
            .any(|e| e.target.contains(literal) || e.evidence.contains(literal));
    let consumed = match binding.role {
        "url" => plan.has(Op::Fetch),
        "path" => plan.has(Op::Read) || plan.effects.iter().any(|e| e.verb == EffectVerb::Write),
        "email" => plan.effects.iter().any(|e| {
            matches!(
                e.verb,
                EffectVerb::Send | EffectVerb::Notify | EffectVerb::Create | EffectVerb::Update
            )
        }),
        "money_policy" => plan
            .effects
            .iter()
            .any(|e| e.policy_literal.as_deref() == Some(literal)),
        _ => !plan.steps.is_empty(),
    };
    named || consumed
}

/// The literal-looking tokens of a detail or target: whole URLs, paths and emails, and
/// each maximal digit run of a token that carries digits.
fn literal_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for word in text.split_whitespace() {
        let token = word.trim_end_matches(['.', ',', ';', ':', ')', ']', '"', '\'']);
        if token.is_empty() {
            continue;
        }
        let path = token.starts_with("./") || (token.starts_with('/') && token.contains('.'));
        let url = token.starts_with("http://") || token.starts_with("https://");
        let email = token.contains('@') && token.contains('.') && !token.starts_with('@');
        if url || path || email {
            out.push(token.to_owned());
        } else if token.bytes().any(|b| b.is_ascii_digit()) {
            out.extend(digit_runs(token));
        }
    }
    out
}

fn digit_runs(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A plan's structural signature: operation words, effect verb:policy words, obligation words.
pub(super) fn signature(plan: &Plan) -> Vec<String> {
    let mut items: Vec<String> = plan
        .steps
        .iter()
        .map(|s| format!("op:{}", s.op.word()))
        .chain(
            plan.effects
                .iter()
                .map(|e| format!("effect:{}:{}", e.verb.word(), e.policy.word())),
        )
        .chain(
            plan.obligations
                .iter()
                .map(|o| format!("obligation:{}", o.kind.word())),
        )
        .collect();
    items.sort();
    items.dedup();
    items
}

/// Symmetric-difference similarity: shared items minus items only one side carries.
fn similarity(a: &[String], b: &[String]) -> isize {
    let shared = a.iter().filter(|item| b.contains(item)).count();
    let shared = isize::try_from(shared).unwrap_or(isize::MAX / 4);
    let (la, lb) = (
        isize::try_from(a.len()).unwrap_or(isize::MAX / 4),
        isize::try_from(b.len()).unwrap_or(isize::MAX / 4),
    );
    2 * shared - la - lb
}

/// Where distinct signatures differ: operations, effects, obligations.
pub(super) fn classify_disagreement(distinct: &[Vec<String>]) -> Vec<String> {
    let mut kinds = Vec::new();
    for prefix in ["op:", "effect:", "obligation:"] {
        let sets: Vec<Vec<&String>> = distinct
            .iter()
            .map(|sig| sig.iter().filter(|s| s.starts_with(prefix)).collect())
            .collect();
        if sets.windows(2).any(|w| w[0] != w[1]) {
            kinds.push(prefix.trim_end_matches(':').to_owned());
        }
    }
    kinds
}

/// The documented deterministic scorer for several feasible candidates without a seat:
/// the candidate closest to every accepted sample under the symmetric-difference
/// similarity (a medoid weighted by sample support, a candidate's own sample excluded);
/// a tie keeps the earliest candidate. Returns an index into `candidates`.
pub(super) fn rank(
    candidates: &[Candidate],
    feasible: &[usize],
    samples: &[(usize, Plan)],
) -> Option<usize> {
    let signatures: Vec<(usize, Vec<String>)> = samples
        .iter()
        .map(|(index, plan)| (*index, signature(plan)))
        .collect();
    feasible
        .iter()
        .copied()
        .filter_map(|k| {
            let candidate = candidates.get(k)?;
            let score: isize = signatures
                .iter()
                .filter(|(index, _)| *index != candidate.sample())
                .map(|(_, sig)| similarity(&candidate.signature, sig))
                .sum();
            Some((score, Reverse(k)))
        })
        .max()
        .map(|(_, Reverse(k))| k)
}

/// The seat's description of one feasible candidate: its signature and what differs
/// from the other feasible candidates.
pub(super) fn describe(candidates: &[Candidate], feasible: &[usize], k: usize) -> String {
    let Some(candidate) = candidates.get(k) else {
        return String::new();
    };
    let others: Vec<&Candidate> = feasible
        .iter()
        .filter(|index| **index != k)
        .filter_map(|index| candidates.get(*index))
        .collect();
    let mut text = format!("plan with {}", candidate.signature.join(", "));
    if others.is_empty() {
        return text;
    }
    let only_here: Vec<&String> = candidate
        .signature
        .iter()
        .filter(|item| !others.iter().all(|o| o.signature.contains(item)))
        .collect();
    let mut lacking: Vec<&String> = others
        .iter()
        .flat_map(|o| o.signature.iter())
        .filter(|item| !candidate.signature.contains(item))
        .collect();
    lacking.sort();
    lacking.dedup();
    if !only_here.is_empty() {
        text.push_str("; only here: ");
        text.push_str(
            &only_here
                .iter()
                .map(|item| item.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    if !lacking.is_empty() {
        text.push_str("; lacking: ");
        text.push_str(
            &lacking
                .iter()
                .map(|item| item.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    text
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::super::plan::{Binding, Effect, EffectVerb, Obligation, ObligationKind, Op, Step};
    use super::super::retrieve::HitKind;
    use super::*;

    const INTENT: &str = "Fetch https://example.com/pricing, classify the page, then draft a note. Ask me before any refund of 100 EUR; never retry more than 3 times.";

    fn step(op: Op, detail: &str, evidence: &str) -> Step {
        Step {
            op,
            evidence: evidence.to_owned(),
            detail: detail.to_owned(),
            categories: Vec::new(),
        }
    }
    fn effect(verb: EffectVerb, target: &str, evidence: &str, policy: EffectPolicy) -> Effect {
        Effect {
            verb,
            target: target.to_owned(),
            evidence: evidence.to_owned(),
            policy,
            policy_literal: None,
        }
    }
    /// The deterministic reading's floor: a literal fetch, an explicit classify, a gated
    /// refund with its policy literal, a retry bound and the URL binding.
    fn floor() -> Plan {
        let mut refund = effect(
            EffectVerb::Refund,
            "any refund of 100 EUR",
            "Ask me before any refund of 100 EUR",
            EffectPolicy::HumanFirst,
        );
        refund.policy_literal = Some("100 EUR".to_owned());
        Plan {
            steps: vec![
                step(
                    Op::Fetch,
                    "https://example.com/pricing",
                    "Fetch https://example.com/pricing",
                ),
                step(Op::Classify, "the page", "classify the page"),
            ],
            effects: vec![refund],
            obligations: vec![Obligation {
                kind: ObligationKind::RetryBound(3),
                evidence: "never retry more than 3 times".to_owned(),
            }],
            bindings: vec![Binding {
                role: "url",
                literal: "https://example.com/pricing".to_owned(),
            }],
            ..Plan::default()
        }
    }
    /// A faithful candidate: the floor plus the draft the model read.
    fn base() -> Plan {
        let mut plan = floor();
        plan.steps
            .push(step(Op::Draft, "a note", "then draft a note"));
        plan
    }
    fn reasons(candidate: &Plan) -> Vec<String> {
        feasibility(candidate, &floor(), INTENT)
            .err()
            .unwrap_or_default()
    }

    #[test]
    fn the_base_and_a_strengthened_reading_are_feasible() {
        assert_eq!(feasibility(&base(), &floor(), INTENT), Ok(()));
        // The model may re-read a recognized clause as another operation, never drop it.
        let mut reread = base();
        reread.steps[1].op = Op::Extract;
        assert_eq!(feasibility(&reread, &floor(), INTENT), Ok(()));
        // An automatic effect may be strengthened to human-first, never the reverse.
        let mut automatic_floor = floor();
        automatic_floor.effects[0].policy = EffectPolicy::Automatic;
        automatic_floor.effects[0].policy_literal = None;
        let mut strengthened = base();
        strengthened.effects[0].policy_literal = None;
        assert_eq!(feasibility(&strengthened, &automatic_floor, INTENT), Ok(()));
    }

    #[test]
    fn removing_a_recognized_operation_is_infeasible() {
        let mut candidate = base();
        candidate.steps.remove(0);
        let why = reasons(&candidate);
        assert!(
            why.iter().any(|r| r
                == "dropped the recognized operation `fetch` (Fetch https://example.com/pricing)"),
            "{why:?}"
        );
        assert!(
            why.iter().any(|r| r
                == "the url `https://example.com/pricing` is no longer carried by any operation or effect"),
            "{why:?}"
        );
    }

    #[test]
    fn adding_an_unanchored_operation_is_infeasible() {
        let mut candidate = base();
        candidate
            .steps
            .push(step(Op::Compute, "a score", "compute a score"));
        let why = reasons(&candidate);
        assert_eq!(
            why,
            ["an operation, effect or obligation lacks an exact nonempty excerpt of the request"]
        );
    }

    #[test]
    fn inventing_an_effect_is_infeasible() {
        let mut candidate = base();
        candidate.effects.push(effect(
            EffectVerb::Send,
            "the note",
            "send the note",
            EffectPolicy::Automatic,
        ));
        let why = reasons(&candidate);
        assert!(
            why.iter()
                .any(|r| r.contains("lacks an exact nonempty excerpt")),
            "{why:?}"
        );
        // An anchored money-moving effect without a gate is never feasible either.
        let mut candidate = base();
        candidate.effects[0].policy = EffectPolicy::Automatic;
        let why = reasons(&candidate);
        assert!(
            why.iter()
                .any(|r| r == "removed the human gate before `refund`"),
            "{why:?}"
        );
        assert!(
            why.iter()
                .any(|r| r == "`refund` moves money without a prior human approval"),
            "{why:?}"
        );
    }

    #[test]
    fn dropping_or_resolving_a_recognized_effect_is_infeasible() {
        let mut candidate = base();
        candidate.effects.clear();
        assert!(
            reasons(&candidate).iter().any(|r| r
                == "dropped the recognized effect `refund` (Ask me before any refund of 100 EUR)"),
            "{:?}",
            reasons(&candidate)
        );
        let mut forbidden_floor = floor();
        forbidden_floor.effects[0].policy = EffectPolicy::Forbidden;
        forbidden_floor.effects[0].policy_literal = None;
        for resolved in [
            EffectPolicy::Automatic,
            EffectPolicy::HumanFirst,
            EffectPolicy::Undecided,
        ] {
            let mut candidate = base();
            candidate.effects[0].policy = resolved;
            candidate.effects[0].policy_literal = None;
            let why = feasibility(&candidate, &forbidden_floor, INTENT).unwrap_err();
            assert!(
                why.iter()
                    .any(|r| r.starts_with("changed the policy of `refund` from forbidden to")),
                "{why:?}"
            );
        }
        let mut candidate = base();
        candidate.effects[0].policy_literal = None;
        assert_eq!(
            reasons(&candidate),
            ["dropped the policy literal `100 EUR` of `refund`"]
        );
    }

    #[test]
    fn dropping_an_obligation_is_infeasible() {
        let mut candidate = base();
        candidate.obligations.clear();
        assert_eq!(
            reasons(&candidate),
            ["dropped the obligation `retry_bound` (never retry more than 3 times)"]
        );
        let mut candidate = base();
        candidate.obligations[0].kind = ObligationKind::RetryBound(5);
        assert_eq!(reasons(&candidate).len(), 1);
    }

    #[test]
    fn altering_a_literal_is_infeasible() {
        let mut candidate = base();
        candidate.steps[0].detail = "https://example.com/price".to_owned();
        let why = reasons(&candidate);
        // The evidence still carries the URL: only the rewritten detail is judged.
        assert_eq!(
            why,
            ["the literal `https://example.com/price` is not in the request"]
        );
        let mut candidate = base();
        candidate.steps[0].detail = "https://example.com/pricing/".to_owned();
        assert_eq!(
            reasons(&candidate),
            ["the literal `https://example.com/pricing/` is not in the request"]
        );
        let mut candidate = base();
        candidate.effects[0].target = "any refund of 1000 EUR".to_owned();
        assert_eq!(
            reasons(&candidate),
            ["the literal `1000` is not in the request"]
        );
        let mut candidate = base();
        candidate.steps[2].detail = "a note in 3 lines".to_owned();
        assert_eq!(reasons(&candidate), Vec::<String>::new());
    }

    #[test]
    fn unsettled_and_empty_candidates_are_infeasible() {
        let mut candidate = base();
        candidate.unknowns.push("a bypassed approval".to_owned());
        assert_eq!(
            reasons(&candidate),
            ["unresolved requested work: a bypassed approval"]
        );
        let empty = Plan::default();
        let why = feasibility(&empty, &Plan::default(), INTENT).unwrap_err();
        assert_eq!(why, ["names no operation and no effect"]);
    }

    fn reading(plan: Plan) -> Reading {
        Reading {
            plan,
            ..Reading::default()
        }
    }
    fn hit(id: &str, patterns: &[&str]) -> Hit {
        Hit {
            id: id.to_owned(),
            kind: HitKind::Family,
            title: id.to_owned(),
            patterns: patterns.iter().map(|p| (*p).to_owned()).collect(),
            score: 1.0,
        }
    }

    #[test]
    fn compose_is_finite_deterministic_and_keeps_an_infeasible_twin_visible() {
        let mut dropped = base();
        dropped.steps.remove(0);
        let mut reread = base();
        reread.steps[1].op = Op::Extract;
        let samples: Vec<(usize, Plan)> = vec![
            (0, base()),
            (1, dropped.clone()),
            (2, base()),
            (3, reread.clone()),
            (4, dropped),
        ];
        let composition = compose(&samples, &reading(floor()), &[], INTENT);
        let sources: Vec<usize> = composition
            .candidates
            .iter()
            .map(Candidate::sample)
            .collect();
        assert_eq!(sources, [0, 1, 3]);
        let feasible: Vec<bool> = composition
            .candidates
            .iter()
            .map(Candidate::feasible)
            .collect();
        assert_eq!(feasible, [true, false, true]);
        assert!(composition.dimensions.is_empty());
        // The cap holds even when every sample is distinct.
        let many: Vec<(usize, Plan)> = (0..12)
            .map(|i| {
                let mut plan = base();
                if i > 0 {
                    plan.obligations.push(Obligation {
                        kind: ObligationKind::RetryBound(u32::try_from(i).unwrap()),
                        evidence: "never retry more than 3 times".to_owned(),
                    });
                    plan.steps[2].detail = format!("note {i}");
                }
                (i, plan)
            })
            .collect();
        assert!(
            compose(&many, &reading(Plan::default()), &[], INTENT)
                .candidates
                .len()
                <= CAP
        );
        let json = composition.candidates[1].to_json(1);
        assert_eq!(json["index"], 1);
        assert_eq!(json["source"], json!({"kind": "cold_sample", "sample": 1}));
        assert_eq!(json["feasible"], false);
        assert!(!json["reasons"].as_array().unwrap().is_empty());
        assert!(json["plan"]["operations"].is_array());
    }

    #[test]
    fn rank_prefers_the_support_weighted_medoid_and_breaks_ties_first() {
        let mut reread = base();
        reread.steps[1].op = Op::Extract;
        let samples: Vec<(usize, Plan)> = vec![(0, reread), (1, base()), (2, base())];
        let composition = compose(&samples, &reading(floor()), &[], INTENT);
        let feasible: Vec<usize> = (0..composition.candidates.len()).collect();
        assert_eq!(rank(&composition.candidates, &feasible, &samples), Some(1));
        // Equal support: the earliest candidate.
        let composition = compose(&samples[..2], &reading(floor()), &[], INTENT);
        let feasible: Vec<usize> = (0..composition.candidates.len()).collect();
        assert_eq!(
            rank(&composition.candidates, &feasible, &samples[..2]),
            Some(0)
        );
        assert_eq!(rank(&composition.candidates, &[], &samples), None);
        let text = describe(&composition.candidates, &feasible, 0);
        assert!(text.starts_with("plan with "), "{text}");
        assert!(text.contains("only here: op:extract"), "{text}");
        assert!(text.contains("lacking: op:classify"), "{text}");
        assert_eq!(
            classify_disagreement(&[
                composition.candidates[0].signature.clone(),
                composition.candidates[1].signature.clone()
            ]),
            ["op"]
        );
    }

    #[test]
    fn pattern_dimensions_are_recorded_never_composed() {
        let hits = [
            hit("A01", &["fetch", "summarize"]),
            hit("D02", &["extract", "fanout", "fanin"]),
            hit("I03", &["batch", "classify"]),
        ];
        let open = compose(&[(0, base())], &reading(floor()), &hits, INTENT);
        assert_eq!(open.candidates.len(), 1);
        assert_eq!(
            open.dimensions,
            [
                Dimension {
                    name: "fanout",
                    hits: vec!["D02".to_owned(), "I03".to_owned()],
                    fixed_by_request: false,
                    expressible: false,
                },
                Dimension {
                    name: "fanin",
                    hits: vec!["D02".to_owned()],
                    fixed_by_request: false,
                    expressible: false,
                },
            ]
        );
        let fixed = compose(
            &[(0, base())],
            &reading(floor()),
            &hits,
            "Pour chaque demande, agrège les résultats.",
        );
        assert!(fixed.dimensions.iter().all(|d| d.fixed_by_request));
        assert_eq!(
            open.dimensions[0].to_json(),
            json!({"dimension": "fanout", "hits": ["D02", "I03"], "fixed_by_request": false, "expressible": false})
        );
    }
}
