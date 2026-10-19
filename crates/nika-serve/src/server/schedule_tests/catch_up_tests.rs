// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a resident catch-up tells its run and its job (spec 01 §`manqué`, spec 04 §`run`).

use super::*;

/// Records what each scheduled run is told, in order, and succeeds.
#[derive(Debug, Default)]
struct TellingBackend {
    told: Mutex<Vec<nika_runtime::RunFacts>>,
    called: tokio::sync::Notify,
}

impl ExecutionBackend for TellingBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async { ExecutionDisposition::Succeeded.into() })
    }

    fn execute_scheduled<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        _ceiling: (Option<f64>, Option<&str>),
        _inputs: &std::collections::BTreeMap<String, serde_json::Value>,
        _cancel: nika_types::cancel::CancelCtx,
        facts: nika_runtime::RunFacts,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        self.told.lock().expect("told facts").push(facts);
        self.called.notify_waiters();
        self.execute(context)
    }
}

/// The facts the first `runs` scheduled runs were told.
async fn told(backend: &TellingBackend, runs: usize) -> Vec<nika_runtime::RunFacts> {
    settles("scheduled runs", async {
        loop {
            let called = backend.called.notified();
            let told = backend.told.lock().expect("told facts").clone();
            if told.len() >= runs {
                return told;
            }
            called.await;
        }
    })
    .await
}

