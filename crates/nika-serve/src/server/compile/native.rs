// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The operator's native authoring seat for `POST /v1/compile` generation 2.
//!
//! Off unless the operator builds the server with [`ServerConfig::with_native_authoring`]
//! (`nika serve --authoring-model`): a default server speaks generation 1 alone, byte for byte.
//! The seat is ONE direct provider model — never a harness, never a decision seat — with its
//! bounds (output tokens and seconds per call, repair rounds, one request's deadline) and an
//! optional Foundry knowledge snapshot, opened, verified and pinned when the listener attaches
//! through the configuration parser and knowledge reader every door shares
//! (`nika_cli_host::compile::{config, knowledge}`). The strategy is fixed: the seat writes the
//! candidate itself (`only`), one sample, at the reasoning effort the operator names. A round
//! permits one physical request by default; the operator must explicitly grant `max_calls` for
//! more. Repair preferences are not grants. Redirects are disabled; provider resends consume
//! that grant. A caller opts in per request and may narrow each bound, never widen one; it names
//! no model, endpoint, credential, path, strategy or effort. The key the seat's provider
//! resolves is withheld from every answer, however the operator supplied it.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nika_cli_host::compile::{config, knowledge};
use nika_error::prelude::{NikaCode, NikaErrorCode, codes};
use nika_kernel::secret::Secret;
use nika_onboard::compile::NativeMode;
use nika_providers::{ProviderRegistry, ProvidersConfig};

use super::super::config::ServerConfig;
use super::replay::Replays;
use super::v2::Bounds;

const DEFAULT_DEADLINE: Duration = Duration::from_secs(300);
const DEFAULT_REPLAY_ENTRIES: usize = 32;
const DEFAULT_REPLAY_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_DEADLINE: Duration = Duration::from_secs(3600);
const MAX_REPLAY_ENTRIES: usize = 1024;
const MAX_REPLAY_TTL: Duration = Duration::from_secs(24 * 3600);

/// The operator's explicit native authoring seat. Nothing here is a caller's choice.
#[derive(Clone)]
#[non_exhaustive]
pub struct NativeAuthoring {
    model: String,
    providers: ProvidersConfig,
    bounds: Bounds,
    named: config::AuthoringSettings,
    replay_entries: usize,
    replay_ttl: Duration,
    withheld: Vec<Secret>,
}

impl NativeAuthoring {
    /// Seat `model` (`provider/name`, a direct provider) with the provider configuration
    /// (keys, endpoints) the composition root resolved. Defaults: 8192 output tokens and
    /// 120 s per call, 3 desired repair rounds within ONE authorized request, a 300 s deadline,
    /// no knowledge, 32 kept rounds for 30 minutes. Use `with_max_calls` to grant more requests.
    /// Validated when the listener attaches, before it binds.
    #[must_use]
    pub fn new(model: impl Into<String>, providers: ProvidersConfig) -> Self {
        Self {
            model: model.into(),
            providers,
            bounds: Bounds {
                max_tokens: config::DEFAULT_MAX_TOKENS,
                call_timeout: config::DEFAULT_CALL_TIMEOUT,
                deadline: DEFAULT_DEADLINE,
                repairs: config::DEFAULT_REPAIRS,
                repairs_explicit: false,
                max_calls: None,
                grant: "operator: NativeAuthoring::with_max_calls",
            },
            named: config::AuthoringSettings::none().with_strategy(NativeMode::Only.word()),
            replay_entries: DEFAULT_REPLAY_ENTRIES,
            replay_ttl: DEFAULT_REPLAY_TTL,
            withheld: Vec::new(),
        }
    }

    /// Output tokens per call (1..=32768).
    #[must_use]
    pub const fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.bounds.max_tokens = tokens;
        self
    }

    /// The wait for one model invocation (up to 600 s); resends consume the request grant.
    #[must_use]
    pub const fn with_call_timeout(mut self, timeout: Duration) -> Self {
        self.bounds.call_timeout = timeout;
        self
    }

    /// One request's whole deadline (up to 3600 s): its work stops there.
    #[must_use]
    pub const fn with_deadline(mut self, deadline: Duration) -> Self {
        self.bounds.deadline = deadline;
        self
    }

    /// Desired repair rounds (0..=5), which require an explicit sufficient `max_calls` grant.
    #[must_use]
    pub const fn with_repairs(mut self, repairs: u32) -> Self {
        self.bounds.repairs = repairs;
        self.bounds.repairs_explicit = true;
        self
    }

    /// Authorize at most this many model invocations and physical HTTP requests per round.
    /// Absent this grant the ceiling is one; zero refuses before the listener binds.
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

    /// How many answer rounds are kept (1..=1024) and for how long (up to 24 h).
    #[must_use]
    pub const fn with_replay(mut self, entries: usize, ttl: Duration) -> Self {
        self.replay_entries = entries;
        self.replay_ttl = ttl;
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
/// The existing flag structure remains source-compatible; absent a grant, one request
/// is authorized. A grant without an authoring model is refused.
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
    Ok(Some(config.with_native_authoring(seat)))
}

