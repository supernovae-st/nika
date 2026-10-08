// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The document reading over every program the pack ships, plus the rich
//! fixtures beside this file: each imports whole with its exact bytes, and
//! every node takes an edit in place whose meaning and bytes are proven
//! separately. The corpus is the owners' (pack examples and templates);
//! nothing here chooses which fields matter.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]

use std::collections::BTreeMap;

use nika_schema::document::{Document, Edit, Node, NodeKind, Path, Refusal};
use nika_schema::{FileId, ParseMode};
use serde_json::{Value, json};

/// One program of the corpus.
struct Program {
    name: String,
    source: String,
}

/// Every pack example and template, then every fixture of `document_fixtures/`.
fn corpus() -> Vec<Program> {
    let mut out = Vec::new();
    for slug in nika_pack::example_slugs() {
        if let Some(source) = nika_pack::example(&slug) {
            out.push(Program {
                name: format!("example/{slug}"),
                source: source.to_owned(),
            });
        }
    }
    for name in nika_pack::template_names() {
        if let Some(source) = nika_pack::template(&name) {
            out.push(Program {
                name: format!("template/{name}"),
                source: source.to_owned(),
            });
        }
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/document_fixtures");
    let mut fixtures: Vec<_> = std::fs::read_dir(&dir)
        .expect("the fixtures directory")
        .map(|entry| entry.expect("a fixture").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "nika"))
        .collect();
    fixtures.sort();
    for path in fixtures {
        out.push(Program {
            name: format!("fixture/{}", path.file_name().unwrap().to_string_lossy()),
            source: std::fs::read_to_string(&path).expect("a fixture body"),
        });
    }
    out
}

/// The programs the strict parser accepts, imported.
fn imported() -> Vec<(String, Document)> {
    let mut out = Vec::new();
    for program in corpus() {
        if let Err(error) = nika_schema::parse(&program.source, FileId::new(0), ParseMode::Strict) {
            // A pack program outside the strict language is not this reading's
            // to import; a fixture written for it must be in the language.
            assert!(
                !program.name.starts_with("fixture/"),
                "{}: {error}",
                program.name
            );
            continue;
        }
        let document = Document::parse(program.source.clone()).unwrap_or_else(|refusal| {
            panic!("{}: a valid program imports: {refusal}", program.name)
        });
        out.push((program.name, document));
    }
    out
}

/// A different literal of the same JSON type (an omitted value gets a string).
fn mutated(value: &Value) -> Option<Value> {
    match value {
        Value::Number(n) => n
            .as_i64()
            .map(|i| json!(i.saturating_add(1)))
            .or_else(|| n.as_f64().map(|f| json!(f + 0.25))),
        Value::Bool(b) => Some(json!(!b)),
        Value::String(text) => Some(json!(format!("{text}x"))),
        Value::Null => Some(json!("probe")),
        Value::Array(_) | Value::Object(_) => None,
    }
}

/// How an edit ended, by refusal kind (`applied` when kept).
fn outcome(result: &Result<nika_schema::document::Applied, Refusal>) -> &'static str {
    match result {
        Ok(_) => "applied",
        Err(refusal) => refusal.kind(),
    }
}

/// The projection a kept edit must leave: the base with `path` set to `value`.
fn with(base: &Value, path: &Path, value: &Value) -> Value {
    let mut out = base.clone();
    *out.pointer_mut(&path.to_pointer())
        .expect("an existing node") = value.clone();
    out
}

#[test]
fn every_valid_program_imports_whole_with_its_exact_bytes() {
    let documents = imported();
    assert!(
        documents.len() >= 40,
        "the corpus is the pack: {}",
        documents.len()
    );
    let mut unplaced = Vec::new();
    for (name, document) in &documents {
        let identity = document.apply(&[]).expect("an empty revision");
        assert_eq!(identity.document().source(), document.source(), "{name}");
        for node in document.nodes() {
            match &node.span {
                Some(span) => assert!(document.source().get(span.clone()).is_some(), "{name}"),
                None => unplaced.push(format!("{name} {}", node.path)),
            }
        }
    }
    assert!(unplaced.is_empty(), "every node is placed: {unplaced:#?}");
}

