// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The operator's standing cost policy on this server (`nika serve
//! --cost-policy`, health `costStandingPolicyV1`): a Run on an unpriced API
//! route that one of its rules names is admitted with no per-run cost review.
//! The authority is this server's startup composition (the rules and the
//! per-run ceiling), never a request, a job body or a workflow. Every judgment
//! is the shared host evaluator's (`nika_cli_host::run_cost`): the project's
//! cost lease, what earlier Runs left, the Run's typed dispatch bound. Every
//! record of an admitted Run names the rule that admitted it.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use nika_cli_host::run_cost::{RunCost, standing_rule};
use nika_error::prelude::{NikaCode, NikaErrorCode, codes};
use nika_providers::admission::{
    CapEvidence, CostHostEvidence, HardMonetaryCap, StandingCostPolicy, StandingRule,
    monetary_default,
};
use nika_service_execution::{ExecutionAccessPlan, ServiceExecutionDriver};
use serde_json::{Value, json};

use super::ServerConfig;

/// The health capability of a server whose backend holds a standing cost policy.
pub(super) const CAPABILITY: &str = "costStandingPolicyV1";

/// The policy's contract, RFC 7386 merge patches over the served document: the
/// `cost_admission` a job's settlement carries, and the review door's answer
/// when both are seated. Neither adds a key to an object a published SDK
/// closes (`openapi::published_sdk_tests`).
pub(super) const OPENAPI: &str = include_str!("cost_policy/openapi.json");
pub(super) const OPENAPI_REVIEW: &str = include_str!("cost_policy/openapi-review.json");

/// What a job hears when the policy names other routes than its own.
pub(super) const UNNAMED: &str = "price unknown: this server's standing cost policy names other routes only, so this Run needs a fresh one-time cost review (a server started with --cost-review admits POST /v1/cost-reviews, then one job carrying that review)";

/// What the review door answers for a Run the policy admits.
pub(super) const NOT_REQUIRED: &str = "this server's standing cost policy names this Run's route and its bounds cover it: the job needs no review; its admission is judged again when it runs and counted in the rule's window";

const POLICY: &str = "this server's standing cost policy (`nika serve --cost-policy`), read at startup: it bounds requests, output, time and Runs per window, and sets no USD cap of its own";
const DISARMED: &str = "no per-run ceiling binds this job: this server's was explicitly disarmed at startup (`--run-cost-ceiling none`) and the job carries none";
const MANUAL: &str = "authenticated job request, not a scheduled occurrence";
const SCHEDULED: &str = "scheduled occurrence: its required maxCostUsd is the per-run ceiling judged as the machine layer";

/// The policy as the production backend holds it: the rules, and the turn its
/// Runs take. One unknown-cost Run holds the project's cost lease at a time,
/// so a second Run a rule names waits for the first instead of failing on the
/// lease; nothing is queued across processes or restarts.
pub(super) struct Seat {
    policy: StandingCostPolicy,
    turn: Arc<tokio::sync::Mutex<()>>,
}

impl Seat {
    pub(super) fn new(policy: StandingCostPolicy) -> Self {
        Self {
            policy,
            turn: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub(super) fn policy(&self) -> &StandingCostPolicy {
        &self.policy
    }

    /// The turn of one Run a rule names, held until that Run's account settled.
    pub(super) async fn turn(&self) -> tokio::sync::OwnedMutexGuard<()> {
        Arc::clone(&self.turn).lock_owned().await
    }
}

/// What a job with no reviewed authority is judged with on a server that holds
/// a standing cost policy: the policy, the per-run ceiling the job runs under
/// (this server's, restricted by the job's own or its schedule's) and whether
/// a schedule fired it.
pub(super) struct StandingJob<'a> {
    pub(super) policy: &'a StandingCostPolicy,
    pub(super) ceiling: Option<f64>,
    pub(super) scheduled: bool,
}

impl StandingJob<'_> {
    /// The rule that names the one unknown-cost route of `plan`, if one does.
    /// A plan with several such routes has no finite Run shape and no rule.
    pub(super) fn rule(&self, plan: &ExecutionAccessPlan) -> Option<&StandingRule> {
        let config = nika_runtime::compose::config_from_env();
        let unknown = nika_service_execution::run_cost::unknown_routes(plan, &config).ok()?;
        match unknown.as_slice() {
            [(_, route)] => self.policy.rule_for(route),
            _ => None,
        }
    }

    /// This server's startup evidence for a Run a rule names: the policy as
    /// the policy layer (read, and setting no USD cap of its own), the per-run
    /// ceiling the job runs under as the machine layer (a present one is a hard
    /// cap the Run's declared worst case must fit under; only an explicit
    /// disarm is observed absent), and the job's origin.
    pub(super) fn evidence(&self) -> CostHostEvidence {
        let policy = CapEvidence::Observed {
            cap: HardMonetaryCap::Absent,
            origin: POLICY.into(),
        };
        let machine = match self.ceiling.map(|usd| monetary_default(Some(usd))) {
            Some(Ok(Some(cap))) => CapEvidence::Observed {
                cap: HardMonetaryCap::Capped(cap),
                origin: format!(
                    "the per-run ceiling this job runs under ({cap}): this server's `--run-cost-ceiling`, restricted by the job's own or its schedule's"
                ),
            },
            Some(_) => CapEvidence::Unknown,
            None => CapEvidence::Observed {
                cap: HardMonetaryCap::Absent,
                origin: DISARMED.into(),
            },
        };
        let occurrence = CapEvidence::NotApplicable {
            origin: if self.scheduled { SCHEDULED } else { MANUAL }.into(),
        };
        CostHostEvidence::new(true, policy, machine, occurrence)
            .with_standing_policy(self.policy.clone())
    }

