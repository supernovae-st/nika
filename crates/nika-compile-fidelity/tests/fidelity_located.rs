// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Law 1 over a file the person names alone: `tarifs-2027.md`, which lives at
//! `./notes/tarifs-2027.md`, is realized by a workflow that reads `./notes/**` through a glob
//! covering it, once the host located it there. Live gauntlet r03, 2026-10-16: a one-word prompt
//! edit was refused twenty times « UNREALIZED PATH: the request names `tarifs-2027.md` » while the
//! workflow read every note. Unlocated, the name stays unrealized.
use nika_compile_fidelity::fidelity::{Diagnostic, laws_observed};
use nika_compile_reader::plan::Plan;
use serde_json::{Value, json};

/// The request, then a later line of the person's naming one note alone.
const INTENT: &str = "Traduis chaque note de ./notes/ en anglais dans ./notes-en/.\nEt si la note tarifs-2027.md échoue, les autres sont quand même traduites ?";

/// A workflow that lists the notes with a glob, reads each, and writes its translation.
fn candidate() -> Value {
    json!({
        "nika": "translate-notes",
        "permits": {"tools": ["nika:glob", "nika:read", "nika:write"],
            "fs": {"read": ["./notes/**"], "write": ["./notes-en/**"]}},
        "tasks": {
            "list_notes": {"invoke": {"tool": "nika:glob", "args": {"pattern": "./notes/*.md"}}},
            "read_note": {"for_each": {"items": "${{ tasks.list_notes.output }}"},
                "invoke": {"tool": "nika:read", "args": {"path": "${{ item }}"}}},
            "write_note": {"for_each": {"items": "${{ tasks.read_note.output }}"},
                "invoke": {"tool": "nika:write",
                    "args": {"path": "./notes-en/${{ item.name }}", "content": "${{ item.text }}"}}}
        }
    })
}

/// The path findings of the laws over the candidate with `world`.
fn unrealized(world: Option<&Value>) -> Vec<String> {
    let mut out: Vec<Diagnostic> = Vec::new();
    laws_observed(
        INTENT,
        &Plan::default(),
        &candidate(),
        &[],
        &[],
        &[],
        world,
        &mut out,
    );
    out.into_iter()
        .filter(|d| d.message.starts_with("UNREALIZED PATH") && d.message.contains("tarifs-2027"))
        .map(|d| d.message)
        .collect()
}

#[test]
fn a_file_named_alone_and_located_under_a_read_folder_is_realized() {
    let located = json!({"observed": [{"path": "./notes/tarifs-2027.md", "state": "located",
        "named": "tarifs-2027.md", "complete": false}]});
    assert_eq!(unrealized(Some(&located)), Vec::<String>::new());
    let unlocated = json!({"observed": [{"path": "tarifs-2027.md", "state": "absent",
        "complete": false}]});
    assert_eq!(
        unrealized(Some(&unlocated)).len(),
        1,
        "unlocated, it stays unrealized"
    );
}

/// The path findings of the laws over `doc` for `intent` with `world`.
fn path_findings(intent: &str, doc: &Value, world: &Value) -> Vec<String> {
    let mut out: Vec<Diagnostic> = Vec::new();
    laws_observed(
        intent,
        &Plan::default(),
        doc,
        &[],
        &[],
        &[],
        Some(world),
        &mut out,
    );
    out.into_iter()
        .filter(|d| d.message.starts_with("UNREALIZED PATH"))
        .map(|d| d.message)
        .collect()
}

/// A workflow reading both statements where the host located them, and writing the summary.
fn reads_both() -> Value {
    json!({
        "nika": "bilan-octobre",
        "const": {"perso": "./releves/compte-perso-octobre.csv",
            "pro": "./releves/compte-pro-octobre.csv"},
        "permits": {"tools": ["nika:read", "nika:write"],
            "fs": {"read": ["./releves/compte-perso-octobre.csv",
                "./releves/compte-pro-octobre.csv"], "write": ["./bilan-octobre.md"]}},
        "tasks": {
            "read_perso": {"invoke": {"tool": "nika:read", "args": {"path": "${{ const.perso }}"}}},
            "read_pro": {"invoke": {"tool": "nika:read", "args": {"path": "${{ const.pro }}"}}},
            "save": {"invoke": {"tool": "nika:write",
                "args": {"path": "./bilan-octobre.md", "content": "x"}}}
        }
    })
}

/// Two files the person says they have in a folder, named in brackets after it, are sources:
/// the workflow reading each where the host located it realizes them (live gauntlet wave 6
/// r01, 2026-10-18: ten revisions reading `./releves/…` were refused as not writing them). A
/// file a request asks to write there stays a destination, which reading it never realizes.
#[test]
fn files_the_person_has_in_a_named_folder_are_read_not_written() {
    const HAS: &str = "Bonjour. J'ai deux fichiers CSV dans le dossier releves (compte-perso-octobre.csv et compte-pro-octobre.csv). Je voudrais un bilan qui additionne les depenses par categorie sur les deux fichiers, et qui ecrit le resultat dans bilan-octobre.md.";
    const WRITES: &str = "Écris le bilan dans compte-perso-octobre.csv.";
    let world = json!({"observed": [
        {"path": "./releves/compte-perso-octobre.csv", "state": "observed", "complete": true},
        {"path": "./releves/compte-pro-octobre.csv", "state": "observed", "complete": true}]});
    assert_eq!(
        path_findings(HAS, &reads_both(), &world),
        Vec::<String>::new()
    );
    let found = path_findings(WRITES, &reads_both(), &world);
    assert!(
        found.len() == 1 && found[0].contains("`compte-perso-octobre.csv`"),
        "{found:#?}"
    );
}
