// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The shared evaluator under an operator's standing cost policy: a Run inside
//! a rule is admitted with no decision and every row names the rule; a Run
//! outside it, a route no rule names and a full window refuse; no ceiling is
//! overridden; an earlier Run that may have been billed still blocks; and the
//! journal a standing host fills is archived, never dropped. Controlled
//! transport only: no provider is called.

use super::*;
use nika_kernel::ai::provider::{InferRequest, Message, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_providers::admission::{CapEvidence, HardMonetaryCap, StandingCostPolicy};
use nika_providers::{AdmissionState, ProviderRegistry, ProvidersConfig};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MODEL: &str = "deepseek/c6-unpriced-fixture";
const LEDGER: &str = nika_dap::cost_journal::STANDING_LEDGER;

/// The operator's rule for the fixture route, with `extra` settings.
fn rule(extra: &str) -> String {
    format!("{MODEL}:requests=4,output-tokens=64,timeout=30,runs=3/hour{extra}")
}

fn policy(rule: &str) -> StandingCostPolicy {
    let config = nika_runtime::compose::config_from_env();
    StandingCostPolicy::resolve(&[rule.to_owned()], &config).expect("the fixture rule")
}

fn rule_id(rule: &str) -> String {
    nika_providers::admission::StandingRule::parse(rule)
        .expect("rule")
        .id()
        .to_owned()
}

/// A host that holds `policy`: no USD cap of its own, the per-run ceiling
/// `ceiling` (USD) as its machine layer, a manual job.
fn host(policy: &StandingCostPolicy, ceiling: Option<f64>) -> CostHostEvidence {
    let layer = |origin: &str| CapEvidence::Observed {
        cap: HardMonetaryCap::Absent,
        origin: origin.into(),
    };
    let machine = match ceiling.map(|usd| monetary_default(Some(usd))) {
        Some(Ok(Some(cap))) => CapEvidence::Observed {
            cap: HardMonetaryCap::Capped(cap),
            origin: "per-run ceiling".into(),
        },
        Some(_) => CapEvidence::Unknown,
        None => layer("ceiling disarmed"),
    };
    let occurrence = CapEvidence::NotApplicable {
        origin: "manual job".into(),
    };
    CostHostEvidence::new(true, layer("standing policy"), machine, occurrence)
        .with_standing_policy(policy.clone())
}

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

/// The evaluator's plan for the single-request fixture Run in `root`.
fn judged(
    root: &Path,
    invocation: &str,
    evidence: CostHostEvidence,
) -> Result<RunCostPlan, String> {
    let (source, wf, plan) = unpriced();
    let ask = || Ok(());
    prepare(
        root,
        Some(root),
        &source,
        invocation.into(),
        &wf,
        None,
        &plan,
        &Inputs::new(),
        None,
        (evidence, &ask),
    )
}

fn reviewed(root: &Path, invocation: &str, evidence: CostHostEvidence) -> Box<ReviewedRun> {
    match judged(root, invocation, evidence) {
        Ok(RunCostPlan::Review(review)) => review,
        Ok(_) => panic!("an unpriced route frames a review"),
        Err(refused) => panic!("{refused}"),
    }
}

/// One Run of the fixture admitted under `policy` at `now`, settled with
/// nothing sent.
fn admitted_run(root: &Path, invocation: &str, policy: &StandingCostPolicy, now: SystemTime) {
    let (source, ..) = unpriced();
    let cost = reviewed(root, invocation, host(policy, None))
        .confirm_standing(&source, now)
        .expect("inside the rule and its window");
    cost.finish().expect("settled");
}

fn ledger(root: &Path) -> Value {
    let text = std::fs::read_to_string(root.join(".nika").join(LEDGER)).expect("the ledger");
    serde_json::from_str(&text).expect("ledger JSON")
}

fn lease_is_free(root: &Path) -> bool {
    clear_exposure(root, "probe").is_ok()
}

/// A Run inside its rule needs no decision: the host counts it, confirms it,
/// and every row of its journal names the rule that admitted it and the
/// bounds its account enforces. The lease is held to its settlement.
#[test]
fn a_run_inside_its_rule_is_admitted_with_no_decision_and_its_rows_name_the_rule() {
    let root = tempfile::tempdir().unwrap();
    let (source, ..) = unpriced();
    let text = rule("");
    let held = policy(&text);
    let review = reviewed(root.path(), "exe-standing-1", host(&held, None));
    let named = review.standing().expect("the rule names this Run");
    assert_eq!(named.id(), rule_id(&text));
    assert_eq!(review.bounds(), (1, 64, Duration::from_secs(30)));
    assert!(rows(root.path()).is_empty(), "framing prepares nothing");
    assert!(
        !root.path().join(".nika").join(LEDGER).exists(),
        "reading the window counts nothing"
    );
    let cost = review
        .confirm_standing(&source, at(1_000))
        .expect("admitted");
    assert_eq!(
        ledger(root.path()),
        json!({"schema": "nika/standing-cost-runs@1", "runs": {MODEL: [1_000]}})
    );
    let prepared = rows(root.path());
    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0]["phase"], "prepared");
    assert_eq!(prepared[0]["invocation"], "exe-standing-1");
    let choice = &prepared[0]["observation"]["unknown_cost"];
    assert_eq!(
        choice["standing"],
        json!({"policy": rule_id(&text), "max_runs": 3, "window_seconds": 3_600})
    );
    assert_eq!(choice["provider"], "deepseek");
    assert_eq!(choice["model"], "c6-unpriced-fixture");
    assert_eq!(choice["max_requests"], 1);
    assert_eq!(choice["max_output_tokens"], 64);
    assert_eq!(choice["timeout_ms"], 30_000);
    assert!(choice["declared_tariff"].is_null());
    assert_eq!(
        prepared[0]["observation"]["overridden_defaults"],
        json!([null, null]),
        "a standing rule overrides no default"
    );
    assert!(!lease_is_free(root.path()), "held until the Run settles");
    let bound = cost
        .config
        .inference_admission
        .as_ref()
        .expect("the Run's account");
    assert_eq!(
        bound
            .snapshot()
            .unwrap()
            .unknown_cost
            .as_ref()
            .and_then(|choice| choice.standing_policy().map(str::to_owned)),
        Some(rule_id(&text))
    );
    cost.finish().expect("settled");
    drop(cost);
    let settled = rows(root.path());
    assert_eq!(settled.len(), 2);
    assert_eq!(settled[1]["phase"], "settled");
    assert_eq!(settled[1]["observation"]["state"], "Closed");
    assert_eq!(settled[1]["observation"]["unknown_calls"], 0);
    assert_eq!(
        settled[1]["observation"]["unknown_cost"]["standing"]["policy"],
        rule_id(&text)
    );
    assert!(lease_is_free(root.path()));
}

