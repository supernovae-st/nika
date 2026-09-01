// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

use jiff::{Timestamp, Zoned};
use nika_error::NikaErrorCode;

use super::*;
use crate::{
    AfterSkip, MissPolicy, Overlap, ScheduleDraft, ScheduleJitter, ScheduleWhenDraft, Shift,
    parse_registry,
};

fn ts(raw: &str) -> Timestamp {
    raw.parse().expect("timestamp")
}

fn zoned(raw: &str) -> Zoned {
    raw.parse().unwrap_or_else(|_| {
        raw.parse::<Timestamp>()
            .expect("timestamp")
            .to_zoned(jiff::tz::TimeZone::UTC)
    })
}

fn draft(when: ScheduleWhenDraft, missed: MissPolicy) -> ScheduleDraft {
    ScheduleDraft {
        id: "daily-report".to_owned(),
        workflow: "workflows/report.nika.yaml".to_owned(),
        when,
        max_cost_usd: 0.25,
        missed,
        max_lateness_seconds: None,
        overlap: Some(Overlap::Sauter),
        after_skip: Some(AfterSkip::ProchainCreneau),
        jitter: None,
        tolerance: None,
        active: None,
        pause_reason: None,
        pause_until: None,
    }
}

fn once(missed: MissPolicy) -> ScheduleDraft {
    draft(
        ScheduleWhenDraft::Once {
            at: "2026-09-01T09:00:00Z".to_owned(),
        },
        missed,
    )
}

fn daily(missed: MissPolicy) -> ScheduleDraft {
    draft(
        ScheduleWhenDraft::Cadence {
            expression: "TZ=UTC 0 9 * * *".to_owned(),
        },
        missed,
    )
}

fn planned(draft: ScheduleDraft, now: &str, state: &ScheduleDecisionState) -> SchedulePlan {
    plan_schedule(
        &draft.validate().expect("valid schedule"),
        &zoned(now),
        state,
        8,
    )
    .expect("plan")
}

fn state_after(slot: &ScheduleSlot) -> ScheduleDecisionState {
    ScheduleDecisionState::after(slot)
}

#[test]
fn once_before_and_at_are_not_due_then_on_time_for_every_missed_policy() {
    for missed in [
        MissPolicy::Rattraper,
        MissPolicy::RattraperUneFois,
        MissPolicy::Sauter,
    ] {
        let before = planned(
            once(missed),
            "2026-09-01T08:59:59Z",
            &ScheduleDecisionState::empty(),
        );
        assert!(matches!(before.due(), ScheduleDueVerdict::NotDue));
        assert_eq!(
            before.earliest_wake_hint(),
            Some(ts("2026-09-01T09:00:00Z"))
        );
        assert_eq!(
            before.next_slots().next().map(ScheduleSlot::scheduled_for),
            Some(ts("2026-09-01T09:00:00Z"))
        );

        let at = planned(
            once(missed),
            "2026-09-01T09:00:00Z",
            &ScheduleDecisionState::empty(),
        );
        assert!(matches!(
            at.due(),
            ScheduleDueVerdict::ScheduledOnTime { .. }
        ));
        assert_eq!(at.earliest_wake_hint(), Some(ts("2026-09-01T09:00:00Z")));
    }
}

