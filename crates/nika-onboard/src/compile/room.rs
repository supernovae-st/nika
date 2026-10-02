// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The observed room, the native door's rehearsal host. A candidate is screened before any room
//! exists: it runs only the room's file builtins and pure tools, every path a literal or a
//! constant names stays in the room, and a read names an observed input or one of its own
//! outputs. Each observed input is then copied whole into a fresh scratch room at the same
//! relative path, through a read-only handle on the original, bounded before any allocation.
//! The candidate is admitted from its bytes over an empty directory of its own (never written
//! into the room) and runs through the existing runtime on a filesystem rooted at the room, with
//! no provider, network, process, gate, secret or nested run. At the time bound the run is
//! stopped where it stands; every operation it started is joined before the outputs are read
//! back, and the room is deleted and verified gone. A candidate the room cannot run safely is
//! not run, and the report says why.

mod arguments;
mod record;
mod screen;
mod world;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use nika_compile::surface::sha256;
use nika_compile_cognition::rehearse::{
    Attempt, Bounds, CopyReceipt, EffectCounts, FinalReceipt, Refusal, Rehearsal, RehearsalFuture,
    RehearsalReport, Rehearse, RoomEvidence,
};
use nika_execution::ExecutionService;
use nika_fs::{EffectLedger, OwnedDir, RoomLimits, RootedFs};
use nika_service_execution::{DeniedTally, ExecutionAccessPlan, ServiceExecutionDriver};

use record::{Ran, Record};
use screen::{Refused, Screened};
use world::Scratch;

/// A rehearsal host over one project: its observed inputs, copied into a scratch room.
#[derive(Clone, Debug)]
pub struct ObservedRoom {
    root: PathBuf,
    bound: Duration,
    scratch_parent: Option<PathBuf>,
}

impl ObservedRoom {
    /// The runtime timeout; preparation, drain and observation are outside it.
    pub const BOUND: Duration = Duration::from_secs(10);

    /// The logical name the candidate is admitted under. It names the candidate's bytes in the
    /// admission snapshot only: no file of that name is written into the room, and a data file
    /// of that name keeps its own bytes.
    pub const LOGICAL_ROOT: &'static str = "candidate.nika";

    /// The bytes of observed inputs one room copies, in total. A larger world is never cut: it
    /// is not rehearsed.
    pub const COPY_BOUND: u64 = 1024 * 1024;

    /// The bytes of text a report keeps of each file it read.
    pub const PREVIEW_BOUND: u64 = 64 * 1024;

    /// A room over the project at `root` with runtime timeout [`ObservedRoom::BOUND`].
    /// Its scratch directories are under the system's temporary directory.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            bound: Self::BOUND,
            scratch_parent: None,
        }
    }

    /// The same room with another runtime timeout; setup and drainage stay outside it.
    #[must_use]
    pub fn with_bound(mut self, bound: Duration) -> Self {
        self.bound = bound;
        self
    }

    /// The same room, its scratch directories created under `parent`.
    #[must_use]
    pub fn with_scratch_parent(mut self, parent: impl Into<PathBuf>) -> Self {
        self.scratch_parent = Some(parent.into());
        self
    }

    /// The project the room copies its observed inputs from.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the room creates its scratch directories, when the host named a place.
    #[must_use]
    pub fn scratch_parent(&self) -> Option<&Path> {
        self.scratch_parent.as_deref()
    }
}

impl Rehearse for ObservedRoom {
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }

    fn bound(&self) -> Duration {
        self.bound
    }

    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        let job = Job {
            project: self.root.clone(),
            scratch_parent: self
                .scratch_parent
                .clone()
                .unwrap_or_else(std::env::temp_dir),
            bound: self.bound,
            candidate: candidate.to_owned(),
            inputs: inputs.to_vec(),
            targets: targets.to_vec(),
            candidate_sha256: sha256(candidate),
        };
        Box::pin(async move {
            let candidate_sha256 = job.candidate_sha256.clone();
            // The whole rehearsal runs on a worker and an executor of its own: the run is driven
            // there, and its room is drained and removed before the worker answers.
            match tokio::task::spawn_blocking(move || job.run()).await {
                Ok(report) => report,
                Err(_) => lost(candidate_sha256),
            }
        })
    }
}

