// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! « Inspect the exact changes » on the real binary, the conversation led by a scripted author
//! on the person's kept local route (127.0.0.1 only): a revision of the saved workflow opens on
//! the lines it changes against the file as it is now, before its exact bytes (live gauntlet
//! r06 B8: the file printed whole, the one changed line left to find). Inspecting writes
//! nothing. Keyless: nothing is run.

use serde_json::{Value, json};

use super::scripted_author::Author;
use super::*;

/// The project's own sources the request names.
const SOURCE: &str = "./news/sources.md";
/// Where the workflow writes.
const OUTPUT: &str = "./news/digest.md";
/// The request: the source named, the output asked for.
const REQUEST: &str =
    "Résume ./news/sources.md en Markdown dans ./news/digest.md, sans rien inventer.";
/// The acts that save the proposal and open its exact changes, in English and as a French
/// request reads them (the request is French): either form is the same act.
const SAVE: [&str; 2] = ["Save workflow", "Enregistrer le workflow"];
const INSPECT: [&str; 2] = [
    "Inspect the exact changes",
    "Examiner les changements exacts",
];
/// What the revision's card says it does to the saved file, in either form.
const REPLACES: [&str; 2] = ["replaces news-digest.nika", "remplace news-digest.nika"];

/// `screen` shows the act in one of its two forms.
fn said(screen: &vt::Screen, act: [&str; 2]) -> bool {
    act.iter().any(|form| screen.contains(form))
}

/// The correction the revision answers.
const CORRECTION: &str = "Fais plutôt une liste à puces.";
/// The summary's instruction, before and after the correction.
const PROSE: &str = "Résume en Markdown les actualités ci-dessous, sans rien inventer";
const LIST: &str = "Résume en liste à puces les actualités ci-dessous, sans rien inventer";

/// The candidate: one read of the named source, one summary told `how`, one write.
fn digest(how: &str) -> String {
    format!(
        "nika: news-digest\nmodel: vllm/s03-seat\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"{SOURCE}\"]\n    write: [\"{OUTPUT}\"]\ntasks:\n  read_news:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"{SOURCE}\" }}\n  summarize:\n    with:\n      news: \"${{{{ tasks.read_news.output }}}}\"\n    infer:\n      max_tokens: 1000\n      prompt: \"{how} : ${{{{ with.news }}}}\"\n  write_digest:\n    with:\n      digest: \"${{{{ tasks.summarize.output }}}}\"\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"{OUTPUT}\", content: \"${{{{ with.digest }}}}\" }}\n"
    )
}

/// What the scripted author does: write and propose the candidate, then, once it is saved,
/// write and propose its revision.
fn script() -> Vec<Value> {
    let write = |how: &str| {
        json!({"tool": "candidate_write", "args": {"source": digest(how),
            "summary": "Tes sources, résumé dans ./news/digest.md", "resolutions": []}})
    };
    vec![
        write(PROSE),
        json!({"tool": "propose", "say": "Voici le workflow.", "args": {}}),
        write(LIST),
        json!({"tool": "propose", "say": "Voici la liste à puces.", "args": {}}),
    ]
}

/// 33 · A revision of the saved workflow, inspected, reads its one changed line first:
/// removed then added against the file as it is now, and the file is untouched.
#[test]
fn inspecting_a_revision_opens_on_the_lines_it_changes() {
    let author = Author::start(script());
    let rig = Rig::new("inspect");
    std::fs::create_dir_all(rig.path("news")).expect("news");
    std::fs::write(rig.path(SOURCE), "# Tech\n- Rust ships a release.\n").expect("sources");
    let base = author.base();
    let env = [
        ("NIKA_VLLM_BASE_URL", base.as_str()),
        ("NIKA_SESSION_DRIVER", "agent"),
    ];
    let mut term = rig.spawn_with("inspect", &[], 120, 40, &env);
    wait_workspace(&mut term);

    term.send(REQUEST);
    term.send("\r");
    term.wait_until("the save decision", |screen| {
        said(screen, SAVE) && said(screen, INSPECT)
    });
    term.send("yes\r");
    term.wait_until("the workflow saved", |screen| {
        // The person writes French: the save is said in French.
        screen.contains("appliqué · écrit `news-digest.nika`")
    });
    let saved = std::fs::read_to_string(rig.path("news-digest.nika")).expect("the saved file");
    assert!(saved.contains(PROSE), "{saved}");

    term.send(CORRECTION);
    term.send("\r");
    term.wait_until("the revision's decision", |screen| {
        said(screen, INSPECT) && said(screen, REPLACES)
    });
    // « 4 Inspect the exact changes », selected by its digit, then Enter.
    term.keys("4");
    term.send("\r");
    term.wait_until("the changed lines", |screen| {
        screen.contains("changes in `news-digest.nika` against the file as it is now")
    });
    term.settle();
    let marked = |sign: &str, words: &str| {
        (term.screen.lines().iter())
            .any(|row| row.contains(words) && row.contains(&format!("{sign}       prompt:")))
    };
    assert!(
        marked("-", "en Markdown") && marked("+", "en liste"),
        "the line removed then added\n{}",
        term.dump()
    );
    assert_eq!(
        std::fs::read_to_string(rig.path("news-digest.nika")).expect("the saved file"),
        saved,
        "inspecting writes nothing"
    );
    rig.no_run("an inspected revision");
    term.leave();
}
