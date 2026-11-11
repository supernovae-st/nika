// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The standing cost policy's law, hermetically: how a rule reads, which
//! routes a policy may hold, what a review confirmed under a rule records and
//! refuses, that no ceiling is ever overridden, and what the bounded account
//! does over a controlled HTTP seam. No provider is called.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
use super::*;
use crate::admission::{
    AdmissionState, CapEvidence, CostHostEvidence, CostReview, HardMonetaryCap, InferenceAdmission,
    UnknownCostChoice,
};
use crate::{ProviderRegistry, ProvidersConfig};
use nika_kernel::ai::provider::{InferRequest, Message, ProviderInferDyn, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_types::cost::Cost;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const MODEL: &str = "scaleway/deepseek-v4-flash-0731";
const ENDPOINT: &str = "https://api.scaleway.ai/v1/chat/completions";
const RULE: &str =
    "scaleway/deepseek-v4-flash-0731:requests=4,output-tokens=4096,timeout=60,runs=200/day";
const PRICED: &str = "scaleway/deepseek-v4-flash-0731:requests=4,output-tokens=4096,timeout=60,runs=200/day,request-bytes=32768,input-usd=0.15,output-usd=0.6";

fn policy(rule: &str) -> StandingCostPolicy {
    StandingCostPolicy::resolve(&[rule.to_owned()], &ProvidersConfig::new()).expect("policy")
}

fn route() -> CostRoute {
    CostRoute::observe(MODEL, ProvidersConfig::new()).expect("the fixture route")
}

fn absent(origin: &str) -> CapEvidence {
    CapEvidence::Observed {
        cap: HardMonetaryCap::Absent,
        origin: origin.into(),
    }
}

/// A host's evidence: unknown cost permitted, no policy or occurrence cap,
/// the machine layer capped at `ceiling` nano-USD or observed absent.
fn evidence(ceiling: Option<i128>) -> CostHostEvidence {
    let machine = ceiling.map_or_else(
        || absent("ceiling disarmed"),
        |nano| CapEvidence::Observed {
            cap: HardMonetaryCap::Capped(Cost::new(nano)),
            origin: "per-run ceiling".into(),
        },
    );
    CostHostEvidence::new(true, absent("policy"), machine, absent("occurrence"))
}

fn review(evidence: CostHostEvidence, defaults: [Option<Cost>; 2]) -> Result<CostReview, String> {
    CostReview::new(
        "candidate".into(),
        "exe-standing".into(),
        route(),
        evidence,
        defaults[0],
        defaults[1],
    )
}

/// The confirmed account's choice as it serializes.
fn choice_of(account: &InferenceAdmission) -> Value {
    serde_json::to_value(account.snapshot().expect("receipt").unknown_cost).expect("choice")
}

#[test]
fn a_rule_reads_its_bounds_and_names_itself_by_its_canonical_form() {
    let rule = StandingRule::parse(RULE).expect("rule");
    assert_eq!(rule.model, MODEL);
    assert_eq!((rule.max_runs(), rule.window().as_secs()), (200, 86_400));
    assert!(!rule.declares_price());
    assert_eq!(
        rule.view(),
        json!({"policy": rule.id(), "model": MODEL, "max_requests": 4,
            "max_output_tokens": 4096, "request_timeout_seconds": 60,
            "max_request_bytes": 1_048_576, "max_runs": 200, "window_seconds": 86_400,
            "price": {"state": "unknown"}})
    );
    let id = rule.id();
    assert!(id.starts_with("scp-") && id.len() == 20, "{id}");
    assert!(id[4..].bytes().all(|b| b.is_ascii_hexdigit()), "{id}");
    // The identity digests the canonical form: order, spacing and the window's
    // spelling do not change it; any bound does.
    for same in [
        "scaleway/deepseek-v4-flash-0731:runs=200/86400s,timeout=60,output-tokens=4096,requests=4",
        "scaleway/deepseek-v4-flash-0731: requests=4 , output-tokens=4096, timeout=60, runs = 200/day",
    ] {
        assert_eq!(StandingRule::parse(same).expect(same).id(), id, "{same}");
    }
    for other in [
        RULE.replace("requests=4", "requests=5"),
        RULE.replace("200/day", "200/hour"),
        format!("{RULE},request-bytes=1024"),
        PRICED.to_owned(),
    ] {
        assert_ne!(
            StandingRule::parse(&other).expect(&other).id(),
            id,
            "{other}"
        );
    }
    let priced = StandingRule::parse(PRICED).expect("priced");
    assert!(priced.declares_price());
    assert_eq!(priced.view()["max_request_bytes"], 32_768);
    assert_eq!(
        priced.view()["price"],
        json!({"state": "host_declared", "currency": "USD", "input_per_million": 0.15,
            "output_per_million": 0.6,
            "basis": "operator-declared in the host's standing cost policy; an estimate, never a provider invoice"})
    );
    // Periods, and a model whose own name holds a colon.
    for (period, seconds) in [
        ("minute", 60),
        ("hour", 3_600),
        ("week", 604_800),
        ("90s", 90),
    ] {
        let rule = StandingRule::parse(&RULE.replace("200/day", &format!("7/{period}")));
        let rule = rule.expect(period);
        assert_eq!((rule.max_runs(), rule.window().as_secs()), (7, seconds));
    }
    let colon = "openrouter/vendor/model:free:requests=1,output-tokens=16,timeout=5,runs=1/day";
    assert_eq!(
        StandingRule::parse(colon).expect("colon").model,
        "openrouter/vendor/model:free"
    );
}

#[test]
fn a_rule_that_does_not_read_is_refused_in_its_own_words() {
    let with = |old: &str, new: &str| RULE.replace(old, new);
    for (text, why) in [
        (
            "scaleway/model".to_owned(),
            "write PROVIDER/MODEL:requests=N",
        ),
        (with("scaleway/", ""), "name the route as provider/model"),
        (
            with("scaleway/deepseek", "scaleway/deep seek"),
            "name the route as provider/model",
        ),
        (with(",timeout=60", ""), "`timeout` is required"),
        (with("requests=4,", ""), "`requests` is required"),
        (
            with("output-tokens=4096,", ""),
            "`output-tokens` is required",
        ),
        (with(",runs=200/day", ""), "`runs` is required"),
        (
            with("requests=4", "requests=0"),
            "`requests` takes a whole number from 1 to 256",
        ),
        (
            with("requests=4", "requests=257"),
            "`requests` takes a whole number from 1 to 256",
        ),
        (
            with("output-tokens=4096", "output-tokens=8193"),
            "`output-tokens` takes a whole number from 1 to 8192",
        ),
        (
            with("timeout=60", "timeout=121"),
            "`timeout` takes a whole number from 1 to 120",
        ),
        (
            with("timeout=60", "timeout=1.5"),
            "`timeout` takes a whole number from 1 to 120",
        ),
        (with("200/day", "200"), "`runs` takes N/PERIOD"),
        (with("200/day", "0/day"), "`runs` takes N/PERIOD"),
        (with("200/day", "100001/day"), "`runs` takes N/PERIOD"),
        (with("200/day", "200/year"), "`runs` takes N/PERIOD"),
        (with("200/day", "200/59s"), "`runs` takes N/PERIOD"),
        (with("200/day", "200/2678401s"), "`runs` takes N/PERIOD"),
        (format!("{RULE},requests=4"), "`requests` is set twice"),
        (format!("{RULE},burst=9"), "`burst` is not a setting"),
        (format!("{RULE},unbounded"), "`unbounded` is not key=value"),
        (
            format!("{RULE},request-bytes=1048577"),
            "`request-bytes` takes a whole number from 1 to 1048576",
        ),
        (
            format!("{RULE},input-usd=0.1"),
            "declare `input-usd` and `output-usd` together",
        ),
        (
            format!("{RULE},output-usd=0.1"),
            "declare `input-usd` and `output-usd` together",
        ),
        (
            format!("{RULE},input-usd=0,output-usd=0"),
            "a declared price states a positive rate",
        ),
        (
            format!("{RULE},input-usd=-1,output-usd=1"),
            "`input-usd` takes a USD amount per million tokens from 0 to 1000000",
        ),
        (
            format!("{RULE},input-usd=1,output-usd=nan"),
            "`output-usd` takes a USD amount per million tokens from 0 to 1000000",
        ),
        (
            format!("{RULE},input-usd=1000001,output-usd=1"),
            "`input-usd` takes a USD amount per million tokens from 0 to 1000000",
        ),
    ] {
        let refused = StandingRule::parse(&text).expect_err(&text);
        assert!(
            refused.starts_with(&format!("standing cost rule {text:?}: ")),
            "{refused}"
        );
        assert!(refused.contains(why), "{text}: {refused}");
    }
}

#[test]
fn a_policy_holds_only_unpriced_routes_and_each_one_once() {
    let none = StandingCostPolicy::resolve(&[], &ProvidersConfig::new());
    assert_eq!(
        none.expect_err("no rule"),
        "a standing cost policy names at least one rule"
    );
    let priced = RULE.replace(MODEL, "deepseek/deepseek-v4-pro");
    let refused = StandingCostPolicy::resolve(&[priced], &ProvidersConfig::new())
        .expect_err("the catalog prices it");
    assert!(
        refused.ends_with(
            "the catalog prices this route: its Runs already run under ordinary USD ceilings and need no standing rule"
        ),
        "{refused}"
    );
    let native = RULE.replace(MODEL, "anthropic/claude-sonnet-4-20250514");
    let refused = StandingCostPolicy::resolve(&[native], &ProvidersConfig::new())
        .expect_err("no unknown-cost admission on this wire");
    assert!(
        refused.contains("unknown-cost admission supports HTTPS OpenAI-compatible text only"),
        "{refused}"
    );
    let twice = [RULE.to_owned(), RULE.replace("requests=4", "requests=2")];
    let refused =
        StandingCostPolicy::resolve(&twice, &ProvidersConfig::new()).expect_err("one route twice");
    assert!(
        refused.ends_with("another rule already names this route"),
        "{refused}"
    );
    let malformed = StandingCostPolicy::resolve(&["nope".to_owned()], &ProvidersConfig::new());
    assert!(
        malformed
            .expect_err("unreadable")
            .starts_with("standing cost rule \"nope\": ")
    );
    // A policy names exact routes: another model, or the same model at an
    // endpoint the host did not have when it composed the policy, is not named.
    let held = policy(RULE);
    let rule = held.rule_for(&route()).expect("the named route");
    assert_eq!(rule.model, MODEL);
    let sibling = CostRoute::observe("scaleway/other-unpriced-model", ProvidersConfig::new());
    assert!(held.rule_for(&sibling.expect("route")).is_none());
    let moved = ProvidersConfig::new()
        .with_base_url("scaleway", "https://gateway.example/v1/chat/completions");
    let elsewhere = CostRoute::observe(MODEL, moved).expect("route");
    assert!(
        held.rule_for(&elsewhere).is_none(),
        "the endpoint is part of the route"
    );
    assert_eq!(
        held.view(),
        json!({"schema": "nika/standing-cost-policy@1", "rules": [rule.view()]})
    );
    let two = [
        RULE.to_owned(),
        RULE.replace(MODEL, "scaleway/other-unpriced-model"),
    ];
    let both = StandingCostPolicy::resolve(&two, &ProvidersConfig::new()).expect("two routes");
    assert_eq!(both.view()["rules"].as_array().map(Vec::len), Some(2));
}

/// Without a standing rule for its route a review is exactly the historical
/// one: the same bounds, the same serialized choice, nothing a host could
/// confirm without a fresh decision.
#[test]
fn a_review_no_rule_names_keeps_its_historical_choice() {
    let historical = json!({"candidate": "candidate", "invocation": "exe-standing",
        "provider": "scaleway", "model": "deepseek-v4-flash-0731", "endpoint": ENDPOINT,
        "max_requests": 1, "max_output_tokens": 8192, "timeout_ms": 120_000,
        "declared_tariff": null});
    let elsewhere = policy(&RULE.replace(MODEL, "scaleway/other-unpriced-model"));
    for evidence in [
        evidence(None),
        evidence(None).with_standing_policy(elsewhere),
    ] {
        assert_eq!(evidence.unknown_cost_refusal_on(&route()), None);
        let review = review(evidence, [None, Some(Cost::new(5_000_000_000))]).expect("review");
        assert_eq!(review.standing_admission(8192), Ok(None));
        assert_eq!(
            review.bounds(),
            (1, 8192, std::time::Duration::from_secs(120))
        );
        let account = review.confirm("candidate", &route()).expect("confirmed");
        assert_eq!(choice_of(&account), historical);
        let receipt = account.snapshot().expect("receipt");
        assert_eq!(
            receipt.overridden_defaults,
            [None, Some(Cost::new(5_000_000_000))],
            "a fresh decision overrides the shown defaults"
        );
        let choice = receipt.unknown_cost.expect("choice");
        assert_eq!(choice.standing_policy(), None);
        assert_eq!(choice.declared_worst_case(), None);
    }
}

/// Inside its rule a review is confirmed with the rule's own bounds, and the
/// account's choice records which rule admitted it, in memory and durably.
#[test]
fn a_review_inside_its_rule_is_confirmed_and_records_the_rule() {
    let held = policy(RULE);
    let id = held.rule_for(&route()).expect("rule").id().to_owned();
    let review = review(evidence(None).with_standing_policy(held), [None, None])
        .expect("review")
        .for_run(3)
        .expect("three requests");
    assert_eq!(
        review
            .standing_admission(4096)
            .map(|rule| rule.map(StandingRule::id)),
        Ok(Some(id.as_str())),
        "the review is judged under the rule that names its route"
    );
    assert_eq!(
        review.bounds(),
        (3, 4096, std::time::Duration::from_secs(60)),
        "the rule narrows the Run review's output and time bounds"
    );
    let admitted = review.standing_admission(4096).expect("inside the rule");
    assert_eq!(admitted.map(StandingRule::id), Some(id.as_str()));
    let account = review.confirm("candidate", &route()).expect("confirmed");
    assert_eq!(
        choice_of(&account),
        json!({"candidate": "candidate", "invocation": "exe-standing",
            "provider": "scaleway", "model": "deepseek-v4-flash-0731", "endpoint": ENDPOINT,
            "max_requests": 3, "max_output_tokens": 4096, "timeout_ms": 60_000,
            "declared_tariff": null,
            "standing": {"policy": id, "max_runs": 200, "window_seconds": 86_400}})
    );
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.overridden_defaults, [None, None]);
    let choice = receipt.unknown_cost.clone().expect("choice");
    assert_eq!(choice.standing_policy(), Some(id.as_str()));
    assert_eq!(choice.declared_worst_case(), None, "no price was declared");
    // The durable form every record keeps names the route by origin and keeps the rule.
    let durable = receipt.durable_observation();
    assert_eq!(durable["schema"], "nika/inference-cost-observation@2");
    assert_eq!(
        durable["unknown_cost"]["standing"],
        json!({"policy": id, "max_runs": 200, "window_seconds": 86_400})
    );
    assert_eq!(
        durable["unknown_cost"]["origin"],
        "https://api.scaleway.ai:443"
    );
    assert_eq!(durable["withheld"], json!([]));
    assert!(
        crate::admission::observation_readable(&durable),
        "the journal reads what it wrote"
    );
}

