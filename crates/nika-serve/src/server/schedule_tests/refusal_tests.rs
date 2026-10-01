// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

// Two observed scans cover a scan already in flight when the fixture changes.
// Repeating the injected instant avoids turning a lost test-clock wake into a hang.
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

async fn wait_for_calls(backend: &CountingBackend, minimum: usize) {
    settles("scheduled backend calls", async {
        loop {
            let called = backend.called.notified();
            if backend.calls() >= minimum {
                break;
            }
            called.await;
        }
    })
    .await;
}

async fn create_once(server: &TestServer, id: &str, workflow: &str, at: &str) {
    let response = server
        .request(&put_request(
            id,
            &body_at(workflow, 0.25, at),
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(response.status, 200, "{}", response.body);
}

async fn api_status(server: &TestServer, id: &str) -> serde_json::Value {
    let response = server
        .request(&get_request(&format!("/v1/schedules/{id}")))
        .await;
    assert_eq!(response.status, 200, "{}", response.body);
    let status = response.json();
    assert_eq!(status["origin"], "api", "{status}");
    status
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn api_refusal_is_visible_while_other_schedules_fire_and_clears_after_healing() {
    let world = TestWorld::new();
    let original = std::fs::read(world.workflows.join("root.nika")).expect("fixture");
    std::fs::write(world.workflows.join("healthy.nika"), &original).expect("healthy workflow");
    let backend = Arc::new(CountingBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:00Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    create_once(&server, "bad", "root.nika", "2026-09-01T09:00:00Z").await;
    create_once(&server, "good", "healthy.nika", "2026-09-01T09:00:00Z").await;
    tick(&clock, "2026-09-01T08:00:00Z[UTC]").await;

    std::fs::write(world.workflows.join("root.nika"), "nika: [broken\n").expect("break");
    tick(&clock, "2026-09-01T09:00:00Z[UTC]").await;
    wait_for_calls(&backend, 1).await;
    let bad = api_status(&server, "bad").await;
    assert_eq!(bad["finding"]["code"], "schedule.admission", "{bad}");
    assert!(bad["finding"]["detail"].is_string(), "{bad}");
    assert!(
        bad["lastDecision"].is_null(),
        "a refused slot stays due: {bad}"
    );
    let good = api_status(&server, "good").await;
    assert!(good.get("finding").is_none(), "{good}");
    assert_eq!(good["lastDecision"]["action"], "claimed", "{good}");
    assert_eq!(backend.calls(), 1, "only the healthy workflow executed");
    assert_eq!(server.request(&get_request("/health")).await.status, 200);

    std::fs::write(world.workflows.join("root.nika"), original).expect("heal");
    tick(&clock, "2026-09-01T09:01:00Z[UTC]").await;
    wait_for_calls(&backend, 2).await;
    let healed = api_status(&server, "bad").await;
    assert!(healed.get("finding").is_none(), "{healed}");
    assert_eq!(healed["lastDecision"]["action"], "claimed", "{healed}");
    assert_eq!(backend.calls(), 2, "healing executes the refused slot once");
    settles("server stop", server.stop()).await.expect("stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_fire_cannot_clear_the_api_refusal_with_the_same_id() {
    let world = TestWorld::new();
    let original = std::fs::read(world.workflows.join("root.nika")).expect("fixture");
    std::fs::write(world.workflows.join("bad.nika"), &original).expect("API workflow");
    std::fs::write(
        world.workflows.join("nika.yaml"),
        "nika: proj\narm:\n  - workflow: root.nika\n    cadence: \"TZ=UTC 2 8 * * *\"\n    plafond: 0.25\n    manqué: sauter\n",
    )
    .expect("project beat due after the API refusal");
    let backend = Arc::new(CountingBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:30Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    create_once(&server, "root", "bad.nika", "2026-09-01T08:01:00Z").await;
    tick(&clock, "2026-09-01T08:00:30Z[UTC]").await;

    std::fs::write(world.workflows.join("bad.nika"), "nika: [broken\n").expect("break API");
    tick(&clock, "2026-09-01T08:01:01Z[UTC]").await;
    let failed = server.request(&get_request("/v1/schedules/root")).await;
    assert_eq!(failed.status, 200, "{}", failed.body);
    assert_eq!(failed.json()["finding"]["code"], "schedule.admission");
    let etag = failed.header("etag").expect("API revision");
    let mut paused: serde_json::Value =
        serde_json::from_str(&body_at("root.nika", 0.25, "2026-09-01T08:01:00Z"))
            .expect("fixture body");
    paused["active"] = json!(false);
    paused["pauseReason"] = json!("paused");
    paused["pauseUntil"] = json!("2099-12-31");
    let update = server
        .request(&put_request(
            "root",
            &paused.to_string(),
            &format!("If-Match: {etag}\r\n"),
            true,
        ))
        .await;
    assert_eq!(update.status, 200, "{}", update.body);
    // Pause through the public API so a retry cannot recreate a wrongly cleared
    // refusal and mask a collision when the other origin subsequently fires.
    tick(&clock, "2026-09-01T08:02:01Z[UTC]").await;
    wait_for_calls(&backend, 1).await;
    let failed_api = api_status(&server, "root").await;
    assert_eq!(
        failed_api["finding"]["code"], "schedule.admission",
        "{failed_api}"
    );
    assert!(failed_api["lastDecision"].is_null(), "{failed_api}");
    assert_eq!(backend.calls(), 1, "the project origin alone executed");
    settles("server stop", server.stop()).await.expect("stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_then_readding_an_inactive_project_beat_drops_its_old_refusal() {
    let world = TestWorld::new();
    let original = std::fs::read(world.workflows.join("root.nika")).expect("fixture");
    write_daily_beat(&world);
    let backend = Arc::new(CountingBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:30Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    tick(&clock, "2026-09-01T08:00:30Z[UTC]").await;
    std::fs::remove_file(world.workflows.join("root.nika")).expect("remove workflow");
    tick(&clock, "2026-09-01T08:01:01Z[UTC]").await;
    let refused = project_finding(&server, "root").await;
    assert_eq!(refused["code"], "schedule.admission", "{refused}");

    std::fs::write(world.workflows.join("nika.yaml"), "nika: proj\n").expect("remove beat");
    tick(&clock, "2026-09-01T08:01:30Z[UTC]").await;
    assert_eq!(
        server
            .request(&get_request("/v1/schedules/root"))
            .await
            .status,
        404,
        "the removed project beat is no longer projected"
    );
    std::fs::write(world.workflows.join("root.nika"), original).expect("restore workflow");
    write_project_beat(
        &world,
        "    actif: false\n    raison: \"paused\"\n    jusqu_au: \"2099-12-31\"\n",
    );
    tick(&clock, "2026-09-01T08:02:00Z[UTC]").await;
    // A broken refresh must expose the last valid, inactive definition.
    // Otherwise a missing or rejected re-add could also explain a 404.
    let registry = world.workflows.join("nika.yaml");
    let inactive = std::fs::read(&registry).expect("inactive registry");
    std::fs::write(&registry, "nika: [broken\n").expect("break refresh");
    tick(&clock, "2026-09-01T08:02:01Z[UTC]").await;
    let retained = server.request(&get_request("/v1/schedules/root")).await;
    assert_eq!(retained.status, 200, "{}", retained.body);
    let retained = retained.json();
    assert_eq!(retained["origin"], "project", "{retained}");
    assert_eq!(retained["active"], false, "{retained}");
    assert_eq!(
        retained["definition"]["workflow"], "root.nika",
        "{retained}"
    );
    assert_eq!(retained["finding"]["code"], "project.invalid", "{retained}");
    std::fs::write(&registry, inactive).expect("restore inactive registry");
    tick(&clock, "2026-09-01T08:02:02Z[UTC]").await;
    let fresh = server.request(&get_request("/v1/schedules/root")).await;
    assert_eq!(
        fresh.status, 404,
        "no finding belongs to the new inactive beat: {}",
        fresh.body
    );
    assert_eq!(
        backend.calls(),
        0,
        "neither the refused nor inactive beat executes"
    );
    settles("server stop", server.stop()).await.expect("stop");
}
