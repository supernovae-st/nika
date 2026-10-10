// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The access plan of a rehearsal. It is resolved only by the pure resolver,
//! over no model override, no pin and no probe rows, and it is refused before
//! anything reads this machine when the caller asks for a pin, when the
//! candidate names a model lane, or when it holds any `infer:` or `agent:`
//! task (a model-less one yields no lane, yet would run on the default model).
//! A zero cost ceiling elsewhere is defense in depth, never this denial.
//! The run itself, over a room, is the `room` child's. A replay trial
//! ([`replay`]) keeps a model step out of the run instead: its plan refuses
//! only a pin, and its runtime reaches no provider.

use std::fmt;

use nika_providers::ExecutionAccessPlan;

use crate::ServiceExecutionDriver;

mod isolated_jq;
pub mod replay;
mod room;
#[cfg(test)]
mod room_tests;

pub use isolated_jq::{IsolatedJq, JqBound, JqHelper};
pub use room::{ADMITTED_TOOLS, DeniedEffects, DeniedTally};

/// Why a rehearsal refuses its access plan. No probe ran and nothing was built.
///
/// A verdict, not a coded error: the rehearsing host reports it as its own
/// not-run outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RehearsalPlanRefusal {
    /// The caller asked for an access pin: a rehearsal pins nothing.
    Pin,
    /// The candidate names a model lane: a rehearsal never reaches a provider.
    ModelLane,
    /// The candidate holds an `infer:` or `agent:` task with no lane of its own.
    ModelVerb,
}

impl fmt::Display for RehearsalPlanRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pin => "a rehearsal takes no access pin",
            Self::ModelLane => {
                "the candidate names a model lane, and a rehearsal reaches no provider"
            }
            Self::ModelVerb => {
                "the candidate holds an infer or agent task, and a rehearsal runs no model"
            }
        })
    }
}

impl ServiceExecutionDriver {
    /// The rehearsal's access plan: the pure resolver over no model override,
    /// no pin and no probe rows. A requested pin, then any model lane, then any
    /// `infer:` or `agent:` task, is refused before the probe cell or the probe
    /// source is touched.
    ///
    /// # Errors
    /// [`RehearsalPlanRefusal`] naming the pin, the lane or the verb.
    pub fn rehearsal_plan(
        &self,
        pin: Option<&str>,
    ) -> Result<ExecutionAccessPlan, RehearsalPlanRefusal> {
        if pin.is_some() {
            return Err(RehearsalPlanRefusal::Pin);
        }
        let plan = self.resolve_access_plan_over(None, None, &[]);
        if !plan.lanes.is_empty() {
            return Err(RehearsalPlanRefusal::ModelLane);
        }
        let verbs = crate::access::verb_needs(&self.workflow);
        if verbs.infer || verbs.agent {
            return Err(RehearsalPlanRefusal::ModelVerb);
        }
        Ok(plan)
    }

    /// A replay trial's access plan: the pure resolver's, over no model override, no pin and
    /// no probe rows. A model lane or an `infer:`/`agent:` task is kept out of the trial rather
    /// than refused: the trial's runtime has no provider transport and holds no key, and its
    /// agent seat refuses every turn, so a model step fails where it stands and the trial's
    /// screen names it not exercised ([`replay::screen`]).
    ///
    /// # Errors
    /// [`RehearsalPlanRefusal::Pin`] when the caller asked for one.
    pub fn replay_plan(
        &self,
        pin: Option<&str>,
    ) -> Result<ExecutionAccessPlan, RehearsalPlanRefusal> {
        if pin.is_some() {
            return Err(RehearsalPlanRefusal::Pin);
        }
        let mut plan = self.resolve_access_plan_over(None, None, &[]);
        // No launch refusal for a model lane: its step fails where it stands.
        plan.lanes.clear();
        Ok(plan)
    }
}
