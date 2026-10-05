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
    /// Attach the selected backend.
    ///
    /// # Errors
    /// Refuses when the adapter cannot be seated or the admitted project
    /// cannot be held as the received-image room.
    #[cfg(feature = "access-harness")]
    pub fn with_harness_backend(
        self,
        backend: std::sync::Arc<dyn nika_kernel::ai::harness::DynAgentBackend>,
        id: String,
    ) -> Result<Self, nika_kernel::HttpError> {
        let seat = nika_verb_agent::harness_path::HarnessSeat::from_backend(backend)
            .map_err(nika_harness::seat_http_err)?;
        let mut runtime = self.with_seat(Some(seat))?;
        runtime.harness_seat_id = Some(id);
        Ok(runtime)
    }

    /// Seat a harness with the admitted project held as its image room. A
    /// refused anchor refuses the seat: there is no ambient store. Without a
    /// project root no store is attached and a received image fails its task.
    #[cfg(feature = "access-harness")]
    pub(crate) fn with_seat(mut self, seat: Seat) -> Result<Self, nika_kernel::HttpError> {
        let Some(seat) = seat else { return Ok(self) };
        self.agent = self.agent.with_harness_seat(seat);
        let Some(project_path) = &self.config.sandbox_root else {
            return Ok(self);
        };
        let room = nika_runtime_laws::image_room::ImageRoom::open(project_path).map_err(|e| {
            nika_harness::seat_http_err(format!("image room {}: {e}", project_path.display()))
        })?;
        let room = std::sync::Arc::new(room);
        self.image_room = Some(std::sync::Arc::clone(&room));
        self.agent = self.agent.with_harness_image_store(room);
        Ok(self)
    }

    #[expect(clippy::unnecessary_wraps, reason = "the ON arm fails · shared shape")]
    #[cfg(not(feature = "access-harness"))]
    pub(crate) fn with_seat(self, seat: Seat) -> Result<Self, nika_kernel::HttpError> {
        let Seat = seat;
        Ok(self)
    }
}
