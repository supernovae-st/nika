// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Host-owned review of unknown monetary cost. Never a permit or an invoice.
#[cfg(test)]
mod bounded_tests;
mod exchange;
pub use exchange::{CostChallenge, CostResponse, PendingCostReview};

use super::{HardMonetaryCap, UnknownCostChoice, UnknownCostPolicy};
use crate::{InferenceAdmission, ProviderRegistry, ProvidersConfig, WireFormat};
use nika_types::cost::Cost;
use std::sync::Arc;
use std::time::Duration;

// Object-safe projection of the kernel clock's monotonic read. The asynchronous
// ClockDyn sleep surface is not object-safe and is not needed by a review.
trait ReviewClock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> std::time::Instant;
}
impl<C: nika_kernel::clock::ClockDyn + std::fmt::Debug> ReviewClock for C {
    fn now(&self) -> std::time::Instant {
        nika_kernel::clock::ClockDyn::now(self)
    }
}

/// Evidence from one actually applicable host configuration layer.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum CapEvidence {
    /// The owner read the applicable configuration and observed its constraint.
    Observed {
        cap: HardMonetaryCap,
        origin: String,
    },
    /// This host composition has no such layer; not a claim that a file was read.
    NotApplicable { origin: String },
    /// Missing, unreadable, or no host evidence. Refuses unknown cost.
    Unknown,
}
impl CapEvidence {
    /// The layer for a public surface: its class, its origin, its cap. No credential rides it.
    fn view(&self) -> serde_json::Value {
        match self {
            Self::Observed { cap, origin } => serde_json::json!({
                "class": "observed", "origin": origin, "cap": cap_view(*cap)}),
            Self::NotApplicable { origin } => {
                serde_json::json!({"class": "not_applicable", "origin": origin})
            }
            Self::Unknown => serde_json::json!({"class": "unknown"}),
        }
    }
    fn cap(&self) -> HardMonetaryCap {
        match self {
            Self::Observed { cap, origin } if !origin.trim().is_empty() => *cap,
            Self::NotApplicable { origin } if !origin.trim().is_empty() => HardMonetaryCap::Absent,
            _ => HardMonetaryCap::Unknown,
        }
    }
}
/// Supplied by the host, never by a model or persisted conversation.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CostHostEvidence {
    allowed: bool,
    layers: [CapEvidence; 3],
}
impl Default for CostHostEvidence {
    fn default() -> Self {
        Self::new(
            false,
            CapEvidence::Unknown,
            CapEvidence::Unknown,
            CapEvidence::Unknown,
        )
    }
}
impl CostHostEvidence {
    /// Evidence for policy, machine and occurrence respectively.
    #[must_use]
    pub fn new(
        allowed: bool,
        policy: CapEvidence,
        machine: CapEvidence,
        occurrence: CapEvidence,
    ) -> Self {
        Self {
            allowed,
            layers: [policy, machine, occurrence],
        }
    }
    /// Attestation for the built-in unmanaged interactive local CLI only.
    /// Its composition has no configured policy/machine-cap source and is not
    /// a scheduled occurrence. The caller must observe that launch scope and
    /// successfully read project/invocation defaults separately on EACH review.
    /// Serve, ARM, library and configured-policy hosts must use `new` instead.
    #[must_use]
    pub fn unmanaged_interactive_local() -> Self {
        Self::new(true,
            CapEvidence::NotApplicable { origin: "local interactive CLI composition: no policy source configured or supported".into() },
            CapEvidence::NotApplicable { origin: "local interactive CLI composition: no machine monetary-cap source configured or supported".into() },
            CapEvidence::NotApplicable { origin: "observed interactive invocation, not an ARM/scheduled occurrence".into() })
    }
    /// The evidence as a public surface shows it: whether the host permits an
    /// unknown-cost choice and, per layer, what it observed. Never a credential.
    #[must_use]
    pub fn view(&self) -> serde_json::Value {
        serde_json::json!({
            "allowed": self.allowed,
            "policy": self.layers[0].view(),
            "machine": self.layers[1].view(),
            "occurrence": self.layers[2].view(),
        })
    }
    /// The refusal this evidence gives every unknown-cost choice (a hard cap,
    /// a denied or unknown layer) before any review is framed, in the words
    /// [`CostReview::new`] gives; `None` when a fresh choice may be reviewed.
    /// A host teaches its own cap's remedy beside this refusal only.
    #[must_use]
    pub fn unknown_cost_refusal(&self) -> Option<String> {
        self.policy(None, None)
            .validate()
            .err()
            .map(|e| e.to_string())
    }
    fn policy(&self, invocation: Option<Cost>, project: Option<Cost>) -> UnknownCostPolicy {
        UnknownCostPolicy::new(
            self.allowed,
            self.layers[0].cap(),
            self.layers[1].cap(),
            self.layers[2].cap(),
            invocation,
            project,
        )
    }
}