#[test]
fn every_scalar_leaf_takes_a_new_value_in_place() {
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    let mut wrong = Vec::new();
    for (name, document) in imported() {
        for node in document.nodes() {
            if node.path.is_root() || matches!(node.kind, NodeKind::Mapping | NodeKind::Sequence) {
                continue;
            }
            let Some(value) = mutated(&node.value) else {
                continue;
            };
            let result = document.apply(&[Edit::set(node.path.clone(), value.clone())]);
            let kind = outcome(&result);
            *tally
                .entry(format!("{kind} {}", node.style.word()))
                .or_default() += 1;
            match &result {
                Ok(applied) => {
                    let revised = applied.document();
                    if !applied.bytes_preserved(document.source()) {
                        wrong.push(format!(
                            "{name} {}: bytes outside the splice moved",
                            node.path
                        ));
                    }
                    if revised.literal() != &with(document.literal(), &node.path, &value) {
                        wrong.push(format!(
                            "{name} {}: meaning moved beyond the edit",
                            node.path
                        ));
                    }
                }
                Err(Refusal::Language { .. }) => {}
                Err(refusal) => wrong.push(format!("{name} {}: {refusal}", node.path)),
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}\n{tally:#?}");
    assert!(
        tally.keys().any(|k| k.starts_with("applied literal")),
        "{tally:#?}"
    );
}

#[test]
fn every_collection_takes_an_entry_and_every_entry_can_go() {
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    let mut wrong = Vec::new();
    for (name, document) in imported() {
        for node in document.nodes() {
            let mut edits = Vec::new();
            match &node.value {
                Value::Object(_) => {
                    edits.push(Edit::insert(node.path.clone(), "zz_probe", json!("probe")));
                }
                Value::Array(items) => {
                    if let Some(last) = items.last() {
                        edits.push(Edit::push(node.path.clone(), last.clone()));
                    }
                }
                _ => {}
            }
            if !node.path.is_root() {
                edits.push(Edit::remove(node.path.clone()));
            }
            for edit in edits {
                let result = document.apply(std::slice::from_ref(&edit));
                *tally
                    .entry(format!(
                        "{} {} {}",
                        edit.op(),
                        outcome(&result),
                        node.style.word()
                    ))
                    .or_default() += 1;
                match &result {
                    Ok(applied) if !applied.bytes_preserved(document.source()) => {
                        wrong.push(format!("{name} {}: bytes moved", node.path));
                    }
                    Ok(_) | Err(Refusal::Language { .. }) => {}
                    Err(Refusal::Layout { detail, .. }) if detail.contains("shares its line") => {
                        // A compact `- key: value` item's first entry: kept, never rewritten.
                    }
                    Err(refusal) => {
                        wrong.push(format!("{name} {} {}: {refusal}", edit.op(), node.path));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}\n{tally:#?}");
}

/// A task's value bytes, dedented to the column its first key stands at.
fn task_text(document: &Document, node: &Node) -> String {
    let span = node.span.clone().expect("a placed task");
    let source = document.source();
    let line = source[..span.start].rfind('\n').map_or(0, |n| n + 1);
    let column = span.start - line;
    source[span]
        .split('\n')
        .enumerate()
        .map(|(i, l)| {
            if i == 0 || l.len() < column {
                l.trim_start().to_owned()
            } else {
                l[column..].to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_task_survives_removal_and_exact_reinsertion() {
    let mut moved = 0;
    for (name, document) in imported() {
        let tasks: Vec<Node> = document
            .nodes()
            .into_iter()
            .filter(|n| n.path.segments().len() == 2 && n.path.segments()[0] == "tasks")
            .collect();
        if tasks.len() < 2 {
            continue;
        }
        for task in tasks {
            let id = task.path.last().expect("a task id").to_owned();
            let text = task_text(&document, &task);
            let applied = document
                .apply(&[
                    Edit::remove(task.path.clone()),
                    Edit::insert_text(Path::new(["tasks"]), id.clone(), text),
                ])
                .unwrap_or_else(|refusal| panic!("{name} {id}: {refusal}"));
            assert_eq!(
                applied.document().literal(),
                document.literal(),
                "{name} {id}"
            );
            assert!(applied.bytes_preserved(document.source()), "{name} {id}");
            moved += 1;
        }
    }
    assert!(moved >= 50, "tasks moved: {moved}");
}

/// A seeded linear congruential sequence: the same chains on every run.
struct Seeded(u64);

impl Seeded {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.0 >> 33).unwrap_or(0) % n.max(1)
    }
}

/// One edit chosen on `document`'s current nodes: a new leaf value, a new
/// entry or item, or a removal.
fn some_edit(document: &Document, seed: &mut Seeded) -> Option<Edit> {
    let nodes: Vec<Node> = document
        .nodes()
        .into_iter()
        .filter(|n| !n.path.is_root())
        .collect();
    let node = nodes.get(seed.below(nodes.len()))?;
    Some(match (&node.value, seed.below(3)) {
        (Value::Object(_), 0) => Edit::insert(
            node.path.clone(),
            format!("zz_{}", seed.below(1000)),
            json!("probe"),
        ),
        (Value::Array(items), 0) => Edit::push(
            node.path.clone(),
            items.last().cloned().unwrap_or(json!("x")),
        ),
        (_, 1) => Edit::remove(node.path.clone()),
        (value, _) => Edit::set(node.path.clone(), mutated(value)?),
    })
}

#[test]
fn chains_of_edits_keep_every_law_from_link_to_link() {
    let mut seed = Seeded(0x6e69_6b61);
    let mut kept = 0;
    for (name, document) in imported() {
        let mut current = document.clone();
        let mut edits = Vec::new();
        for _ in 0..12 {
            let Some(edit) = some_edit(&current, &mut seed) else {
                continue;
            };
            match current.apply(std::slice::from_ref(&edit)) {
                Ok(applied) => {
                    current = applied.into_document();
                    edits.push(edit);
                }
                Err(Refusal::Language { .. }) => {}
                Err(Refusal::Layout { detail, .. }) if detail.contains("shares its line") => {}
                Err(refusal) => panic!(
                    "{name} after {} edits, {}: {refusal}",
                    edits.len(),
                    edit.op()
                ),
            }
        }
        // The kept edits replayed in one revision reach the same bytes.
        let replayed = document
            .apply(&edits)
            .unwrap_or_else(|refusal| panic!("{name}: the chain replays: {refusal}"));
        assert_eq!(replayed.document().source(), current.source(), "{name}");
        assert!(replayed.bytes_preserved(document.source()), "{name}");
        kept += edits.len();
    }
    assert!(kept >= 300, "edits kept across the corpus: {kept}");
}

/// Whether any `${{ }}` island of `value` still reads `tasks.<id>`.
fn reads_task(value: &Value, id: &str) -> bool {
    match value {
        Value::String(text) => nika_schema::expression::scan_templates(text)
            .unwrap_or_default()
            .iter()
            .any(|island| {
                nika_schema::expression::expr_refs(&island.expr).iter().any(|r| {
                    matches!(r, nika_schema::expression::NamespaceRef::Tasks { id: other, .. } if other == id)
                })
            }),
        Value::Array(items) => items.iter().any(|v| reads_task(v, id)),
        Value::Object(map) => map.values().any(|v| reads_task(v, id)),
        _ => false,
    }
}

#[test]
fn every_task_is_renamed_with_every_reference_it_owns() {
    let mut renamed = 0;
    for (name, document) in imported() {
        let ids: Vec<String> = document
            .workflow()
            .tasks
            .iter()
            .map(|t| t.value.id.value.clone())
            .collect();
        for id in ids {
            let to = format!("{id}_r");
            let applied = document
                .apply(&[Edit::rename(Path::new(["tasks", id.as_str()]), to.clone())])
                .unwrap_or_else(|refusal| panic!("{name} {id}: {refusal}"));
            let revised = applied.document();
            assert!(applied.bytes_preserved(document.source()), "{name} {id}");
            assert!(
                !reads_task(revised.literal(), &id),
                "{name}: an island still reads {id}"
            );
            assert!(
                revised.value(&Path::new(["tasks", to.as_str()])).is_some(),
                "{name} {id}"
            );
            let after_keys = revised.literal()["tasks"]
                .as_object()
                .into_iter()
                .flatten()
                .any(|(_, t)| {
                    t.pointer("/after")
                        .and_then(Value::as_object)
                        .is_some_and(|a| a.contains_key(&id))
                });
            assert!(!after_keys, "{name}: an after key still names {id}");
            renamed += 1;
        }
    }
    assert!(renamed >= 150, "tasks renamed: {renamed}");
}

// ── The language inventory, derived from its owners ─────────────────────

/// A minimal valid program's task block.
const PROBE_TASK: &str = "nika: probe\ntasks:\n  t:\n    exec:\n      command: [\"true\"]\n";

/// Each closed block of the language: a position pattern (`*` is any key
/// or index) and a program whose block carries the unknown key `zz_probe`.
/// The strict parser's refusal lists the block's keys ("the fields here").
fn probes() -> Vec<(&'static str, String)> {
    let task = |body: &str| format!("nika: probe\ntasks:\n  t:\n{body}");
    let exec = "    exec:\n      command: [\"true\"]\n";
    vec![
        ("", format!("zz_probe: 1\n{PROBE_TASK}")),
        ("tasks/*", task(&format!("    zz_probe: 1\n{exec}"))),
        (
            "tasks/*/infer",
            task("    infer:\n      prompt: x\n      zz_probe: 1\n"),
        ),
        ("tasks/*/exec", task(&format!("{exec}      zz_probe: 1\n"))),
        (
            "tasks/*/invoke",
            task("    invoke:\n      tool: nika:log\n      zz_probe: 1\n"),
        ),
        (
            "tasks/*/agent",
            task("    agent:\n      prompt: x\n      zz_probe: 1\n"),
        ),
        (
            "tasks/*/retry",
            task(&format!(
                "{exec}    retry:\n      max_attempts: 2\n      zz_probe: 1\n"
            )),
        ),
        (
            "tasks/*/on_error",
            task(&format!(
                "{exec}    on_error:\n      recover: x\n      zz_probe: 1\n"
            )),
        ),
        (
            "tasks/*/for_each",
            task(&format!(
                "{exec}    for_each:\n      items: [1]\n      zz_probe: 1\n"
            )),
        ),
        (
            "tasks/*/lift/*",
            task(&format!(
                "{exec}    lift:\n      - law: data-as-code\n        because: x\n        zz_probe: 1\n"
            )),
        ),
        (
            "tasks/*/infer/thinking",
            task(
                "    infer:\n      prompt: x\n      thinking:\n        enabled: true\n        zz_probe: 1\n",
            ),
        ),
        (
            "tasks/*/infer/vision/*",
            task(
                "    infer:\n      prompt: x\n      vision:\n        - source: file\n          path: a.png\n          zz_probe: 1\n",
            ),
        ),
        // Each vision source has its own keys: probe both forms.
        (
            "tasks/*/infer/vision/*",
            task(
                "    infer:\n      prompt: x\n      vision:\n        - source: url\n          url: \"https://a.invalid/x.png\"\n          zz_probe: 1\n",
            ),
        ),
        (
            "inputs/*",
            format!("inputs:\n  p:\n    type: string\n    zz_probe: 1\n{PROBE_TASK}"),
        ),
        (
            "secrets/*",
            format!("secrets:\n  s:\n    source: env\n    key: K\n    zz_probe: 1\n{PROBE_TASK}"),
        ),
        (
            "secrets/*/egress/*",
            format!(
                "secrets:\n  s:\n    source: env\n    key: K\n    egress:\n      - to: x\n        zz_probe: 1\n{PROBE_TASK}"
            ),
        ),
        (
            "outputs/*",
            format!("{PROBE_TASK}outputs:\n  o:\n    value: \"x\"\n    zz_probe: 1\n"),
        ),
        ("permits", format!("permits:\n  zz_probe: 1\n{PROBE_TASK}")),
        (
            "permits/fs",
            format!("permits:\n  fs:\n    zz_probe: []\n{PROBE_TASK}"),
        ),
        (
            "permits/net",
            format!("permits:\n  net:\n    zz_probe: []\n{PROBE_TASK}"),
        ),
        ("run", format!("run:\n  zz_probe: 1\n{PROBE_TASK}")),
        (
            "run/entropy",
            format!("run:\n  entropy:\n    seeded: 1\n    zz_probe: 1\n{PROBE_TASK}"),
        ),
    ]
}

/// The keys the strict parser lists when `program` carries an unknown key.
fn parser_keys(program: &str) -> Option<Vec<String>> {
    match nika_schema::parse(program, FileId::new(0), ParseMode::Strict) {
        Err(nika_schema::SchemaError::UnknownField {
            field,
            teaching: Some(teaching),
            ..
        }) if field == "zz_probe" => {
            let list = teaching.rsplit("the fields here: ").next()?;
            Some(list.split(" · ").map(|k| k.trim().to_owned()).collect())
        }
        _ => None,
    }
}

/// The Spec schema's closed keysets, by the position pattern of their block.
fn schema_keysets() -> BTreeMap<&'static str, Vec<String>> {
    let schema: Value = serde_json::from_str(nika_pack::schema_json()).expect("the Spec schema");
    let at = |pointer: &str| -> Vec<String> {
        schema
            .pointer(pointer)
            .and_then(|node| node.get("properties"))
            .and_then(Value::as_object)
            .map(|props| props.keys().cloned().collect())
            .unwrap_or_default()
    };
    BTreeMap::from([
        ("", at("")),
        ("inputs/*", at("/properties/inputs/additionalProperties")),
        ("secrets/*", at("/properties/secrets/additionalProperties")),
        (
            "secrets/*/egress/*",
            at("/properties/secrets/additionalProperties/properties/egress/items"),
        ),
        ("permits", at("/properties/permits")),
        ("permits/fs", at("/properties/permits/properties/fs")),
        ("permits/net", at("/properties/permits/properties/net")),
        ("run", at("/properties/run")),
        (
            "run/entropy",
            at("/properties/run/properties/entropy/anyOf/1"),
        ),
        (
            "outputs/*",
            at("/properties/outputs/additionalProperties/anyOf/1"),
        ),
        ("tasks/*", at("/$defs/task")),
        ("tasks/*/for_each", at("/$defs/task/properties/for_each")),
        ("tasks/*/lift/*", at("/$defs/task/properties/lift/items")),
        ("tasks/*/infer", at("/$defs/infer")),
        (
            "tasks/*/infer/thinking",
            at("/$defs/infer/properties/thinking"),
        ),
        (
            "tasks/*/infer/vision/*",
            at("/$defs/infer/properties/vision/items"),
        ),
        ("tasks/*/exec", at("/$defs/exec")),
        ("tasks/*/invoke", at("/$defs/invoke")),
        ("tasks/*/agent", at("/$defs/agent")),
        ("tasks/*/retry", at("/$defs/retry")),
        ("tasks/*/on_error", at("/$defs/onError")),
    ])
}

/// The vocabulary crate's published keysets, by position pattern.
fn vocab_keysets() -> BTreeMap<&'static str, Vec<String>> {
    use nika_schema::types::keys;
    let own = |set: &[&str]| set.iter().map(|k| (*k).to_owned()).collect::<Vec<_>>();
    BTreeMap::from([
        ("", own(nika_schema::parser::TOP_LEVEL_KEYS)),
        ("tasks/*", own(keys::TASK_KEYS)),
        ("tasks/*/infer", own(keys::INFER_KEYS)),
        ("tasks/*/exec", own(keys::EXEC_KEYS)),
        ("tasks/*/invoke", own(keys::INVOKE_KEYS)),
        ("tasks/*/agent", own(keys::AGENT_KEYS)),
        ("tasks/*/infer/thinking", own(keys::THINKING_KEYS)),
        ("tasks/*/retry", own(keys::RETRY_KEYS)),
        ("tasks/*/on_error", own(keys::ON_ERROR_KEYS)),
        ("inputs/*", own(keys::INPUT_KEYS)),
        ("const/*", own(keys::CONST_TYPED_KEYS)),
        ("secrets/*", own(keys::SECRET_KEYS)),
        ("secrets/*/egress/*", own(keys::EGRESS_KEYS)),
        ("outputs/*", own(keys::TYPED_OUTPUT_KEYS)),
        ("permits", own(keys::PERMITS_KEYS)),
        ("permits/fs", own(keys::PERMITS_FS_KEYS)),
        ("permits/net", own(keys::PERMITS_NET_KEYS)),
        ("run", own(keys::RUN_KEYS)),
        ("run/entropy", own(keys::RUN_ENTROPY_MAP_KEYS)),
    ])
}

/// Whether `path` sits at `pattern/key` (`*` is any one segment).
fn at_position(path: &Path, pattern: &str, key: &str) -> bool {
    let mut wanted: Vec<&str> = if pattern.is_empty() {
        Vec::new()
    } else {
        pattern.split('/').collect()
    };
    wanted.push(key);
    let segments = path.segments();
    segments.len() == wanted.len()
        && segments
            .iter()
            .zip(&wanted)
            .all(|(s, w)| *w == "*" || s == w)
}

/// The Spec schema's closed value vocabularies: (position, key, values).
fn schema_variants() -> Vec<(&'static str, &'static str, Vec<String>)> {
    let schema: Value = serde_json::from_str(nika_pack::schema_json()).expect("the Spec schema");
    let values = |pointer: &str| -> Vec<String> {
        schema
            .pointer(pointer)
            .and_then(|node| node.get("enum"))
            .and_then(Value::as_array)
            .map(|v| {
                v.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    vec![
        (
            "tasks/*/after",
            "*",
            values("/$defs/task/properties/after/patternProperties/^[a-z][a-z0-9_]*$"),
        ),
        (
            "tasks/*/exec",
            "capture",
            values("/$defs/exec/properties/capture"),
        ),
        (
            "tasks/*/exec",
            "decode",
            values("/$defs/exec/properties/decode"),
        ),
        (
            "tasks/*/retry",
            "backoff_strategy",
            values("/$defs/retry/properties/backoff_strategy"),
        ),
        (
            "tasks/*/lift/*",
            "law",
            values("/$defs/task/properties/lift/items/properties/law"),
        ),
        (
            "tasks/*/infer/vision/*",
            "source",
            values("/$defs/infer/properties/vision/items/properties/source"),
        ),
        (
            "secrets/*",
            "source",
            values("/properties/secrets/additionalProperties/properties/source"),
        ),
        (
            "run",
            "entropy",
            values("/properties/run/properties/entropy/anyOf/0"),
        ),
        ("run", "clock", values("/properties/run/properties/clock")),
    ]
}

/// One inventory row's coverage over the corpus.
#[derive(Default)]
struct Covered {
    programs: usize,
    nodes: usize,
    set_applied: usize,
    set_refused_by_parser: usize,
    remove_applied: usize,
}

fn covered(documents: &[(String, Document)], pattern: &str, key: &str) -> Covered {
    let mut out = Covered::default();
    for (_, document) in documents {
        let hits: Vec<Node> = document
            .nodes()
            .into_iter()
            .filter(|n| at_position(&n.path, pattern, key))
            .collect();
        if hits.is_empty() {
            continue;
        }
        out.programs += 1;
        out.nodes += hits.len();
        for node in hits {
            if let Some(value) = mutated(&node.value) {
                match document.apply(&[Edit::set(node.path.clone(), value)]) {
                    Ok(_) => out.set_applied += 1,
                    Err(Refusal::Language { .. }) => out.set_refused_by_parser += 1,
                    Err(_) => {}
                }
            }
            if document.apply(&[Edit::remove(node.path.clone())]).is_ok() {
                out.remove_applied += 1;
            }
        }
    }
    out
}

#[test]
fn the_language_inventory_is_derived_from_its_owners_and_covered() {
    use std::fmt::Write as _;
    let documents = imported();
    let mut rows: BTreeMap<(String, String), Vec<&'static str>> = BTreeMap::new();
    let mut unobservable = Vec::new();
    for (pattern, program) in probes() {
        match parser_keys(&program) {
            Some(keys) => {
                for key in keys {
                    rows.entry((pattern.to_owned(), key))
                        .or_default()
                        .push("parser");
                }
            }
            None => unobservable.push(pattern),
        }
    }
    for (owner, sets) in [("vocab", vocab_keysets()), ("spec", schema_keysets())] {
        for (pattern, keys) in sets {
            for key in keys {
                rows.entry((pattern.to_owned(), key))
                    .or_default()
                    .push(owner);
            }
        }
    }
    let mut receipt = String::from(
        "position\tkey\tparser\tvocab\tspec\tprograms\tnodes\tset_applied\tset_refused_by_parser\tremove_applied\n",
    );
    let mut uncovered = Vec::new();
    for ((pattern, key), owners) in &rows {
        let c = covered(&documents, pattern, key);
        let mark = |owner: &str| if owners.contains(&owner) { "yes" } else { "-" };
        let _ = writeln!(
            receipt,
            "{pattern}\t{key}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            mark("parser"),
            mark("vocab"),
            mark("spec"),
            c.programs,
            c.nodes,
            c.set_applied,
            c.set_refused_by_parser,
            c.remove_applied
        );
        if c.programs == 0 {
            uncovered.push(format!("{pattern}/{key}"));
        }
    }
    receipt.push_str("\nvariant_position\tvalue\tprograms\n");
    let mut uncovered_variants = Vec::new();
    for (pattern, key, values) in schema_variants() {
        assert!(
            !values.is_empty(),
            "the Spec schema states the values of {pattern}/{key}"
        );
        for value in values {
            let programs = documents
                .iter()
                .filter(|(_, d)| {
                    d.nodes()
                        .iter()
                        .any(|n| at_position(&n.path, pattern, key) && n.value == json!(value))
                })
                .count();
            let _ = writeln!(receipt, "{pattern}/{key}\t{value}\t{programs}");
            if programs == 0 {
                uncovered_variants.push(format!("{pattern}/{key}={value}"));
            }
        }
    }
    let _ = writeln!(receipt, "\nprograms\t{}", documents.len());
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("document-coverage.tsv");
    std::fs::write(&out, &receipt).expect("the coverage receipt");
    assert!(
        unobservable.is_empty(),
        "every probed block lists its keys: {unobservable:?}"
    );
    assert!(
        uncovered.is_empty() && uncovered_variants.is_empty(),
        "rows no program exercises:\n{uncovered:#?}\n{uncovered_variants:#?}\nreceipt: {}",
        out.display()
    );
}
