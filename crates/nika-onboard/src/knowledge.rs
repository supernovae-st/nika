// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The knowledge door: `--knowledge <release root>` admits a Foundry knowledge release
//! ([`RELEASE_FORMAT`]: `knowledge/manifest.json` · one JSONL per kind · the relations · the files
//! its blocks name · the notices) and composes, for one intent, the authoring pack the seat reads
//! beside the card — the families the intent belongs to, their patterns and the checked blocks
//! that realize them (and the solved examples and the skill of the leading family, which a
//! policy-R release never holds) — plus the repair principles the release wires to diagnostic
//! codes, for the repair rounds. Deterministic retrieval (BM25 over the rows' text and the
//! release's graph), bounded (three families · eight patterns · four blocks · three examples ·
//! one skill · ~40 KiB), and stated: the selection record names every row it selected, why, and
//! whether it was presented or excluded (and for what reason); the identity carries the release's
//! version, its `SNAPSHOT_SHA256` (the sha256 of its manifest's bytes) and this builder's version.
//! The order of what is taken is relevance — each recalled family, then the direct text match, in
//! turn — never the rows' ids, and a block is presented with the holes, effects, capabilities,
//! known failures and version its row states. The selection is this door's own (Rust BM25 over the
//! Foundry graph), not the one the Foundry producer computes, and the record says so.
//!
//! Admitted against a trusted identity, never trusted for its own claims. One strict admission
//! ([`Snapshot::open`] on disk, [`Snapshot::from_files`] in memory, [`ADMISSION_PROFILE`]) serves
//! every product door (`nika compile`, the session, serve).
//!
//! The embedder names the [`TrustedIdentity`] it expects (the release's `SNAPSHOT_SHA256` and
//! its policy), from its own release record. A source without one is refused before anything is
//! collected. A named directory carries none today. This build embeds one qualified release,
//! admitted in memory against the identity its owner issued: the knowledge where nothing names
//! any.
//!
//! The release is read once:
//! - every byte is bound to the manifest's pins;
//! - every row to its digest, its kind's closed and typed schema, the manifest's target, its
//!   evidence and its lineage;
//! - every relation to admitted rows.
//!
//! A release that fails any rule is refused as a typed [`KnowledgeError::Unavailable`]. Nothing is
//! loaded: no partial load, no other source.
//!
//! A pack presents the admitted bytes and never reads a second time. A pack composed elsewhere for
//! one intent is not admitted ([`KnowledgeError::PackNotAdmitted`]). The pack's own digest (every
//! reference and repair principle it can present) rides the identity's `door` record.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::compile::{AuthoringKnowledge, KnowledgeReference};
use nika_event::source_id::sha256_hex;
use serde_json::{Value, json};

/// The strict admission of a release: its profile and its typed refusals.
mod admission;
/// The release this build embeds: its issued bytes compiled in, admitted in memory.
pub(crate) mod bundled;
/// The byte contract a release shares with its producer: strict JSON and the canonical digest.
mod canonical;
/// A synthetic release the strict door admits (tests, and doors with `test-support`).
#[cfg(any(test, feature = "test-support"))]
pub mod fixture;
/// The historical snapshot layout, read for the retrieval baseline tests only.
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod legacy;
/// The pinned snapshot identity and the records a session stamps on an outcome (C7 · D1).
pub mod pin;

pub use admission::{ADMISSION_PROFILE, RELEASE_FORMAT, RefusalCode, TrustedIdentity};

/// The snapshot identity as an answer may carry it: every hash, count and selection, no host
/// path (the snapshot directory, the files root).
pub fn redact_host_paths(identity: &mut serde_json::Value) {
    if let Some(identity) = identity.as_object_mut() {
        identity.remove("dir");
        if let Some(verification) = identity
            .get_mut("verification")
            .and_then(serde_json::Value::as_object_mut)
        {
            verification.remove("files_root");
        }
    }
}

/// This builder's version, stated beside the snapshot digest (v2: every presented byte is
/// verified against the manifest's pins, and the pack states its own digest; v3: relevance
/// survives deduplication, each recalled family gets its block in turn, the receipt separates
/// selected, excluded with a reason and presented, and a block states its row's metadata; v4:
/// one strict admission against a trusted identity (profile r1 of the shared contract), every
/// presented byte admitted when the release opens, never read again).
pub const PACK_BUILDER: &str = "nika-compile/knowledge-door-v4";
const FAMILIES: usize = 3;
const PATTERNS: usize = 8;
const BLOCKS: usize = 4;
const EXAMPLES: usize = 3;
const SKILLS: usize = 1;
/// The most bytes one referenced file contributes.
const FILE_BYTES: usize = 6 * 1024;
/// The most bytes the whole pack contributes.
const PACK_BYTES: usize = 40 * 1024;
/// The most bytes one block's metadata (holes, effects, capabilities, failures, version) adds.
const METADATA_BYTES: usize = 1024;
/// The most repair principles one repair round carries.
const PRINCIPLES: usize = 3;

