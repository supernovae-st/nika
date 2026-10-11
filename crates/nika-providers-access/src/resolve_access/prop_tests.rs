// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resolver's property tests (beside the module at the 1,500-line wall).

use super::*;
use proptest::prelude::*;

fn class_strategy() -> impl Strategy<Value = AccessClass> {
    prop_oneof![
        Just(AccessClass::Local),
        Just(AccessClass::Api),
        Just(AccessClass::Harness),
        Just(AccessClass::Oauth),
        Just(AccessClass::Mock),
    ]
}

/// Pins draw from BOTH grammars the flag accepts: candidate-id
/// shaped tokens AND the class wire strings (the review's blind
/// spot: `[a-e]{1,6}` can never spell `local`).
fn pin_strategy() -> impl Strategy<Value = Option<String>> {
    proptest::option::of(prop_oneof![
        "[a-e]{1,6}".prop_map(String::from),
        class_strategy().prop_map(|c| c.as_str().to_owned()),
    ])
}

prop_compose! {
    fn candidate_strategy()(
        id in "[a-e]{1,6}",
        class in class_strategy(),
        configured in any::<bool>(),
        fix in proptest::option::of("[A-Z_]{0,12}"),
    ) -> AccessCandidate {
        let candidate = AccessCandidate::new(id, class, configured);
        match fix {
            Some(var) => candidate.with_fix_var(var),
            None => candidate,
        }
    }
}

/// A deterministic Fisher-Yates from a seed — the shuffle itself
/// must not depend on ambient randomness (instrument law).
fn shuffle(mut v: Vec<AccessCandidate>, seed: u64) -> Vec<AccessCandidate> {
    let mut state = seed | 1;
    for i in (1..v.len()).rev() {
        // Numerical Recipes LCG — cheap, deterministic, test-only.
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        #[allow(clippy::cast_possible_truncation)]
        let j = (state % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

proptest! {
    /// THE determinism law: enumeration order never matters — the
    /// resolver totally orders candidates internally.
    #[test]
    fn permutation_never_changes_the_outcome(
        candidates in proptest::collection::vec(candidate_strategy(), 0..8),
        pin in pin_strategy(),
        provider in "[a-e]{1,3}",
        allow in proptest::option::of(proptest::collection::vec("[a-e]{1,3}", 0..3)),
        seed in any::<u64>(),
    ) {
        let model = format!("{provider}/m");
        let first = resolve_access(&model, &candidates, allow.as_deref(), pin.as_deref());
        let shuffled = shuffle(candidates, seed);
        let second = resolve_access(&model, &shuffled, allow.as_deref(), pin.as_deref());
        prop_assert_eq!(first, second);
    }

    /// Witness totality: every rejection names an input candidate
    /// and carries a non-empty witness; a chosen path is genuinely
    /// admissible; a refusal accounts for EVERY candidate.
    #[test]
    fn witnesses_are_total_and_the_chosen_is_admissible(
        candidates in proptest::collection::vec(candidate_strategy(), 0..8),
        pin in pin_strategy(),
        provider in "[a-e]{1,3}",
        allow in proptest::option::of(proptest::collection::vec("[a-e]{1,3}", 0..3)),
    ) {
        let model = format!("{provider}/m");
        let ids: Vec<&str> = candidates.iter().map(|c| c.access.as_str()).collect();
        match resolve_access(&model, &candidates, allow.as_deref(), pin.as_deref()) {
            Ok(plan) => {
                prop_assert!(ids.contains(&plan.access.as_str()));
                prop_assert!(plan.rejected.len() < candidates.len());
                for r in &plan.rejected {
                    prop_assert!(ids.contains(&r.access.as_str()));
                    prop_assert!(!r.witness.is_empty());
                }
                // (id · class) does not single out a row when twin
                // ids ride the input (the shrunk regression case:
                // one configured, one not — the resolver picks the
                // configured twin). The honest predicate: SOME
                // configured input carries the chosen (id · class).
                prop_assert!(
                    candidates.iter().any(|c| c.access == plan.access
                        && c.class == plan.chosen
                        && c.configured),
                    "the chosen path must name a configured input row"
                );
                if let Some(list) = allow.as_deref() {
                    prop_assert!(list.iter().any(|p| p == &provider));
                }
                if let Some(p) = pin.as_deref() {
                    prop_assert!(plan.pinned);
                    prop_assert!(p == plan.access || p == plan.chosen.as_str());
                }
            }
            Err(refusal) => {
                prop_assert_eq!(refusal.rejected.len(), candidates.len());
                for r in &refusal.rejected {
                    prop_assert!(ids.contains(&r.access.as_str()));
                    prop_assert!(!r.witness.is_empty());
                }
            }
        }
    }
}
