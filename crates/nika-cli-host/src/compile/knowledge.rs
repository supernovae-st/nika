// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The knowledge door: `--knowledge <snapshot dir>` reads a Foundry knowledge snapshot
//! (`manifest.json` · one JSONL per kind · `relations.jsonl`) and composes, for one intent,
//! the authoring pack the seat reads beside the card — the families the intent belongs to,
//! their patterns and the checked blocks that realize them, the solved examples that read
//! alike, the skill of the leading family — plus the repair principles the snapshot wires to
//! diagnostic codes, for the repair rounds. Deterministic retrieval (BM25 over the rows'
//! text), bounded (three families · eight patterns · four blocks · three examples · one
//! skill · ~40 KiB), and stated: the selection record names every row and why, the
//! identity carries the snapshot's version and digest and this builder's version. Foundry
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

use nika_event::source_id::sha256_hex;
use nika_onboard::compile::{AuthoringKnowledge, KnowledgeReference};
use serde_json::{Value, json};

/// This builder's version, stated beside the snapshot digest (v2: every presented byte is
/// verified against the manifest's pins, and the pack states its own digest).
pub const PACK_BUILDER: &str = "nika-compile/knowledge-door-v2";
const FAMILIES: usize = 3;
const PATTERNS: usize = 8;
const BLOCKS: usize = 4;
const EXAMPLES: usize = 3;
const SKILLS: usize = 1;
/// The most bytes one referenced file contributes.
const FILE_BYTES: usize = 6 * 1024;
/// The most bytes the whole pack contributes.
const PACK_BYTES: usize = 40 * 1024;
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
        composition.selection["bytes"] = json!(PACK_BYTES - composition.budget);
        composition.selection["files"] = json!({
            "verified": composition.verified,
            "unpinned": composition.unpinned,
            "absent": composition.absent,
        });
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
        let families = rank(intent, self.rows("families"), FAMILIES, |r| {
            text_of(r, &["title", "need", "facets", "evidence"])
        });
        for (id, score) in &families {
            composition.select("families", id, &format!("bm25 {score:.2}"));
        }
        families
    }

    /// 2 · the graph (family RECOMMENDS pack CONTAINS pattern) and 3 · the patterns by BM25
    /// too (the graph is a prior, not a prison); then 4 · the blocks that REALIZE them, as
    /// checked shapes with their text from the files root.
    fn recall_shapes(
        &self,
        intent: &str,
        families: &[(String, f64)],
        composition: &mut Composition,
    ) -> Result<(), KnowledgeError> {
        let mut patterns: BTreeMap<String, String> = BTreeMap::new();
        for (family, _) in families {
            for pack in self.targets(family, "RECOMMENDS") {
                for pattern in self.targets(pack, "CONTAINS") {
                    patterns
                        .entry(pattern.to_owned())
                        .or_insert_with(|| format!("{family} via {pack}"));
                }
            }
        }
        for (id, score) in rank(intent, self.rows("patterns"), PATTERNS, |r| {
            text_of(r, &["title", "purpose", "notes"])
        }) {
            patterns
                .entry(id)
                .or_insert_with(|| format!("bm25 {score:.2}"));
        }
        let mut blocks: BTreeMap<String, String> = BTreeMap::new();
        for (pattern, why) in patterns.iter().take(PATTERNS) {
            composition.select("patterns", pattern, why);
            if let Some(row) = self.row(pattern) {
                let line = format!(
                    "- {} — {} {}",
                    pattern,
                    text_of(row, &["purpose"]),
                    text_of(row, &["notes"])
                );
                composition.take("pattern", pattern, line);
            }
            for block in self.sources(pattern, "REALIZES") {
                blocks
                    .entry(block.to_owned())
                    .or_insert_with(|| format!("realizes {pattern}"));
            }
        }
        for (block, why) in blocks.iter().take(BLOCKS) {
            let Some((row, text)) = self.row_with_file(block, composition)? else {
                continue;
            };
            composition.select("blocks", block, why);
            let text = format!(
                "{} — {}\n```yaml\n{}\n```",
                text_of(row, &["title"]),
                text_of(row, &["purpose"]),
                text.trim_end()
            );
            composition.take("block", block, text);
        }
        Ok(())
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
        for (id, score) in rank(intent, examples.iter().copied(), EXAMPLES, |r| {
            text_of(r, &["intent", "title"])
        }) {
            let Some((row, text)) = self.row_with_file(&id, composition)? else {
                continue;
            };
            composition.select("examples", &id, &format!("bm25 {score:.2}"));
            let text = format!(
                "intent: {}\n```yaml\n{}\n```",
                text_of(row, &["intent"]),
                text.trim_end()
            );
            composition.take("example", &id, text);
        }
        Ok(())
    }

    /// 6 · the leading family's skill.
    fn recall_skill(
        &self,
        families: &[(String, f64)],
        composition: &mut Composition,
    ) -> Result<(), KnowledgeError> {
        for (family, _) in families.iter().take(SKILLS) {
            let Some(skill) = self
                .rows("skills")
                .iter()
                .find(|s| s.get("family").and_then(Value::as_str) == Some(family))
            else {
                continue;
            };
            let id = skill
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let Some((_, text)) = self.row_with_file(&id, composition)? else {
                continue;
            };
            composition.select("skills", &id, &format!("leading family {family}"));
            composition.take("skill", &id, text);
        }
        Ok(())
    }

    /// A row and the text of the file it names, or None when either is absent.
    fn row_with_file(
        &self,
        id: &str,
        composition: &mut Composition,
    ) -> Result<Option<(&Value, String)>, KnowledgeError> {
        let Some(row) = self.row(id) else {
            return Ok(None);
        };
        let Some(file) = row.get("file").and_then(Value::as_str) else {
            return Ok(None);
        };
        Ok(self.file_text(file, composition)?.map(|text| (row, text)))
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
}

