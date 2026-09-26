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
    /// Unknown-cost transport currently supports HTTPS OpenAI-compatible text only.
    /// # Errors
    /// Unsupported adapter, malformed model or missing endpoint.
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
        Ok(Self {
            provider: provider.into(),
            model: profile.resolve_model(name).into(),
            endpoint: endpoint.into(),
        })
    }
    /// Whether numeric USD catalog admission cannot qualify this exact route.
    ///
    /// An exact vendored row of all-zero rates is a declared free seat
    /// (`openrouter/...:free` and the other zero rows in the pricing
    /// snapshot). It is not an unknown charge, so Run does not ask for a
    /// fresh human choice and a non-interactive host is not refused.
    /// A missing row, a positive rate, or a `contains` match stays unknown:
    /// silence is never metered as zero.
    #[must_use]
    pub fn needs_unknown_choice(&self) -> bool {
        if InferenceAdmission::qualify(&self.provider, &self.model, &self.endpoint).is_ok() {
            return false;
        }
        !nika_catalog::declares_exact_zero_price(&self.provider, &self.model)
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
    evidence: CostHostEvidence,
    defaults: [Option<Cost>; 2],
    max_requests: u32,
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
            evidence,
            defaults: [invocation_default, project_default],
            max_requests: 1,
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
        format!(
            "USD cost is unknown; a charge is possible on {}/{}.\nAt most {} requests; each at most {} output tokens and {} seconds (at most {} seconds of model wait). Any schema re-asks consume this same request bound. No automatic transport retry.\nOverrides only the shown defaults (invocation: {}; project: {}); no hard cap is overridden.\nContinue once? yes / no",
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
    #[must_use]
    pub fn details(&self) -> String {
        format!(
            "candidate {} · invocation {} · endpoint {} · host {:?}",
            self.candidate, self.invocation, self.route.endpoint, self.evidence
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
    #[test]
    fn selected_adapter_is_not_substituted_for_unsupported_subscription() {
        assert!(CostRoute::observe("codex/gpt-5", ProvidersConfig::new()).is_err());
        assert!(CostRoute::observe("anthropic/claude-opus", ProvidersConfig::new()).is_err());
        let review = review().for_session();
        assert!(review.question().contains("7 requests"));
        assert!(review.question().contains("1260 seconds"));
    }
}
