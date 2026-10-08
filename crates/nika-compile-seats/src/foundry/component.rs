// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An executable Foundry component (R4 · C13): a checked block of an admitted release — a whole
//! `.nika` program — named by its id, the release version and the release digest, with its exact
//! bytes and the holes its producer marked (document key paths, who fills each). Only an
//! admitted release resolves a reference ([`ComponentCatalog`]); a reference is read from a typed
//! field ([`ComponentRef::from_value`]), never found in prose. Every other kind of knowledge (a
//! pattern, a family, a skill, a repair principle) stays knowledge to read: resolving it is
//! [`Unresolved::NotExecutable`], never an execution claim.

use std::fmt;

use nika_compile::surface::sha256;
use serde_json::{Value, json};

/// The id prefix of the one executable kind a release ships.
pub const BLOCK: &str = "block:";

/// The id prefixes of the kinds a release ships as knowledge only.
const KNOWLEDGE: [(&str, &str); 10] = [
    ("pattern:", "pattern"),
    ("family:", "family"),
    ("pack:", "pattern pack"),
    ("diagnostic:", "diagnostic"),
    ("repair:", "repair principle"),
    ("src:", "source artifact"),
    ("skeleton:", "skeleton"),
    ("skill:", "skill"),
    ("example:", "example"),
    ("nika:", "callable contract"),
];

/// A reference to one component: the block id, and the release it must come from — its version,
/// its `SNAPSHOT_SHA256`, either or both. A reference without either resolves against whatever
/// release the catalogue holds, and the receipt names the one it got.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ComponentRef {
    /// `block:<name>`.
    pub id: String,
    /// The release's `knowledge_version`, when the reference pins one.
    pub version: Option<String>,
    /// The release's `SNAPSHOT_SHA256`, when the reference pins one.
    pub release: Option<String>,
}

impl ComponentRef {
    /// A reference to `id`, unpinned.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: None,
            release: None,
        }
    }

    /// The same reference pinned to a release version.
    #[must_use]
    pub fn at_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// The same reference pinned to a release digest.
    #[must_use]
    pub fn in_release(mut self, release: impl Into<String>) -> Self {
        self.release = Some(release.into());
        self
    }

    /// The reference a proposal states: exactly `{"id": …}` with an optional `"version"` and an
    /// optional `"release"`, each a string. Anything else is no reference: a text that merely
    /// looks like an id is data, and a reference is never found by scanning prose.
    ///
    /// # Errors
    /// [`Unresolved::Malformed`] naming what departs from that closed shape.
    pub fn from_value(value: &Value) -> Result<Self, Unresolved> {
        let malformed = |why: &str| Unresolved::Malformed(why.to_owned());
        let object = value
            .as_object()
            .ok_or_else(|| malformed("a component reference is an object"))?;
        if let Some(key) = object
            .keys()
            .find(|key| !matches!(key.as_str(), "id" | "version" | "release"))
        {
            return Err(malformed(&format!("an unknown key `{key}`")));
        }
        let text = |key: &str| -> Result<Option<String>, Unresolved> {
            match object.get(key) {
                None => Ok(None),
                Some(Value::String(text)) if !text.trim().is_empty() && text.trim() == text => {
                    Ok(Some(text.clone()))
                }
                Some(_) => Err(malformed(&format!(
                    "`{key}` is not a trimmed, non-empty text"
                ))),
            }
        };
        let id = text("id")?.ok_or_else(|| malformed("no `id`"))?;
        Ok(Self {
            id,
            version: text("version")?,
            release: text("release")?,
        })
    }

    /// The reference as a proposal states it ([`Self::from_value`] reads it back).
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut value = json!({"id": self.id});
        if let Some(version) = &self.version {
            value["version"] = json!(version);
        }
        if let Some(release) = &self.release {
            value["release"] = json!(release);
        }
        value
    }

    /// The block's name after its prefix, when the id is a block id of the release grammar
    /// (`[A-Za-z0-9][A-Za-z0-9._:/@+-]{0,199}` after the prefix): the kind is judged first, so a
    /// knowledge id is [`Unresolved::NotExecutable`] and anything else [`Unresolved::NotAComponent`].
    ///
    /// # Errors
    /// Why `id` names no block.
    pub fn block_name(&self) -> Result<&str, Unresolved> {
        if let Some(name) = self.id.strip_prefix(BLOCK) {
            let mut chars = name.chars();
            let first = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
            let rest = chars.all(|c| c.is_ascii_alphanumeric() || "._:/@+-".contains(c));
            if first && rest && name.chars().count() <= 200 {
                return Ok(name);
            }
            return Err(Unresolved::NotAComponent(self.id.clone()));
        }
        match KNOWLEDGE
            .iter()
            .find(|(prefix, _)| self.id.starts_with(prefix))
        {
            Some((_, kind)) => Err(Unresolved::NotExecutable {
                id: self.id.clone(),
                kind: (*kind).to_owned(),
            }),
            None => Err(Unresolved::NotAComponent(self.id.clone())),
        }
    }
}

