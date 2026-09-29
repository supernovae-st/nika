// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Join a stated effect to the deterministic reading without weakening its policy.

use super::{ProposedEffect, same_write};
use crate::plan::{Effect, EffectPolicy, EffectVerb, Plan};

pub(super) fn merge(
    plan: &mut Plan,
    effect: ProposedEffect,
    verb: EffectVerb,
    evidence: String,
    policy: EffectPolicy,
) {
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
        // How the write shapes its value is the model's to state: no reading states it.
        existing.alone |= effect.alone && verb == EffectVerb::Write;
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
        let mut made = Effect::new(verb, effect.target, evidence, policy);
        made.alone = effect.alone && verb == EffectVerb::Write;
        plan.effects.push(made);
    }
}