/// Why the knowledge door refused a source: a named source is never replaced by no knowledge.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KnowledgeError {
    /// This reader does not implement the configured knowledge-source kind.
    UnsupportedSource,
    /// The strict door refused the release at `root`: knowledge unavailable, never a partial
    /// load and never another source.
    Unavailable {
        /// The release root named.
        root: PathBuf,
        /// The first cause admission found.
        code: RefusalCode,
        /// What it found, in words.
        detail: String,
    },
    /// The file is not a knowledge pack.
    NotAPack {
        /// The file named.
        file: PathBuf,
        /// Why.
        why: String,
    },
    /// A pack composed elsewhere for one intent: nothing binds its bytes to an admitted release,
    /// so no product door enters it.
    PackNotAdmitted {
        /// The pack file named.
        file: PathBuf,
    },
}

impl std::fmt::Display for KnowledgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSource => {
                f.write_str("this knowledge source is not supported by this reader")
            }
            Self::Unavailable { root, code, detail } => write!(
                f,
                "knowledge unavailable: the strict door refused `{}` ({code}: {detail}) — name a release this engine admits, or turn the knowledge off (--no-knowledge · NIKA_KNOWLEDGE=off)",
                root.display()
            ),
            Self::NotAPack { file, why } => {
                write!(f, "`{}` is not a knowledge pack ({why})", file.display())
            }
            Self::PackNotAdmitted { file } => write!(
                f,
                "the knowledge pack `{}` is not admitted: a pack composed elsewhere is bound to no admitted release — name the release root instead (--knowledge · NIKA_KNOWLEDGE)",
                file.display()
            ),
        }
    }
}

impl std::error::Error for KnowledgeError {}

/// An admitted release: its manifest and pins, the rows by kind, the relations, and every
/// file's admitted bytes — what a pack presents is what admission verified.
#[derive(Debug)]
pub struct Snapshot {
    dir: PathBuf,
    manifest: Value,
    rows: BTreeMap<String, Vec<Value>>,
    relations: Vec<Value>,
    /// The manifest's pins: a path under the release root → its sha256.
    pins: BTreeMap<String, String>,
    /// Every file's admitted bytes but the manifest's, by path under the root.
    files: BTreeMap<String, Vec<u8>>,
    /// Every row file's sha256 as admitted, by name.
    row_files: BTreeMap<String, String>,
    /// The sha256 of the manifest's bytes as admitted: the release's `SNAPSHOT_SHA256`, which
    /// content-addresses every file through the manifest's pins.
    manifest_sha256: String,
    /// The admission that verified it ([`ADMISSION_PROFILE`]).
    admission: &'static str,
}

impl Snapshot {
    /// Admit the release at the absolute root `dir` through the strict door, against the
    /// identity an embedder trusts. Nothing of `dir` is touched without one. Then:
    /// - every file is collected on held descriptors and read once;
    /// - every byte is bound to its pin;
    /// - every row and relation is checked against this reader's profile
    ///   ([`ADMISSION_PROFILE`]).
    ///
    /// # Errors
    /// [`KnowledgeError::Unavailable`] with the first cause found ([`RefusalCode`]): no trusted
    /// identity is [`RefusalCode::Untrusted`]. Nothing is loaded from a refused release, not even
    /// part of it.
    pub fn open(dir: &Path, identity: Option<&TrustedIdentity>) -> Result<Self, KnowledgeError> {
        Self::admitted(
            dir.to_path_buf(),
            admission::admit(dir, identity, &mut |_, _| {}),
        )
    }

    /// Admit a release an embedder holds in memory: its files by relative path, the manifest's
    /// included. The rules are the disk form's, its collection aside: the paths and bounds are
    /// judged before anything is hashed. `label` names the source in a refusal (a build's own
    /// knowledge root, say).
    ///
    /// # Errors
    /// As [`Self::open`].
    pub fn from_files(
        label: &str,
        files: BTreeMap<String, Vec<u8>>,
        identity: Option<&TrustedIdentity>,
    ) -> Result<Self, KnowledgeError> {
        Self::admitted(
            PathBuf::from(label),
            admission::admit_memory(files, identity),
        )
    }

    /// The snapshot an admission kept, or its refusal bound to the source it named.
    fn admitted(
        dir: PathBuf,
        admitted: Result<admission::Admitted, admission::Refusal>,
    ) -> Result<Self, KnowledgeError> {
        let admitted = admitted.map_err(|admission::Refusal(_step, code, detail)| {
            KnowledgeError::Unavailable {
                root: dir.clone(),
                code,
                detail,
            }
        })?;
        Ok(Self {
            dir,
            manifest: admitted.manifest,
            rows: admitted.rows,
            relations: admitted.relations,
            pins: admitted.pins,
            files: admitted.files,
            row_files: admitted.row_files,
            manifest_sha256: admitted.manifest_sha256,
            admission: ADMISSION_PROFILE,
        })
    }

