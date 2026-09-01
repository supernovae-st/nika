//! Resident schedule planner/firer integration.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jiff::{Timestamp, Zoned};
use nika_cadence::{
    ArmGeneration, ScheduleDecision, ScheduleDecisionState, ScheduleDefinition, ScheduleDraft,
    ScheduleDueVerdict, ScheduleOrigin, SchedulePlanError, ScheduleRevision, ScheduleSlot,
    plan_schedule,
};
use sha2::{Digest as _, Sha256};
use tokio::sync::watch;
use tokio::task::JoinSet;

use crate::JobOrigin;
use crate::schedule::{ScheduleClaimEvidence, ScheduleSlotAction};

use super::{AuthorityState, PreparedScheduledRun, ServerError};

const FALLBACK_RESCAN: Duration = Duration::from_secs(5);
const PLANNER_PROJECTION: usize = 1;
const GENERATION_DOMAIN: &[u8] = b"nika/resident-schedule-generation@1\0";

struct ResidentSchedule {
    origin: ScheduleOrigin,
    definition: ScheduleDefinition,
}

/// A project beat whose declaration the planner refused at load. The
/// refusal is projected onto schedule status; the beat never rides the
/// scan loop as a live declaration that silently never fires.
#[derive(Debug, Clone)]
pub(super) struct RefusedProjectSchedule {
    definition: ScheduleDefinition,
    error: SchedulePlanError,
}

impl RefusedProjectSchedule {
    pub(super) fn definition(&self) -> &ScheduleDefinition {
        &self.definition
    }

    pub(super) fn error(&self) -> &SchedulePlanError {
        &self.error
    }
}

/// The load-time refusal projection, replaced wholesale by every scan.
pub(super) type ProjectRefusals = Arc<Mutex<BTreeMap<String, RefusedProjectSchedule>>>;

struct ProjectLoad {
    live: Vec<ResidentSchedule>,
    refused: Vec<RefusedProjectSchedule>,
}

struct FireCandidate {
    schedule: ResidentSchedule,
    slot: ScheduleSlot,
    decision: ScheduleDecision,
}

struct SkipCandidate {
    schedule: ResidentSchedule,
    slot: ScheduleSlot,
    decision: ScheduleDecision,
    reason: String,
}

enum PlannedAction {
    Fire(FireCandidate),
    Skip(SkipCandidate),
}

struct Scan {
    actions: Vec<PlannedAction>,
    earliest: Option<Timestamp>,
}

/// Drive both project and API declarations through one planner and firer.
pub(super) async fn run(
    state: Arc<AuthorityState>,
    mut stop: watch::Receiver<bool>,
) -> Result<(), ServerError> {
    let mut active = BTreeSet::new();
    let mut executions = JoinSet::new();
    loop {
        let scan_state = Arc::clone(&state);
        let scan = tokio::task::spawn_blocking(move || scan(&scan_state))
            .await
            .map_err(|_| ServerError::BlockingTask)??;
        for action in scan.actions {
            match action {
                PlannedAction::Fire(candidate) => {
                    let key = schedule_key(
                        candidate.schedule.origin,
                        candidate.schedule.definition.id(),
                    );
                    if active.contains(&key) {
                        handle_overlap(&state, candidate)?;
                        continue;
                    }
                    if let Some(prepared) = prepare_claim(&state, candidate).await? {
                        active.insert(key.clone());
                        executions.spawn_blocking(move || (key, prepared.execute()));
                    }
                }
                PlannedAction::Skip(candidate) => consume_skip(&state, candidate)?,
            }
        }
        let delay = wake_delay(scan.earliest, state.clock.now().timestamp());
        let sleep = state.clock.sleep(delay);
        tokio::pin!(sleep);
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
            }
            () = state.schedule_wake.notified() => {}
            () = &mut sleep => {}
            joined = executions.join_next(), if !executions.is_empty() => {
                let (key, result) = joined
                    .ok_or(ServerError::ExecutionTask)?
                    .map_err(|_| ServerError::ExecutionTask)?;
                active.remove(&key);
                result?;
                state.schedule_wake.notify_one();
            }
        }
    }
    while let Some(joined) = executions.join_next().await {
        let (_, result) = joined.map_err(|_| ServerError::ExecutionTask)?;
        result?;
    }
    Ok(())
}