impl fmt::Display for ComponentRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id)?;
        if let Some(version) = &self.version {
            write!(f, " (version {version})")?;
        }
        if let Some(release) = &self.release {
            write!(f, " (release {})", release.get(..12).unwrap_or(release))?;
        }
        Ok(())
    }
}

/// One parameter of a component: a key path of its document, who fills it, and the producer's
/// note on how.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Hole {
    /// The key path (`const.batch_path` · `tasks.totals.invoke.args.expression` · `inputs`).
    pub name: String,
    /// Who fills it: `human` (the request or the person states it), `deterministic` (read from
    /// the observed data), or another owner the release names.
    pub owner: String,
    /// The producer's note, when it states one.
    pub note: Option<String>,
}

impl Hole {
    /// One hole.
    #[must_use]
    pub fn new(name: impl Into<String>, owner: impl Into<String>, note: Option<String>) -> Self {
        Self {
            name: name.into(),
            owner: owner.into(),
            note,
        }
    }

    /// Whether `path` is this hole or lies under it (`inputs.company.default` under `inputs`).
    #[must_use]
    pub fn covers(&self, path: &str) -> bool {
        path == self.name
            || path
                .strip_prefix(self.name.as_str())
                .is_some_and(|rest| rest.starts_with('.'))
    }
}

/// The admitted release a component was resolved from.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Release {
    /// The `knowledge_version` its manifest names.
    pub version: String,
    /// Its `SNAPSHOT_SHA256`: the sha256 of the manifest's bytes as admitted.
    pub snapshot_sha256: String,
    /// The admission profile that verified it.
    pub profile: String,
}

impl Release {
    /// One release identity.
    #[must_use]
    pub fn new(
        version: impl Into<String>,
        snapshot_sha256: impl Into<String>,
        profile: impl Into<String>,
    ) -> Self {
        Self {
            version: version.into(),
            snapshot_sha256: snapshot_sha256.into(),
            profile: profile.into(),
        }
    }

    /// The identity as a receipt names it.
    #[must_use]
    pub fn record(&self) -> Value {
        json!({
            "version": self.version,
            "snapshot_sha256": self.snapshot_sha256,
            "profile": self.profile,
        })
    }
}

/// An admitted executable component: its identity in its release, its exact admitted bytes, its
/// holes, and the effects, authority and callables its row declares. The declared authority is a
/// need, never a grant: an expansion carries no permit of the component.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Component {
    /// `block:<name>`.
    pub id: String,
    /// The release it was resolved from.
    pub release: Release,
    /// The row's canonical digest, as admitted.
    pub row_sha256: String,
    /// The program file under the release root (`blocks/<slug>.nika`).
    pub file: String,
    /// The sha256 the manifest pins for that file.
    pub file_sha256: String,
    /// The file's exact admitted bytes.
    pub source: String,
    /// The holes the producer marked.
    pub holes: Vec<Hole>,
    /// The effects the row declares (`fs.read` · `fs.write`).
    pub effects: Vec<String>,
    /// The authority the row declares it needs (`permits.fs` · `permits.tools`).
    pub authority: Vec<String>,
    /// The callables its tasks reach (`nika:read` · `nika:jq`).
    pub callables: Vec<String>,
    /// The capability interfaces the row names.
    pub interfaces: Vec<String>,
    /// The row's status (`EXPERIMENTAL` · `QUALIFIED` · …).
    pub status: String,
    /// The row's proof level (`CHECKED`).
    pub proof_level: String,
}

