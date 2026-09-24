// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

/// A synthetic snapshot: each row file written from its rows, each root file under `foundry/`,
/// every file pinned by the manifest.
fn custom(rows: &[(&str, Vec<Value>)], files: &[(&str, String)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let snap = root.join(".local/foundry/snapshots/knowledge-c");
    let mut pins = serde_json::Map::new();
    for (name, rows) in rows {
        let text = rows
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        write(&snap.join(name), &text);
        pins.insert(
            format!("knowledge/{name}"),
            json!(sha256_hex(text.as_bytes())),
        );
    }
    for (relative, text) in files {
        write(&root.join("foundry").join(relative), text);
        pins.insert((*relative).to_owned(), json!(sha256_hex(text.as_bytes())));
    }
    write(
        &snap.join("manifest.json"),
        &json!({"knowledge_version": "knowledge-c", "digest": "c", "files": pins}).to_string(),
    );
    (dir, snap)
}

fn edge(from: &str, rel: &str, to: &str) -> Value {
    json!({"from": from, "rel": rel, "to": to})
}

fn ids<'a>(pack: &'a AuthoringKnowledge, kind: &str) -> Vec<&'a str> {
    pack.references
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| r.id.as_str())
        .collect()
}

/// Nine generic graph patterns whose ids sort first, and one pattern the request names word for
/// word whose id sorts last: relevance, never the ids' order, decides what is presented.
#[test]
fn relevance_not_id_order_decides_the_patterns_and_blocks_presented() {
    let mut patterns: Vec<Value> = (1..=9)
        .map(|n| json!({"id": format!("pattern:a{n}"), "title": "generic step", "purpose": "a step", "notes": ""}))
        .collect();
    patterns.push(json!({"id": "pattern:zz-quarantine", "title": "Quarantine invalid invoices", "purpose": "move invalid invoices aside", "notes": "quarantine"}));
    let mut relations: Vec<Value> = vec![edge("family:batch", "RECOMMENDS", "pack:batch")];
    for n in 1..=9 {
        relations.push(edge("pack:batch", "CONTAINS", &format!("pattern:a{n}")));
    }
    let mut blocks = Vec::new();
    let mut files = Vec::new();
    for n in 1..=4 {
        blocks.push(json!({"id": format!("block:a{n}"), "title": "generic block", "purpose": "steps", "file": format!("blocks/a{n}.nika")}));
        relations.push(edge(
            &format!("block:a{n}"),
            "REALIZES",
            &format!("pattern:a{n}"),
        ));
        files.push((
            format!("blocks/a{n}.nika"),
            format!("nika: a{n}\ntasks: {{}}\n"),
        ));
    }
    blocks.push(json!({"id": "block:zz-quarantine", "title": "Quarantine block", "purpose": "validate then quarantine", "file": "blocks/zz.nika"}));
    relations.push(edge(
        "block:zz-quarantine",
        "REALIZES",
        "pattern:zz-quarantine",
    ));
    files.push((
        "blocks/zz.nika".to_owned(),
        "nika: zz\ntasks: {}\n".to_owned(),
    ));
    let files: Vec<(&str, String)> = files.iter().map(|(p, t)| (p.as_str(), t.clone())).collect();
    let (_dir, snap) = custom(
        &[
            (
                "families.jsonl",
                vec![json!({"id": "family:batch", "title": "Batch", "need": "process the batch"})],
            ),
            ("patterns.jsonl", patterns),
            ("blocks.jsonl", blocks),
            ("relations.jsonl", relations),
        ],
        &files,
    );
    let pack = Snapshot::open(&snap)
        .unwrap()
        .pack("Quarantine the invalid invoices of the batch", None)
        .unwrap();
    let presented = ids(&pack, "pattern");
    assert!(
        presented.contains(&"pattern:zz-quarantine"),
        "{presented:?}"
    );
    assert!(
        presented
            .iter()
            .position(|id| *id == "pattern:zz-quarantine")
            < Some(2),
        "the direct match takes the second slot, after the leading family's first: {presented:?}"
    );
    assert!(
        ids(&pack, "block").contains(&"block:zz-quarantine"),
        "{:?}",
        pack.selection["blocks"]
    );
}

