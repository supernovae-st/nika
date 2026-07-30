// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Spec ⊆ catalog — every canonical provider prefix the vendored
// canon.yaml names must resolve as a canonical id in nika-catalog.
//
// Direction matters: the catalog MAY know more than the language admits
// (an un-blessed provider stays reachable via the `openai` + base_url
// hatch · spec stdlib/providers-v0.1.md §The three lists), so this
// asserts inclusion, never equality. What it refuses: a vendored spec
// naming a prefix the shipped catalog cannot resolve, or a canon whose
// own count disagrees with its own lists.

use std::collections::BTreeSet;

/// Parse `providers.{count,items.{local,cloud,test}}` out of the
/// vendored canon.yaml (via the same embedded surface the binary uses).
fn canon_providers() -> (u64, Vec<String>) {
    let canon: serde_yaml_bw::Value =
        serde_yaml_bw::from_str(nika_pack::canon()).expect("vendored canon.yaml parses");
    let providers = canon
        .get("providers")
        .expect("canon.yaml carries a providers section");
    let count = providers
        .get("count")
        .and_then(serde_yaml_bw::Value::as_u64)
        .expect("providers.count is an integer");
    let items = providers.get("items").expect("providers.items exists");
    let mut ids = Vec::new();
    for tier in ["local", "cloud", "test"] {
        let seq = items
            .get(tier)
            .and_then(serde_yaml_bw::Value::as_sequence)
            .unwrap_or_else(|| panic!("providers.items.{tier} is a list"));
        for v in seq {
            ids.push(
                v.as_str()
                    .expect("every canonical prefix is a string")
                    .to_owned(),
            );
        }
    }
    (count, ids)
}

#[test]
fn canon_count_equals_its_own_lists_and_stays_unique() {
    let (count, ids) = canon_providers();
    let unique: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "a canonical prefix appears twice across local/cloud/test"
    );
    assert_eq!(
        count,
        ids.len() as u64,
        "providers.count disagrees with the union of its own lists"
    );
}

#[test]
fn every_canonical_prefix_is_a_catalog_id() {
    let (_, ids) = canon_providers();
    let catalog: BTreeSet<&str> = nika_catalog::all_providers().iter().map(|p| p.id).collect();
    let missing: Vec<&str> = ids
        .iter()
        .map(String::as_str)
        .filter(|id| !catalog.contains(id))
        .collect();
    assert!(
        missing.is_empty(),
        "canonical prefixes the shipped catalog cannot resolve: {missing:?} \
         (catalog knows {} providers · the spec freeze must be a subset)",
        catalog.len()
    );
}
