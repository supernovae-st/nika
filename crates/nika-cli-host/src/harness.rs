// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The harness adapter surface of the host (P3 B6 · feature
//! `access-harness`): the registry's probes projected for the two
//! consumers — the resolver channel (provider-row shape, R-5c one
//! channel) and the doctor section. Lives beside `probe.rs` under the
//! 1,500-LOC file law; re-exported there so every call site reads
//! `probe::…` unchanged.

use nika_providers::ProviderRegistry;
use nika_providers::probe::{ProviderProbe, harness_access_probe};

/// P3 B6 · every shipped adapter as a RESOLVER probe row (feature
/// `access-harness`). Undetected rows stay on the vec so `--access
/// claude-code` is a known token (NIKA-1803, never 1802). `key_present`
/// is the ACP speaker; `configured` is the harness's own sign-in;
/// `fix_var` carries the dummy-readable install / sign-in line.
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
        .map(|row| {
            harness_access_probe(
                row.id.clone(),
                serves_of.get(&row.id).cloned().unwrap_or_default(),
                row.version.is_some(),
                row.authenticated == Some(true),
                row.product_present,
            )
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
