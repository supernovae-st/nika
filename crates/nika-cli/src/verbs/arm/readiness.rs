// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The CLI join of the schedule readiness receipt: capture the same world
//! a fire admits, let the service driver judge the program
//! (`scheduled_program`) and `nika-arm` the schedule, the evidence and the
//! status. Read-only, and never a transferable activation or monetary
//! authority certificate.

use std::path::Path;

use nika_arm::readiness::{
    Blocker, ProgramFacts, ProjectBinding, ReceiptIdentity, ScheduleReadinessReceipt,
};
use nika_cadence::Beat;
use nika_cadence::registry::ArmRegistry;

/// The receipt of `registry`'s beat `index`, declared in `project_file`.
pub(super) fn receipt<'a>(
    registry: &'a ArmRegistry,
    index: usize,
    label: &str,
    project_file: &Path,
    now: &jiff::Zoned,
) -> Option<ScheduleReadinessReceipt<'a>> {
    let root = project_file.parent().unwrap_or_else(|| Path::new("."));
    let beat = registry.beats().nth(index)?;
    let binding = ProjectBinding::new(
        nika_runtime::project_root_fingerprint(root),
        project_file.display().to_string(),
        label.to_owned(),
    );
    let (identity, program) = program(root, beat, binding);
    let evidence = super::state::ArmState::at_project(root).inspect(label, &now.timestamp());
    ScheduleReadinessReceipt::assemble(registry, index, now, identity, program, evidence)
}

fn program(root: &Path, beat: &Beat, binding: ProjectBinding) -> (ReceiptIdentity, ProgramFacts) {
    let unavailable = |kind: &str, message: String| {
        let identity = ReceiptIdentity::new(None, None, None, binding.clone());
        (identity, ProgramFacts::unavailable(kind, message))
    };
    let project = match nika_fs::OwnedDir::open(root) {
        Ok(project) => project,
        Err(error) => return unavailable("project_unavailable", error.to_string()),
    };
    let service = nika_execution::ExecutionService::default();
    let admitted = match service.admit(&project, Path::new(&beat.workflow)) {
        Ok(admitted) => admitted,
        Err(error) => return unavailable("workflow_unavailable", error.to_string()),
    };
    let snapshot = admitted.snapshot();
    let identity = ReceiptIdentity::new(
        snapshot
            .unit(snapshot.root())
            .map(|unit| unit.digest().to_owned()),
        Some(snapshot.digest().to_owned()),
        Some(nika_cadence::ArmGeneration::compute(
            beat,
            snapshot.digest(),
        )),
        binding,
    );
    // The registry law requires `plafond`; its absence is never a zero.
    let Some(ceiling) = beat.plafond else {
        let message = "plafond absent after validation".to_owned();
        return (
            identity,
            ProgramFacts::unavailable("plafond_absent", message),
        );
    };
    let pairs: Vec<String> = beat.input_vars().collect();
    let config = nika_runtime::compose::config_from_env();
    let judged = nika_service_execution::run_cost::scheduled_program(
        admitted.workflow(),
        admitted.check(),
        &config,
        &pairs,
        ceiling,
    );
    let blockers = judged
        .blockers
        .into_iter()
        .map(|blocker| {
            Blocker::new(
                blocker.kind,
                blocker.code.map(str::to_owned),
                blocker.subject,
                blocker.message,
            )
            .with_reason(blocker.reason.map(str::to_owned))
        })
        .collect();
    let program = ProgramFacts::new(
        true,
        Some(judged.required_inputs_ready),
        judged.model_cost_ready,
        blockers,
        judged.unknowns,
        judged.document,
    );
    (identity, program)
}
