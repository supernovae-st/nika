// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Fresh local Run confirmation, independent of Session authoring approval.

// Host interaction/protocol projection is this module's effect boundary.
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use nika_providers::InferenceAdmission;
use nika_providers::admission::{
    CostChallenge, CostHostEvidence, CostResponse, CostReview, CostRoute, PendingCostReview,
    monetary_default,
};
use std::path::{Path, PathBuf};
mod exchange;
mod readiness;
mod shape;
pub use exchange::ReviewChannel;
use nika_dap::cost_journal::{self, Cleared, JournalWitness, RunJournal};
pub(crate) use readiness::readiness;

type Inputs = std::collections::BTreeMap<String, serde_json::Value>;

/// A fresh Run decision and its observation journal; never recovered authority.
/// A declared-free observer (`account.observes_declared_free_only()`) has no
/// journal: the trace's terminal frame carries its receipt, and it never
/// replaces the Run's own `--max-cost-usd` gate.
#[non_exhaustive]
pub struct RunCost {
    pub account: InferenceAdmission,
    pub config: nika_runtime::RuntimeConfig,
    journal: Option<RunJournal>,
}
impl RunCost {
    /// Append observation only; no callable authority is serialized.
    /// # Errors
    /// Unreadable account or unwritable descriptor-rooted journal.
    pub fn observe(&self, phase: &str) -> Result<(), String> {
        let journal = self.journal.as_ref();
        journal.map_or(Ok(()), |j| j.observe(phase).map_err(|e| e.to_string()))
    }
    /// Close live authority and persist the final observation, including uncertainty.
    /// # Errors
    /// Account closure or journal failure; the caller must report possible billing.
    pub fn finish(&self) -> Result<(), String> {
        match &self.journal {
            Some(journal) => journal.settle().map_err(|e| e.to_string()),
            None => self.account.close("Run ended").map_err(|e| e.to_string()),
        }
    }
}

/// The live account the journal's rows read (the rows are DAP's, the account the host's).
struct Account(InferenceAdmission);
impl cost_journal::RunAccount for Account {
    fn observation(&self) -> std::io::Result<serde_json::Value> {
        let receipt = self.0.snapshot().map_err(std::io::Error::other)?;
        Ok(receipt.observation())
    }
    fn close(&self, why: &str) -> std::io::Result<()> {
        self.0.close(why).map_err(std::io::Error::other)
    }
}

/// Take the writer lease and read what earlier Runs left, before any question:
/// a live Run that has not settled, or an exposure not yet reconciled, refuses.
fn clear_exposure(root: &Path, observer: &str) -> Result<Cleared, String> {
    let root = nika_fs::OwnedDir::open(root).map_err(|e| e.to_string())?;
    let cleared = cost_journal::clear(&root, observer).map_err(|e| e.to_string())?;
    cleared.map_err(|blocked| blocked.to_string())
}

/// What a Run's cost needs before any effect, from the one evaluator every host shares.
#[non_exhaustive]
pub enum RunCostPlan {
    /// No unknown-cost or observed route: the plan keeps its composition.
    Unneeded,
    /// Declared-free or run-time routes: the observer bound before any effect.
    Observer(Box<RunCost>),
    /// One fresh review of an unknown-cost Run, holding this project's cost lease.
    Review(Box<ReviewedRun>),
}

/// One framed review. Its challenge is what the host shows; only
/// [`Self::confirm`] with an explicit answer turns it into the Run's account.
pub struct ReviewedRun {
    pending: PendingCostReview,
    cleared: Cleared,
    places: [PathBuf; 2],
    source: String,
    invocation: String,
    wf: nika_schema::raw::RawWorkflow,
    inputs: Inputs,
    model: String,
    prior: JournalWitness,
    bounds: (u32, u32, std::time::Duration),
    defaults: [Option<f64>; 2],
}