/// Two obligations, two families: four blocks realize the leading family's patterns and sort
/// first, one realizes the secondary family's; the secondary obligation keeps its block.
#[test]
fn a_secondary_obligation_keeps_its_block_beside_the_leading_familys() {
    let mut blocks = Vec::new();
    let mut relations = vec![
        edge("family:filter", "RECOMMENDS", "pack:filter"),
        edge("pack:filter", "CONTAINS", "pattern:filter-rows"),
        edge("pack:filter", "CONTAINS", "pattern:keep-header"),
        edge("family:total", "RECOMMENDS", "pack:total"),
        edge("pack:total", "CONTAINS", "pattern:sum-amounts"),
    ];
    let mut files = Vec::new();
    for n in 1..=4 {
        blocks.push(json!({"id": format!("block:a-filter-{n}"), "title": "Filter block", "purpose": "keep rows", "file": format!("blocks/f{n}.nika")}));
        relations.push(edge(
            &format!("block:a-filter-{n}"),
            "REALIZES",
            "pattern:filter-rows",
        ));
        files.push((
            format!("blocks/f{n}.nika"),
            format!("nika: f{n}\ntasks: {{}}\n"),
        ));
    }
    blocks.push(json!({"id": "block:z-total", "title": "Total block", "purpose": "sum a column", "file": "blocks/total.nika"}));
    relations.push(edge("block:z-total", "REALIZES", "pattern:sum-amounts"));
    files.push((
        "blocks/total.nika".to_owned(),
        "nika: total\ntasks: {}\n".to_owned(),
    ));
    let files: Vec<(&str, String)> = files.iter().map(|(p, t)| (p.as_str(), t.clone())).collect();
    let (_dir, snap) = custom(
        &[
            (
                "families.jsonl",
                vec![
                    json!({"id": "family:filter", "title": "Filter rows", "need": "keep only the paid rows of a CSV"}),
                    json!({"id": "family:total", "title": "Total", "need": "write the total amount"}),
                ],
            ),
            (
                "patterns.jsonl",
                vec![
                    json!({"id": "pattern:filter-rows", "title": "Filter rows", "purpose": "keep rows", "notes": ""}),
                    json!({"id": "pattern:keep-header", "title": "Keep header", "purpose": "same header", "notes": ""}),
                    json!({"id": "pattern:sum-amounts", "title": "Sum", "purpose": "sum amounts", "notes": ""}),
                ],
            ),
            ("blocks.jsonl", blocks),
            ("relations.jsonl", relations),
        ],
        &files,
    );
    let pack = Snapshot::open(&snap)
        .unwrap()
        .pack(
            "Keep only the paid rows of sales.csv, then write the total amount to total.txt",
            None,
        )
        .unwrap();
    let blocks = ids(&pack, "block");
    assert!(
        blocks.contains(&"block:z-total"),
        "{blocks:?} · {}",
        pack.selection["blocks"]
    );
    assert!(
        blocks.iter().any(|b| b.starts_with("block:a-filter")),
        "{blocks:?}"
    );
    assert!(ids(&pack, "pattern").contains(&"pattern:sum-amounts"));
}

/// Four large blocks and three large examples exceed the pack's byte cap: what the cap leaves
/// out is recorded as selected and excluded with its reason, never as presented.
#[test]
fn a_selected_item_the_byte_cap_leaves_out_is_recorded_excluded_never_presented() {
    let big = |name: &str| format!("nika: {name}\n# {}\ntasks: {{}}\n", "x".repeat(FILE_BYTES));
    let mut relations = vec![edge("family:report", "RECOMMENDS", "pack:report")];
    let mut patterns = Vec::new();
    let mut blocks = Vec::new();
    let mut examples = Vec::new();
    let mut files = Vec::new();
    for n in 1..=4 {
        patterns.push(json!({"id": format!("pattern:p{n}"), "title": "report step", "purpose": "report", "notes": ""}));
        relations.push(edge("pack:report", "CONTAINS", &format!("pattern:p{n}")));
        blocks.push(json!({"id": format!("block:b{n}"), "title": "Report block", "purpose": "report", "file": format!("blocks/b{n}.nika")}));
        relations.push(edge(
            &format!("block:b{n}"),
            "REALIZES",
            &format!("pattern:p{n}"),
        ));
        files.push((format!("blocks/b{n}.nika"), big(&format!("b{n}"))));
    }
    for n in 1..=3 {
        examples.push(json!({"id": format!("example:e{n}"), "intent": "write the weekly sales report", "file": format!("examples/e{n}/workflow.nika")}));
        files.push((
            format!("examples/e{n}/workflow.nika"),
            big(&format!("e{n}")),
        ));
    }
    let files: Vec<(&str, String)> = files.iter().map(|(p, t)| (p.as_str(), t.clone())).collect();
    let (_dir, snap) = custom(
        &[
            (
                "families.jsonl",
                vec![
                    json!({"id": "family:report", "title": "Report", "need": "write the weekly sales report"}),
                ],
            ),
            ("patterns.jsonl", patterns),
            ("blocks.jsonl", blocks),
            ("examples.jsonl", examples),
            ("relations.jsonl", relations),
        ],
        &files,
    );
    let pack = Snapshot::open(&snap)
        .unwrap()
        .pack("Write the weekly sales report", None)
        .unwrap();
    let presented: BTreeSet<(String, String)> = pack
        .references
        .iter()
        .map(|r| (r.kind.clone(), r.id.clone()))
        .collect();
    let mut excluded = 0;
    for (list, kind) in [
        ("patterns", "pattern"),
        ("blocks", "block"),
        ("examples", "example"),
    ] {
        for entry in pack.selection[list].as_array().unwrap() {
            let id = entry["id"].as_str().unwrap().to_owned();
            let shown = presented.contains(&(kind.to_owned(), id.clone()));
            assert_eq!(entry["presented"], json!(shown), "{list} {id}: {entry}");
            if !shown {
                excluded += 1;
                let why = entry["excluded"].as_str().unwrap_or_default();
                assert!(why.contains("byte cap"), "{list} {id}: {entry}");
            }
        }
        let receipt = &pack.selection["receipt"][list];
        assert_eq!(
            receipt["presented"].as_u64().unwrap() + receipt["excluded"].as_u64().unwrap(),
            receipt["selected"].as_u64().unwrap(),
            "{list}: {receipt}"
        );
    }
    assert!(
        excluded >= 1,
        "the cap left something out: {}",
        pack.selection
    );
    assert!(pack.selection["bytes"].as_u64().unwrap() <= 40 * 1024);
}