#[test]
fn once_after_applies_missed_policy_then_the_distinct_lateness_bound() {
    let caught = planned(
        once(MissPolicy::Rattraper),
        "2026-09-01T09:10:00Z",
        &ScheduleDecisionState::empty(),
    );
    assert!(matches!(
        caught.due(),
        ScheduleDueVerdict::CatchUp {
            missed_slots: 1,
            ..
        }
    ));

    let caught_once = planned(
        once(MissPolicy::RattraperUneFois),
        "2026-09-01T09:10:00Z",
        &ScheduleDecisionState::empty(),
    );
    assert!(matches!(
        caught_once.due(),
        ScheduleDueVerdict::CatchUp {
            missed_slots: 1,
            ..
        }
    ));

    let skipped = planned(
        once(MissPolicy::Sauter),
        "2026-09-01T09:10:00Z",
        &ScheduleDecisionState::empty(),
    );
    assert!(matches!(
        skipped.due(),
        ScheduleDueVerdict::SkippedMissed {
            missed_slots: 1,
            ..
        }
    ));

    for missed in [
        MissPolicy::Rattraper,
        MissPolicy::RattraperUneFois,
        MissPolicy::Sauter,
    ] {
        let mut at_boundary = once(missed);
        at_boundary.max_lateness_seconds = Some(600);
        let plan = planned(
            at_boundary,
            "2026-09-01T09:10:00Z",
            &ScheduleDecisionState::empty(),
        );
        assert!(
            !matches!(plan.due(), ScheduleDueVerdict::SkippedTooLate { .. }),
            "the inclusive maximum remains eligible"
        );

        let mut beyond = once(missed);
        beyond.max_lateness_seconds = Some(599);
        let plan = planned(
            beyond,
            "2026-09-01T09:10:00Z",
            &ScheduleDecisionState::empty(),
        );
        assert!(matches!(
            plan.due(),
            ScheduleDueVerdict::SkippedTooLate {
                lateness_seconds: 600,
                maximum_seconds: 599,
                ..
            }
        ));
    }
}

#[test]
fn consumed_once_survives_restart_clock_rollback_and_identical_reapply() {
    let definition = once(MissPolicy::Rattraper).validate().expect("definition");
    let first = plan_schedule(
        &definition,
        &zoned("2026-09-01T09:00:00Z"),
        &ScheduleDecisionState::empty(),
        4,
    )
    .expect("first plan");
    let slot = first.due().slot().expect("due slot");
    let durable = state_after(slot);

    for now in [
        "2026-08-31T09:00:00Z",
        "2026-09-01T09:00:00Z",
        "2027-09-01T09:00:00Z",
    ] {
        let replay = plan_schedule(&definition, &zoned(now), &durable, 4).expect("replay");
        assert!(matches!(
            replay.due(),
            ScheduleDueVerdict::OnceConsumed { slot_id, .. }
                if slot_id == slot.id()
        ));
        assert_eq!(replay.next_slots().count(), 0);
        assert_eq!(replay.earliest_wake_hint(), None);
    }

    let identical = once(MissPolicy::Rattraper)
        .validate()
        .expect("identical definition");
    assert_eq!(definition.revision(), identical.revision());
    assert!(matches!(
        plan_schedule(&identical, &zoned("2026-09-02T09:00:00Z"), &durable, 4)
            .expect("reapplied")
            .due(),
        ScheduleDueVerdict::OnceConsumed { .. }
    ));
}

#[test]
fn cadence_distinguishes_on_time_catch_up_once_and_skip() {
    let on_time = planned(
        daily(MissPolicy::Rattraper),
        "2026-09-01T09:02:00Z",
        &ScheduleDecisionState::empty(),
    );
    assert!(matches!(
        on_time.due(),
        ScheduleDueVerdict::ScheduledOnTime { .. }
    ));

    let seed = planned(
        daily(MissPolicy::Rattraper),
        "2026-09-01T09:00:00Z",
        &ScheduleDecisionState::empty(),
    );
    let durable = state_after(seed.due().slot().expect("seed slot"));

    let catch_all = planned(
        daily(MissPolicy::Rattraper),
        "2026-09-04T12:00:00Z",
        &durable,
    );
    assert!(matches!(
        catch_all.due(),
        ScheduleDueVerdict::CatchUp {
            slot,
            missed_slots: 3,
        } if slot.scheduled_for() == ts("2026-09-02T09:00:00Z")
    ));

    let catch_once = planned(
        daily(MissPolicy::RattraperUneFois),
        "2026-09-04T12:00:00Z",
        &durable,
    );
    assert!(matches!(
        catch_once.due(),
        ScheduleDueVerdict::CatchUp {
            slot,
            missed_slots: 3,
        } if slot.scheduled_for() == ts("2026-09-04T09:00:00Z")
    ));

    let skipped = planned(daily(MissPolicy::Sauter), "2026-09-04T12:00:00Z", &durable);
    assert!(matches!(
        skipped.due(),
        ScheduleDueVerdict::SkippedMissed {
            slot,
            missed_slots: 3,
        } if slot.scheduled_for() == ts("2026-09-04T09:00:00Z")
    ));
}