    /// The snapshot's version, as its manifest names it.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        manifest_text(&self.manifest, "knowledge_version")
    }

    /// The digest the manifest DECLARES (the exporter's), when it declares one: a release
    /// declares none — its identity is [`Self::manifest_sha256`], computed here.
    #[must_use]
    pub fn digest(&self) -> Option<&str> {
        manifest_text(&self.manifest, "digest")
    }

    /// The sha256 of the manifest's bytes as admitted — the release's `SNAPSHOT_SHA256`: any
    /// change to any of its files changes a pin, and so the manifest's bytes.
    #[must_use]
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }

    /// The digest of the row files as admitted (each name and sha256, in name order): the
    /// identity of the rows held.
    #[must_use]
    pub fn rows_sha256(&self) -> String {
        use std::fmt::Write as _;
        let lines = self
            .row_files
            .iter()
            .fold(String::new(), |mut lines, (name, sha)| {
                let _ = writeln!(lines, "{name} {sha}");
                lines
            });
        sha256_hex(lines.as_bytes())
    }

    /// The release's identity for the provenance record: the version its manifest names, its
    /// `SNAPSHOT_SHA256` and the rows' digest (computed here), the builder, and the admission that
    /// verified it, with the format and the policy the manifest names.
    #[must_use]
    pub fn identity(&self) -> Value {
        let declared = |key: &str| self.manifest.get(key).cloned().unwrap_or(Value::Null);
        json!({
            "version": declared("knowledge_version"),
            "digest": declared("digest"),
            "source_commit": declared("source_commit"),
            "pack_builder": PACK_BUILDER,
            "dir": self.dir.display().to_string(),
            "manifest_sha256": self.manifest_sha256,
            "snapshot_sha256": self.manifest_sha256,
            "rows_sha256": self.rows_sha256(),
            "verification": {
                "admission": self.admission,
                "format": declared("format"),
                "profile": declared("profile"),
                "policy": declared("policy"),
                "manifest_pins": self.pins.len(),
                "row_files": self.row_files.len(),
            },
        })
    }

    fn rows(&self, kind: &str) -> &[Value] {
        self.rows.get(kind).map_or(&[], Vec::as_slice)
    }

    fn row(&self, id: &str) -> Option<&Value> {
        let kind = id.split(':').next()?;
        let file = match kind {
            "family" => "families",
            "pack" => "pattern_packs",
            "pattern" => "patterns",
            "block" => "blocks",
            "example" => "examples",
            "skill" => "skills",
            "repair" => "repair_principles",
            "diagnostic" => "diagnostics",
            other => other,
        };
        self.rows(file)
            .iter()
            .find(|r| r.get("id").and_then(Value::as_str) == Some(id))
    }

    /// The targets of `from --rel--> to` edges leaving `from`.
    fn targets(&self, from: &str, rel: &str) -> Vec<&str> {
        self.relations
            .iter()
            .filter(|r| {
                r.get("from").and_then(Value::as_str) == Some(from)
                    && r.get("rel").and_then(Value::as_str) == Some(rel)
            })
            .filter_map(|r| r.get("to").and_then(Value::as_str))
            .collect()
    }

    /// The sources of `from --rel--> to` edges reaching `to`.
    fn sources(&self, to: &str, rel: &str) -> Vec<&str> {
        self.relations
            .iter()
            .filter(|r| {
                r.get("to").and_then(Value::as_str) == Some(to)
                    && r.get("rel").and_then(Value::as_str) == Some(rel)
            })
            .filter_map(|r| r.get("from").and_then(Value::as_str))
            .collect()
    }

    /// A referenced file's admitted text, bounded; `None` when the release holds no such file
    /// (the composition records the absence) or its bytes are not UTF-8.
    fn file_text(&self, relative: &str, composition: &mut Composition) -> Option<String> {
        let Some(bytes) = self.files.get(relative) else {
            composition.absent.push(relative.to_owned());
            return None;
        };
        composition.verified += 1;
        std::str::from_utf8(bytes)
            .ok()
            .map(|text| cut(text, FILE_BYTES))
    }

    /// The authoring pack for one intent: the references the seat reads, the selection
    /// record, the identity with the pack's own digest. `exclude_corpus` keeps a benchmark
    /// honest: no example of the case's own corpus is recalled.
    ///
    /// # Errors
    /// None once admitted: the strict door verified every byte a pack can present when the
    /// release opened ([`Self::open`]); the result keeps the door's callers' type.
    pub fn pack(
        &self,
        intent: &str,
        exclude_corpus: Option<&str>,
    ) -> Result<AuthoringKnowledge, KnowledgeError> {
        let mut composition = Composition::new(intent);
        let families = self.recall_families(intent, &mut composition);
        self.recall_shapes(intent, &families, &mut composition);
        self.recall_examples(intent, exclude_corpus, &mut composition);
        self.recall_skill(&families, &mut composition);
        composition.close();
        let mut pack = AuthoringKnowledge {
            identity: self.identity(),
            selection: composition.selection,
            references: composition.references,
            repairs: self.repair_index(),
        };
        pack.identity["door"] = json!({
            "kind": "snapshot",
            "builder": PACK_BUILDER,
            "pack_sha256": pack_sha256(&pack),
        });
        Ok(pack)
    }

    /// 1 · the families by BM25 over their need, title and facets.
    fn recall_families(&self, intent: &str, composition: &mut Composition) -> Vec<(String, f64)> {
        let mut families = rank(intent, self.rows("families"), usize::MAX, |r| {
            text_of(r, &["title", "need", "facets", "evidence"])
        });
        let available = self.rows("families").len();
        composition.count("families", available, families.len(), FAMILIES);
        families.truncate(FAMILIES);
        for (id, score) in &families {
            composition.select("families", id, &format!("bm25 {score:.2}"));
        }
        families
    }

    /// 2 · each recalled family's patterns through the graph (family RECOMMENDS pack CONTAINS
    /// pattern) and 3 · the patterns BM25 matches directly (the graph is a prior, not a prison),
    /// taken in turn — the first of each source, then the second — so relevance, never the ids'
    /// order, decides the eight; then 4 · the blocks that REALIZE each source's patterns, the one
    /// covering the most first, in turn too: a secondary obligation keeps its block beside the
    /// leading family's. Every block is presented with its row's metadata.
    fn recall_shapes(
        &self,
        intent: &str,
        families: &[(String, f64)],
        composition: &mut Composition,
    ) {
        let mut sources: Vec<(String, Vec<(String, String)>)> = families
            .iter()
            .map(|(family, _)| (family.clone(), self.family_patterns(family)))
            .collect();
        let direct = rank(intent, self.rows("patterns"), usize::MAX, |r| {
            text_of(r, &["title", "purpose", "notes"])
        });
        sources.push((
            "bm25".to_owned(),
            direct
                .into_iter()
                .map(|(id, score)| (id, format!("bm25 {score:.2}")))
                .collect(),
        ));
        let lists: Vec<Vec<(String, String)>> =
            sources.iter().map(|(_, list)| list.clone()).collect();
        let patterns = interleave(&lists);
        let available = self.rows("patterns").len();
        composition.count("patterns", available, patterns.len(), PATTERNS);
        for (pattern, why) in patterns.iter().take(PATTERNS) {
            let text = self
                .row(pattern)
                .map(|row| {
                    format!(
                        "- {} — {} {}",
                        pattern,
                        text_of(row, &["purpose"]),
                        text_of(row, &["notes"])
                    )
                })
                .ok_or_else(|| "no row in the snapshot".to_owned());
            composition.offer("patterns", "pattern", pattern, why, text);
        }
        let covering: Vec<Vec<(String, String)>> = sources
            .iter()
            .map(|(source, list)| self.covering_blocks(source, list))
            .collect();
        let blocks = interleave(&covering);
        composition.count("blocks", self.rows("blocks").len(), blocks.len(), BLOCKS);
        for (block, why) in blocks.iter().take(BLOCKS) {
            let text = self.block_text(block, composition);
            composition.offer("blocks", "block", block, why, text);
        }
    }

    /// A family's patterns through the graph (it RECOMMENDS a pack that CONTAINS them), in the
    /// relations' order, each with the path that reached it.
    fn family_patterns(&self, family: &str) -> Vec<(String, String)> {
        let mut patterns: Vec<(String, String)> = Vec::new();
        for pack in self.targets(family, "RECOMMENDS") {
            for pattern in self.targets(pack, "CONTAINS") {
                if !patterns.iter().any(|(seen, _)| seen == pattern) {
                    patterns.push((pattern.to_owned(), format!("{family} via {pack}")));
                }
            }
        }
        patterns
    }

    /// The blocks that REALIZE a source's patterns, the one covering the most of them first
    /// (among equals, the first reached), each with what it realizes and for which source.
    fn covering_blocks(
        &self,
        source: &str,
        patterns: &[(String, String)],
    ) -> Vec<(String, String)> {
        let mut cover: Vec<(String, Vec<String>)> = Vec::new();
        for (pattern, _) in patterns {
            for block in self.sources(pattern, "REALIZES") {
                match cover.iter_mut().find(|(seen, _)| seen == block) {
                    Some((_, realized)) => realized.push(pattern.clone()),
                    None => cover.push((block.to_owned(), vec![pattern.clone()])),
                }
            }
        }
        // A stable sort: among blocks covering as many patterns, the first reached stays first.
        cover.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        cover
            .into_iter()
            .map(|(block, realized)| {
                let why = format!("realizes {} · {source}", realized.join(", "));
                (block, why)
            })
            .collect()
    }

    /// A block as the seat reads it — title and purpose, the metadata that keeps it from being
    /// misused, its code — or why it cannot be presented.
    fn block_text(&self, id: &str, composition: &mut Composition) -> Result<String, String> {
        let mut omitted = Vec::new();
        let text = self.presentable(id, composition, |row, code| {
            let metadata;
            (metadata, omitted) = block_metadata(row);
            format!(
                "{} — {}\n{metadata}```yaml\n{}\n```",
                text_of(row, &["title"]),
                text_of(row, &["purpose"]),
                code.trim_end()
            )
        });
        if !omitted.is_empty() {
            composition
                .omitted
                .push(json!({"id": id, "metadata_omitted": omitted}));
        }
        text
    }

    /// 5 · the examples that read alike, never one of the case's own corpus.
    fn recall_examples(
        &self,
        intent: &str,
        exclude_corpus: Option<&str>,
        composition: &mut Composition,
    ) {
        let examples: Vec<&Value> = self
            .rows("examples")
            .iter()
            .filter(|r| {
                exclude_corpus.is_none_or(|c| r.get("corpus").and_then(Value::as_str) != Some(c))
            })
            .collect();
        let ranked = rank(intent, examples.iter().copied(), usize::MAX, |r| {
            text_of(r, &["intent", "title"])
        });
        composition.count("examples", examples.len(), ranked.len(), EXAMPLES);
        for (id, score) in ranked.into_iter().take(EXAMPLES) {
            let text = self.presentable(&id, composition, |row, text| {
                format!(
                    "intent: {}\n```yaml\n{}\n```",
                    text_of(row, &["intent"]),
                    text.trim_end()
                )
            });
            composition.offer(
                "examples",
                "example",
                &id,
                &format!("bm25 {score:.2}"),
                text,
            );
        }
    }

    /// 6 · the leading family's skill.
    fn recall_skill(&self, families: &[(String, f64)], composition: &mut Composition) {
        let skills: Vec<(String, String)> = families
            .iter()
            .filter_map(|(family, _)| {
                let skill = self
                    .rows("skills")
                    .iter()
                    .find(|s| s.get("family").and_then(Value::as_str) == Some(family))?;
                let id = skill.get("id").and_then(Value::as_str)?.to_owned();
                Some((id, format!("family {family}")))
            })
            .collect();
        composition.count("skills", self.rows("skills").len(), skills.len(), SKILLS);
        for (id, why) in skills.into_iter().take(SKILLS) {
            let text = self.presentable(&id, composition, |_, text| text);
            composition.offer("skills", "skill", &id, &why, text);
        }
    }

    /// A selected row's text as the seat reads it (`render` over the row and its file's admitted
    /// text), or why it cannot be presented: no row, no file named, a file the release lacks or
    /// whose bytes are not UTF-8.
    fn presentable(
        &self,
        id: &str,
        composition: &mut Composition,
        render: impl FnOnce(&Value, String) -> String,
    ) -> Result<String, String> {
        let Some(row) = self.row(id) else {
            return Err("no row in the snapshot".to_owned());
        };
        let Some(file) = row.get("file").and_then(Value::as_str) else {
            return Err("the row names no file".to_owned());
        };
        match self.file_text(file, composition) {
            Some(text) => Ok(render(row, text)),
            None => Err(format!("`{file}` is absent from the release or not UTF-8")),
        }
    }

    /// The repair principles by diagnostic code (`diagnostic:<CODE> --SUGGESTS_REPAIR-->
    /// repair:<ID>`), each as one line: the title and the strategy.
    fn repair_index(&self) -> BTreeMap<String, Vec<String>> {
        let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for edge in &self.relations {
            if edge.get("rel").and_then(Value::as_str) != Some("SUGGESTS_REPAIR") {
                continue;
            }
            let (Some(from), Some(to)) = (
                edge.get("from").and_then(Value::as_str),
                edge.get("to").and_then(Value::as_str),
            ) else {
                continue;
            };
            let Some(code) = from.strip_prefix("diagnostic:") else {
                continue;
            };
            let Some(row) = self.row(to) else {
                continue;
            };
            let line = format!(
                "repair principle « {} »: {}",
                text_of(row, &["title"]),
                text_of(row, &["strategy"])
            );
            let lines = index.entry(code.to_owned()).or_default();
            if lines.len() < PRINCIPLES {
                lines.push(line);
            }
        }
        index
    }
}

