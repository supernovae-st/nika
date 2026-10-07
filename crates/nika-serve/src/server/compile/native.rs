// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The operator's native authoring seat for `POST /v1/compile` generation 2.
//!
//! Off unless the operator builds the server with [`ServerConfig::with_native_authoring`]
//! (`nika serve --authoring-model`): a default server speaks generation 1 alone, byte for byte.
//! The seat is ONE direct provider model — never a harness — with its route's completion
//! capacity and any explicit operator limits, an optional decision model that judges in place
//! of the author (`--decision-model`, the `nika compile` words), plus an
//! optional Foundry knowledge snapshot, opened, verified and pinned when the listener attaches
//! through the configuration parser and knowledge reader every door shares
//! (`nika_cli_host::compile::{config, knowledge}`). The strategy is fixed: the shared default
//! `escalate` (the compiler writes the source), one sample, at the operator's effort. A round
//! has no implicit request, repair or whole-round limit. Redirects are disabled; requests are
//! counted and any explicit `max_calls` is enforced. A caller opts in per request and may
//! narrow each bound, never widen one; it names
//! no model, endpoint, credential, path, strategy or effort. The key the seat's provider
//! resolves is withheld from every answer, however the operator supplied it.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nika_cli_host::compile::{config, knowledge, typesafe};
use nika_error::prelude::{NikaCode, NikaErrorCode, codes};
use nika_kernel::secret::Secret;
use nika_onboard::compile::room::JqHelper;
use nika_onboard::remote_door::decision::{DecisionModel, direct_provider};
use nika_providers::ProvidersConfig;

use super::super::config::ServerConfig;
use super::replay::Replays;
use super::v2::Bounds;

/// The operator's explicit native authoring seat. Nothing here is a caller's choice.
#[derive(Clone)]
#[non_exhaustive]
pub struct NativeAuthoring {
    model: String,
    providers: ProvidersConfig,
    bounds: Bounds,
    named: config::AuthoringSettings,
    replay_entries: Option<usize>,
    replay_ttl: Option<Duration>,
    withheld: Vec<Secret>,
    decision: Option<String>,
    trials: Option<JqHelper>,
}

impl NativeAuthoring {
    /// Seat `model` (`provider/name`, a direct provider) with the provider configuration
    /// (keys, endpoints) the composition root resolved. Completion limits follow that route;
    /// request count, repair count and whole-round duration have no implicit ceiling.
    /// Replay storage has no implicit count, size or expiry limit. It is held in memory by
    /// this server run and is forgotten when the process stops.
    /// Validated when the listener attaches, before it binds.
    #[must_use]
    pub fn new(model: impl Into<String>, providers: ProvidersConfig) -> Self {
        let model = model.into();
        let route =
            nika_providers::authoring::policy::completion_bounds(&model, false, providers.clone());
        Self {
            model,
            providers,
            bounds: Bounds {
                max_tokens: route.max_tokens,
                initial_tokens: route.initial_tokens,
                call_timeout: route.timeout,
                deadline: None,
                repairs: None,
                max_calls: None,
                grant: "operator: NativeAuthoring::with_max_calls",
            },
            named: config::AuthoringSettings::none(),
            replay_entries: None,
            replay_ttl: None,
            withheld: Vec::new(),
            decision: None,
            trials: None,
        }
    }

    /// Try each final candidate on the caller's `trial_inputs` in the shared observed room, `jq`
    /// evaluating its steps; without it, `trial_inputs` is refused.
    #[must_use]
    pub fn with_trials(mut self, jq: JqHelper) -> Self {
        self.trials = Some(jq);
        self
    }

    /// Seat a decision model that judges in place of the author; it opens (its key withheld)
    /// when the listener attaches, or refuses the server.
    #[must_use]
    pub fn with_decision_model(mut self, model: impl Into<String>) -> Self {
        self.decision = Some(model.into());
        self
    }

