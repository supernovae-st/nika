// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The TUI's own words follow the language the person writes, as the Session reads it from
//! their lines: a request written in French reads the decision's keys in French under the
//! Session's words, one written in English keeps them in English. The copy is deterministic:
//! no model is called, nothing is saved or run.

use super::*;

/// The same request in each language.
const FRENCH: &str = "Lis ./notes/brief.md et écris-le dans ./out/copie.md";
const ENGLISH: &str = "Read ./notes/brief.md and write it to ./out/copy.md";

/// The decision's keys, as each language says them on the hint row, the question
/// and acts this host builds beside the Session's words, the review card's own
/// words around the Session's facts, the field beside the decision, the
/// workspace's header and aside, and the proposal's faces (their names and what
/// the graph face says of the structure).
const KEYS_FR: [&str; 13] = [
    "↑↓ choisir",
    "Entrée valide",
    "Que faire de cette proposition relue ?",
    "Enregistrer le workflow",
    "Examiner les changements exacts",
    "crée compiled-workflow.nika",
    "répétition · aucune liée",
    "ou tapez votre propre réponse",
    "Préparer avec :",
    "Nika · Fichiers",
    "cette conversation",
    "[graphe] vérification",
    "Structure vérifiée · ce fichier seul",
];
const KEYS_EN: [&str; 13] = [
    "↑↓ choose",
    "Enter acts",
    "What should happen to this reviewed",
    "Save workflow",
    "Inspect the exact changes",
    "creates compiled-workflow.nika",
    "rehearsal · none bound",
    "or type your own reply",
    "Prepare with:",
    "Nika · Files",
    "this conversation",
    "[graph] check",
    "Structure checked · this file only",
];

/// 34 · A request in French reads the proposal's decision in French (its question, its acts,
/// their keys on the hint row) and the armed exit says what a second `Ctrl+C` does in French;
/// the same request in English keeps every one of them in English.
#[test]
fn a_french_request_reads_the_keys_in_french_and_english_keeps_english() {
    for (tag, request, keys, other, armed) in [
        (
            "34-language-fr",
            FRENCH,
            KEYS_FR,
            KEYS_EN,
            "Ctrl+C encore quitte · autre touche reste",
        ),
        (
            "34-language-en",
            ENGLISH,
            KEYS_EN,
            KEYS_FR,
            "Ctrl+C again leaves · any key stays",
        ),
    ] {
        let rig = Rig::new("language");
        std::fs::create_dir_all(rig.path("notes")).expect("notes");
        std::fs::write(rig.path("notes/brief.md"), "# Brief\n").expect("brief");
        let before = rig.tree();
        let mut term = rig.spawn(tag, 120, 36);
        wait_workspace(&mut term);
        term.send(&format!("{request}\r"));
        term.wait_workspace_frame("the proposal's decision", |screen| {
            action_prompt_shown(screen) && keys.iter().all(|key| screen.contains(key))
        });
        assert!(
            other.iter().all(|key| !term.screen.contains(key)),
            "one language on the hint row\n{}",
            term.dump()
        );
        assert_eq!(rig.tree(), before, "a proposal writes nothing");
        // One `Ctrl+C` arms the exit: the row says what a second press does.
        term.send("\x03");
        term.wait_workspace_frame("the armed exit", |screen| screen.contains(armed));
        term.send("\x03");
        let status = term.finish();
        assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
        term.assert_restored();
        rig.no_model_call("a deterministic copy proposed");
        rig.no_run("a proposal");
    }
}

/// 34 · Before the person writes, the first screen speaks this machine's locale (live gauntlet
/// 6, r04 and r10: « Prepare with: … » on a French machine until the first French line): under
/// `LANG=fr_FR.UTF-8` the top bar and the aside read French at once; the person's English
/// request then reads its decision in English.
#[test]
fn the_first_screen_speaks_the_locale_until_the_persons_words_decide() {
    let rig = Rig::new("language-locale");
    std::fs::create_dir_all(rig.path("notes")).expect("notes");
    std::fs::write(rig.path("notes/brief.md"), "# Brief\n").expect("brief");
    let french = [("LANG", "fr_FR.UTF-8")];
    let mut term = rig.spawn_with("34-language-locale", &[], 120, 36, &french);
    wait_workspace(&mut term);
    term.wait_workspace_frame("the first screen in French", |screen| {
        screen.contains("Préparer avec :") && screen.contains("cette conversation")
    });
    assert!(
        !term.screen.contains("Prepare with:") && !term.screen.contains("this conversation"),
        "one language before the first line\n{}",
        term.dump()
    );
    term.send(&format!("{ENGLISH}\r"));
    term.wait_workspace_frame("the proposal's decision in English", |screen| {
        action_prompt_shown(screen) && KEYS_EN.iter().all(|key| screen.contains(key))
    });
    term.send("\x03");
    term.wait_workspace_frame("the armed exit", |screen| {
        screen.contains("Ctrl+C again leaves · any key stays")
    });
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    term.assert_restored();
    rig.no_model_call("a deterministic copy proposed");
    rig.no_run("a proposal");
}