fn scan(state: &AuthorityState) -> Result<Scan, ServerError> {
    let now = state.clock.now();
    let mut schedules = Vec::new();
    for definition in state.schedules.all()? {
        schedules.push(ResidentSchedule {
            origin: ScheduleOrigin::Api,
            definition,
        });
    }
    if let Some(project) = state.project.get() {
        match load_project(project, &now) {
            Ok(load) => {
                publish_refusals(state, load.refused);
                schedules.extend(load.live);
            }
            Err(ServerError::ScheduledAdmission) => publish_refusals(state, Vec::new()),
            Err(error) => return Err(error),
        }
    }
    let mut actions = Vec::new();
    let mut earliest = None;
    for schedule in schedules {
        let prior = state
            .schedules
            .decision_state(schedule.origin, schedule.definition.id())?;
        let Ok(plan) = plan_schedule(&schedule.definition, &now, &prior, PLANNER_PROJECTION) else {
            continue;
        };
        earliest = earlier(earliest, plan.earliest_wake_hint());
        match plan.due() {
            ScheduleDueVerdict::ScheduledOnTime { slot } => {
                actions.push(PlannedAction::Fire(FireCandidate {
                    schedule,
                    slot: slot.clone(),
                    decision: ScheduleDecision::Scheduled,
                }));
            }
            ScheduleDueVerdict::CatchUp { slot, .. } => {
                actions.push(PlannedAction::Fire(FireCandidate {
                    schedule,
                    slot: slot.clone(),
                    decision: ScheduleDecision::CatchUp,
                }));
            }
            ScheduleDueVerdict::SkippedMissed { slot, .. } => {
                actions.push(PlannedAction::Skip(SkipCandidate {
                    schedule,
                    slot: slot.clone(),
                    decision: ScheduleDecision::CatchUp,
                    reason: "missed_policy".to_owned(),
                }));
            }
            ScheduleDueVerdict::SkippedTooLate { slot, .. } => {
                actions.push(PlannedAction::Skip(SkipCandidate {
                    schedule,
                    slot: slot.clone(),
                    decision: ScheduleDecision::CatchUp,
                    reason: "max_lateness".to_owned(),
                }));
            }
            _ => {}
        }
    }
    Ok(Scan { actions, earliest })
}

async fn prepare_claim(
    state: &Arc<AuthorityState>,
    candidate: FireCandidate,
) -> Result<Option<PreparedScheduledRun>, ServerError> {
    let Some(project) = state.project.get().cloned() else {
        return Ok(None);
    };
    let service = state.service;
    let coordinator = state.coordinator.clone();
    let store = Arc::clone(&state.schedules);
    let clock = Arc::clone(&state.clock);
    tokio::task::spawn_blocking(move || {
        if candidate.schedule.origin == ScheduleOrigin::Project
            && !project_revision_current(
                &project,
                candidate.schedule.definition.id(),
                &candidate.schedule.definition.revision(),
            )?
        {
            return Ok(None);
        }
        let admitted = service
            .admit(
                &project,
                Path::new(candidate.schedule.definition.workflow()),
            )
            .map_err(|_| ServerError::ScheduledAdmission)?;
        let generation = generation(&candidate.schedule.definition, &admitted)?;
        let fired_at = clock.now();
        let origin = JobOrigin::schedule(
            candidate.schedule.origin,
            candidate.schedule.definition.id(),
            &candidate.schedule.definition.revision(),
            candidate.slot.id(),
            candidate.decision,
            candidate.slot.scheduled_for(),
            fired_at.timestamp(),
            &generation,
        )?;
        let prepared = coordinator.prepare_scheduled_with_max_cost(
            admitted,
            origin,
            Some(candidate.schedule.definition.max_cost_usd()),
        )?;
        let claim_now = clock.now();
        if !fire_is_still_due(&store, &candidate, &claim_now)? {
            return Ok(None);
        }
        let claim = ScheduleClaimEvidence::new(
            prepared.run_id().as_str().to_owned(),
            prepared
                .execution_id()
                .ok_or(ServerError::ScheduledAdmission)?
                .to_owned(),
            prepared
                .trace_id()
                .ok_or(ServerError::ScheduledAdmission)?
                .to_owned(),
            generation,
        );
        let consumed = store.consume_claimed_slot(
            candidate.schedule.origin,
            &candidate.schedule.definition,
            &candidate.slot,
            candidate.decision,
            claim_now.timestamp(),
            claim,
        )?;
        Ok(consumed.then_some(prepared))
    })
    .await
    .map_err(|_| ServerError::BlockingTask)?
}

