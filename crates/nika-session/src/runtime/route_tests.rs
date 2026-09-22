// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The semantic route of open language: the typed state and the raw line
//! decide the act through a bounded classifier, never a word list. These
//! tests prove the ROUTING ARCHITECTURE handles the sentences (a scripted
//! classifier stands in for the intelligence, with the adversarial corpus
//! the routing addendum names) — not that a lexicon holds every word.

use std::collections::BTreeMap;

use super::tests::{COPY, COPY_DEST, ready, tree};
use super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::reasoner::NoReasoner;
use crate::turn::{
    ConservativeFallback, RoutingMethod, SessionPhase, TurnAct, TurnClassifier, TurnContext,
    TurnDecision,
};

/// A classifier scripted by sentence — the stand-in for a decision seat
/// or the session's intelligence; UNKNOWN for anything it was not told.
struct Scripted(BTreeMap<&'static str, TurnAct>);

impl TurnClassifier for Scripted {
    fn classify(&mut self, _context: &TurnContext, raw: &str) -> TurnDecision {
        let act = self.0.get(raw.trim()).copied().unwrap_or(TurnAct::Unknown);
        TurnDecision::new(act, RoutingMethod::Model)
    }
}

/// The routing addendum's corpus: sentences whose first word lies about
/// their act — a DISCUSS that starts like a command, a MODIFY that starts
/// like a question, a mixed line with a yes in it, in two languages.
fn corpus() -> Scripted {
    Scripted(BTreeMap::from([
        ("does it write anything", TurnAct::Discuss),
        ("qu'est-ce que ça lit exactement", TurnAct::Discuss),
        ("can you explain what it writes?", TurnAct::Discuss),
        ("Can it write outside the project?", TurnAct::Discuss),
        ("Tell me what this sends.", TurnAct::Discuss),
        ("Est-ce que ça écrit quelque part ?", TurnAct::Discuss),
        (
            "actually write it to ./out/copie-2.md instead",
            TurnAct::Modify,
        ),
        ("what I actually want is ./out/final.md", TurnAct::Modify),
        (
            "can you write it to ./out/final.md instead?",
            TurnAct::Modify,
        ),
        (
            "What I actually want is the Tuesday report only.",
            TurnAct::Modify,
        ),
        (
            "Can you change the output to ./out/report.md?",
            TurnAct::Modify,
        ),
        ("Tell it to send only on Friday.", TurnAct::Modify),
        (
            "Tu peux le faire seulement du mardi au vendredi ?",
            TurnAct::Modify,
        ),
        ("En fait écris-le dans ./out/report.md", TurnAct::Modify),
        ("Looks good except don't send it.", TurnAct::Modify),
        (
            "Ça a l'air bon sauf que je veux rien envoyer",
            TurnAct::Modify,
        ),
        ("Oui mais enlève Slack avant", TurnAct::Mixed),
        ("Run it, but only on Fridays.", TurnAct::Mixed),
        ("run it", TurnAct::RequestRun),
        ("build me a digest of the docs", TurnAct::NewWork),
        ("mock/echo", TurnAct::Answer),
    ]))
}

fn session_with(classifier: Box<dyn TurnClassifier>) -> (tempfile::TempDir, SessionRuntime) {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(dir.path().join("notes/brief.md"), "brief\n").expect("brief");
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(IntelligenceKind::None, DataLocus::None),
        Box::new(NoReasoner),
    );
    s.with_classifier(classifier);
    (dir, s)
}

