// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The FROZEN execution-access plan (One Door · wave 1) — every static
//! model lane resolved ONCE at composition, then CONSUMED, never
//! re-derived: the runtime seats from it and routes each task by its
//! lane, the human announce projects it, the boot manifest records it,
//! the task terminal stamps the lane that actually served.
//!
//! Before this module the same question was answered five times on one
//! run path (check · boot manifest · admission · announce · dispatch),
//! and the harness seat was gated on `--access` being TYPED — so an
//! unpinned run announced `→ codex (harness)` and then dialed the
//! provider API with whatever key sat in the environment. The law this
//! module makes structural: **resolve once · execute that exact plan
//! or refuse before task 1**. Run time may re-verify liveness; it never
//! re-plans (`nika_types::access::RejectionLayer::Liveness`).

use std::collections::BTreeMap;

use nika_types::access::{AccessClass, AccessPlan, HarnessRuntime};

use crate::probe::ProviderProbe;
use crate::resolve_access::{
    AccessCandidate, AccessRefusal, PinRefusal, VerbNeeds, access_plan_map, candidates_for,
    provider_of, refuse_pin_for_verbs, resolve_access,
};

/// What ONE static model is asked to do — the verbs that read it. A
/// harness candidate is judged against these: an ACP-only seat drives
/// `agent:`, never a one-shot `infer:` (the infer-grade attestation).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModelNeed {
    /// The requested `provider/name` (templated models never reach here).
    pub model: String,
    /// At least one `infer:` task runs on it.
    pub infer: bool,
    /// At least one `agent:` task runs on it.
    pub agent: bool,
}

impl ModelNeed {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(model: impl Into<String>, infer: bool, agent: bool) -> Self {
        Self {
            model: model.into(),
            infer,
            agent,
        }
    }
}

/// One admitted lane — the plan for one static model plus how many
/// paths competed for it. The announce's « chosen over … » tail NAMES
/// the outranked paths from `plan.outranked`; this count is its
/// fallback when no outranked row was recorded — never a second
/// enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedLane {
    /// The admitted access decision.
    pub plan: AccessPlan,
    /// Every path this machine offered for the model (admitted + rejected).
    pub candidates: usize,
}

/// The verdict for one static model.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LaneVerdict {
    /// A path was admitted.
    Admitted(ResolvedLane),
    /// No path survived — every candidate carries its witness.
    Refused(AccessRefusal),
}

/// The frozen plan for one execution attempt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExecutionAccessPlan {
    /// Every static model → its verdict (`BTreeMap` · deterministic order).
    pub lanes: BTreeMap<String, LaneVerdict>,
    /// The operator's `--access` pin, verbatim.
    pub pin: Option<String>,
    /// The ONE harness seat this run may spawn (a pinned seat serves
    /// every model; a resolved seat serves its own harness lanes).
    pub seat: Option<String>,
    /// A refused pin (NIKA-1800..1803) — the plan is not runnable.
    pub pin_refusal: Option<PinRefusal>,
}

impl ResolvedLane {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(plan: AccessPlan, candidates: usize) -> Self {
        Self { plan, candidates }
    }
}

impl ExecutionAccessPlan {
    /// Construct (INV-019) — tests and embedders that carry their own
    /// resolution; production plans come from [`resolve_execution_plan`].
    #[must_use]
    pub fn new(
        lanes: BTreeMap<String, LaneVerdict>,
        pin: Option<String>,
        seat: Option<String>,
        pin_refusal: Option<PinRefusal>,
    ) -> Self {
        Self {
            lanes,
            pin,
            seat,
            pin_refusal,
        }
    }

    /// The per-model decisions as the `check --json` rows read them
    /// (admitted plan · or the refusal with every witness).
    #[must_use]
    pub fn into_decisions(self) -> Vec<(String, Result<AccessPlan, AccessRefusal>)> {
        self.lanes
            .into_iter()
            .map(|(model, verdict)| match verdict {
                LaneVerdict::Admitted(lane) => (model, Ok(lane.plan)),
                LaneVerdict::Refused(refusal) => (model, Err(refusal)),
            })
            .collect()
    }

    /// The admitted lane for `model`, when there is one.
    #[must_use]
    pub fn lane(&self, model: &str) -> Option<&ResolvedLane> {
        match self.lanes.get(model) {
            Some(LaneVerdict::Admitted(lane)) => Some(lane),
            _ => None,
        }
    }