#[test]
fn cadence_max_lateness_boundary_is_inclusive_after_miss_classification() {
    let seed = planned(
        daily(MissPolicy::RattraperUneFois),
        "2026-09-01T09:00:00Z",
        &ScheduleDecisionState::empty(),
    );
    let durable = state_after(seed.due().slot().expect("seed slot"));

    let mut boundary = daily(MissPolicy::RattraperUneFois);
    boundary.max_lateness_seconds = Some(600);
    assert!(matches!(
        planned(boundary, "2026-09-02T09:10:00Z", &durable).due(),
        ScheduleDueVerdict::CatchUp { .. }
    ));

    let mut beyond = daily(MissPolicy::RattraperUneFois);
    beyond.max_lateness_seconds = Some(599);
    assert!(matches!(
        planned(beyond, "2026-09-02T09:10:00Z", &durable).due(),
        ScheduleDueVerdict::SkippedTooLate {
            lateness_seconds: 600,
            maximum_seconds: 599,
            ..
        }
    ));
}

#[test]
fn slept_process_and_clock_forward_recompute_from_durable_slot_not_cached_hint() {
    let definition = daily(MissPolicy::RattraperUneFois)
        .validate()
        .expect("definition");
    let initial = plan_schedule(
        &definition,
        &zoned("2026-09-01T08:00:00Z"),
        &ScheduleDecisionState::empty(),
        1,
    )
    .expect("initial");
    assert_eq!(
        initial.earliest_wake_hint(),
        Some(ts("2026-09-01T09:00:00Z"))
    );

    let fired = plan_schedule(
        &definition,
        &zoned("2026-09-01T09:00:00Z"),
        &ScheduleDecisionState::empty(),
        1,
    )
    .expect("fired");
    let durable = state_after(fired.due().slot().expect("slot"));

    let woke_late = plan_schedule(&definition, &zoned("2026-09-05T18:00:00Z"), &durable, 1)
        .expect("fresh recomputation");
    assert!(matches!(
        woke_late.due(),
        ScheduleDueVerdict::CatchUp {
            slot,
            missed_slots: 4,
        } if slot.scheduled_for() == ts("2026-09-05T09:00:00Z")
    ));
    assert_eq!(
        woke_late.earliest_wake_hint(),
        Some(ts("2026-09-05T18:00:00Z"))
    );
}

#[test]
fn clock_backward_does_not_rearm_the_last_durable_cadence_slot() {
    let definition = daily(MissPolicy::Rattraper).validate().expect("definition");
    let fired = plan_schedule(
        &definition,
        &zoned("2026-09-01T09:00:00Z"),
        &ScheduleDecisionState::empty(),
        1,
    )
    .expect("fired");
    let durable = state_after(fired.due().slot().expect("slot"));

    let rollback =
        plan_schedule(&definition, &zoned("2026-08-31T12:00:00Z"), &durable, 2).expect("rollback");
    assert!(matches!(rollback.due(), ScheduleDueVerdict::NotDue));
    assert_eq!(
        rollback
            .next_slots()
            .next()
            .map(ScheduleSlot::scheduled_for),
        Some(ts("2026-09-02T09:00:00Z"))
    );
    assert_eq!(
        rollback.earliest_wake_hint(),
        Some(ts("2026-09-02T09:00:00Z"))
    );
}

#[test]
fn inactive_pause_is_a_machine_verdict_and_never_a_wake_hint() {
    let mut paused = daily(MissPolicy::Rattraper);
    paused.active = Some(false);
    paused.pause_reason = Some("operator maintenance".to_owned());
    paused.pause_until = Some("2026-09-10".to_owned());
    let plan = planned(
        paused,
        "2026-09-01T09:00:00Z",
        &ScheduleDecisionState::empty(),
    );
    assert!(matches!(
        plan.due(),
        ScheduleDueVerdict::PausedInactive {
            reason,
            pause_until,
        } if reason == "operator maintenance" && pause_until == "2026-09-10"
    ));
    assert_eq!(plan.next_slots().count(), 0);
    assert_eq!(plan.earliest_wake_hint(), None);

    let active = planned(
        daily(MissPolicy::Rattraper),
        "2026-09-01T08:00:00Z",
        &ScheduleDecisionState::empty(),
    );
    assert!(matches!(active.due(), ScheduleDueVerdict::NotDue));
    assert_eq!(
        active.earliest_wake_hint(),
        Some(ts("2026-09-01T09:00:00Z"))
    );
}