fn cap_view(cap: HardMonetaryCap) -> serde_json::Value {
    match cap {
        HardMonetaryCap::Absent => serde_json::json!("absent"),
        HardMonetaryCap::Capped(limit) => serde_json::json!({"capped_usd": limit.to_string()}),
        _ => serde_json::json!("unknown"),
    }
}

/// Exact adapter, resolved wire model and full endpoint from the existing registry.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CostRoute {
    pub provider: String,
    pub model: String,
    pub endpoint: String,
}
impl CostRoute {
    /// Keyless route observation using the same configuration as the caller.
    /// Unknown-cost transport currently supports HTTPS OpenAI-compatible text only,
    /// on an endpoint in its canonical form ([`crate::canonical_endpoint`]): one
    /// the URL parser would rewrite, or one with userinfo, a query or a fragment,
    /// is refused before any review, naming at most its origin.
    /// # Errors
    /// Unsupported adapter, malformed model, missing or noncanonical endpoint.
    pub fn observe(model: &str, config: ProvidersConfig) -> Result<Self, String> {
        let (provider, name) = model
            .split_once('/')
            .ok_or("select a provider-qualified model")?;
        let provider = crate::canonical_provider(provider);
        let registry = ProviderRegistry::without_http(config);
        let profile = registry
            .profiles()
            .iter()
            .find(|p| p.id == provider)
            .ok_or("the selected provider is not supported by unknown-cost admission")?;
        if profile.wire != WireFormat::OpenAiCompat {
            return Err("unknown-cost admission supports HTTPS OpenAI-compatible text only; subscription authorization is separate".into());
        }
        let endpoint = registry
            .effective_base_url(provider)
            .ok_or("selected endpoint is unknown")?;
        if !endpoint.starts_with("https://") || name.is_empty() {
            return Err("unknown-cost admission requires an exact HTTPS route and model".into());
        }
        if !crate::canonical_endpoint(endpoint) {
            let origin = crate::route_origin(endpoint).unwrap_or_else(|| "this provider".into());
            return Err(format!(
                "unknown-cost admission requires the endpoint configured for {origin} in its canonical form (as the URL parser writes it, with a path and no userinfo, query or fragment): nothing was reviewed or sent"
            ));
        }
        Ok(Self {
            provider: provider.into(),
            model: profile.resolve_model(name).into(),
            endpoint: endpoint.into(),
        })
    }
    /// Scheme, host and effective port of the endpoint, from the URL parser
    /// ([`crate::route_origin`]): where the request goes, never userinfo, path,
    /// query or fragment, and `unknown origin` when it has none to project (a
    /// credential is refused, never stripped). The exact endpoint stays bound
    /// through the review's private witness.
    #[must_use]
    pub fn origin(&self) -> String {
        crate::route_origin(&self.endpoint).unwrap_or_else(|| "unknown origin".into())
    }
    /// Whether numeric USD catalog admission cannot qualify this exact route.
    #[must_use]
    pub fn needs_unknown_choice(&self) -> bool {
        InferenceAdmission::qualify(&self.provider, &self.model, &self.endpoint).is_err()
    }
}

/// Whether the existing catalog prices a native non-compatible API route.
/// Preserves that legacy paid path; an override never borrows its native price.
#[must_use]
pub fn native_catalog_price_known(model: &str, config: ProvidersConfig) -> bool {
    let Some((provider, name)) = model.split_once('/') else {
        return false;
    };
    let provider = crate::canonical_provider(provider);
    let registry = ProviderRegistry::without_http(config);
    registry
        .profiles()
        .iter()
        .find(|p| p.id == provider)
        .is_some_and(|p| {
            p.wire != WireFormat::OpenAiCompat
                && registry.effective_base_url(provider) == Some(p.base_url)
                && nika_catalog::find_pricing_scoped(provider, p.resolve_model(name)).is_some()
        })
}