#[test]
fn a_run_outside_its_rule_is_refused_in_the_rules_words() {
    let held = policy(RULE);
    let id = held.rule_for(&route()).expect("rule").id().to_owned();
    let standing = || evidence(None).with_standing_policy(held.clone());
    let wide = review(standing(), [None, None])
        .expect("review")
        .for_run(5)
        .expect("five requests");
    assert_eq!(
        wide.standing_admission(4096).expect_err("five requests"),
        format!(
            "this Run may send 5 requests and the standing cost rule {id} admits at most 4 per Run on {MODEL}; nothing was sent"
        )
    );
    assert!(
        wide.confirm("candidate", &route()).is_err(),
        "no account exists for a Run outside its rule"
    );
    let tall = review(standing(), [None, None]).expect("review");
    assert_eq!(
        tall.standing_admission(4097).expect_err("too much output"),
        format!(
            "this Run asks up to 4097 output tokens per request and the standing cost rule {id} admits at most 4096 on {MODEL}; nothing was sent"
        )
    );
    let exact = review(standing(), [None, None])
        .expect("review")
        .for_run(4)
        .expect("four requests");
    assert!(
        exact
            .standing_admission(4096)
            .expect("the bound itself")
            .is_some()
    );
}

/// A standing rule overrides no ceiling. Without a declared price an unknown
/// price fits under none: a hard cap, an invocation default and a project
/// default all refuse, as do a host that does not permit unknown cost and a
/// layer it did not observe.
#[test]
fn a_rule_without_a_price_fits_under_no_ceiling() {
    let held = policy(RULE);
    let capped = evidence(Some(1_000_000_000)).with_standing_policy(held.clone());
    assert_eq!(
        capped.unknown_cost_refusal_on(&route()),
        capped.unknown_cost_refusal(),
        "the cap's own refusal, which the host teaches its remedy for"
    );
    assert!(capped.unknown_cost_refusal().is_some());
    assert_eq!(
        review(capped, [None, None]).expect_err("a hard cap"),
        "a $1.000000 ceiling binds this Run and its standing rule declares no price: an unknown price fits under no ceiling; nothing was sent"
    );
    for defaults in [
        [Some(Cost::new(2_000_000_000)), None],
        [None, Some(Cost::new(2_000_000_000))],
    ] {
        let standing = evidence(None).with_standing_policy(held.clone());
        assert_eq!(
            review(standing, defaults).expect_err("a default is a ceiling too"),
            "a $2.000000 ceiling binds this Run and its standing rule declares no price: an unknown price fits under no ceiling; nothing was sent"
        );
    }
    let unobserved = CostHostEvidence::new(
        true,
        absent("policy"),
        CapEvidence::Unknown,
        absent("occurrence"),
    );
    let denied = CostHostEvidence::new(
        false,
        absent("policy"),
        absent("machine"),
        absent("occurrence"),
    );
    for evidence in [unobserved, denied] {
        let standing = evidence.with_standing_policy(held.clone());
        assert!(standing.unknown_cost_refusal_on(&route()).is_some());
        assert_eq!(
            review(standing, [None, None]).expect_err("no authority"),
            "unknown cost is unavailable: the host must permit it and have observed every policy, machine and occurrence cap"
        );
    }
}