#[test]
fn webhook_has_no_timed_slot_or_wake() {
    let plan = planned(
        draft(ScheduleWhenDraft::Webhook, MissPolicy::Rattraper),
        "2026-09-01T09:00:00Z",
        &ScheduleDecisionState::empty(),
    );
    assert!(matches!(plan.due(), ScheduleDueVerdict::NotDue));
    assert_eq!(plan.next_slots().count(), 0);
    assert_eq!(plan.earliest_wake_hint(), None);
}

#[test]
fn dst_gap_and_fold_keep_the_existing_oracles_shift_evidence() {
    let gap = draft(
        ScheduleWhenDraft::Cadence {
            expression: "TZ=Europe/Paris 30 2 * * *".to_owned(),
        },
        MissPolicy::Rattraper,
    )
    .validate()
    .expect("gap definition");
    let gap_plan = plan_schedule(
        &gap,
        &zoned("2026-03-29T00:00:00+01:00[Europe/Paris]"),
        &ScheduleDecisionState::empty(),
        1,
    )
    .expect("gap plan");
    let gap_slot = gap_plan.next_slots().next().expect("gap slot");
    assert_eq!(gap_slot.scheduled_for(), ts("2026-03-29T01:00:00Z"));
    assert_eq!(gap_slot.shift(), Shift::AdvancedFirstValid);
    assert_eq!(
        gap_slot.requested_civil().map(|civil| civil.to_string()),
        Some("2026-03-29T02:30:00".to_owned())
    );

    let fold_plan = plan_schedule(
        &gap,
        &zoned("2026-10-25T00:00:00+02:00[Europe/Paris]"),
        &ScheduleDecisionState::empty(),
        1,
    )
    .expect("fold plan");
    let fold_slot = fold_plan.next_slots().next().expect("fold slot");
    assert_eq!(fold_slot.scheduled_for(), ts("2026-10-25T00:30:00Z"));
    assert_eq!(fold_slot.shift(), Shift::FoldedFirst);
}

#[test]
fn projection_is_bounded() {
    let candidate = daily(MissPolicy::Rattraper);
    let definition = candidate.validate().expect("definition");
    let plan = plan_schedule(
        &definition,
        &zoned("2026-09-01T08:00:00Z"),
        &ScheduleDecisionState::empty(),
        usize::MAX,
    )
    .expect("plan");
    assert_eq!(plan.next_slots().count(), MAX_SCHEDULE_PROJECTION_SLOTS);
}

#[test]
fn project_and_api_equivalents_share_revision_and_slots() {
    let registry = parse_registry(
        "nika: project\narm:\n  - workflow: workflows/report.nika.yaml\n    cadence: TZ=Europe/Paris lundi 9h00\n    plafond: 0.25\n    manqué: rattraper-une-fois\n",
    )
    .expect("project registry");
    let beat = registry.beats().next().expect("beat");
    let project = ScheduleDraft::from_project("daily-report", beat)
        .expect("project draft")
        .validate()
        .expect("project definition");

    let api = draft(
        ScheduleWhenDraft::Cadence {
            expression: " TZ=Europe/Paris 0 9 * * 1 ".to_owned(),
        },
        MissPolicy::RattraperUneFois,
    )
    .validate()
    .expect("api definition");
    assert_eq!(project.revision(), api.revision());

    let now = zoned("2026-09-01T00:00:00Z");
    let project_slots: Vec<_> = plan_schedule(&project, &now, &ScheduleDecisionState::empty(), 4)
        .expect("project plan")
        .next_slots()
        .cloned()
        .collect();
    let api_slots: Vec<_> = plan_schedule(&api, &now, &ScheduleDecisionState::empty(), 4)
        .expect("api plan")
        .next_slots()
        .cloned()
        .collect();
    assert_eq!(project_slots, api_slots);
}