/// The Run's monetary class of one API route: one predicate for a literal
/// route the Run reviews before it starts and a route rendered at run time.
/// `Ok(None)`: a qualified admission tariff or a native catalog price admits
/// it. `Ok(Some(route))`: its USD cost is unknown, so only a fresh choice may.
/// # Errors
/// Neither an unknown-cost route nor a native price can judge the model.
pub fn unknown_cost_route(
    model: &str,
    config: ProvidersConfig,
) -> Result<Option<CostRoute>, String> {
    match CostRoute::observe(model, config.clone()) {
        Ok(route) if route.needs_unknown_choice() => Ok(Some(route)),
        Ok(_) => Ok(None),
        Err(_) if native_catalog_price_known(model, config) => Ok(None),
        Err(why) => Err(why),
    }
}

/// The per-request output ceiling of a Run review (and of any review not built for a Session).
pub const RUN_REVIEW_MAX_OUTPUT_TOKENS: u32 = 8192;
/// The per-request deadline of a Run review (and of any review not built for a Session).
pub const RUN_REVIEW_TIMEOUT: Duration = Duration::from_secs(120);
/// The requests one Session authoring turn may make under a fresh unknown-cost review: the turn
/// classification (1), the COLD plan with its one evidence repair (2), the native candidate (1)
/// and its three repair rounds (3) — a reported truncation spends one of those repairs to widen
/// the output, never a transport retry. A stronger-model retry never runs under an account.
pub const SESSION_REVIEW_MAX_REQUESTS: u32 = 7;
/// The Session's hard per-request output ceiling (its first native call starts lower and may
/// widen up to this only after a reported truncation).
pub const SESSION_REVIEW_MAX_OUTPUT_TOKENS: u32 = 32_768;
/// The Session's per-request deadline.
pub const SESSION_REVIEW_TIMEOUT: Duration = Duration::from_secs(180);