/// With a declared price the Run's worst case is known before any request:
/// every request at its size and output bounds, at the declared rates. A
/// ceiling admits the Run only when that worst case fits under it.
#[test]
fn a_declared_price_admits_a_run_whose_worst_case_fits_every_ceiling() {
    let held = policy(PRICED);
    let id = held.rule_for(&route()).expect("rule").id().to_owned();
    // 32,768 bytes at 150 nano-USD and 4,096 output tokens at 600 nano-USD.
    let per_request: i128 = 32_768 * 150 + 4_096 * 600;
    assert_eq!(per_request, 7_372_800);
    let standing = |ceiling: Option<i128>| evidence(ceiling).with_standing_policy(held.clone());
    assert_eq!(
        standing(Some(1_000_000_000)).unknown_cost_refusal_on(&route()),
        None,
        "a declared price leaves a present ceiling to the review"
    );
    let fits = review(standing(Some(1_000_000_000)), [None, None])
        .expect("one request fits")
        .for_run(4)
        .expect("four requests");
    assert!(
        fits.standing_admission(4096)
            .expect("four fit too")
            .is_some()
    );
    let account = fits.confirm("candidate", &route()).expect("confirmed");
    let choice = choice_of(&account);
    assert_eq!(choice["max_request_bytes"], 32_768);
    assert_eq!(
        choice["standing"],
        json!({"policy": id, "max_runs": 200, "window_seconds": 86_400,
            "declared_worst_case_nano_usd": "29491200"})
    );
    assert_eq!(
        choice["declared_tariff"],
        json!({"provider": "scaleway", "model": "deepseek-v4-flash-0731", "endpoint": ENDPOINT,
            "billing_provider": "scaleway", "currency": "USD", "unit": "per_million_tokens",
            "nano_per_token": [150, 600, 150],
            "provenance": "operator-declared in the host's standing cost policy; an estimate, never a provider invoice",
            "version": id})
    );
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(
        receipt
            .unknown_cost
            .as_ref()
            .and_then(UnknownCostChoice::declared_worst_case),
        Some(Cost::new(4 * per_request))
    );
    assert_eq!(receipt.overridden_defaults, [None, None]);
    // A default the Run fits under is honoured, never recorded as overridden,
    // where a fresh decision records the defaults it overrode.
    let five = Some(Cost::new(5_000_000_000));
    let under_default = review(standing(None), [None, five])
        .expect("one request fits under five dollars")
        .confirm("candidate", &route())
        .expect("confirmed");
    let receipt_under = under_default.snapshot().expect("receipt");
    assert_eq!(
        receipt_under.overridden_defaults,
        [None, None],
        "a standing rule overrides no default"
    );
    let durable = receipt.durable_observation();
    assert_eq!(durable["unknown_cost"]["max_request_bytes"], 32_768);
    assert_eq!(
        durable["unknown_cost"]["standing"]["declared_worst_case_nano_usd"],
        "29491200"
    );
    assert!(crate::admission::observation_readable(&durable));
    // The ceiling is exact: the worst case itself fits, one nano-USD less does not.
    let four = |ceiling: i128| {
        review(standing(Some(ceiling)), [None, None])
            .expect("one request fits")
            .for_run(4)
            .expect("four requests")
    };
    assert!(four(4 * per_request).standing_admission(4096).is_ok());
    assert!(four(4 * per_request - 1).standing_admission(4096).is_err());
    let short = four(20_000_000);
    assert_eq!(
        short
            .standing_admission(4096)
            .expect_err("four requests do not fit"),
        "the declared worst case of this Run ($0.029491: every request at its size and output bounds, at the declared rates) exceeds the $0.020000 ceiling it runs under; nothing was sent"
    );
    assert!(short.confirm("candidate", &route()).is_err());
    // A project default is a ceiling a standing Run must fit under, never an
    // overridable one; a single request that does not fit refuses at once.
    let default = [None, Some(Cost::new(5_000_000))];
    assert_eq!(
        review(standing(None), default).expect_err("the project ceiling"),
        "the declared worst case of this Run ($0.007372: every request at its size and output bounds, at the declared rates) exceeds the $0.005000 ceiling it runs under; nothing was sent"
    );
    assert!(review(standing(None), [None, Some(Cost::new(per_request))]).is_ok());
    assert!(review(standing(None), [None, Some(Cost::new(per_request - 1))]).is_err());
    // A zero ceiling vetoes every positive worst case.
    assert!(review(standing(Some(0)), [None, None]).is_err());
    // Without a request-size bound the worst case counts the 1 MiB of every
    // admitted request.
    let unbounded = policy(&PRICED.replace(",request-bytes=32768", ""));
    let review = review(evidence(None).with_standing_policy(unbounded), [None, None]);
    let account = review
        .expect("review")
        .confirm("candidate", &route())
        .expect("confirmed");
    assert_eq!(
        account
            .snapshot()
            .expect("receipt")
            .unknown_cost
            .and_then(|choice| choice.declared_worst_case()),
        Some(Cost::new(1_048_576 * 150 + 4_096 * 600))
    );
}