/// A manifest's text field, when it states one.
fn manifest_text<'a>(manifest: &'a Value, key: &str) -> Option<&'a str> {
    manifest.get(key).and_then(Value::as_str)
}

/// The digest of what a pack can present to a seat: every reference (kind · id · the sha256 of
/// its text, in the seat's order) and every repair principle by diagnostic code.
#[must_use]
pub fn pack_sha256(pack: &AuthoringKnowledge) -> String {
    let references: Vec<Value> = pack
        .references
        .iter()
        .map(|r| json!([r.kind, r.id, sha256_hex(r.text.as_bytes())]))
        .collect();
    let record = json!({"references": references, "repairs": pack.repairs});
    sha256_hex(record.to_string().as_bytes())
}

/// The pack under composition: the selection record, the references taken, the bytes left, the
/// admitted files presented and the files the release lacks.
struct Composition {
    selection: Value,
    references: Vec<KnowledgeReference>,
    budget: usize,
    verified: usize,
    absent: Vec<String>,
    /// The blocks whose metadata did not fit whole, with the fields left out.
    omitted: Vec<Value>,
}

/// The kinds a pack presents as references, by selection list.
const PRESENTED_LISTS: [&str; 4] = ["patterns", "blocks", "examples", "skills"];

impl Composition {
    fn new(intent: &str) -> Self {
        Self {
            selection: json!({
                "intent_tokens": tokens(intent).len(),
                "retriever": "bm25",
                "selector": {
                    "families": "BM25 over title, need, facets and evidence",
                    "patterns": "each recalled family's patterns (RECOMMENDS a pack that CONTAINS them), then BM25 over title, purpose and notes, taken in turn",
                    "blocks": "for each of those sources, the blocks that REALIZE its patterns, the most covering first, taken in turn",
                    "examples": "BM25 over intent and title",
                    "skills": "the leading family's",
                    "note": "this door's own selection (Rust BM25 over the Foundry graph), not the Foundry producer's",
                },
                "families": [],
                "patterns": [],
                "blocks": [],
                "examples": [],
                "skills": [],
                "receipt": {},
            }),
            references: Vec::new(),
            budget: PACK_BYTES,
            verified: 0,
            absent: Vec::new(),
            omitted: Vec::new(),
        }
    }