/// The addendum's first milestone: a proposal waits; a question does not
/// change it; a change is a revision (the raw words, the proposal kept
/// when the revision cannot settle); a MODIFY that starts with « what »
/// and a DISCUSS that starts with « can » are told apart by the route,
/// never by their first word; a near-identical shape goes the other way.
#[test]
fn the_route_tells_a_question_from_a_change_whatever_the_first_word() {
    let (dir, mut s) = session_with(Box::new(corpus()));
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("a proposal");
    };
    // DISCUSS: the proposal held, nothing revised.
    for question in [
        "does it write anything",
        "qu'est-ce que ça lit exactement",
        "can you explain what it writes?",
        "Can it write outside the project?",
    ] {
        let TurnOutcome::Held { id: held, preview } = s.consent(question) else {
            panic!("{question}: the proposal still waits");
        };
        assert_eq!(held, id, "{question}");
        assert!(preview.contains("still waits"), "{question}: {preview}");
        assert!(
            !preview.contains("could not revise"),
            "{question}: a question never revises"
        );
    }
    // MODIFY, however phrased: the revision is asked with the raw words;
    // without a seat the edit door cannot settle it, the proposal waits.
    for change in [
        "actually write it to ./out/copie-2.md instead",
        "what I actually want is ./out/final.md",
        "can you write it to ./out/final.md instead?",
        "Can you change the output to ./out/report.md?",
        "Looks good except don't send it.",
    ] {
        let TurnOutcome::Held { id: held, preview } = s.consent(change) else {
            panic!("{change}: the proposal still waits after a revision that could not settle");
        };
        assert_eq!(held, id, "{change}");
        assert!(
            preview.contains("could not revise") && preview.contains(change.trim()),
            "{change}: the raw words are kept: {preview}"
        );
        assert!(
            !preview.to_ascii_lowercase().contains("rephrase")
                && !preview.to_ascii_lowercase().contains("jq"),
            "{change}: no machine-owned words: {preview}"
        );
    }
    // MIXED: a yes in it is never a consent — the change first.
    let TurnOutcome::Held { preview, .. } = s.consent("Oui mais enlève Slack avant") else {
        panic!("a mixed line is never a consent");
    };
    assert!(preview.contains("could not revise"), "{preview}");
    assert!(!dir.path().join(COPY_DEST).exists(), "nothing applied");
    // REQUEST_RUN at the consent prompt: consent is never a run.
    assert!(
        matches!(s.consent("run it"), TurnOutcome::Held { ref preview, .. } if preview.contains("consent is never a run"))
    );
    // The routes are recorded, by hash, never the words.
    let routes = s.routes();
    assert!(routes.len() >= 10, "{routes:?}");
    assert!(
        routes
            .iter()
            .all(|r| r.phase == SessionPhase::ProposalPending && r.raw_hash.len() == 12)
    );
    assert!(routes.iter().any(|r| r.act == TurnAct::Modify));
    assert!(routes.iter().any(|r| r.act == TurnAct::Discuss));
    // A protocol token still applies — authority stays with the protocol.
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(ref t) if t.contains("applied")));
    assert!(dir.path().join(COPY_DEST).exists());
    // The routes are instrumented: `/details` lists them (phase · act · how · hash).
    let details = s.details();
    assert!(
        details.contains("routes:") && details.contains("MODIFY") && details.contains("Model"),
        "{details}"
    );
}

/// No intelligence to route: an open line at the consent prompt is
/// UNKNOWN — the proposal kept, the words not lost, the protocol forms
/// named; nothing is guessed, nothing is applied.
#[test]
fn without_intelligence_an_open_line_keeps_everything_and_says_so() {
    let (dir, mut s) = session_with(Box::new(ConservativeFallback));
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("a proposal");
    };
    let TurnOutcome::Held { id: held, preview } =
        s.consent("actually write it to ./out/copie-2.md instead")
    else {
        panic!("held");
    };
    assert_eq!(held, id);
    assert!(
        preview.contains("not a consent")
            && preview.contains("no intelligence is available to read what it means")
            && preview.contains("nothing changed")
            && preview.contains("`yes` applies the proposal"),
        "{preview}"
    );
    assert!(!dir.path().join(COPY_DEST).exists());
    assert_eq!(
        s.routes().last().map(|r| r.method),
        Some(RoutingMethod::Fallback)
    );
    // Engine facts still answer without any intelligence (zero tokens).
    assert!(
        matches!(s.consent("what workflows are here?"), TurnOutcome::Held { ref preview, .. } if preview.contains("alpha.nika")),
        "an engine fact answers before any route"
    );
}

