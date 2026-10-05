// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authoring configuration every door that seats the compiler shares: WHEN the seat writes the
//! candidate itself (the compiler's own [`NativeMode`], the `--authoring-strategy` word) and WHICH
//! knowledge it reads beside the card (a Foundry snapshot directory the door composes a pack from
//! per intent, the release this build embeds, or a pack another builder composed for one intent).
//! One parser: `nika compile` resolves its flags and the environment through [`resolve`], the
//! session resolves a host's typed values and the environment it read once at its open through the
//! same function. A door's own explicit values win over the environment's; a knowledge source under
//! `off` is refused, never carried unread (only the native door reads knowledge). The explicit
//! reasoning effort every seat asks for is resolved the same way, and every door bounds and builds
//! one seat's policy through [`call_bounds`] and [`AuthoringConfig::policy`].
//!
//! The knowledge choice is typed ([`KnowledgeChoice`]) and every door resolves it alike: the
//! first layer that says anything decides — the door's own explicit values, else the
//! environment's — and nothing said anywhere is this build's default: the release it embeds
//! ([`KnowledgeChoice::Default`]), admitted like any release, or nothing read under the strategy
//! `off` ([`KnowledgeChoice::Unread`]); said, never silent. A source named and refused never
//! falls back to it.
//! Knowledge is turned off by `--no-knowledge` on a door's own layer, or by the exact
//! environment word `NIKA_KNOWLEDGE=off`; off beside a source on the same layer is refused.
//!
//! A named release is admitted only against the identity an embedder trusts. A host names one on
//! its own layer with [`AuthoringSettings::with_knowledge_release`]. A flag or the environment
//! never carries one, so a directory they name is refused, typed, until a qualified identity
//! source is wired; nothing falls back.

use std::path::{Path, PathBuf};
use std::time::Duration;

use nika_compile::AuthoringReasoning;

use crate::compile::{AuthoringPolicy, NativeMode};
use crate::knowledge::TrustedIdentity;

/// The output cap one authoring call gets when its operator names none, on every door.
pub const DEFAULT_MAX_TOKENS: u32 = 8192;
/// The wait for one authoring call when its operator names none, on every door.
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(120);
/// The wait for one call of a harness seat (the operator's own agent through ACP, which thinks
/// and tools longer than one API call) when its operator names none.
pub const HARNESS_CALL_TIMEOUT: Duration = Duration::from_secs(300);
/// The largest output cap any door grants one authoring call.
pub const MAX_OUTPUT_TOKENS: u32 = 32_768;
/// The longest any door waits on one authoring call.
pub const MAX_CALL_TIMEOUT: Duration = Duration::from_secs(600);
/// The repair rounds a native candidate may buy when its operator names none, on every door.
pub const DEFAULT_REPAIRS: u32 = 3;
/// The most repair rounds any door grants a native candidate.
pub const MAX_REPAIRS: u32 = 5;

/// Check one authoring call's bounds as every door checks them: 1..=32768 output tokens and a
/// wait above zero and at most 600 s.
///
/// # Errors
/// The bound out of range, in the words every door refuses it with.
pub fn check_call_bounds(max_tokens: u32, timeout: Duration) -> Result<(), &'static str> {
    if !(1..=MAX_OUTPUT_TOKENS).contains(&max_tokens) {
        return Err("authoring output tokens per call must be 1..=32768");
    }
    if timeout.is_zero() || timeout > MAX_CALL_TIMEOUT {
        return Err("the authoring timeout per call must be above zero and at most 600 s");
    }
    Ok(())
}

/// One call's bounds: the operator's, else the doors' defaults ([`HARNESS_CALL_TIMEOUT`] for a
/// harness seat), checked by [`check_call_bounds`]. The cap never depends on the reasoning
/// effort: a call that runs out of it stays a failure.
///
/// # Errors
/// A bound out of range.
pub fn call_bounds(
    max_tokens: Option<u32>,
    timeout: Option<Duration>,
    harness: bool,
) -> Result<(u32, Duration), &'static str> {
    let default_timeout = if harness {
        HARNESS_CALL_TIMEOUT
    } else {
        DEFAULT_CALL_TIMEOUT
    };
    let bounds = (
        max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        timeout.unwrap_or(default_timeout),
    );
    check_call_bounds(bounds.0, bounds.1)?;
    Ok(bounds)
}

/// The strategy an explicit authoring seat gets when nothing names one: the native door opens
/// after the private plan fails a human.
pub const DEFAULT_STRATEGY: NativeMode = NativeMode::Escalate;