    /// A family the recall selected: it steers the patterns, it is never a reference itself.
    fn select(&mut self, kind: &str, id: &str, why: &str) {
        if let Some(list) = self.selection.get_mut(kind).and_then(Value::as_array_mut) {
            list.push(json!({"id": id, "why": why}));
        }
    }

    /// How many rows of a kind the snapshot holds, how many the recall ranked, and how many of
    /// those the count cap selects.
    fn count(&mut self, list: &str, available: usize, candidates: usize, cap: usize) {
        self.selection["receipt"][list] = json!({
            "available": available,
            "candidates": candidates,
            "selected": candidates.min(cap),
            "over_count": candidates.saturating_sub(cap),
        });
    }

    /// A selected reference joins the pack while the byte budget holds it; past the budget, or
    /// when its text cannot be read, it is recorded as selected and excluded with the reason —
    /// never as presented.
    fn offer(
        &mut self,
        list: &str,
        kind: &'static str,
        id: &str,
        why: &str,
        text: Result<String, String>,
    ) {
        let excluded = match text {
            Ok(text) if text.len() <= self.budget => {
                self.budget -= text.len();
                self.references.push(KnowledgeReference {
                    kind: kind.to_owned(),
                    id: id.to_owned(),
                    text,
                });
                None
            }
            Ok(text) => Some(format!(
                "pack byte cap: {} bytes, {} of {PACK_BYTES} left",
                text.len(),
                self.budget
            )),
            Err(reason) => Some(reason),
        };
        let entry = match excluded {
            None => json!({"id": id, "why": why, "presented": true}),
            Some(reason) => json!({"id": id, "why": why, "presented": false, "excluded": reason}),
        };
        if let Some(entries) = self.selection.get_mut(list).and_then(Value::as_array_mut) {
            entries.push(entry);
        }
    }