fn consume_skip(state: &AuthorityState, candidate: SkipCandidate) -> Result<(), ServerError> {
    if candidate.schedule.origin == ScheduleOrigin::Project
        && let Some(project) = state.project.get()
        && !project_revision_current(
            project,
            candidate.schedule.definition.id(),
            &candidate.schedule.definition.revision(),
        )?
    {
        return Ok(());
    }
    let now = state.clock.now();
    if !skip_is_still_due(&state.schedules, &candidate, &now)? {
        return Ok(());
    }
    state.schedules.consume_slot(
        candidate.schedule.origin,
        &candidate.schedule.definition,
        &candidate.slot,
        candidate.decision,
        ScheduleSlotAction::Skipped,
        now.timestamp(),
        Some(candidate.reason),
    )?;
    Ok(())
}

fn fire_is_still_due(
    store: &crate::ScheduleStore,
    candidate: &FireCandidate,
    now: &Zoned,
) -> Result<bool, ServerError> {
    let prior = store.decision_state(
        candidate.schedule.origin,
        candidate.schedule.definition.id(),
    )?;
    let Ok(plan) = plan_schedule(
        &candidate.schedule.definition,
        now,
        &prior,
        PLANNER_PROJECTION,
    ) else {
        return Ok(false);
    };
    let due = match plan.due() {
        ScheduleDueVerdict::ScheduledOnTime { slot } => Some((slot, ScheduleDecision::Scheduled)),
        ScheduleDueVerdict::CatchUp { slot, .. } => Some((slot, ScheduleDecision::CatchUp)),
        _ => None,
    };
    Ok(due.is_some_and(|(slot, decision)| {
        slot.id() == candidate.slot.id() && decision == candidate.decision
    }))
}

fn skip_is_still_due(
    store: &crate::ScheduleStore,
    candidate: &SkipCandidate,
    now: &Zoned,
) -> Result<bool, ServerError> {
    let prior = store.decision_state(
        candidate.schedule.origin,
        candidate.schedule.definition.id(),
    )?;
    let Ok(plan) = plan_schedule(
        &candidate.schedule.definition,
        now,
        &prior,
        PLANNER_PROJECTION,
    ) else {
        return Ok(false);
    };
    let slot = match plan.due() {
        ScheduleDueVerdict::SkippedMissed { slot, .. } if candidate.reason == "missed_policy" => {
            Some(slot)
        }
        ScheduleDueVerdict::SkippedTooLate { slot, .. } if candidate.reason == "max_lateness" => {
            Some(slot)
        }
        _ => None,
    };
    Ok(slot.is_some_and(|slot| slot.id() == candidate.slot.id()))
}

fn handle_overlap(state: &AuthorityState, candidate: FireCandidate) -> Result<(), ServerError> {
    match candidate.schedule.definition.overlap() {
        nika_cadence::Overlap::Sauter => consume_skip(
            state,
            SkipCandidate {
                schedule: candidate.schedule,
                slot: candidate.slot,
                decision: candidate.decision,
                reason: "overlap".to_owned(),
            },
        ),
        _ => Ok(()),
    }
}

fn generation(
    definition: &ScheduleDefinition,
    admitted: &nika_execution::AdmittedExecution,
) -> Result<ArmGeneration, ServerError> {
    let unit = admitted
        .snapshot()
        .unit(admitted.snapshot().root())
        .ok_or(ServerError::ScheduledAdmission)?;
    let mut hasher = Sha256::new();
    hasher.update(GENERATION_DOMAIN);
    hasher.update(definition.revision().as_str().as_bytes());
    hasher.update([0]);
    hasher.update(unit.bytes());
    ArmGeneration::from_wire(&format!("{:x}", hasher.finalize()))
        .ok_or(ServerError::ScheduledAdmission)
}

fn publish_refusals(state: &AuthorityState, refused: Vec<RefusedProjectSchedule>) {
    let mut projection = state
        .project_refusals
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    projection.clear();
    for refusal in refused {
        projection.insert(refusal.definition.id().to_owned(), refusal);
    }
}

fn load_project(project: &nika_fs::OwnedDir, now: &Zoned) -> Result<ProjectLoad, ServerError> {
    let mut live = Vec::new();
    let mut refused = Vec::new();
    for definition in load_project_definitions(project)? {
        match plan_schedule(
            &definition,
            now,
            &ScheduleDecisionState::empty(),
            PLANNER_PROJECTION,
        ) {
            Ok(_) => live.push(ResidentSchedule {
                origin: ScheduleOrigin::Project,
                definition,
            }),
            Err(error) => refused.push(RefusedProjectSchedule { definition, error }),
        }
    }
    Ok(ProjectLoad { live, refused })
}