/// A validated seat: what one bound server authors with.
pub(in crate::server) struct Seat {
    pub(super) model: String,
    /// The canonical provider id, for the receipt's backend.
    pub(super) provider: String,
    pub(super) providers: ProvidersConfig,
    pub(super) bounds: Bounds,
    /// The shared configuration: strategy `only`, the snapshot, the reasoning effort.
    pub(super) authoring: config::AuthoringConfig,
    knowledge: Option<knowledge::pin::KnowledgePin>,
    /// The seat's resolved key and the operator's further values: never answered.
    withheld: Vec<Secret>,
    pub(in crate::server) replays: Arc<Replays>,
    /// Raised once when the server stops: every round of this seat stops with it.
    halt: tokio::sync::watch::Sender<bool>,
}

/// The pinned snapshot no longer reads as pinned (changed, stale or gone).
pub(super) struct ContextChanged;

impl Seat {
    /// Validate a seat: bounds, a direct provider model that resolves with its key, the shared
    /// configuration (strategy `only`, a snapshot, the effort word named; only that word can
    /// refuse there), no environment read. The provider's key joins the withheld values.
    pub(in crate::server) fn open(config: &NativeAuthoring) -> Result<Self, NativeAuthoringError> {
        let bounds = bounds(config)?;
        let (provider, key) = direct_provider(&config.model, &config.providers)?;
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
        self.withheld
            .iter()
            .map(Secret::expose)
            .filter(|secret| !secret.is_empty())
            .any(|secret| {
                contains(document, secret.as_bytes())
                    || escaped(secret).is_some_and(|escaped| contains(document, escaped.as_bytes()))
            })
    }
}

/// A value as the body of a JSON string spells it (the enclosing quotes removed, exactly one
/// each side).
fn escaped(value: &str) -> Option<String> {
    let quoted = serde_json::to_string(value).ok()?;
    Some(quoted.strip_prefix('"')?.strip_suffix('"')?.to_owned())
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn bounds(config: &NativeAuthoring) -> Result<Bounds, NativeAuthoringError> {
    let bounds = config.bounds;
    let refuse = |why| Err(NativeAuthoringError::Bound(why));
    config::check_call_bounds(bounds.max_tokens, bounds.call_timeout)
        .map_err(NativeAuthoringError::Bound)?;
    if bounds.deadline.is_zero() || bounds.deadline > MAX_DEADLINE {
        return refuse("the authoring deadline per request must be above zero and at most 3600 s");
    }
    if bounds.repairs > config::MAX_REPAIRS {
        return refuse("authoring repair rounds must be 0..=5");
    }
    if !(1..=MAX_REPLAY_ENTRIES).contains(&config.replay_entries)
        || config.replay_ttl.is_zero()
        || config.replay_ttl > MAX_REPLAY_TTL
    {
        return refuse("kept answer rounds must be 1..=1024 for at most 24 h");
    }
    if bounds.authority().is_err() {
        return refuse(
            "authoring max_calls must be positive and honor explicitly configured repairs",
        );
    }
    Ok(bounds)
}

/// A direct provider model that resolves now (known provider, its key present): its canonical
/// id and the key it resolved (`None` when keyless). A harness seat or any other name refuses,
/// never falls back.
fn direct_provider(
    model: &str,
    providers: &ProvidersConfig,
) -> Result<(String, Option<Secret>), NativeAuthoringError> {
    let refuse = |reason: String| NativeAuthoringError::Model {
        model: model.to_owned(),
        reason,
    };
    let Some((id, _)) = model.split_once('/') else {
        return Err(refuse("name it `provider/name`".to_owned()));
    };
    if nika_types::access::HarnessRuntime::lookup(id).is_some() {
        return Err(refuse(
            "a harness seat cannot author on the server; seat a direct provider model".to_owned(),
        ));
    }
    let http = nika_runtime::compose::provider_http().map_err(|error| refuse(error.to_string()))?;
    let resolved = ProviderRegistry::new(Arc::new(http), providers.clone())
        .resolve(model)
        .map_err(|error| refuse(error.to_string()))?;
    Ok((
        nika_providers::profile::canonical_provider(id).to_owned(),
        resolved.key().cloned(),
    ))
}