/// The durable schema stays closed around the rule. A record keeps the rule
/// and the size bound as the account wrote them, and one that carries
/// anything else there does not read at all.
#[test]
fn a_durable_record_keeps_the_rule_and_refuses_anything_else_there() {
    type Break = fn(&mut Value);
    let cases: [(&str, Break); 6] = [
        ("an unknown key in the rule", |o| {
            o["unknown_cost"]["standing"]["extra"] = json!(1);
        }),
        ("a rule with no identity", |o| {
            o["unknown_cost"]["standing"]
                .as_object_mut()
                .map(|rule| rule.remove("policy"));
        }),
        ("a window count written as text", |o| {
            o["unknown_cost"]["standing"]["max_runs"] = json!("200");
        }),
        ("a worst case written as a number", |o| {
            o["unknown_cost"]["standing"]["declared_worst_case_nano_usd"] = json!(7_372_800);
        }),
        ("a rule that is not an object", |o| {
            o["unknown_cost"]["standing"] = json!("scp-0000000000000000");
        }),
        ("a size bound written as text", |o| {
            o["unknown_cost"]["max_request_bytes"] = json!("32768");
        }),
    ];
    let held = policy(PRICED);
    let id = held.rule_for(&route()).expect("rule").id().to_owned();
    let review = review(evidence(None).with_standing_policy(held), [None, None]).expect("review");
    let account = review.confirm("candidate", &route()).expect("confirmed");
    let durable = account.snapshot().expect("receipt").durable_observation();
    assert_eq!(durable["unknown_cost"]["max_request_bytes"], 32_768);
    assert_eq!(
        durable["unknown_cost"]["standing"],
        json!({"policy": id, "max_runs": 200, "window_seconds": 86_400,
            "declared_worst_case_nano_usd": "7372800"})
    );
    let project = crate::route_identity::project_observation;
    assert_eq!(project(&durable).as_ref(), Some(&durable));
    for (name, break_it) in cases {
        let mut broken = durable.clone();
        break_it(&mut broken);
        assert_eq!(project(&broken), None, "{name}");
        assert!(!crate::admission::observation_readable(&broken), "{name}");
    }
}