/// The evaluator for one Run: an unknown-cost route clears the project under
/// `root`, then `ask` (the host's gate) speaks before the review is framed
/// under `evidence`; `launch` resolves paths (`None`: the current directory).
/// # Errors
/// A refused scope, lease, earlier Run, host, evidence or policy.
#[allow(clippy::too_many_arguments)]
pub fn prepare(
    root: &Path,
    launch: Option<&Path>,
    source: &str,
    invocation: String,
    wf: &nika_schema::raw::RawWorkflow,
    model_override: Option<&str>,
    plan: &nika_providers::ExecutionAccessPlan,
    inputs: &Inputs,
    invocation_default: Option<f64>,
    (evidence, ask): (CostHostEvidence, &dyn Fn() -> Result<(), String>),
) -> Result<RunCostPlan, String> {
    let config = nika_runtime::compose::config_from_env();
    let mut unknown = readiness::unknown_routes(plan, &config)?;
    if unknown.is_empty() {
        let observer = declared_free(wf, plan, &config, model_override, inputs)?;
        return Ok(observer.map_or(RunCostPlan::Unneeded, |c| RunCostPlan::Observer(c.into())));
    }
    // Record what earlier Runs left before any refusal of this host or shape:
    // a host that cannot ask still leaves a killed Run's UNKNOWN on record.
    let cleared = clear_exposure(root, &invocation)?;
    ask()?;
    // The host's own review stays a message: the typed reason renders its words.
    let bound = nika_service_execution::run_cost::request_bound(wf, plan, unknown.len())
        .map_err(|e| e.to_string())?;
    let launch = match launch {
        Some(launch) => launch.to_path_buf(),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    let files = shape::read_witness(&cleared, root, wf, &launch)?;
    if invocation_default == Some(0.0) {
        return Err("zero invocation ceiling refuses unknown spend before HTTP".into());
    }
    let project = project(&launch, "project monetary configuration is unreadable")?;
    let project_default = project.as_ref().and_then(|(_, p)| p.ceiling);
    let (model, route) = unknown.remove(0);
    let candidate = witness(source, inputs, &(&project, &files));
    let review = CostReview::new(
        candidate,
        invocation.clone(),
        route,
        evidence,
        monetary_default(invocation_default)?,
        monetary_default(project_default)?,
    )?
    .for_run(bound)?;
    let bounds = (
        review.max_requests(),
        review.max_output_tokens(),
        review.request_timeout(),
    );
    let pending = PendingCostReview::new(
        review,
        nika_event::source_id::sha256_hex(source.as_bytes()),
        nika_event::source_id::sha256_hex(format!("{inputs:?}:{files:?}").as_bytes()),
    );
    Ok(RunCostPlan::Review(Box::new(ReviewedRun {
        pending,
        prior: cleared.journal().map_err(|e| e.to_string())?,
        cleared,
        places: [root.into(), launch],
        source: source.into(),
        invocation,
        wf: wf.clone(),
        inputs: inputs.clone(),
        model,
        bounds,
        defaults: [invocation_default, project_default],
    })))
}

fn project(
    launch: &Path,
    unreadable: &str,
) -> Result<Option<(PathBuf, nika_vocab::project::Project)>, String> {
    let project = nika_vocab::project::discover_reachable(launch).map_err(|e| e.to_string())?;
    if project.unreachable.is_some() {
        return Err(unreadable.into());
    }
    Ok(project.found)
}

impl ReviewedRun {
    /// The observation this review shows, never callable authority.
    #[must_use]
    pub fn challenge(&self) -> &CostChallenge {
        self.pending.challenge()
    }
    /// The journal's exact bytes after this review cleared it.
    #[must_use]
    pub fn prior_journal(&self) -> &JournalWitness {
        &self.prior
    }
    /// Requests, output tokens per request and per-request deadline it admits.
    #[must_use]
    pub fn bounds(&self) -> (u32, u32, std::time::Duration) {
        self.bounds
    }
    /// The invocation and project defaults approval overrides once.
    #[must_use]
    pub fn defaults(&self) -> [Option<f64>; 2] {
        self.defaults
    }
    /// Consume the review with the host's explicit answer, re-observing all it
    /// bound (`source` as read now); the `prepared` row is written only here.
    /// # Errors
    /// A decline, a changed witness, an expired review, or a journal failure.
    pub fn confirm(self, answer: &CostResponse, source: &str) -> Result<RunCost, String> {
        let [root, launch] = &self.places;
        if source != self.source {
            return Err("workflow changed during review; Run not started".into());
        }
        if !self.cleared.same_place(root).map_err(|e| e.to_string())? {
            return Err("the project directory was replaced during review; Run not started".into());
        }
        if self.cleared.journal().map_err(|e| e.to_string())? != self.prior {
            return Err("the cost journal changed during review; Run not started".into());
        }
        let project_now = project(launch, "project configuration became unreadable")?;
        let files_now = shape::read_witness(&self.cleared, root, &self.wf, launch)?;
        let candidate = witness(source, &self.inputs, &(&project_now, &files_now));
        let route = CostRoute::observe(&self.model, nika_runtime::compose::config_from_env())?;
        let account = self.pending.confirm(answer, &candidate, &route)?;
        let config = nika_runtime::RuntimeConfig::new(None, 0)
            .with_inference_admission(&account, &candidate, &self.invocation)
            .map_err(|e| e.to_string())?;
        let journal = RunJournal::new(
            self.cleared,
            self.invocation,
            Box::new(Account(account.clone())),
        );
        let cost = RunCost {
            account,
            config,
            journal: Some(journal),
        };
        cost.observe("prepared")?;
        Ok(cost)
    }
}

/// Unsupported hosts and workflow shapes never borrow this approval.
/// A Run with no unknown-cost route gets only a declared-free observer, whose
/// model-less tasks ride the workflow's own `model:` here.
/// # Errors
/// Unsupported or changed scope, unreadable evidence, decline, or journal failure.
#[allow(clippy::too_many_arguments)]
pub fn review(
    root: &Path,
    file: &str,
    source: &str,
    invocation: String,
    wf: &nika_schema::raw::RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    inputs: &Inputs,
    invocation_default: Option<f64>,
    channel: impl Into<ReviewChannel>,
) -> Result<Option<RunCost>, String> {
    review_with_model(
        root,
        file,
        source,
        invocation,
        wf,
        None,
        plan,
        inputs,
        invocation_default,
        channel,
    )
}

/// [`review`] for a Run whose `--model` (`model_override`) is the lane its
/// model-less tasks ride.
/// # Errors
/// As [`review`].
#[allow(clippy::too_many_arguments)]
pub fn review_with_model(
    root: &Path,
    file: &str,
    source: &str,
    invocation: String,
    wf: &nika_schema::raw::RawWorkflow,
    model_override: Option<&str>,
    plan: &nika_providers::ExecutionAccessPlan,
    inputs: &Inputs,
    invocation_default: Option<f64>,
    channel: impl Into<ReviewChannel>,
) -> Result<Option<RunCost>, String> {
    let channel = channel.into();
    let ask = || {
        if channel.available() {
            return Ok(());
        }
        Err(String::from(
            "price unknown: this host cannot obtain a fresh one-time choice; use an interactive local `nika run` or a host with explicit cap evidence and confirmation",
        ))
    };
    let evidence = CostHostEvidence::unmanaged_interactive_local();
    let review = match prepare(
        root,
        None,
        source,
        invocation,
        wf,
        model_override,
        plan,
        inputs,
        invocation_default,
        (evidence, &ask),
    )? {
        RunCostPlan::Unneeded => return Ok(None),
        RunCostPlan::Observer(cost) => return Ok(Some(*cost)),
        RunCostPlan::Review(review) => review,
    };
    let answer = channel.ask(review.challenge())?;
    let actual_source = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    review.confirm(&answer, &actual_source).map(Some)
}

/// Exact declared-free text routes (E4) and `model:` values rendered at run
/// time (C4): a per-Run observer, with no review, lease or journal, bound
/// before any effect. A shape or route the Run would refuse as a literal
/// refuses here once inputs decide it; any other plan keeps its composition.
fn declared_free(
    wf: &nika_schema::raw::RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    config: &nika_providers::ProvidersConfig,
    model_override: Option<&str>,
    inputs: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<Option<RunCost>, String> {
    let observes = nika_service_execution::run_cost::observes;
    Ok(observes(wf, plan, config, model_override, Some(inputs))?.then(|| run_observer(wf)))
}

/// The fresh observer an answered leg binds, as a manual `--resume` does: never
/// unknown-cost authority. Its first leg judged the inputs; any doubt binds.
#[must_use]
pub fn leg_observer(
    wf: &nika_schema::raw::RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    model_override: Option<&str>,
) -> Option<nika_runtime::RuntimeConfig> {
    let config = nika_runtime::compose::config_from_env();
    let observes =
        nika_service_execution::run_cost::observes(wf, plan, &config, model_override, None);
    observes.unwrap_or(true).then(|| run_observer(wf).config)
}

fn run_observer(wf: &nika_schema::raw::RawWorkflow) -> RunCost {
    let (account, config) = nika_service_execution::run_cost::observer(wf);
    RunCost {
        account,
        config,
        journal: None,
    }
}

fn witness(
    source: &str,
    inputs: &std::collections::BTreeMap<String, serde_json::Value>,
    project: &impl std::fmt::Debug,
) -> String {
    nika_event::source_id::sha256_hex(format!("{source:?}:{inputs:?}:{project:?}").as_bytes())
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_types
)]
mod tests {
    use super::*;
    use nika_dap::cost_journal::JOURNAL;
    use nika_runtime::compose::RunSeams;
    use std::io::BufRead as _;
    fn cost(root: &Path) -> RunCost {
        let route = CostRoute::observe(
            "deepseek/deepseek-v4-pro",
            nika_providers::ProvidersConfig::new(),
        )
        .unwrap();
        let account = CostReview::new(
            "candidate".into(),
            "run-1".into(),
            route.clone(),
            CostHostEvidence::unmanaged_interactive_local(),
            None,
            None,
        )
        .unwrap()
        .confirm("candidate", &route)
        .unwrap();
        let config = nika_runtime::RuntimeConfig::new(None, 0)
            .with_inference_admission(&account, "candidate", "run-1")
            .unwrap();
        let cleared = clear_exposure(root, "run-1").expect("a fresh project's cost lease is free");
        let journal = RunJournal::new(cleared, "run-1".into(), Box::new(Account(account.clone())));
        RunCost {
            account,
            config,
            journal: Some(journal),
        }
    }
    fn rows(root: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(root.join(".nika").join(JOURNAL))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    #[test]
    fn prepared_run_is_not_replayed_and_settled_observation_cannot_restore_authority() {
        let root = tempfile::tempdir().unwrap();
        let cost = cost(root.path());
        cost.observe("prepared").unwrap();
        let live = clear_exposure(root.path(), "run-2").unwrap_err();
        assert!(
            live.contains("holds this project's cost lease: an unknown-cost Run in flight or a review waiting for its answer"),
            "{live}"
        );
        cost.finish().unwrap();
        let rows_now = rows(root.path());
        let row = rows_now.last().unwrap();
        assert_eq!(row["phase"], "settled");
        assert_eq!(row["observation"]["known_subtotal_nano_usd"], "0");
        assert_eq!(row["observation"]["unknown_calls"], 0);
        assert!(row["observation"]["limit_nano_usd"].is_null());
        assert_eq!(row["lease"]["pid"], std::process::id());
        assert_eq!(
            cost.account.snapshot().unwrap().state,
            nika_providers::AdmissionState::Closed
        );
        drop(cost);
        assert!(clear_exposure(root.path(), "run-2").is_ok());
        assert_eq!(rows(root.path()).len(), 2, "a settled Run settles once");
    }
    /// Control · an early return after `prepared` (a pre-dispatch failure before
    /// the runtime starts) is no death: dropping the unfinished Run settles what
    /// its account observed — closed, nothing sent — and never reads unknown.
    #[test]
    fn a_run_that_ends_without_finish_settles_what_its_account_observed() {
        let root = tempfile::tempdir().unwrap();
        let cost = cost(root.path());
        cost.observe("prepared").unwrap();
        drop(cost);
        let rows = rows(root.path());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["phase"], "settled");
        assert_eq!(rows[1]["observation"]["state"], "Closed");
        assert_eq!(rows[1]["observation"]["unknown_calls"], 0);
        assert!(clear_exposure(root.path(), "run-2").is_ok());
    }
    /// The private journal (lease · settle-on-drop) keeps the public type's
    /// auto traits: a host may still move a reviewed Run across threads.
    #[test]
    fn a_reviewed_run_stays_send_and_sync() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<RunCost>();
    }
    /// C3 · N3 · a host that cannot ask (automation, `--json`) still records
    /// what an earlier killed Run left: the review takes the lease and derives
    /// that Run's UNKNOWN before it refuses the channel, so the disposition is
    /// on record, and the refusal names it. A clean project still refuses for
    /// the channel and writes no journal.
    #[test]
    fn a_host_that_cannot_ask_still_records_a_killed_runs_unknown() {
        use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
        let model = "deepseek/c3-unpriced-fixture";
        let source = format!(
            "nika: automated\nmodel: {model}\npermits: {{}}\ntasks:\n  ask:\n    infer: {{ prompt: hi, max_tokens: 16 }}\n"
        );
        let wf = nika_schema::parse(
            &source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .unwrap();
        let ready = ProviderReadiness::new(
            true,
            true,
            None,
            None,
            true,
            ExecutionLocus::Cloud,
            nika_types::access::AccessClass::Api,
        );
        let plan = nika_providers::resolve_execution_plan(
            &[nika_providers::ModelNeed::new(model, true, false)],
            &[ProviderProbe::new(
                "deepseek",
                true,
                true,
                "DEEPSEEK_API_KEY",
                false,
                ready,
                "https://api.deepseek.com",
            )],
            Some("api"),
        );
        let refuse = |root: &Path| {
            review(
                root,
                "wf.nika",
                &source,
                "exe-next".into(),
                &wf,
                &plan,
                &std::collections::BTreeMap::new(),
                None,
                ReviewChannel::Unavailable,
            )
            .err()
            .expect("an unavailable channel never admits")
        };
        let clean = tempfile::tempdir().unwrap();
        assert!(refuse(clean.path()).starts_with("price unknown"));
        // Taking custody may create the journal's empty inode, but refusing this
        // host records no preparation, consent or provider attempt.
        let journal = std::fs::read(clean.path().join(".nika").join(JOURNAL)).unwrap();
        assert_eq!(journal, b"");
        // A writer on this host prepared, then let its lease go unsettled.
        let root = tempfile::tempdir().unwrap();
        let nika = nika_fs::OwnedDir::open(root.path())
            .unwrap()
            .create_below(&[".nika"])
            .unwrap();
        let prepared = serde_json::json!({"schema": "nika/run-cost-observation@1",
            "invocation": "exe-killed", "phase": "prepared",
            "observation": cost(tempfile::tempdir().unwrap().path()).account.snapshot().unwrap().observation(),
            "lease": cost_journal::Writer::this_process().json()});
        cost_journal::append_row(&nika, &prepared.to_string()).unwrap();
        let refused = refuse(root.path());
        assert!(
            refused.contains("Run exe-killed ended without a settlement"),
            "{refused}"
        );
        let rows = rows(root.path());
        assert_eq!(rows.len(), 2, "the derived unknown is on record");
        assert_eq!(rows[1]["phase"], "unknown");
        assert_eq!(rows[1]["unsettled"]["observed_by"], "exe-next");
    }
    const FREE: &str = "openrouter/qwen/qwen3.8-27b:free";
    fn free_plan() -> nika_providers::ExecutionAccessPlan {
        use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
        let ready = ProviderReadiness::new(
            true,
            true,
            None,
            None,
            true,
            ExecutionLocus::Cloud,
            nika_types::access::AccessClass::Api,
        );
        nika_providers::resolve_execution_plan(
            &[nika_providers::ModelNeed::new(FREE, true, false)],
            &[ProviderProbe::new(
                "openrouter",
                true,
                true,
                "OPENROUTER_API_KEY",
                false,
                ready,
                "https://openrouter.ai/api/v1/chat/completions",
            )],
            Some("api"),
        )
    }
    fn free_wf(envelope: &str, infer: &str) -> nika_schema::raw::RawWorkflow {
        let source = format!(
            "nika: free\nmodel: {FREE}\n{envelope}permits: {{}}\ntasks:\n  draft:\n    infer: {{ prompt: text, max_tokens: 64{infer} }}\n"
        );
        nika_schema::parse(
            &source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .unwrap()
    }
    fn free_run(envelope: &str, infer: &str) -> Result<Option<RunCost>, String> {
        let config = nika_providers::ProvidersConfig::new();
        declared_free(
            &free_wf(envelope, infer),
            &free_plan(),
            &config,
            None,
            &std::collections::BTreeMap::new(),
        )
    }
    /// C2 · an exact declared-free text Run gets its own scoped observer and
    /// nothing else: no review, no lease (the function never sees a project,
    /// so Runs in one project never wait on each other), no journal row. It
    /// never replaces the Run's budget gate, and ending it closes only it.
    #[test]
    fn a_declared_free_run_gets_a_scoped_observer_without_a_journal() {
        let first = free_run("", "").unwrap().expect("an observer");
        let second = free_run("", "").unwrap().expect("a second Run proceeds");
        assert!(first.account.observes_declared_free_only());
        let bound = first.config.inference_admission.as_ref().unwrap();
        assert!(bound.observes_declared_free_only());
        assert!(first.journal.is_none() && first.observe("prepared").is_ok());
        first.finish().unwrap();
        let state = |c: &RunCost| c.account.snapshot().unwrap().state;
        assert_eq!(state(&first), nika_providers::AdmissionState::Closed);
        assert_eq!(state(&second), nika_providers::AdmissionState::Open);
    }
    /// A shape the observer cannot admit refuses before any account exists,
    /// naming the task; a plan with no declared-free route composes as today.
    #[test]
    fn unsupported_free_shapes_refuse_and_other_plans_keep_todays_composition() {
        let vision = ", vision: [{ source: file, path: './image.png' }]";
        let refused = free_run("", vision).err().expect("refused");
        assert!(refused.starts_with("Run refused before any provider call"));
        assert!(refused.contains("task `draft`") && refused.contains("with vision"));
        let none = nika_providers::resolve_execution_plan(&[], &[], None);
        let wf = nika_schema::parse(
            "nika: local\npermits: {}\ntasks:\n  ok:\n    invoke: { tool: 'nika:assert', args: { condition: true } }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .unwrap();
        let config = nika_providers::ProvidersConfig::new();
        assert!(
            declared_free(
                &wf,
                &none,
                &config,
                None,
                &std::collections::BTreeMap::new()
            )
            .unwrap()
            .is_none()
        );
    }
    /// C4 · a `model:` rendered at run time binds the Run observer even with no
    /// declared-free lane in the plan, and a value its inputs already decide is
    /// refused before any effect as that literal would be. An answered leg
    /// binds the same fresh observer; a literal plan keeps its composition.
    #[test]
    fn a_run_time_model_binds_the_observer_and_its_decided_value_is_judged() {
        let wf = |infer: &str| {
            nika_schema::parse(
                &format!(
                    "nika: dynamic\ninputs:\n  m: {{ type: string, required: true }}\npermits: {{}}\ntasks:\n  draft:\n    infer: {{ prompt: text, model: \"${{{{ inputs.m }}}}\", max_tokens: 64{infer} }}\n"
                ),
                nika_schema::FileId::new(0),
                nika_schema::ParseMode::Strict,
            )
            .unwrap()
        };
        let none = nika_providers::resolve_execution_plan(&[], &[], None);
        let config = nika_providers::ProvidersConfig::new();
        let run = |wf: &nika_schema::raw::RawWorkflow, model: &str| {
            let inputs = [("m".to_owned(), serde_json::Value::from(model))].into();
            declared_free(wf, &none, &config, None, &inputs)
        };
        let paid = run(&wf(""), "deepseek/deepseek-v4-pro")
            .unwrap()
            .expect("observer");
        assert!(paid.account.observes_declared_free_only() && paid.journal.is_none());
        assert!(paid.config.inference_admission.is_some());
        let vision = wf(", vision: [{ source: file, path: './image.png' }]");
        let refused = run(&vision, FREE).err().expect("free vision");
        assert!(
            refused.starts_with("Run refused before any provider call"),
            "{refused}"
        );
        assert!(refused.contains("task `draft`") && refused.contains("with vision"));
        let unknown = run(&wf(""), "mistral/mistral-small-latest")
            .err()
            .expect("unknown");
        assert!(unknown.contains("USD cost is unknown"), "{unknown}");
        assert!(
            leg_observer(&vision, &none, None).is_some(),
            "a leg never runs unobserved"
        );
        let literal = free_wf("", "");
        assert!(
            leg_observer(&literal, &free_plan(), None).is_some(),
            "C2 lane"
        );
        let paid_only = nika_schema::parse(
            "nika: paid\nmodel: deepseek/deepseek-v4-pro\npermits: {}\ntasks:\n  a:\n    infer: { prompt: text, max_tokens: 64 }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .unwrap();
        assert!(
            leg_observer(&paid_only, &none, None).is_none(),
            "today's composition"
        );
    }
    /// The host config replaces compose's default, so it carries the run's
    /// own jitter seed: a seeded run stays replay-stable under the observer.
    #[test]
    fn the_observer_config_keeps_a_seeded_runs_jitter_seed() {
        let run = "run: { entropy: { seeded: 42 } }\n";
        let seeded = free_run(run, "").unwrap().unwrap();
        let wf = free_wf(run, "");
        let expected = RunSeams::of(wf.run.as_ref().map(|r| &r.value)).jitter_seed;
        assert_eq!(seeded.config.jitter_seed, expected);
        assert_ne!(
            expected,
            RunSeams::of(None).jitter_seed,
            "the seed is the run's"
        );
    }
    #[test]
    fn missing_or_corrupt_observation_fields_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let dir = nika_fs::OwnedDir::open(root.path())
            .unwrap()
            .create_below(&[".nika"])
            .unwrap();
        dir.append_line(
            JOURNAL,
            r#"{"schema":"nika/run-cost-observation@1","invocation":"run-1","phase":"settled"}"#,
        )
        .unwrap();
        assert!(clear_exposure(root.path(), "run-2").is_err());
    }
    /// The killed writer: prepares under the lease, names itself, then waits to
    /// be killed. Only the parent test below enters it (the marker file).
    #[test]
    fn killed_writer_fixture_child() {
        if !Path::new(".p3-killed-writer-fixture").exists() {
            return;
        }
        let cost = cost(Path::new("."));
        cost.observe("prepared").unwrap();
        println!("PREPARED {}", std::process::id());
        std::thread::sleep(std::time::Duration::from_secs(30));
        drop(cost);
    }
    /// P3 · a writer killed after `prepared` runs no handler. While it lives its
    /// lease refuses a second review and nothing is judged; once the kernel has
    /// released it, the next review records THAT Run as unknown — once, appended
    /// after the untouched row it was derived from — and still refuses.
    #[test]
    fn a_killed_writer_is_recorded_unknown_by_the_next_review_and_still_blocks() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".p3-killed-writer-fixture"), "test only").unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "run_cost::tests::killed_writer_fixture_child",
                "--nocapture",
                "--quiet",
            ])
            .current_dir(root.path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
        let pid: u64 = loop {
            let mut line = String::new();
            assert!(out.read_line(&mut line).unwrap() > 0, "the child prepared");
            if let Some(pid) = line.trim().strip_prefix("PREPARED ") {
                break pid.parse().unwrap();
            }
        };
        let live = clear_exposure(root.path(), "run-2").unwrap_err();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            live.contains(&format!("process {pid} holds this project's cost lease")),
            "{live}"
        );
        assert_eq!(rows(root.path()).len(), 1, "a live writer is never judged");
        let refused = clear_exposure(root.path(), "run-2").unwrap_err();
        assert!(
            refused.contains(&format!(
                "Run run-1 ended without a settlement; its writer, process {pid}, no longer holds the cost lease"
            )),
            "{refused}"
        );
        let text = std::fs::read_to_string(root.path().join(".nika").join(JOURNAL)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "one durable unknown, appended");
        let derived: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(derived["invocation"], "run-1");
        assert_eq!(derived["phase"], "unknown");
        assert_eq!(derived["unsettled"]["writer"]["pid"], pid);
        assert_eq!(
            derived["unsettled"]["prior_sha256"],
            nika_event::source_id::sha256_hex(lines[0].as_bytes())
        );
        assert_eq!(derived["unsettled"]["observed_by"], "run-2");
        assert!(clear_exposure(root.path(), "run-3").is_err(), "no retry");
        assert_eq!(rows(root.path()).len(), 2, "recorded once");
    }
    /// An unpriced direct route (no catalog tariff) under an API plan: the
    /// shape every unknown-cost review below frames.
    fn unpriced() -> (
        String,
        nika_schema::raw::RawWorkflow,
        nika_providers::ExecutionAccessPlan,
    ) {
        use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
        let model = "deepseek/c6-unpriced-fixture";
        let source = format!(
            "nika: hosted\nmodel: {model}\npermits: {{}}\ntasks:\n  ask:\n    infer: {{ prompt: hi, max_tokens: 16 }}\n"
        );
        let wf = nika_schema::parse(
            &source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .unwrap();
        let ready = ProviderReadiness::new(
            true,
            true,
            None,
            None,
            true,
            ExecutionLocus::Cloud,
            nika_types::access::AccessClass::Api,
        );
        let plan = nika_providers::resolve_execution_plan(
            &[nika_providers::ModelNeed::new(model, true, false)],
            &[ProviderProbe::new(
                "deepseek",
                true,
                true,
                "DEEPSEEK_API_KEY",
                false,
                ready,
                "https://api.deepseek.com",
            )],
            Some("api"),
        );
        (source, wf, plan)
    }
    /// A host with its own authority (no terminal) frames the review: the lease
    /// is held while it waits, and nothing is prepared before its answer.
    fn framed(root: &Path, invocation: &str) -> Box<ReviewedRun> {
        let (source, wf, plan) = unpriced();
        let ask = || Ok(());
        match prepare(
            root,
            Some(root),
            &source,
            invocation.into(),
            &wf,
            None,
            &plan,
            &std::collections::BTreeMap::new(),
            None,
            (CostHostEvidence::unmanaged_interactive_local(), &ask),
        )
        .unwrap()
        {
            RunCostPlan::Review(review) => review,
            _ => panic!("an unpriced route needs a review"),
        }
    }
    /// C6 · the shared door: prepare frames one review (bounds, defaults, the
    /// journal it cleared) and holds the lease; confirm with an explicit yes
    /// writes the `prepared` row, the account settles once. A decline consumes
    /// the review, prepares nothing and lets the lease go.
    #[test]
    fn a_hosted_review_confirms_once_and_a_decline_prepares_nothing() {
        let root = tempfile::tempdir().unwrap();
        let (source, ..) = unpriced();
        let review = framed(root.path(), "exe-c6");
        assert_eq!(review.bounds().0, 1);
        assert_eq!(review.defaults(), [None, None]);
        assert_eq!(review.prior_journal().length, 0);
        assert!(
            clear_exposure(root.path(), "other")
                .unwrap_err()
                .contains("holds this project's cost lease"),
            "the review holds the lease while it waits"
        );
        let answer = review.challenge().response(true);
        let cost = review.confirm(&answer, &source).unwrap();
        assert_eq!(rows(root.path())[0]["phase"], "prepared");
        assert_eq!(rows(root.path())[0]["invocation"], "exe-c6");
        cost.finish().unwrap();
        drop(cost);
        assert_eq!(rows(root.path()).len(), 2, "settled once");
        let declined = framed(root.path(), "exe-no");
        let answer = declined.challenge().response(false);
        let refused = declined.confirm(&answer, &source).err().unwrap();
        assert!(refused.contains("declined"), "{refused}");
        assert_eq!(rows(root.path()).len(), 2, "nothing prepared");
        assert!(
            clear_exposure(root.path(), "next").is_ok(),
            "the lease is free"
        );
    }
    /// C6 · re-observation before authority: a foreign journal change, a project
    /// directory replaced at the same path (a copy put back), or changed source
    /// refuses before any `prepared` row, and the review is spent.
    #[test]
    fn a_changed_journal_place_or_source_refuses_before_authority() {
        let (source, ..) = unpriced();
        let root = tempfile::tempdir().unwrap();
        let review = framed(root.path(), "exe-journal");
        std::fs::write(root.path().join(".nika").join(JOURNAL), "\n").unwrap();
        let answer = review.challenge().response(true);
        let refused = review.confirm(&answer, &source).err().unwrap();
        assert!(refused.contains("cost journal changed"), "{refused}");
        let base = tempfile::tempdir().unwrap();
        let project = base.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let review = framed(&project, "exe-place");
        std::fs::rename(&project, base.path().join("original")).unwrap();
        std::fs::create_dir_all(project.join(".nika")).unwrap();
        let answer = review.challenge().response(true);
        let refused = review.confirm(&answer, &source).err().unwrap();
        assert!(refused.contains("directory was replaced"), "{refused}");
        assert!(!project.join(".nika").join(JOURNAL).exists());
        let fresh = tempfile::tempdir().unwrap();
        let review = framed(fresh.path(), "exe-source");
        let answer = review.challenge().response(true);
        let refused = review.confirm(&answer, "nika: other\n").err().unwrap();
        assert!(refused.contains("workflow changed"), "{refused}");
        let journal = std::fs::read(fresh.path().join(".nika").join(JOURNAL)).unwrap();
        assert_eq!(journal, b"", "custody's empty inode: no row was written");
    }
    /// C6 · zero vetoes unknown spend before any question, whoever supplies
    /// it: `--max-cost-usd 0` (with or without a positive project default),
    /// and a project `ceiling:` of zero, negative or NaN, which the project
    /// file already refuses (a ceiling bounds at the positive real), so no
    /// review is framed and no consent can override it. A positive project
    /// ceiling stays an overridable default; a model-free plan never reads it.
    #[test]
    fn zero_or_invalid_ceilings_veto_unknown_spend_before_any_review() {
        let (source, wf, plan) = unpriced();
        let attempt = |root: &Path, invocation: Option<f64>| {
            let ask = || Ok(());
            let evidence = CostHostEvidence::unmanaged_interactive_local();
            let inputs = std::collections::BTreeMap::new();
            let run = "exe-zero".to_owned();
            prepare(
                root,
                Some(root),
                &source,
                run,
                &wf,
                None,
                &plan,
                &inputs,
                invocation,
                (evidence, &ask),
            )
        };
        let project = |ceiling: &str| {
            let root = tempfile::tempdir().unwrap();
            if !ceiling.is_empty() {
                let file = format!("nika: zero\nceiling: {ceiling}\n");
                std::fs::write(root.path().join("nika.yaml"), file).unwrap();
            }
            root
        };
        for ceiling in ["", "5"] {
            let root = project(ceiling);
            let refused = attempt(root.path(), Some(0.0)).err().expect("zero vetoes");
            assert!(refused.contains("zero invocation ceiling"), "{refused}");
        }
        for ceiling in ["0", "0.0", "-1", ".nan"] {
            for invocation in [None, Some(2.0)] {
                let root = project(ceiling);
                let refused = attempt(root.path(), invocation).err().expect("refused");
                assert!(refused.contains("positive real"), "{ceiling}: {refused}");
                assert!(
                    !refused.contains("safe"),
                    "no diagnostic claims cost safety"
                );
            }
        }
        let root = project("5");
        let Ok(RunCostPlan::Review(review)) = attempt(root.path(), None) else {
            panic!("a positive project ceiling stays reviewable");
        };
        assert_eq!(review.defaults(), [None, Some(5.0)]);
        drop(review);
        let root = project("0");
        let none = nika_providers::resolve_execution_plan(&[], &[], None);
        let local = nika_schema::parse(
            "nika: local\npermits: {}\ntasks:\n  ok:\n    invoke: { tool: 'nika:assert', args: { condition: true } }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .unwrap();
        let ask = || Ok(());
        let evidence = CostHostEvidence::unmanaged_interactive_local();
        let inputs = std::collections::BTreeMap::new();
        let free = prepare(
            root.path(),
            Some(root.path()),
            "nika: local\n",
            "exe-free".into(),
            &local,
            None,
            &none,
            &inputs,
            None,
            (evidence, &ask),
        );
        assert!(
            matches!(free, Ok(RunCostPlan::Unneeded)),
            "a model-free plan never reads it"
        );
    }
}
