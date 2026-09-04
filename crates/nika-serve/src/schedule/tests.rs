use std::sync::Arc;

use jiff::{Timestamp, Zoned};
use nika_cadence::{
    MissPolicy, ScheduleDecision, ScheduleDecisionState, ScheduleDraft, ScheduleDueVerdict,
    ScheduleFindingKind, ScheduleOrigin, ScheduleRevision, ScheduleWhenDraft, parse_registry,
    plan_schedule,
};
use nika_error::NikaErrorCode;

use super::*;

fn draft(id: impl Into<String>, workflow: impl Into<String>) -> ScheduleDraft {
    let registry = parse_registry(
        "nika: store-tests\narm:\n  - workflow: base.nika.yaml\n    cadence: on-webhook\n    plafond: 0.25\n    manqué: rattraper-une-fois\n",
    )
    .expect("registry");
    let beat = registry.beats().next().expect("beat");
    let mut draft = ScheduleDraft::from_project("base", beat).expect("draft");
    draft.id = id.into();
    draft.workflow = workflow.into();
    draft.when = ScheduleWhenDraft::Webhook;
    draft.max_lateness_seconds = Some(3_600);
    draft
}

fn created_revision(outcome: ScheduleApplyOutcome) -> Option<ScheduleRevision> {
    match outcome {
        ScheduleApplyOutcome::Created(definition) => Some(definition.revision()),
        _ => None,
    }
}

fn created_definition(outcome: ScheduleApplyOutcome) -> Option<nika_cadence::ScheduleDefinition> {
    match outcome {
        ScheduleApplyOutcome::Created(definition) => Some(definition),
        _ => None,
    }
}

fn once_draft(id: &str, workflow: &str) -> ScheduleDraft {
    ScheduleDraft::new(
        id.to_owned(),
        workflow.to_owned(),
        ScheduleWhenDraft::Once {
            at: "2026-09-01T09:00:00Z".to_owned(),
        },
        0.25,
        MissPolicy::RattraperUneFois,
    )
}

fn due_once(definition: &nika_cadence::ScheduleDefinition) -> nika_cadence::ScheduleSlot {
    let now: Zoned = "2026-09-01T09:00:01Z[UTC]".parse().expect("now");
    let plan =
        plan_schedule(definition, &now, &ScheduleDecisionState::empty(), 1).expect("once plan");
    match plan.due() {
        ScheduleDueVerdict::ScheduledOnTime { slot } | ScheduleDueVerdict::CatchUp { slot, .. } => {
            Some(slot.clone())
        }
        _ => None,
    }
    .expect("due once slot")
}

/// ADR-132 · #1352 · the schedule store carries the writer's stamp and
/// refuses a newer protocol's state.
#[test]
fn the_schedule_store_is_stamped_and_refuses_a_newer_writer() {
    let root = tempfile::tempdir().expect("root");
    drop(ScheduleStore::open(root.path()).expect("store"));
    let path = root.path().join("schedules/state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
    let mine = crate::writer::WriterStamp::this_engine();
    assert_eq!(
        state["writer"]["engine_version"], mine.engine_version,
        "{state}"
    );
    state["writer"]["machine_protocol_version"] =
        serde_json::json!(mine.machine_protocol_version + 1);
    std::fs::write(&path, format!("{state}\n")).expect("write newer");
    let refused = ScheduleStore::open(root.path()).expect_err("refused");
    assert!(
        matches!(refused, ScheduleStoreError::WrittenByNewerEngine(_)),
        "{refused}"
    );
}

#[test]
fn same_create_apply_twice_is_unchanged() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");

    let first = store
        .apply(
            draft("daily", "daily.nika.yaml"),
            ScheduleApplyPrecondition::Create,
        )
        .expect("create");
    let second = store
        .apply(
            draft("daily", "daily.nika.yaml"),
            ScheduleApplyPrecondition::Create,
        )
        .expect("retry");

    assert!(matches!(first, ScheduleApplyOutcome::Created(_)));
    assert!(matches!(second, ScheduleApplyOutcome::Unchanged(_)));
}