/// What one post answers.
enum Reply {
    /// A complete answer naming `model`, reporting these input tokens.
    Answer { model: &'static str, input: u64 },
    /// An HTTP status with no usable body.
    Status(u16),
}

struct Wire {
    posts: AtomicUsize,
    reply: Reply,
}

impl Wire {
    fn new(reply: Reply) -> Arc<Self> {
        Arc::new(Self {
            posts: AtomicUsize::new(0),
            reply,
        })
    }
    fn posts(&self) -> usize {
        self.posts.load(Ordering::SeqCst)
    }
}

impl HttpPostDyn for Wire {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.posts.fetch_add(1, Ordering::SeqCst);
        assert_eq!(req.url, ENDPOINT);
        assert!(!req.follow_redirects);
        assert!(
            req.timeout
                .is_some_and(|t| t <= std::time::Duration::from_secs(60)),
            "the rule's per-request deadline bounds the transport"
        );
        let (status, body) = match self.reply {
            Reply::Answer { model, input } => (
                200,
                json!({"id": "standing-fixture", "model": model,
                    "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": input, "completion_tokens": 20,
                        "total_tokens": input + 20}})
                .to_string(),
            ),
            Reply::Status(status) => (status, "{}".to_owned()),
        };
        Ok(HttpResponse::new(
            status,
            std::collections::BTreeMap::new(),
            body.into(),
            &req.url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("a standing Run never streams");
    }
}

/// The account a host confirms for a three-request Run under `rule`, and the
/// provider that dispatches through `wire` under it.
fn standing_provider(
    rule: &str,
    wire: &Arc<Wire>,
) -> (InferenceAdmission, crate::ResolvedProvider<Wire>) {
    let review = review(
        evidence(None).with_standing_policy(policy(rule)),
        [None, None],
    )
    .expect("review")
    .for_run(3)
    .expect("three requests");
    review.standing_admission(64).expect("inside the rule");
    let account = review.confirm("candidate", &route()).expect("confirmed");
    let provider = ProviderRegistry::new(
        Arc::clone(wire),
        ProvidersConfig::new().with_key("scaleway", Secret::new("fixture")),
    )
    .with_inference_admission(account.clone())
    .resolve(MODEL)
    .expect("the fixture route resolves");
    (account, provider)
}

fn request(prompt: &str) -> InferRequest {
    let mut request = InferRequest::new(MODEL, vec![Message::text(Role::User, prompt)]);
    request.max_tokens = Some(64);
    request
}

/// A Run a rule admitted dispatches inside the rule's bounds and stops at
/// them. Its calls are recorded as sent with an unknown USD cost, never as
/// zero, and the request beyond the Run's total never reaches the transport.
#[tokio::test]
async fn a_standing_account_stops_at_its_bounds_and_never_prices_what_it_cannot() {
    let wire = Wire::new(Reply::Answer {
        model: "deepseek-v4-flash-0731",
        input: 100,
    });
    let (account, provider) = standing_provider(RULE, &wire);
    let mut oversized = request("hi");
    oversized.max_tokens = Some(4097);
    assert!(
        provider.infer(oversized).await.is_err(),
        "output above the rule"
    );
    assert_eq!(
        wire.posts(),
        0,
        "refused before any byte, and nothing reserved"
    );
    for sent in 1..=3 {
        provider
            .infer(request("hi"))
            .await
            .expect("inside the bound");
        assert_eq!(wire.posts(), sent);
    }
    let refused = provider.infer(request("hi")).await.expect_err("the fourth");
    assert!(
        refused
            .to_string()
            .contains("unknown-cost request/output/concurrency bound exhausted"),
        "{refused}"
    );
    assert_eq!(
        wire.posts(),
        3,
        "nothing left for the request beyond the total"
    );
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Open);
    assert_eq!(receipt.unknown_calls, 3);
    assert_eq!(
        receipt.estimated,
        Cost::zero(),
        "a subtotal of known costs only"
    );
    let observation = receipt.observation();
    assert_eq!(observation["known_subtotal_nano_usd"], "0");
    assert_eq!(observation["unknown_calls"], 3);
    assert!(
        observation["limit_nano_usd"].is_null(),
        "no USD limit is claimed"
    );
    for attempt in observation["unknown_attempts"]
        .as_array()
        .expect("attempts")
    {
        assert_eq!(attempt["sent"], true);
        assert!(
            attempt["estimated_nano_usd"].is_null(),
            "unknown, never zero"
        );
        assert_eq!(attempt["note"], "completed; USD cost unknown");
        assert_eq!(attempt["pricing"]["kind"], "unknown");
    }
}