#[test]
fn active_timed_overlap_replace_refuses_until_a_preemption_law_exists() {
    let mut candidate = daily(MissPolicy::Rattraper);
    candidate.overlap = Some(Overlap::Remplacer);
    candidate.after_skip = None;
    let definition = candidate.validate().expect("definition");
    assert!(matches!(
        plan_schedule(
            &definition,
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        ),
        Err(SchedulePlanError::UnsupportedOverlapReplace)
    ));

    let mut paused = daily(MissPolicy::Rattraper);
    paused.overlap = Some(Overlap::Remplacer);
    paused.after_skip = None;
    paused.active = Some(false);
    paused.pause_reason = Some("maintenance".to_owned());
    paused.pause_until = Some("2026-09-10".to_owned());
    assert!(matches!(
        plan_schedule(
            &paused.validate().expect("paused definition"),
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        )
        .expect("inactive plans without effective times")
        .due(),
        ScheduleDueVerdict::PausedInactive { .. }
    ));

    let mut webhook = draft(ScheduleWhenDraft::Webhook, MissPolicy::Rattraper);
    webhook.overlap = Some(Overlap::Remplacer);
    webhook.after_skip = None;
    assert!(matches!(
        plan_schedule(
            &webhook.validate().expect("webhook definition"),
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        )
        .expect("webhooks have no effective timed slot")
        .due(),
        ScheduleDueVerdict::NotDue
    ));
}

#[test]
fn active_timed_overlap_queue_refuses_until_a_queueing_law_exists() {
    let mut candidate = daily(MissPolicy::Rattraper);
    candidate.overlap = Some(Overlap::File);
    candidate.after_skip = None;
    let definition = candidate.validate().expect("definition");
    assert!(matches!(
        plan_schedule(
            &definition,
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        ),
        Err(SchedulePlanError::UnsupportedOverlapQueue)
    ));

    let mut paused = daily(MissPolicy::Rattraper);
    paused.overlap = Some(Overlap::File);
    paused.after_skip = None;
    paused.active = Some(false);
    paused.pause_reason = Some("maintenance".to_owned());
    paused.pause_until = Some("2026-09-10".to_owned());
    assert!(matches!(
        plan_schedule(
            &paused.validate().expect("paused definition"),
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        )
        .expect("inactive plans without effective times")
        .due(),
        ScheduleDueVerdict::PausedInactive { .. }
    ));

    let mut webhook = draft(ScheduleWhenDraft::Webhook, MissPolicy::Rattraper);
    webhook.overlap = Some(Overlap::File);
    webhook.after_skip = None;
    assert!(matches!(
        plan_schedule(
            &webhook.validate().expect("webhook definition"),
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        )
        .expect("webhooks have no effective timed slot")
        .due(),
        ScheduleDueVerdict::NotDue
    ));
}

#[test]
fn active_timed_after_skip_on_completion_refuses_until_a_trigger_law_exists() {
    let mut candidate = daily(MissPolicy::Rattraper);
    candidate.overlap = Some(Overlap::Sauter);
    candidate.after_skip = Some(AfterSkip::ACompletion);
    let definition = candidate.validate().expect("definition");
    assert!(matches!(
        plan_schedule(
            &definition,
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        ),
        Err(SchedulePlanError::UnsupportedAfterSkipOnCompletion)
    ));

    let mut paused = daily(MissPolicy::Rattraper);
    paused.overlap = Some(Overlap::Sauter);
    paused.after_skip = Some(AfterSkip::ACompletion);
    paused.active = Some(false);
    paused.pause_reason = Some("maintenance".to_owned());
    paused.pause_until = Some("2026-09-10".to_owned());
    assert!(matches!(
        plan_schedule(
            &paused.validate().expect("paused definition"),
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        )
        .expect("inactive plans without effective times")
        .due(),
        ScheduleDueVerdict::PausedInactive { .. }
    ));

    let mut webhook = draft(ScheduleWhenDraft::Webhook, MissPolicy::Rattraper);
    webhook.overlap = Some(Overlap::Sauter);
    webhook.after_skip = Some(AfterSkip::ACompletion);
    assert!(matches!(
        plan_schedule(
            &webhook.validate().expect("webhook definition"),
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        )
        .expect("webhooks have no effective timed slot")
        .due(),
        ScheduleDueVerdict::NotDue
    ));
}

