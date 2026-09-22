// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The seat's proposal: its closed shape, its decoding, and the merge that joins it to the
//! deterministic reading under the anchoring, accounting and policy laws. The deterministic
//! facts win every disagreement; a model changes HOW a duty is realized, never whether.
//! Split from `cognition.rs` at the file-LOC cap (2026-09-22); the laws are unchanged.

use super::backstops::{gate_finds_its_effect, reconcile_refund_backstop, starts_with_prohibition};
use super::{backstop, plan_record, record_ledger};
use crate::plan::{Effect, EffectPolicy, EffectVerb, Obligation, ObligationKind, Op, Plan, Step};
use crate::{CompileOutcome, DiagnosticKind, QuestionType, lexicon::Reading};
use nika_kernel::ai::provider::{ContentBlock, InferResponse, StopReason};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Proposal {
    steps: Vec<ProposedStep>,
    effects: Vec<ProposedEffect>,
    obligations: Vec<ProposedObligation>,
    constraints: Vec<String>,
    unknowns: Vec<String>,
    #[serde(default)]
    regions: Vec<ProposedRegion>,
    #[serde(default)]
    approval_bypass: Option<ProposedBypass>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedStep {
    op: String,
    detail: String,
    evidence: String,
    #[serde(default, deserialize_with = "nullable_vec")]
    categories: Vec<String>,
    #[serde(default)]
    computation: Option<crate::predicate::ProposedComputation>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedEffect {
    verb: String,
    target: String,
    policy: String,
    evidence: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedObligation {
    kind: String,
    #[serde(default)]
    value: Option<u32>,
    evidence: String,
}
/// One contiguous region of the request and what it is for: the accounting the model owes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProposedRegion {
    pub(crate) text: String,
    pub(crate) role: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedBypass {
    present: bool,
    #[serde(default, deserialize_with = "nullable_string")]
    evidence: String,
}

/// A provider's strict structured-output mode may turn an optional property into an explicit
/// `null`; the decoder reads it as the absent default rather than refusing the plan.
pub(crate) fn nullable_vec<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(d)?.unwrap_or_default())
}

pub(crate) fn nullable_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

/// Typographic quotes read as their plain twins: a model that answers `“version”` for a
/// request that wrote `"version"` still names the same text.
fn fold_quote(ch: char) -> char {
    match ch {
        '“' | '”' | '„' | '«' | '»' => '"',
        '‘' | '’' | '‚' => '\'',
        other => other,
    }
}

/// The exact request excerpt a proposal's evidence names: the evidence itself when it is a
/// verbatim substring, else the request substring it matches once runs of whitespace are
/// folded on both sides (a model may wrap a line or drop a double space; it may not change
/// a word). None when nothing in the request matches.
pub(crate) fn exact_excerpt(intent: &str, evidence: &str) -> Option<String> {
    let evidence = evidence.trim();
    if evidence.is_empty() {
        return None;
    }
    if intent.contains(evidence) {
        return Some(evidence.to_owned());
    }
    let mut folded = String::new();
    let mut offsets: Vec<usize> = Vec::new();
    let mut pending_space = false;
    for (index, ch) in intent.char_indices() {
        if ch.is_whitespace() {
            pending_space = !folded.is_empty();
            continue;
        }
        if pending_space {
            folded.push(' ');
            offsets.push(index);
            pending_space = false;
        }
        let ch = fold_quote(ch);
        folded.push(ch);
        for _ in 0..ch.len_utf8() {
            offsets.push(index);
        }
    }
    // An excerpt that abbreviates a long clause with an ellipsis names the contiguous span
    // from its first fragment to its last; every fragment must occur, in order, verbatim.
    let fragments: Vec<String> = evidence
        .chars()
        .map(fold_quote)
        .collect::<String>()
        .replace('…', "...")
        .split("...")
        .map(|part| part.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|part| !part.is_empty())
        .collect();
    if fragments.is_empty() {
        return None;
    }
    let first_at = folded.find(fragments.first()?)?;
    let mut cursor = first_at + fragments.first()?.len();
    let mut last_end = cursor;
    for fragment in fragments.iter().skip(1) {
        let at = folded.get(cursor..)?.find(fragment.as_str())? + cursor;
        cursor = at + fragment.len();
        last_end = cursor;
    }
    let start = *offsets.get(first_at)?;
    let last = *offsets.get(last_end - 1)?;
    let end = last + intent.get(last..)?.chars().next()?.len_utf8();
    intent.get(start..end).map(str::to_owned)
}

/// The head of a rejected excerpt for the finding: enough to see what the model wrote,
/// never the whole text.
fn excerpt_head(text: &str) -> String {
    let trimmed = text.trim();
    let mut head: String = trimmed.chars().take(80).collect();
    if head.len() < trimmed.len() {
        head.push('…');
    }
    head
}

/// The model's own accounting, read back: a region it labelled as producing (operation,
/// effect, obligation, constraint, policy) must overlap an element the merged plan carries
/// (a step, an effect, an obligation, a constraint or a policy literal). A region consumed
/// without an element is a clause the proposal dropped, and a dropped clause is not
/// understood: it becomes an unknown, never a silent omission.
fn unproduced_regions(plan: &Plan, regions: &[ProposedRegion]) -> Vec<String> {
    let fold = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let overlaps = |a: &str, b: &str| {
        let (a, b) = (fold(a), fold(b));
        !a.is_empty() && !b.is_empty() && (a.contains(&b) || b.contains(&a))
    };
    let mut produced: Vec<String> = Vec::new();
    produced.extend(plan.steps.iter().map(|s| s.evidence.clone()));
    produced.extend(plan.effects.iter().map(|e| e.evidence.clone()));
    produced.extend(plan.obligations.iter().map(|o| o.evidence.clone()));
    produced.extend(plan.constraints.iter().cloned());
    produced.extend(plan.effects.iter().filter_map(|e| e.policy_literal.clone()));
    let mut gaps = Vec::new();
    for region in regions {
        let text = region.text.trim();
        let producing = matches!(
            region.role.as_str(),
            "operation" | "effect" | "obligation" | "constraint" | "policy"
        );
        if text.is_empty() || !producing {
            continue;
        }
        // A short region (a connector, a heading, a few words) never carries requested work.
        if text.split_whitespace().count() < 4 {
            continue;
        }
        if produced.iter().any(|p| overlaps(p, text)) {
            continue;
        }
        gaps.push(format!(
            "The proposal read `{text}` as {} but produced nothing for it; that part of the request is not understood.",
            region.role
        ));
    }
    gaps
}

pub(super) fn decode(response: &InferResponse, out: &mut CompileOutcome) -> Option<Proposal> {
    let text = match response.content.as_slice() {
        [ContentBlock::Text { text }]
            if text.len() <= 65_536 && response.stop_reason == StopReason::EndTurn =>
        {
            text
        }
        _ if response.stop_reason == StopReason::MaxTokens => {
            // A reasoning seat spends part of its output cap on its reasoning: 16 of 52
            // gpt-5-mini proposals stopped at exactly 4000 output tokens (eco-60, 2026-09-22).
            // The cap is the operator's knob; the finding names it instead of the shape.
            let spent = response
                .usage_reported
                .then_some(response.usage.output_tokens)
                .map_or_else(|| "its".to_owned(), |n| format!("{n} output tokens, its"));
            crate::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_provider",
                format!(
                    "The seat stopped at {spent} output cap before the plan was complete (a reasoning seat spends part of the cap on its reasoning). Raise --authoring-max-tokens (up to 8192) or seat a model that reasons less; nothing partial was assembled."
                ),
            );
            return None;
        }
        _ => {
            crate::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_plan",
                "Authoring must return one complete bounded JSON text, without tools or other content.",
            );
            return None;
        }
    };
    if let Ok(plan) = serde_json::from_str(text) {
        Some(plan)
    } else {
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "authoring_plan",
            "The authoring response is not a valid closed semantic plan. No source was emitted.",
        );
        None
    }
}