    /// The record's totals: the bytes and files presented, per kind the entries presented and
    /// excluded, and — when nothing at all was recalled — that the seat reads the card alone.
    fn close(&mut self) {
        self.selection["bytes"] = json!(PACK_BYTES - self.budget);
        self.selection["files"] = json!({
            "verified": self.verified,
            "absent": self.absent,
        });
        if !self.omitted.is_empty() {
            self.selection["metadata_omitted"] = json!(self.omitted);
        }
        for list in PRESENTED_LISTS {
            let entries = self.selection[list]
                .as_array()
                .map_or(&[][..], Vec::as_slice);
            let presented = entries
                .iter()
                .filter(|e| e["presented"] == Value::Bool(true))
                .count();
            let excluded = entries.len() - presented;
            self.selection["receipt"][list]["presented"] = json!(presented);
            self.selection["receipt"][list]["excluded"] = json!(excluded);
        }
        let recalled = ["families", "patterns", "examples"].iter().any(|list| {
            self.selection[*list]
                .as_array()
                .is_some_and(|l| !l.is_empty())
        });
        if !recalled {
            self.selection["no_match"] = json!(
                "no family, pattern or example shares a word with the request: nothing is presented, and the seat reads the card alone"
            );
        }
    }
}

/// Take ranked sources in turn — the first of each, then the second of each… — keeping each id
/// once with the reason it first arrived with: the sources' relevance order, never the ids'.
fn interleave(sources: &[Vec<(String, String)>]) -> Vec<(String, String)> {
    let mut taken: Vec<(String, String)> = Vec::new();
    let depth = sources.iter().map(Vec::len).max().unwrap_or(0);
    for at in 0..depth {
        for source in sources {
            if let Some((id, why)) = source.get(at)
                && !taken.iter().any(|(seen, _)| seen == id)
            {
                taken.push((id.clone(), why.clone()));
            }
        }
    }
    taken
}

