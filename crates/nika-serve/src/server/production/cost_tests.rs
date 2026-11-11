// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resident's cost gate for a job with no reviewed authority (manual,
//! snapshot or scheduled): the one host evaluator `nika run` uses (C6). An
//! admitted API route whose price needs a fresh one-time choice refuses before
//! the worker starts, with the review door named; exact priced, local and
//! model-free plans keep their composition; a declared-free route binds the
//! per-Run observer. Plans come from the public constructors; nothing is
//! dispatched and no environment is written. (The route-observation law, its
//! wrong-endpoint and override cases included, is tested with its owner:
//! `nika_service_execution::run_cost`.)

use std::collections::BTreeMap;
use std::path::Path;

use nika_providers::{ExecutionAccessPlan, LaneVerdict, ResolvedLane};
use nika_service_execution::ServiceExecutionDriver;
use nika_types::access::{AccessClass, AccessPlan, BillingClass};

use super::unreviewed_cost;

fn plan_with(
    model: &str,
    provider: &str,
    chosen: AccessClass,
    billing: BillingClass,
) -> ExecutionAccessPlan {
    let access = AccessPlan::new(
        model,
        provider,
        provider,
        chosen,
        billing,
        false,
        Vec::new(),
    );
    let mut lanes = BTreeMap::new();
    lanes.insert(
        model.to_owned(),
        LaneVerdict::Admitted(ResolvedLane::new(access, 1)),
    );
    ExecutionAccessPlan::new(lanes, None, None, None)
}

/// A driver over one admitted workflow that asks `model` for bounded text.
#[cfg(test)]
fn driver(root: &Path, model: &str, max_tokens: u32) -> ServiceExecutionDriver {
    let source = format!(
        "nika: gate\nmodel: {model}\npermits: {{}}\ntasks:\n  ask:\n    infer: {{ prompt: hi, max_tokens: {max_tokens} }}\n"
    );
    admitted(root, &source)
}

/// A driver over `source`, admitted as the project's `gate.nika`.
#[cfg(test)]
fn admitted(root: &Path, source: &str) -> ServiceExecutionDriver {
    std::fs::write(root.join("gate.nika"), source).expect("workflow");
    let project = nika_fs::OwnedDir::open(root).expect("project");
    let service = nika_execution::ExecutionService::default();
    let admitted = service
        .admit(&project, Path::new("gate.nika"))
        .expect("a clean workflow admits");
    let session = service.begin(admitted);
    ServiceExecutionDriver::new(session.context(), root).expect("driver")
}

#[cfg(test)]
fn judge(
    model: &str,
    provider: &str,
    chosen: AccessClass,
    billing: BillingClass,
) -> Result<bool, String> {
    let root = tempfile::tempdir().expect("root");
    let plan = plan_with(model, provider, chosen, billing);
    // A reasoning seat needs room for its thinking before Check admits it.
    let max_tokens = if provider == "deepseek" { 512 } else { 16 };
    let cost = unreviewed_cost(
        root.path(),
        &driver(root.path(), model, max_tokens),
        &plan,
        &BTreeMap::new(),
        None,
    )?;
    // Whatever the verdict, the lease the evaluator may have taken is free again.
    let project = nika_fs::OwnedDir::open(root.path()).expect("project");
    let clear = nika_dap::cost_journal::clear(&project, "after").expect("journal");
    assert!(
        clear.is_ok(),
        "the evaluator never keeps the project's cost lease"
    );
    Ok(cost.is_some())
}

#[test]
fn a_default_openai_route_without_a_review_refuses_and_names_the_door() {
    let refused = judge(
        "openai/gpt-4.1-mini",
        "openai",
        AccessClass::Api,
        BillingClass::ApiMetered,
    )
    .expect_err("an unreviewed unknown-cost route must not reach the provider");
    assert!(refused.contains("price unknown"), "{refused}");
    assert!(refused.contains("--cost-review"), "{refused}");
    assert!(
        refused.contains("scheduled occurrences stay refused"),
        "{refused}"
    );
}

#[test]
fn deepseek_direct_stays_admitted_without_an_observer() {
    let observed = judge(
        "deepseek/deepseek-flash",
        "deepseek",
        AccessClass::Api,
        BillingClass::ApiMetered,
    );
    assert_eq!(observed, Ok(false));
}

#[test]
fn a_local_lane_is_never_observed_as_an_api_price() {
    let observed = judge(
        "ollama/llama3.2",
        "ollama",
        AccessClass::Local,
        BillingClass::Local,
    );
    assert_eq!(observed, Ok(false));
}