/// The proposal joins the deterministic reading; deterministic facts win every disagreement,
/// and the proposal must account for every region of the request.
#[allow(clippy::too_many_lines)] // one validation walk over steps, effects, obligations, regions
pub(super) fn merge(
    intent: &str,
    proposal: Proposal,
    reading: &Reading,
    out: &mut CompileOutcome,
) -> Option<Plan> {
    // The deterministic reading contributes its POLICY floor (effects with their policy,
    // obligations, constraints, bindings, unknowns), never its operation guesses: a clause the
    // reader consumed is not understanding, and the model must account for every region.
    let mut plan = reading.plan.clone();
    plan.steps = Vec::new();
    for step in proposal.steps {
        let Some(op) = Op::parse(&step.op) else {
            reject(out, "unknown operation in the proposal");
            return None;
        };
        let Some(evidence) = exact_excerpt(intent, &step.evidence) else {
            reject(
                out,
                &format!(
                    "an operation lacks an exact source excerpt (`{}` names `{}`)",
                    step.op,
                    excerpt_head(&step.evidence)
                ),
            );
            return None;
        };
        if op == Op::Compute && starts_with_prohibition(&evidence) {
            // "Do not copy more than 10 consecutive words" is a rule the prose obeys, not a
            // computation the workflow runs; it shapes prompts as a constraint.
            if !plan.constraints.contains(&evidence) {
                plan.constraints.push(evidence);
            }
            continue;
        }
        if op == Op::Compute
            && let Some(computation) = step.computation.as_ref().filter(|c| c.present)
            && let Some(rule) = crate::predicate::typed_rule(intent, &evidence, computation)
            && !plan.rules.iter().any(|r| r.text() == rule.text())
        {
            plan.rules.push(rule);
        }
        plan.push_step(Step::new(op, evidence, step.detail, step.categories));
    }
    for effect in proposal.effects {
        let (Some(verb), Some(policy)) = (
            EffectVerb::parse(&effect.verb),
            match effect.policy.as_str() {
                "automatic" => Some(EffectPolicy::Automatic),
                "human_first" => Some(EffectPolicy::HumanFirst),
                "forbidden" => Some(EffectPolicy::Forbidden),
                "unspecified" => Some(EffectPolicy::Undecided),
                "conflict" => Some(EffectPolicy::Conflict),
                _ => None,
            },
        ) else {
            reject(out, "unknown effect verb or policy in the proposal");
            return None;
        };
        let Some(evidence) = exact_excerpt(intent, &effect.evidence) else {
            reject(
                out,
                &format!(
                    "an effect lacks an exact source excerpt (`{}` names `{}`); no effect was invented",
                    effect.verb,
                    excerpt_head(&effect.evidence)
                ),
            );
            return None;
        };
        if let Some(existing) = plan
            .effects
            .iter_mut()
            .find(|e| e.verb == verb && same_write(verb, &e.target, &effect.target))
        {
            // The deterministic policy is the floor: a model may only strengthen a plain
            // request. Any other disagreement about a recognized effect is a human question.
            if !effect.target.trim().is_empty() {
                existing.target.clone_from(&effect.target);
                existing.evidence.clone_from(&evidence);
            }
            if existing.policy == EffectPolicy::Automatic && policy != EffectPolicy::Automatic {
                existing.policy = policy;
            } else if existing.policy != policy {
                plan.unknowns.push(format!(
                    "The proposal reads `{}` as {} while the request's explicit wording reads {}; the disagreement is not settled by a model.",
                    verb.word(),
                    policy.word(),
                    existing.policy.word()
                ));
            }
        } else {
            plan.effects
                .push(Effect::new(verb, effect.target, evidence, policy));
        }
    }
    gate_finds_its_effect(&mut plan);
    for obligation in proposal.obligations {
        let Some(evidence) = exact_excerpt(intent, &obligation.evidence) else {
            reject(
                out,
                &format!(
                    "an obligation lacks an exact source excerpt (`{}` names `{}`)",
                    obligation.kind,
                    excerpt_head(&obligation.evidence)
                ),
            );
            return None;
        };
        let kind = match (obligation.kind.as_str(), obligation.value) {
            ("dedup", _) => ObligationKind::Dedup,
            ("revision_check", _) => ObligationKind::RevisionCheck,
            ("retry_bound", Some(n)) if n > 0 => ObligationKind::RetryBound(n),
            _ => {
                reject(out, "an obligation is malformed");
                return None;
            }
        };
        if !plan
            .obligations
            .iter()
            .any(|o| o.kind.word() == kind.word())
        {
            plan.obligations.push(Obligation::new(kind, evidence));
        }
    }
    // A step proposed over the very words of a safeguard the request states (« dédoublonne
    // le callback par identifiant » as a classify, « vérifie de nouveau la version courante
    // … » as a validate) is that safeguard: its obligation carries the words, and a second
    // element over one clause would invent an operation. The doubled step is not assembled.
    let fold = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    // The reader states a safeguard over the clause it read (« dédoublonne … et vérifie de
    // nouveau … »): the step's words lie inside it. Only the look-alikes of a safeguard (a
    // classify or a computation for a dedup, a validate for a recheck) are dropped; any
    // other operation over those words stays visible.
    let safeguards: Vec<String> = plan
        .obligations
        .iter()
        .filter(|o| {
            matches!(
                o.kind,
                ObligationKind::Dedup | ObligationKind::RevisionCheck
            )
        })
        .map(|o| fold(&o.evidence))
        .collect();
    let mut doubled = Vec::new();
    plan.steps.retain(|step| {
        let words = fold(&step.evidence);
        let over_safeguard = matches!(step.op, Op::Classify | Op::Compute | Op::Validate)
            && !words.is_empty()
            && safeguards.iter().any(|s| s.contains(&words));
        if over_safeguard {
            doubled.push((step.op.word(), step.evidence.clone()));
        }
        !over_safeguard
    });
    for (op, evidence) in doubled {
        crate::finding(
            out,
            DiagnosticKind::Applied,
            "authoring_plan",
            format!(
                "`{evidence}` is the safeguard the request states, carried by its obligation; the proposal's `{op}` over the same words was not assembled."
            ),
        );
    }
    for constraint in proposal.constraints {
        if !plan.constraints.contains(&constraint) {
            plan.constraints.push(constraint);
        }
    }
    plan.unknowns.extend(proposal.unknowns);
    // Semantic accounting: the request must be covered by regions the model can name.
    if let Some(bypass) = proposal.approval_bypass
        && bypass.present
        && exact_excerpt(intent, &bypass.evidence).is_some()
    {
        plan.unknowns.push(format!(
            "The request presupposes, reuses or skips an approval it does not give ({}); the compiler never grants that authority.",
            bypass.evidence.trim()
        ));
    }
    for gap in accounting_gaps(intent, &proposal.regions) {
        plan.unknowns.push(gap);
    }
    for gap in unproduced_regions(&plan, &proposal.regions) {
        plan.unknowns.push(gap);
    }
    // A numeric rule the model demoted to guidance is an operation: promoted here so the
    // composer's signature and feasibility see the compute step.
    crate::shape::promote_stated_rules(&mut plan, intent);
    backstop(intent, &mut plan);
    reconcile_refund_backstop(&mut plan, &proposal.regions);
    plan.unknowns.dedup();
    if !plan.unknowns.is_empty() {
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "intent",
            "The semantic plan contains unresolved requested work; no substitute workflow was emitted.",
        );
        for unknown in &plan.unknowns {
            crate::finding(out, DiagnosticKind::Unknown, "intent", unknown.clone());
        }
        crate::question(
            out,
            "intent.clarification",
            "Supply a complete replacement request including all work still wanted. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
        record_ledger(out, &crate::ledger::Ledger::extract(&plan));
        out.provenance.plan = Some(plan_record(&plan, None));
        return None;
    }
    if plan.steps.is_empty() && plan.effects.is_empty() {
        reject(out, "the proposal names no operation and no effect");
        return None;
    }
    Some(plan)
}