/// The strategy words, in the flag's order.
pub const STRATEGY_WORDS: [&str; 4] = ["escalate", "only", "sketch", "off"];

/// The compiler's native mode a strategy word names; `None` for any other word.
#[must_use]
pub fn native_mode(word: &str) -> Option<NativeMode> {
    match word.trim() {
        "escalate" => Some(NativeMode::Escalate),
        "only" => Some(NativeMode::Only),
        "sketch" => Some(NativeMode::Sketch),
        "off" => Some(NativeMode::Off),
        _ => None,
    }
}

/// Where the knowledge an authoring seat reads comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KnowledgeSource {
    /// A Foundry knowledge release root the strict door admits whole or refuses: the door
    /// composes the pack for each intent from the bytes it admitted.
    Snapshot {
        /// The release root.
        dir: PathBuf,
        /// A corpus whose examples are never recalled (a benchmark's own).
        exclude_corpus: Option<String>,
        /// The identity the layer that named the root trusts, from its own release record:
        /// without one the door refuses the root before it collects anything.
        identity: Option<TrustedIdentity>,
    },
    /// A pack another builder composed for ONE intent: no product door enters it, since nothing
    /// binds it to an admitted release ([`crate::knowledge::KnowledgeError::PackNotAdmitted`]).
    Pack {
        /// The pack file (JSON).
        file: PathBuf,
    },
    /// The release this build embeds, read where nothing names knowledge: no path, its identity
    /// the build's own issued constants, admitted by the strict memory door at every use.
    Embedded {
        /// A corpus whose examples are never recalled (a benchmark's own), as for any release.
        exclude_corpus: Option<String>,
    },
}

/// The settings layer that decided a knowledge choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KnowledgeLayer {
    /// A door's own explicit values: a flag, a host's typed value, the operator's serve flags.
    Explicit,
    /// The environment's: `NIKA_KNOWLEDGE` · `NIKA_KNOWLEDGE_PACK`.
    Environment,
}

impl KnowledgeLayer {
    /// The layer in one word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Environment => "environment",
        }
    }
}

/// The word `NIKA_KNOWLEDGE` takes, exactly, to turn the knowledge off.
pub const KNOWLEDGE_OFF: &str = "off";

/// What a door resolved for its authoring knowledge, before any byte is read.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KnowledgeChoice {
    /// A source a layer named: the strict door admits it or refuses it, never another source.
    Named {
        /// The source, as named.
        source: KnowledgeSource,
        /// The layer that named it.
        by: KnowledgeLayer,
    },
    /// Knowledge turned off by a layer: `--no-knowledge` on a door's own, or the exact word
    /// `NIKA_KNOWLEDGE=off` in the environment.
    Disabled {
        /// The layer that turned it off.
        by: KnowledgeLayer,
    },
    /// Nothing named anywhere: the release this build embeds ([`KnowledgeSource::Embedded`]),
    /// admitted like any release and composed where the strategy reads knowledge.
    Default,
    /// Nothing named anywhere under the strategy `off`: no knowledge is read, the compile is the
    /// pure one.
    Unread,
}

impl KnowledgeChoice {
    /// The choice in the words a status line says.
    #[must_use]
    pub fn words(&self) -> String {
        match self {
            Self::Named { by, .. } => format!("knowledge named ({})", by.word()),
            Self::Disabled {
                by: KnowledgeLayer::Explicit,
            } => "knowledge off (explicit)".to_owned(),
            Self::Disabled {
                by: KnowledgeLayer::Environment,
            } => format!("knowledge off (NIKA_KNOWLEDGE={KNOWLEDGE_OFF})"),
            Self::Default => "knowledge embedded (default)".to_owned(),
            Self::Unread => "knowledge unread (strategy off)".to_owned(),
        }
    }
}

/// The raw words a door was handed — its own explicit values, or the environment's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuthoringSettings {
    /// A strategy word (`escalate` · `only` · `sketch` · `off`).
    pub strategy: Option<String>,
    /// A knowledge snapshot directory.
    pub knowledge: Option<PathBuf>,
    /// A corpus the knowledge door never recalls.
    pub knowledge_exclude: Option<String>,
    /// A pack file composed for one intent.
    pub knowledge_pack: Option<PathBuf>,
    /// A reasoning effort word (`low` · `high` · `max`).
    pub reasoning: Option<String>,
    /// Source recovery rounds as a word (`0` · `1` · `2` · `3`): an explicit operator consent.
    pub source_recovery: Option<String>,
    /// Knowledge turned off on this layer (`--no-knowledge`).
    pub knowledge_off: bool,
    /// The identity this layer trusts for the snapshot it names, from an embedder's own release
    /// record ([`Self::with_knowledge_release`]); a flag or the environment never names one.
    pub knowledge_identity: Option<TrustedIdentity>,
}

