// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The judge of a document an intelligence wrote in a Session's conversation: a selection the
//! author states is admitted on the person's own words, one the Session verified itself (an offer
//! it showed) is judged for its scope and use only and is never the author's to state, a value no
//! selection covers is an invented literal, and a document that does not parse is refused before
//! any law. Nothing is called.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;

use nika_compile_cognition::judge_document;
use nika_compile_fidelity::fidelity::resolution::Resolution;
use serde_json::{Value, json};

const HACKER_NEWS: &str = "https://news.ycombinator.com";
const DIGEST: &str = "./news/digest.md";
/// A request that delegates the sources and leaves the output name to the author.
const DELEGATING: &str = "recupere les news tech recentes, resume les et ecris le resume en markdown dans un dossier du projet. les sources publiques tu les choisis toi meme";
const DELEGATION: &str = "les sources publiques tu les choisis toi meme";
const OUTPUT_WORDS: &str = "ecris le resume en markdown dans un dossier du projet";
/// A request that names neither: the Session offered both, the person accepted.
const OPEN: &str = "fais moi un workflow tres simple qui recupere les news tech recentes, les resume et ecrit le resultat en markdown dans un dossier du projet\noui tout me va";

/// A digest workflow: one GET, one summary, one write.
fn digest(url: &str, output: &str) -> String {
    let host = url.split_once("://").map_or(url, |(_, rest)| rest);
    let mut source = format!(
        "nika: news-digest\nmodel: mock/echo\npermits:\n  tools: [\"nika:fetch\", \"nika:write\"]\n  net:\n    http: [\"{host}\"]\n  fs:\n    write: [\"{output}\"]\ntasks:\n"
    );
    write!(
        source,
        "  news:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"{url}\", method: GET }}\n"
    )
    .unwrap();
    source.push_str("  summarize:\n    with:\n      news: \"${{ tasks.news.output }}\"\n");
    source.push_str("    infer:\n      max_tokens: 1000\n      prompt: \"Résume en Markdown les actualités ci-dessous, sans rien inventer : ${{ with.news }}\"\n");
    write!(
        source,
        "  write_digest:\n    with:\n      digest: \"${{{{ tasks.summarize.output }}}}\"\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"{output}\", content: \"${{{{ with.digest }}}}\" }}\n"
    )
    .unwrap();
    source
}

fn rows(values: &[Value]) -> Vec<Resolution> {
    Resolution::read_all(values).unwrap()
}

fn offered() -> Vec<Resolution> {
    let offer = |value: &str, role: &str| {
        json!({"value": value, "kind": "offered", "role": role, "message": "u2",
            "question": "plan", "option": "recommended"})
    };
    rows(&[
        offer(HACKER_NEWS, "read_source"),
        offer(DIGEST, "output_path"),
    ])
}

#[test]
fn selections_on_the_persons_own_words_let_the_document_stand() {
    let authored = rows(&[
        json!({"value": HACKER_NEWS, "kind": "delegated", "role": "read_source",
            "excerpt": DELEGATION}),
        json!({"value": DIGEST, "kind": "derived", "role": "output_path",
            "excerpt": OUTPUT_WORDS}),
    ]);
    let refusals = judge_document(
        DELEGATING,
        &digest(HACKER_NEWS, DIGEST),
        (&authored, &[]),
        None,
    );
    assert!(refusals.is_empty(), "{refusals:?}");
}

#[test]
fn a_value_no_selection_covers_is_an_invented_literal() {
    let refusals = judge_document(DELEGATING, &digest(HACKER_NEWS, DIGEST), (&[], &[]), None);
    assert!(
        refusals
            .iter()
            .any(|r| r.starts_with("INVENTED LITERAL") && r.contains("news.ycombinator.com")),
        "{refusals:?}"
    );
}

#[test]
fn an_offer_the_session_verified_is_its_own_to_state_never_the_authors() {
    let document = digest(HACKER_NEWS, DIGEST);
    let hosted = judge_document(OPEN, &document, (&[], &offered()), None);
    assert!(hosted.is_empty(), "{hosted:?}");
    let claimed = judge_document(OPEN, &document, (&offered(), &[]), None);
    assert!(
        claimed
            .iter()
            .any(|r| r.starts_with("UNAUTHORIZED SELECTION") && r.contains(HACKER_NEWS)),
        "{claimed:?}"
    );
}

#[test]
fn a_document_that_does_not_parse_is_refused_before_any_law() {
    let refusals = judge_document(DELEGATING, "nika: broken\ntasks: [", (&[], &[]), None);
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert!(refusals[0].contains("does not parse"), "{refusals:?}");
}
