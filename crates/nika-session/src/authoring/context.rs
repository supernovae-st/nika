// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's authoring context — WHEN the compiler's seat writes the candidate itself and
//! WHICH Foundry knowledge it reads beside the card, under the seat the human chose. The same
//! configuration `nika compile` resolves, through the same parser
//! ([`nika_cli_host::compile::config`]): read from the environment ONCE when a host door opens
//! the session, or handed by a host as typed values. A named snapshot is opened when the
//! configuration is resolved — whatever the seat — to verify and pin its identity (its version,
//! its digest, the digest of its row files); it is opened and verified again at every seated
//! use: a snapshot that changed under the session is refused, never presented under the identity
//! the session pinned. A configuration that cannot be honored is said when the session opens and
//! refused at the first seated turn — never a silent card alone. Under a deterministic seat the
//! configuration is only stated (`/status`): no pack is composed, nothing is presented to a
//! model, nothing is sent.

use std::path::PathBuf;

use nika_cli_host::compile::config::{
    self, AuthoringConfig, AuthoringSettings, ConfigError, DEFAULT_STRATEGY, KnowledgeSource,
};
use nika_cli_host::compile::knowledge::{KnowledgeError, Snapshot};
use nika_onboard::compile::{AuthoringKnowledge, NativeMode};
use serde_json::{Value, json};

use super::DecisionSetup;

/// Why a session's authoring configuration cannot be honored.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AuthoringContextError {
    /// The configuration itself: an unknown strategy word, knowledge named under `off`, an
    /// explicit exclusion with no snapshot.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// The knowledge source cannot be read or verified: not a snapshot, a stale snapshot.
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
        /// The identity found on disk, in the same words.
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

/// The identity a session pinned for its knowledge snapshot when it opened.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KnowledgePin {
    /// The snapshot directory.
    pub dir: PathBuf,
    /// A corpus whose examples are never recalled.
    pub exclude_corpus: Option<String>,
    /// The snapshot's version, as its manifest names it.
    pub version: Option<String>,
    /// The digest the manifest declares (the exporter's, never recomputed).
    pub digest: Option<String>,
    /// The sha256 of the manifest's bytes as read (computed): the pin's integrity, with the rows.
    pub manifest_sha256: String,
    /// The digest of the row files as the door read them (computed).
    pub rows_sha256: String,
}

impl KnowledgePin {
    /// The pin's identity record (what a receipt names).
    #[must_use]
    pub fn record(&self) -> Value {
        json!({
            "version": self.version,
            "digest": self.digest,
            "digest_is": "declared by the manifest, not recomputed",
            "manifest_sha256": self.manifest_sha256,
            "rows_sha256": self.rows_sha256,
            "dir": self.dir.display().to_string(),
            "exclude_corpus": self.exclude_corpus,
        })
    }

    /// The pin as a snapshot on disk states it now.
    fn of(snapshot: &Snapshot) -> (Option<&str>, Option<&str>, &str, String) {
        (
            snapshot.version(),
            snapshot.digest(),
            snapshot.manifest_sha256(),
            snapshot.rows_sha256(),
        )
    }

    /// The identity in words: version · declared digest · manifest · rows (cut at twelve).
    fn words(version: Option<&str>, digest: Option<&str>, manifest: &str, rows: &str) -> String {
        format!(
            "{} (declared digest {} · manifest {} · rows {})",
            version.unwrap_or("unversioned"),
            short(digest.unwrap_or("none")),
            short(manifest),
            short(rows)
        )
    }
}

/// The session's authoring configuration: the strategy the one policy carries, the knowledge
/// snapshot pinned for the session, or the reason the configuration cannot be honored.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuthoringContext {
    strategy: NativeMode,
    knowledge: Option<KnowledgePin>,
    refusal: Option<AuthoringContextError>,
    source: &'static str,
    decision: Option<DecisionSetup>,
    project: Option<PathBuf>,
}

impl Default for AuthoringContext {
    fn default() -> Self {
        Self {
            strategy: DEFAULT_STRATEGY,
            knowledge: None,
            refusal: None,
            source: "default",
            decision: None,
            project: None,
        }
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

    /// This project root: a seated compile observes the files its request names under it (the
    /// shared bounded observer — headers, keys, short categorical values, never a row), never
    /// through a link that leads outside it. Without a root nothing is observed.
    #[must_use]
    pub fn with_project_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.project = Some(root.into());
        self
    }

    /// The project root a seated compile observes under, when one is set.
    #[must_use]
    pub fn project_root(&self) -> Option<&std::path::Path> {
        self.project.as_deref()
    }