impl AuthoringSettings {
    /// Nothing named.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// The environment's words for an authoring seat: `NIKA_AUTHORING_STRATEGY`, `NIKA_KNOWLEDGE`,
    /// `NIKA_KNOWLEDGE_EXCLUDE`, `NIKA_KNOWLEDGE_PACK`, `NIKA_AUTHORING_REASONING`,
    /// `NIKA_AUTHORING_SOURCE_RECOVERY` — names and a count, never a secret. Empty values name
    /// nothing; `NIKA_KNOWLEDGE=off` exactly is the environment's word for knowledge off, which
    /// [`resolve`] reads on that layer alone.
    #[must_use]
    #[allow(clippy::disallowed_methods)] // strategy, directory, corpus and effort names, NON-secret
    pub fn from_env() -> Self {
        let text = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        Self {
            strategy: text("NIKA_AUTHORING_STRATEGY"),
            knowledge: text("NIKA_KNOWLEDGE").map(PathBuf::from),
            knowledge_exclude: text("NIKA_KNOWLEDGE_EXCLUDE"),
            knowledge_pack: text("NIKA_KNOWLEDGE_PACK").map(PathBuf::from),
            reasoning: text("NIKA_AUTHORING_REASONING"),
            source_recovery: text("NIKA_AUTHORING_SOURCE_RECOVERY"),
            knowledge_off: false,
            knowledge_identity: None,
        }
    }

    /// This reasoning effort word.
    #[must_use]
    pub fn with_reasoning(mut self, word: impl Into<String>) -> Self {
        self.reasoning = Some(word.into());
        self
    }

    /// The reasoning effort word alone, with the knowledge off on this layer: all a door without
    /// an authoring seat reads (a decision seat asks the effort; the strategy and the knowledge
    /// are an authoring seat's), so it composes nothing, the embedded release included.
    #[must_use]
    pub fn reasoning_only(&self) -> Self {
        Self {
            reasoning: self.reasoning.clone(),
            knowledge_off: true,
            ..Self::none()
        }
    }

    /// This strategy word.
    #[must_use]
    pub fn with_strategy(mut self, word: impl Into<String>) -> Self {
        self.strategy = Some(word.into());
        self
    }

    /// This knowledge snapshot directory, with the corpus it never recalls, and no trusted
    /// identity: the door refuses it until one is named
    /// ([`Self::with_knowledge_release`]).
    #[must_use]
    pub fn with_knowledge(mut self, dir: impl Into<PathBuf>, exclude: Option<String>) -> Self {
        self.knowledge = Some(dir.into());
        self.knowledge_exclude = exclude;
        self.knowledge_identity = None;
        self
    }

    /// This release root with the identity the host trusts for it, from its own release record
    /// (never read from the root itself): the typed way a host injects a named source.
    #[must_use]
    pub fn with_knowledge_release(
        mut self,
        dir: impl Into<PathBuf>,
        identity: TrustedIdentity,
    ) -> Self {
        self.knowledge = Some(dir.into());
        self.knowledge_identity = Some(identity);
        self
    }

    /// This corpus never recalled, whichever snapshot is named (this side's or the other's).
    #[must_use]
    pub fn with_knowledge_exclude(mut self, corpus: impl Into<String>) -> Self {
        self.knowledge_exclude = Some(corpus.into());
        self
    }

    /// This pack file.
    #[must_use]
    pub fn with_knowledge_pack(mut self, file: impl Into<PathBuf>) -> Self {
        self.knowledge_pack = Some(file.into());
        self
    }

    /// Knowledge turned off on this layer, whatever source the other layer names.
    #[must_use]
    pub fn with_knowledge_off(mut self) -> Self {
        self.knowledge_off = true;
        self
    }

    /// A door's own words, each as its flag names it (none named is none): the strategy, the
    /// snapshot, the pack, the excluded corpus, the reasoning effort, and knowledge turned off.
    #[must_use]
    pub fn from_flags(
        strategy: Option<&str>,
        knowledge: Option<&Path>,
        pack: Option<&Path>,
        exclude: Option<&str>,
        reasoning: Option<&str>,
        knowledge_off: bool,
    ) -> Self {
        Self {
            strategy: strategy.map(str::to_owned),
            knowledge: knowledge.map(Path::to_path_buf),
            knowledge_exclude: exclude.map(str::to_owned),
            knowledge_pack: pack.map(Path::to_path_buf),
            reasoning: reasoning.map(str::to_owned),
            knowledge_off,
            ..Self::none()
        }
    }
}