fn load_project_definitions(
    project: &nika_fs::OwnedDir,
) -> Result<Vec<ScheduleDefinition>, ServerError> {
    let Some(text) = project
        .read_optional(nika_vocab::project::FILE_NAME)
        .map_err(|error| ServerError::WorkflowRoot(error.kind()))?
    else {
        return Ok(Vec::new());
    };
    let shape = nika_vocab::project::parse(&text).map_err(|_| ServerError::ScheduledAdmission)?;
    let registry =
        nika_cadence::parse_registry(&text).map_err(|_| ServerError::ScheduledAdmission)?;
    if shape.arm().len() != registry.beat_count()
        || nika_cadence::validate(&registry).next().is_some()
    {
        return Err(ServerError::ScheduledAdmission);
    }
    let labels = nika_cadence::emit::labels(&registry);
    registry
        .beats()
        .zip(labels)
        .map(|(beat, id)| {
            ScheduleDraft::from_project(id, beat)
                .and_then(ScheduleDraft::validate)
                .map_err(|_| ServerError::ScheduledAdmission)
        })
        .collect()
}

fn project_revision_current(
    project: &nika_fs::OwnedDir,
    id: &str,
    revision: &ScheduleRevision,
) -> Result<bool, ServerError> {
    Ok(load_project_definitions(project)?
        .into_iter()
        .any(|definition| definition.id() == id && definition.revision() == *revision))
}

fn schedule_key(origin: ScheduleOrigin, id: &str) -> String {
    origin.key(id)
}

fn earlier(left: Option<Timestamp>, right: Option<Timestamp>) -> Option<Timestamp> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(left), None) => Some(left),
        (None, right) => right,
    }
}

fn wake_delay(earliest: Option<Timestamp>, now: Timestamp) -> Duration {
    let Some(earliest) = earliest else {
        return FALLBACK_RESCAN;
    };
    let milliseconds = earliest
        .as_millisecond()
        .saturating_sub(now.as_millisecond());
    Duration::from_millis(u64::try_from(milliseconds).unwrap_or_default()).min(FALLBACK_RESCAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lost_wake_is_bounded_by_fallback_rescan() {
        assert_eq!(wake_delay(None, Timestamp::MIN), FALLBACK_RESCAN);
    }

    #[test]
    fn project_and_api_ids_never_share_an_overlap_key() {
        assert_ne!(
            schedule_key(ScheduleOrigin::Api, "daily"),
            schedule_key(ScheduleOrigin::Project, "daily")
        );
    }

    #[test]
    fn preclaim_recomputation_refuses_a_clock_rollback() {
        let root = tempfile::tempdir().expect("state root");
        let store = crate::ScheduleStore::open(root.path()).expect("store");
        let draft = ScheduleDraft::new(
            "rollback".to_owned(),
            "root.nika.yaml".to_owned(),
            nika_cadence::ScheduleWhenDraft::Once {
                at: "2026-09-01T09:00:00Z".to_owned(),
            },
            0.25,
            nika_cadence::MissPolicy::RattraperUneFois,
        );
        let definition = match store
            .apply(draft, crate::ScheduleApplyPrecondition::Create)
            .expect("create")
        {
            crate::ScheduleApplyOutcome::Created(definition) => Some(definition),
            _ => None,
        }
        .expect("created definition");
        let due_now: Zoned = "2026-09-01T09:00:00Z[UTC]".parse().expect("due now");
        let plan = plan_schedule(
            &definition,
            &due_now,
            &nika_cadence::ScheduleDecisionState::empty(),
            1,
        )
        .expect("plan");
        let slot = match plan.due() {
            ScheduleDueVerdict::ScheduledOnTime { slot } => Some(slot.clone()),
            _ => None,
        }
        .expect("due slot");
        let candidate = FireCandidate {
            schedule: ResidentSchedule {
                origin: ScheduleOrigin::Api,
                definition,
            },
            slot,
            decision: ScheduleDecision::Scheduled,
        };
        assert!(fire_is_still_due(&store, &candidate, &due_now).expect("still due"));
        let rolled_back: Zoned = "2026-09-01T08:59:59Z[UTC]".parse().expect("rolled back");
        assert!(!fire_is_still_due(&store, &candidate, &rolled_back).expect("recomputed refusal"));
    }
}
