// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! #1349 — the server-level default budget ceiling: a manual
//! `POST /v1/jobs` run never again executes with no spend ceiling; a
//! declaration restricts the default, never widens it; an invalid
//! configured ceiling refuses at startup.

use std::sync::Mutex;

use super::*;

/// The probe the schedule suites speak: records whether a ceiling rode
/// the admission all the way to the execution seam.
#[derive(Debug, Default)]
struct BudgetBackend {
    max_cost_usd: Mutex<Option<f64>>,
}

impl BudgetBackend {
    fn max_cost_usd(&self) -> Option<f64> {
        *self.max_cost_usd.lock().expect("recorded max cost")
    }
}

impl ExecutionBackend for BudgetBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async { ExecutionDisposition::Succeeded.into() })
    }

    fn execute_with_max_cost<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        *self.max_cost_usd.lock().expect("record max cost") = max_cost_usd;
        self.execute(context)
    }
}

/// (a) A manual `POST /v1/jobs` run declares no ceiling of its own —
/// the SERVER default rides the admission and reaches the runtime
/// exactly the way a schedule's `maxCostUsd` does.
#[tokio::test(flavor = "multi_thread")]
async fn a_manual_job_runs_under_the_server_default_budget_ceiling() {
    let world = TestWorld::new();
    let backend = Arc::new(BudgetBackend::default());
    let server = world
        .start(
            backend.clone(),
            limits().with_default_max_cost_usd(Some(0.75)),
        )
        .await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "manual-default-ceiling",
            &auth_header(),
        ))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("succeeded");
    assert_eq!(
        backend.max_cost_usd(),
        Some(0.75),
        "the server default reaches the runtime when the run declares none"
    );
    server.stop().await.expect("clean stop");
}

/// The escape hatch is EXPLICIT: an embedder that disarms the default
/// gets the old unceilinged behavior — never silently.
#[tokio::test(flavor = "multi_thread")]
async fn an_explicitly_disarmed_default_leaves_a_manual_job_unceilinged() {
    let world = TestWorld::new();
    let backend = Arc::new(BudgetBackend::default());
    let server = world
        .start(backend.clone(), limits().with_default_max_cost_usd(None))
        .await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "manual-disarmed-ceiling",
            &auth_header(),
        ))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("succeeded");
    assert_eq!(
        backend.max_cost_usd(),
        None,
        "an explicit None disarms the server default"
    );
    server.stop().await.expect("clean stop");
}

/// (c) A configured default that is zero, negative, NaN, or infinite
/// would silently DISARM the guard it claims to arm (the CLI's
/// `--max-cost-usd` parser refuses the same class) — startup refuses.
#[tokio::test(flavor = "multi_thread")]
async fn an_invalid_default_budget_ceiling_refuses_before_state_io() {
    let world = TestWorld::new();
    let backend = Arc::new(BudgetBackend::default());
    for ceiling in [0.0, -0.5, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let config = ResidentConfig::new(&world.state)
            .with_limits(limits().with_default_max_cost_usd(Some(ceiling)));
        assert!(
            matches!(
                ResidentAuthority::open(config, backend.clone()).await,
                Err(ServerError::InvalidConfig(_))
            ),
            "ceiling {ceiling} must refuse at startup"
        );
    }
}
