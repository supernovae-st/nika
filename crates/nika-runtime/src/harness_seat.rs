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
/// When adapters are compiled in, every shipped runtime rides the vec
/// (undetected included) so a known token is never NIKA-1802.
pub(crate) fn access_probes() -> Vec<nika_providers::probe::ProviderProbe> {
    let mut probes = nika_providers::probe::collect_provider_probes(
        &nika_providers::ProviderRegistry::without_http(crate::compose::config_from_env()),
    );
    #[cfg(feature = "access-harness")]
    {
        probes.extend(harness_provider_rows());
    }
    probes
}

/// Same projection the CLI host uses — one derivation
/// ([`nika_providers::probe::harness_access_probe`]).
#[cfg(feature = "access-harness")]
fn harness_provider_rows() -> Vec<nika_providers::probe::ProviderProbe> {
    let Ok(rows) = nika_harness::registry() else {
        return Vec::new();
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
            nika_providers::probe::harness_access_probe(
                row.id.clone(),
                serves_of.get(&row.id).cloned().unwrap_or_default(),
                row.version.is_some(),
                row.authenticated == Some(true),
                row.product_present,
            )
        })
        .collect()
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

/// The declared adapter id for the boot-manifest `harness_seat` stamp
/// (B6): the trace names the execution override beside the resolver's
/// plan. `None` in both builds when nothing is declared — and always
/// without the feature (no seat exists to name).
#[cfg(feature = "access-harness")]
pub(crate) fn declared_id() -> Option<String> {
    nika_harness::declared_adapter_id()
}

/// The feature-off twin (always `None` — no seat exists to name).
#[cfg(not(feature = "access-harness"))]
pub(crate) const fn declared_id() -> Option<String> {
    None
}

impl<S, T, H, P, D, C> crate::Runtime<S, T, H, P, D, C> {
    /// Reseat the agent from an explicit `--access` pin. Pin wins over
    /// `NIKA_HARNESS_ADAPTER`. Class `harness` seats the first ready
    /// runtime in G-3 order. Other class / provider pins leave the env
    /// seat (if any) alone.
    ///
    /// # Errors
    ///
    /// A declared pin whose registry row cannot be built.
    #[cfg(feature = "access-harness")]
    pub fn with_harness_from_pin(
        mut self,
        pin: Option<&str>,
    ) -> Result<Self, nika_kernel::HttpError> {
        let wrap = |why: String| nika_kernel::HttpError::Connection {
            reason: format!("harness seat: {why}"),
        };
        let id = match pin {
            Some(p) if nika_types::access::HarnessRuntime::lookup(p).is_some() => {
                Some(p.to_owned())
            }
            Some("harness") => {
                nika_providers::first_ready_harness(&self.access_probes).map(str::to_owned)
            }
            _ => return Ok(self),
        };
        let Some(id) = id else {
            return Ok(self);
        };
        let Some(backend) = nika_harness::seat_from_id(&id).map_err(wrap)? else {
            return Ok(self);
        };
        let seat =
            nika_verb_agent::harness_path::HarnessSeat::from_backend(std::sync::Arc::new(backend))
                .map_err(wrap)?;
        self.agent = self.agent.with_harness_seat(seat);
        self.harness_seat_id = Some(id);
        Ok(self)
    }

    /// Feature-off twin — the pin is judged at admission; nothing to seat.
    #[expect(clippy::unnecessary_wraps, reason = "the ON arm fails · shared shape")]
    #[cfg(not(feature = "access-harness"))]
    pub fn with_harness_from_pin(self, pin: Option<&str>) -> Result<Self, nika_kernel::HttpError> {
        let _ = pin;
        Ok(self)
    }
}
