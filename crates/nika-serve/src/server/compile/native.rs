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
//! candidate itself (`only`), one sample, so a request makes at most `1 + repairs` logical calls
//! (the provider transport may resend one after a 429, 503 or 529, inside that call's wait). A
//! caller opts in per request and may narrow each bound, never widen one; it names no model,
//! endpoint, credential, path or strategy. The key the seat's provider resolves is withheld from
//! every answer, however the operator supplied it.

use std::fmt;
use std::path::{Path, PathBuf};
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

const DEFAULT_MAX_TOKENS: u32 = 8192;
const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_DEADLINE: Duration = Duration::from_secs(300);
const DEFAULT_REPAIRS: u32 = 3;
const DEFAULT_REPLAY_ENTRIES: usize = 32;
const DEFAULT_REPLAY_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_OUTPUT_TOKENS: u32 = 32_768;
const MAX_CALL_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_DEADLINE: Duration = Duration::from_secs(3600);
const MAX_REPAIRS: u32 = 5;
const MAX_REPLAY_ENTRIES: usize = 1024;
const MAX_REPLAY_TTL: Duration = Duration::from_secs(24 * 3600);

/// The operator's explicit native authoring seat. Nothing here is a caller's choice.
#[derive(Clone)]
#[non_exhaustive]
pub struct NativeAuthoring {
    model: String,
    providers: ProvidersConfig,
    max_tokens: u32,
    call_timeout: Duration,
    deadline: Duration,
    repairs: u32,
    knowledge: Option<(PathBuf, Option<String>)>,
    replay_entries: usize,
    replay_ttl: Duration,
    withheld: Vec<Secret>,
}

impl NativeAuthoring {
    /// Seat `model` (`provider/name`, a direct provider) with the provider configuration
    /// (keys, endpoints) the composition root resolved. Defaults: 8192 output tokens and
    /// 120 s per call, 3 repair rounds, a 300 s deadline per request, no knowledge, 32 kept
    /// rounds for 30 minutes. Validated when the listener attaches, before it binds.
    #[must_use]
    pub fn new(model: impl Into<String>, providers: ProvidersConfig) -> Self {
        Self {
            model: model.into(),
            providers,
            max_tokens: DEFAULT_MAX_TOKENS,
            call_timeout: DEFAULT_CALL_TIMEOUT,
            deadline: DEFAULT_DEADLINE,
            repairs: DEFAULT_REPAIRS,
            knowledge: None,
            replay_entries: DEFAULT_REPLAY_ENTRIES,
            replay_ttl: DEFAULT_REPLAY_TTL,
            withheld: Vec::new(),
        }
    }