impl Composition {
    fn new(intent: &str) -> Self {
        Self {
            selection: json!({
                "intent_tokens": tokens(intent).len(),
                "retriever": "bm25",
                "families": [],
                "patterns": [],
                "blocks": [],
                "examples": [],
                "skills": [],
            }),
            references: Vec::new(),
            budget: PACK_BYTES,
            verified: 0,
            unpinned: Vec::new(),
            absent: Vec::new(),
        }
    }

    fn select(&mut self, kind: &str, id: &str, why: &str) {
        if let Some(list) = self.selection.get_mut(kind).and_then(Value::as_array_mut) {
            list.push(json!({"id": id, "why": why}));
        }
    }

    /// A reference joins the pack while the budget holds it; past the budget it is left out.
    fn take(&mut self, kind: &'static str, id: &str, text: String) {
        if text.len() > self.budget {
            return;
        }
        self.budget -= text.len();
        self.references.push(KnowledgeReference {
            kind: kind.to_owned(),
            id: id.to_owned(),
            text,
        });
    }
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
    nika_onboard::compile::fold(text)
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
mod tests {
    use super::*;

    #[test]
    fn a_pack_file_enters_the_door_as_composed_and_a_non_pack_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pack.json");
        std::fs::write(
            &path,
            json!({
                "identity": {"version": "knowledge-v12", "digest": "abc", "pack_builder": "foundry/v13"},
                "selection": {"families": ["family:triage"]},
                "references": [
                    {"kind": "pattern", "id": "pattern:classify-and-route", "text": "classify, then route"},
                    {"kind": "block", "id": "block:x", "text": 7}
                ],
                "repairs": {"NIKA-SEC-004": ["ask the endpoint as const.<system>_endpoint"], "NIKA-X": "not a list"}
            })
            .to_string(),
        )
        .unwrap();
        let pack = pack_from_file(&path).unwrap().unwrap();
        assert_eq!(pack.identity["pack_builder"], "foundry/v13");
        assert_eq!(pack.identity["door"]["kind"], "file");
        assert_eq!(
            pack.identity["door"]["pack_sha256"].as_str().map(str::len),
            Some(64),
            "the door states the digest of what the pack can present"
        );
        assert_eq!(pack.selection["families"][0], "family:triage");
        assert_eq!(
            pack.references.len(),
            1,
            "a row without a text is not a reference"
        );
        assert_eq!(pack.references[0].id, "pattern:classify-and-route");
        assert_eq!(
            pack.repairs["NIKA-SEC-004"][0],
            "ask the endpoint as const.<system>_endpoint"
        );
        assert!(pack.repairs["NIKA-X"].is_empty());
        std::fs::write(&path, "{\"identity\": {}}").unwrap();
        assert_eq!(
            pack_from_file(&path).unwrap(),
            None,
            "an empty pack carries no knowledge"
        );
        std::fs::write(&path, "not json").unwrap();
        assert!(matches!(
            pack_from_file(&path),
            Err(KnowledgeError::NotAPack { .. })
        ));
        assert!(matches!(
            pack_from_file(&dir.path().join("absent.json")),
            Err(KnowledgeError::NotAPack { .. })
        ));
        // A declared identity that is not an object is kept, never a panic.
        std::fs::write(
            &path,
            json!({"identity": "v13", "references": [{"kind": "pattern", "id": "p", "text": "t"}]})
                .to_string(),
        )
        .unwrap();
        let pack = pack_from_file(&path).unwrap().unwrap();
        assert_eq!(pack.identity["declared"], "v13");
        assert_eq!(pack.identity["door"]["kind"], "file");
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// The row files of the miniature snapshot, by name.
    const ROW_FILES: [&str; 8] = [
        "families.jsonl",
        "pattern_packs.jsonl",
        "patterns.jsonl",
        "blocks.jsonl",
        "examples.jsonl",
        "skills.jsonl",
        "repair_principles.jsonl",
        "relations.jsonl",
    ];

