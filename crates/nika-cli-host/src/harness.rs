// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The harness adapter surface of the host (P3 B6 · feature
//! `access-harness`): the registry's probes projected for the two
//! consumers — the resolver channel (provider-row shape, R-5c one
//! channel) and the doctor section. Lives beside `probe.rs` under the
//! 1,500-LOC file law; re-exported there so every call site reads
//! `probe::…` unchanged.

use nika_providers::probe::ProviderProbe;
#[cfg(feature = "access-harness")]
use nika_providers::probe::harness_access_probe;

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
    nika_harness::presence_facts(rows)
        .into_iter()
        .map(|fact| {
            harness_access_probe(
                fact.id,
                fact.serves,
                fact.acp_present,
                fact.configured,
                fact.product_present,
            )
        })
        .collect()
}

/// The access-probe rows every admission surface judges (P3 B6): the
/// provider rows PLUS the harness rows when the feature is on — ONE
/// door, so the run's gate, `check`/`explain` and the resident's jobs
/// can never drift (since wave 1b the door itself is
/// [`nika_service_execution::access::access_probes_env`]; this is its
/// CLI-facing name).
#[must_use]
pub fn access_probes_with_harness() -> Vec<ProviderProbe> {
    nika_service_execution::access::access_probes_env()
}
