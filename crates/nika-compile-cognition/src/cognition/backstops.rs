// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The backstops of the cold merge: a gate the reader saw finds its effect in the proposal,
//! a refund the reader guarded is carried or explained, a prohibition is read from its head.

use super::super::paths::{self, PathShape};
use super::super::plan::{EffectPolicy, EffectVerb, Plan};
use super::ProposedRegion;

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

/// The reader's refund backstop is a word-level guard ("refund" appears, no refund effect
/// recognized). Once a proposal exists, its own accounting decides: the unknown is withdrawn
/// when the merged plan carries a refund effect, or when every region that mentions a refund
/// was read as an operation, a constraint or context (a status value such as "refunded" in a
/// filter). A region read as an effect, a policy or unknown keeps the guard, unless its only
/// refund word is a bound `./` file name (`Save ./out/refund-tickets.json`). Absolute paths
/// stay words; if no mention remains, this reconciler keeps the pre-existing guard.
pub(super) fn reconcile_refund_backstop(plan: &mut Plan, regions: &[ProposedRegion]) {
    const GUARD: &str = "The request mentions a refund that no recognized effect carries";
    if !plan.unknowns.iter().any(|u| u.starts_with(GUARD)) {
        return;
    }
    let files = bound_files(plan);
    let mentions = |text: &str| {
        let lower = (files.iter()).fold(text.to_lowercase(), |lower, file| {
            lower.replace(file.as_str(), " ")
        });
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

/// Bound `./` files and globs, lowercased. An absolute path may name an API endpoint;
/// its lexical binding alone does not establish a local file role.
fn bound_files(plan: &Plan) -> Vec<String> {
    (plan.bindings.iter())
        .filter(|b| b.role == "path" && b.literal.starts_with("./"))
        .filter(|b| {
            matches!(
                paths::token(&b.literal),
                Some(PathShape::File(_) | PathShape::Glob(_))
            )
        })
        .map(|b| b.literal.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::super::plan::Binding;
    use super::*;

    const GUARD: &str = "The request mentions a refund that no recognized effect carries; a refund is never dropped silently.";

    fn region(text: &str, role: &str) -> ProposedRegion {
        ProposedRegion {
            text: text.to_owned(),
            role: role.to_owned(),
        }
    }

    /// Whether the guard survives the proposal's accounting of `regions`.
    fn kept(bound: &[&str], regions: &[ProposedRegion]) -> bool {
        let mut plan = Plan::default();
        plan.bindings = (bound.iter()).map(|p| Binding::new("path", *p)).collect();
        plan.unknowns.push(GUARD.to_owned());
        reconcile_refund_backstop(&mut plan, regions);
        plan.unknowns.iter().any(|u| u == GUARD)
    }

    #[test]
    fn a_refund_named_only_by_a_bound_file_is_settled_by_the_other_regions() {
        let file = "./out/refund-tickets.json";
        let regions = || {
            [
                region("keep only the support tickets tagged refund", "operation"),
                region(&format!("Save {file} as an object with `count`"), "effect"),
            ]
        };
        assert!(!kept(&[file], &regions()));
        // The same file name with no binding stays a word of an effect region: kept.
        assert!(kept(&[], &regions()));
        // A refund the effect region asks keeps the guard, whatever the file is called.
        let asked = [region(
            &format!("refund the customer and save the receipt to {file}"),
            "effect",
        )];
        assert!(kept(&[file], &asked));
        // A directory is not a file name: its words stay.
        let dir = [region("summarize ./refunds/ into ./out/s.md", "effect")];
        assert!(kept(&["./refunds/"], &dir));
    }

    #[test]
    fn absolute_endpoints_and_refunds_after_local_names_keep_the_guard() {
        for (text, path) in [
            ("POST /api/refund.json for order 123", "/api/refund.json"),
            (
                "Appelle /api/remboursement.json pour la commande 123",
                "/api/remboursement.json",
            ),
            (
                "Save ./out/refund-tickets.json, then refund the customer",
                "./out/refund-tickets.json",
            ),
            (
                "Écris ./out/remboursement.json, puis rembourse le client",
                "./out/remboursement.json",
            ),
        ] {
            assert!(kept(&[path], &[region(text, "effect")]), "{text}");
        }
    }

    #[test]
    fn removing_every_file_name_mention_does_not_reconcile_an_existing_guard() {
        let path = "./out/refund-tickets.json";
        assert!(kept(&[path], &[region(&format!("Save {path}"), "effect")]));
        assert!(kept(&[path], &[]));
    }
}
