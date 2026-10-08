// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Local Run launch configuration; negotiation is not an approval.
use nika_display::check_render::RepairTarget;

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct RunHostOptions {
    pub repair_target: Option<RepairTarget>,
    pub cost_review_stdio: bool,
    /// The witness the captured source must have, or nothing runs (`--expect-source`).
    pub expected_source: Option<String>,
    /// The closure the admitted world must have, or nothing runs (`--expect-world`).
    pub expected_world: Option<String>,
}

/// What a Session's check judged, which the run it requests must capture (machine lane).
#[derive(Clone, Debug, Default, clap::Args)]
#[non_exhaustive]
pub struct SessionBinding {
    /// Run only a source with this BLAKE3 witness: the bytes a Session checked (machine lane).
    #[arg(long, value_name = "BLAKE3", hide = true)]
    pub expect_source: Option<String>,
    /// Run only a world with this closure: the workflow, children and skills it checked.
    #[arg(long, value_name = "SHA256", hide = true)]
    pub expect_world: Option<String>,
}
impl From<RepairTarget> for RunHostOptions {
    fn from(target: RepairTarget) -> Self {
        Some(target).into()
    }
}
impl From<Option<RepairTarget>> for RunHostOptions {
    fn from(repair_target: Option<RepairTarget>) -> Self {
        Self {
            repair_target,
            ..Self::default()
        }
    }
}
impl RunHostOptions {
    #[must_use]
    pub fn with_cost_review_stdio(mut self, enabled: bool) -> Self {
        self.cost_review_stdio = enabled;
        self
    }
    #[must_use]
    pub fn with_expected_source(mut self, witness: Option<String>) -> Self {
        self.expected_source = witness;
        self
    }
    #[must_use]
    pub fn with_session_binding(mut self, binding: SessionBinding) -> Self {
        (self.expected_source, self.expected_world) = (binding.expect_source, binding.expect_world);
        self
    }
}