    /// Every admitted lane, in model order.
    pub fn admitted(&self) -> impl Iterator<Item = (&str, &ResolvedLane)> {
        self.lanes
            .iter()
            .filter_map(|(model, verdict)| match verdict {
                LaneVerdict::Admitted(lane) => Some((model.as_str(), lane)),
                LaneVerdict::Refused(_) => None,
            })
    }

    /// The first refused lane, in model order.
    #[must_use]
    pub fn first_refused(&self) -> Option<(&str, &AccessRefusal)> {
        self.lanes
            .iter()
            .find_map(|(model, verdict)| match verdict {
                LaneVerdict::Refused(refusal) => Some((model.as_str(), refusal)),
                LaneVerdict::Admitted(_) => None,
            })
    }

    /// Every static model has an admitted path and no pin was refused —
    /// the run may start. Anything else refuses before task 1.
    #[must_use]
    pub fn is_admitted(&self) -> bool {
        self.pin_refusal.is_none() && self.first_refused().is_none()
    }

    /// The harness seat that serves `model`: a pinned seat serves every
    /// model (the envelope `model:` is a hint under a seat pin, never a
    /// serves-filter); a resolved seat serves only the lanes that chose
    /// it. `None` = the provider path (API · local · mock).
    #[must_use]
    pub fn seat_for(&self, model: &str) -> Option<&str> {
        let seat = self.seat.as_deref()?;
        if self.pin_is_seat() {
            return Some(seat);
        }
        self.lane(model)
            .filter(|lane| lane.plan.chosen == AccessClass::Harness && lane.plan.access == seat)
            .map(|_| seat)
    }

    fn pin_is_seat(&self) -> bool {
        self.pin.as_deref().is_some_and(|pin| {
            HarnessRuntime::lookup(pin).is_some() || pin == AccessClass::Harness.as_str()
        })
    }
}

/// Resolve the plan for one execution attempt — the ONE derivation.
/// `needs` are the EFFECTIVE models (a `--model` override already
/// applied) with the verbs that read each; templated models (`${{ }}`)
/// are dispatch-time facts and never appear in the plan.
#[must_use]
pub fn resolve_execution_plan(
    needs: &[ModelNeed],
    probes: &[ProviderProbe],
    pin: Option<&str>,
) -> ExecutionAccessPlan {
    let verbs = VerbNeeds::new(needs.iter().any(|n| n.infer), needs.iter().any(|n| n.agent));
    resolve_execution_plan_for(needs, probes, pin, verbs)
}

/// [`resolve_execution_plan`] with the verbs the WORKFLOW carries (W3-F1:
/// a model-less `infer:` task yields no model need, so the pin judge saw
/// no infer at all and blessed a seat whose product binary was gone).
#[must_use]
pub fn resolve_execution_plan_for(
    needs: &[ModelNeed],
    probes: &[ProviderProbe],
    pin: Option<&str>,
    verbs: VerbNeeds,
) -> ExecutionAccessPlan {
    let statics: Vec<&ModelNeed> = needs.iter().filter(|n| !n.model.contains("${{")).collect();
    match pin {
        Some(pin) => pinned_plan(&statics, probes, pin, verbs),
        None => resolved_plan(&statics, probes),
    }
}