/// A block is presented with the metadata that keeps it from being misused, bounded, before
/// its code.
#[test]
fn a_block_states_its_holes_effects_capabilities_known_failures_and_version() {
    let block = json!({
        "id": "block:csv-total", "title": "CSV total", "purpose": "filter rows, total a column",
        "file": "blocks/csv-total.nika",
        "holes": [{"name": "const.source_path", "owner": "human", "note": "the file the request names"}, {"name": "tasks.compute.invoke.args.expression", "owner": "machine"}],
        "effects": ["fs.read", "fs.write"],
        "authority": ["permits.fs", "permits.tools"],
        "interfaces": ["ReadableSource", "Aggregator"],
        "callables": ["nika:read", "nika:jq", "nika:write"],
        "known_failure_modes": ["cells are text: forgetting tonumber sums strings"],
        "pin": {"binary": "nika 0.120.3 (578352a31)", "spec_sha": "4b6eaadde483bcc9db9c05b022afbedfb107f37e"},
        "check_receipt": {"verdict": "CURRENT_CHECKED", "codes": []},
        "status": "EXPERIMENTAL",
        "proof_level": "OUTCOME_CORRECT"
    });
    let (_dir, snap) = custom(
        &[
            (
                "families.jsonl",
                vec![json!({"id": "family:total", "title": "Total", "need": "total a csv column"})],
            ),
            (
                "patterns.jsonl",
                vec![
                    json!({"id": "pattern:total", "title": "Total", "purpose": "sum", "notes": ""}),
                ],
            ),
            ("blocks.jsonl", vec![block]),
            (
                "relations.jsonl",
                vec![
                    edge("family:total", "RECOMMENDS", "pack:t"),
                    edge("pack:t", "CONTAINS", "pattern:total"),
                    edge("block:csv-total", "REALIZES", "pattern:total"),
                ],
            ),
        ],
        &[(
            "blocks/csv-total.nika",
            "nika: csv-total\ntasks: {}\n".to_owned(),
        )],
    );
    let pack = Snapshot::open(&snap)
        .unwrap()
        .pack("Total the amount column of a csv", None)
        .unwrap();
    let text = &pack
        .references
        .iter()
        .find(|r| r.kind == "block")
        .unwrap()
        .text;
    for needle in [
        "holes: const.source_path (human: the file the request names); tasks.compute.invoke.args.expression (machine)",
        "effects: fs.read, fs.write",
        "authority: permits.fs, permits.tools",
        "capabilities: ReadableSource, Aggregator",
        "callables: nika:read, nika:jq, nika:write",
        "known failures: cells are text: forgetting tonumber sums strings",
        "version: nika 0.120.3 (578352a31) · spec 4b6eaadde483 · check CURRENT_CHECKED · EXPERIMENTAL · proof OUTCOME_CORRECT",
    ] {
        assert!(text.contains(needle), "{needle} missing from:\n{text}");
    }
    assert!(text.find("holes:") < text.find("```yaml"), "{text}");
}

/// A request no row shares a word with presents nothing, says so, and the pack still composes:
/// the seat reads the card alone, and nothing is invented.
#[test]
fn a_request_nothing_matches_is_stated_as_no_match_and_the_pack_still_composes() {
    let (_dir, snap) = snapshot();
    let pack = Snapshot::open(&snap)
        .unwrap()
        .pack("zzz qqq", None)
        .unwrap();
    assert!(pack.references.is_empty(), "{:?}", pack.references);
    assert!(
        pack.selection["no_match"]
            .as_str()
            .is_some_and(|s| s.contains("card alone")),
        "{}",
        pack.selection
    );
    assert_eq!(
        pack.identity["door"]["pack_sha256"],
        json!(pack_sha256(&pack))
    );
    assert_eq!(
        pack.repairs.get("NIKA-PARSE-022").map(Vec::len),
        Some(1),
        "repairs stay available"
    );
}

