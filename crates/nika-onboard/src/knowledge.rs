// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The knowledge door: `--knowledge <snapshot dir>` reads a Foundry knowledge snapshot
//! (`manifest.json` · one JSONL per kind · `relations.jsonl`) and composes, for one intent,
//! the authoring pack the seat reads beside the card — the families the intent belongs to,
//! their patterns and the checked blocks that realize them, the solved examples that read
//! alike, the skill of the leading family — plus the repair principles the snapshot wires to
//! diagnostic codes, for the repair rounds. Deterministic retrieval (BM25 over the rows'
//! text and the snapshot's graph), bounded (three families · eight patterns · four blocks ·
//! three examples · one skill · ~40 KiB), and stated: the selection record names every row it
//! selected, why, and whether it was presented or excluded (and for what reason); the identity
//! carries the snapshot's version and digest and this builder's version. The order of what is
//! taken is relevance — each recalled family, then the direct text match, in turn — never the
//! rows' ids, and a block is presented with the holes, effects, capabilities, known failures and
//! version its row states. The selection is this door's own (Rust BM25 over the Foundry graph),
//! not the one the Foundry producer computes, and the record says so. Foundry
//! measured the effect of such a pack on the reference engineer (2026-09-22: 1/20 → 17/20
//! on twenty generalization intents); this door lets the compiler's own seat read it.
//!
//! Verified: a Foundry manifest pins the sha256 of every file under its knowledge root
//! (`files`, keyed by the path under that root). Every row file the door holds and every file
//! it presents is compared to that pin before a byte reaches a seat: a snapshot whose bytes are
//! not the ones its identity names (the root changed after the export, a row edited) is refused
//! as stale, never presented under that identity; a file the manifest does not pin is presented
//! as unverified and named so in the record. The pack's own digest (every reference and repair
//! principle it can present) rides the identity's `door` record. One door: `nika compile
//! --knowledge` and the session compose through this code.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::compile::{AuthoringKnowledge, KnowledgeReference};
use nika_event::source_id::sha256_hex;
use serde_json::{Value, json};

/// This builder's version, stated beside the snapshot digest (v2: every presented byte is
/// verified against the manifest's pins, and the pack states its own digest; v3: relevance
/// survives deduplication, each recalled family gets its block in turn, the receipt separates
/// selected, excluded with a reason and presented, and a block states its row's metadata).
pub const PACK_BUILDER: &str = "nika-compile/knowledge-door-v3";
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
    /// The directory holds no readable Foundry snapshot manifest.
    NotASnapshot {
        /// The directory named.
        dir: PathBuf,
        /// What was missing or unreadable.
        why: String,
    },
    /// A file the door read is not the file its manifest pins: the snapshot is stale.
    Stale {
        /// The snapshot's version, as its manifest names it.
        version: String,
        /// The file, as the manifest names it (a path under the knowledge root).
        file: String,
        /// The sha256 the manifest pins.
        expected: String,
        /// The sha256 of the bytes read.
        found: String,
    },
    /// The file is not a knowledge pack.
    NotAPack {
        /// The file named.
        file: PathBuf,
        /// Why.
        why: String,
    },
}

impl std::fmt::Display for KnowledgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotASnapshot { dir, why } => {
                write!(f, "`{}` is not a knowledge snapshot ({why})", dir.display())
            }
            Self::Stale {
                version,
                file,
                expected,
                found,
            } => write!(
                f,
                "knowledge snapshot `{version}` is stale: `{file}` reads sha256 {found:.12}…, its manifest pins {expected:.12}… — export the snapshot again, or name one its files still match"
            ),
            Self::NotAPack { file, why } => {
                write!(f, "`{}` is not a knowledge pack ({why})", file.display())
            }
        }
    }
}

impl std::error::Error for KnowledgeError {}