/// A pin is a pin: the pin judge (NIKA-180x · the same teaching the
/// admission gate speaks) decides first; the lanes are the pinned
/// resolution the boot manifest already stamped before this module.
fn pinned_plan(
    needs: &[&ModelNeed],
    probes: &[ProviderProbe],
    pin: &str,
    verbs: VerbNeeds,
) -> ExecutionAccessPlan {
    let models: Vec<&str> = needs.iter().map(|n| n.model.as_str()).collect();
    let has_infer = verbs.infer || needs.iter().any(|n| n.infer);
    let has_agent = verbs.agent || needs.iter().any(|n| n.agent);
    let pin_refusal =
        refuse_pin_for_verbs(models.iter().copied(), probes, pin, has_infer, has_agent);
    // The admitted lanes come from the ONE pinned resolver (a ready seat
    // pin serves EVERY static model — the envelope `model:` is a hint
    // there); every static model the resolver dropped keeps its lane as
    // REFUSED with the witnesses (wave 2: a refused lane must reach
    // `check`'s ACCESS rung; the map used to drop it silently).
    let owned: Vec<String> = models.iter().map(|m| (*m).to_owned()).collect();
    let mut lanes: BTreeMap<String, LaneVerdict> = access_plan_map(&owned, probes, Some(pin))
        .into_iter()
        .map(|(model, plan)| {
            let candidates = candidates_for(probes, provider_of(&model)).len();
            (
                model,
                LaneVerdict::Admitted(ResolvedLane { plan, candidates }),
            )
        })
        .collect();
    for model in &models {
        if lanes.contains_key(*model) {
            continue;
        }
        let candidates = candidates_for(probes, provider_of(model));
        let verdict = match resolve_access(model, &candidates, None, Some(pin)) {
            Ok(plan) => LaneVerdict::Admitted(ResolvedLane {
                plan,
                candidates: candidates.len(),
            }),
            Err(refusal) => LaneVerdict::Refused(refusal),
        };
        lanes.insert((*model).to_owned(), verdict);
    }
    let seat = if pin_refusal.is_none() {
        pinned_seat(pin, probes)
    } else {
        None
    };
    ExecutionAccessPlan {
        lanes,
        pin: Some(pin.to_owned()),
        seat,
        pin_refusal,
    }
}

fn pinned_seat(pin: &str, probes: &[ProviderProbe]) -> Option<String> {
    if let Some(rt) = HarnessRuntime::lookup(pin) {
        return Some(rt.id.to_owned());
    }
    (pin == AccessClass::Harness.as_str())
        .then(|| first_ready_seat(probes).map(str::to_owned))
        .flatten()
}

#[cfg(feature = "access-harness")]
fn first_ready_seat(probes: &[ProviderProbe]) -> Option<&str> {
    crate::resolve_access::first_ready_infer_harness(probes)
        .or_else(|| crate::resolve_access::first_ready_harness(probes))
}

#[cfg(not(feature = "access-harness"))]
fn first_ready_seat(probes: &[ProviderProbe]) -> Option<&str> {
    crate::resolve_access::first_ready_harness(probes)
}

/// The unpinned resolution: every model through the ONE resolver over
/// the candidates this machine offers, with the verb-eligibility law
/// applied to harness rows, and ONE seat per run.
fn resolved_plan(needs: &[&ModelNeed], probes: &[ProviderProbe]) -> ExecutionAccessPlan {
    let mut lanes = BTreeMap::new();
    let mut seat: Option<String> = None;
    for need in needs {
        let candidates: Vec<AccessCandidate> = candidates_for(probes, provider_of(&need.model))
            .into_iter()
            .map(|c| eligible(c, need, seat.as_deref(), probes))
            .collect();
        let verdict = match resolve_access(&need.model, &candidates, None, None) {
            Ok(plan) => {
                if plan.chosen == AccessClass::Harness && seat.is_none() {
                    seat = Some(plan.access.clone());
                }
                LaneVerdict::Admitted(ResolvedLane {
                    plan,
                    candidates: candidates.len(),
                })
            }
            Err(refusal) => LaneVerdict::Refused(refusal),
        };
        lanes.insert(need.model.clone(), verdict);
    }
    ExecutionAccessPlan {
        lanes,
        pin: None,
        seat,
        pin_refusal: None,
    }
}

/// A harness row stays a candidate but loses its admissibility (with a
/// witness the judge prints verbatim) when it cannot honor the verbs
/// that read the model — an ACP-only seat never serves a one-shot
/// `infer:` — or when another seat already holds this run (one seat per
/// run; a second adapter would be a second execution boundary).
fn eligible(
    c: AccessCandidate,
    need: &ModelNeed,
    seat: Option<&str>,
    probes: &[ProviderProbe],
) -> AccessCandidate {
    if c.class != AccessClass::Harness || !c.configured {
        return c;
    }
    if let Some(seated) = seat
        && seated != c.access
    {
        let id = c.access.clone();
        return ineligible(
            c,
            format!(
                "one seat per run · `{seated}` already holds it (pin `--access {id}` to choose)"
            ),
        );
    }
    if need.infer && !infer_grade_ready(&c.access, probes) {
        let id = c.access.clone();
        return ineligible(
            c,
            format!("`{id}` is not infer-grade for a one-shot infer: (pin it for agent: only)"),
        );
    }
    c
}

fn ineligible(c: AccessCandidate, witness: String) -> AccessCandidate {
    AccessCandidate::new(c.access, c.class, false)
        .with_billing(c.billing)
        .with_fix_var(witness)
}

