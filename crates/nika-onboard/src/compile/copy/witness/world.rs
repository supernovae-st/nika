// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An observed before-state, consumed only by a coherent completed rehearsal over the same
//! candidate and copied inputs. This witnesses freshness, never consent or intent correctness.

use std::path::{Path, PathBuf};

use nika_compile::surface::sha256;
use nika_compile_cognition::rehearse::{Attempt, Rehearsal, RehearsalReport, judged_run};
use nika_compile_fidelity::behavior::RunEnd;

use super::{Seen, Witness, changed, observe, relative};

#[cfg(test)]
mod tests;

/// The request's original sources and destinations observed before a rehearsal. Its private
/// fields can only be populated by rooted, bounded observation; it is not a saved proof.
#[derive(Debug)]
#[non_exhaustive]
pub struct WorldBefore {
    root: PathBuf,
    candidate_sha256: String,
    inputs: Vec<String>,
    targets: Vec<String>,
    world: Vec<(String, Seen)>,
}

impl WorldBefore {
    /// Observe the request's paths before handing a candidate to the room. Destinations are
    /// observed here without adding them to the room's inputs. Paths are never inferred from
    /// the candidate; the caller supplies the effective request's sources and destinations.
    ///
    /// # Errors
    /// Refuses a path outside the project, a symlink, an unreadable/non-regular file, or a
    /// read exceeding the observer's existing bound. An absent destination remains absent.
    pub async fn capture(
        root: &Path,
        candidate: &str,
        inputs: &[String],
        targets: &[String],
    ) -> Result<Self, String> {
        let mut paths: Vec<String> = Vec::new();
        for path in inputs.iter().chain(targets) {
            let at = relative(path)
                .ok_or_else(|| format!("`{path}` is not a path inside the project"))?;
            if !paths.iter().any(|old| relative(old).as_ref() == Some(&at)) {
                paths.push(path.clone());
            }
        }
        let observed = observe(root, &paths).await?;
        let world = paths
            .into_iter()
            .zip(observed)
            .map(|(path, seen)| {
                seen.map(|seen| (path.clone(), seen)).map_err(|why| {
                    format!("`{path}` cannot be observed before rehearsal: it {why}")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            root: root.to_owned(),
            candidate_sha256: sha256(candidate),
            inputs: inputs.to_vec(),
            targets: targets.to_vec(),
            world,
        })
    }

    /// Bind a completed report to this before-state, then observe the original project again
    /// after the host drained. Every input receipt must match the source observed beforehand;
    /// every published or declared output must have a before-state too. No destination is
    /// copied into the room merely to manufacture that state.
    ///
    /// This consumes the capture. It does not run a workflow, record consent, prove a whole
    /// output from a preview, or provide an atomic snapshot against concurrent external edits.
    ///
    /// # Errors
    /// Refuses another candidate, an incomplete/invalid report, an unobserved output, a copy
    /// differing from the before-state, or any drift in the original sources or destinations.
    pub async fn witness(
        self,
        candidate: &str,
        report: &RehearsalReport,
    ) -> Result<Witness, String> {
        if sha256(candidate) != self.candidate_sha256
            || report.candidate_sha256 != self.candidate_sha256
        {
            return Err("the rehearsal names another candidate than the captured world".to_owned());
        }
        if !matches!(report.attempt, Attempt::Completed { .. }) || report.admitted_digest.is_empty()
        {
            return Err("the rehearsal has no completed admitted attempt".to_owned());
        }
        let Rehearsal::Passed { outputs } = &report.outcome else {
            return Err("the rehearsal has no passed result to bind".to_owned());
        };
        let declared: Vec<String> = outputs.iter().map(|output| output.path.clone()).collect();
        let run = judged_run("observed", report, &self.inputs, &self.targets, &declared);
        if !matches!(run.end, RunEnd::Completed) {
            return Err(
                "the rehearsal has no coherent, drained and effect-free observation".to_owned(),
            );
        }
        for path in declared.iter().chain(&report.observation.ledger.written) {
            if self.before(path).is_none() {
                return Err(format!(
                    "the output `{path}` has no observed original before-state"
                ));
            }
        }
        for copy in &report.observation.copies {
            if !matches!(self.before(&copy.path), Some(Seen::File(digest)) if *digest == copy.source)
            {
                return Err(format!(
                    "the copied input `{}` differs from its observed before-state",
                    copy.path
                ));
            }
        }
        let paths: Vec<String> = self.world.iter().map(|(path, _)| path.clone()).collect();
        let now = observe(&self.root, &paths).await?;
        let moved: Vec<String> = self
            .world
            .iter()
            .zip(&now)
            .filter_map(|((path, before), after)| changed(path, before, after))
            .collect();
        if !moved.is_empty() {
            return Err(moved.join(" · "));
        }
        Ok(Witness::new(self.candidate_sha256, self.world).writing(&self.targets, &self.inputs))
    }

    fn before(&self, path: &str) -> Option<&Seen> {
        let at = relative(path)?;
        self.world
            .iter()
            .find_map(|(path, seen)| (relative(path).as_ref() == Some(&at)).then_some(seen))
    }
}
