// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authoring configuration every door that seats the compiler shares: WHEN the seat writes
//! the candidate itself (the compiler's own [`NativeMode`], the `--authoring-strategy` word) and
//! WHICH knowledge it reads beside the card (a Foundry snapshot directory the door composes a pack
//! from per intent, or a pack another builder composed for one intent). One parser: `nika compile`
//! resolves its flags and the environment through [`resolve`], the session resolves a host's typed
//! values and the environment it read once at its open through the same function. A door's own
//! explicit values win over the environment's; a knowledge source under `off` is refused, never
//! carried unread (only the native door reads knowledge). The explicit reasoning effort every
//! seat asks for is resolved the same way, and every door bounds and builds one seat's policy
//! through [`call_bounds`] and [`AuthoringConfig::policy`] (R4 B16).

use std::path::PathBuf;
use std::time::Duration;

use nika_compile::AuthoringReasoning;

use crate::compile::{AuthoringPolicy, NativeMode};

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
    /// A Foundry snapshot directory (`manifest.json` · one JSONL per kind · `relations.jsonl`):
    /// the door composes the pack for each intent and verifies every byte it presents.
    Snapshot {
        /// The snapshot directory.
        dir: PathBuf,
        /// A corpus whose examples are never recalled (a benchmark's own).
        exclude_corpus: Option<String>,
    },
    /// A pack another builder composed for ONE intent, entered as composed.
    Pack {
        /// The pack file (JSON).
        file: PathBuf,
    },
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
}

impl AuthoringSettings {
    /// Nothing named.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// The words the environment names for an authoring seat: `NIKA_AUTHORING_STRATEGY`,
    /// `NIKA_KNOWLEDGE`, `NIKA_KNOWLEDGE_EXCLUDE`, `NIKA_KNOWLEDGE_PACK`,
    /// `NIKA_AUTHORING_REASONING` — a strategy word, directories, a corpus name and an effort
    /// word, never a secret. Empty values name nothing.
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
        }
    }

    /// This reasoning effort word.
    #[must_use]
    pub fn with_reasoning(mut self, word: impl Into<String>) -> Self {
        self.reasoning = Some(word.into());
        self
    }

    /// The reasoning effort word alone: all a door without an authoring seat reads (a decision
    /// seat asks the effort; the strategy and the knowledge are an authoring seat's).
    #[must_use]
    pub fn reasoning_only(&self) -> Self {
        Self {
            reasoning: self.reasoning.clone(),
            ..Self::none()
        }
    }

    /// This strategy word.
    #[must_use]
    pub fn with_strategy(mut self, word: impl Into<String>) -> Self {
        self.strategy = Some(word.into());
        self
    }

    /// This knowledge snapshot directory, with the corpus it never recalls.
    #[must_use]
    pub fn with_knowledge(mut self, dir: impl Into<PathBuf>, exclude: Option<String>) -> Self {
        self.knowledge = Some(dir.into());
        self.knowledge_exclude = exclude;
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
}

/// The configuration a door resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuthoringConfig {
    /// When the seat writes the candidate itself.
    pub strategy: NativeMode,
    /// The knowledge it reads beside the card, when one is named.
    pub knowledge: Option<KnowledgeSource>,
    /// The explicit reasoning effort every seat asks for, when one is named (R4 B16).
    pub reasoning: Option<AuthoringReasoning>,
}