/// Without a standing policy for its route, a review is the historical one:
/// nothing names a rule, and a host that asks a standing admission of it is
/// refused before anything is counted, prepared or written.
#[test]
fn a_review_no_rule_names_is_never_admitted_without_a_decision() {
    let (source, ..) = unpriced();
    let elsewhere = policy(&rule("").replace(MODEL, "deepseek/another-unpriced-fixture"));
    for evidence in [
        CostHostEvidence::unmanaged_interactive_local(),
        host(&elsewhere, None),
    ] {
        let root = tempfile::tempdir().unwrap();
        let review = reviewed(root.path(), "exe-unnamed", evidence);
        assert!(review.standing().is_none());
        assert_eq!(review.bounds(), (1, 8192, Duration::from_secs(120)));
        let no_rule = "no standing cost rule names this Run: only a fresh decision confirms it";
        assert_eq!(
            review.confirm_standing(&source, at(1_000)).err().unwrap(),
            no_rule
        );
        assert!(rows(root.path()).is_empty(), "nothing was prepared");
        assert!(!root.path().join(".nika").join(LEDGER).exists());
        assert!(lease_is_free(root.path()));
    }
    // Under a ceiling such a route is the cap's own refusal, as without a policy.
    let root = tempfile::tempdir().unwrap();
    let capped = judged(root.path(), "exe-capped", host(&elsewhere, Some(1.0)));
    let Ok(RunCostPlan::HardCapped(why)) = capped else {
        panic!("a route no rule names stays under the cap");
    };
    assert_eq!(
        why,
        "catalog admission refused: unknown cost is unavailable: policy must permit it and policy/machine/occurrence caps must be explicitly absent"
    );
}