    /// Explicit positive output-token limit per call.
    #[must_use]
    pub const fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.bounds.max_tokens = tokens;
        self
    }

    /// An explicit positive wait for one model invocation.
    #[must_use]
    pub const fn with_call_timeout(mut self, timeout: Duration) -> Self {
        self.bounds.call_timeout = timeout;
        self
    }

    /// An explicit positive whole-round deadline: its work stops there.
    #[must_use]
    pub const fn with_deadline(mut self, deadline: Duration) -> Self {
        self.bounds.deadline = Some(deadline);
        self
    }

    /// An explicit repair-round limit (zero disables repairs).
    #[must_use]
    pub const fn with_repairs(mut self, repairs: u32) -> Self {
        self.bounds.repairs = Some(repairs);
        self
    }

    /// Authorize at most this many model invocations and physical HTTP requests per round.
    /// Absent this limit no count is imposed; zero refuses before the listener binds.
    #[must_use]
    pub const fn with_max_calls(mut self, max_calls: u32) -> Self {
        self.bounds.max_calls = Some(max_calls);
        self
    }

    /// A Foundry knowledge snapshot directory and a corpus whose examples are never recalled.
    /// Without a trusted identity the door refuses it when the listener attaches.
    #[must_use]
    pub fn with_knowledge(
        mut self,
        dir: impl Into<PathBuf>,
        exclude_corpus: Option<String>,
    ) -> Self {
        self.named = self.named.with_knowledge(dir, exclude_corpus);
        self
    }

    /// A release root with the identity the operator's host trusts for it, from its own
    /// release record: admitted and pinned when the listener attaches.
    #[must_use]
    pub fn with_knowledge_release(
        mut self,
        dir: impl Into<PathBuf>,
        identity: knowledge::TrustedIdentity,
    ) -> Self {
        self.named = self.named.with_knowledge_release(dir, identity);
        self
    }

    /// The knowledge turned off on the operator's layer (`--no-knowledge`): nothing is pinned,
    /// and the shared parser refuses it beside a named snapshot.
    #[must_use]
    pub fn without_knowledge(mut self) -> Self {
        self.named = self.named.with_knowledge_off();
        self
    }

    /// The reasoning effort word every seat call asks (low · high · max).
    #[must_use]
    pub fn with_reasoning(mut self, word: impl Into<String>) -> Self {
        self.named = self.named.with_reasoning(word);
        self
    }

    /// Explicitly limit the kept answer rounds and their lifetime. Both values must be
    /// positive, and the lifetime must be representable by the monotonic clock. Without
    /// this option, count and lifetime have no implicit limit within this server run.
    #[must_use]
    pub const fn with_replay(mut self, entries: usize, ttl: Duration) -> Self {
        self.replay_entries = Some(entries);
        self.replay_ttl = Some(ttl);
        self
    }

    /// A further value no compile answer may carry, beside the key the seat's provider resolves
    /// (always withheld): a document that holds one, raw or JSON-escaped, is refused whole,
    /// never rewritten. Every nonempty value counts, however short.
    #[must_use]
    pub fn with_withheld(mut self, secret: Secret) -> Self {
        self.withheld.push(secret);
        self
    }
}

impl fmt::Debug for NativeAuthoring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeAuthoring")
            .field("model", &self.model)
            .field("bounds", &self.bounds)
            .field("knowledge", &self.named.knowledge.is_some())
            .field("decision", &self.decision)
            .finish_non_exhaustive()
    }
}

/// Why a native authoring seat cannot be honored. Refused before the listener binds.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum NativeAuthoringError {
    /// A seat was named without the HTTP listener it serves.
    #[error(
        "native authoring seats the HTTP compile door: pass --bind, --workflows and --token-file"
    )]
    NeedsListener,
    /// The model is not a direct provider model this server can call.
    #[error("the authoring model `{model}` cannot be seated: {reason}")]
    Model {
        /// The model, as named.
        model: String,
        /// Why.
        reason: String,
    },
    /// A bound is outside its range.
    #[error("{0}")]
    Bound(&'static str),
    /// The knowledge snapshot cannot be pinned.
    #[error("the knowledge snapshot cannot be pinned: {0}")]
    Knowledge(String),
}

/// An operator's configuration refusal, as every `nika serve` launch refusal is.
impl NikaErrorCode for NativeAuthoringError {
    fn nika_code(&self) -> NikaCode {
        codes::NIKA_001
    }
}

pub use nika_cli_host::compile::NativeAuthoringArgs;

/// Attach the seat `nika serve --authoring-model` names to the listener configuration. Absent
/// the flag, the configuration is returned untouched and nothing is read. Named, the provider
/// configuration (`config_from_env`, its own key precedence) and the effort word (the flag,
/// else `NIKA_AUTHORING_REASONING`) are read once, now; the seat — and the key it resolves,
/// withheld from every answer — is validated when the listener attaches.
///
/// # Errors
/// [`NativeAuthoringError::NeedsListener`] when no listener is configured.
pub fn seat_native_authoring(
    http: Option<ServerConfig>,
    flags: &NativeAuthoringArgs,
) -> Result<Option<ServerConfig>, NativeAuthoringError> {
    seat_native_authoring_with_calls(http, flags, None)
}