#[test]
fn active_timed_tolerance_refuses_until_the_firm_law_exists() {
    let mut candidate = daily(MissPolicy::Rattraper);
    candidate.tolerance = Some("3/4".to_owned());
    let definition = candidate.validate().expect("definition");
    assert!(matches!(
        plan_schedule(
            &definition,
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        ),
        Err(SchedulePlanError::UnsupportedTolerance)
    ));

    let mut paused = daily(MissPolicy::Rattraper);
    paused.tolerance = Some("3/4".to_owned());
    paused.active = Some(false);
    paused.pause_reason = Some("maintenance".to_owned());
    paused.pause_until = Some("2026-09-10".to_owned());
    assert!(matches!(
        plan_schedule(
            &paused.validate().expect("paused definition"),
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        )
        .expect("inactive plans without effective times")
        .due(),
        ScheduleDueVerdict::PausedInactive { .. }
    ));

    let mut webhook = draft(ScheduleWhenDraft::Webhook, MissPolicy::Rattraper);
    webhook.tolerance = Some("3/4".to_owned());
    assert!(matches!(
        plan_schedule(
            &webhook.validate().expect("webhook definition"),
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        )
        .expect("webhooks have no effective timed slot")
        .due(),
        ScheduleDueVerdict::NotDue
    ));
}

#[test]
fn active_timed_hash_jitter_refuses_until_a_law_exists() {
    let mut jittered = daily(MissPolicy::Rattraper);
    jittered.jitter = Some(ScheduleJitter::Hash);
    let definition = jittered.validate().expect("definition");
    assert!(matches!(
        plan_schedule(
            &definition,
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        ),
        Err(SchedulePlanError::UnsupportedHashJitter)
    ));

    let mut paused = daily(MissPolicy::Rattraper);
    paused.jitter = Some(ScheduleJitter::Hash);
    paused.active = Some(false);
    paused.pause_reason = Some("maintenance".to_owned());
    paused.pause_until = Some("2026-09-10".to_owned());
    assert!(matches!(
        plan_schedule(
            &paused.validate().expect("paused definition"),
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        )
        .expect("inactive plans without effective times")
        .due(),
        ScheduleDueVerdict::PausedInactive { .. }
    ));

    let mut webhook = draft(ScheduleWhenDraft::Webhook, MissPolicy::Rattraper);
    webhook.jitter = Some(ScheduleJitter::Hash);
    assert!(matches!(
        plan_schedule(
            &webhook.validate().expect("webhook definition"),
            &zoned("2026-09-01T08:00:00Z"),
            &ScheduleDecisionState::empty(),
            4,
        )
        .expect("webhooks have no effective timed slot")
        .due(),
        ScheduleDueVerdict::NotDue
    ));
}

#[test]
fn planner_refusals_speak_the_schedule_registry_code() {
    assert_eq!(
        SchedulePlanError::UnsupportedHashJitter.nika_code(),
        nika_error::codes::NIKA_017
    );
    assert_eq!(
        SchedulePlanError::UnsupportedOverlapReplace.nika_code(),
        nika_error::codes::NIKA_017
    );
    assert_eq!(
        SchedulePlanError::UnsupportedOverlapQueue.nika_code(),
        nika_error::codes::NIKA_017
    );
    assert_eq!(
        SchedulePlanError::UnsupportedAfterSkipOnCompletion.nika_code(),
        nika_error::codes::NIKA_017
    );
    assert_eq!(
        SchedulePlanError::UnsupportedTolerance.nika_code(),
        nika_error::codes::NIKA_017
    );
    assert_eq!(
        SchedulePlanError::InvalidCanonicalCadence("bad".to_owned()).nika_code(),
        nika_error::codes::NIKA_017
    );
}
