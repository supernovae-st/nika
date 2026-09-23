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
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use nika_onboard::compile::{AuthoringKnowledge, KnowledgeReference};
use serde_json::{Value, json};

/// This builder's version, stated beside the snapshot digest.
pub(super) const PACK_BUILDER: &str = "nika-compile/knowledge-door-v1";
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

/// A snapshot on disk: its manifest, the rows by kind, the relations by source id.
pub(super) struct Snapshot {
    dir: PathBuf,
    files_root: Option<PathBuf>,
    manifest: Value,
    rows: BTreeMap<String, Vec<Value>>,
    relations: Vec<Value>,
}

impl Snapshot {
    /// Open a snapshot directory; None when it carries no readable manifest.
    #[must_use]
    pub(super) fn open(dir: &Path) -> Option<Self> {
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).ok()?).ok()?;
        let mut rows = BTreeMap::new();
        for entry in std::fs::read_dir(dir).ok()?.filter_map(Result::ok) {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if let Some(kind_file) = name.strip_suffix(".jsonl")
                && kind_file != "relations"
            {
                rows.insert(kind_file.to_owned(), jsonl(&path));
            }
        }
        let relations = jsonl(&dir.join("relations.jsonl"));
        Some(Self {
            files_root: files_root(dir),
            dir: dir.to_path_buf(),
            manifest,
            rows,
            relations,
        })
    }

    /// The snapshot's identity for the provenance record: version, digest, pins, builder.
    #[must_use]
    pub(super) fn identity(&self) -> Value {
        json!({
            "version": self.manifest.get("knowledge_version").cloned().unwrap_or(Value::Null),
            "digest": self.manifest.get("digest").cloned().unwrap_or(Value::Null),
            "source_commit": self.manifest.get("source_commit").cloned().unwrap_or(Value::Null),
            "pack_builder": PACK_BUILDER,
            "dir": self.dir.display().to_string(),
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

    /// A referenced file's text, bounded, or None when the snapshot's files root is unknown
    /// or the file is absent.
    fn file_text(&self, relative: &str) -> Option<String> {
        let root = self.files_root.as_ref()?;
        let text = std::fs::read_to_string(root.join(relative)).ok()?;
        Some(cut(&text, FILE_BYTES))
    }

    /// The authoring pack for one intent: the references the seat reads, the selection
    /// record, the identity. `exclude_corpus` keeps a benchmark honest: no example of the
    /// case's own corpus is recalled.
    #[must_use]
    pub(super) fn pack(&self, intent: &str, exclude_corpus: Option<&str>) -> AuthoringKnowledge {
        let mut composition = Composition::new(intent);
        let families = self.recall_families(intent, &mut composition);
        self.recall_shapes(intent, &families, &mut composition);
        self.recall_examples(intent, exclude_corpus, &mut composition);
        self.recall_skill(&families, &mut composition);
        composition.selection["bytes"] = json!(PACK_BYTES - composition.budget);
        AuthoringKnowledge {
            identity: self.identity(),
            selection: composition.selection,
            references: composition.references,
            repairs: self.repair_index(),
        }
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
    ) {
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
            let Some((row, text)) = self.row_with_file(block) else {
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
        for (id, score) in rank(intent, examples.iter().copied(), EXAMPLES, |r| {
            text_of(r, &["intent", "title"])
        }) {
            let Some((row, text)) = self.row_with_file(&id) else {
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
    }

    /// 6 · the leading family's skill.
    fn recall_skill(&self, families: &[(String, f64)], composition: &mut Composition) {
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
            let Some((_, text)) = self.row_with_file(&id) else {
                continue;
            };
            composition.select("skills", &id, &format!("leading family {family}"));
            composition.take("skill", &id, text);
        }
    }

    /// A row and the text of the file it names, or None when either is absent.
    fn row_with_file(&self, id: &str) -> Option<(&Value, String)> {
        let row = self.row(id)?;
        let text = row
            .get("file")
            .and_then(Value::as_str)
            .and_then(|f| self.file_text(f))?;
        Some((row, text))
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

/// The pack under composition: the selection record, the references taken, the bytes left.
struct Composition {
    selection: Value,
    references: Vec<KnowledgeReference>,
    budget: usize,
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

fn jsonl(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A miniature snapshot in the bench layout: `foundry/{blocks,examples,skills}` beside
    /// `.local/foundry/snapshots/knowledge-t/`.
    fn snapshot() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let snap = root.join(".local/foundry/snapshots/knowledge-t");
        write(
            &snap.join("manifest.json"),
            r#"{"knowledge_version": "knowledge-t", "digest": "abc123", "source_commit": "deadbeef", "kinds": {"family": 2}}"#,
        );
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
        (dir, snap)
    }

    #[test]
    fn the_pack_recalls_the_family_its_patterns_blocks_examples_and_skill_and_states_why() {
        let (_dir, snap) = snapshot();
        let snapshot = Snapshot::open(&snap).expect("opens");
        assert_eq!(snapshot.identity()["version"], "knowledge-t");
        assert_eq!(snapshot.identity()["digest"], "abc123");
        assert_eq!(snapshot.identity()["pack_builder"], PACK_BUILDER);
        let pack = snapshot.pack(
            "Chaque lundi matin, envoie-moi un récapitulatif des tickets ouverts de ./tickets.json",
            Some("sealed"),
        );
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
    fn a_directory_without_a_manifest_is_no_snapshot_and_a_stranger_intent_recalls_little() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Snapshot::open(dir.path()).is_none());
        let (_dir, snap) = snapshot();
        let pack = Snapshot::open(&snap).unwrap().pack("zzz qqq", None);
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