impl Component {
    /// A component of `release` with its exact `source`; the other facts start empty and are
    /// filled by the resolver from the row.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        release: Release,
        file: impl Into<String>,
        file_sha256: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            release,
            row_sha256: String::new(),
            file: file.into(),
            file_sha256: file_sha256.into(),
            source: source.into(),
            holes: Vec::new(),
            effects: Vec::new(),
            authority: Vec::new(),
            callables: Vec::new(),
            interfaces: Vec::new(),
            status: String::new(),
            proof_level: String::new(),
        }
    }

    /// A block row of an admitted release and its file's admitted bytes, as one component: the
    /// row's identity, holes, effects, authority, callables, interfaces, status and proof.
    ///
    /// # Errors
    /// [`Unresolved::Integrity`] when the bytes are absent, not UTF-8, or not the ones the row
    /// pins.
    pub fn from_row(
        release: Release,
        row: &Value,
        bytes: Option<&[u8]>,
    ) -> Result<Self, Unresolved> {
        let text = |key: &str| row[key].as_str().unwrap_or_default().to_owned();
        let lines = |key: &str| -> Vec<String> {
            (row[key].as_array().into_iter().flatten())
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        };
        let id = text("id");
        let source = bytes.and_then(|bytes| std::str::from_utf8(bytes).ok());
        let Some(source) = source else {
            return Err(Unresolved::Integrity(id));
        };
        let mut component = Self::new(id, release, text("file"), text("file_sha256"), source);
        if !component.verified() {
            return Err(Unresolved::Integrity(component.id));
        }
        component.row_sha256 = text("sha256");
        component.holes = (row["holes"].as_array().into_iter().flatten())
            .filter_map(|hole| {
                let note = hole["note"].as_str().map(str::to_owned);
                Some(Hole::new(
                    hole["name"].as_str()?,
                    hole["owner"].as_str()?,
                    note,
                ))
            })
            .collect();
        component.effects = lines("effects");
        component.authority = lines("authority");
        component.callables = lines("callables");
        component.interfaces = lines("interfaces");
        component.status = text("status");
        component.proof_level = text("proof_level");
        Ok(component)
    }

    /// Whether the bytes held are the ones the release pins.
    #[must_use]
    pub fn verified(&self) -> bool {
        sha256(&self.source) == self.file_sha256
    }

    /// The reference that resolves to exactly this component.
    #[must_use]
    pub fn reference(&self) -> ComponentRef {
        ComponentRef::new(self.id.clone())
            .at_version(self.release.version.clone())
            .in_release(self.release.snapshot_sha256.clone())
    }

    /// The hole that covers `path`, when one does.
    #[must_use]
    pub fn hole(&self, path: &str) -> Option<&Hole> {
        self.holes.iter().find(|hole| hole.covers(path))
    }

    /// The component's identity as a receipt names it: never its bytes again.
    #[must_use]
    pub fn record(&self) -> Value {
        json!({
            "id": self.id,
            "release": self.release.record(),
            "row_sha256": self.row_sha256,
            "file": self.file,
            "file_sha256": self.file_sha256,
            "status": self.status,
            "proof_level": self.proof_level,
        })
    }
}

/// Why a reference resolves to no admitted component. Each is said, never replaced by another
/// component or by the component's text.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unresolved {
    /// The proposal's reference departs from its closed shape.
    Malformed(String),
    /// The id is of no kind a release ships.
    NotAComponent(String),
    /// The id names knowledge to read (a pattern, a family, a skill…), never a program.
    NotExecutable {
        /// The id named.
        id: String,
        /// Its kind.
        kind: String,
    },
    /// No such block in the admitted release.
    Unknown(String),
    /// The reference pins another release version than the one admitted.
    VersionMismatch {
        /// The id named.
        id: String,
        /// The version the reference pins.
        wanted: String,
        /// The version the catalogue holds.
        held: String,
    },
    /// The reference pins another release digest than the one admitted.
    ReleaseMismatch {
        /// The id named.
        id: String,
        /// The digest the reference pins.
        wanted: String,
        /// The digest the catalogue holds.
        held: String,
    },
    /// The release holds the row but not bytes its pin names, or none at all.
    Integrity(String),
}

impl fmt::Display for Unresolved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(why) => write!(f, "no component reference: {why}"),
            Self::NotAComponent(id) => {
                write!(f, "`{id}` names no executable component (`block:<name>`)")
            }
            Self::NotExecutable { id, kind } => write!(
                f,
                "`{id}` is a {kind}: knowledge to read, never a component to expand"
            ),
            Self::Unknown(id) => write!(f, "the admitted release holds no component `{id}`"),
            Self::VersionMismatch { id, wanted, held } => write!(
                f,
                "`{id}` is pinned at release version `{wanted}`; the admitted release is `{held}`"
            ),
            Self::ReleaseMismatch { id, wanted, held } => write!(
                f,
                "`{id}` is pinned to release {}; the admitted release is {}",
                wanted.get(..12).unwrap_or(wanted),
                held.get(..12).unwrap_or(held)
            ),
            Self::Integrity(id) => write!(
                f,
                "`{id}`'s program bytes are not the ones its release pins: not expanded"
            ),
        }
    }
}

impl std::error::Error for Unresolved {}

/// What resolves a reference to an admitted component: the knowledge door's admitted release
/// (`nika_onboard::knowledge::Snapshot`), or a test's. A catalogue answers for one release.
pub trait ComponentCatalog: Send + Sync {
    /// The release this catalogue holds.
    fn release(&self) -> Release;
    /// The component `reference` names, judged against [`Self::release`].
    ///
    /// # Errors
    /// Why it resolves to none ([`Unresolved`]).
    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved>;
}

/// The pins of `reference` judged against `release`: the version, then the digest.
///
/// # Errors
/// The first pin the release does not satisfy.
pub fn pinned(reference: &ComponentRef, release: &Release) -> Result<(), Unresolved> {
    if let Some(wanted) = &reference.version
        && *wanted != release.version
    {
        return Err(Unresolved::VersionMismatch {
            id: reference.id.clone(),
            wanted: wanted.clone(),
            held: release.version.clone(),
        });
    }
    if let Some(wanted) = &reference.release
        && *wanted != release.snapshot_sha256
    {
        return Err(Unresolved::ReleaseMismatch {
            id: reference.id.clone(),
            wanted: wanted.clone(),
            held: release.snapshot_sha256.clone(),
        });
    }
    Ok(())
}