#[test]
fn equivalent_cadence_forms_are_one_normalized_spec() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");
    let mut readable = draft("weekly", "weekly.nika.yaml");
    readable.when = ScheduleWhenDraft::Cadence {
        expression: "TZ=Europe/Paris lundi 9h00".to_owned(),
    };
    let mut cron = draft("weekly", "weekly.nika.yaml");
    cron.when = ScheduleWhenDraft::Cadence {
        expression: " TZ=Europe/Paris  0 9 * * 1 ".to_owned(),
    };

    let first = store
        .apply(readable, ScheduleApplyPrecondition::Create)
        .expect("create");
    let second = store
        .apply(cron, ScheduleApplyPrecondition::Create)
        .expect("normalized retry");

    assert!(matches!(first, ScheduleApplyOutcome::Created(_)));
    assert!(matches!(second, ScheduleApplyOutcome::Unchanged(_)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_revision_a_updates_have_one_winner() {
    let root = tempfile::tempdir().expect("root");
    let first_store = Arc::new(ScheduleStore::open(root.path()).expect("first store"));
    let second_store = Arc::new(ScheduleStore::open(root.path()).expect("second store"));
    let revision = created_revision(
        first_store
            .apply(
                draft("daily", "a.nika.yaml"),
                ScheduleApplyPrecondition::Create,
            )
            .expect("create"),
    )
    .expect("created outcome");
    let barrier = Arc::new(tokio::sync::Barrier::new(3));

    let handles =
        [(first_store, "b.nika.yaml"), (second_store, "c.nika.yaml")].map(|(store, workflow)| {
            let barrier = Arc::clone(&barrier);
            let revision = revision.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                store
                    .apply(
                        draft("daily", workflow),
                        ScheduleApplyPrecondition::Revision(revision),
                    )
                    .expect("apply")
            })
        });
    barrier.wait().await;
    let mut outcomes = Vec::new();
    for handle in handles {
        outcomes.push(handle.await.expect("worker"));
    }

    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ScheduleApplyOutcome::Updated(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ScheduleApplyOutcome::Conflict { .. }))
            .count(),
        1
    );
}

#[test]
fn lost_update_response_retries_as_unchanged() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");
    let revision = created_revision(
        store
            .apply(
                draft("daily", "a.nika.yaml"),
                ScheduleApplyPrecondition::Create,
            )
            .expect("create"),
    )
    .expect("created outcome");

    let updated = store
        .apply(
            draft("daily", "b.nika.yaml"),
            ScheduleApplyPrecondition::Revision(revision.clone()),
        )
        .expect("update");
    let retried = store
        .apply(
            draft("daily", "b.nika.yaml"),
            ScheduleApplyPrecondition::Revision(revision),
        )
        .expect("retry");

    assert!(matches!(updated, ScheduleApplyOutcome::Updated(_)));
    assert!(matches!(retried, ScheduleApplyOutcome::Unchanged(_)));
}

#[test]
fn restart_recovers_and_replays() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");
    store
        .apply(
            draft("daily", "daily.nika.yaml"),
            ScheduleApplyPrecondition::Create,
        )
        .expect("create");
    drop(store);

    let reopened = ScheduleStore::open(root.path()).expect("reopen");
    let outcome = reopened
        .apply(
            draft("daily", "daily.nika.yaml"),
            ScheduleApplyPrecondition::Create,
        )
        .expect("replay");
    assert!(matches!(outcome, ScheduleApplyOutcome::Unchanged(_)));
}

