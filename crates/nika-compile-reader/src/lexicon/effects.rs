// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The effect words of a clause: the verbs a text names, the endpoint family a gate
//! phrase may name by its verb alone, and how a named obligation or effect joins the
//! plan (one effect per verb, two literal files are two writes, a gate naming the
//! action merges with the kindred effect that has the destination).

use super::super::objects;
use super::super::plan::{Effect, EffectPolicy, EffectVerb, Obligation, Plan};
use super::{Head, head_of};

/// The effect verbs a text names. A word the request lists as a column (`name, email e
/// city`) is a column, never the verb it spells.
#[must_use]
pub fn effect_words(lower: &str, columns: &[String]) -> Vec<EffectVerb> {
    let masked = objects::mask_columns(lower, columns);
    let lower = masked.as_str();
    let mut verbs = Vec::new();
    for word in lower.split(|c: char| !c.is_alphanumeric() && c != '\'' && c != ' ') {
        let mut seen = false;
        for candidate in word.split_whitespace() {
            if let Some((_, Head::Effect(verb))) = head_of(candidate) {
                if !verbs.contains(verb) {
                    verbs.push(*verb);
                }
                seen = true;
            }
        }
        if !seen && lower.contains("remboursement") && !verbs.contains(&EffectVerb::Refund) {
            verbs.push(EffectVerb::Refund);
        }
    }
    if (lower.contains("write")
        || lower.contains("écri")
        || lower.contains("enregistre")
        || lower.contains("save")
        || lower.contains("scriv")
        || lower.contains("salva")
        || lower.contains("escrib")
        || lower.contains("guarda"))
        && (lower.contains("disk")
            || lower.contains("disque")
            || lower.contains("disco")
            || lower.contains("file")
            || lower.contains("fichier")
            || lower.contains("archivo")
            || lower.contains("fichero")
            || lower.contains("./"))
        && !verbs.contains(&EffectVerb::Write)
    {
        verbs.push(EffectVerb::Write);
    }
    for (needle, verb) in [
        ("remboursement", EffectVerb::Refund),
        ("refund", EffectVerb::Refund),
        ("envoi", EffectVerb::Send),
        ("sending", EffectVerb::Send),
        ("writing", EffectVerb::Write),
        ("written", EffectVerb::Write),
        ("saving", EffectVerb::Write),
        ("publishing", EffectVerb::Publish),
        ("publication", EffectVerb::Publish),
        ("paiement", EffectVerb::Pay),
        ("payment", EffectVerb::Pay),
        ("commande", EffectVerb::Order),
        ("crédit", EffectVerb::Pay),
        ("credits", EffectVerb::Pay),
        ("rimborso", EffectVerb::Refund),
        ("reembolso", EffectVerb::Refund),
        ("invio", EffectVerb::Send),
        ("envío", EffectVerb::Send),
        ("pubblicazione", EffectVerb::Publish),
        ("publicación", EffectVerb::Publish),
        ("pagamento", EffectVerb::Pay),
        ("pago", EffectVerb::Pay),
        ("pedido", EffectVerb::Order),
    ] {
        if lower.contains(needle) && !verbs.contains(&verb) {
            verbs.push(verb);
        }
    }
    verbs
}

pub(super) fn push_obligation(plan: &mut Plan, obligation: Obligation) {
    if !plan
        .obligations
        .iter()
        .any(|o| o.kind.word() == obligation.kind.word())
    {
        plan.obligations.push(obligation);
    }
}

/// The endpoint family: send, publish and notify all reach a stated destination. A gate
/// phrase that names the action by a verb word alone ("ask me before sending", "don't
/// publish until I approve") gates the stated effect of the family ("post it to `<url>`"),
/// never a phantom effect of its own verb with no destination.
const ENDPOINT_FAMILY: [EffectVerb; 3] =
    [EffectVerb::Send, EffectVerb::Publish, EffectVerb::Notify];

pub(crate) fn kindred(a: EffectVerb, b: EffectVerb) -> bool {
    a == b || (ENDPOINT_FAMILY.contains(&a) && ENDPOINT_FAMILY.contains(&b))
}

/// An effect a gate phrase named by its verb word alone: gated, with no destination.
fn names_only_the_action(effect: &Effect) -> bool {
    effect.policy == EffectPolicy::HumanFirst && !objects::has_literal(&effect.target)
}

pub(super) fn push_effect(plan: &mut Plan, effect: Effect) {
    // Two writes to two literal files are two effects; anything else merges by verb, and a
    // gate naming the action by a verb word merges with the kindred effect that has the
    // destination.
    let other_file = |e: &Effect| {
        effect.verb == EffectVerb::Write
            && objects::has_literal(&effect.target)
            && objects::has_literal(&e.target)
            && e.target != effect.target
    };
    let gate_of_kin = |e: &Effect| {
        kindred(e.verb, effect.verb) && (names_only_the_action(e) || names_only_the_action(&effect))
    };
    if let Some(existing) = plan
        .effects
        .iter_mut()
        .find(|e| (e.verb == effect.verb && !other_file(e)) || gate_of_kin(e))
    {
        match (existing.policy, effect.policy) {
            (EffectPolicy::Automatic | EffectPolicy::HumanFirst, EffectPolicy::Forbidden)
            | (EffectPolicy::Forbidden, EffectPolicy::Automatic | EffectPolicy::HumanFirst) => {
                existing.policy = EffectPolicy::Conflict;
                existing.evidence = format!("{} / {}", existing.evidence, effect.evidence);
            }
            (EffectPolicy::Automatic, EffectPolicy::HumanFirst) => {
                existing.policy = EffectPolicy::HumanFirst;
            }
            (_, EffectPolicy::Undecided) | (EffectPolicy::Undecided, _) => {
                existing.policy = EffectPolicy::Undecided;
            }
            _ => {}
        }
        if existing.policy_literal.is_none() {
            existing.policy_literal = effect.policy_literal;
        }
        if !objects::has_literal(&existing.target) && objects::has_literal(&effect.target) {
            existing.target = effect.target;
            // The stated effect owns its verb and its clause; the gate phrase that came
            // first only set its policy.
            if existing.verb != effect.verb {
                existing.verb = effect.verb;
                existing.evidence = effect.evidence;
            }
        }
    } else {
        plan.effects.push(effect);
    }
}