/// The metadata of a block row that keeps a seat from misusing the block, in priority order —
/// the holes to fill (owner, note), effects, authority, capabilities, callables, known failure
/// modes, the version it was checked at — as whole lines within `METADATA_BYTES`: a field that
/// does not fit is named as omitted, never cut mid-line.
fn block_metadata(row: &Value) -> (String, Vec<&'static str>) {
    let list = |key: &str, sep: &str| -> Option<String> {
        let items: Vec<&str> = row
            .get(key)?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        (!items.is_empty()).then(|| items.join(sep))
    };
    let holes = row.get("holes").and_then(Value::as_array).map(|holes| {
        let holes: Vec<String> = holes
            .iter()
            .filter_map(|hole| {
                let name = hole.get("name")?.as_str()?;
                let owner = hole
                    .get("owner")
                    .and_then(Value::as_str)
                    .unwrap_or("unowned");
                Some(match hole.get("note").and_then(Value::as_str) {
                    Some(note) => format!("{name} ({owner}: {note})"),
                    None => format!("{name} ({owner})"),
                })
            })
            .collect();
        holes.join("; ")
    });
    let version: Vec<String> = [
        ("", "/pin/binary"),
        ("spec ", "/pin/spec_sha"),
        ("check ", "/check_receipt/verdict"),
        ("", "/status"),
        ("proof ", "/proof_level"),
    ]
    .iter()
    .filter_map(|(label, pointer)| {
        let value = row.pointer(pointer)?.as_str()?;
        let value = if *pointer == "/pin/spec_sha" {
            value.get(..12).unwrap_or(value)
        } else {
            value
        };
        Some(format!("{label}{value}"))
    })
    .collect();
    let fields = [
        ("holes", holes.filter(|h| !h.is_empty())),
        ("effects", list("effects", ", ")),
        ("authority", list("authority", ", ")),
        ("capabilities", list("interfaces", ", ")),
        ("callables", list("callables", ", ")),
        ("known failures", list("known_failure_modes", "; ")),
        (
            "version",
            (!version.is_empty()).then(|| version.join(" · ")),
        ),
    ];
    let (mut text, mut omitted) = (String::new(), Vec::new());
    for (label, value) in fields {
        let Some(value) = value else { continue };
        let line = format!("{label}: {value}\n");
        if text.len() + line.len() <= METADATA_BYTES {
            text.push_str(&line);
        } else {
            omitted.push(label);
        }
    }
    if !omitted.is_empty() {
        use std::fmt::Write as _;
        let _ = writeln!(
            text,
            "metadata omitted at {METADATA_BYTES} bytes: {}",
            omitted.join(", ")
        );
    }
    (text, omitted)
}

/// The words of a text, folded and lowercased, three letters or more.
fn tokens(text: &str) -> Vec<String> {
    crate::compile::fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(str::to_owned)
        .collect()
}

fn text_of(row: &Value, fields: &[&str]) -> String {
    fields
        .iter()
        .filter_map(|f| row.get(*f))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// BM25 over the rows' text for one query: the top `k` ids with their score, positive
/// scores only.
// The counts are rows and words of a snapshot (hundreds, thousands): far below the 2^53
// where a usize stops converting exactly.
#[allow(clippy::cast_precision_loss)]
fn rank<'a>(
    query: &str,
    rows: impl IntoIterator<Item = &'a Value>,
    k: usize,
    text: impl Fn(&Value) -> String,
) -> Vec<(String, f64)> {
    let docs: Vec<(String, Vec<String>)> = rows
        .into_iter()
        .filter_map(|r| {
            let id = r.get("id")?.as_str()?.to_owned();
            Some((id, tokens(&text(r))))
        })
        .collect();
    if docs.is_empty() {
        return Vec::new();
    }
    let n = docs.len() as f64;
    let avg = docs.iter().map(|(_, t)| t.len()).sum::<usize>() as f64 / n;
    let mut df: BTreeMap<&str, f64> = BTreeMap::new();
    for (_, terms) in &docs {
        for term in terms.iter().collect::<BTreeSet<_>>() {
            *df.entry(term.as_str()).or_insert(0.0) += 1.0;
        }
    }
    let query: Vec<String> = tokens(query);
    let (k1, b) = (1.5_f64, 0.75_f64);
    let mut scored: Vec<(String, f64)> = docs
        .iter()
        .map(|(id, terms)| {
            let len = terms.len() as f64;
            let score: f64 = query
                .iter()
                .map(|q| {
                    let tf = terms.iter().filter(|t| *t == q).count() as f64;
                    if tf == 0.0 {
                        return 0.0;
                    }
                    let d = df.get(q.as_str()).copied().unwrap_or(0.0);
                    let idf = ((n - d + 0.5) / (d + 0.5) + 1.0).ln();
                    idf * (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * len / avg))
                })
                .sum();
            (id.clone(), score)
        })
        .filter(|(_, s)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    scored
}