    /// The files under the miniature knowledge root the rows name.
    const ROOT_FILES: [&str; 4] = [
        "blocks/digest.nika",
        "examples/tickets-digest/workflow.nika",
        "examples/sealed/workflow.nika",
        "skills/scheduled-digest/SKILL.md",
    ];

    /// The manifest the Foundry exporter writes: every row file pinned under `knowledge/`,
    /// every file of the root pinned under its own path.
    fn pin_manifest(root: &Path, snap: &Path) {
        let mut files = serde_json::Map::new();
        for name in ROW_FILES {
            let bytes = std::fs::read(snap.join(name)).unwrap();
            files.insert(format!("knowledge/{name}"), json!(sha256_hex(&bytes)));
        }
        for relative in ROOT_FILES {
            let bytes = std::fs::read(root.join("foundry").join(relative)).unwrap();
            files.insert(relative.to_owned(), json!(sha256_hex(&bytes)));
        }
        write(
            &snap.join("manifest.json"),
            &json!({
                "knowledge_version": "knowledge-t",
                "digest": "abc123",
                "source_commit": "deadbeef",
                "kinds": {"family": 2},
                "files": files,
            })
            .to_string(),
        );
    }

    /// A miniature snapshot in the bench layout: `foundry/{blocks,examples,skills}` beside
    /// `.local/foundry/snapshots/knowledge-t/`, its manifest pinning every file.
    fn snapshot() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let snap = root.join(".local/foundry/snapshots/knowledge-t");
        write(
            &snap.join("families.jsonl"),
            concat!(
                r#"{"id": "family:scheduled-digest", "kind": "family", "title": "Scheduled digest", "need": "Every cadence, gather tickets from a source, summarize, deliver to a channel"}"#,
                "\n",
                r#"{"id": "family:csv-report", "kind": "family", "title": "CSV report", "need": "Read a CSV, keep rows, total amounts, write a report"}"#,
                "\n",
            ),
        );
        write(
            &snap.join("pattern_packs.jsonl"),
            r#"{"id": "pack:digest", "kind": "pattern_pack", "members": ["pattern:summarize"]}"#,
        );
        write(
            &snap.join("patterns.jsonl"),
            concat!(
                r#"{"id": "pattern:summarize", "kind": "pattern", "title": "Summarize", "purpose": "One infer over the gathered text.", "notes": "state max_tokens"}"#,
                "\n",
                r#"{"id": "pattern:parse-csv-records", "kind": "pattern", "title": "Parse CSV", "purpose": "Rows from a CSV file.", "notes": "nika:convert"}"#,
                "\n",
            ),
        );
        write(
            &snap.join("blocks.jsonl"),
            r#"{"id": "block:digest", "kind": "block", "title": "Digest block", "purpose": "read, summarize, notify", "file": "blocks/digest.nika"}"#,
        );
        write(
            &snap.join("examples.jsonl"),
            concat!(
                r#"{"id": "example:tickets-digest", "kind": "example", "corpus": "dev", "intent": "Every Monday, summarize the open tickets from ./tickets.json and send it", "file": "examples/tickets-digest/workflow.nika"}"#,
                "\n",
                r#"{"id": "example:sealed-digest", "kind": "example", "corpus": "sealed", "intent": "Each week summarize tickets and send", "file": "examples/sealed/workflow.nika"}"#,
                "\n",
            ),
        );
        write(
            &snap.join("skills.jsonl"),
            r#"{"id": "skill:scheduled-digest", "kind": "skill", "family": "family:scheduled-digest", "file": "skills/scheduled-digest/SKILL.md"}"#,
        );
        write(
            &snap.join("repair_principles.jsonl"),
            r#"{"id": "repair:TASKS_AS_LIST", "kind": "repair_principle", "title": "tasks is a map", "strategy": "rewrite the list as a map keyed by id"}"#,
        );
        write(
            &snap.join("relations.jsonl"),
            concat!(
                r#"{"from": "family:scheduled-digest", "rel": "RECOMMENDS", "to": "pack:digest"}"#,
                "\n",
                r#"{"from": "pack:digest", "rel": "CONTAINS", "to": "pattern:summarize"}"#,
                "\n",
                r#"{"from": "block:digest", "rel": "REALIZES", "to": "pattern:summarize"}"#,
                "\n",
                r#"{"from": "diagnostic:NIKA-PARSE-022", "rel": "SUGGESTS_REPAIR", "to": "repair:TASKS_AS_LIST"}"#,
                "\n",
            ),
        );
        write(
            &root.join("foundry/blocks/digest.nika"),
            "nika: digest\ntasks: {}\n",
        );
        write(
            &root.join("foundry/examples/tickets-digest/workflow.nika"),
            "nika: tickets-digest\ntasks: {}\n",
        );
        write(
            &root.join("foundry/examples/sealed/workflow.nika"),
            "nika: sealed\ntasks: {}\n",
        );
        write(
            &root.join("foundry/skills/scheduled-digest/SKILL.md"),
            "# Scheduled digest\nWhen: a cadence and a channel.\n",
        );
        pin_manifest(root, &snap);
        (dir, snap)
    }