#[test]
fn consumed_once_and_origin_collision_survive_restart() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");
    let api = created_definition(
        store
            .apply(
                once_draft("same", "base.nika.yaml"),
                ScheduleApplyPrecondition::Create,
            )
            .expect("create"),
    )
    .expect("created definition");
    let slot = due_once(&api);
    let decided_at: Timestamp = "2026-09-01T09:00:01Z".parse().expect("decision time");
    assert!(
        store
            .consume_slot(
                ScheduleOrigin::Api,
                &api,
                &slot,
                ScheduleDecision::CatchUp,
                ScheduleSlotAction::Claimed,
                decided_at,
                None,
            )
            .expect("api claim")
    );
    assert!(
        store
            .consume_slot(
                ScheduleOrigin::Project,
                &api,
                &slot,
                ScheduleDecision::Scheduled,
                ScheduleSlotAction::Skipped,
                decided_at,
                Some("project collision".to_owned()),
            )
            .expect("project skip")
    );
    drop(store);

    let reopened = ScheduleStore::open(root.path()).expect("reopen");
    let api_state = reopened
        .decision_state(ScheduleOrigin::Api, "same")
        .expect("api state");
    let project = reopened
        .last_decision(ScheduleOrigin::Project, "same")
        .expect("project state")
        .expect("project decision");
    assert_eq!(project.action(), ScheduleSlotAction::Skipped);
    assert_eq!(project.reason(), Some("project collision"));
    let now: Zoned = "2026-09-01T08:59:00Z[UTC]"
        .parse()
        .expect("rolled-back clock");
    let replay = plan_schedule(&api, &now, &api_state, 1).expect("replay plan");
    assert!(matches!(
        replay.due(),
        ScheduleDueVerdict::OnceConsumed { .. }
    ));
}

#[test]
fn mutation_immediately_before_claim_invalidates_stale_candidate() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");
    let old = created_definition(
        store
            .apply(
                once_draft("near-fire", "base.nika.yaml"),
                ScheduleApplyPrecondition::Create,
            )
            .expect("create"),
    )
    .expect("created definition");
    let slot = due_once(&old);
    let updated = once_draft("near-fire", "changed.nika.yaml");
    assert!(matches!(
        store
            .apply(updated, ScheduleApplyPrecondition::Revision(old.revision()))
            .expect("update"),
        ScheduleApplyOutcome::Updated(_)
    ));
    let decided_at: Timestamp = "2026-09-01T09:00:01Z".parse().expect("decision time");
    assert!(
        !store
            .consume_slot(
                ScheduleOrigin::Api,
                &old,
                &slot,
                ScheduleDecision::CatchUp,
                ScheduleSlotAction::Claimed,
                decided_at,
                None,
            )
            .expect("stale claim")
    );
    assert!(
        store
            .last_decision(ScheduleOrigin::Api, "near-fire")
            .expect("last decision")
            .is_none()
    );
}

#[test]
fn durable_decisions_are_monotone_and_have_a_fixed_count_ceiling() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");
    let decided_at: Timestamp = "2026-09-01T10:00:01Z".parse().expect("decision time");
    let newer = once_draft("monotone", "base.nika.yaml")
        .validate()
        .expect("newer definition");
    let newer_slot = due_once(&newer);
    assert!(
        store
            .consume_slot(
                ScheduleOrigin::Project,
                &newer,
                &newer_slot,
                ScheduleDecision::Scheduled,
                ScheduleSlotAction::Claimed,
                decided_at,
                None,
            )
            .expect("newer claim")
    );
    let mut older_draft = once_draft("monotone", "base.nika.yaml");
    older_draft.when = ScheduleWhenDraft::Once {
        at: "2026-08-01T09:00:00Z".to_owned(),
    };
    let older = older_draft.validate().expect("older definition");
    let older_now: Zoned = "2026-08-01T09:00:01Z[UTC]".parse().expect("older now");
    let older_plan =
        plan_schedule(&older, &older_now, &ScheduleDecisionState::empty(), 1).expect("older plan");
    let older_slot = match older_plan.due() {
        ScheduleDueVerdict::ScheduledOnTime { slot } | ScheduleDueVerdict::CatchUp { slot, .. } => {
            Some(slot.clone())
        }
        _ => None,
    }
    .expect("older slot");
    assert!(
        !store
            .consume_slot(
                ScheduleOrigin::Project,
                &older,
                &older_slot,
                ScheduleDecision::Scheduled,
                ScheduleSlotAction::Claimed,
                decided_at,
                None,
            )
            .expect("monotone refusal")
    );

    for index in 1..super::model::MAX_DURABLE_SCHEDULE_DECISIONS {
        let definition = once_draft(&format!("bounded-{index}"), "base.nika.yaml")
            .validate()
            .expect("bounded definition");
        let slot = due_once(&definition);
        assert!(
            store
                .consume_slot(
                    ScheduleOrigin::Project,
                    &definition,
                    &slot,
                    ScheduleDecision::Scheduled,
                    ScheduleSlotAction::Claimed,
                    decided_at,
                    None,
                )
                .expect("within decision bound")
        );
    }
    let excess = once_draft("one-decision-too-many", "base.nika.yaml")
        .validate()
        .expect("excess definition");
    let excess_slot = due_once(&excess);
    assert!(matches!(
        store.consume_slot(
            ScheduleOrigin::Project,
            &excess,
            &excess_slot,
            ScheduleDecision::Scheduled,
            ScheduleSlotAction::Claimed,
            decided_at,
            None,
        ),
        Err(ScheduleStoreError::DecisionLimit { maximum })
            if maximum == super::model::MAX_DURABLE_SCHEDULE_DECISIONS
    ));
}