    /// Output tokens per call (1..=32768).
    #[must_use]
    pub const fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = tokens;
        self
    }

    /// The wait for one logical call (up to 600 s). The compiler never repeats a call; the
    /// provider transport may resend it after a 429, 503 or 529, inside this wait.
    #[must_use]
    pub const fn with_call_timeout(mut self, timeout: Duration) -> Self {
        self.call_timeout = timeout;
        self
    }

    /// One request's whole deadline (up to 3600 s): its work stops there.
    #[must_use]
    pub const fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// Repair rounds per request (0..=5): at most `1 + repairs` logical calls.
    #[must_use]
    pub const fn with_repairs(mut self, repairs: u32) -> Self {
        self.repairs = repairs;
        self
    }

    /// A Foundry knowledge snapshot directory, pinned when the listener attaches; a corpus
    /// whose examples are never recalled.
    #[must_use]
    pub fn with_knowledge(
        mut self,
        dir: impl Into<PathBuf>,
        exclude_corpus: Option<String>,
    ) -> Self {
        self.knowledge = Some((dir.into(), exclude_corpus));
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
            .field("max_tokens", &self.max_tokens)
            .field("call_timeout", &self.call_timeout)
            .field("deadline", &self.deadline)
            .field("repairs", &self.repairs)
            .field("knowledge", &self.knowledge.is_some())
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

/// `nika serve`'s explicit native authoring seat (the `nika compile` flag words, no
/// environment fallback). Absent `--authoring-model`, nothing is read and nothing changes.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct NativeAuthoringArgs {
    /// Seat native authoring on POST /v1/compile generation 2 with this direct provider model
    /// (`provider/name`); each caller still opts in (`cognition: "explicitProvider"`). Its key
    /// and endpoint are read from the environment now, never per request. Requires `--bind`.
    #[arg(
        long = "authoring-model",
        value_name = "PROVIDER/NAME",
        requires = "bind"
    )]
    pub model: Option<String>,
    /// Output tokens per call (1..=32768, default 8192); a caller may only narrow it.
    #[arg(long = "authoring-max-tokens", value_name = "N", requires = "model")]
    pub max_tokens: Option<u32>,
    /// Seconds per logical call (1..=600, default 120); the transport may resend one after a
    /// 429, 503 or 529 inside it, the compiler never repeats it.
    #[arg(long = "authoring-timeout", value_name = "SECONDS", requires = "model")]
    pub timeout: Option<u64>,
    /// Seconds per request (1..=3600, default 300): the work stops and the request answers 408.
    #[arg(
        long = "authoring-deadline",
        value_name = "SECONDS",
        requires = "model"
    )]
    pub deadline: Option<u64>,
    /// Repair rounds per request (0..=5, default 3): at most 1 + N logical calls.
    #[arg(long = "authoring-repairs", value_name = "N", requires = "model")]
    pub repairs: Option<u32>,
    /// A Foundry knowledge snapshot directory, verified and pinned at start; the seat reads
    /// the pack composed for each request's intent.
    #[arg(long = "knowledge", value_name = "DIR", requires = "model")]
    pub knowledge: Option<PathBuf>,
    /// A corpus whose examples the knowledge door never recalls.
    #[arg(
        long = "knowledge-exclude",
        value_name = "CORPUS",
        requires = "knowledge"
    )]
    pub knowledge_exclude: Option<String>,
}

/// Attach the seat `nika serve --authoring-model` names to the listener configuration. Absent
/// the flag, the configuration is returned untouched and nothing is read. Named, the provider
/// configuration (`config_from_env`, its own key precedence) is read once, now; the seat — and
/// the key it resolves, withheld from every answer — is validated when the listener attaches.
///
/// # Errors
/// [`NativeAuthoringError::NeedsListener`] when no listener is configured.
pub fn seat_native_authoring(
    http: Option<ServerConfig>,
    flags: &NativeAuthoringArgs,
) -> Result<Option<ServerConfig>, NativeAuthoringError> {
    let Some(model) = flags.model.as_deref() else {
        return Ok(http);
    };
    let Some(config) = http else {
        return Err(NativeAuthoringError::NeedsListener);
    };
    let mut seat = NativeAuthoring::new(model, nika_runtime::compose::config_from_env());
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
    Ok(Some(config.with_native_authoring(seat)))
}

/// A validated seat: what one bound server authors with.
pub(in crate::server) struct Seat {
    pub(super) model: String,
    /// The canonical provider id, for the receipt's backend.
    pub(super) provider: String,
    pub(super) providers: ProvidersConfig,
    pub(super) bounds: Bounds,
    knowledge: Option<Pin>,
    /// The seat's resolved key and the operator's further values: never answered.
    withheld: Vec<Secret>,
    pub(in crate::server) replays: Arc<Replays>,
    /// Raised once when the server stops: every round of this seat stops with it.
    halt: tokio::sync::watch::Sender<bool>,
}

/// The snapshot pinned at attach: its directory and exclusion, and the bytes it was read with.
struct Pin {
    dir: PathBuf,
    exclude: Option<String>,
    manifest_sha256: String,
    rows_sha256: String,
}

/// The pinned snapshot no longer reads as pinned (changed, stale or gone).
pub(super) struct ContextChanged;