/// The run grammar is closed: « run it » runs; « Run it, but only on
/// Fridays. » is not a run — its act is routed and a change comes first.
#[test]
fn a_run_line_with_a_change_in_it_is_not_a_run() {
    let (dir, mut s) = session_with(Box::new(corpus()));
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(_)));
    let outcome = s.turn("Run it, but only on Fridays.");
    assert!(
        matches!(outcome, TurnOutcome::Refusal(ref r) if r.text.contains("a run with a change in it")),
        "{outcome:?}"
    );
    assert!(
        !dir.path().join(".nika").join("traces").exists(),
        "nothing ran"
    );
    assert!(
        matches!(s.turn("run it"), TurnOutcome::RunRequested { .. }),
        "the closed grammar still runs"
    );
}

/// At a question: a question about the question explains it and it still
/// waits; without intelligence a line is the answer (the fallback binds).
#[test]
fn at_a_question_a_question_explains_and_the_fallback_binds() {
    let (_dir, mut s) = session_with(Box::new(corpus()));
    let intent = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";
    let TurnOutcome::Question { key, .. } = s.turn(intent) else {
        panic!("the compiler asks for the model");
    };
    assert_eq!(key, "model");
    let TurnOutcome::Aside(text) = s.turn("Tell me what this sends.") else {
        panic!("a question about the question is an aside");
    };
    assert!(text.contains("This answer fills `model`"), "{text}");
    assert!(s.pending_question().is_some(), "the question still waits");
    // An ANSWER binds to the pending key: the question is answered.
    assert!(matches!(s.turn("mock/echo"), TurnOutcome::Proposal { .. }));
    // Without any intelligence the fallback binds a line as the answer too
    // (a short line at a question is the answer more often than not).
    let (_dir2, mut f) = session_with(Box::new(ConservativeFallback));
    assert!(matches!(f.turn(intent), TurnOutcome::Question { .. }));
    // …except a line that ends with `?`: the hint decides only here, when
    // nothing could judge the line — the question is explained, not bound.
    let TurnOutcome::Aside(text) = f.turn("what happens if I leave it empty?") else {
        panic!("an unread question is asked, not bound");
    };
    assert!(text.contains("This answer fills `model`"), "{text}");
    assert!(f.pending_question().is_some(), "the question still waits");
    assert!(matches!(f.turn("mock/echo"), TurnOutcome::Proposal { .. }));
}

/// A request the deterministic reader called « not work » but the route
/// calls `NEW_WORK` (a French total over a CSV, files named) is work: with no
/// intelligence chosen yet the first screen is asked in context and the
/// line is kept — never answered as conversation with a plan nothing builds.
#[test]
fn new_work_the_reader_missed_asks_for_an_intelligence_and_keeps_the_line() {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join("data")).expect("data");
    std::fs::write(
        dir.path().join("data/ventes.csv"),
        "date,montant,statut\n2026-09-01,10,payé\n",
    )
    .expect("csv");
    let line = "Fais-moi le total de ce qu'on a encaissé dans ./data/ventes.csv (uniquement les ventes payées) et mets ça dans ./out/total.md";
    let mut s = SessionRuntime::open_unchosen(
        dir.path(),
        crate::intelligence::IntelligenceCensus {
            seats: vec![],
            api_keys: vec![],
            locals: vec![],
        },
        None,
        Box::new(|_| Box::new(NoReasoner)),
    );
    s.with_classifier(Box::new(Scripted(BTreeMap::from([(
        line,
        TurnAct::NewWork,
    )]))));
    let out = s.turn(line);
    assert!(
        s.pending_choice(),
        "the first screen is asked in context: {out:?}"
    );
}