/// A snapshot on disk: its manifest and its pins, the rows by kind (every row file compared to
/// its pin), the relations by source id.
#[derive(Debug)]
pub struct Snapshot {
    dir: PathBuf,
    files_root: Option<PathBuf>,
    manifest: Value,
    rows: BTreeMap<String, Vec<Value>>,
    relations: Vec<Value>,
    /// The manifest's pins: a path under the knowledge root → its sha256.
    pins: BTreeMap<String, String>,
    /// Every row file held, by name: the sha256 of the bytes read, and whether a pin covered it.
    row_files: BTreeMap<String, (String, bool)>,
    /// The sha256 of the manifest's bytes as read — computed here, unlike the `digest` the
    /// manifest declares (the exporter's, never recomputed by this door).
    manifest_sha256: String,
}

impl Snapshot {
    /// Open a snapshot directory: its manifest, then every row file, each compared to the
    /// manifest's pin (`knowledge/<file>` under the knowledge root).
    ///
    /// # Errors
    /// No readable manifest ([`KnowledgeError::NotASnapshot`]), or a row file whose bytes are not
    /// the ones the manifest pins ([`KnowledgeError::Stale`]).
    pub fn open(dir: &Path) -> Result<Self, KnowledgeError> {
        let not = |why: String| KnowledgeError::NotASnapshot {
            dir: dir.to_path_buf(),
            why,
        };
        let text = std::fs::read_to_string(dir.join("manifest.json"))
            .map_err(|e| not(format!("manifest.json: {e}")))?;
        let manifest: Value = serde_json::from_str(&text)
            .map_err(|e| not(format!("manifest.json is not JSON: {e}")))?;
        if !manifest.is_object() {
            return Err(not("manifest.json is not a JSON object".to_owned()));
        }
        let pins: BTreeMap<String, String> = manifest
            .get("files")
            .and_then(Value::as_object)
            .map(|files| {
                files
                    .iter()
                    .filter_map(|(path, sha)| Some((path.clone(), sha.as_str()?.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        let version = manifest_text(&manifest, "knowledge_version").unwrap_or("unversioned");
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map_err(|e| not(e.to_string()))?
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .collect();
        names.sort();
        let mut rows = BTreeMap::new();
        let mut relations = Vec::new();
        let mut row_files = BTreeMap::new();
        for name in names {
            // The exporter's row files, one JSONL per kind (the lowercase suffix it writes).
            let Some(kind) = name.strip_suffix(".jsonl") else {
                continue;
            };
            let bytes = std::fs::read(dir.join(&name)).map_err(|e| not(format!("{name}: {e}")))?;
            let sha = sha256_hex(&bytes);
            let pinned = verify(&pins, version, &format!("knowledge/{name}"), &sha)?;
            let parsed = jsonl(&String::from_utf8_lossy(&bytes));
            if kind == "relations" {
                relations = parsed;
            } else {
                rows.insert(kind.to_owned(), parsed);
            }
            row_files.insert(name, (sha, pinned));
        }
        Ok(Self {
            files_root: files_root(dir),
            dir: dir.to_path_buf(),
            manifest,
            rows,
            relations,
            pins,
            row_files,
            manifest_sha256: sha256_hex(text.as_bytes()),
        })
    }

    /// The snapshot's version, as its manifest names it.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        manifest_text(&self.manifest, "knowledge_version")
    }

    /// The digest the manifest DECLARES (the exporter's): stated, never recomputed here — the
    /// integrity this door computes is [`Self::manifest_sha256`] and the per-file pins.
    #[must_use]
    pub fn digest(&self) -> Option<&str> {
        manifest_text(&self.manifest, "digest")
    }

    /// The sha256 of the manifest's bytes as this door read them: any change to the manifest
    /// (a re-pinned file under the same declared version and digest) changes it.
    #[must_use]
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }

    /// The digest of the row files as the door read them (each name and sha256, in name
    /// order): the identity of the rows held, pinned or not.
    #[must_use]
    pub fn rows_sha256(&self) -> String {
        use std::fmt::Write as _;
        let lines = self
            .row_files
            .iter()
            .fold(String::new(), |mut lines, (name, (sha, _))| {
                let _ = writeln!(lines, "{name} {sha}");
                lines
            });
        sha256_hex(lines.as_bytes())
    }

    /// The snapshot's identity for the provenance record: version and declared digest (the
    /// manifest's words), the manifest's and the rows' sha256 (computed here), the builder, and
    /// what the door verified.
    #[must_use]
    pub fn identity(&self) -> Value {
        let unpinned: Vec<&str> = self
            .row_files
            .iter()
            .filter(|(_, (_, pinned))| !pinned)
            .map(|(name, _)| name.as_str())
            .collect();
        json!({
            "version": self.manifest.get("knowledge_version").cloned().unwrap_or(Value::Null),
            "digest": self.manifest.get("digest").cloned().unwrap_or(Value::Null),
            "source_commit": self.manifest.get("source_commit").cloned().unwrap_or(Value::Null),
            "pack_builder": PACK_BUILDER,
            "dir": self.dir.display().to_string(),
            "manifest_sha256": self.manifest_sha256,
            "rows_sha256": self.rows_sha256(),
            "verification": {
                "digest": "declared by the manifest, not recomputed",
                "manifest_pins": self.pins.len(),
                "row_files": self.row_files.len(),
                "row_files_unpinned": unpinned,
                "files_root": self.files_root.as_ref().map(|p| p.display().to_string()),
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

    /// A referenced file's text, bounded, compared to its pin first; None when the snapshot's
    /// files root is unknown or the file is absent (the composition records the absence).
    fn file_text(
        &self,
        relative: &str,
        composition: &mut Composition,
    ) -> Result<Option<String>, KnowledgeError> {
        let Some(root) = self.files_root.as_ref() else {
            return Ok(None);
        };
        let Ok(bytes) = std::fs::read(root.join(relative)) else {
            composition.absent.push(relative.to_owned());
            return Ok(None);
        };
        let version = self.version().unwrap_or("unversioned");
        if verify(&self.pins, version, relative, &sha256_hex(&bytes))? {
            composition.verified += 1;
        } else {
            composition.unpinned.push(relative.to_owned());
        }
        Ok(String::from_utf8(bytes)
            .ok()
            .map(|text| cut(&text, FILE_BYTES)))
    }

    /// The authoring pack for one intent: the references the seat reads, the selection
    /// record, the identity with the pack's own digest. `exclude_corpus` keeps a benchmark
    /// honest: no example of the case's own corpus is recalled.
    ///
    /// # Errors
    /// A referenced file whose bytes are not the ones the manifest pins
    /// ([`KnowledgeError::Stale`]): no pack is composed from a stale snapshot.
    pub fn pack(
        &self,
        intent: &str,
        exclude_corpus: Option<&str>,
    ) -> Result<AuthoringKnowledge, KnowledgeError> {
        let mut composition = Composition::new(intent);
        let families = self.recall_families(intent, &mut composition);
        self.recall_shapes(intent, &families, &mut composition)?;
        self.recall_examples(intent, exclude_corpus, &mut composition)?;
        self.recall_skill(&families, &mut composition)?;
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
    ) -> Result<(), KnowledgeError> {
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
            let text = self.block_text(block, composition)?;
            composition.offer("blocks", "block", block, why, text);
        }
        Ok(())
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
    fn block_text(
        &self,
        id: &str,
        composition: &mut Composition,
    ) -> Result<Result<String, String>, KnowledgeError> {
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
        })?;
        if !omitted.is_empty() {
            composition
                .omitted
                .push(json!({"id": id, "metadata_omitted": omitted}));
        }
        Ok(text)
    }

    /// 5 · the examples that read alike, never one of the case's own corpus.
    fn recall_examples(
        &self,
        intent: &str,
        exclude_corpus: Option<&str>,
        composition: &mut Composition,
    ) -> Result<(), KnowledgeError> {
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
            })?;
            composition.offer(
                "examples",
                "example",
                &id,
                &format!("bm25 {score:.2}"),
                text,
            );
        }
        Ok(())
    }

    /// 6 · the leading family's skill.
    fn recall_skill(
        &self,
        families: &[(String, f64)],
        composition: &mut Composition,
    ) -> Result<(), KnowledgeError> {
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
            let text = self.presentable(&id, composition, |_, text| text)?;
            composition.offer("skills", "skill", &id, &why, text);
        }
        Ok(())
    }

    /// A selected row's text as the seat reads it (`render` over the row and its file's text), or
    /// why it cannot be presented: no row, no file named, or a file absent or unreadable.
    fn presentable(
        &self,
        id: &str,
        composition: &mut Composition,
        render: impl FnOnce(&Value, String) -> String,
    ) -> Result<Result<String, String>, KnowledgeError> {
        let Some(row) = self.row(id) else {
            return Ok(Err("no row in the snapshot".to_owned()));
        };
        let Some(file) = row.get("file").and_then(Value::as_str) else {
            return Ok(Err("the row names no file".to_owned()));
        };
        Ok(match self.file_text(file, composition)? {
            Some(text) => Ok(render(row, text)),
            None if self.files_root.is_none() => {
                Err(format!("no files root to read `{file}` from"))
            }
            None => Err(format!(
                "`{file}` is absent or not UTF-8 under the files root"
            )),
        })
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

/// Compare the bytes read at `path` to the manifest's pin: `Ok(true)` when a pin covers it and
/// matches, `Ok(false)` when no pin covers it, a stale refusal when the pin differs.
fn verify(
    pins: &BTreeMap<String, String>,
    version: &str,
    path: &str,
    found: &str,
) -> Result<bool, KnowledgeError> {
    match pins.get(path) {
        Some(expected) if expected.eq_ignore_ascii_case(found) => Ok(true),
        Some(expected) => Err(KnowledgeError::Stale {
            version: version.to_owned(),
            file: path.to_owned(),
            expected: expected.clone(),
            found: found.to_owned(),
        }),
        None => Ok(false),
    }
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
/// files verified, unpinned and absent.
struct Composition {
    selection: Value,
    references: Vec<KnowledgeReference>,
    budget: usize,
    verified: usize,
    unpinned: Vec<String>,
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
            unpinned: Vec::new(),
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
            "unpinned": self.unpinned,
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

/// The files root a snapshot's `file` fields are relative to: the first ancestor of the
/// snapshot directory that holds a `foundry/` directory with `blocks/` or `examples/`
/// inside it (a snapshot lives under a `.local/` tree beside that directory).
fn files_root(dir: &Path) -> Option<PathBuf> {
    for ancestor in dir.ancestors().skip(1).take(6) {
        let candidate = ancestor.join("foundry");
        if candidate.join("blocks").is_dir() || candidate.join("examples").is_dir() {
            return Some(candidate);
        }
    }
    None
}

fn jsonl(text: &str) -> Vec<Value> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
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

/// A pack another builder composed for one intent, read from a JSON file: `identity` and
/// `selection` verbatim, `references` as `{kind, id, text}` rows, `repairs` as diagnostic code
/// → strategies; the identity gains the door's record (`door`: the file and the pack's digest).
/// `Ok(None)` when the file holds no reference and no repair (the seat reads the card alone,
/// and the receipt says so by carrying no knowledge).
///
/// # Errors
/// A file that cannot be read, is not JSON or is not an object ([`KnowledgeError::NotAPack`]).
pub fn pack_from_file(path: &Path) -> Result<Option<AuthoringKnowledge>, KnowledgeError> {
    let not = |why: String| KnowledgeError::NotAPack {
        file: path.to_path_buf(),
        why,
    };
    let text = std::fs::read_to_string(path).map_err(|e| not(e.to_string()))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| not(format!("not JSON: {e}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| not("not a JSON object".to_owned()))?;
    let references = object
        .get("references")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    Some(KnowledgeReference {
                        kind: row.get("kind")?.as_str()?.to_owned(),
                        id: row.get("id")?.as_str()?.to_owned(),
                        text: row.get("text")?.as_str()?.to_owned(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let repairs = object
        .get("repairs")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(code, strategies)| {
                    let strategies = strategies
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default();
                    (code.clone(), strategies)
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    if references.is_empty() && repairs.is_empty() {
        return Ok(None);
    }
    // A declared identity that is not an object is kept whole beside the door's record.
    let identity = match object.get("identity") {
        Some(declared @ Value::Object(_)) => declared.clone(),
        Some(declared) => json!({ "declared": declared }),
        None => json!({}),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