/// Attach the operator's seat with an optional explicit physical-request ceiling.
/// The existing flag structure remains source-compatible; absent a limit no request count
/// is imposed. A limit without an authoring model is refused.
///
/// # Errors
/// Returns the same seating errors as [`seat_native_authoring`], or a bound error
/// when a request grant has no model to authorize.
pub fn seat_native_authoring_with_calls(
    http: Option<ServerConfig>,
    flags: &NativeAuthoringArgs,
    max_calls: Option<u32>,
) -> Result<Option<ServerConfig>, NativeAuthoringError> {
    let Some(model) = flags.model.as_deref() else {
        if max_calls.is_some() {
            return Err(NativeAuthoringError::Bound(
                "authoring max_calls requires a model",
            ));
        }
        return Ok(http);
    };
    let Some(config) = http else {
        return Err(NativeAuthoringError::NeedsListener);
    };
    let mut seat = NativeAuthoring::new(model, nika_runtime::compose::config_from_env());
    if let Some(calls) = max_calls {
        seat = seat.with_max_calls(calls);
    }
    if let Some(tokens) = flags.max_tokens {
        seat = seat.with_max_tokens(tokens);
    }
    if let Some(seconds) = flags.timeout {
        seat = seat.with_call_timeout(Duration::from_secs(seconds));
    }
    if let Some(seconds) = flags.deadline {
        seat = seat.with_deadline(Duration::from_secs(seconds));
    }
    if let Some(repairs) = flags.repairs {
        seat = seat.with_repairs(repairs);
    }
    if let Some(dir) = &flags.knowledge {
        seat = seat.with_knowledge(dir, flags.knowledge_exclude.clone());
    }
    if flags.no_knowledge {
        seat = seat.without_knowledge();
    }
    if let Some(word) = config::reasoning_word(flags.reasoning.as_deref()) {
        seat = seat.with_reasoning(word);
    }
    if let Some(model) = &flags.decision_model {
        seat = seat.with_decision_model(model.as_str());
    }
    if let Ok(exe) = std::env::current_exe() {
        seat = seat.with_trials(JqHelper::new(exe));
    }
    Ok(Some(config.with_native_authoring(seat)))
}

/// A validated seat: what one bound server authors with.
pub(in crate::server) struct Seat {
    pub(super) model: String,
    /// The canonical provider id, for the receipt's backend.
    pub(super) provider: String,
    pub(super) providers: ProvidersConfig,
    pub(super) bounds: Bounds,
    /// The shared configuration: the default strategy, the snapshot, the reasoning effort.
    pub(super) authoring: config::AuthoringConfig,
    knowledge: Option<knowledge::pin::KnowledgePin>,
    /// The seat's resolved key and the operator's further values: never answered.
    withheld: Vec<Secret>,
    pub(in crate::server) replays: Arc<Replays>,
    /// Raised once when the server stops: every round of this seat stops with it.
    halt: tokio::sync::watch::Sender<bool>,
    pub(in crate::server) decision: Option<DecisionModel>,
    pub(in crate::server) trials: Option<JqHelper>,
}

/// The pinned snapshot no longer reads as pinned (changed, stale or gone).
pub(super) struct ContextChanged;