/// The first `max` bytes of a text on a character boundary.
fn cut(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n# … cut at {max} bytes", &text[..end])
}

/// A pack another builder composed for one intent, read strictly from a JSON file: strict JSON
/// (a key stated twice refuses) holding only `identity` (an object) · `selection` ·
/// `references` (`[{kind, id, text}]`, every row whole) · `repairs` (`{code: [strategy]}`),
/// nothing dropped or skipped; the identity gains the door's record (`door`: the file and the
/// pack's digest). No product door enters such a pack ([`KnowledgeError::PackNotAdmitted`]):
/// nothing binds it to an admitted release. Never `Ok(None)`: a pack with no reference and no
/// repair is refused.
///
/// # Errors
/// A file that cannot be read, is not strict JSON, or departs from that closed shape
/// ([`KnowledgeError::NotAPack`]).
pub fn pack_from_file(path: &Path) -> Result<Option<AuthoringKnowledge>, KnowledgeError> {
    let not = |why: String| KnowledgeError::NotAPack {
        file: path.to_path_buf(),
        why,
    };
    let text = std::fs::read_to_string(path).map_err(|e| not(e.to_string()))?;
    // A pack is no release: no value bound but its own size (the depth bound still holds).
    let value = canonical::strict_json(&text, usize::MAX)
        .map_err(|e| not(format!("not strict JSON: {e}")))?;
    let Some(object) = value.as_object() else {
        return Err(not("not a JSON object".to_owned()));
    };
    let fields = ["identity", "selection", "references", "repairs"];
    if let Some(key) = object.keys().find(|key| !fields.contains(&key.as_str())) {
        return Err(not(format!("an unknown key `{key}`")));
    }
    let references = pack_references(object.get("references")).map_err(not)?;
    let repairs = pack_repairs(object.get("repairs")).map_err(not)?;
    if references.is_empty() && repairs.is_empty() {
        return Err(not("no reference and no repair".to_owned()));
    }
    let identity = match object.get("identity") {
        Some(declared @ Value::Object(_)) => declared.clone(),
        None => json!({}),
        Some(_) => return Err(not("the identity is not an object".to_owned())),
    };
    let mut pack = AuthoringKnowledge {
        identity,
        selection: object
            .get("selection")
            .cloned()
            .unwrap_or_else(|| json!({})),
        references,
        repairs,
    };
    pack.identity["door"] = json!({
        "kind": "file",
        "path": path.display().to_string(),
        "pack_sha256": pack_sha256(&pack),
    });
    Ok(Some(pack))
}

/// A pack's references, every row exactly `{kind, id, text}` strings; none named is none.
fn pack_references(value: Option<&Value>) -> Result<Vec<KnowledgeReference>, String> {
    let rows = match value {
        None => return Ok(Vec::new()),
        Some(Value::Array(rows)) => rows,
        Some(_) => return Err("references is not a list".to_owned()),
    };
    rows.iter()
        .enumerate()
        .map(|(at, row)| {
            let field = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_owned);
            let whole = row.as_object().is_some_and(|object| object.len() == 3);
            match (whole, field("kind"), field("id"), field("text")) {
                (true, Some(kind), Some(id), Some(text)) => {
                    Ok(KnowledgeReference { kind, id, text })
                }
                _ => Err(format!(
                    "references[{at}] is not exactly {{kind, id, text}}"
                )),
            }
        })
        .collect()
}

/// A pack's repair principles, each code naming a list of strategies; none named is none.
fn pack_repairs(value: Option<&Value>) -> Result<BTreeMap<String, Vec<String>>, String> {
    let map = match value {
        None => return Ok(BTreeMap::new()),
        Some(Value::Object(map)) => map,
        Some(_) => return Err("repairs is not an object".to_owned()),
    };
    map.iter()
        .map(|(code, strategies)| {
            let items = strategies
                .as_array()
                .ok_or_else(|| format!("repairs.{code} is not a list"))?;
            let strategies: Option<Vec<String>> = items
                .iter()
                .map(|item| item.as_str().map(str::to_owned))
                .collect();
            strategies
                .map(|list| (code.clone(), list))
                .ok_or_else(|| format!("repairs.{code} holds a strategy that is not text"))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