impl Seat {
    /// Validate a seat: bounds, a direct provider model that resolves with its key, the
    /// snapshot opened and pinned. The key the provider resolved — whatever supplied it — joins
    /// the withheld values.
    pub(in crate::server) fn open(config: &NativeAuthoring) -> Result<Self, NativeAuthoringError> {
        let bounds = bounds(config)?;
        let (provider, key) = direct_provider(&config.model, &config.providers)?;
        let knowledge = match &config.knowledge {
            Some((dir, exclude)) => Some(pin(dir, exclude.clone())?),
            None => None,
        };
        let mut withheld = config.withheld.clone();
        withheld.extend(key);
        Ok(Self {
            model: config.model.clone(),
            provider,
            providers: config.providers.clone(),
            bounds,
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

    /// The pinned snapshot, opened again: `None` without one; refused unless its manifest and
    /// row files still read as they did at attach. Every generation-2 round checks it first.
    pub(super) fn context(
        &self,
    ) -> Result<Option<(knowledge::Snapshot, Option<&str>)>, ContextChanged> {
        let Some(pin) = &self.knowledge else {
            return Ok(None);
        };
        let snapshot = knowledge::Snapshot::open(&pin.dir).map_err(|_| ContextChanged)?;
        if snapshot.manifest_sha256() != pin.manifest_sha256
            || snapshot.rows_sha256() != pin.rows_sha256
        {
            return Err(ContextChanged);
        }
        Ok(Some((snapshot, pin.exclude.as_deref())))
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
    let refuse = |why| Err(NativeAuthoringError::Bound(why));
    if !(1..=MAX_OUTPUT_TOKENS).contains(&config.max_tokens) {
        return refuse("authoring output tokens per call must be 1..=32768");
    }
    if config.call_timeout.is_zero() || config.call_timeout > MAX_CALL_TIMEOUT {
        return refuse("the authoring timeout per call must be above zero and at most 600 s");
    }
    if config.deadline.is_zero() || config.deadline > MAX_DEADLINE {
        return refuse("the authoring deadline per request must be above zero and at most 3600 s");
    }
    if config.repairs > MAX_REPAIRS {
        return refuse("authoring repair rounds must be 0..=5");
    }
    if !(1..=MAX_REPLAY_ENTRIES).contains(&config.replay_entries)
        || config.replay_ttl.is_zero()
        || config.replay_ttl > MAX_REPLAY_TTL
    {
        return refuse("kept answer rounds must be 1..=1024 for at most 24 h");
    }
    Ok(Bounds {
        max_tokens: config.max_tokens,
        call_timeout: config.call_timeout,
        deadline: config.deadline,
        repairs: config.repairs,
    })
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

/// Pin a snapshot through the configuration parser every door shares: the strategy is the
/// seat's (`only`), the knowledge a snapshot directory with its exclusion.
fn pin(dir: &Path, exclude: Option<String>) -> Result<Pin, NativeAuthoringError> {
    let named = config::AuthoringSettings::none()
        .with_strategy(NativeMode::Only.word())
        .with_knowledge(dir, exclude);
    let resolved = config::resolve(&named, &config::AuthoringSettings::none())
        .map_err(|error| NativeAuthoringError::Knowledge(error.to_string()))?;
    let Some(config::KnowledgeSource::Snapshot {
        dir,
        exclude_corpus,
    }) = resolved.knowledge
    else {
        return Err(NativeAuthoringError::Knowledge(
            "only a snapshot directory can be pinned".to_owned(),
        ));
    };
    let snapshot = knowledge::Snapshot::open(&dir)
        .map_err(|error| NativeAuthoringError::Knowledge(error.to_string()))?;
    Ok(Pin {
        manifest_sha256: snapshot.manifest_sha256().to_owned(),
        rows_sha256: snapshot.rows_sha256(),
        dir,
        exclude: exclude_corpus,
    })
}
