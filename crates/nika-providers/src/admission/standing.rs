// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The operator's standing cost policy: one authorization made once, in the
//! host's own composition, for Runs on an API route this binary's catalog does
//! not price. It is never a request field, a workflow key or a persisted
//! conversation: a host builds it from its startup flags or configuration and
//! hands it to the review inside its evidence
//! ([`super::CostHostEvidence::with_standing_policy`]).
//!
//! A rule names one model and carries bounds the admission account enforces
//! without a price: requests per Run, output tokens and seconds per request,
//! optionally the request's size, and Runs per window (the host counts those
//! in the project's standing ledger). A rule may also declare a USD price per
//! million tokens: the operator's estimate, never a provider's invoice. With
//! one, a Run's declared worst case is known before any request, so a USD
//! ceiling can admit it; without one no USD ceiling can, and every ceiling the
//! Run is under must be absent. A standing rule overrides no ceiling.

use super::{BOUNDED_REQUEST_BYTES, CostRoute, RUN_REVIEW_MAX_OUTPUT_TOKENS, RUN_REVIEW_TIMEOUT};
use crate::ProvidersConfig;
use sha2::Digest as _;
use std::fmt::Write as _;
use std::time::Duration;

/// The most requests one rule admits per Run. A Run's last journal row
/// records every request it sent: at this bound, the rows of a Run take under
/// half the journal's read bound (485 kB measured against 1 MiB, every request
/// answered under a declared price).
pub const STANDING_MAX_REQUESTS: u32 = 256;
/// The most Runs one rule admits per window.
const STANDING_MAX_RUNS: u32 = 100_000;
/// The longest window a rule counts its Runs over (31 days).
const STANDING_MAX_WINDOW: Duration = Duration::from_secs(31 * 24 * 60 * 60);
/// The shortest window a rule counts its Runs over.
const SHORTEST_WINDOW: Duration = Duration::from_secs(60);
/// The largest USD rate per million tokens a rule declares.
const LARGEST_RATE: f64 = 1_000_000.0;

/// The rule grammar, as every refusal teaches it.
const GRAMMAR: &str = "write PROVIDER/MODEL:requests=N,output-tokens=N,timeout=SECONDS,runs=N/PERIOD (PERIOD: minute, hour, day, week or <seconds>s), then optionally request-bytes=N and input-usd=USD,output-usd=USD (per million tokens)";

/// Where a declared price comes from, as every record of it says.
pub(super) const DECLARED_PROVENANCE: &str =
    "operator-declared in the host's standing cost policy; an estimate, never a provider invoice";

/// One rule of a standing cost policy: the model it names, the bounds the
/// engine enforces without a price and, optionally, a declared USD price.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StandingRule {
    model: String,
    pub(super) max_requests: u32,
    pub(super) max_output_tokens: u32,
    pub(super) timeout: Duration,
    max_runs: u32,
    window: Duration,
    pub(super) max_request_bytes: Option<u32>,
    /// USD per million input and output tokens, when the operator declares them.
    pub(super) price: Option<[f64; 2]>,
    id: String,
}