/// The report of a worker that stopped before it answered: no run it can vouch for, and no
/// verified cleanup.
fn lost(candidate_sha256: String) -> RehearsalReport {
    RehearsalReport::new(
        Rehearsal::NotRun {
            reason: "the rehearsal worker stopped before it reported".to_owned(),
        },
        Attempt::NeverAttempted,
        EffectCounts::none(),
        candidate_sha256,
    )
    .with_room(RoomEvidence::new(true, false))
}

/// One rehearsal, owned by its worker.
struct Job {
    project: PathBuf,
    scratch_parent: PathBuf,
    bound: Duration,
    candidate: String,
    inputs: Vec<String>,
    targets: Vec<String>,
    candidate_sha256: String,
}

/// The admitted candidate, ready to run in its prepared room.
struct Ready {
    copies: Vec<CopyReceipt>,
    admitted_digest: String,
    driver: ServiceExecutionDriver,
    plan: ExecutionAccessPlan,
}

impl Job {
    /// Screen the candidate, then rehearse it on an executor of this worker's own.
    fn run(self) -> RehearsalReport {
        let screened = match screen::screen(&self.candidate, &self.inputs) {
            Ok(screened) => screened,
            Err(refused) => return record::refused(self.candidate_sha256, refused),
        };
        let executor = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        match executor {
            Ok(executor) => executor.block_on(self.rehearse(&screened)),
            Err(error) => record::refused(
                self.candidate_sha256,
                Refused::new(
                    Refusal::NotBuilt,
                    format!("the rehearsal executor could not start: {error}"),
                ),
            ),
        }
    }

    /// Make the room, prepare it, run the candidate there, read it back, close and remove it.
    async fn rehearse(self, screened: &Screened) -> RehearsalReport {
        let (scratch, room_dir, admit_dir) = match Scratch::create(&self.scratch_parent) {
            Ok(made) => made,
            Err(error) => {
                let why = format!("no room could be made under the scratch parent: {error}");
                return record::refused(
                    self.candidate_sha256,
                    Refused::new(Refusal::NotBuilt, why),
                );
            }
        };
        let ledger = EffectLedger::new(RoomLimits::new(world::ROOM_BYTES, world::ROOM_FILES));
        let room = Arc::new(RootedFs::new(room_dir, Arc::clone(&ledger)));
        let prepared = self.prepare(&room, &admit_dir, &scratch, screened).await;
        drop(admit_dir);
        let mut drained = world::next_phase(&ledger).await.is_ok();
        let ready = match prepared {
            Ok(ready) => ready,
            Err((refused, copies)) => {
                // No run: the room's last two phases close empty.
                for _ in 0..2 {
                    drained &= world::next_phase(&ledger).await.is_ok();
                }
                let record = self.close(scratch, room, &ledger, copies, Vec::new(), drained);
                return record.withdrawn(refused);
            }
        };
        let started = Instant::now();
        let tally = Arc::new(DeniedTally::default());
        let run = ready
            .driver
            .rehearse_over(Arc::clone(&room), ready.plan, Arc::clone(&tally));
        let settled = tokio::time::timeout(self.bound, run).await.ok();
        drained &= world::next_phase(&ledger).await.is_ok();
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let ran = Ran {
            settled,
            elapsed_ms,
            denied: tally.counted(),
        };
        let finals = self.read_back(&room, screened).await;
        drained &= world::next_phase(&ledger).await.is_ok();
        let mut record = self.close(scratch, room, &ledger, ready.copies, finals, drained);
        record.admitted_digest = ready.admitted_digest;
        record.ran(&ran, screened)
    }