impl AuthoringConfig {
    /// The policy one seat authors under, as every door builds it (R4 B16): `model` bounded by
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
        Ok(match self.reasoning {
            Some(reasoning) => policy.with_reasoning(reasoning),
            None => policy,
        })
    }

    /// The knowledge door: a pre-composed pack enters as composed (an empty one carries no
    /// knowledge); a snapshot composes the pack for the intent the compiler reads (a revision's
    /// request with its change, a clarification's replacement), every presented byte verified
    /// against the snapshot's manifest. The provenance names the snapshot and the selection.
    ///
    /// # Errors
    /// A pack that is not a pack, a directory that is not a snapshot, a stale snapshot.
    pub fn with_knowledge(
        &self,
        request: crate::compile::CompileRequest,
        fallback_intent: &str,
    ) -> Result<crate::compile::CompileRequest, crate::knowledge::KnowledgeError> {
        match &self.knowledge {
            None => Ok(request),
            Some(KnowledgeSource::Pack { file }) => {
                Ok(match crate::knowledge::pack_from_file(file)? {
                    Some(pack) => request.with_authoring_knowledge(pack),
                    None => request,
                })
            }
            Some(KnowledgeSource::Snapshot {
                dir,
                exclude_corpus,
            }) => {
                let intent = crate::compile::revise_intent(&request)
                    .unwrap_or_else(|| fallback_intent.to_owned());
                if intent.trim().is_empty() {
                    return Ok(request);
                }
                let pack = crate::knowledge::Snapshot::open(dir)?
                    .pack(&intent, exclude_corpus.as_deref())?;
                Ok(request.with_authoring_knowledge(pack))
            }
        }
    }
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
    /// A corpus exclusion is named explicitly, but no snapshot is named to exclude it from (no
    /// knowledge, or a pack composed elsewhere): a held-out corpus is never silently unguarded.
    ExclusionWithoutSnapshot {
        /// The corpus named.
        corpus: String,
    },
    /// The reasoning effort word is none of the levels.
    UnknownReasoning(String),
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
                "the corpus `{corpus}` is excluded explicitly, but no knowledge snapshot is named to exclude it from — name the snapshot (--knowledge · NIKA_KNOWLEDGE), or drop the exclusion"
            ),
            Self::UnknownReasoning(word) => write!(
                f,
                "`{word}` is not a reasoning effort — low · high · max (--authoring-reasoning · NIKA_AUTHORING_REASONING)"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Resolve a door's explicit values over the environment's: the explicit strategy, else the
/// environment's, else [`DEFAULT_STRATEGY`]; the explicit knowledge source (a pack before a
/// snapshot), else the environment's (the same order). The corpus a benchmark excludes applies
/// to whichever snapshot is named (the explicit one, else the environment's), whichever side
/// named the corpus. Knowledge under `off` is refused: the only door that reads it never opens.
/// An exclusion named explicitly with no snapshot to exclude it from is refused; the
/// environment's, with none, simply has nothing to exclude.
///
/// # Errors
/// An unknown strategy word, knowledge named under `off`, an explicit exclusion without a
/// snapshot, or an unknown reasoning effort word.
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
    let knowledge = source_of(explicit, exclude).or_else(|| source_of(env, exclude));
    if strategy == NativeMode::Off
        && let Some(source) = &knowledge
    {
        return Err(ConfigError::KnowledgeUnread {
            source: match source {
                KnowledgeSource::Snapshot { dir, .. } => dir.display().to_string(),
                KnowledgeSource::Pack { file } => file.display().to_string(),
            },
        });
    }
    if let Some(corpus) = &explicit.knowledge_exclude
        && !matches!(knowledge, Some(KnowledgeSource::Snapshot { .. }))
    {
        return Err(ConfigError::ExclusionWithoutSnapshot {
            corpus: corpus.clone(),
        });
    }
    Ok(AuthoringConfig {
        strategy,
        knowledge,
        reasoning: reasoning(explicit, env)?,
    })
}

/// The effort word a door's flag names, else the environment's `NIKA_AUTHORING_REASONING`
/// (R4 B16): for a door that reads its flags once and checks the word when its seat opens.
#[must_use]
pub fn reasoning_word(flag: Option<&str>) -> Option<String> {
    flag.map(str::to_owned)
        .or_else(|| AuthoringSettings::from_env().reasoning)
}

/// The explicit reasoning effort a door's own word names, else the environment's (R4 B16):
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

/// One side's knowledge source: its pack before its snapshot.
fn source_of(settings: &AuthoringSettings, exclude: Option<&String>) -> Option<KnowledgeSource> {
    if let Some(file) = &settings.knowledge_pack {
        return Some(KnowledgeSource::Pack { file: file.clone() });
    }
    settings
        .knowledge
        .as_ref()
        .map(|dir| KnowledgeSource::Snapshot {
            dir: dir.clone(),
            exclude_corpus: exclude.cloned(),
        })
}

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
    fn nothing_named_is_the_default_strategy_without_knowledge() {
        let config = resolve(&AuthoringSettings::none(), &AuthoringSettings::none()).unwrap();
        assert_eq!(config.strategy, NativeMode::Escalate);
        assert_eq!(config.knowledge, None);
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
            })
        );
        // The explicit corpus wins over the environment's.
        let env = AuthoringSettings::none().with_knowledge("/env/snap", Some("dev".to_owned()));
        assert_eq!(
            resolve(&explicit, &env).unwrap().knowledge,
            Some(KnowledgeSource::Snapshot {
                dir: PathBuf::from("/env/snap"),
                exclude_corpus: Some("heldout".to_owned()),
            })
        );
        // No snapshot to exclude it from (nothing, or a pack composed elsewhere): refused.
        for env in [
            AuthoringSettings::none(),
            AuthoringSettings::none().with_knowledge_pack("/env/pack.json"),
        ] {
            assert_eq!(
                resolve(&explicit, &env),
                Err(ConfigError::ExclusionWithoutSnapshot {
                    corpus: "heldout".to_owned()
                })
            );
        }
        // The environment's own exclusion with no snapshot has nothing to exclude.
        let ambient = AuthoringSettings::none().with_knowledge_exclude("heldout");
        assert_eq!(
            resolve(&AuthoringSettings::none(), &ambient)
                .unwrap()
                .knowledge,
            None
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
        // `off` without knowledge stays a legitimate ablation.
        let off = AuthoringSettings::none().with_strategy("off");
        assert_eq!(
            resolve(&off, &AuthoringSettings::none()).unwrap().strategy,
            NativeMode::Off
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