#[test]
fn a_catalog_priced_native_default_stays_admitted() {
    let model = "anthropic/claude-sonnet-4-20250514";
    let observed = judge(
        model,
        "anthropic",
        AccessClass::Api,
        BillingClass::ApiMetered,
    );
    assert_eq!(observed, Ok(false));
}

/// C6 defect 2 (B12): a refused job's record carries its message through the
/// one shared bound, which keeps the review route the refusal names.
#[test]
fn the_unreviewed_refusal_keeps_its_review_route_in_the_job_record() {
    let bounded = crate::server::bound_message(super::UNREVIEWED);
    assert!(
        bounded.contains("admits POST /v1/cost-reviews, then one job"),
        "{bounded}"
    );
}

/// B12 r5 · an unreviewed job whose fan sends nothing binds the per-Run
/// observer, as `nika run` does, and its validated inputs decide: the same
/// workflow with its declared default or with items refuses before the worker.
#[test]
fn an_unreviewed_zero_fan_binds_the_observer_and_its_inputs_decide() {
    let model = "openai/gpt-4.1-mini";
    let plan = plan_with(model, "openai", AccessClass::Api, BillingClass::ApiMetered);
    let source = format!(
        "nika: gate\nmodel: {model}\ninputs:\n  items: {{ type: {{ array: string }}, default: [a] }}\npermits: {{}}\ntasks:\n  ask:\n    for_each: {{ items: '${{{{ inputs.items }}}}' }}\n    infer: {{ prompt: 'x ${{{{ item }}}}', max_tokens: 16 }}\n"
    );
    let judge = |items: Option<serde_json::Value>| {
        let root = tempfile::tempdir().expect("root");
        let inputs: BTreeMap<String, serde_json::Value> =
            items.into_iter().map(|v| ("items".to_owned(), v)).collect();
        unreviewed_cost(
            root.path(),
            &admitted(root.path(), &source),
            &plan,
            &inputs,
            None,
        )
    };
    let zero = judge(Some(serde_json::json!([])))
        .expect("zero work needs no review")
        .expect("the observer");
    assert!(zero.account.observes_declared_free_only());
    for items in [None, Some(serde_json::json!(["a", "b"]))] {
        let refused = judge(items).err().expect("positive work needs a review");
        assert!(refused.contains("price unknown"), "{refused}");
    }
}

/// C4 parity on the resident: an exact declared-free route binds the same
/// per-Run observer `nika run` binds, never unknown-cost authority.
#[test]
fn a_declared_free_route_binds_the_observer() {
    let free = "openrouter/nvidia/nemotron-3-super-120b-a12b:free";
    let observed = judge(
        free,
        "openrouter",
        AccessClass::Api,
        BillingClass::ApiMetered,
    );
    assert_eq!(observed, Ok(true));
}

mod standing {
    //! The same gate on a backend that holds the operator's standing cost
    //! policy: a Run a rule names is admitted with no review and its record
    //! says how; a route no rule names, a Run outside its rule and a ceiling
    //! an unknown price cannot fit under refuse; a second Run waits its turn.
    //! Nothing is dispatched.

    use std::collections::BTreeMap;
    use std::sync::Arc;

    use nika_providers::admission::{StandingCostPolicy, StandingRule};
    use nika_types::access::{AccessClass, BillingClass};
    use serde_json::json;

    use super::super::{UNREVIEWED, judge_unreviewed, unreviewed_cost};
    use super::{driver, plan_with};
    use crate::server::cost_policy::{Seat, StandingJob, UNNAMED};

    const MODEL: &str = "deepseek/c6-unpriced-fixture";

    fn rule(extra: &str) -> String {
        format!("{MODEL}:requests=4,output-tokens=64,timeout=30,runs=2/hour{extra}")
    }

    fn policy(rule: &str) -> StandingCostPolicy {
        let config = nika_runtime::compose::config_from_env();
        StandingCostPolicy::resolve(&[rule.to_owned()], &config).expect("the fixture rule")
    }

    fn id(rule: &str) -> String {
        StandingRule::parse(rule).expect("rule").id().to_owned()
    }

    fn plan() -> nika_providers::ExecutionAccessPlan {
        plan_with(
            MODEL,
            "deepseek",
            AccessClass::Api,
            BillingClass::ApiMetered,
        )
    }