impl Seat {
    /// Validate a seat: bounds, a direct provider model that resolves with its key, the shared
    /// configuration (the default strategy, a snapshot, the effort word named; only that word can
    /// refuse there), no environment read. The provider's key joins the withheld values.
    pub(in crate::server) fn open(config: &NativeAuthoring) -> Result<Self, NativeAuthoringError> {
        let bounds = bounds(config)?;
        let (provider, key) =
            direct_provider(&config.model, &config.providers).map_err(|reason| {
                NativeAuthoringError::Model {
                    model: config.model.clone(),
                    reason,
                }
            })?;
        let none = config::AuthoringSettings::none();
        let authoring =
            config::resolve(&config.named, &none).map_err(|error| NativeAuthoringError::Model {
                model: config.model.clone(),
                reason: error.to_string(),
            })?;
        let knowledge = knowledge::pin::KnowledgePin::of_config(&authoring)
            .map_err(|error| NativeAuthoringError::Knowledge(error.to_string()))?;
        let mut withheld = config.withheld.clone();
        withheld.extend(key);
        let opened = (config.decision.as_deref())
            .map(|model| DecisionModel::open(model, &config.providers, typesafe::seat))
            .transpose()
            .map_err(|reason| NativeAuthoringError::Model {
                model: config.decision.clone().unwrap_or_default(),
                reason,
            })?;
        let decision = opened.map(|(seat, key)| {
            withheld.extend(key);
            seat
        });
        Ok(Self {
            model: config.model.clone(),
            provider,
            providers: config.providers.clone(),
            bounds,
            authoring,
            knowledge,
            withheld,
            replays: Replays::new(config.replay_entries, config.replay_ttl),
            halt: tokio::sync::watch::channel(false).0,
            decision,
            trials: config.trials.clone(),
        })
    }

    /// Stop every round of this seat, now and for good (the server is stopping).
    pub(super) fn halt(&self) {
        self.halt.send_replace(true);
    }

    /// The stop signal a round watches.
    pub(super) fn halted(&self) -> tokio::sync::watch::Receiver<bool> {
        self.halt.subscribe()
    }

    /// The pinned snapshot, opened again against its trusted identity: `None` without one;
    /// refused unless it still reads as at attach. Every generation-2 round checks it first.
    pub(super) fn context(
        &self,
    ) -> Result<Option<(knowledge::Snapshot, Option<&str>)>, ContextChanged> {
        let Some(pin) = &self.knowledge else {
            return Ok(None);
        };
        let snapshot = pin.reopen().map_err(|_| ContextChanged)?;
        if pin.moved(&snapshot).is_some() {
            return Err(ContextChanged);
        }
        Ok(Some((snapshot, pin.exclude_corpus.as_deref())))
    }

    /// Whether a document carries a withheld value, raw or as a JSON string carries it
    /// (escaped): every nonempty value counts, however short.
    pub(super) fn discloses(&self, document: &[u8]) -> bool {
        nika_onboard::remote_door::decision::discloses(&self.withheld, document)
    }
}

fn bounds(config: &NativeAuthoring) -> Result<Bounds, NativeAuthoringError> {
    let bounds = config.bounds;
    let refuse = |why| Err(NativeAuthoringError::Bound(why));
    config::check_call_bounds(bounds.max_tokens, bounds.call_timeout)
        .map_err(NativeAuthoringError::Bound)?;
    if bounds.deadline.is_some_and(|deadline| {
        deadline.is_zero() || std::time::Instant::now().checked_add(deadline).is_none()
    }) {
        return refuse("the authoring deadline must be positive and representable by the clock");
    }
    if config.replay_entries == Some(0) {
        return refuse("an explicit kept-round count must be positive");
    }
    if config
        .replay_ttl
        .is_some_and(|ttl| ttl.is_zero() || std::time::Instant::now().checked_add(ttl).is_none())
    {
        return refuse(
            "an explicit kept-round lifetime must be positive and representable by the clock",
        );
    }
    if bounds.authority().is_err() {
        return refuse("authoring max_calls must be positive");
    }
    Ok(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_effective_route_without_implicit_round_limits() {
        let model = "deepseek/deepseek-v4-pro";
        for providers in [
            ProvidersConfig::new(),
            ProvidersConfig::new()
                .with_base_url("deepseek", "https://gateway.invalid/v1/chat/completions"),
        ] {
            let route = nika_providers::authoring::policy::completion_bounds(
                model,
                false,
                providers.clone(),
            );
            let seat = NativeAuthoring::new(model, providers);
            assert_eq!(seat.bounds.max_tokens, route.max_tokens);
            assert_eq!(seat.bounds.initial_tokens, route.initial_tokens);
            assert_eq!(seat.bounds.call_timeout, route.timeout);
            assert_eq!(seat.bounds.deadline, None);
            assert_eq!(seat.bounds.repairs, None);
            assert_eq!(seat.bounds.max_calls, None);
            assert!(bounds(&seat).is_ok());
        }
        let direct = NativeAuthoring::new(model, ProvidersConfig::new());
        assert_eq!(direct.bounds.max_tokens, 393_216);
        assert_eq!(direct.bounds.initial_tokens, 131_072);
    }
}