/// One pending review. No callable admission handle exists until confirmation.
#[derive(Debug)]
#[non_exhaustive]
pub struct CostReview {
    candidate: String,
    invocation: String,
    route: CostRoute,
    prior_report: Option<(String, String)>,
    evidence: CostHostEvidence,
    defaults: [Option<Cost>; 2],
    max_requests: u32,
    max_in_flight: u32,
    breakdown: Vec<String>,
    authored_retry: bool,
    max_output_tokens: u32,
    request_timeout: Duration,
    reviewed_at: std::time::Instant,
    clock: Arc<dyn ReviewClock>,
}
impl CostReview {
    /// One request, [`RUN_REVIEW_MAX_OUTPUT_TOKENS`] output tokens, [`RUN_REVIEW_TIMEOUT`];
    /// retries remain zero.
    /// # Errors
    /// Invalid review identity or host evidence that cannot admit unknown cost.
    pub fn new(
        candidate: String,
        invocation: String,
        route: CostRoute,
        evidence: CostHostEvidence,
        invocation_default: Option<Cost>,
        project_default: Option<Cost>,
    ) -> Result<Self, String> {
        Self::new_with_clock(
            candidate,
            invocation,
            route,
            evidence,
            invocation_default,
            project_default,
            Arc::new(nika_clock::SystemClock),
        )
    }
    /// Create a review using the host's kernel clock for its whole lifetime.
    /// # Errors
    /// Invalid review identity or host evidence that cannot admit unknown cost.
    pub fn new_with_clock<C: nika_kernel::clock::ClockDyn + std::fmt::Debug + 'static>(
        candidate: String,
        invocation: String,
        route: CostRoute,
        evidence: CostHostEvidence,
        invocation_default: Option<Cost>,
        project_default: Option<Cost>,
        clock: Arc<C>,
    ) -> Result<Self, String> {
        let review = Self {
            candidate,
            invocation,
            route,
            prior_report: None,
            evidence,
            defaults: [invocation_default, project_default],
            max_requests: 1,
            max_in_flight: 1,
            breakdown: Vec::new(),
            authored_retry: false,
            max_output_tokens: RUN_REVIEW_MAX_OUTPUT_TOKENS,
            request_timeout: RUN_REVIEW_TIMEOUT,
            reviewed_at: nika_kernel::clock::ClockDyn::now(clock.as_ref()),
            clock,
        };
        // Validate evidence and identity without creating reusable authority.
        let account = InferenceAdmission::new_unknown(review.choice()?, review.policy())
            .map_err(|e| e.to_string())?;
        account
            .close("review only; not confirmed")
            .map_err(|e| e.to_string())?;
        Ok(review)
    }
    /// Session classifier/reader/candidate share one finite review, sized to the calls one
    /// Session authoring turn may actually make ([`SESSION_REVIEW_MAX_REQUESTS`]) with the
    /// Session's per-request output ceiling and deadline. A Run review keeps its own bounds.
    #[must_use]
    pub fn for_session(mut self) -> Self {
        self.max_requests = SESSION_REVIEW_MAX_REQUESTS;
        self.max_output_tokens = SESSION_REVIEW_MAX_OUTPUT_TOKENS;
        self.request_timeout = SESSION_REVIEW_TIMEOUT;
        self
    }

    /// Reserve `requests` more in this same review and account for an explicit source recovery
    /// the operator configured (the compiler's `recovery_requests`). The question states the
    /// allowance they are added to and `worst_case`, the configuration's theoretical bound; none
    /// reserved changes nothing, never a second account and never a retry.
    #[must_use]
    pub fn with_recovery_requests(mut self, requests: u32, worst_case: u32) -> Self {
        if requests > 0 {
            self.breakdown.push(format!(
                "Allowance {} + {requests} reserved for the explicit source recovery the operator configured; this configuration's theoretical worst case is {worst_case} requests, so this bound may stop it first.",
                self.max_requests
            ));
            self.max_requests = self.max_requests.saturating_add(requests);
        }
        self
    }

    /// Display retained legacy exposure beside this fresh invocation. The host must bind
    /// the report and durable record to the candidate and re-observe both before confirming.
    /// This never restores a numeric account or settles an earlier charge.
    #[must_use]
    pub fn after_legacy(mut self, report: super::LegacyCostReport) -> Self {
        self.prior_report = Some(report.into_display());
        self
    }

    /// Show completed prior scopes without reusing their authority or reconciling their price.
    /// The host binds the report/project witness and observes them again before confirmation.
    #[must_use]
    pub fn after_completed(mut self, report: super::CompletedCostReport) -> Self {
        self.prior_report = Some(report.into_display());
        self
    }

    /// The requests this review would admit.
    #[must_use]
    pub fn max_requests(&self) -> u32 {
        self.max_requests
    }

    /// The per-request output ceiling this review would admit.
    #[must_use]
    pub fn max_output_tokens(&self) -> u32 {
        self.max_output_tokens
    }

    /// The per-request deadline this review would admit.
    #[must_use]
    pub fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// The requests, per-request output tokens and per-request deadline this
    /// review would admit, together: the triple a host shows, from its owner.
    #[must_use]
    pub fn bounds(&self) -> (u32, u32, Duration) {
        (
            self.max_requests,
            self.max_output_tokens,
            self.request_timeout,
        )
    }

    /// A Run host derives this upper bound from checked, sequential direct infers.
    /// This builder creates no authority; confirmation still binds one invocation.
    /// # Errors
    /// A zero bound cannot authorize a model request.
    pub fn for_run(mut self, max_requests: u32) -> Result<Self, String> {
        if max_requests == 0 {
            return Err("unknown-cost Run requires a positive structural request bound".into());
        }
        self.max_requests = max_requests;
        Ok(self)
    }

    /// At most `max_in_flight` of the Run's requests at once, inside the same
    /// total; the confirmed choice reserves both atomically.
    /// # Errors
    /// Zero, or more than the review's total.
    pub fn with_concurrency(mut self, max_in_flight: u32) -> Result<Self, String> {
        if max_in_flight == 0 || max_in_flight > self.max_requests {
            return Err("unknown-cost concurrency must be positive and within the total".into());
        }
        self.max_in_flight = max_in_flight;
        Ok(self)
    }

    /// The finite breakdown the question shows, one line per task.
    #[must_use]
    pub fn with_breakdown(mut self, lines: Vec<String>) -> Self {
        self.breakdown = lines;
        self
    }

    /// Whether the Run authored retries inside its total: only then may a
    /// received 429 or 503 be followed by another request, and the question
    /// says so.
    #[must_use]
    pub fn with_authored_retry(mut self, authored: bool) -> Self {
        self.authored_retry = authored;
        self
    }

    /// The requests this review would admit in flight at once.
    #[must_use]
    pub fn max_in_flight(&self) -> u32 {
        self.max_in_flight
    }

    fn choice(&self) -> Result<UnknownCostChoice, String> {
        UnknownCostChoice::new(
            self.candidate.clone(),
            self.invocation.clone(),
            self.route.provider.clone(),
            self.route.model.clone(),
            self.route.endpoint.clone(),
            self.max_requests,
            self.max_output_tokens,
            self.request_timeout,
        )
        .and_then(|choice| choice.with_max_in_flight(self.max_in_flight))
        .map(|choice| {
            if self.authored_retry {
                choice.with_authored_retry()
            } else {
                choice
            }
        })
        .map_err(|e| e.to_string())
    }
    fn policy(&self) -> UnknownCostPolicy {
        self.evidence.policy(self.defaults[0], self.defaults[1])
    }
    /// Concise copy for an explicit one-time yes/no decision.
    #[must_use]
    pub fn question(&self) -> String {
        let defaults = self
            .defaults
            .map(|c| c.map_or_else(|| "none".into(), |v| v.to_string()));
        let seconds = self.request_timeout.as_secs();
        // A fan or an authored retry shows its breakdown and concurrency; a
        // sequential Run keeps its historical words byte for byte.
        let concurrency = (self.max_in_flight > 1 || !self.breakdown.is_empty()).then(|| {
            format!(
                "At most {} in flight at once; authored retries and schema re-asks consume this same request bound.",
                self.max_in_flight
            )
        });
        // A task retry the workflow authored is the runtime's, never a hidden
        // resend by the transport, which stays at zero.
        let retry = self.authored_retry.then(|| {
            "Task retries authored in the workflow (retry.max_attempts) may send a new request only after a completed response or a received 429 or 503; the transport never resends on its own, and any other failure stops every further request.".to_owned()
        });
        let mut multiplicity = String::new();
        for line in self
            .breakdown
            .iter()
            .cloned()
            .chain(concurrency)
            .chain(retry)
        {
            multiplicity.push('\n');
            multiplicity.push_str(&line);
        }
        let prior = self.prior_report.as_ref().map_or_else(String::new, |report| {
            format!("{}\nThis choice authorizes only the NEW invocation described below; no ceiling covers the earlier unknown charge. A stated budget is not this authorization.\n", report.1)
        });
        format!(
            "{prior}USD cost is unknown; a charge is possible on {}/{}.\nAt most {} requests; each at most {} output tokens and {} seconds (at most {} seconds of model wait). Any schema re-asks consume this same request bound. No automatic transport retry.{multiplicity}\nOverrides only the shown defaults (invocation: {}; project: {}); no hard cap is overridden.\nContinue once? yes / no",
            self.route.provider,
            self.route.model,
            self.max_requests,
            self.max_output_tokens,
            seconds,
            u64::from(self.max_requests) * seconds,
            defaults[0],
            defaults[1]
        )
    }
    /// Exact review identity and evidence for a details surface, not a credential.
    /// The route is named by its origin, never its path; the host evidence by its
    /// public view (each layer's class, origin and cap), never a debug dump.
    #[must_use]
    pub fn details(&self) -> String {
        let prior = self
            .prior_report
            .as_ref()
            .map_or_else(String::new, |report| {
                format!(" · prior report {}", report.0)
            });
        format!(
            "candidate {} · invocation {} · origin {} · host {}{prior}",
            self.candidate,
            self.invocation,
            self.route.origin(),
            self.evidence.view()
        )
    }
    /// Consume the review after the host receives explicit confirmation and
    /// independently re-observes the candidate and selected route. Not a Run grant.
    /// # Errors
    /// Candidate or route changed, or policy does not allow this choice.
    pub fn confirm(self, candidate: &str, route: &CostRoute) -> Result<InferenceAdmission, String> {
        if self
            .clock
            .now()
            .checked_duration_since(self.reviewed_at)
            .is_none_or(|elapsed| elapsed > Duration::from_secs(300))
        {
            return Err("cost review expired; review again before confirming".into());
        }
        if candidate != self.candidate || route != &self.route {
            return Err(
                "the candidate or selected route changed; review the new request before confirming"
                    .into(),
            );
        }
        InferenceAdmission::new_unknown(self.choice()?, self.policy())
            .and_then(|a| a.for_scope(candidate, &self.invocation))
            .map_err(|e| e.to_string())
    }
}