    fn rows(root: &std::path::Path) -> Vec<serde_json::Value> {
        let journal = root.join(".nika").join(nika_dap::cost_journal::JOURNAL);
        std::fs::read_to_string(journal)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).expect("row"))
            .collect()
    }

    /// A Run the policy names is admitted with no review. What the job's
    /// record will say of it names the rule, its bounds and the evidence, and
    /// says the price is unknown: never a figure, never free.
    #[test]
    fn a_run_the_policy_names_is_admitted_and_its_record_says_how() {
        let root = tempfile::tempdir().expect("root");
        let text = rule("");
        let held = policy(&text);
        let job = StandingJob {
            policy: &held,
            ceiling: None,
            scheduled: false,
        };
        let driver = driver(root.path(), MODEL, 16);
        let cost = unreviewed_cost(root.path(), &driver, &plan(), &BTreeMap::new(), Some(&job))
            .expect("the rule admits it")
            .expect("its account");
        assert!(!cost.account.observes_declared_free_only());
        let admission = job.admission(&cost).expect("a standing admission");
        assert_eq!(admission["schema"], "nika/cost-admission@1");
        assert_eq!(admission["authority"], "standing_policy");
        assert_eq!(admission["price"], "unknown");
        assert_eq!(
            admission["choice"]["standing"],
            json!({"policy": id(&text), "max_runs": 2, "window_seconds": 3_600})
        );
        assert_eq!(admission["choice"]["provider"], "deepseek");
        assert_eq!(admission["choice"]["model"], "c6-unpriced-fixture");
        assert_eq!(
            admission["choice"]["origin"],
            "https://api.deepseek.com:443"
        );
        assert_eq!(admission["choice"]["max_requests"], 1);
        assert_eq!(admission["choice"]["max_output_tokens"], 64);
        assert_eq!(admission["choice"]["timeout_ms"], 30_000);
        assert!(admission["choice"]["declared_tariff"].is_null());
        assert!(
            admission["choice"].get("endpoint").is_none(),
            "a record names the route's origin, never its path"
        );
        assert_eq!(admission["host"], job.evidence().view());
        assert_eq!(admission["host"]["machine"]["cap"], "absent");
        let text_of = admission.to_string();
        assert!(
            !text_of.contains("free") && !text_of.contains("\"$0"),
            "{text_of}"
        );
        let prepared = rows(root.path());
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0]["phase"], "prepared");
        assert_eq!(
            prepared[0]["observation"]["unknown_cost"]["standing"]["policy"],
            id(&text)
        );
        cost.finish().expect("settled");
        drop(cost);
        assert_eq!(rows(root.path())[1]["phase"], "settled");
        // A priced or local job on the same backend is judged as without a policy.
        let priced = "deepseek/deepseek-flash";
        let other = plan_with(
            priced,
            "deepseek",
            AccessClass::Api,
            BillingClass::ApiMetered,
        );
        let priced_root = tempfile::tempdir().expect("root");
        let judged = unreviewed_cost(
            priced_root.path(),
            &super::driver(priced_root.path(), priced, 512),
            &other,
            &BTreeMap::new(),
            Some(&job),
        );
        assert!(
            judged
                .expect("a priced route keeps its composition")
                .is_none()
        );
    }

    /// A route no rule names refuses in the policy's words, and a server
    /// with no policy keeps its own, byte for byte.
    #[test]
    fn a_route_no_rule_names_is_refused_in_the_policys_words() {
        let elsewhere = policy(&rule("").replace(MODEL, "deepseek/another-unpriced-fixture"));
        let job = StandingJob {
            policy: &elsewhere,
            ceiling: None,
            scheduled: false,
        };
        let judge = |standing: Option<&StandingJob<'_>>| {
            let root = tempfile::tempdir().expect("root");
            let driver = driver(root.path(), MODEL, 16);
            let refused =
                unreviewed_cost(root.path(), &driver, &plan(), &BTreeMap::new(), standing)
                    .err()
                    .expect("no rule names this route");
            assert!(rows(root.path()).is_empty(), "nothing was prepared");
            let ledger = root
                .path()
                .join(".nika")
                .join(nika_dap::cost_journal::STANDING_LEDGER);
            assert!(!ledger.exists(), "nothing was counted");
            refused
        };
        assert_eq!(judge(Some(&job)), UNNAMED);
        assert_eq!(judge(None), UNREVIEWED);
        assert_eq!(
            UNREVIEWED,
            "price unknown: this Run's exact route needs a fresh one-time cost review, which a job without one never obtains (a server started with --cost-review admits POST /v1/cost-reviews, then one job carrying that review); scheduled occurrences stay refused"
        );
    }

    /// No ceiling is overridden: a rule without a price refuses under the
    /// job's ceiling and teaches its remedy; with a declared price the Run
    /// runs under that ceiling and its record says the price is the operator's.
    #[test]
    fn a_ceiling_refuses_an_unknown_price_and_admits_a_declared_worst_case() {
        let text = rule("");
        let held = policy(&text);
        let capped = StandingJob {
            policy: &held,
            ceiling: Some(1.0),
            scheduled: false,
        };
        let root = tempfile::tempdir().expect("root");
        let driver = driver(root.path(), MODEL, 16);
        let refused = unreviewed_cost(
            root.path(),
            &driver,
            &plan(),
            &BTreeMap::new(),
            Some(&capped),
        )
        .err()
        .expect("an unknown price fits under no ceiling");
        assert_eq!(
            refused,
            format!(
                "price unknown: a $1.000000 ceiling binds this job and the standing cost rule {} declares no price, so the ceiling cannot bound it: declare input-usd and output-usd on the rule, or start with --run-cost-ceiling none",
                id(&text)
            )
        );
        assert!(rows(root.path()).is_empty());
        let priced_text = rule(",request-bytes=2048,input-usd=1,output-usd=2");
        let priced = policy(&priced_text);
        let job = StandingJob {
            policy: &priced,
            ceiling: Some(1.0),
            scheduled: true,
        };
        let cost = unreviewed_cost(root.path(), &driver, &plan(), &BTreeMap::new(), Some(&job))
            .expect("the declared worst case fits under one dollar")
            .expect("its account");
        let admission = job.admission(&cost).expect("a standing admission");
        assert_eq!(admission["price"], "host_declared");
        assert_eq!(
            admission["choice"]["standing"]["declared_worst_case_nano_usd"],
            "2176000"
        );
        assert_eq!(admission["choice"]["declared_tariff"]["currency"], "USD");
        assert_eq!(
            admission["host"]["machine"]["cap"],
            json!({"capped_usd": "$1.000000"})
        );
        cost.finish().expect("settled");
        drop(cost);
        let tight = StandingJob {
            policy: &priced,
            ceiling: Some(0.001),
            scheduled: false,
        };
        assert_eq!(
            unreviewed_cost(
                root.path(),
                &driver,
                &plan(),
                &BTreeMap::new(),
                Some(&tight)
            )
            .err()
            .expect("the worst case exceeds the ceiling"),
            "the declared worst case of this Run ($0.002176: every request at its size and output bounds, at the declared rates) exceeds the $0.001000 ceiling it runs under; nothing was sent"
        );
    }

    /// One unknown-cost Run holds the project's cost lease at a time. On a
    /// backend that holds the policy a second Run a rule names waits for the
    /// first to settle and is then admitted, instead of failing on the lease.
    #[tokio::test]
    async fn a_second_run_the_policy_names_waits_for_the_first_to_settle() {
        let root = tempfile::tempdir().expect("root");
        let seat = Some(Arc::new(Seat::new(policy(&rule("")))));
        let (first_driver, second_driver) = (
            driver(root.path(), MODEL, 16),
            driver(root.path(), MODEL, 16),
        );
        let (inputs, plan) = (BTreeMap::new(), plan());
        let standing = (&seat, (None, false));
        let (first, first_turn) =
            judge_unreviewed(root.path(), &first_driver, &plan, &inputs, standing)
                .await
                .expect("the first is admitted");
        let first = first.expect("its account");
        assert!(first_turn.is_some(), "it holds its turn");
        let second = judge_unreviewed(root.path(), &second_driver, &plan, &inputs, standing);
        tokio::pin!(second);
        let waited = tokio::time::timeout(std::time::Duration::from_millis(150), &mut second).await;
        assert!(
            waited.is_err(),
            "the second waits while the first holds the lease"
        );
        first.finish().expect("the first settles");
        drop(first);
        drop(first_turn);
        let (second, second_turn) = second.await.expect("then the second is admitted");
        assert!(second_turn.is_some());
        second.expect("its account").finish().expect("settled");
        drop(second_turn);
        let phases: Vec<_> = rows(root.path())
            .iter()
            .map(|row| row["phase"].clone())
            .collect();
        assert_eq!(phases, ["prepared", "settled", "prepared", "settled"]);
        // The rule's window (two Runs an hour) is now full: the third is refused.
        let third = driver(root.path(), MODEL, 16);
        let refused = judge_unreviewed(root.path(), &third, &plan, &inputs, standing)
            .await
            .err()
            .expect("the window is full");
        assert!(
            refused.contains("admits 2 Runs of deepseek/c6-unpriced-fixture per 3600 seconds and 2 ran in that window"),
            "{refused}"
        );
    }
}