    const DIGEST_INTENT: &str =
        "Chaque lundi matin, envoie-moi un récapitulatif des tickets ouverts de ./tickets.json";

    #[test]
    fn the_pack_recalls_the_family_its_patterns_blocks_examples_and_skill_and_states_why() {
        let (_dir, snap) = snapshot();
        let snapshot = Snapshot::open(&snap).expect("opens");
        assert_eq!(snapshot.identity()["version"], "knowledge-t");
        assert_eq!(snapshot.identity()["digest"], "abc123");
        assert_eq!(snapshot.identity()["pack_builder"], PACK_BUILDER);
        assert_eq!(snapshot.version(), Some("knowledge-t"));
        assert_eq!(snapshot.digest(), Some("abc123"));
        let pack = snapshot.pack(DIGEST_INTENT, Some("sealed")).expect("pack");
        let kinds: Vec<(&str, &str)> = pack
            .references
            .iter()
            .map(|r| (r.kind.as_str(), r.id.as_str()))
            .collect();
        assert!(
            kinds.contains(&("pattern", "pattern:summarize")),
            "{kinds:?}"
        );
        assert!(kinds.contains(&("block", "block:digest")), "{kinds:?}");
        assert!(
            kinds.contains(&("example", "example:tickets-digest")),
            "{kinds:?}"
        );
        assert!(
            !kinds.iter().any(|(_, id)| *id == "example:sealed-digest"),
            "the case's own corpus is never recalled: {kinds:?}"
        );
        assert!(
            kinds.contains(&("skill", "skill:scheduled-digest")),
            "{kinds:?}"
        );
        let block = pack.references.iter().find(|r| r.kind == "block").unwrap();
        assert!(block.text.contains("nika: digest"), "{}", block.text);
        assert_eq!(
            pack.selection["families"][0]["id"],
            "family:scheduled-digest"
        );
        assert!(
            pack.selection["blocks"][0]["why"]
                .as_str()
                .unwrap()
                .contains("realizes pattern:summarize")
        );
        assert!(pack.selection["bytes"].as_u64().unwrap() > 0);
        assert_eq!(
            pack.repairs.get("NIKA-PARSE-022").map(Vec::len),
            Some(1),
            "{:?}",
            pack.repairs
        );
    }