    /// The refusal of a Run a rule names when the job's ceiling stops it before
    /// any review: the one refusal whose remedy this server teaches. A ceiling
    /// that reads as a USD amount stops only a rule that declares no price (a
    /// declared worst case is judged against it instead). One that does not
    /// read stops every rule, and the refusal says that rather than blame the
    /// rule. A job under no ceiling never reaches this refusal.
    pub(super) fn capped(&self, rule: &StandingRule) -> String {
        let Some(Ok(Some(ceiling))) = self.ceiling.map(|usd| monetary_default(Some(usd))) else {
            return format!(
                "price unknown: the per-run ceiling this job runs under does not read as a USD amount, so the standing cost rule {} admits nothing under it",
                rule.id()
            );
        };
        let remedy = if self.scheduled {
            "a schedule always carries maxCostUsd, so its rule must declare input-usd and output-usd"
        } else {
            "declare input-usd and output-usd on the rule, or start with --run-cost-ceiling none"
        };
        format!(
            "price unknown: a {ceiling} ceiling binds this job and the standing cost rule {} declares no price, so the ceiling cannot bound it: {remedy}",
            rule.id()
        )
    }

    /// Whether a job of this Run would be inside the policy now: the rule that
    /// names it, as a public document shows it. The shared evaluator judges
    /// it without the project's cost lease (`run_cost::standing_rule`: the
    /// route, the Run's bounds, every ceiling, the rule's window), so the
    /// answer never waits on a Run in flight and counts or holds nothing; the
    /// job's own admission judges it again and reads what earlier Runs left.
    /// `None`: no rule names the Run, the Run is outside its rule, the window
    /// is full or unreadable, or `sequential_only` (version 1 of the review
    /// door) and the Run fans out or retries.
    pub(super) fn admits(
        &self,
        root: &Path,
        driver: &ServiceExecutionDriver,
        plan: &ExecutionAccessPlan,
        inputs: &BTreeMap<String, Value>,
        sequential_only: bool,
    ) -> Option<Value> {
        let (wf, invocation) = (driver.workflow(), driver.execution_id().to_string());
        let now = std::time::SystemTime::now();
        let rule = standing_rule(root, &invocation, wf, plan, inputs, self.evidence(), now);
        let rule = rule.ok().flatten()?;
        let bound = nika_service_execution::run_cost::dispatch_bound(wf, plan, 1, inputs);
        if sequential_only && bound.is_ok_and(|bound| bound.multiplied()) {
            return None;
        }
        Some(rule.view())
    }

    /// What the job's record says of a Run this policy admitted, inside its
    /// settlement (`nika/cost-admission@1`): that a standing rule admitted it,
    /// whether its price stayed unknown or was declared by the operator, the
    /// bounded choice its account enforces (named by its route's origin, never
    /// its endpoint) and the evidence it was judged under. `None` for any
    /// other account.
    pub(super) fn admission(&self, cost: &RunCost) -> Option<Value> {
        let receipt = cost.account.snapshot().ok()?;
        let choice = receipt.unknown_cost.as_ref()?;
        choice.standing_policy()?;
        let price = if choice.declared_worst_case().is_some() {
            "host_declared"
        } else {
            "unknown"
        };
        let durable = receipt.durable_observation();
        Some(json!({
            "schema": "nika/cost-admission@1",
            "authority": "standing_policy",
            "price": price,
            "choice": durable["unknown_cost"],
            "host": self.evidence().view(),
        }))
    }
}

/// Apply `nika serve --cost-policy <RULE>` (repeatable) to the optional
/// listener: each rule is read and its route observed under this process's
/// provider configuration, now, so a server that starts holds exactly the
/// policy its operator wrote. It changes no ceiling and seats no review door.
///
/// # Errors
/// A rule without a listener, or a rule this host cannot hold
/// ([`StandingCostPolicy::resolve`]: it does not read, its model cannot be
/// admitted at unknown cost, the catalog already prices its route, or another
/// rule names the same route).
pub fn seat_cost_policy(
    server: Option<ServerConfig>,
    rules: &[String],
) -> Result<Option<ServerConfig>, CostPolicyError> {
    if rules.is_empty() {
        return Ok(server);
    }
    let Some(server) = server else {
        return Err(CostPolicyError::NeedsListener);
    };
    let config = nika_runtime::compose::config_from_env();
    let policy = StandingCostPolicy::resolve(rules, &config).map_err(CostPolicyError::Rule)?;
    Ok(Some(server.with_cost_policy(policy)))
}

/// Why a standing cost policy cannot be held. Refused before the listener binds.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum CostPolicyError {
    /// A rule was written without the HTTP listener whose jobs it admits.
    #[error(
        "a standing cost policy admits the HTTP listener's jobs: pass --bind, --workflows and --token-file"
    )]
    NeedsListener,
    /// A rule this host cannot hold, in the policy reader's own sentence.
    #[error("{0}")]
    Rule(String),
}

/// An operator's configuration refusal, as every `nika serve` launch refusal is.
impl NikaErrorCode for CostPolicyError {
    fn nika_code(&self) -> NikaCode {
        codes::NIKA_001
    }
}

#[cfg(test)]
mod tests;