/// The configuration a door resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuthoringConfig {
    /// When the seat writes the candidate itself.
    pub strategy: NativeMode,
    /// The knowledge it reads beside the card: a named source, or the embedded release.
    pub knowledge: Option<KnowledgeSource>,
    /// What the knowledge resolved to and which layer decided: named, off, the embedded
    /// default, or unread.
    pub choice: KnowledgeChoice,
    /// The explicit reasoning effort every seat asks for, when one is named.
    pub reasoning: Option<AuthoringReasoning>,
    /// The source recovery rounds the operator named (0: none), each charged to the same seat.
    pub source_recovery: u32,
}

impl AuthoringConfig {
    /// The policy one seat authors under, as every door builds it: `model` bounded by
    /// `max_tokens` and `timeout` (checked by [`check_call_bounds`]), with this configuration's
    /// strategy and reasoning effort. Samples and repairs keep the policy's own defaults.
    ///
    /// # Errors
    /// A bound out of range.
    pub fn policy(
        &self,
        model: &str,
        max_tokens: u32,
        timeout: Duration,
    ) -> Result<AuthoringPolicy, &'static str> {
        check_call_bounds(max_tokens, timeout)?;
        let policy = AuthoringPolicy::new(model, max_tokens, timeout).with_native(self.strategy);
        let policy = policy.with_source_recovery(self.source_recovery);
        Ok(match self.reasoning {
            Some(reasoning) => policy.with_reasoning(reasoning),
            None => policy,
        })
    }

    /// The knowledge door: a release the strict door admits — named on disk, or the one this
    /// build embeds — composes the pack for the intent the compiler reads (a revision's request
    /// with its change, a clarification's replacement), every presented byte admitted when the
    /// release opened; a pack composed elsewhere is refused before it is read; knowledge off or
    /// unread attaches nothing, including on a reused request. The provenance names the release
    /// and the selection.
    ///
    /// # Errors
    /// A release the strict door refuses ([`crate::knowledge::KnowledgeError::Unavailable`]), a
    /// pack composed elsewhere ([`crate::knowledge::KnowledgeError::PackNotAdmitted`]).
    pub fn with_knowledge(
        &self,
        mut request: crate::compile::CompileRequest,
        fallback_intent: &str,
    ) -> Result<crate::compile::CompileRequest, crate::knowledge::KnowledgeError> {
        request.authoring_knowledge = None;
        match &self.knowledge {
            None => Ok(request),
            Some(KnowledgeSource::Pack { file }) => {
                Err(crate::knowledge::KnowledgeError::PackNotAdmitted { file: file.clone() })
            }
            Some(KnowledgeSource::Snapshot {
                dir,
                exclude_corpus,
                identity,
            }) => composed(request, fallback_intent, exclude_corpus.as_deref(), || {
                crate::knowledge::Snapshot::open(dir, identity.as_ref())
            }),
            Some(KnowledgeSource::Embedded { exclude_corpus }) => {
                composed(request, fallback_intent, exclude_corpus.as_deref(), || {
                    crate::knowledge::bundled::admit(Some(&crate::knowledge::bundled::identity()?))
                })
            }
        }
    }
}

/// The request with the pack composed for the intent the compiler reads, from the release `open`
/// admits — opened only for an intent with words: none attaches nothing.
fn composed(
    request: crate::compile::CompileRequest,
    fallback_intent: &str,
    exclude_corpus: Option<&str>,
    open: impl FnOnce() -> Result<crate::knowledge::Snapshot, crate::knowledge::KnowledgeError>,
) -> Result<crate::compile::CompileRequest, crate::knowledge::KnowledgeError> {
    let intent =
        crate::compile::revise_intent(&request).unwrap_or_else(|| fallback_intent.to_owned());
    if intent.trim().is_empty() {
        return Ok(request);
    }
    let pack = open()?.pack(&intent, exclude_corpus)?;
    Ok(request.with_authoring_knowledge(pack))
}