    /// Copy the observed world into the room, then admit the candidate from its bytes alone over
    /// the empty admission directory and resolve its rehearsal plan.
    async fn prepare(
        &self,
        room: &RootedFs,
        admit_dir: &OwnedDir,
        scratch: &Scratch,
        screened: &Screened,
    ) -> Result<Ready, (Refused, Vec<CopyReceipt>)> {
        let originals = world::originals(&self.project)
            .await
            .map_err(|refused| (refused, Vec::new()))?;
        let result = world::copy_in(&originals, room, &screened.inputs).await;
        // Every read of the originals is joined before anything else happens.
        let _ = world::next_phase(originals.ledger()).await;
        drop(originals);
        let copies = result.map_err(|refused| (refused, Vec::new()))?;
        match self.admit(admit_dir, scratch) {
            Ok((admitted_digest, driver, plan)) => Ok(Ready {
                copies,
                admitted_digest,
                driver,
                plan,
            }),
            Err(refused) => Err((refused, copies)),
        }
    }

    /// Admit the candidate's own bytes through the One Door, bind the driver to them, and
    /// resolve the plan a rehearsal runs under.
    fn admit(
        &self,
        admit_dir: &OwnedDir,
        scratch: &Scratch,
    ) -> Result<(String, ServiceExecutionDriver, ExecutionAccessPlan), Refused> {
        let refused = |why: String| Refused::new(Refusal::Admission, why);
        let service = ExecutionService::default();
        let admitted = service
            .admit_root_bytes(
                admit_dir,
                Path::new(ObservedRoom::LOGICAL_ROOT),
                self.candidate.as_bytes(),
            )
            .map_err(|error| {
                refused(format!("the admission door refused the candidate: {error}"))
            })?;
        let digest = admitted.snapshot().digest().to_owned();
        let session = service.begin(admitted);
        let driver = ServiceExecutionDriver::new(session.context(), scratch.room())
            .ok_or_else(|| refused("the admission door lost the candidate's root".to_owned()))?;
        if driver.root_source() != self.candidate {
            return Err(refused(
                "the admission door admitted other bytes than the candidate's".to_owned(),
            ));
        }
        let plan = driver
            .rehearsal_plan(None)
            .map_err(|refusal| Refused::new(Refusal::Plan, refusal.to_string()))?;
        Ok((digest, driver, plan))
    }

    /// Read back every output the candidate declares, then every target the caller names, once
    /// each, as the room spells them.
    async fn read_back(&self, room: &RootedFs, screened: &Screened) -> Vec<(String, FinalReceipt)> {
        let mut paths: Vec<(String, String)> = screened
            .outputs
            .iter()
            .map(|output| (output.path.clone(), output.at.clone()))
            .collect();
        for target in &self.targets {
            let Ok(at) = screen::room_path(target, "the target") else {
                continue;
            };
            if !paths.iter().any(|(_, known)| *known == at) {
                paths.push((target.clone(), at));
            }
        }
        let mut finals = Vec::with_capacity(paths.len());
        for (path, at) in paths {
            let state = world::final_state(room, &at).await;
            finals.push((at, FinalReceipt::new(path, state)));
        }
        finals
    }

    /// Close the room: its ledger's facts, then the scratch directory removed and verified gone.
    fn close(
        &self,
        scratch: Scratch,
        room: Arc<RootedFs>,
        ledger: &Arc<EffectLedger>,
        copies: Vec<CopyReceipt>,
        finals: Vec<(String, FinalReceipt)>,
        drained: bool,
    ) -> Record {
        let facts = world::facts(ledger, drained);
        drop(room);
        let cleaned = scratch.remove();
        let time_ms = u64::try_from(self.bound.as_millis()).unwrap_or(u64::MAX);
        Record {
            candidate_sha256: self.candidate_sha256.clone(),
            admitted_digest: String::new(),
            copies,
            finals,
            ledger: facts,
            cleaned,
            bounds: Bounds::new(
                time_ms,
                ObservedRoom::COPY_BOUND,
                ObservedRoom::PREVIEW_BOUND,
            ),
        }
    }
}
