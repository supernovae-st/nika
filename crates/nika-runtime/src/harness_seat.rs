// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Composer seat line. This crate is at the 15k wall.

#[cfg(feature = "access-harness")]
pub(crate) type Seat = Option<nika_verb_agent::harness_path::HarnessSeat>;
#[cfg(not(feature = "access-harness"))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seat;

#[cfg(feature = "access-harness")]
pub(crate) fn seat_from_env() -> Result<Seat, nika_kernel::HttpError> {
    let Some(b) = nika_harness::seat_from_env().map_err(nika_harness::seat_http_err)? else {
        return Ok(None);
    };
    nika_verb_agent::harness_path::HarnessSeat::from_backend(std::sync::Arc::new(b))
        .map(Some)
        .map_err(nika_harness::seat_http_err)
}

#[expect(clippy::unnecessary_wraps, reason = "the ON arm fails · shared shape")]
#[cfg(not(feature = "access-harness"))]
pub(crate) const fn seat_from_env() -> Result<Seat, nika_kernel::HttpError> {
    Ok(Seat)
}

#[cfg(feature = "access-harness")]
pub(crate) fn declared_id() -> Option<String> {
    nika_harness::declared_adapter_id()
}

#[cfg(not(feature = "access-harness"))]
pub(crate) const fn declared_id() -> Option<String> {
    None
}

impl<S, T, H, P, D, C> crate::Runtime<S, T, H, P, D, C> {
    /// Seat `--access` (pin wins over env).
    ///
    /// # Errors
    ///
    /// The registry row cannot be built.
    #[cfg(feature = "access-harness")]
    pub fn with_harness_from_pin(
        mut self,
        pin: Option<&str>,
    ) -> Result<Self, nika_kernel::HttpError> {
        let ready = nika_providers::first_ready_harness(&self.access_probes);
        let Some((b, id)) =
            nika_harness::seat_from_pin(pin, ready).map_err(nika_harness::seat_http_err)?
        else {
            return Ok(self);
        };
        self.agent = self.agent.with_harness_seat(
            nika_verb_agent::harness_path::HarnessSeat::from_backend(std::sync::Arc::new(b))
                .map_err(nika_harness::seat_http_err)?,
        );
        self.harness_seat_id = Some(id);
        Ok(self)
    }

    #[expect(clippy::unnecessary_wraps, reason = "the ON arm fails · shared shape")]
    #[cfg(not(feature = "access-harness"))]
    pub fn with_harness_from_pin(self, pin: Option<&str>) -> Result<Self, nika_kernel::HttpError> {
        let _ = pin;
        Ok(self)
    }
}
