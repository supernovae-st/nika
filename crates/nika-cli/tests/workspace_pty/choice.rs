// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A proposal's choice on the real binary, the conversation led by a scripted author on the
//! person's kept local route (127.0.0.1 only): the choice covers the conversation, so the reply
//! the author writes to words typed beside it reads in the card's head (live gauntlet r09 B1);
//! those words are said, never restored as « not taken » (r08 B4); `annuler` on its own line
//! discards the proposal through the consent door, as the hint says, with no model call and no
//! file written; the person's bubbles read their own words, never a prompt (r07 B9). Keyless:
//! nothing is saved or run.

use serde_json::{Value, json};

use super::scripted_author::Author;
use super::*;

/// The project's own sources the request names.
const SOURCE: &str = "./news/sources.md";
/// Where the proposal writes by default, and the other folder its choice offers.
const OUTPUT: &str = "./news/digest.md";
const NOTES: &str = "./notes/digest.md";
/// The request: the source named, the output left to the author.
const REQUEST: &str = "Résume ./news/sources.md en Markdown, sans rien inventer.";
/// The choice the proposal leaves open.
const WHERE: &str = "Où écrire le résumé ?";
/// Words typed beside the choice: no answer among its offers.
const BESIDE: &str = "je voulais juste le lire ici, pas besoin de fichier";
/// What the author replies to them, the choice left open.
const NOTED: &str = "D'accord, je ne change rien pour l'instant.";

/// The candidate: one read of the named source, one summary, one write.
fn digest() -> String {
    format!(
        "nika: news-digest\nmodel: vllm/s03-seat\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"{SOURCE}\"]\n    write: [\"{OUTPUT}\"]\ntasks:\n  read_news:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"{SOURCE}\" }}\n  summarize:\n    with:\n      news: \"${{{{ tasks.read_news.output }}}}\"\n    infer:\n      max_tokens: 1000\n      prompt: \"Résume en Markdown les actualités ci-dessous, sans rien inventer : ${{{{ with.news }}}}\"\n  write_digest:\n    with:\n      digest: \"${{{{ tasks.summarize.output }}}}\"\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"{OUTPUT}\", content: \"${{{{ with.digest }}}}\" }}\n"
    )
}

/// What the scripted author does: write the candidate (the source spelled as the person typed
/// it needs no selection), propose it with the output folder left open, then answer the words
/// typed beside that choice in words alone.
fn script() -> Vec<Value> {
    let folder = |key: &str, path: &str, recommended: bool| {
        json!({"key": key, "label": path, "recommended": recommended,
            "values": [{"role": "output_path", "value": path}]})
    };
    vec![
        json!({"tool": "candidate_write", "args": {"source": digest(),
            "summary": "Tes sources, résumé dans ./news/digest.md", "resolutions": []}}),
        json!({"tool": "propose", "say": "Voici le workflow.", "args": {"open": [
            {"key": "folder", "question": WHERE, "role": "output_path",
             "options": [folder("news", OUTPUT, true), folder("notes", NOTES, false)]}]}}),
        json!({"say": NOTED}),
    ]
}

/// The composer's row: the one holding the answer line's prompt.
fn composer_row(screen: &vt::Screen) -> String {
    (screen.lines().into_iter())
        .rev()
        .find(|row| {
            ["reply ›", "reply >", "réponse ›", "nika ›"]
                .iter()
                .any(|prompt| row.contains(prompt))
        })
        .unwrap_or_default()
}

/// 31 · Words typed beside a proposal's choice reach the author and are answered: its reply
/// reads above the choice, nothing comes back as « not taken », and the choice still waits.
/// `annuler` then discards the proposal at once, with no model call and no file written.
#[test]
fn a_proposal_choice_reads_the_words_beside_it_and_annuler_discards_it() {
    let author = Author::start(script());
    let rig = Rig::new("choice");
    std::fs::create_dir_all(rig.path("news")).expect("news");
    std::fs::write(rig.path(SOURCE), "# Tech\n- Rust ships a release.\n").expect("sources");
    let before = rig.tree();
    let base = author.base();
    let env = [
        ("NIKA_VLLM_BASE_URL", base.as_str()),
        ("NIKA_SESSION_DRIVER", "agent"),
    ];
    let mut term = rig.spawn_with("choice", &[], 120, 40, &env);
    wait_workspace(&mut term);

    term.send(REQUEST);
    term.send("\r");
    term.wait_until("the proposal's choice", |screen| {
        screen.contains(WHERE) && screen.contains("cancel discards")
    });

    let read = author.bodies().len();
    term.send(BESIDE);
    term.send("\r");
    term.wait_until("the author's reply above the choice", |screen| {
        screen.contains("je ne change rien") && screen.contains(WHERE)
    });
    term.settle();
    assert!(
        author.bodies().len() > read,
        "the words reached the author\n{}",
        term.dump()
    );
    assert!(
        !term.screen.contains("not taken"),
        "words the author answered are not restored\n{}",
        term.dump()
    );
    assert!(
        !composer_row(&term.screen).contains("je voulais"),
        "the draft is empty\n{}",
        term.dump()
    );

    let read = author.bodies().len();
    term.send("annuler");
    term.send("\r");
    term.wait_until("the proposal discarded", |screen| {
        screen.contains("discarded") && !screen.contains(WHERE)
    });
    term.settle();
    assert_eq!(
        author.bodies().len(),
        read,
        "annuler reached no model\n{}",
        term.dump()
    );
    // The person's bubbles read their own words, never the prompt they typed them after.
    assert!(
        term.screen.contains("│ annuler"),
        "the cancel word as typed\n{}",
        term.dump()
    );
    // The prompt of a reply, in either language the words beside the choice read in.
    assert!(
        !term.screen.contains("reply ›") && !term.screen.contains("réponse ›"),
        "no prompt in a bubble\n{}",
        term.dump()
    );
    assert_eq!(rig.tree(), before, "nothing was written");
    rig.no_run("a discarded proposal");
    term.leave();
}