/// The lexical prefilter shares no word between a French phrasing and English rows: nothing is
/// recalled and the record says so (a stated limit, never a fabricated match); a French row is
/// matched through accent folding.
#[test]
fn the_lexical_prefilter_across_french_and_english_is_stated_not_hidden() {
    let rows = |need: &str| {
        vec![
            (
                "families.jsonl",
                vec![json!({"id": "family:paid", "title": "Filter and aggregate", "need": need})],
            ),
            ("relations.jsonl", vec![]),
        ]
    };
    let french = "Garde uniquement les lignes réglées puis additionne leurs montants";
    let (_dir, english) = custom(&rows("keep only the paid rows and sum their amounts"), &[]);
    let pack = Snapshot::open(&english)
        .unwrap()
        .pack(french, None)
        .unwrap();
    assert!(
        pack.selection["families"].as_array().unwrap().is_empty(),
        "{}",
        pack.selection
    );
    assert!(pack.selection["no_match"].is_string(), "{}", pack.selection);
    let (_dir2, bilingual) = custom(
        &rows("garder les lignes reglees, additionner les montants"),
        &[],
    );
    let pack = Snapshot::open(&bilingual)
        .unwrap()
        .pack(french, None)
        .unwrap();
    assert_eq!(
        pack.selection["families"][0]["id"], "family:paid",
        "{}",
        pack.selection
    );
}

/// The selection names its own selector: this door's Rust BM25 and Foundry graph, not the
/// producer's selection, under a builder version that changed with the selection.
#[test]
fn the_selection_names_its_selector_and_the_builder_version() {
    let (_dir, snap) = snapshot();
    let pack = Snapshot::open(&snap)
        .unwrap()
        .pack(DIGEST_INTENT, Some("sealed"))
        .unwrap();
    assert_eq!(PACK_BUILDER, "nika-compile/knowledge-door-v3");
    assert_eq!(pack.identity["door"]["builder"], PACK_BUILDER);
    let selector = &pack.selection["selector"];
    assert!(
        selector["note"]
            .as_str()
            .is_some_and(|s| s.contains("not the Foundry producer")),
        "{selector}"
    );
    assert_eq!(pack.selection["retriever"], "bm25");
    assert_eq!(pack.selection["receipt"]["families"]["available"], 2);
}

/// Metadata past its byte bound is left out by whole field, named in the text and in the
/// receipt — never cut mid-line into something that reads complete.
#[test]
fn metadata_past_its_bound_is_named_as_omitted_never_cut_mid_line() {
    let failures: Vec<String> = (0..40)
        .map(|n| format!("failure mode {n} described at length"))
        .collect();
    let block = json!({
        "id": "block:long", "title": "Long", "purpose": "many failures", "file": "blocks/long.nika",
        "holes": [{"name": "const.source_path", "owner": "human"}],
        "effects": ["fs.read"],
        "known_failure_modes": failures,
        "status": "EXPERIMENTAL"
    });
    let (_dir, snap) = custom(
        &[
            (
                "families.jsonl",
                vec![json!({"id": "family:long", "title": "Long", "need": "read the long file"})],
            ),
            (
                "patterns.jsonl",
                vec![
                    json!({"id": "pattern:read", "title": "Read", "purpose": "read", "notes": ""}),
                ],
            ),
            ("blocks.jsonl", vec![block]),
            (
                "relations.jsonl",
                vec![
                    edge("family:long", "RECOMMENDS", "pack:l"),
                    edge("pack:l", "CONTAINS", "pattern:read"),
                    edge("block:long", "REALIZES", "pattern:read"),
                ],
            ),
        ],
        &[("blocks/long.nika", "nika: long\ntasks: {}\n".to_owned())],
    );
    let pack = Snapshot::open(&snap)
        .unwrap()
        .pack("Read the long file", None)
        .unwrap();
    let text = &pack
        .references
        .iter()
        .find(|r| r.kind == "block")
        .unwrap()
        .text;
    assert!(text.contains("holes: const.source_path (human)"), "{text}");
    assert!(
        text.contains("version: EXPERIMENTAL"),
        "a later field that fits stays: {text}"
    );
    assert!(
        !text.contains("known failures:"),
        "never a partial line: {text}"
    );
    assert!(
        text.contains("metadata omitted at 1024 bytes: known failures"),
        "{text}"
    );
    assert_eq!(
        pack.selection["metadata_omitted"],
        json!([{"id": "block:long", "metadata_omitted": ["known failures"]}])
    );
}
