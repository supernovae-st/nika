// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a `save & run` runs after its save (NIK-14): the set's own run request exactly, or the one
//! workflow it saves under the run admission's own spending. A target is never chosen by its
//! position, and no value passes through a command line on its way to the admission.

use std::path::PathBuf;

use crate::change::ProjectChangeSet;

/// The run a `save & run` asks once its save checked clean, typed for the run admission.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SaveRun {
    /// The workflow to run, relative to the root.
    pub workflow: PathBuf,
    /// Its inputs, `name=value`, exactly as the request carried them.
    pub vars: Vec<String>,
    /// The access pin the request named, when it did.
    pub access_pin: Option<String>,
    /// The ceiling the request stated; `None` leaves the amount to the admission's own spending
    /// (the saved proposal's reviewed decision), never a default made up here.
    pub max_cost_usd: Option<f64>,
}

impl ProjectChangeSet {
    /// What a `save & run` of this set runs after its save: its own run request exactly, else the
    /// one workflow it saves, with no amount stated.
    ///
    /// # Errors
    /// Why there is none: a stated ceiling that is not a finite nonnegative amount, no workflow,
    /// or several workflows and no request naming the one to run.
    pub fn save_run(&self) -> Result<SaveRun, &'static str> {
        if let Some(run) = &self.run {
            if !run.max_cost_usd.is_finite() || run.max_cost_usd < 0.0 {
                return Err(
                    "this proposal's own run request states a ceiling that is not a finite nonnegative amount — `yes` saves it",
                );
            }
            return Ok(SaveRun {
                workflow: run.workflow.clone(),
                vars: run.vars.clone(),
                access_pin: run.access_pin.clone(),
                max_cost_usd: Some(run.max_cost_usd),
            });
        }
        match self.workflows().as_slice() {
            [one] => Ok(SaveRun {
                workflow: one.clone(),
                vars: Vec::new(),
                access_pin: None,
                max_cost_usd: None,
            }),
            [] => Err("this proposal saves no workflow to run — `yes` saves it"),
            _ => Err(
                "this proposal saves several workflows and names none to run — `yes` saves them, then « run <file> » runs the one you name",
            ),
        }
    }
}
