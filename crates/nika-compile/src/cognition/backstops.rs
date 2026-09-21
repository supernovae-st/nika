// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The backstops of the cold merge: a gate the reader saw finds its effect in the proposal,
//! a refund the reader guarded is carried or explained, a prohibition is read from its head.

use super::super::plan::{EffectPolicy, EffectVerb, Plan};
use super::ProposedRegion;

/// Words that open a prohibition in the languages the compiler meets.
const PROHIBITION_CUES: &[&str] = &[
    "do not ", "don't ", "never ", "ne ", "n'", "no ", "non ", "nicht ", "keine ", "sans ",
    "jamais ", "nunca ", "mai ", "niemals ",
];

/// A prohibition ("Do not copy …", "Ne cite pas …", "No copies …") at the head of an excerpt.
pub(crate) fn starts_with_prohibition(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    PROHIBITION_CUES.iter().any(|cue| lower.starts_with(cue))
}

/// The reader's refund backstop is a word-level guard ("refund" appears, no refund effect
/// recognized). Once a proposal exists, its own accounting decides: the unknown is withdrawn
/// when the merged plan carries a refund effect, or when every region that mentions a refund
/// was read as an operation, a constraint or context (a status value such as "refunded" in a
/// filter). A region read as an effect, a policy or unknown keeps the guard.
/// A final human gate the deterministic reader saw with no effect to guard (its effect heads
/// are in a language the lexicon does not read) finds its effect in the proposal: the unknown
/// lifts and, when no effect is gated yet, the last automatic effect becomes human-first, as
/// the reader itself does when it knows the verb. The gate stays unresolved when the proposal
/// names no effect at all.
pub(super) fn gate_finds_its_effect(plan: &mut Plan) {
    let gate = super::lexicon::GATE_WITHOUT_EFFECT;
    if plan.effects.is_empty() || !plan.unknowns.iter().any(|u| u == gate) {
        return;
    }
    plan.unknowns.retain(|u| u != gate);
    if plan
        .effects
        .iter()
        .all(|e| e.policy != EffectPolicy::HumanFirst)
        && let Some(last) = plan
            .effects
            .iter_mut()
            .rev()
            .find(|e| e.policy == EffectPolicy::Automatic)
    {
        last.policy = EffectPolicy::HumanFirst;
    }
}

pub(super) fn reconcile_refund_backstop(plan: &mut Plan, regions: &[ProposedRegion]) {
    const GUARD: &str = "The request mentions a refund that no recognized effect carries";
    if !plan.unknowns.iter().any(|u| u.starts_with(GUARD)) {
        return;
    }
    let mentions = |text: &str| {
        let lower = text.to_lowercase();
        lower.contains("refund") || lower.contains("rembours")
    };
    let carried = plan.effects.iter().any(|e| e.verb == EffectVerb::Refund);
    let mentioning: Vec<&ProposedRegion> = regions.iter().filter(|r| mentions(&r.text)).collect();
    let explained = !mentioning.is_empty()
        && mentioning.iter().all(|r| {
            matches!(
                r.role.as_str(),
                "operation" | "constraint" | "context" | "obligation"
            )
        });
    if carried || explained {
        plan.unknowns.retain(|u| !u.starts_with(GUARD));
    }
}