/// A rule's request-size bound is enforced before any reservation or byte.
#[tokio::test]
async fn a_request_above_the_rules_size_bound_is_refused_before_any_byte() {
    let wire = Wire::new(Reply::Answer {
        model: "deepseek-v4-flash-0731",
        input: 100,
    });
    let (account, provider) = standing_provider(&format!("{RULE},request-bytes=400"), &wire);
    let refused = provider
        .infer(request(&"x".repeat(600)))
        .await
        .expect_err("a 600-byte prompt");
    let refused = refused.to_string();
    assert!(
        refused.contains(
            "exceeds the 400 bytes its standing cost rule admits per request; nothing was sent"
        ),
        "{refused}"
    );
    assert_eq!(wire.posts(), 0);
    let receipt = account.snapshot().expect("receipt");
    assert!(receipt.unknown_attempts.is_empty(), "nothing was reserved");
    assert_eq!(receipt.state, AdmissionState::Open);
    provider
        .infer(request("hi"))
        .await
        .expect("a small request still runs");
    assert_eq!(wire.posts(), 1);
}

/// The ambiguous-admission rule protects a standing Run as it protects a
/// reviewed one: a request that left without a usable answer leaves the
/// account Uncertain, its attempt recorded as possibly billed, and nothing
/// more is sent.
#[tokio::test]
async fn a_request_that_left_without_a_usable_answer_blocks_the_standing_account() {
    let wire = Wire::new(Reply::Status(500));
    let (account, provider) = standing_provider(RULE, &wire);
    assert!(provider.infer(request("hi")).await.is_err());
    assert_eq!(wire.posts(), 1);
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Uncertain);
    assert_eq!(receipt.unknown_calls, 1);
    assert!(receipt.unknown_attempts[0].sent);
    assert_eq!(
        receipt.unknown_attempts[0].note,
        "possibly billed; no automatic retry"
    );
    assert_eq!(
        receipt.refusal.as_deref(),
        Some("sent request ended without usable settlement; unknown charge retained")
    );
    assert!(
        provider.infer(request("hi")).await.is_err(),
        "two requests remained"
    );
    assert_eq!(
        wire.posts(),
        1,
        "no further dispatch under an Uncertain account"
    );
    assert_eq!(receipt.durable_observation()["state"], "Uncertain");
}

