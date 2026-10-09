// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's authoring context — WHEN the compiler's seat writes the candidate itself and
//! WHICH Foundry knowledge it reads beside the card, under the seat the human chose. The same
//! configuration `nika compile` resolves, through the same parser
//! ([`nika_cli_host::compile::config`]): read from the environment ONCE when a host door opens
//! the session, or handed by a host as typed values. A named release is admitted by the strict
//! door when the configuration is resolved — whatever the seat — and its identity pinned (its
//! version, its `SNAPSHOT_SHA256`, the digest of its rows); it is admitted again at every seated
//! use: a release that changed under the session is refused, never presented under the identity
//! the session pinned. Nothing named is the release this build embeds, pinned the same way (in
//! memory, no path); knowledge off and unread are stated (`/status`), never silent. A
//! configuration that cannot be honored is said when the session opens and refused at the first
//! seated turn — never a silent card alone. Under a deterministic seat the configuration is only
//! stated: no pack is composed, nothing is presented to a model, nothing is sent.

use std::path::PathBuf;

use nika_cli_host::compile::config::{
    self, AuthoringSettings, ConfigError, DEFAULT_STRATEGY, KnowledgeChoice, KnowledgeSource,
};
use nika_cli_host::compile::knowledge::{KnowledgeError, RefusalCode};
use nika_onboard::compile::{AuthoringKnowledge, AuthoringReasoning, NativeMode};
// The pin owns its identity beside the snapshot door (C7 · D1); the policy stays here.
use nika_onboard::knowledge::pin::KnowledgePin;

use super::DecisionSetup;

/// Why a session's authoring configuration cannot be honored.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AuthoringContextError {
    /// The configuration itself: an unknown strategy word, knowledge named under `off`, an
    /// explicit exclusion with no snapshot.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// The strict door refused the named release: knowledge unavailable, typed.
    #[error(transparent)]
    Knowledge(#[from] KnowledgeError),
    /// A pack composed elsewhere for ONE request cannot serve a session's requests.
    #[error(
        "the pack `{}` was composed for one request; a session composes a pack for each of its requests — name the snapshot it came from (NIKA_KNOWLEDGE) instead",
        file.display()
    )]
    PackForOneRequest {
        /// The pack file named.
        file: PathBuf,
    },
    /// A knowledge source this session does not know how to read.
    #[error("a knowledge source this session cannot read")]
    UnsupportedSource,
    /// The snapshot on disk is no longer the one the session pinned when it opened.
    #[error(
        "the knowledge snapshot changed under this session: pinned {pinned}, found {found} — open the session again to author under the new snapshot"
    )]
    Changed {
        /// The pinned identity, in words (version · digest · rows).
        pinned: String,
        /// What is found on disk: the same words for a release admitted on its own, the strict
        /// door's words for bytes the pinned identity no longer names.
        found: String,
    },
    /// The operator selected a decision seat the session cannot build (no key, a malformed
    /// model, a vendor not wired here): refused visibly, never silently dropped.
    #[error("decision seat {seat}: {why}")]
    Decision {
        /// The selection as named (`typesafe/jev-1.13.0`).
        seat: String,
        /// Why it cannot be built.
        why: String,
    },
}

/// The session's authoring configuration: the strategy the one policy carries, the knowledge
/// snapshot pinned for the session, or the reason the configuration cannot be honored.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuthoringContext {
    strategy: NativeMode,
    knowledge: Option<KnowledgePin>,
    /// What the knowledge resolved to (named, off, the embedded default, unread): stated, never
    /// hashed — off and unread present the same nothing, and a release read is its pin.
    choice: KnowledgeChoice,
    refusal: Option<AuthoringContextError>,
    source: &'static str,
    decision: Option<DecisionSetup>,
    project: Option<PathBuf>,
    /// The explicit reasoning effort every seated call asks for, when one is named, or the word
    /// the parser refused: resolved apart from the rest, so no other refusal drops it.
    reasoning: Result<Option<AuthoringReasoning>, ConfigError>,
    /// The source recovery rounds the operator named (0: none), printed only when named.
    pub(crate) recovery: u32,
    /// The host's settings and the environment's this configuration was resolved from, as read
    /// once: a later explicit choice resolves through the same parser, never another read.
    settings: (AuthoringSettings, AuthoringSettings),
}

#[allow(clippy::missing_fields_in_debug)] // the hashed identity bytes stay the pre-choice form
impl std::fmt::Debug for AuthoringContext {
    /// The derived form, the reasoning effort appended only when one is named: a context
    /// naming none keeps the derived form every question identity and cost binding hashes. The
    /// knowledge choice is never printed: a release read is its pin, off and unread the same
    /// nothing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("AuthoringContext");
        debug
            .field("strategy", &self.strategy)
            .field("knowledge", &self.knowledge)
            .field("refusal", &self.refusal)
            .field("source", &self.source)
            .field("decision", &self.decision)
            .field("project", &self.project);
        match &self.reasoning {
            Ok(Some(level)) => debug.field("reasoning", level),
            Err(refused) => debug.field("reasoning", refused),
            Ok(None) => &mut debug,
        };
        if self.recovery > 0 {
            debug.field("source_recovery", &self.recovery);
        }
        debug.finish()
    }
}