/// The job the schedule's last decision claimed, once it succeeded.
async fn claimed_job(server: &TestServer, id: &str) -> serde_json::Value {
    settles("the claimed run settles", async {
        loop {
            let status = server
                .request(&get_request(&format!("/v1/schedules/{id}")))
                .await
                .json();
            if let Some(run_id) = status["lastDecision"]["claim"]["runId"].as_str() {
                let job = server
                    .request(&get_request(&format!("/v1/jobs/{run_id}")))
                    .await
                    .json();
                if job["status"] == "succeeded" {
                    return job;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
}

/// An every-minute API schedule under `missed`, created at 08:00:30: its 08:00
/// slot fires on time, the watermark a later silence counts from. The resident
/// then stops, as a machine that sleeps.
async fn fired_on_time_then_down(world: &TestWorld, clock: &Arc<ManualClock>, missed: &str) {
    let server = world
        .start_with_clock(Arc::new(TellingBackend::default()), limits(), clock.clone())
        .await;
    let every_minute = json!({
        "workflow": "root.nika",
        "when": {"kind": "cadence", "expression": "TZ=UTC * * * * *"},
        "maxCostUsd": 0.25,
        "missed": missed,
        "maxLatenessSeconds": 3600,
        "overlap": "skip",
        "afterSkip": "next_slot"
    })
    .to_string();
    let created = server
        .request(&put_request(
            "silence",
            &every_minute,
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(created.status, 200, "{}", created.body);
    let on_time = claimed_job(&server, "silence").await;
    let origin = &on_time["receipt"]["origin"];
    assert_eq!(origin["decision"], "scheduled", "{origin}");
    assert_eq!(origin["scheduled_for"], "2026-09-01T08:00:00Z", "{origin}");
    server.stop().await.expect("the resident goes down");
}

/// Back after 08:01 to 08:04 passed unfired, a `catch-up-once` fire
/// tells its run the silence it answers for (spec 04 §`run`: `{ first, last,
/// count }`), and the job's schedule origin carries it (`missed_first` ·
/// `missed_count`), its `scheduled_for` the silence's last slot.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_catch_up_once_tells_its_run_and_its_job_the_silence_it_answers_for() {
    let world = TestWorld::new();
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:30Z[UTC]"));
    fired_on_time_then_down(&world, &clock, "catch-up-once").await;
    clock.advance_to("2026-09-01T08:04:30Z[UTC]");
    let backend = Arc::new(TellingBackend::default());
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    let told = told(&backend, 1).await;
    let (at, source) = told[0].scheduled_for.expect("the slot");
    assert_eq!(at.to_string(), "2026-09-01T08:04:00.000Z");
    assert_eq!(source, nika_runtime::ScheduledForSource::Slot);
    let missed = told[0]
        .missed
        .expect("a catch-up-once answers for its silence");
    assert_eq!(
        (
            missed.first.to_string(),
            missed.last.to_string(),
            missed.count
        ),
        (
            "2026-09-01T08:01:00.000Z".to_owned(),
            "2026-09-01T08:04:00.000Z".to_owned(),
            4
        )
    );
    let job = claimed_job(&server, "silence").await;
    let origin = &job["receipt"]["origin"];
    assert_eq!(origin["decision"], "catch_up", "{origin}");
    assert_eq!(origin["scheduled_for"], "2026-09-01T08:04:00Z", "{origin}");
    assert_eq!(origin["missed_first"], "2026-09-01T08:01:00Z", "{origin}");
    assert_eq!(origin["missed_count"], 4, "{origin}");
    server.stop().await.expect("stop");
}

/// A healthy project beat is observed through its durable claim, not the API schedule route.
async fn claimed_project_job(
    world: &TestWorld,
    server: &TestServer,
    id: &str,
) -> serde_json::Value {
    let store = crate::ScheduleStore::open(&world.state).expect("the project schedule store");
    settles("the claimed project run settles", async {
        loop {
            if let Some(decided) = store
                .last_decision(nika_cadence::ScheduleOrigin::Project, id)
                .expect("the project decision reads")
                && let Some(claim) = decided.claim()
            {
                let job = server
                    .request(&get_request(&format!("/v1/jobs/{}", claim.run_id())))
                    .await
                    .json();
                if job["status"] == "succeeded" {
                    return job;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
}

// Two observed scans cover a scan already in flight when the clock moves (as `refusal_tests`).
async fn tick(clock: &ManualClock, now: &str) {
    let minimum = clock.sleeps.load(Ordering::SeqCst) + 2;
    settles("schedule scans", async {
        loop {
            clock.advance_to(now);
            if clock.sleeps.load(Ordering::SeqCst) >= minimum {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
}

/// `{ rattraper: N }` on the resident (spec 01 §`manqué`): back at 08:11:30 after
/// the 08:00–08:05 window, a project beat under `{ rattraper: 2 }` skips 08:01
/// to 08:03, then fires 08:04 and 08:05, oldest first, each run told only its
/// own slot. Both slots exceed the five-minute grace; the second waits for
/// the first instead of an overlap skip.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bounded_catch_up_fires_the_n_latest_oldest_first() {
    let world = TestWorld::new();
    std::fs::write(
        world.workflows.join("nika.yaml"),
        "nika: proj\narm:\n  - workflow: root.nika\n    cadence: \"TZ=UTC 0-5 8 * * *\"\n    plafond: 0.25\n    manqué: { rattraper: 2 }\n",
    )
    .expect("project nika.yaml");
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:30Z[UTC]"));
    let first = world
        .start_with_clock(Arc::new(TellingBackend::default()), limits(), clock.clone())
        .await;
    let on_time = claimed_project_job(&world, &first, "root").await;
    let origin = &on_time["receipt"]["origin"];
    assert_eq!(origin["scheduled_for"], "2026-09-01T08:00:00Z", "{origin}");
    first.stop().await.expect("the resident goes down");
    clock.advance_to("2026-09-01T08:11:30Z[UTC]");
    let backend = Arc::new(TellingBackend::default());
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    // A due slot asks for a zero wake, which only an advance of the test clock grants.
    let told = settles("the bounded catch-up", async {
        loop {
            let told = backend.told.lock().expect("told facts").clone();
            if told.len() >= 2 {
                return told;
            }
            tick(&clock, "2026-09-01T08:11:30Z[UTC]").await;
        }
    })
    .await;
    let slots: Vec<String> = told
        .iter()
        .map(|facts| facts.scheduled_for.expect("a slot").0.to_string())
        .collect();
    assert_eq!(
        slots,
        ["2026-09-01T08:04:00.000Z", "2026-09-01T08:05:00.000Z"]
    );
    assert!(
        told.iter().all(|facts| facts.missed.is_none()),
        "each run is told only its own slot: {told:?}"
    );
    let last = claimed_project_job(&world, &server, "root").await;
    let origin = &last["receipt"]["origin"];
    assert_eq!(origin["decision"], "catch_up", "{origin}");
    assert_eq!(origin["scheduled_for"], "2026-09-01T08:05:00Z", "{origin}");
    assert!(origin.get("missed_first").is_none(), "{origin}");
    server.stop().await.expect("stop");
}