/// Why a configuration cannot be honored.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigError {
    /// The strategy word is none of the four.
    UnknownStrategy(String),
    /// Knowledge is named under a strategy whose door never reads it.
    KnowledgeUnread {
        /// The knowledge source, as named.
        source: String,
    },
    /// A corpus exclusion is named explicitly, but no release is read to exclude it from
    /// (knowledge off, a pack composed elsewhere, or nothing read under the strategy `off`): a
    /// held-out corpus is never silently unguarded.
    ExclusionWithoutSnapshot {
        /// The corpus named.
        corpus: String,
    },
    /// The reasoning effort word is none of the levels.
    UnknownReasoning(String),
    /// The source recovery word is no count in `0..=3`, or names rounds under `off` or `only`.
    SourceRecovery(String),
    /// Knowledge turned off beside a source on the same settings layer.
    ContradictoryKnowledge {
        /// The layer that says both.
        layer: KnowledgeLayer,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStrategy(word) => write!(
                f,
                "`{word}` is not an authoring strategy — escalate · only · sketch · off"
            ),
            Self::KnowledgeUnread { source } => write!(
                f,
                "knowledge `{source}` is named under the strategy `off`, whose seat never reads it — name escalate, only or sketch, or unset the knowledge"
            ),
            Self::ExclusionWithoutSnapshot { corpus } => write!(
                f,
                "the corpus `{corpus}` is excluded explicitly, but no knowledge release is being read — unset source, pack and knowledge-off settings and select escalate, only or sketch to use the embedded default, or drop the exclusion"
            ),
            Self::UnknownReasoning(word) => write!(
                f,
                "`{word}` is not a reasoning effort — low · high · max (--authoring-reasoning · NIKA_AUTHORING_REASONING)"
            ),
            Self::SourceRecovery(w) => write!(f, "`{w}`: source recovery is 0..=3 (0 off or only)"),
            Self::ContradictoryKnowledge { layer } => write!(
                f,
                "the {} settings turn the knowledge off and name a source — keep one: the source, or knowledge off (--no-knowledge · NIKA_KNOWLEDGE=off)",
                layer.word()
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Resolve a door's explicit values over the environment's: the explicit strategy, else the
/// environment's, else [`DEFAULT_STRATEGY`]; the knowledge of the first layer that says
/// anything — the explicit one, else the environment's — and, when neither does, the release
/// this build embeds ([`KnowledgeChoice::Default`]), or nothing read under the strategy `off`
/// ([`KnowledgeChoice::Unread`]). On a layer, knowledge off beside a source is refused, and a
/// pack comes before a snapshot; the environment's knowledge off is the exact word
/// `NIKA_KNOWLEDGE=off`, while an explicit path named `off` is a directory. A named source is
/// never replaced by the embedded release. The corpus a benchmark excludes applies to whichever
/// release is read, the embedded one included, whichever side named the corpus. Knowledge named
/// under the strategy `off` is refused: the only door that reads it never opens. An exclusion
/// named explicitly with no release read to exclude it from (knowledge off, a pack, unread) is
/// refused; the environment's, with none, simply has nothing to exclude.
///
/// # Errors
/// An unknown strategy word, knowledge off beside a source on one layer, knowledge named under
/// `off`, an explicit exclusion without a release to filter, or an unknown reasoning effort word.
pub fn resolve(
    explicit: &AuthoringSettings,
    env: &AuthoringSettings,
) -> Result<AuthoringConfig, ConfigError> {
    let strategy = match explicit.strategy.as_deref().or(env.strategy.as_deref()) {
        Some(word) => {
            native_mode(word).ok_or_else(|| ConfigError::UnknownStrategy(word.to_owned()))?
        }
        None => DEFAULT_STRATEGY,
    };
    let exclude = explicit
        .knowledge_exclude
        .as_ref()
        .or(env.knowledge_exclude.as_ref());
    let choice = match layer_choice(explicit, KnowledgeLayer::Explicit, exclude)? {
        Some(choice) => choice,
        None => match layer_choice(env, KnowledgeLayer::Environment, exclude)? {
            Some(choice) => choice,
            None if strategy == NativeMode::Off => KnowledgeChoice::Unread,
            None => KnowledgeChoice::Default,
        },
    };
    let knowledge = match &choice {
        KnowledgeChoice::Named { source, .. } => Some(source.clone()),
        KnowledgeChoice::Default => Some(KnowledgeSource::Embedded {
            exclude_corpus: exclude.cloned(),
        }),
        KnowledgeChoice::Disabled { .. } | KnowledgeChoice::Unread => None,
    };
    if strategy == NativeMode::Off
        && let Some(source) = &knowledge
    {
        return Err(ConfigError::KnowledgeUnread {
            source: match source {
                KnowledgeSource::Snapshot { dir, .. } => dir.display().to_string(),
                KnowledgeSource::Pack { file } => file.display().to_string(),
                KnowledgeSource::Embedded { .. } => "the embedded release".to_owned(),
            },
        });
    }
    if let Some(corpus) = &explicit.knowledge_exclude
        && !matches!(
            knowledge,
            Some(KnowledgeSource::Snapshot { .. } | KnowledgeSource::Embedded { .. })
        )
    {
        return Err(ConfigError::ExclusionWithoutSnapshot {
            corpus: corpus.clone(),
        });
    }
    let word = (explicit.source_recovery.as_deref()).or(env.source_recovery.as_deref());
    let source_recovery =
        AuthoringPolicy::recovery_rounds(word, strategy).map_err(ConfigError::SourceRecovery)?;
    Ok(AuthoringConfig {
        strategy,
        knowledge,
        choice,
        reasoning: reasoning(explicit, env)?,
        source_recovery,
    })
}

/// The effort word a door's flag names, else the environment's `NIKA_AUTHORING_REASONING`
///: for a door that reads its flags once and checks the word when its seat opens.
#[must_use]
pub fn reasoning_word(flag: Option<&str>) -> Option<String> {
    flag.map(str::to_owned)
        .or_else(|| AuthoringSettings::from_env().reasoning)
}

/// The explicit reasoning effort a door's own word names, else the environment's:
/// `None` when neither names one. Both pass the same closed parser; the environment is not read
/// for a level the door names.
///
/// # Errors
/// A word that is none of the levels.
pub fn reasoning(
    explicit: &AuthoringSettings,
    env: &AuthoringSettings,
) -> Result<Option<AuthoringReasoning>, ConfigError> {
    explicit
        .reasoning
        .as_deref()
        .or(env.reasoning.as_deref())
        .map(|word| {
            AuthoringReasoning::parse(word.trim())
                .ok_or_else(|| ConfigError::UnknownReasoning(word.to_owned()))
        })
        .transpose()
}

/// One layer's knowledge as that layer's own grammar reads it: off (`knowledge_off`, or on the
/// environment's layer alone the exact word [`KNOWLEDGE_OFF`]), else its pack before its
/// snapshot, else nothing said; off beside a source on the layer is a contradiction.
fn layer_choice(
    settings: &AuthoringSettings,
    layer: KnowledgeLayer,
    exclude: Option<&String>,
) -> Result<Option<KnowledgeChoice>, ConfigError> {
    let word = layer == KnowledgeLayer::Environment
        && settings
            .knowledge
            .as_ref()
            .is_some_and(|named| named.as_os_str() == KNOWLEDGE_OFF);
    let source = match (&settings.knowledge_pack, &settings.knowledge) {
        (Some(file), _) => Some(KnowledgeSource::Pack { file: file.clone() }),
        (None, Some(dir)) if !word => Some(KnowledgeSource::Snapshot {
            dir: dir.clone(),
            exclude_corpus: exclude.cloned(),
            identity: settings.knowledge_identity.clone(),
        }),
        (None, _) => None,
    };
    match (settings.knowledge_off || word, source) {
        (true, Some(_)) => Err(ConfigError::ContradictoryKnowledge { layer }),
        (true, None) => Ok(Some(KnowledgeChoice::Disabled { by: layer })),
        (false, Some(source)) => Ok(Some(KnowledgeChoice::Named { source, by: layer })),
        (false, None) => Ok(None),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod knowledge_choice_tests;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_four_words_are_the_compilers_modes_and_nothing_else_is() {
        assert_eq!(native_mode("escalate"), Some(NativeMode::Escalate));
        assert_eq!(native_mode(" only "), Some(NativeMode::Only));
        assert_eq!(native_mode("sketch"), Some(NativeMode::Sketch));
        assert_eq!(native_mode("off"), Some(NativeMode::Off));
        assert_eq!(native_mode("native"), None);
        assert_eq!(STRATEGY_WORDS.map(|w| native_mode(w).is_some()), [true; 4]);
    }

    #[test]
    fn nothing_named_is_the_default_strategy_and_the_embedded_release() {
        let config = resolve(&AuthoringSettings::none(), &AuthoringSettings::none()).unwrap();
        assert_eq!(config.strategy, NativeMode::Escalate);
        assert_eq!(config.choice, KnowledgeChoice::Default);
        assert_eq!(
            config.knowledge,
            Some(KnowledgeSource::Embedded {
                exclude_corpus: None
            })
        );
    }

    /// A door without an authoring seat reads the effort alone: its layer turns the knowledge
    /// off, so neither a named release nor the embedded one is composed.
    #[test]
    fn a_door_without_an_authoring_seat_reads_the_effort_and_composes_nothing() {
        let named = AuthoringSettings::none()
            .with_strategy("only")
            .with_knowledge("/flag/snap", None)
            .with_reasoning("max");
        assert_eq!(
            named.reasoning_only(),
            AuthoringSettings::none()
                .with_reasoning("max")
                .with_knowledge_off()
        );
        let env = AuthoringSettings::none()
            .with_knowledge("/env/snap", None)
            .with_reasoning("high");
        let config = resolve(&named.reasoning_only(), &env.reasoning_only()).unwrap();
        assert_eq!(
            config.choice,
            KnowledgeChoice::Disabled {
                by: KnowledgeLayer::Explicit
            }
        );
        assert_eq!(config.knowledge, None);
        assert_eq!(config.strategy, DEFAULT_STRATEGY);
        assert_eq!(config.reasoning, Some(AuthoringReasoning::Max));
        let config = resolve(
            &AuthoringSettings::none().reasoning_only(),
            &env.reasoning_only(),
        )
        .unwrap();
        assert_eq!(
            (config.knowledge, config.reasoning),
            (None, Some(AuthoringReasoning::High))
        );
    }

    /// `NIKA_KNOWLEDGE=off`, as the environment's raw word reaches the parser, resolves to no
    /// knowledge at all: not a directory named `off`, not a pack, not another source.
    #[test]
    fn knowledge_off_in_the_environment_resolves_to_no_knowledge() {
        let env = AuthoringSettings::none().with_knowledge("off", None);
        let config = resolve(&AuthoringSettings::none(), &env).expect("resolves");
        assert_eq!(config.knowledge, None, "`off` turns the knowledge off");
    }

    #[test]
    fn explicit_values_win_over_the_environment_and_a_pack_over_a_snapshot() {
        let env = AuthoringSettings::none()
            .with_strategy("sketch")
            .with_knowledge("/env/snap", Some("sealed".to_owned()))
            .with_knowledge_pack("/env/pack.json");
        let explicit = AuthoringSettings::none()
            .with_strategy("only")
            .with_knowledge("/flag/snap", None);
        let config = resolve(&explicit, &env).unwrap();
        assert_eq!(config.strategy, NativeMode::Only);
        assert_eq!(
            config.knowledge,
            Some(KnowledgeSource::Snapshot {
                dir: PathBuf::from("/flag/snap"),
                exclude_corpus: Some("sealed".to_owned()),
                identity: None,
            }),
            "the explicit snapshot wins over the environment's pack; the excluded corpus holds for it"
        );
        let config = resolve(&AuthoringSettings::none(), &env).unwrap();
        assert_eq!(config.strategy, NativeMode::Sketch);
        assert_eq!(
            config.knowledge,
            Some(KnowledgeSource::Pack {
                file: PathBuf::from("/env/pack.json")
            }),
            "on one side a pack wins over a snapshot"
        );
    }

    #[test]
    fn an_explicit_exclusion_guards_the_environments_snapshot_and_is_never_dropped() {
        // The held-out corpus named explicitly holds for the snapshot the environment names.
        let explicit = AuthoringSettings::none().with_knowledge_exclude("heldout");
        let env = AuthoringSettings::none().with_knowledge("/env/snap", None);
        assert_eq!(
            resolve(&explicit, &env).unwrap().knowledge,
            Some(KnowledgeSource::Snapshot {
                dir: PathBuf::from("/env/snap"),
                exclude_corpus: Some("heldout".to_owned()),
                identity: None,
            })
        );
        // The explicit corpus wins over the environment's.
        let env = AuthoringSettings::none().with_knowledge("/env/snap", Some("dev".to_owned()));
        assert_eq!(
            resolve(&explicit, &env).unwrap().knowledge,
            Some(KnowledgeSource::Snapshot {
                dir: PathBuf::from("/env/snap"),
                exclude_corpus: Some("heldout".to_owned()),
                identity: None,
            })
        );
        // Nothing named: it guards the release this build embeds, never dropped.
        let embedded = Some(KnowledgeSource::Embedded {
            exclude_corpus: Some("heldout".to_owned()),
        });
        assert_eq!(
            resolve(&explicit, &AuthoringSettings::none())
                .unwrap()
                .knowledge,
            embedded
        );
        // A pack composed elsewhere reads no release to exclude it from: refused.
        assert_eq!(
            resolve(
                &explicit,
                &AuthoringSettings::none().with_knowledge_pack("/env/pack.json")
            ),
            Err(ConfigError::ExclusionWithoutSnapshot {
                corpus: "heldout".to_owned()
            })
        );
        // The environment's own exclusion guards the embedded release the same way.
        let ambient = AuthoringSettings::none().with_knowledge_exclude("heldout");
        assert_eq!(
            resolve(&AuthoringSettings::none(), &ambient)
                .unwrap()
                .knowledge,
            embedded
        );
    }

    #[test]
    fn an_unknown_word_and_knowledge_under_off_are_refused() {
        let unknown = AuthoringSettings::none().with_strategy("native");
        assert_eq!(
            resolve(&unknown, &AuthoringSettings::none()),
            Err(ConfigError::UnknownStrategy("native".to_owned()))
        );
        let unread = AuthoringSettings::none()
            .with_strategy("off")
            .with_knowledge("/snap", None);
        let error = resolve(&unread, &AuthoringSettings::none()).unwrap_err();
        assert!(matches!(error, ConfigError::KnowledgeUnread { .. }));
        assert!(error.to_string().contains("never reads it"), "{error}");
        // `off` without knowledge stays a legitimate ablation: nothing read, the pure compile.
        let off = AuthoringSettings::none().with_strategy("off");
        let config = resolve(&off, &AuthoringSettings::none()).unwrap();
        assert_eq!(
            (config.strategy, config.choice, config.knowledge),
            (NativeMode::Off, KnowledgeChoice::Unread, None)
        );
    }

    #[test]
    fn the_reasoning_word_is_the_doors_over_the_environments_and_one_of_three() {
        let none = AuthoringSettings::none;
        let env = none().with_reasoning("high");
        let explicit = none().with_reasoning("max");
        let level = |explicit: &AuthoringSettings, env: &AuthoringSettings| {
            resolve(explicit, env).unwrap().reasoning
        };
        assert_eq!(level(&explicit, &env), Some(AuthoringReasoning::Max));
        assert_eq!(level(&none(), &env), Some(AuthoringReasoning::High));
        assert_eq!(level(&none(), &none()), None);
        // The door's own level: the environment's is not read, however it is spelled.
        let wrong = none().with_reasoning("medium");
        assert_eq!(
            reasoning(&explicit, &wrong),
            Ok(Some(AuthoringReasoning::Max))
        );
        for word in ["medium", "MAX", "maximum", "none"] {
            let named = none().with_reasoning(word);
            let refused = Err(ConfigError::UnknownReasoning(word.to_owned()));
            assert_eq!(reasoning(&named, &none()), refused, "flag {word}");
            assert_eq!(reasoning(&none(), &named), refused, "env {word}");
            let error = resolve(&named, &none()).unwrap_err();
            assert!(error.to_string().contains("low · high · max"), "{error}");
        }
    }

    #[test]
    fn every_door_bounds_one_call_alike_and_the_policy_carries_the_level() {
        let seconds = Duration::from_secs;
        assert_eq!(call_bounds(None, None, false), Ok((8192, seconds(120))));
        assert_eq!(call_bounds(None, None, true), Ok((8192, seconds(300))));
        assert_eq!(
            call_bounds(Some(32_768), Some(seconds(600)), false),
            Ok((32_768, seconds(600)))
        );
        for (tokens, wait) in [(0, 120), (32_769, 120), (8192, 0), (8192, 601)] {
            let bounds = call_bounds(Some(tokens), Some(seconds(wait)), false);
            assert!(bounds.is_err(), "{tokens} {wait}");
        }
        let only = AuthoringSettings::none().with_strategy("only");
        let mut config = resolve(&only, &AuthoringSettings::none()).unwrap();
        let model = "deepseek/deepseek-v4-pro";
        let plain = config.policy(model, 8192, DEFAULT_CALL_TIMEOUT).unwrap();
        assert_eq!(plain.native, NativeMode::Only);
        assert_eq!(
            (plain.reasoning, plain.samples, plain.repairs),
            (None, 1, 3)
        );
        config.reasoning = Some(AuthoringReasoning::Max);
        let max = config.policy(model, 8192, DEFAULT_CALL_TIMEOUT).unwrap();
        assert_eq!(max.reasoning, Some(AuthoringReasoning::Max));
        assert_eq!(max.max_tokens, 8192, "the level never moves the cap");
        assert!(config.policy(model, 0, DEFAULT_CALL_TIMEOUT).is_err());
    }
}