#[test]
fn unknown_persisted_decision_enum_refuses_recovery() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");
    let definition = once_draft("closed-decision", "base.nika.yaml")
        .validate()
        .expect("definition");
    let slot = due_once(&definition);
    let decided_at: Timestamp = "2026-09-01T09:00:01Z".parse().expect("decision time");
    store
        .consume_slot(
            ScheduleOrigin::Project,
            &definition,
            &slot,
            ScheduleDecision::Scheduled,
            ScheduleSlotAction::Claimed,
            decided_at,
            None,
        )
        .expect("decision");
    drop(store);
    let path = root.path().join("schedules/state.json");
    let state = std::fs::read_to_string(&path).expect("state");
    let forged = state.replace("\"action\":\"claimed\"", "\"action\":\"future\"");
    assert_ne!(forged, state, "test must alter the closed enum");
    std::fs::write(path, forged).expect("forged state");
    assert!(matches!(
        ScheduleStore::open(root.path()),
        Err(ScheduleStoreError::Corrupt(_))
    ));
}

#[test]
fn corrupt_snapshot_refuses_reopen() {
    let root = tempfile::tempdir().expect("root");
    drop(ScheduleStore::open(root.path()).expect("store"));
    std::fs::write(root.path().join("schedules/state.json"), b"{broken\n").expect("corrupt");

    assert!(matches!(
        ScheduleStore::open(root.path()),
        Err(ScheduleStoreError::Corrupt(_))
    ));
}

#[cfg(unix)]
#[test]
fn symlinked_schedule_directory_cannot_escape() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::tempdir().expect("outside");
    symlink(outside.path(), root.path().join("schedules")).expect("link");

    assert!(matches!(
        ScheduleStore::open(root.path()),
        Err(ScheduleStoreError::Io(_))
    ));
    assert!(!outside.path().join("state.json").exists());
}

#[cfg(unix)]
#[test]
fn symlinked_state_file_cannot_redirect_recovery() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::NamedTempFile::new().expect("outside");
    drop(ScheduleStore::open(root.path()).expect("store"));
    let state = root.path().join("schedules/state.json");
    std::fs::remove_file(&state).expect("remove state");
    symlink(outside.path(), &state).expect("link state");

    assert!(matches!(
        ScheduleStore::open(root.path()),
        Err(ScheduleStoreError::Io(_))
    ));
    assert_eq!(std::fs::read(outside.path()).expect("outside bytes"), b"");
}

#[test]
fn oversized_id_spec_and_count_refuse_without_mutation() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");
    let bad_id = store
        .apply(
            draft("x".repeat(256), "x.nika.yaml"),
            ScheduleApplyPrecondition::Create,
        )
        .expect_err("id");
    assert!(matches!(
        bad_id,
        ScheduleStoreError::InvalidSchedule(ref finding)
            if finding.kind() == ScheduleFindingKind::Id
    ));

    let mut oversized = draft("oversized", format!("{}.nika.yaml", "w".repeat(1014)));
    oversized.active = Some(false);
    oversized.pause_reason = Some("\0".repeat(1_024));
    oversized.pause_until = Some("2026-12-31".to_owned());
    assert!(matches!(
        store.apply(oversized, ScheduleApplyPrecondition::Create),
        Err(ScheduleStoreError::ScheduleTooLarge { .. })
    ));

    for index in 0..MAX_API_SCHEDULES {
        store
            .apply(
                draft(format!("item-{index}"), "x.nika.yaml"),
                ScheduleApplyPrecondition::Create,
            )
            .expect("within count");
    }
    assert!(matches!(
        store.apply(draft("one-too-many", "x.nika.yaml"), ScheduleApplyPrecondition::Create),
        Err(ScheduleStoreError::ScheduleLimit { maximum }) if maximum == MAX_API_SCHEDULES
    ));
}