/// A Run outside its rule refuses in the rule's words before any host could
/// confirm it: nothing is counted, nothing prepared, and the lease is free.
#[test]
fn a_run_outside_its_rule_refuses_before_anything_is_counted() {
    let text = rule("");
    let held = policy(&text);
    let root = tempfile::tempdir().unwrap();
    let (fan, asked, _) = hosted(root.path(), FAN, &Inputs::new(), host(&held, None));
    assert_eq!(asked, 1);
    assert_eq!(
        fan.err().expect("six requests are outside a rule of four"),
        format!(
            "this Run may send 6 requests and the standing cost rule {} admits at most 4 per Run on {MODEL}; nothing was sent",
            rule_id(&text)
        )
    );
    let narrow = rule("").replace("output-tokens=64", "output-tokens=8");
    let refused = judged(root.path(), "exe-tall", host(&policy(&narrow), None));
    assert_eq!(
        refused
            .err()
            .expect("16 output tokens are outside a rule of 8"),
        format!(
            "this Run asks up to 16 output tokens per request and the standing cost rule {} admits at most 8 on {MODEL}; nothing was sent",
            rule_id(&narrow)
        )
    );
    assert!(rows(root.path()).is_empty());
    assert!(!root.path().join(".nika").join(LEDGER).exists());
    assert!(lease_is_free(root.path()));
    // A fan the rule covers is admitted with its typed bound, in flight included.
    let wide = policy(&rule("").replace("requests=4", "requests=6"));
    let (fan, _, source) = hosted(root.path(), FAN, &Inputs::new(), host(&wide, None));
    let Ok(RunCostPlan::Review(review)) = fan else {
        panic!("six requests fit a rule of six");
    };
    let cost = review
        .confirm_standing(&source, at(1_000))
        .expect("admitted");
    let choice = serde_json::to_value(cost.account.snapshot().unwrap().unknown_cost).unwrap();
    assert_eq!(choice["max_requests"], 6);
    assert_eq!(choice["max_in_flight"], 2);
    assert_eq!(choice["authored_retry"], true);
    cost.finish().unwrap();
}

/// A rule's window admits its Runs and no more, across reviews (and so across
/// restarts of the host): the Run past it refuses with the count and the
/// instant the window reopens, prepares nothing, and is admitted from then.
#[test]
fn a_full_window_refuses_the_next_run_until_it_reopens() {
    let root = tempfile::tempdir().unwrap();
    let (source, ..) = unpriced();
    let text = rule("");
    let held = policy(&text);
    for (run, now) in [1_000_u64, 1_100, 1_200].into_iter().enumerate() {
        admitted_run(root.path(), &format!("exe-window-{run}"), &held, at(now));
    }
    assert_eq!(rows(root.path()).len(), 6);
    let review = reviewed(root.path(), "exe-window-3", host(&held, None));
    assert_eq!(
        review.confirm_standing(&source, at(1_300)).err().unwrap(),
        format!(
            "the standing cost rule {} admits 3 Runs of {MODEL} per 3600 seconds and 3 ran in that window: the next is admitted from 1970-01-01T01:16:40Z; nothing was sent",
            rule_id(&text)
        )
    );
    assert_eq!(rows(root.path()).len(), 6, "nothing was prepared");
    assert_eq!(
        ledger(root.path())["runs"][MODEL],
        json!([1_000, 1_100, 1_200])
    );
    assert!(lease_is_free(root.path()));
    admitted_run(root.path(), "exe-window-4", &held, at(4_600));
    assert_eq!(rows(root.path()).len(), 8);
    assert_eq!(
        ledger(root.path())["runs"][MODEL],
        json!([1_100, 1_200, 4_600])
    );
}

