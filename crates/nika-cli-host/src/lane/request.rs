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
}