#[test]
fn non_finite_and_non_positive_costs_refuse_through_canonical_validation() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");
    for cost in [f64::NAN, -1.0, f64::INFINITY] {
        let mut candidate = draft("cost", "x.nika.yaml");
        candidate.max_cost_usd = cost;
        assert!(matches!(
            store.apply(candidate, ScheduleApplyPrecondition::Create),
            Err(ScheduleStoreError::InvalidSchedule(ref finding))
                if finding.kind() == ScheduleFindingKind::Cost
        ));
    }
}

#[test]
fn representation_is_deterministic_across_apply_order() {
    let left = tempfile::tempdir().expect("left");
    let right = tempfile::tempdir().expect("right");
    let left_store = ScheduleStore::open(left.path()).expect("left store");
    let right_store = ScheduleStore::open(right.path()).expect("right store");
    for candidate in [draft("b", "b.nika.yaml"), draft("a", "a.nika.yaml")] {
        left_store
            .apply(candidate, ScheduleApplyPrecondition::Create)
            .expect("left apply");
    }
    for candidate in [draft("a", "a.nika.yaml"), draft("b", "b.nika.yaml")] {
        right_store
            .apply(candidate, ScheduleApplyPrecondition::Create)
            .expect("right apply");
    }

    let left_bytes = std::fs::read(left.path().join("schedules/state.json")).expect("left bytes");
    let right_bytes =
        std::fs::read(right.path().join("schedules/state.json")).expect("right bytes");
    assert_eq!(left_bytes, right_bytes);
}

#[test]
fn stale_and_absent_update_revisions_cannot_overwrite() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");
    let absent = store
        .apply(
            draft("missing", "x.nika.yaml"),
            ScheduleApplyPrecondition::Revision(
                ScheduleRevision::from_wire(&format!("sha256:{}", "0".repeat(64)))
                    .expect("revision"),
            ),
        )
        .expect("absent verdict");
    assert!(matches!(
        absent,
        ScheduleApplyOutcome::Conflict { current: None }
    ));

    let original = created_revision(
        store
            .apply(
                draft("daily", "a.nika.yaml"),
                ScheduleApplyPrecondition::Create,
            )
            .expect("create"),
    )
    .expect("created outcome");
    let changed = store
        .apply(
            draft("daily", "b.nika.yaml"),
            ScheduleApplyPrecondition::Revision(original.clone()),
        )
        .expect("change");
    assert!(matches!(changed, ScheduleApplyOutcome::Updated(_)));
    let conflict = store
        .apply(
            draft("daily", "c.nika.yaml"),
            ScheduleApplyPrecondition::Revision(original),
        )
        .expect("conflict");
    assert!(matches!(
        conflict,
        ScheduleApplyOutcome::Conflict { current: Some(_) }
    ));
}

#[test]
fn persisted_schema_refuses_secret_fields() {
    let root = tempfile::tempdir().expect("root");
    let store = ScheduleStore::open(root.path()).expect("store");
    store
        .apply(
            draft("daily", "daily.nika.yaml"),
            ScheduleApplyPrecondition::Create,
        )
        .expect("create");
    drop(store);
    let path = root.path().join("schedules/state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
    state["schedules"][0]["bearer_token"] = serde_json::json!("secret");
    std::fs::write(&path, serde_json::to_vec(&state).expect("encode")).expect("write");

    assert!(matches!(
        ScheduleStore::open(root.path()),
        Err(ScheduleStoreError::Corrupt(_))
    ));
}

#[test]
fn store_failures_speak_the_schedule_registry_code() {
    assert_eq!(
        ScheduleStoreError::LockPoisoned.nika_code(),
        nika_error::codes::NIKA_018
    );
}