impl StandingRule {
    /// Read one rule as an operator writes it:
    /// `PROVIDER/MODEL:requests=N,output-tokens=N,timeout=SECONDS,runs=N/PERIOD`,
    /// then optionally `request-bytes=N` and `input-usd=USD,output-usd=USD` (per
    /// million tokens). The four bounds are required: a rule that bounds
    /// nothing is not one. Every bound only narrows what a Run review admits
    /// ([`RUN_REVIEW_MAX_OUTPUT_TOKENS`], [`RUN_REVIEW_TIMEOUT`]).
    /// # Errors
    /// A rule that does not read, an unknown or repeated setting, a bound out
    /// of range, a price half declared or with no positive rate.
    pub fn parse(text: &str) -> Result<Self, String> {
        let refused = |why: String| format!("standing cost rule {text:?}: {why}");
        let (model, settings) = text
            .rsplit_once(':')
            .ok_or_else(|| refused(GRAMMAR.to_owned()))?;
        let model = model.trim();
        let named = model
            .split_once('/')
            .is_some_and(|(provider, name)| !provider.is_empty() && !name.is_empty());
        if !named
            || model
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == ',')
        {
            return Err(refused(format!(
                "name the route as provider/model · {GRAMMAR}"
            )));
        }
        let mut read = Settings::default();
        for setting in settings.split(',') {
            read.take(setting.trim()).map_err(&refused)?;
        }
        read.rule(model).map_err(refused)
    }

    /// The rule's identity: a digest of its canonical form, so the same rule
    /// text names the same policy across restarts and in every record.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The Runs the rule admits per [`Self::window`].
    #[must_use]
    pub fn max_runs(&self) -> u32 {
        self.max_runs
    }

    /// The window the rule counts its Runs over.
    #[must_use]
    pub fn window(&self) -> Duration {
        self.window
    }

    /// Whether the operator declared a USD price: only then can a USD ceiling
    /// admit a Run of this rule.
    pub(super) fn declares_price(&self) -> bool {
        self.price.is_some()
    }

    /// Why a Run that may send `requests` requests, each asking at most
    /// `output_tokens` output tokens, is outside this rule; `None` inside it.
    pub(super) fn refusal(&self, requests: u32, output_tokens: u32) -> Option<String> {
        if requests > self.max_requests {
            return Some(format!(
                "this Run may send {requests} requests and the standing cost rule {} admits at most {} per Run on {}; nothing was sent",
                self.id, self.max_requests, self.model
            ));
        }
        (output_tokens > self.max_output_tokens).then(|| {
            format!(
                "this Run asks up to {output_tokens} output tokens per request and the standing cost rule {} admits at most {} on {}; nothing was sent",
                self.id, self.max_output_tokens, self.model
            )
        })
    }

    /// The rule as a public document shows it: its identity, the model, every
    /// bound and the price's state. Never an endpoint, a credential or a count
    /// of Runs already admitted.
    #[must_use]
    pub fn view(&self) -> serde_json::Value {
        let price = match self.price {
            Some([input, output]) => serde_json::json!({
                "state": "host_declared", "currency": "USD",
                "input_per_million": input, "output_per_million": output,
                "basis": DECLARED_PROVENANCE,
            }),
            None => serde_json::json!({"state": "unknown"}),
        };
        serde_json::json!({
            "policy": self.id,
            "model": self.model,
            "max_requests": self.max_requests,
            "max_output_tokens": self.max_output_tokens,
            "request_timeout_seconds": self.timeout.as_secs(),
            "max_request_bytes": self.request_bytes(),
            "max_runs": self.max_runs,
            "window_seconds": self.window.as_secs(),
            "price": price,
        })
    }

    /// The largest request body a Run of this rule sends, in bytes: the rule's
    /// own bound, or the bound of every admitted request.
    pub(super) fn request_bytes(&self) -> u32 {
        let fixed = u32::try_from(BOUNDED_REQUEST_BYTES).unwrap_or(u32::MAX);
        self.max_request_bytes.unwrap_or(fixed)
    }

    /// The canonical text the identity digests: every bound in one fixed
    /// order, the window in seconds.
    fn canonical(&self) -> String {
        let mut text = format!(
            "{}:requests={},output-tokens={},timeout={},runs={}/{}s",
            self.model,
            self.max_requests,
            self.max_output_tokens,
            self.timeout.as_secs(),
            self.max_runs,
            self.window.as_secs()
        );
        if let Some(bytes) = self.max_request_bytes {
            let _ = write!(text, ",request-bytes={bytes}");
        }
        if let Some([input, output]) = self.price {
            let _ = write!(text, ",input-usd={input},output-usd={output}");
        }
        text
    }
}

/// The settings of one rule as they are read, each at most once.
#[derive(Default)]
struct Settings {
    requests: Option<u32>,
    output_tokens: Option<u32>,
    timeout: Option<u64>,
    runs: Option<(u32, Duration)>,
    request_bytes: Option<u32>,
    input_usd: Option<f64>,
    output_usd: Option<f64>,
}

impl Settings {
    fn take(&mut self, setting: &str) -> Result<(), String> {
        let (key, value) = setting
            .split_once('=')
            .ok_or_else(|| format!("`{setting}` is not key=value · {GRAMMAR}"))?;
        let (key, value) = (key.trim(), value.trim());
        let taken = match key {
            "requests" => fill(
                &mut self.requests,
                count(value, u64::from(STANDING_MAX_REQUESTS)),
            ),
            "output-tokens" => fill(
                &mut self.output_tokens,
                count(value, u64::from(RUN_REVIEW_MAX_OUTPUT_TOKENS)),
            ),
            "timeout" => fill(
                &mut self.timeout,
                count(value, RUN_REVIEW_TIMEOUT.as_secs()).map(u64::from),
            ),
            "runs" => fill(&mut self.runs, runs(value)),
            "request-bytes" => fill(
                &mut self.request_bytes,
                count(
                    value,
                    u64::try_from(BOUNDED_REQUEST_BYTES).unwrap_or(u64::MAX),
                ),
            ),
            "input-usd" => fill(&mut self.input_usd, rate(value)),
            "output-usd" => fill(&mut self.output_usd, rate(value)),
            _ => return Err(format!("`{key}` is not a setting · {GRAMMAR}")),
        };
        taken.map_err(|why| format!("`{key}` {why}"))
    }