    #[test]
    fn every_presented_byte_is_the_snapshots_and_the_pack_states_its_digest() {
        let (_dir, snap) = snapshot();
        let snapshot = Snapshot::open(&snap).expect("opens");
        let identity = snapshot.identity();
        assert_eq!(identity["verification"]["row_files"], 8);
        assert_eq!(
            identity["verification"]["row_files_unpinned"],
            json!([]),
            "every row file is pinned and matched"
        );
        assert_eq!(identity["rows_sha256"], json!(snapshot.rows_sha256()));
        assert_eq!(
            identity["manifest_sha256"].as_str().map(str::len),
            Some(64),
            "the manifest's own bytes, computed"
        );
        assert_eq!(
            identity["verification"]["digest"],
            "declared by the manifest, not recomputed"
        );
        let pack = snapshot.pack(DIGEST_INTENT, Some("sealed")).expect("pack");
        assert_eq!(
            pack.selection["files"]["verified"], 3,
            "the block, the example and the skill were compared to their pins: {}",
            pack.selection["files"]
        );
        assert_eq!(pack.selection["files"]["unpinned"], json!([]));
        let digest = pack.identity["door"]["pack_sha256"].as_str().unwrap();
        assert_eq!(digest, pack_sha256(&pack));
        // The same snapshot and intent compose the same pack, byte for byte.
        let again = snapshot.pack(DIGEST_INTENT, Some("sealed")).expect("pack");
        assert_eq!(again, pack);
        // Another intent presents other bytes, and says so.
        let other = snapshot
            .pack("Read ./a.csv, keep rows, total amounts", None)
            .expect("pack");
        assert_ne!(other.identity["door"]["pack_sha256"], json!(digest));
    }

    #[test]
    fn a_row_file_edited_after_the_export_is_refused_as_stale() {
        let (_dir, snap) = snapshot();
        write(
            &snap.join("patterns.jsonl"),
            r#"{"id": "pattern:summarize", "kind": "pattern", "title": "Summarize", "purpose": "An edited purpose."}"#,
        );
        match Snapshot::open(&snap) {
            Err(KnowledgeError::Stale { version, file, .. }) => {
                assert_eq!(version, "knowledge-t");
                assert_eq!(file, "knowledge/patterns.jsonl");
            }
            other => panic!("an edited row file is stale: {other:?}"),
        }
    }

