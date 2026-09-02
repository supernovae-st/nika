// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The one machine-truth projection (RAMS-12) — three provider facets,
//! one derivation.
//!
//! Three surfaces speak a provider number and the 2026-07-31 audit
//! caught them at « 15 » (welcome) · « 10 » (doctor's cloud rows) ·
//! « 38 » (catalog) under the same bare word — three true numbers
//! reading as one contradiction (A-06 · Dmitri · Yuki). The numbers
//! were never wrong; the FACETS were unnamed. This module is the single
//! derivation every renderer reads, and the transversal e2e test
//! (`nika-cli/tests/machine_truth_surfaces.rs`) pins each RENDERED
//! surface to its facet — a future drift fails a test instead of
//! confusing a user.
//!
//! The facets deliberately do NOT nest: the catalog carries entries the
//! runtime cannot execute without a key, and the runtime wires local
//! engines the catalog data does not carry yet (the local five ride
//! `CANONICAL_IDS`, not catalog rows) — so neither count contains the
//! other, and any render that implies a subset lies.

use nika_providers::ProviderRegistry;
use nika_providers::probe::ProviderProbe;

/// The three facets a provider count can mean. Every surface that
/// prints one of these numbers names WHICH — that is the whole repair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineTruth {
    /// Entries in the embedded static catalog (`nika catalog` rows —
    /// providers with model/pricing data compiled in).
    pub catalog_entries: usize,
    /// Providers wired in THIS build — the registry a run executes
    /// through, cloud and local engines alike.
    pub wired: usize,
    /// The wired providers that take an API key — the doctor's cloud
    /// rows, the welcome's « of N clouds » denominator.
    pub cloud_key_slots: usize,
    /// The keyless local servers among the wired (#1398 · the facet's split).
    pub local: usize,
}

impl MachineTruth {
    /// Derive from an existing provider-probe slice — the shape the
    /// doctor and the welcome already hold (`probe::collect`).
    #[must_use]
    pub fn from_probes(probes: &[ProviderProbe]) -> Self {
        Self {
            catalog_entries: nika_catalog::all_providers().len(),
            wired: probes.len(),
            cloud_key_slots: probes.iter().filter(|p| p.requires_key).count(),
            local: probes.iter().filter(|p| !p.requires_key).count(),
        }
    }

    /// The one wired-facet sentence (#1398): the same words, the same
    /// numbers, on the card, the catalog header and the check refusal.
    #[must_use]
    pub fn wired_facet(&self) -> String {
        nika_providers::wired_facet(self.wired, self.local)
    }

    /// Derive from a registry — the catalog verb's path (it composes
    /// the same registry a run would use, overrides included).
    #[must_use]
    pub fn from_registry(registry: &ProviderRegistry) -> Self {
        Self::from_probes(&nika_providers::probe::collect_provider_probes(registry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_probe_slice_still_carries_the_catalog_facet() {
        let truth = MachineTruth::from_probes(&[]);
        assert_eq!(truth.wired, 0);
        assert_eq!(truth.cloud_key_slots, 0);
        assert!(
            truth.catalog_entries > 0,
            "the embedded catalog is never empty"
        );
    }

    /// D-2026-08-04-N1 · the access partition needs NO new facet today:
    /// probes exclude `mock`, so `api == cloud_key_slots` and
    /// `local == wired - cloud_key_slots` — the numbers already live
    /// under the key-vocabulary names, and a second name for the same
    /// number is the exact A-06 disease this module exists to cure.
    /// This pin BREAKS the day a keyless non-local class (harness ·
    /// oauth · P3) joins the probe slice — forcing the facet decision
    /// then, with real rows on the table.
    #[test]
    fn access_partition_is_the_facet_arithmetic_until_p3() {
        use nika_providers::probe::AccessClass;
        let registry = ProviderRegistry::without_http(nika_runtime::compose::config_from_env());
        let probes = nika_providers::probe::collect_provider_probes(&registry);
        let truth = MachineTruth::from_probes(&probes);
        let api = probes
            .iter()
            .filter(|p| p.readiness.access == AccessClass::Api)
            .count();
        let local = probes
            .iter()
            .filter(|p| p.readiness.access == AccessClass::Local)
            .count();
        assert_eq!(api, truth.cloud_key_slots, "api IS the key-slot facet");
        assert_eq!(
            api + local,
            truth.wired,
            "the partition covers the wired set"
        );
    }

    #[test]
    fn registry_facets_hold_their_arithmetic() {
        let registry = ProviderRegistry::without_http(nika_runtime::compose::config_from_env());
        let truth = MachineTruth::from_registry(&registry);
        assert!(truth.wired > 0, "a build ships at least one wired provider");
        assert!(
            truth.cloud_key_slots <= truth.wired,
            "every key slot is a wired provider"
        );
        assert!(
            truth.cloud_key_slots < truth.wired,
            "the sovereign path exists: at least one wired provider takes no key"
        );
    }
}