/// Convert a validated default downward to nanodollars. Never manufactures a price.
/// # Errors
/// Invalid/negative/non-finite defaults or overflow.
pub fn monetary_default(amount: Option<f64>) -> Result<Option<Cost>, String> {
    amount
        .map(|n| {
            if !n.is_finite() || !(0.0..=1_000_000_000.0).contains(&n) {
                return Err("invalid monetary default".into());
            }
            // This is a displayed overridable default, not a hard admission limit.
            // The finite bound above keeps this below 1e18; floor intentionally
            // discards sub-nanodollar fractions before the integer conversion.
            #[allow(clippy::cast_possible_truncation)]
            let nanos = (n * 1_000_000_000.0).floor() as i128;
            Ok(Cost::new(nanos))
        })
        .transpose()
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    #[derive(Debug)]
    struct TestClock(std::sync::Mutex<std::time::Instant>);
    impl nika_kernel::clock::ClockDyn for TestClock {
        fn now(&self) -> std::time::Instant {
            *self.0.lock().expect("test clock")
        }
        fn system_now(&self) -> std::time::SystemTime {
            std::time::SystemTime::UNIX_EPOCH
        }
        async fn sleep(&self, _: Duration) {}
    }
    #[test]
    fn the_injected_clock_expires_review_without_waiting_and_refuses_clock_regression() {
        let base = std::time::Instant::now();
        for (now, accepted) in [
            (base + Duration::from_secs(299), true),
            (base + Duration::from_secs(301), false),
            (base.checked_sub(Duration::from_secs(1)).unwrap(), false),
        ] {
            let clock = Arc::new(TestClock(std::sync::Mutex::new(base)));
            let review = CostReview::new_with_clock(
                "c".into(),
                "i".into(),
                route(),
                CostHostEvidence::unmanaged_interactive_local(),
                None,
                None,
                clock.clone(),
            )
            .expect("review");
            *clock.0.lock().expect("advance") = now;
            assert_eq!(review.confirm("c", &route()).is_ok(), accepted);
        }
    }
    fn route() -> CostRoute {
        CostRoute::observe("deepseek/deepseek-v4-pro", ProvidersConfig::new())
            .expect("native route")
    }
    /// An explicit source recovery reserves its requests in the same Session review: the bound
    /// shown before confirmation grows by exactly them, and none are reserved without it.
    #[test]
    fn a_recovery_reservation_extends_the_same_session_review() {
        let session = review().for_session();
        assert_eq!(session.max_requests(), SESSION_REVIEW_MAX_REQUESTS);
        let plain = session.question();
        let reserved = session.with_recovery_requests(3, 70);
        assert_eq!(reserved.max_requests(), SESSION_REVIEW_MAX_REQUESTS + 3);
        let asked = reserved.question();
        assert!(asked.contains("Allowance 7 + 3 reserved"), "{asked}");
        assert!(
            asked.contains("theoretical worst case is 70 requests"),
            "{asked}"
        );
        assert!(asked.contains("At most 10 requests"), "{asked}");
        let none = review().for_session().with_recovery_requests(0, 66);
        assert_eq!(none.max_requests(), SESSION_REVIEW_MAX_REQUESTS);
        assert_eq!(
            none.question(),
            plain,
            "no reservation: the question is unchanged"
        );
    }
    fn review() -> CostReview {
        CostReview::new(
            "candidate-a".into(),
            "invocation-a".into(),
            route(),
            CostHostEvidence::unmanaged_interactive_local(),
            Some(Cost::new(20_000_000)),
            Some(Cost::new(10_000_000)),
        )
        .expect("review")
    }
    #[test]
    fn unknown_host_and_every_hard_cap_refuse_without_authority() {
        assert!(
            CostReview::new(
                "c".into(),
                "i".into(),
                route(),
                CostHostEvidence::default(),
                None,
                None
            )
            .is_err()
        );
        for cap in [
            HardMonetaryCap::Unknown,
            HardMonetaryCap::Capped(Cost::zero()),
            HardMonetaryCap::Capped(Cost::new(1)),
        ] {
            for layer in 0..3 {
                let mut layers = std::array::from_fn(|_| CapEvidence::NotApplicable {
                    origin: "test composition".into(),
                });
                layers[layer] = CapEvidence::Observed {
                    cap,
                    origin: "test applicable config".into(),
                };
                let [p, m, o] = layers;
                assert!(
                    CostReview::new(
                        "c".into(),
                        "i".into(),
                        route(),
                        CostHostEvidence::new(true, p, m, o),
                        None,
                        None
                    )
                    .is_err()
                );
            }
        }
    }
    #[test]
    fn wrong_route_model_candidate_and_expired_review_refuse() {
        let mut changed = route();
        changed.endpoint.push_str("/different");
        assert!(review().confirm("candidate-a", &changed).is_err());
        changed = route();
        changed.model.push_str("-other");
        assert!(review().confirm("candidate-a", &changed).is_err());
        assert!(review().confirm("candidate-b", &route()).is_err());
        let mut expired = review();
        expired.reviewed_at = std::time::Instant::now()
            .checked_sub(Duration::from_secs(301))
            .unwrap();
        assert!(expired.confirm("candidate-a", &route()).is_err());
    }
    const SENTINEL: &str = "C6-SECRET-PATH-TOKEN";

    /// The origin is the URL parser's (`route_origin`): a canonical route keeps
    /// the exact origin it always showed, a credential is refused rather than
    /// stripped, and a backslash never carries a path into it.
    #[test]
    fn the_origin_keeps_scheme_host_and_effective_port_only() {
        let at = |endpoint: &str| CostRoute {
            provider: "deepseek".into(),
            model: "m".into(),
            endpoint: endpoint.into(),
        };
        for (endpoint, origin) in [
            (
                "https://api.deepseek.com/v1",
                "https://api.deepseek.com:443",
            ),
            (
                "https://127.0.0.1:18443/v1/C6-SECRET-PATH-TOKEN",
                "https://127.0.0.1:18443",
            ),
            (
                "https://user:C6-SECRET-PATH-TOKEN@host.example/v1",
                "unknown origin",
            ),
            (
                "https://host.example/v1?key=C6-SECRET-PATH-TOKEN#frag",
                "https://host.example:443",
            ),
            ("http://[::1]:8080/v1", "http://[::1]:8080"),
            ("https://[::1]/v1", "https://[::1]:443"),
            (
                "https://127.0.0.1:18443\\C6-SECRET-PATH-TOKEN\\v1",
                "https://127.0.0.1:18443",
            ),
            (
                "https://Host.Example/C6-SECRET-PATH-TOKEN",
                "https://host.example:443",
            ),
        ] {
            let seen = at(endpoint).origin();
            assert_eq!(seen, origin, "{endpoint}");
            assert!(!seen.contains(SENTINEL));
        }
    }

    /// Endpoints the URL parser would rewrite or that carry what a route must
    /// not: backslash, userinfo, query, fragment, case, an explicit default
    /// port, a raw Unicode host, a space and no path.
    const NONCANONICAL: [&str; 9] = [
        "https://127.0.0.1:18443\\C6-SECRET-PATH-TOKEN\\v1",
        "https://user:C6-SECRET-PATH-TOKEN@gateway.example/v1",
        "https://gateway.example/v1?key=C6-SECRET-PATH-TOKEN",
        "https://gateway.example/v1#C6-SECRET-PATH-TOKEN",
        "https://Gateway.Example/C6-SECRET-PATH-TOKEN/v1",
        "https://gateway.example:443/C6-SECRET-PATH-TOKEN/v1",
        "https://gätéway.example/C6-SECRET-PATH-TOKEN/v1",
        "https://gateway.example/C6 SECRET PATH TOKEN/v1",
        "https://gateway.example",
    ];

    #[test]
    fn a_noncanonical_endpoint_is_refused_before_any_review() {
        for endpoint in NONCANONICAL {
            let config = ProvidersConfig::new().with_base_url("deepseek", endpoint);
            let refused =
                CostRoute::observe("deepseek/deepseek-v4-pro", config.clone()).expect_err(endpoint);
            assert!(
                !refused.contains("SECRET") && !refused.contains("/v1"),
                "{endpoint}: {refused}"
            );
            assert!(
                unknown_cost_route("deepseek/deepseek-v4-pro", config).is_err(),
                "{endpoint}"
            );
            let route = CostRoute {
                provider: "deepseek".into(),
                model: "deepseek-v4-pro".into(),
                endpoint: endpoint.into(),
            };
            let review = CostReview::new(
                "c".into(),
                "i".into(),
                route,
                CostHostEvidence::unmanaged_interactive_local(),
                None,
                None,
            );
            assert!(review.is_err(), "{endpoint}: no review, so no request");
        }
        let canonical = ProvidersConfig::new()
            .with_base_url("deepseek", format!("https://gateway.example/{SENTINEL}/v1"));
        let route = CostRoute::observe("deepseek/deepseek-v4-pro", canonical).expect("canonical");
        assert_eq!(route.origin(), "https://gateway.example:443");
        assert!(
            route.endpoint.contains(SENTINEL),
            "the exact route stays in memory"
        );
    }

    #[test]
    fn review_and_challenge_screens_name_the_origin_never_the_path() {
        let config = ProvidersConfig::new()
            .with_base_url("deepseek", format!("https://gateway.example/{SENTINEL}/v1"));
        let route = CostRoute::observe("deepseek/deepseek-v4-pro", config).expect("route");
        let review = CostReview::new(
            "c".into(),
            "i".into(),
            route,
            CostHostEvidence::unmanaged_interactive_local(),
            None,
            None,
        )
        .expect("review");
        let details = review.details();
        assert!(
            details.contains("origin https://gateway.example:443 ·"),
            "{details}"
        );
        assert!(!details.contains(SENTINEL), "{details}");
        // The host evidence reads as its public view, never a Rust debug dump.
        assert!(details.contains(r#"host {"allowed":true,"#), "{details}");
        assert!(!details.contains("CostHostEvidence"), "{details}");
        let pending = PendingCostReview::new(review, "s".into(), "i".into());
        let challenge = pending.challenge();
        for screen in [challenge.display(), challenge.details()] {
            assert!(!screen.contains(SENTINEL), "{screen}");
        }
        assert!(
            challenge
                .display()
                .contains(" at https://gateway.example\n")
        );
        assert!(
            challenge
                .details()
                .contains("Origin: https://gateway.example:443\n")
        );
        let ipc = serde_json::to_string(challenge).expect("challenge");
        assert!(ipc.contains(SENTINEL), "the host IPC keeps the exact route");
    }
    #[test]
    fn the_evidence_view_names_each_layer() {
        let view = CostHostEvidence::new(
            true,
            CapEvidence::NotApplicable {
                origin: "test composition".into(),
            },
            CapEvidence::Observed {
                cap: HardMonetaryCap::Capped(Cost::new(2_000_000_000)),
                origin: "machine cap".into(),
            },
            CapEvidence::Unknown,
        )
        .view();
        assert_eq!(view["allowed"], true);
        assert_eq!(view["policy"]["class"], "not_applicable");
        assert_eq!(view["machine"]["class"], "observed");
        assert_eq!(
            view["machine"]["cap"]["capped_usd"],
            Cost::new(2_000_000_000).to_string()
        );
        assert_eq!(view["occurrence"], serde_json::json!({"class": "unknown"}));
    }
    #[test]
    fn native_price_text_never_carries_an_override_endpoint() {
        let config = ProvidersConfig::new()
            .with_base_url("deepseek", format!("https://127.0.0.1:18443/v1/{SENTINEL}"));
        let route = CostRoute::observe("deepseek/deepseek-v4-pro", config).expect("override route");
        assert!(
            route.endpoint.contains(SENTINEL),
            "the private witness keeps the exact route"
        );
        let review = CostReview::new(
            "c".into(),
            "i".into(),
            route,
            CostHostEvidence::unmanaged_interactive_local(),
            None,
            None,
        )
        .expect("review");
        let pending = PendingCostReview::new(review, "s".into(), "i".into());
        assert!(!pending.challenge().native_price.contains(SENTINEL));
        assert!(!pending.challenge().route.origin().contains(SENTINEL));
    }
    #[test]
    fn selected_adapter_is_not_substituted_for_unsupported_subscription() {
        assert!(CostRoute::observe("codex/gpt-5", ProvidersConfig::new()).is_err());
        assert!(CostRoute::observe("anthropic/claude-opus", ProvidersConfig::new()).is_err());
        let review = review().for_session();
        assert!(review.question().contains("7 requests"));
        assert!(review.question().contains("1260 seconds"));
    }
}
