// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The composer's seat line (P3 B4.5). Everything else lives with its
//! owner — `nika-harness` reads the declaration, `nika-verb-agent`
//! builds and takes the seat — because this crate sits at its 15k wall
//! and every line here is rent.

/// The seat a machine offers: the declared harness under the feature,
/// a zero-sized witness without it (the composer reads the same in
/// both builds).
#[cfg(feature = "access-harness")]
pub(crate) type Seat = Option<nika_verb_agent::harness_path::HarnessSeat>;
/// The feature-off twin.
#[cfg(not(feature = "access-harness"))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seat;

/// The access-probe rows the `--access` gate judges (presence only).
pub(crate) fn access_probes() -> Vec<nika_providers::probe::ProviderProbe> {
    nika_providers::probe::collect_provider_probes(&nika_providers::ProviderRegistry::without_http(
        crate::compose::config_from_env(),
    ))
}

/// Read this machine's declaration into a seat — a declared-but-broken
/// adapter REFUSES rather than substitute the native loop (A-4).
#[cfg(feature = "access-harness")]
pub(crate) fn seat_from_env() -> Result<Seat, nika_kernel::HttpError> {
    let wrap = |why: String| nika_kernel::HttpError::Connection {
        reason: format!("harness seat: {why}"),
    };
    let Some(b) = nika_harness::seat_from_env().map_err(wrap)? else {
        return Ok(None);
    };
    nika_verb_agent::harness_path::HarnessSeat::from_backend(std::sync::Arc::new(b))
        .map(Some)
        .map_err(wrap)
}

/// The feature-off twin — the `Result` is shared on purpose so the
/// composer's one `seat_from_env()?` line is identical in both builds.
#[expect(clippy::unnecessary_wraps, reason = "the ON arm fails · shared shape")]
#[cfg(not(feature = "access-harness"))]
pub(crate) const fn seat_from_env() -> Result<Seat, nika_kernel::HttpError> {
    Ok(Seat)
}