/// A declared price is recorded as the operator's estimate on every attempt.
/// This route's usage receipt is not one the engine qualifies as complete, so
/// no call is priced on it: the cost stays unknown, never the estimate and
/// never zero. A reply that reports more input tokens than the rule's
/// request-size bound contradicts what the declared worst case rests on, and
/// blocks.
#[tokio::test]
async fn a_declared_price_is_recorded_as_an_estimate_and_a_reply_over_its_bound_blocks() {
    let wire = Wire::new(Reply::Answer {
        model: "deepseek-v4-flash-0731",
        input: 100,
    });
    let (account, provider) = standing_provider(PRICED, &wire);
    provider.infer(request("hi")).await.expect("answered");
    let receipt = account.snapshot().expect("receipt");
    let attempt = &receipt.unknown_attempts[0];
    assert_eq!(
        attempt.pricing["kind"],
        "user_declared_estimate_not_invoice"
    );
    assert_eq!(attempt.pricing["nano_per_token"], json!([150, 600, 150]));
    assert_eq!(attempt.estimated, None, "unqualified usage prices nothing");
    assert_eq!(attempt.note, "completed; USD cost unknown");
    assert_eq!(receipt.state, AdmissionState::Open);
    let over = Wire::new(Reply::Answer {
        model: "deepseek-v4-flash-0731",
        input: 32_769,
    });
    let (account, provider) = standing_provider(PRICED, &over);
    let refused = provider
        .infer(request("hi"))
        .await
        .expect_err("over the bound");
    assert!(
        refused.to_string().contains(
            "the reply reports more input tokens than the request-size bound of its standing rule: the rule's declared worst case no longer holds"
        ),
        "{refused}"
    );
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Uncertain);
    assert_eq!(receipt.unknown_attempts[0].estimated, None);
    assert!(provider.infer(request("hi")).await.is_err());
    assert_eq!(over.posts(), 1, "nothing is sent after the contradiction");
    // The same reply under a rule that declares no price contradicts nothing.
    let unpriced = Wire::new(Reply::Answer {
        model: "deepseek-v4-flash-0731",
        input: 32_769,
    });
    let (account, provider) = standing_provider(RULE, &unpriced);
    provider
        .infer(request("hi"))
        .await
        .expect("no declared worst case to hold");
    assert_eq!(
        account.snapshot().expect("receipt").state,
        AdmissionState::Open
    );
}
