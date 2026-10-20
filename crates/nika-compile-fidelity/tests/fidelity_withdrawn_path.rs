// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A file the person takes back is no longer owed (Law 1): their later line that stops it
//! (« ne lis pas profil.md ») or an offer they picked that says it is no longer read withdraws the
//! request's earlier mention, and naming it again asks for it again. Live gauntlet, 2026-10-15:
//! the founder's « ne lis pas profil.md, copie mon profil dans le workflow » left `profil.md`
//! owed, and every workflow without the read was refused until they restated their whole request.

use std::collections::BTreeMap;

use nika_compile_fidelity::fidelity::{Diagnostic, laws_answered};
use nika_compile_reader::lexicon;
use serde_json::{Value, json};

/// The request: the profile file is named as a source.
const REQUEST: &str = "Chaque matin, recupere les offres de https://remotive.com/api/remote-jobs, \
                       score-les par rapport a mon profil.md, et ecris jobs.md dans ce dossier.";
/// The question whose offer withdraws the file, and that offer's label.
const KEY: &str = "profile_route";
const BAKED: &str = "Copier ton profil dans le workflow (100 % automatique ; profil.md n'est plus \
                     lu, donc ses modifications ne sont pas prises en compte)";

/// A candidate that fetches the board and writes the digest, the profile copied into its
/// prompt: it never reads `profil.md`.
fn baked() -> Value {
    json!({"nika": "jobs", "tasks": {
        "jobs": {"invoke": {"tool": "nika:fetch",
            "args": {"url": "https://remotive.com/api/remote-jobs", "method": "GET"}}},
        "score": {"with": {"jobs": "${{ tasks.jobs.output }}"},
            "infer": {"prompt": "Score ${{ with.jobs }} for a backend Rust/Go profile."}},
        "write_jobs": {"with": {"scored": "${{ tasks.score.output }}"},
            "invoke": {"tool": "nika:write",
                "args": {"path": "./jobs.md", "content": "${{ with.scored }}"}}},
    }, "permits": {"fs": {"write": ["./jobs.md"]}, "net": {"http": ["remotive.com"]},
        "tools": ["nika:fetch", "nika:write"]}})
}

/// The Session's description of the person's pick of `label` on line `u2`, located as the
/// second line of their words.
fn pick(label: &str) -> String {
    let provenance =
        json!({"kind": "offered", "message": "u2", "question": KEY, "option": "baked"});
    json!({
        "bound_values": [],
        "bindings": [],
        "choice_context": {"question_key": KEY, "question": "Lequel préfères-tu ?",
            "human_citation": "u2", "human_lines": [1, 2],
            "selected_options": [{"key": "baked", "label": label, "values": [],
                "provenance": provenance}],
            "human_reply": "baked"},
    })
    .to_string()
}

/// The paths Law 1 finds unrealized for `intent` over `doc` with `answers`.
fn unrealized(intent: &str, doc: &Value, answers: &[String]) -> Vec<String> {
    let answers: BTreeMap<String, String> = (answers.iter())
        .map(|raw| (KEY.to_owned(), raw.clone()))
        .collect();
    let plan = lexicon::read(intent).plan;
    let mut out: Vec<Diagnostic> = Vec::new();
    laws_answered(intent, &plan, doc, (&answers, &[]), None, &mut out);
    (out.iter())
        .filter(|d| d.kind == "path")
        .filter_map(|d| d.message.split('`').nth(1).map(str::to_owned))
        .collect()
}

/// The request alone owes the profile. The person's later line that stops it withdraws it; a
/// line after that naming it again asks for it again.
#[test]
fn a_typed_withdrawal_takes_the_file_back() {
    let doc = baked();
    assert_eq!(unrealized(REQUEST, &doc, &[]), ["profil.md"]);
    let withdrawn = format!("{REQUEST}\nne lis pas profil.md, copie mon profil dans le workflow");
    assert_eq!(unrealized(&withdrawn, &doc, &[]), [""; 0]);
    let again = format!("{withdrawn}\nfinalement, lis quand même profil.md");
    assert_eq!(unrealized(&again, &doc, &[]), ["profil.md"]);
}

/// An offer the person picked that says the file is no longer read withdraws it; their later
/// line naming it again asks for it again.
#[test]
fn a_picked_withdrawal_takes_the_file_back() {
    let doc = baked();
    let intent = format!("{REQUEST}\nbaked");
    assert_eq!(unrealized(&intent, &doc, &[pick(BAKED)]), [""; 0]);
    let again = format!("{intent}\nnon, lis profil.md");
    assert_eq!(unrealized(&again, &doc, &[pick(BAKED)]), ["profil.md"]);
    let other = "Lire profil.md à chaque run, avec ta validation avant d'écrire jobs.md";
    assert_eq!(unrealized(&intent, &doc, &[pick(other)]), ["profil.md"]);
}