/// A transport that answers every post with its status and body.
struct Wire(AtomicUsize, u16, &'static str);

/// The fixture route's complete answer, with the usage it reports.
const ANSWER: &str = r#"{"id":"standing-fixture","model":"c6-unpriced-fixture","choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}"#;

impl HttpPostDyn for Wire {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        assert!(!request.follow_redirects);
        Ok(HttpResponse::new(
            self.1,
            std::collections::BTreeMap::new(),
            self.2.into(),
            &request.url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("a standing Run never streams");
    }
}

/// The ambiguous-admission rule keeps protecting standing Runs: a request
/// that left without a usable answer settles its Run Uncertain, and the next
/// Run of the same rule is refused before it is counted, until an operator
/// reconciles the exposure. So is a Run whose writer died after `prepared`.
#[tokio::test]
async fn a_run_that_may_have_been_billed_blocks_the_next_standing_run() {
    let root = tempfile::tempdir().unwrap();
    let (source, ..) = unpriced();
    let held = policy(&rule(""));
    let cost = reviewed(root.path(), "exe-ambiguous", host(&held, None))
        .confirm_standing(&source, at(1_000))
        .expect("admitted");
    let wire = Arc::new(Wire(AtomicUsize::new(0), 500, "{}"));
    let provider = ProviderRegistry::new(
        Arc::clone(&wire),
        ProvidersConfig::new().with_key("deepseek", Secret::new("fixture")),
    )
    .with_inference_admission(cost.config.inference_admission.clone().expect("account"))
    .resolve(MODEL)
    .expect("the fixture route");
    let mut request = InferRequest::new(MODEL, vec![Message::text(Role::User, "hi")]);
    request.max_tokens = Some(16);
    assert!(provider.infer_reported(request).await.is_err());
    assert_eq!(wire.0.load(Ordering::SeqCst), 1);
    assert_eq!(
        cost.account.snapshot().unwrap().state,
        AdmissionState::Uncertain
    );
    cost.finish().expect("the uncertainty is written");
    drop(cost);
    let settled = rows(root.path());
    assert_eq!(settled[1]["observation"]["state"], "Uncertain");
    assert_eq!(settled[1]["observation"]["unknown_calls"], 1);
    assert!(
        settled[1]["observation"]["unknown_attempts"][0]["estimated_nano_usd"].is_null(),
        "possibly billed, never zero"
    );
    let blocked = judged(root.path(), "exe-next", host(&held, None))
        .err()
        .expect("the earlier Run blocks");
    assert!(
        blocked.contains(
            "an earlier unknown-cost Run may have been billed: Run exe-ambiguous settled with a sent request whose charge is unknown"
        ),
        "{blocked}"
    );
    assert!(blocked.contains("no automatic retry"), "{blocked}");
    assert_eq!(rows(root.path()).len(), 2, "nothing was prepared");
    assert_eq!(
        ledger(root.path())["runs"][MODEL],
        json!([1_000]),
        "nor counted"
    );
    // A Run whose writer let the lease go after `prepared` blocks too.
    let killed = tempfile::tempdir().unwrap();
    let nika = nika_fs::OwnedDir::open(killed.path())
        .unwrap()
        .create_below(&[".nika"])
        .unwrap();
    let mut prepared = settled[0].clone();
    prepared["invocation"] = json!("exe-killed");
    prepared["lease"] = cost_journal::Writer::this_process().json();
    cost_journal::append_row(&nika, &prepared.to_string()).unwrap();
    let blocked = judged(killed.path(), "exe-after", host(&held, None))
        .err()
        .expect("a killed Run blocks");
    assert!(
        blocked.contains("Run exe-killed ended without a settlement"),
        "{blocked}"
    );
    assert_eq!(rows(killed.path())[1]["phase"], "unknown");
    assert!(!killed.path().join(".nika").join(LEDGER).exists());
}

/// A standing rule overrides no ceiling. With a declared price the Run is
/// admitted under a present ceiling only when its declared worst case fits,
/// and its rows record the declaration; without one the ceiling refuses it.
#[test]
fn a_ceiling_admits_only_a_declared_worst_case_that_fits_under_it() {
    let (source, ..) = unpriced();
    let text = rule(",request-bytes=2048,input-usd=1,output-usd=2");
    let priced = policy(&text);
    // 2,048 bytes at 1,000 nano-USD and 64 output tokens at 2,000 nano-USD.
    let root = tempfile::tempdir().unwrap();
    let cost = reviewed(root.path(), "exe-priced", host(&priced, Some(1.0)))
        .confirm_standing(&source, at(1_000))
        .expect("the worst case fits under one dollar");
    let choice = &rows(root.path())[0]["observation"]["unknown_cost"];
    assert_eq!(choice["max_request_bytes"], 2_048);
    assert_eq!(
        choice["standing"],
        json!({"policy": rule_id(&text), "max_runs": 3, "window_seconds": 3_600,
            "declared_worst_case_nano_usd": "2176000"})
    );
    assert_eq!(choice["declared_tariff"]["currency"], "USD");
    assert_eq!(
        choice["declared_tariff"]["nano_per_token"],
        json!([1_000, 2_000, 1_000])
    );
    assert_eq!(choice["declared_tariff"]["version"], rule_id(&text));
    assert_eq!(
        choice["declared_tariff"]["origin"],
        "https://api.deepseek.com:443"
    );
    cost.finish().unwrap();
    drop(cost);
    let tight = tempfile::tempdir().unwrap();
    assert_eq!(
        judged(tight.path(), "exe-tight", host(&priced, Some(0.001)))
            .err()
            .expect("the worst case exceeds a tenth of a cent"),
        "the declared worst case of this Run ($0.002176: every request at its size and output bounds, at the declared rates) exceeds the $0.001000 ceiling it runs under; nothing was sent"
    );
    assert!(rows(tight.path()).is_empty());
    assert!(!tight.path().join(".nika").join(LEDGER).exists());
    assert!(lease_is_free(tight.path()));
    // Without a declared price the ceiling is the cap's own refusal.
    let unpriced_rule = policy(&rule(""));
    let capped = judged(tight.path(), "exe-capped", host(&unpriced_rule, Some(1.0)));
    assert!(matches!(capped, Ok(RunCostPlan::HardCapped(_))));
    // The project's own `ceiling:` is a ceiling a standing Run must fit under.
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("nika.yaml"),
        "nika: capped\nceiling: 5\n",
    )
    .unwrap();
    assert_eq!(
        judged(project.path(), "exe-project", host(&unpriced_rule, None))
            .err()
            .expect("the project ceiling binds"),
        "a $5.000000 ceiling binds this Run and its standing rule declares no price: an unknown price fits under no ceiling; nothing was sent"
    );
    let fits = reviewed(project.path(), "exe-project-priced", host(&priced, None));
    assert_eq!(fits.defaults(), [None, Some(5.0)]);
    assert!(
        fits.standing().is_some(),
        "the declared worst case fits under 5 USD"
    );
}

/// The journal's read bound (nika-dap): an admission refuses a larger journal.
const READ_BOUND: usize = 1_048_576;

/// What the engine reserves in the journal for a Run of `requests` requests.
fn reserve(requests: u32) -> usize {
    usize::try_from(ROW_RESERVE).unwrap() * (usize::try_from(requests).unwrap() + 2)
}

/// As many settled Runs that clear as fit in `most` bytes of journal: the two
/// rows `seed` of one settled Run, repeated under other invocations.
fn settled_runs(seed: &str, most: usize) -> String {
    let mut filled = String::new();
    for run in 0.. {
        let next = seed.replace("exe-seed", &format!("exe-seed-{run}"));
        if filled.len() + next.len() > most {
            break;
        }
        filled.push_str(&next);
    }
    filled
}

/// A fan of `requests` single-attempt requests, one at a time.
fn fan_of(requests: u32) -> String {
    let items: Vec<String> = (0..requests).map(|n| format!("i{n}")).collect();
    format!(
        "  ask:\n    for_each: {{ items: [{}], max_parallel: 1 }}\n    infer: {{ prompt: 'x ${{{{ item }}}}', max_tokens: 16 }}\n",
        items.join(", ")
    )
}

/// A host that admits Runs with no decision fills the journal, which every
/// admission reads whole and refuses past its read bound. Before a standing
/// Run, a cleared journal that leaves room for that Run's rows stays where it
/// is; one that does not is archived whole, and the Run starts a new journal.
/// A host without a standing policy archives nothing.
#[test]
fn a_journal_a_standing_host_fills_is_archived_when_a_run_needs_its_room() {
    let root = tempfile::tempdir().unwrap();
    let held = policy(&rule("").replace("runs=3/hour", "runs=1000/hour"));
    admitted_run(root.path(), "exe-seed", &held, at(1_000));
    let journal = root.path().join(".nika").join(JOURNAL);
    let seed = std::fs::read_to_string(&journal).unwrap();
    // A single-request Run: room for its two rows and its one request.
    let spare = reserve(1);
    assert!(
        seed.len() < spare,
        "one single-request Run writes {} bytes of journal",
        seed.len()
    );
    let within = settled_runs(&seed, READ_BOUND - spare);
    std::fs::write(&journal, &within).unwrap();
    admitted_run(root.path(), "exe-within", &held, at(1_100));
    let grown = std::fs::read_to_string(&journal).unwrap();
    assert!(
        grown.starts_with(&within) && grown.len() > within.len(),
        "the journal left room: nothing is archived"
    );
    let filled = settled_runs(&seed, READ_BOUND);
    assert!(filled.len() > READ_BOUND - spare && filled.len() <= READ_BOUND);
    std::fs::write(&journal, &filled).unwrap();
    let local = CostHostEvidence::unmanaged_interactive_local();
    drop(reviewed(root.path(), "exe-plain", local));
    assert_eq!(
        std::fs::read_to_string(&journal).unwrap(),
        filled,
        "a host without a standing policy archives nothing"
    );
    admitted_run(root.path(), "exe-after", &held, at(1_200));
    let digest = nika_event::source_id::sha256_hex(filled.as_bytes());
    let archive = root.path().join(".nika").join(format!(
        "inference-cost-observations.{}.ndjson",
        &digest[..16]
    ));
    assert_eq!(
        std::fs::read_to_string(&archive).unwrap(),
        filled,
        "the archive holds the journal's exact bytes"
    );
    let fresh = rows(root.path());
    assert_eq!(fresh.len(), 2, "the Run starts a new journal");
    assert_eq!(fresh[0]["invocation"], "exe-after");
    assert_eq!(fresh[1]["phase"], "settled");
    assert!(lease_is_free(root.path()));
}

/// The room reserved for a standing Run covers what it writes. A Run writes
/// two rows and its last one records every request it sent. From one request
/// to the most a rule can carry, every request answered (the longest record),
/// with or without a declared price, on the largest journal that is not
/// archived first: the Run's rows fit in their reserve, the journal stays
/// within its read bound, and the next admission reads it.
#[tokio::test]
async fn the_room_reserved_for_a_standing_run_covers_the_rows_it_writes() {
    let most = nika_providers::admission::STANDING_MAX_REQUESTS;
    for (requests, extra) in [
        (1, ""),
        (1, ",request-bytes=2048,input-usd=1,output-usd=2"),
        (64, ""),
        (64, ",request-bytes=2048,input-usd=1,output-usd=2"),
        (most, ""),
        (most, ",request-bytes=2048,input-usd=1,output-usd=2"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let text = rule(extra)
            .replace("requests=4", &format!("requests={requests}"))
            .replace("runs=3/hour", "runs=1000/hour");
        let held = policy(&text);
        admitted_run(root.path(), "exe-seed", &held, at(1_000));
        let journal = root.path().join(".nika").join(JOURNAL);
        let seed = std::fs::read_to_string(&journal).unwrap();
        let filled = settled_runs(&seed, READ_BOUND.saturating_sub(reserve(requests)));
        std::fs::write(&journal, &filled).unwrap();
        let fan = fan_of(requests);
        let (plan, _, source) = hosted(root.path(), &fan, &Inputs::new(), host(&held, None));
        let Ok(RunCostPlan::Review(review)) = plan else {
            panic!("a fan of {requests} is inside a rule of {requests}");
        };
        assert_eq!(review.bounds().0, requests);
        let cost = review
            .confirm_standing(&source, at(1_100))
            .expect("admitted");
        let wire = Arc::new(Wire(AtomicUsize::new(0), 200, ANSWER));
        let provider = ProviderRegistry::new(
            Arc::clone(&wire),
            ProvidersConfig::new().with_key("deepseek", Secret::new("fixture")),
        )
        .with_inference_admission(cost.config.inference_admission.clone().expect("account"))
        .resolve(MODEL)
        .expect("the fixture route");
        for _ in 0..requests {
            let mut request = InferRequest::new(MODEL, vec![Message::text(Role::User, "hi")]);
            request.max_tokens = Some(16);
            provider.infer_reported(request).await.expect("answered");
        }
        cost.finish().expect("settled");
        drop(cost);
        let whole = std::fs::read_to_string(&journal).unwrap();
        assert!(
            whole.starts_with(&filled),
            "the journal left room: nothing was archived"
        );
        let last: Value = serde_json::from_str(whole.lines().last().unwrap()).unwrap();
        assert_eq!(last["phase"], "settled");
        assert_eq!(last["observation"]["state"], "Closed", "{requests}{extra}");
        assert_eq!(
            last["observation"]["unknown_attempts"]
                .as_array()
                .map(Vec::len),
            usize::try_from(requests).ok()
        );
        let wrote = whole.len() - filled.len();
        assert!(
            wrote <= reserve(requests),
            "{requests} requests{extra} wrote {wrote} bytes into a reserve of {}",
            reserve(requests)
        );
        assert!(whole.len() <= READ_BOUND, "{}", whole.len());
        drop(reviewed(root.path(), "exe-next", host(&held, None)));
    }
}

/// The room is counted per request: a journal that would keep a single-request
/// Run is archived before a Run of 64 requests is framed, because it leaves
/// less than that Run's rows may take.
#[test]
fn a_larger_run_archives_a_journal_a_smaller_one_would_keep() {
    let root = tempfile::tempdir().unwrap();
    let text = rule("")
        .replace("requests=4", "requests=64")
        .replace("runs=3/hour", "runs=1000/hour");
    let held = policy(&text);
    admitted_run(root.path(), "exe-seed", &held, at(1_000));
    let journal = root.path().join(".nika").join(JOURNAL);
    let seed = std::fs::read_to_string(&journal).unwrap();
    let filled = settled_runs(&seed, READ_BOUND - reserve(1));
    assert!(filled.len() > READ_BOUND - reserve(64));
    std::fs::write(&journal, &filled).unwrap();
    admitted_run(root.path(), "exe-single", &held, at(1_100));
    let kept = std::fs::read_to_string(&journal).unwrap();
    assert!(kept.starts_with(&filled), "room for one request: kept");
    std::fs::write(&journal, &filled).unwrap();
    let (plan, _, _) = hosted(root.path(), &fan_of(64), &Inputs::new(), host(&held, None));
    let Ok(RunCostPlan::Review(review)) = plan else {
        panic!("a fan of 64 is inside a rule of 64");
    };
    assert_eq!(review.bounds().0, 64);
    assert!(rows(root.path()).is_empty(), "archived before it is framed");
    let digest = nika_event::source_id::sha256_hex(filled.as_bytes());
    let archive = root.path().join(".nika").join(format!(
        "inference-cost-observations.{}.ndjson",
        &digest[..16]
    ));
    assert_eq!(std::fs::read_to_string(&archive).unwrap(), filled);
}

/// A host that only asks (the review door) is answered without the project's
/// cost lease: the rule that names the Run when a job of it would be inside
/// that rule now, and nothing otherwise. It never waits on a Run in flight,
/// and it prepares, counts and holds nothing.
#[test]
fn a_host_that_only_asks_is_answered_without_the_lease() {
    let (_, wf, plan) = unpriced();
    let text = rule("");
    let held = policy(&text);
    let asked = |root: &Path, evidence: CostHostEvidence, now: u64| {
        standing_rule(
            root,
            "exe-asked",
            &wf,
            &plan,
            &Inputs::new(),
            evidence,
            at(now),
        )
    };
    let root = tempfile::tempdir().unwrap();
    let named = asked(root.path(), host(&held, None), 1_000).expect("judged");
    assert_eq!(named.map(|rule| rule.id().to_owned()), Some(rule_id(&text)));
    assert!(
        !root.path().join(".nika").exists(),
        "asking creates nothing"
    );
    // A Run in flight holds the lease: the answer does not wait for it.
    let in_flight = reviewed(root.path(), "exe-in-flight", host(&held, None));
    assert!(!lease_is_free(root.path()));
    assert!(
        asked(root.path(), host(&held, None), 1_000)
            .expect("judged")
            .is_some()
    );
    drop(in_flight);
    // Outside the rule, under a ceiling it cannot fit, or with no rule: nothing.
    let narrow = policy(&rule("").replace("output-tokens=64", "output-tokens=8"));
    assert_eq!(asked(root.path(), host(&narrow, None), 1_000), Ok(None));
    assert_eq!(asked(root.path(), host(&held, Some(1.0)), 1_000), Ok(None));
    let elsewhere = policy(&rule("").replace(MODEL, "deepseek/another-unpriced-fixture"));
    assert_eq!(asked(root.path(), host(&elsewhere, None), 1_000), Ok(None));
    let local = CostHostEvidence::unmanaged_interactive_local();
    assert_eq!(asked(root.path(), local, 1_000), Ok(None));
    let priced = policy(&rule(",request-bytes=2048,input-usd=1,output-usd=2"));
    assert!(
        asked(root.path(), host(&priced, Some(1.0)), 1_000)
            .expect("judged")
            .is_some()
    );
    assert_eq!(
        asked(root.path(), host(&priced, Some(0.001)), 1_000),
        Ok(None)
    );
    // The project's own ceiling binds a rule without a price.
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("nika.yaml"),
        "nika: capped\nceiling: 5\n",
    )
    .unwrap();
    assert_eq!(asked(project.path(), host(&held, None), 1_000), Ok(None));
    assert!(
        asked(project.path(), host(&priced, None), 1_000)
            .expect("judged")
            .is_some()
    );
    // A full window: nothing until it reopens; an unreadable ledger is an error.
    for now in [1_000_u64, 1_100, 1_200] {
        admitted_run(root.path(), &format!("exe-asked-{now}"), &held, at(now));
    }
    assert_eq!(asked(root.path(), host(&held, None), 1_300), Ok(None));
    assert!(
        asked(root.path(), host(&held, None), 4_600)
            .expect("judged")
            .is_some()
    );
    std::fs::write(root.path().join(".nika").join(LEDGER), "not a ledger").unwrap();
    assert_eq!(
        asked(root.path(), host(&held, None), 1_300).unwrap_err(),
        "the standing ledger is unreadable: its Runs cannot be counted"
    );
}
