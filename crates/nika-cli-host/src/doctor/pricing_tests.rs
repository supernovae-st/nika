// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pricing-snapshot line — identity, staleness, and the facet its
//! counts name.
//!
//! Descended from `doctor/tests.rs` at the 1500-line wall (the descent
//! law: extraction, never exemption), on the commit that stopped the
//! line calling price rows « models » (#1179). Same `super::*` reach,
//! same fixtures — a sibling module, not a new contract.

use super::*;

fn pricing(age_days: Option<u32>) -> PricingProbe {
    PricingProbe {
        as_of: "2026-07-07".to_owned(),
        sha: "d31a39603aa5419d".to_owned(),
        rules: 606,
        providers: 10,
        age_days,
    }
}

#[test]
fn pricing_line_names_the_snapshot_identity() {
    let f = pricing_finding(&pricing(Some(3)));
    assert_eq!(f.level, Level::Ok);
    // #1179 · the count NAMES its facet. `rules` are price rows, and
    // calling them « models » set 633 beside `nika catalog`'s 69 under
    // one word — two inventories, one session, an order of magnitude.
    assert!(f.detail.contains("606 price rules"), "{}", f.detail);
    assert!(
        !f.detail.contains("models"),
        "the pricing line counts price rows and priced providers — it \
         must not speak the word `models` at all, or it lands beside \
         `nika catalog`'s model count as one word for two inventories: {}",
        f.detail
    );
    assert!(f.detail.contains("10 providers priced"), "{}", f.detail);
    assert!(f.detail.contains("2026-07-07"), "{}", f.detail);
    assert!(f.detail.contains("d31a39603aa5419d"), "{}", f.detail);
    assert!(
        f.detail.contains("list rates"),
        "the public-catalog basis is named — private/proxy deals are \
             not reflected and the line must say so: {}",
        f.detail
    );
    assert!(f.fix.is_none());
}

#[test]
fn stale_pricing_snapshot_warns_with_the_upgrade_fix() {
    // The staleness gap no surveyed tool closes: past the threshold
    // the line flips ⚠ and prints the exact remedy.
    let f = pricing_finding(&pricing(Some(PRICING_STALE_DAYS + 1)));
    assert_eq!(f.level, Level::Warn);
    assert!(f.detail.contains("days old"), "{}", f.detail);
    assert!(
        f.fix.as_deref().is_some_and(|x| x.contains("upgrade nika")),
        "{:?}",
        f.fix
    );
    // AT the threshold stays green (stale means PAST it).
    assert_eq!(
        pricing_finding(&pricing(Some(PRICING_STALE_DAYS))).level,
        Level::Ok
    );
    // An uncomputable age never guesses stale.
    assert_eq!(pricing_finding(&pricing(None)).level, Level::Ok);
}
