// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::json;

use super::*;

fn lines(queued: &[Queued]) -> Vec<&str> {
    queued.iter().map(|q| q.line.as_str()).collect()
}

fn ids(queued: &[Queued]) -> Vec<String> {
    queued.iter().map(|q| q.id.clone()).collect()
}

/// The lines a run returned unsent.
fn returned(steering: &Steering) -> Vec<String> {
    (steering.records().into_iter())
        .filter(|q| q.state == QueuedState::Returned)
        .map(|q| q.line)
        .collect()
}

#[test]
fn each_mode_is_taken_in_order_and_leaves_the_other_queued() {
    let steering = Steering::new();
    steering.open();
    steering.follow_up("then summarize").unwrap();
    steering.steer("use Le Monde too").unwrap();
    assert_eq!(steering.steer("   "), Err(QueueRefused::Blank));
    steering.steer("and TechCrunch").unwrap();
    assert!(steering.pending(QueueMode::Steer));
    let steered = steering.take(QueueMode::Steer);
    assert_eq!(lines(&steered), ["use Le Monde too", "and TechCrunch"]);
    assert!(!steering.pending(QueueMode::Steer));
    assert!(steering.pending(QueueMode::FollowUp));
    assert_eq!(
        lines(&steering.take(QueueMode::FollowUp)),
        ["then summarize"]
    );
    assert!(steering.take(QueueMode::FollowUp).is_empty());
}

/// A host queues from its own thread; the run sees the line through its clone.
#[test]
fn a_line_queued_on_another_thread_reaches_the_run() {
    let steering = Steering::new();
    steering.open();
    let host = steering.clone();
    let queued = std::thread::scope(|scope| {
        scope
            .spawn(move || host.steer("stop reading the RSS feed"))
            .join()
            .unwrap()
    });
    let queued = queued.unwrap();
    assert_eq!((queued.id.as_str(), queued.mode), ("l1", QueueMode::Steer));
    assert_eq!(steering.take(QueueMode::Steer), [queued]);
}

#[test]
fn each_line_has_an_identity_and_a_state_a_host_shows() {
    let steering = Steering::new();
    steering.open();
    let steer = steering.steer("use b instead").unwrap();
    let follow = steering.follow_up("and c").unwrap();
    assert_eq!(ids(&[steer.clone(), follow.clone()]), ["l1", "l2"]);
    let taken = steering.take(QueueMode::Steer);
    steering.entered(&taken[0].id, "u2");
    steering.close();
    assert_eq!(returned(&steering), ["and c"]);
    let records = steering.records();
    assert_eq!(
        serde_json::to_value(&records).unwrap(),
        json!([
            {"id": "l1", "mode": "steer", "line": "use b instead", "state": "entered",
                "cite": "u2"},
            {"id": "l2", "mode": "follow_up", "line": "and c", "state": "returned"}
        ])
    );
    let back: Vec<Queued> =
        serde_json::from_value(serde_json::to_value(&records).unwrap()).unwrap();
    assert_eq!(back, records);
}

/// Only a run reading the queue takes a line: before it opens and after it closes, a line is
/// refused, never left for a later run.
#[test]
fn a_queue_no_run_reads_refuses_lines() {
    let steering = Steering::new();
    assert!(!steering.reading());
    assert_eq!(steering.steer("too early"), Err(QueueRefused::NotReading));
    steering.open();
    assert!(steering.reading());
    steering.follow_up("in time").unwrap();
    steering.close();
    assert_eq!(returned(&steering), ["in time"]);
    assert_eq!(
        steering.follow_up("too late"),
        Err(QueueRefused::NotReading)
    );
    assert_eq!(QueueRefused::NotReading.as_str(), "not_reading");
}

/// A new run forgets the last run's lines; identities keep counting.
#[test]
fn a_new_run_forgets_the_lines_of_the_last_one() {
    let steering = Steering::new();
    steering.open();
    steering.steer("first").unwrap();
    steering.close();
    steering.open();
    assert!(steering.records().is_empty());
    assert_eq!(steering.steer("second").unwrap().id, "l2");
}

#[test]
fn lines_taken_and_put_back_keep_their_place() {
    let steering = Steering::new();
    steering.open();
    steering.steer("first").unwrap();
    steering.steer("second").unwrap();
    let taken = steering.take(QueueMode::Steer);
    steering.steer("newer").unwrap();
    steering.requeue(&ids(&taken));
    assert_eq!(lines(&steering.drain()), ["first", "second", "newer"]);
    assert!(
        steering
            .records()
            .iter()
            .all(|q| q.state == QueuedState::Returned)
    );
}

/// A run takes a bounded number of lines, whatever became of them; the next run takes lines
/// again.
#[test]
fn a_run_takes_a_bounded_number_of_lines() {
    let steering = Steering::new();
    steering.open();
    for n in 0..MAX_QUEUED {
        steering.follow_up(format!("line {n}")).unwrap();
    }
    let taken = steering.take(QueueMode::FollowUp);
    steering.entered(&taken[0].id, "u2");
    assert_eq!(steering.steer("one more"), Err(QueueRefused::Full));
    assert_eq!(QueueRefused::Full.as_str(), "full");
    assert_eq!(steering.records().len(), MAX_QUEUED);
    steering.close();
    steering.open();
    let next = steering.steer("next run").unwrap();
    assert_eq!(next.id, format!("l{}", MAX_QUEUED + 1));
}
