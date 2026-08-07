// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The harness adapter surface of the host (P3 B6 · feature
//! `access-harness`): the registry's probes projected for the two
//! consumers — the resolver channel (provider-row shape, R-5c one
//! channel) and the doctor section. Lives beside `probe.rs` under the
//! 1,500-LOC file law; re-exported there so every call site reads
//! `probe::…` unchanged.

use nika_providers::ProviderRegistry;
use nika_providers::probe::ProviderProbe;

/// P3 B6 · the harness adapters as RESOLVER probe rows (feature
/// `access-harness`): the registry rows that probed detected become
/// harness-class [`ProviderProbe`] rows (`serves` set · the auth
/// surface's verdict in `configured`), riding the SAME vec as the
/// provider rows so the admission gate, `check --json` and `explain`
/// all read one channel (R-5c). An undetected adapter yields no row —
/// its place is the doctor section, never the resolver's input.
#[cfg(feature = "access-harness")]
#[must_use]
pub fn harness_provider_rows() -> Vec<ProviderProbe> {
    let Ok(rows) = nika_harness::registry() else {
        return Vec::new(); // a broken registry offers nothing (fail-closed)
    };
    let serves_of: std::collections::BTreeMap<String, Vec<String>> = rows
        .iter()
        .map(|r| {
            (
                r.adapter.id.clone(),
                r.serves.iter().map(|s| (*s).to_owned()).collect(),
            )
        })
        .collect();
    nika_harness::probe_adapters_sync(rows)
        .into_iter()
        // An undetected adapter yields no row (its place is the doctor
        // section, never the resolver's input).
        .filter(|row| row.version.is_some())
        .map(|row| {
            let configured = row.authenticated == Some(true);
            let readiness = nika_providers::probe::ProviderReadiness::new(
                true,
                configured,
                None,
                None,
                false,
                nika_providers::probe::ExecutionLocus::Loopback,
                nika_types::access::AccessClass::Harness,
            );
            ProviderProbe::new(row.id.clone(), false, configured, "", false, readiness, "")
                .with_serves(serves_of.get(&row.id).cloned().unwrap_or_default())
        })
        .collect()
}

/// The access-probe rows every admission surface judges (P3 B6): the
/// provider rows PLUS the harness rows when the feature is on — ONE
/// fn, so the run's gate and `check`/`explain` can never drift (the
/// composer's `production_runtime` internal collection is the
/// non-CLI default; this is the CLI surfaces' superset).
#[cfg(feature = "access-harness")]
#[must_use]
pub fn access_probes_with_harness() -> Vec<ProviderProbe> {
    let mut probes = nika_providers::probe::collect_provider_probes(
        &ProviderRegistry::without_http(nika_runtime::compose::config_from_env()),
    );
    probes.extend(harness_provider_rows());
    probes
}

/// Feature-off twin of [`access_probes_with_harness`] — the same call
/// reads identically in both builds (the seat's zero-sized-witness
/// precedent): the provider rows alone.
#[cfg(not(feature = "access-harness"))]
#[must_use]
pub fn access_probes_with_harness() -> Vec<ProviderProbe> {
    nika_providers::probe::collect_provider_probes(&ProviderRegistry::without_http(
        nika_runtime::compose::config_from_env(),
    ))
}
