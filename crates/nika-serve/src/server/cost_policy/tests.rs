// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The policy's own words on this server: the launch flag holds exactly the
//! rules it reads, the evidence names each layer as the server observed it,
//! and every refusal a job's record keeps fits that record's message bound.

use super::*;
use crate::server::bound_message;

const MODEL: &str = "deepseek/c6-unpriced-fixture";

fn rule(extra: &str) -> String {
    format!("{MODEL}:requests=4,output-tokens=64,timeout=30,runs=3/hour{extra}")
}

fn policy(rule: &str) -> StandingCostPolicy {
    let config = nika_runtime::compose::config_from_env();
    StandingCostPolicy::resolve(&[rule.to_owned()], &config).expect("the fixture rule")
}

fn listener() -> ServerConfig {
    let bind = "127.0.0.1:0".parse().expect("loopback");
    ServerConfig::new(bind, "workflows", "serve.token")
}

/// Why a launch does not hold `rules`.
fn refused(server: Option<ServerConfig>, rules: &[String]) -> CostPolicyError {
    seat_cost_policy(server, rules)
        .map(drop)
        .expect_err("a launch that holds no such rules")
}

#[test]
fn the_launch_flag_holds_exactly_the_rules_it_reads() {
    use nika_error::prelude::{NikaErrorCode as _, codes};
    assert!(seat_cost_policy(None, &[]).is_ok_and(|server| server.is_none()));
    let plain = seat_cost_policy(Some(listener()), &[]);
    let plain = plain.ok().flatten().expect("listener");
    assert!(plain.cost_policy().is_none(), "off unless asked");
    let lone = refused(None, &[rule("")]);
    assert_eq!(lone, CostPolicyError::NeedsListener);
    assert_eq!(
        lone.to_string(),
        "a standing cost policy admits the HTTP listener's jobs: pass --bind, --workflows and --token-file"
    );
    let unreadable = refused(Some(listener()), &["nope".to_owned()]);
    assert_eq!(
        unreadable.nika_code(),
        codes::NIKA_001,
        "a launch refusal, as every other"
    );
    assert!(
        matches!(unreadable, CostPolicyError::Rule(_)),
        "{unreadable:?}"
    );
    let unreadable = unreadable.to_string();
    assert!(
        unreadable.starts_with("standing cost rule \"nope\": write PROVIDER/MODEL:"),
        "{unreadable}"
    );
    let priced = rule("").replace(MODEL, "deepseek/deepseek-flash");
    let priced = refused(Some(listener()), &[priced]).to_string();
    assert!(
        priced.ends_with(
            "the catalog prices this route: its Runs already run under ordinary USD ceilings and need no standing rule"
        ),
        "{priced}"
    );
    let two = [
        rule(""),
        rule("").replace(MODEL, "deepseek/another-unpriced-fixture"),
    ];
    let seated = seat_cost_policy(Some(listener()), &two);
    let seated = seated.ok().flatten().expect("two rules on a listener");
    let held = seated.cost_policy().expect("the policy");
    let rules = held.view()["rules"].as_array().cloned().expect("rules");
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0]["model"], MODEL);
    assert_eq!(rules[0]["max_runs"], 3);
    assert_eq!(rules[0]["window_seconds"], 3_600);
    assert_eq!(rules[1]["model"], "deepseek/another-unpriced-fixture");
}

#[test]
fn the_evidence_names_each_layer_as_this_server_observed_it() {
    let held = policy(&rule(""));
    let job = |ceiling, scheduled| StandingJob {
        policy: &held,
        ceiling,
        scheduled,
    };
    assert_eq!(
        job(None, false).evidence().view(),
        json!({"allowed": true,
            "policy": {"class": "observed", "cap": "absent", "origin": "this server's standing cost policy (`nika serve --cost-policy`), read at startup: it bounds requests, output, time and Runs per window, and sets no USD cap of its own"},
            "machine": {"class": "observed", "cap": "absent", "origin": "no per-run ceiling binds this job: this server's was explicitly disarmed at startup (`--run-cost-ceiling none`) and the job carries none"},
            "occurrence": {"class": "not_applicable", "origin": "authenticated job request, not a scheduled occurrence"}})
    );
    let capped = job(Some(1.0), true).evidence().view();
    assert_eq!(
        capped["machine"],
        json!({"class": "observed", "cap": {"capped_usd": "$1.000000"}, "origin": "the per-run ceiling this job runs under ($1.000000): this server's `--run-cost-ceiling`, restricted by the job's own or its schedule's"})
    );
    assert_eq!(
        capped["occurrence"]["origin"],
        "scheduled occurrence: its required maxCostUsd is the per-run ceiling judged as the machine layer"
    );
    // A zero ceiling is a binding one; a ceiling that cannot be read is unknown.
    assert_eq!(
        job(Some(0.0), false).evidence().view()["machine"]["cap"],
        json!({"capped_usd": "$0.000000"})
    );
    assert_eq!(
        job(Some(f64::NAN), false).evidence().view()["machine"],
        json!({"class": "unknown"})
    );
    // The evidence carries the policy: a review of the named route is judged
    // under its rule.
    let config = nika_runtime::compose::config_from_env();
    let route = nika_providers::admission::CostRoute::observe(MODEL, config).expect("route");
    let named = held.rule_for(&route).expect("rule").id().to_owned();
    let review = nika_providers::admission::CostReview::new(
        "candidate".into(),
        "exe-evidence".into(),
        route,
        job(None, false).evidence(),
        None,
        None,
    )
    .expect("review");
    assert_eq!(
        review
            .standing_admission(64)
            .map(|rule| rule.map(|rule| rule.id().to_owned())),
        Ok(Some(named))
    );
}

#[test]
fn every_refusal_a_job_record_keeps_fits_its_message_bound() {
    let text = rule("");
    let held = policy(&text);
    let id = nika_providers::admission::StandingRule::parse(&text)
        .expect("rule")
        .id()
        .to_owned();
    let named = |job: &StandingJob<'_>| {
        let route = nika_providers::admission::CostRoute::observe(
            MODEL,
            nika_runtime::compose::config_from_env(),
        )
        .expect("route");
        job.capped(job.policy.rule_for(&route).expect("rule"))
    };
    let manual = named(&StandingJob {
        policy: &held,
        ceiling: Some(1.0),
        scheduled: false,
    });
    assert_eq!(
        manual,
        format!(
            "price unknown: a $1.000000 ceiling binds this job and the standing cost rule {id} declares no price, so the ceiling cannot bound it: declare input-usd and output-usd on the rule, or start with --run-cost-ceiling none"
        )
    );
    let scheduled = named(&StandingJob {
        policy: &held,
        ceiling: Some(0.05),
        scheduled: true,
    });
    assert_eq!(
        scheduled,
        format!(
            "price unknown: a $0.050000 ceiling binds this job and the standing cost rule {id} declares no price, so the ceiling cannot bound it: a schedule always carries maxCostUsd, so its rule must declare input-usd and output-usd"
        )
    );
    for kept in [manual.as_str(), scheduled.as_str(), UNNAMED] {
        assert_eq!(bound_message(kept), kept, "the record keeps it whole");
        assert!(kept.len() <= 240, "{}", kept.len());
    }
    assert!(UNNAMED.contains("admits POST /v1/cost-reviews, then one job"));
}