    fn rule(self, model: &str) -> Result<StandingRule, String> {
        let missing = |name: &str| format!("`{name}` is required · {GRAMMAR}");
        let price = match (self.input_usd, self.output_usd) {
            (None, None) => None,
            (Some(input), Some(output)) if input > 0.0 || output > 0.0 => Some([input, output]),
            (Some(_), Some(_)) => {
                return Err(
                    "a declared price states a positive rate: a rule never declares a route free"
                        .to_owned(),
                );
            }
            _ => return Err("declare `input-usd` and `output-usd` together".to_owned()),
        };
        let (max_runs, window) = self.runs.ok_or_else(|| missing("runs"))?;
        let mut rule = StandingRule {
            model: model.to_owned(),
            max_requests: self.requests.ok_or_else(|| missing("requests"))?,
            max_output_tokens: self.output_tokens.ok_or_else(|| missing("output-tokens"))?,
            timeout: Duration::from_secs(self.timeout.ok_or_else(|| missing("timeout"))?),
            max_runs,
            window,
            max_request_bytes: self.request_bytes,
            price,
            id: String::new(),
        };
        let digest =
            sha2::Sha256::digest(format!("nika/standing-cost-policy@1\n{}", rule.canonical()));
        rule.id = digest.iter().take(8).fold("scp-".to_owned(), |mut id, b| {
            let _ = write!(id, "{b:02x}");
            id
        });
        Ok(rule)
    }
}

/// Keep a setting's first value; a second one refuses.
fn fill<T>(slot: &mut Option<T>, value: Result<T, String>) -> Result<(), String> {
    if slot.is_some() {
        return Err("is set twice".to_owned());
    }
    *slot = Some(value?);
    Ok(())
}

/// A whole number from 1 to `most`.
fn count(value: &str, most: u64) -> Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|n| *n >= 1 && u64::from(*n) <= most)
        .ok_or_else(|| format!("takes a whole number from 1 to {most}"))
}

/// `N/PERIOD`: the Runs admitted per window.
fn runs(value: &str) -> Result<(u32, Duration), String> {
    let teaching = || {
        format!(
            "takes N/PERIOD with N from 1 to {STANDING_MAX_RUNS} and PERIOD minute, hour, day, week or <seconds>s ({} to {} seconds)",
            SHORTEST_WINDOW.as_secs(),
            STANDING_MAX_WINDOW.as_secs()
        )
    };
    let (n, period) = value.split_once('/').ok_or_else(teaching)?;
    let n = count(n.trim(), u64::from(STANDING_MAX_RUNS)).map_err(|_| teaching())?;
    let seconds = match period.trim() {
        "minute" => Some(60),
        "hour" => Some(3_600),
        "day" => Some(86_400),
        "week" => Some(604_800),
        other => other.strip_suffix('s').and_then(|s| s.parse::<u64>().ok()),
    };
    let window = seconds
        .map(Duration::from_secs)
        .filter(|w| (SHORTEST_WINDOW..=STANDING_MAX_WINDOW).contains(w))
        .ok_or_else(teaching)?;
    Ok((n, window))
}

/// A finite USD rate per million tokens, from 0 to [`LARGEST_RATE`].
fn rate(value: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .ok()
        .filter(|usd| usd.is_finite() && (0.0..=LARGEST_RATE).contains(usd))
        .ok_or_else(|| format!("takes a USD amount per million tokens from 0 to {LARGEST_RATE}"))
}

/// The rules a host composed, each with the exact route it names on this host.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StandingCostPolicy {
    rules: Vec<(StandingRule, CostRoute)>,
}

impl StandingCostPolicy {
    /// Read `rules` and observe the route each one names under `config`, the
    /// configuration its Runs dispatch with. A rule is refused, never ignored:
    /// a host that starts holds exactly the policy its operator wrote.
    /// # Errors
    /// No rule; a rule that does not read ([`StandingRule::parse`]); a model
    /// unknown-cost admission cannot serve ([`CostRoute::observe`]); a route
    /// the catalog prices (its Runs already run under ordinary USD ceilings);
    /// two rules for one route.
    pub fn resolve(rules: &[String], config: &ProvidersConfig) -> Result<Self, String> {
        if rules.is_empty() {
            return Err("a standing cost policy names at least one rule".to_owned());
        }
        let mut resolved: Vec<(StandingRule, CostRoute)> = Vec::with_capacity(rules.len());
        for text in rules {
            let rule = StandingRule::parse(text)?;
            let refused = |why: &str| format!("standing cost rule {text:?}: {why}");
            let route = CostRoute::observe(&rule.model, config.clone()).map_err(|e| refused(&e))?;
            if !route.needs_unknown_choice() {
                return Err(refused(
                    "the catalog prices this route: its Runs already run under ordinary USD ceilings and need no standing rule",
                ));
            }
            if resolved.iter().any(|(_, named)| *named == route) {
                return Err(refused("another rule already names this route"));
            }
            resolved.push((rule, route));
        }
        Ok(Self { rules: resolved })
    }

    /// The rule that names exactly `route` (its provider, wire model and
    /// endpoint), if one does.
    #[must_use]
    pub fn rule_for(&self, route: &CostRoute) -> Option<&StandingRule> {
        self.rules
            .iter()
            .find(|(_, named)| named == route)
            .map(|(rule, _)| rule)
    }

    /// The policy as a public document shows it: every rule's
    /// [`StandingRule::view`], in the order the operator wrote them.
    #[must_use]
    pub fn view(&self) -> serde_json::Value {
        let rules: Vec<serde_json::Value> =
            self.rules.iter().map(|(rule, _)| rule.view()).collect();
        serde_json::json!({"schema": "nika/standing-cost-policy@1", "rules": rules})
    }
}

#[cfg(test)]
mod tests;