    #[test]
    fn a_presented_file_changed_in_the_root_is_refused_as_stale_never_presented() {
        let (dir, snap) = snapshot();
        let before = Snapshot::open(&snap).expect("opens");
        write(
            &dir.path().join("foundry/blocks/digest.nika"),
            "nika: digest-edited-after-export\ntasks: {}\n",
        );
        // Re-pinned by a new export under the SAME declared version and digest, the snapshot is
        // consistent again — and only the manifest's own bytes say it is not the same snapshot.
        pin_manifest(dir.path(), &snap);
        let repinned = Snapshot::open(&snap).expect("consistent");
        assert!(repinned.pack(DIGEST_INTENT, Some("sealed")).is_ok());
        assert_eq!(
            (repinned.version(), repinned.digest()),
            (before.version(), before.digest())
        );
        assert_eq!(repinned.rows_sha256(), before.rows_sha256());
        assert_ne!(repinned.manifest_sha256(), before.manifest_sha256());
        // Back to a stale snapshot: the old manifest, the edited block.
        std::fs::write(
            snap.join("manifest.json"),
            serde_json::to_string(&before.manifest).unwrap(),
        )
        .unwrap();
        let snapshot = Snapshot::open(&snap).expect("the rows still match");
        let error = snapshot
            .pack(DIGEST_INTENT, Some("sealed"))
            .expect_err("the block's bytes are not the snapshot's");
        match &error {
            KnowledgeError::Stale { file, .. } => assert_eq!(file, "blocks/digest.nika"),
            other => panic!("stale: {other:?}"),
        }
        assert!(error.to_string().contains("is stale"), "{error}");
    }

    #[test]
    fn a_manifest_that_pins_nothing_is_presented_as_unverified_and_says_so() {
        let (_dir, snap) = snapshot();
        write(
            &snap.join("manifest.json"),
            r#"{"knowledge_version": "hand-made", "digest": "d"}"#,
        );
        let snapshot = Snapshot::open(&snap).expect("opens");
        assert_eq!(
            snapshot.identity()["verification"]["row_files_unpinned"]
                .as_array()
                .map(Vec::len),
            Some(8)
        );
        let pack = snapshot.pack(DIGEST_INTENT, Some("sealed")).expect("pack");
        assert_eq!(pack.selection["files"]["verified"], 0);
        assert_eq!(
            pack.selection["files"]["unpinned"].as_array().map(Vec::len),
            Some(3),
            "{}",
            pack.selection["files"]
        );
    }

    #[test]
    fn a_directory_without_a_manifest_is_no_snapshot_and_a_stranger_intent_recalls_little() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            Snapshot::open(dir.path()),
            Err(KnowledgeError::NotASnapshot { .. })
        ));
        write(&dir.path().join("manifest.json"), "[1, 2]");
        let error = Snapshot::open(dir.path()).unwrap_err();
        assert!(error.to_string().contains("not a JSON object"), "{error}");
        let (_dir, snap) = snapshot();
        let pack = Snapshot::open(&snap)
            .unwrap()
            .pack("zzz qqq", None)
            .unwrap();
        assert!(
            pack.references.iter().all(|r| r.kind != "example"),
            "{:?}",
            pack.selection
        );
    }

    #[test]
    fn bm25_ranks_the_row_that_shares_the_rare_words_first() {
        let rows = vec![
            json!({"id": "a", "t": "read a csv file and total the amounts"}),
            json!({"id": "b", "t": "summarize open tickets every monday"}),
            json!({"id": "c", "t": "fetch a page"}),
        ];
        let ranked = rank("summarize tickets", &rows, 2, |r| text_of(r, &["t"]));
        assert_eq!(ranked[0].0, "b", "{ranked:?}");
        assert_eq!(ranked.len(), 1, "no shared word, no hit: {ranked:?}");
        // A shared small word is a hit too (« the »), ranked below the rare words.
        let ranked = rank("summarize the tickets", &rows, 3, |r| text_of(r, &["t"]));
        assert_eq!(ranked.len(), 2, "{ranked:?}");
        assert_eq!(ranked[0].0, "b");
        assert_eq!(ranked[1].0, "a");
        assert_eq!(cut("héllo wörld", 6).lines().next(), Some("héllo"));
    }
}