/// Regions the proposal left unaccounted, or named as unknown. A model that cannot say what a
/// span of the request is for has not understood it; nothing is dropped silently.
fn accounting_gaps(intent: &str, regions: &[ProposedRegion]) -> Vec<String> {
    let mut gaps = Vec::new();
    if regions.is_empty() {
        // A seat that returned no regions is not penalized here; the anchored evidence of
        // steps and effects remains the floor.
        return gaps;
    }
    let mut covered = vec![false; intent.len()];
    for region in regions {
        let text = region.text.trim();
        if text.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(pos) = intent.get(from..).and_then(|rest| rest.find(text)) {
            let start = from + pos;
            let end = start + text.len();
            for flag in covered.iter_mut().take(end).skip(start) {
                *flag = true;
            }
            from = end;
        }
        if region.role == "unknown" {
            gaps.push(format!(
                "The proposal could not map this part of the request: {text}"
            ));
        }
    }
    // Any uncovered run of meaningful characters is an unaccounted region.
    let mut run = String::new();
    let mut runs = Vec::new();
    for (index, ch) in intent.char_indices() {
        let flagged = covered.get(index).copied().unwrap_or(true);
        if flagged {
            if run.trim().chars().filter(|c| c.is_alphanumeric()).count() >= 12 {
                runs.push(run.trim().to_owned());
            }
            run.clear();
        } else {
            run.push(ch);
        }
    }
    if run.trim().chars().filter(|c| c.is_alphanumeric()).count() >= 12 {
        runs.push(run.trim().to_owned());
    }
    for text in runs {
        gaps.push(format!(
            "The proposal does not account for this part of the request: {text}"
        ));
    }
    gaps
}

/// Two writes are one effect only when they name the same file; a write that names no
/// file joins the recognized one, a write to another file is its own effect.
fn same_write(verb: EffectVerb, existing: &str, proposed: &str) -> bool {
    verb != EffectVerb::Write
        || match (
            crate::paths::single_file(existing),
            crate::paths::single_file(proposed),
        ) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
}

pub(super) fn reject(out: &mut CompileOutcome, why: &str) {
    crate::finding(
        out,
        DiagnosticKind::Unknown,
        "authoring_plan",
        format!("The semantic plan was not assembled: {why}."),
    );
}