    /// Set the project root in place (the session's own root, at each seated compile).
    pub(crate) fn set_project_root(&mut self, root: &std::path::Path) {
        if self.project.as_deref() != Some(root) {
            self.project = Some(root.to_path_buf());
        }
    }

    /// A host's typed values over the environment's (either may name nothing), resolved by the
    /// parser every door shares; a named snapshot is opened, verified and pinned now.
    #[must_use]
    pub fn from_settings(explicit: &AuthoringSettings, env: &AuthoringSettings) -> Self {
        let source = if *explicit != AuthoringSettings::none() {
            "host"
        } else if *env != AuthoringSettings::none() {
            "environment"
        } else {
            "default"
        };
        match Self::pin(explicit, env) {
            Ok((strategy, knowledge)) => Self {
                strategy,
                knowledge,
                refusal: None,
                source,
                decision: None,
                project: None,
            },
            Err(error) => Self {
                refusal: Some(error),
                source,
                ..Self::default()
            },
        }
    }

    /// The resolved strategy and the pinned snapshot, or why they cannot be honored.
    fn pin(
        explicit: &AuthoringSettings,
        env: &AuthoringSettings,
    ) -> Result<(NativeMode, Option<KnowledgePin>), AuthoringContextError> {
        let AuthoringConfig {
            strategy,
            knowledge,
            ..
        } = config::resolve(explicit, env)?;
        let pin = match knowledge {
            None => None,
            Some(KnowledgeSource::Snapshot {
                dir,
                exclude_corpus,
            }) => {
                let snapshot = Snapshot::open(&dir)?;
                Some(KnowledgePin {
                    version: snapshot.version().map(str::to_owned),
                    digest: snapshot.digest().map(str::to_owned),
                    manifest_sha256: snapshot.manifest_sha256().to_owned(),
                    rows_sha256: snapshot.rows_sha256(),
                    dir,
                    exclude_corpus,
                })
            }
            Some(KnowledgeSource::Pack { file }) => {
                return Err(AuthoringContextError::PackForOneRequest { file });
            }
            Some(_) => return Err(AuthoringContextError::UnsupportedSource),
        };
        Ok((strategy, pin))
    }

    /// When the seat writes the candidate itself.
    #[must_use]
    pub fn strategy(&self) -> NativeMode {
        self.strategy
    }

    /// The knowledge snapshot pinned for the session, when one is named.
    #[must_use]
    pub fn knowledge(&self) -> Option<&KnowledgePin> {
        self.knowledge.as_ref()
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
        format!("{}{decision}", self.base_line())
    }

    /// The strategy and knowledge words of the `/status` line.
    fn base_line(&self) -> String {
        let strategy = self.strategy.word();
        match &self.knowledge {
            None => format!(
                "authoring context · strategy {strategy} ({}) · no knowledge snapshot",
                self.source
            ),
            Some(pin) => format!(
                "authoring context · strategy {strategy} ({}) · knowledge {} · declared digest {} · manifest {} · rows {} · presented only under a selected model seat",
                self.source,
                pin.version.as_deref().unwrap_or("unversioned"),
                short(pin.digest.as_deref().unwrap_or("none")),
                short(&pin.manifest_sha256),
                short(&pin.rows_sha256)
            ),
        }
    }

    /// The pack for one intent from the pinned snapshot, opened and verified again: `Ok(None)`
    /// when no snapshot is pinned; refused when the snapshot on disk is no longer the one pinned
    /// — its manifest's own bytes, its rows, its declared version or digest — or a byte it would
    /// present is not its manifest's.
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
        let snapshot = Snapshot::open(&pin.dir)?;
        let (version, digest, manifest, rows) = KnowledgePin::of(&snapshot);
        if version != pin.version.as_deref()
            || digest != pin.digest.as_deref()
            || manifest != pin.manifest_sha256
            || rows != pin.rows_sha256
        {
            return Err(AuthoringContextError::Changed {
                pinned: KnowledgePin::words(
                    pin.version.as_deref(),
                    pin.digest.as_deref(),
                    &pin.manifest_sha256,
                    &pin.rows_sha256,
                ),
                found: KnowledgePin::words(version, digest, manifest, &rows),
            });
        }
        Ok(Some(snapshot.pack(intent, pin.exclude_corpus.as_deref())?))
    }
}

/// The first twelve characters of a digest.
fn short(digest: &str) -> String {
    digest.chars().take(12).collect()
}