impl Default for AuthoringContext {
    /// Nothing named on either layer, resolved as every door resolves it: this build's embedded
    /// release, pinned now. A release the strict door refuses is this context's refusal, said and
    /// typed, never a panic.
    fn default() -> Self {
        Self::from_settings(&AuthoringSettings::none(), &AuthoringSettings::none())
    }
}

impl AuthoringContext {
    /// The configuration the environment names (`NIKA_AUTHORING_STRATEGY` · `NIKA_KNOWLEDGE` ·
    /// `NIKA_KNOWLEDGE_EXCLUDE` · `NIKA_KNOWLEDGE_PACK`, and the session's operator-selected
    /// decision seat [`DECISION_ENV`](super::DECISION_ENV)), read now — a host door reads it once,
    /// when it opens the session. A named snapshot is opened now to pin its identity.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_settings(&AuthoringSettings::none(), &AuthoringSettings::from_env())
            .with_decision(DecisionSetup::from_env())
    }

    /// This operator-selected decision seat (or none). A seat that cannot be built becomes this
    /// context's refusal — said at `/status`, refused at the first seated turn — unless an
    /// earlier refusal already stands.
    #[must_use]
    pub fn with_decision(mut self, decision: Option<DecisionSetup>) -> Self {
        if let Some(setup) = &decision
            && let Some(why) = setup.refusal()
            && self.refusal.is_none()
        {
            self.refusal = Some(AuthoringContextError::Decision {
                seat: setup.model().to_owned(),
                why: why.to_owned(),
            });
        }
        self.decision = decision;
        self
    }

    /// The operator-selected decision seat, when one is named.
    #[must_use]
    pub fn decision(&self) -> Option<&DecisionSetup> {
        self.decision.as_ref()
    }

    /// This project root: every compile observes the files its request names under it (the
    /// shared bounded observer — headers, keys, short categorical values, never a row), never
    /// through a link that leads outside it. Without a root nothing is observed.
    #[must_use]
    pub fn with_project_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.project = Some(root.into());
        self
    }

    /// The project root every compile observes under, when one is set.
    #[must_use]
    pub fn project_root(&self) -> Option<&std::path::Path> {
        self.project.as_deref()
    }

    /// A host's typed values over the environment's (either may name nothing), resolved by the
    /// parser every door shares; the release it reads (the one named, or the one this build
    /// embeds) is admitted, verified and pinned now.
    #[must_use]
    pub fn from_settings(explicit: &AuthoringSettings, env: &AuthoringSettings) -> Self {
        let source = if *explicit != AuthoringSettings::none() {
            "host"
        } else if *env != AuthoringSettings::none() {
            "environment"
        } else {
            "default"
        };
        // The level resolves apart, through the same parser: no other refusal drops it.
        let reasoning = config::reasoning(explicit, env);
        let settings = (explicit.clone(), env.clone());
        match Self::pin(explicit, env) {
            Ok((resolved, knowledge)) => Self {
                strategy: resolved.strategy,
                knowledge,
                choice: resolved.choice,
                refusal: None,
                source,
                decision: None,
                project: None,
                reasoning,
                recovery: resolved.source_recovery,
                settings,
            },
            // Field by field, never through `Default`, which resolves and may refuse in turn: a
            // refused configuration pins nothing and reads nothing.
            Err(error) => Self {
                strategy: DEFAULT_STRATEGY,
                knowledge: None,
                choice: KnowledgeChoice::Unread,
                refusal: Some(error),
                source,
                decision: None,
                project: None,
                reasoning,
                recovery: 0,
                settings,
            },
        }
    }

    /// The same configuration with the release this build embeds named explicitly over either
    /// layer's knowledge: the strategy, the effort, a held-out corpus, the decision seat, the
    /// project root and the source word stay as they were; the release is admitted and pinned now.
    #[must_use]
    pub fn with_embedded_knowledge(&self) -> Self {
        let (explicit, env) = &self.settings;
        let explicit = explicit.clone().with_knowledge_embedded();
        let mut context = Self::from_settings(&explicit, env).with_decision(self.decision.clone());
        context.source = self.source;
        context.project.clone_from(&self.project);
        context
    }

    /// What the knowledge resolved to before any admission: the source a layer named (kept when
    /// the strict door refused it), the embedded default, off or unread; `None` when the
    /// configuration itself does not resolve.
    #[must_use]
    pub fn knowledge_named(&self) -> Option<KnowledgeChoice> {
        let (explicit, env) = &self.settings;
        config::resolve(explicit, env)
            .ok()
            .map(|config| config.choice)
    }

    /// The resolved configuration and the pinned release, or why they cannot be honored.
    fn pin(
        explicit: &AuthoringSettings,
        env: &AuthoringSettings,
    ) -> Result<(config::AuthoringConfig, Option<KnowledgePin>), AuthoringContextError> {
        let config = config::resolve(explicit, env)?;
        match &config.knowledge {
            None | Some(KnowledgeSource::Snapshot { .. } | KnowledgeSource::Embedded { .. }) => {}
            Some(KnowledgeSource::Pack { file }) => {
                return Err(AuthoringContextError::PackForOneRequest { file: file.clone() });
            }
            Some(_) => return Err(AuthoringContextError::UnsupportedSource),
        }
        // The one pin every door takes: a release on disk, or the one this build embeds.
        let pin = KnowledgePin::of_config(&config)?;
        Ok((config, pin))
    }

    /// When the seat writes the candidate itself.
    #[must_use]
    pub fn strategy(&self) -> NativeMode {
        self.strategy
    }

    /// The source recovery rounds the operator named (0: none), as a cost review reserves them.
    #[must_use]
    pub fn recovery(&self) -> u32 {
        self.recovery
    }

    /// The knowledge release pinned for the session: the one named, or the one this build embeds
    /// when nothing is named; none when the knowledge is off or unread.
    #[must_use]
    pub fn knowledge(&self) -> Option<&KnowledgePin> {
        self.knowledge.as_ref()
    }

    /// What the knowledge resolved to: a named release, off (and which layer said so), the
    /// release this build embeds, or unread (the strategy `off`, or a refused configuration).
    #[must_use]
    pub fn knowledge_choice(&self) -> &KnowledgeChoice {
        &self.choice
    }

    /// Why the configuration cannot be honored, when it cannot.
    #[must_use]
    pub fn refusal(&self) -> Option<&AuthoringContextError> {
        self.refusal.as_ref()
    }

    /// Where the configuration came from: `default` · `environment` · `host`.
    #[must_use]
    pub fn source(&self) -> &'static str {
        self.source
    }

    /// The explicit reasoning effort every seated call asks for, when one is named.
    #[must_use]
    pub fn reasoning(&self) -> Option<AuthoringReasoning> {
        self.reasoning.as_ref().ok().copied().flatten()
    }

    /// The level every call of the conversation asks, or why none may be asked: a word the parser
    /// refused is never read as no level.
    ///
    /// # Errors
    /// The configured word the shared parser refused.
    pub fn reasoning_asked(&self) -> Result<Option<AuthoringReasoning>, ConfigError> {
        self.reasoning.clone()
    }

    /// The `/status` line: the strategy and its source, the pinned knowledge, or the refusal —
    /// and the operator-selected decision seat, when one is named.
    #[must_use]
    pub fn line(&self) -> String {
        let decision = self
            .decision
            .as_ref()
            .map_or_else(String::new, |d| format!(" · {}", d.line()));
        if let Some(why) = &self.refusal {
            return format!(
                "authoring context · refused ({}): {why}{decision}",
                self.source
            );
        }
        // Scoped to the LLM calls: the TypeSafe decision seat is a separate backend (B19).
        let effort = self.reasoning().map_or_else(String::new, |r| {
            let seat = (self.decision.is_some()).then_some(
                "; the TypeSafe decision seat is a separate backend: no effort is sent to it",
            );
            let seat = seat.unwrap_or_default();
            format!(
                " · reasoning effort {} asked of every LLM call{seat}",
                r.word()
            )
        });
        format!("{}{effort}{decision}", self.base_line())
    }

    /// The strategy and knowledge words of the `/status` line.
    fn base_line(&self) -> String {
        let strategy = self.strategy.word();
        match &self.knowledge {
            None => format!(
                "authoring context · strategy {strategy} ({}) · {}",
                self.source,
                self.choice.words()
            ),
            Some(pin) => format!(
                "authoring context · strategy {strategy} ({}) · {} · presented only under a selected model seat",
                self.source,
                pin.status_words()
            ),
        }
    }

    /// The pack for one intent from the pinned release, admitted again against the same trusted
    /// identity: `Ok(None)` when none is pinned or the intent has no words (as at the compile
    /// door); refused when the release is no longer the one pinned — its manifest's own bytes,
    /// its rows, its declared version or digest — or a byte it would present is not its
    /// manifest's.
    ///
    /// # Errors
    /// [`AuthoringContextError::Changed`] when the snapshot changed under the session,
    /// [`AuthoringContextError::Knowledge`] when it is no snapshot any more or is stale.
    pub fn compose(
        &self,
        intent: &str,
    ) -> Result<Option<AuthoringKnowledge>, AuthoringContextError> {
        let Some(pin) = &self.knowledge else {
            return Ok(None);
        };
        if intent.trim().is_empty() {
            return Ok(None);
        }
        let snapshot = pin.reopen().map_err(|error| match error {
            // Other bytes under the pin's trusted identity: the release changed under the session.
            KnowledgeError::Unavailable {
                code: RefusalCode::IdentityMismatch,
                detail,
                ..
            } => AuthoringContextError::Changed {
                pinned: pin.identity_words(),
                found: detail,
            },
            other => other.into(),
        })?;
        if let Some((pinned, found)) = pin.moved(&snapshot) {
            return Err(AuthoringContextError::Changed { pinned, found });
        }
        Ok(Some(snapshot.pack(intent, pin.exclude_corpus.as_deref())?))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