fn infer_grade_ready(id: &str, probes: &[ProviderProbe]) -> bool {
    HarnessRuntime::lookup(id)
        .is_some_and(|rt| crate::resolve_access::named_infer_grade_ready(rt, probes))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    #[cfg(feature = "access-harness")]
    use nika_types::access::BillingClass;

    use super::*;
    use crate::probe::{ExecutionLocus, ProviderReadiness};

    fn api_probe(id: &str, key_present: bool) -> ProviderProbe {
        ProviderProbe::new(
            id,
            true,
            key_present,
            format!("{}_API_KEY", id.to_uppercase()),
            false,
            ProviderReadiness::new(
                true,
                key_present,
                None,
                None,
                true,
                ExecutionLocus::Cloud,
                AccessClass::Api,
            ),
            "https://api.example.com",
        )
    }

    #[cfg(feature = "access-harness")]
    fn harness_probe(id: &str, serves: &[&str], signed_in: bool) -> ProviderProbe {
        ProviderProbe::new(
            id,
            false,
            true,
            "",
            false,
            ProviderReadiness::new(
                true,
                signed_in,
                None,
                None,
                false,
                ExecutionLocus::Loopback,
                AccessClass::Harness,
            ),
            "",
        )
        .with_serves(serves.iter().map(|s| (*s).to_owned()).collect())
    }

    fn infer(model: &str) -> ModelNeed {
        ModelNeed::new(model, true, false)
    }

    /// The P0 fixture: the operator's key is set (invalid or not — the
    /// resolver never reads a value) and a ready codex row serves the
    /// same provider. Unpinned, the plan seats codex for the lane, and
    /// that seat is what the runtime routes on — never the key.
    #[cfg(feature = "access-harness")]
    #[test]
    fn a_ready_infer_grade_seat_wins_the_lane_and_is_the_runs_seat() {
        let probes = vec![
            api_probe("openai", true),
            harness_probe("codex", &["openai"], true),
        ];
        let plan = resolve_execution_plan(&[infer("openai/gpt-5-mini")], &probes, None);
        assert!(plan.is_admitted());
        let lane = plan.lane("openai/gpt-5-mini").expect("admitted");
        assert_eq!(lane.plan.access, "codex");
        assert_eq!(lane.plan.chosen, AccessClass::Harness);
        assert_eq!(lane.plan.billing, BillingClass::Unknown, "never guessed");
        assert_eq!(
            lane.candidates, 2,
            "the announce counts the api path it outranked"
        );
        assert_eq!(plan.seat.as_deref(), Some("codex"));
        assert_eq!(plan.seat_for("openai/gpt-5-mini"), Some("codex"));
    }

    /// An ACP-only seat (claude-code · gemini-cli …) is a candidate for
    /// an `agent:` lane but never for a one-shot `infer:` — the API path
    /// wins the infer lane and the witness says why the seat stepped
    /// aside. This is the lane the shipped 0.116.2 announced as
    /// `→ gemini-cli (harness)` while executing over the API.
    #[cfg(feature = "access-harness")]
    #[test]
    fn an_acp_only_seat_never_serves_an_infer_lane() {
        let probes = vec![
            api_probe("gemini", true),
            harness_probe("gemini-cli", &["gemini"], true),
        ];
        let plan = resolve_execution_plan(&[infer("gemini/gemini-2.5-flash")], &probes, None);
        let lane = plan
            .lane("gemini/gemini-2.5-flash")
            .expect("the api lane admits");
        assert_eq!(lane.plan.chosen, AccessClass::Api);
        assert_eq!(lane.candidates, 2);
        let witness = lane.plan.rejected.iter().find(|r| r.access == "gemini-cli");
        let witness = witness.expect("the seat is rejected with a witness, never silently dropped");
        assert!(
            witness.witness.contains("not infer-grade"),
            "{}",
            witness.witness
        );
        assert_eq!(plan.seat, None, "no harness lane → no seat is spawned");
        assert_eq!(plan.seat_for("gemini/gemini-2.5-flash"), None);
    }

    /// The same ACP-only seat DOES serve an `agent:` lane.
    #[cfg(feature = "access-harness")]
    #[test]
    fn an_acp_seat_serves_an_agent_lane() {
        let probes = vec![
            api_probe("anthropic", true),
            harness_probe("claude-code", &["anthropic"], true),
        ];
        let need = ModelNeed::new("anthropic/claude-sonnet-4-6", false, true);
        let plan = resolve_execution_plan(&[need], &probes, None);
        let lane = plan.lane("anthropic/claude-sonnet-4-6").expect("admitted");
        assert_eq!(lane.plan.access, "claude-code");
        assert_eq!(plan.seat.as_deref(), Some("claude-code"));
    }

    /// No configured path at all: the lane is REFUSED with the env-var
    /// witness — the plan is not admitted, the run stops before task 1
    /// (the key used to « sail to the endpoint »).
    #[test]
    fn an_unconfigured_only_path_refuses_the_lane_before_task_one() {
        let probes = vec![api_probe("mistral", false)];
        let plan = resolve_execution_plan(&[infer("mistral/mistral-small-latest")], &probes, None);
        assert!(!plan.is_admitted());
        let (model, refusal) = plan.first_refused().expect("refused");
        assert_eq!(model, "mistral/mistral-small-latest");
        assert!(
            refusal.rejected[0]
                .witness
                .contains("MISTRAL_API_KEY unset"),
            "{}",
            refusal.rejected[0].witness
        );
    }

    /// The mock rehearsal always resolves — probes exclude it, the
    /// bridge synthesizes its keyless candidate (one candidate: the
    /// announce stays silent).
    #[test]
    fn mock_resolves_keyless_on_every_machine() {
        let plan = resolve_execution_plan(&[infer("mock/echo")], &[], None);
        let lane = plan.lane("mock/echo").expect("mock admits");
        assert_eq!(lane.plan.chosen, AccessClass::Mock);
        assert_eq!(lane.candidates, 1);
        assert!(plan.is_admitted());
    }

    /// A templated model is not a static fact — absent from the plan,
    /// never guessed (the dispatch layer judges it).
    #[test]
    fn templated_models_never_enter_the_plan() {
        let plan = resolve_execution_plan(
            &[infer("mock/echo"), infer("${{ inputs.model }}")],
            &[],
            None,
        );
        assert_eq!(plan.lanes.len(), 1);
        assert!(plan.is_admitted());
    }

    /// An explicit `--access api` keeps the metered path even when a
    /// ready seat outranks it — a pin is a pin (never a substitute), and
    /// the plan spawns no seat.
    #[cfg(feature = "access-harness")]
    #[test]
    fn an_api_pin_keeps_the_key_path_and_spawns_no_seat() {
        let probes = vec![
            api_probe("openai", true),
            harness_probe("codex", &["openai"], true),
        ];
        let plan = resolve_execution_plan(&[infer("openai/gpt-5-mini")], &probes, Some("api"));
        assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
        let lane = plan.lane("openai/gpt-5-mini").expect("pinned api lane");
        assert_eq!(lane.plan.chosen, AccessClass::Api);
        assert!(lane.plan.pinned);
        assert_eq!(plan.seat, None);
        assert_eq!(plan.seat_for("openai/gpt-5-mini"), None);
    }

    /// A seat pin serves EVERY static model (the envelope `model:` is a
    /// hint under a seat pin) and the seat is the pinned product token.
    #[cfg(feature = "access-harness")]
    #[test]
    fn a_seat_pin_serves_every_model() {
        let probes = vec![harness_probe("codex", &["openai"], true)];
        let plan = resolve_execution_plan(
            &[
                infer("openai/gpt-5-mini"),
                infer("anthropic/claude-sonnet-4-6"),
            ],
            &probes,
            Some("codex"),
        );
        assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
        assert_eq!(plan.seat.as_deref(), Some("codex"));
        assert_eq!(plan.seat_for("anthropic/claude-sonnet-4-6"), Some("codex"));
        for (_, lane) in plan.admitted() {
            assert_eq!(
                lane.plan.billing,
                BillingClass::Unknown,
                "a seat's lane is never priced"
            );
        }
    }

    /// An unsatisfied pin refuses the whole plan with the pin judge's
    /// own teaching (NIKA-180x) — the runtime never sees it.
    #[test]
    fn an_unknown_pin_token_refuses_the_plan() {
        let plan = resolve_execution_plan(&[infer("mock/echo")], &[], Some("locale"));
        assert!(!plan.is_admitted());
        assert!(matches!(
            plan.pin_refusal,
            Some(PinRefusal::UnknownToken { .. })
        ));
        assert_eq!(plan.seat, None);
    }

    /// One seat per run: once a lane rides codex, another ready seat
    /// that serves a second provider steps aside with a witness, and the
    /// second lane falls to its next path.
    #[cfg(feature = "access-harness")]
    #[test]
    fn one_seat_per_run_the_second_harness_row_steps_aside() {
        let probes = vec![
            api_probe("openai", true),
            api_probe("anthropic", true),
            harness_probe("codex", &["openai"], true),
            harness_probe("claude-code", &["anthropic"], true),
        ];
        let needs = [
            ModelNeed::new("anthropic/claude-sonnet-4-6", false, true),
            infer("openai/gpt-5-mini"),
        ];
        let plan = resolve_execution_plan(&needs, &probes, None);
        assert!(plan.is_admitted());
        assert_eq!(
            plan.seat.as_deref(),
            Some("claude-code"),
            "the first harness lane seats"
        );
        let second = plan.lane("openai/gpt-5-mini").expect("admitted");
        assert_eq!(
            second.plan.chosen,
            AccessClass::Api,
            "codex steps aside · api serves"
        );
        let witness = second
            .plan
            .rejected
            .iter()
            .find(|r| r.access == "codex")
            .expect("witness");
        assert!(
            witness.witness.contains("one seat per run"),
            "{}",
            witness.witness
        );
        assert_eq!(plan.seat_for("openai/gpt-5-mini"), None);
        assert_eq!(
            plan.seat_for("anthropic/claude-sonnet-4-6"),
            Some("claude-code")
        );
    }

    /// W3-F1 · a seat is TWO binaries: with the product gone and only the
    /// ACP speaker on PATH, a pin serving an `infer:` refuses (the
    /// infer-grade seat spawns the product); a pin serving an `agent:`
    /// still stands (ACP is the agent's door).
    #[cfg(feature = "access-harness")]
    #[test]
    fn a_pinned_seat_without_its_product_binary_refuses_an_infer_workflow() {
        let probes = vec![
            api_probe("openai", true),
            harness_probe("codex", &["openai"], true).with_product_present(false),
        ];
        let infer_only =
            resolve_execution_plan_for(&[], &probes, Some("codex"), VerbNeeds::new(true, false));
        assert!(
            matches!(infer_only.pin_refusal, Some(PinRefusal::Unavailable { .. })),
            "{:?}",
            infer_only.pin_refusal
        );
        assert!(infer_only.seat.is_none(), "a refused pin seats nothing");
        let agent_only =
            resolve_execution_plan_for(&[], &probes, Some("codex"), VerbNeeds::new(false, true));
        assert!(
            agent_only.pin_refusal.is_none(),
            "{:?}",
            agent_only.pin_refusal
        );
        assert_eq!(agent_only.seat.as_deref(), Some("codex"));
        // A static model does not change the verdict: the pin still refuses.
        let seated = resolve_execution_plan(&[infer("openai/gpt-5.2")], &probes, Some("codex"));
        assert!(seated.pin_refusal.is_some(), "{:?}", seated.pin_refusal);
        assert!(!seated.is_admitted());
    }

    /// W3-F3 · a READY path that lost the ranking rides the lane's
    /// rejections, so the machine row says a choice happened.
    #[cfg(feature = "access-harness")]
    #[test]
    fn an_outranked_ready_path_rides_the_lane_outranked_rows() {
        let probes = vec![
            api_probe("openai", true),
            harness_probe("codex", &["openai"], true),
        ];
        let plan = resolve_execution_plan(&[infer("openai/gpt-5-mini")], &probes, None);
        let lane = plan.lane("openai/gpt-5-mini").expect("admitted");
        assert_eq!(lane.plan.access, "codex");
        assert!(
            lane.plan.rejected.is_empty(),
            "available to a pin: {:?}",
            lane.plan.rejected
        );
        assert_eq!(lane.plan.outranked.len(), 1, "{:?}", lane.plan.outranked);
        let loser = &lane.plan.outranked[0];
        assert_eq!(loser.access, "openai");
        assert_eq!(
            loser.dimension,
            nika_types::access::RejectionDimension::Outranked
        );
        assert!(
            loser.witness.contains("ranked below `codex`"),
            "{}",
            loser.witness
        );
        assert_eq!(
            lane.candidates,
            lane.plan.outranked.len() + 1,
            "one winner, the rest named"
        );
    }
}
