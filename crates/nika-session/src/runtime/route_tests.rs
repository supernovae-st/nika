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
use crate::reasoner::{NoReasoner, ScriptedReasoner};
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

#[test]
fn a_cancel_label_discards_the_proposal_without_granting_any_effect() {
    let (dir, mut s) = session_with(Box::new(Scripted(BTreeMap::from([(
        "Ne fais rien ; annule cette proposition.",
        TurnAct::parse("CANCEL"),
    )]))));
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("proposal");
    };
    let outcome = s.consent("Ne fais rien ; annule cette proposition.");
    assert!(
        matches!(outcome, TurnOutcome::Facts(ref text) if text.contains("discarded")),
        "{outcome:?}"
    );
    assert!(s.pending_proposal().is_none());
    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Refusal(_)));
    assert!(!dir.path().join(COPY_DEST).exists());
}

#[test]
fn a_cancel_label_drops_the_question_instead_of_binding_an_answer() {
    let (_dir, mut s) = session_with(Box::new(Scripted(BTreeMap::from([(
        "I no longer want this workflow.",
        TurnAct::parse("CANCEL"),
    )]))));
    assert!(matches!(s.turn("Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md"), TurnOutcome::Question { .. }));
    let outcome = s.turn("I no longer want this workflow.");
    assert!(
        matches!(outcome, TurnOutcome::Facts(ref text) if text.contains("discarded")),
        "{outcome:?}"
    );
    assert!(s.pending_question().is_none());
    assert!(s.pending_proposal().is_none());
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

/// A conversational reasoner cannot replace the original request with its
/// paraphrase when the compiler cannot revise it. Keep the original proposal
/// (and every obligation) rather than compiling a model-authored replacement.
#[test]
fn an_unsettled_revision_cannot_replace_the_original_with_a_paraphrase() {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(dir.path().join("notes/brief.md"), "brief\n").expect("brief");
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(
            IntelligenceKind::Local {
                provider: "mock".to_owned(),
            },
            DataLocus::None,
        ),
        Box::new(ScriptedReasoner::new(vec![
            // A plausible rewrite still cannot replace the original + change.
            "Read ./notes/brief.md and write it to ./out/copie-2.md".to_owned(),
        ])),
    );
    s.with_classifier(Box::new(corpus()));
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("original proposal");
    };
    let original = s.pending.as_ref().expect("pending").clone();
    let TurnOutcome::Held { id: held, preview } =
        s.consent("actually write it to ./out/copie-2.md instead")
    else {
        panic!("an unsupported revision keeps the original proposal");
    };
    assert_eq!(held, id);
    assert!(preview.contains("could not revise"), "{preview}");
    assert_eq!(s.pending.as_ref().expect("pending").goal, original.goal);
    assert_eq!(
        format!("{:?}", s.pending.as_ref().expect("pending").changes),
        format!("{:?}", original.changes),
        "a paraphrase may not replace the workflow or its obligations"
    );
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(ref t) if t.contains("applied")));
    let saved = std::fs::read_to_string(dir.path().join(COPY_DEST)).expect("saved");
    assert!(
        saved.contains("copy.md") && !saved.contains("copie-2.md"),
        "{saved}"
    );
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

/// The card when nothing could be built tells the two truths apart: a seat's
/// draft the fidelity check refused is an authoring failure (say it again,
/// another model), a deterministic reader's unsupported clause is a gap in
/// what Nika can express — and neither blames the human's words.
#[test]
fn the_cannot_build_card_tells_an_authoring_failure_from_a_language_gap() {
    let mut out = crate::authoring::compile_deterministic(
        &nika_onboard::compile::CompileRequest::create(super::tests::UNSETTLED),
    )
    .expect("compiles");
    let gap = super::authoring::cannot_express_text(&out);
    assert!(
        gap.starts_with("Nika cannot express this automation yet"),
        "{gap}"
    );
    assert!(
        gap.contains("what helps: say the outcome in one sentence"),
        "{gap}"
    );
    out.provenance.cognition = nika_onboard::compile::AuthoringCognition::ExplicitProvider;
    let failed = super::authoring::cannot_express_text(&out);
    assert!(
        failed.starts_with("Nika could not finish building this automation"),
        "{failed}"
    );
    assert!(
        failed.contains("send it again unchanged") && failed.contains("`/intelligence`"),
        "{failed}"
    );
    // An internal failure never asks the human to rewrite or split the request.
    for misleading in ["rephrase", "split the work", "describe the whole"] {
        assert!(
            !failed.contains(misleading),
            "never « {misleading} »: {failed}"
        );
    }
}

/// A cut answer is named precisely, without the command line's advice, as an internal limit.
#[test]
fn a_truncated_answer_is_named_as_an_internal_limit_not_the_request() {
    let said = super::authoring::human_reasons(vec![
        "The seat's answer was cut at the authoring cap (16384 output tokens): raise --authoring-max-tokens, or seat a model that does not spend the budget on its reasoning.".to_owned(),
    ]);
    assert_eq!(said.len(), 1);
    assert!(said[0].contains("16384-token output limit"), "{said:?}");
    assert!(
        said[0].contains("not a problem with your request"),
        "{said:?}"
    );
    assert!(!said[0].contains("--authoring-max-tokens"), "{said:?}");
}

/// A cloud model the catalog does not price is refused at the model
/// question, in words, with the priced models of its provider; a priced
/// cloud model and a local engine pass; a line that is not a model is left
/// to the compiler. At the question itself the round keeps waiting.
#[test]
fn an_unpriced_cloud_model_is_refused_at_the_question_in_words() {
    let text =
        super::authoring::unpriced_model_text("deepseek/deepseek-unpriced-v0").expect("unpriced");
    assert!(
        text.contains("not priced in Nika's catalog") && text.contains("NIKA-1709"),
        "{text}"
    );
    // The hint names models as the reasoner speaks them, never a catalog row id.
    assert!(
        text.contains("deepseek/deepseek-flash") && !text.contains(": deepseek/flash"),
        "{text}"
    );
    // The seat itself is never refused at the question, spacing aside.
    let own = crate::authoring::AuthoringSeat::Provider {
        model: "openai/gpt-oss-120b".to_owned(),
    };
    assert!(super::authoring::is_own_seat(&own, " openai/gpt-oss-120b "));
    assert!(!super::authoring::is_own_seat(&own, "openai/gpt-5.2"));
    let none = crate::authoring::AuthoringSeat::Deterministic { why: None };
    assert!(!super::authoring::is_own_seat(&none, "openai/gpt-oss-120b"));
    let priced_cloud = nika_catalog::all_providers()
        .iter()
        .filter(|p| p.requires_key)
        .find_map(|p| {
            p.models
                .iter()
                .find(|m| nika_catalog::find_pricing_scoped(p.id, m.model).is_some())
                .map(|m| format!("{}/{}", p.id, m.model))
        })
        .expect("the catalog prices at least one cloud model");
    assert!(
        super::authoring::unpriced_model_text(&priced_cloud).is_none(),
        "{priced_cloud}"
    );
    assert!(
        super::authoring::unpriced_model_text("ollama/llama3.1").is_none(),
        "local: unpriced by nature"
    );
    assert!(super::authoring::unpriced_model_text("mock/echo").is_none());
    assert!(super::authoring::unpriced_model_text("five lines").is_none());
    // At the question: refused in words, the round waits; a passing answer binds.
    let (_dir, mut s) = session_with(Box::new(corpus()));
    let TurnOutcome::Question { key, .. } = s.turn(
        "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md",
    ) else {
        panic!("the compiler asks for the model");
    };
    assert_eq!(key, "model");
    let TurnOutcome::Question { key, question } = s.turn("deepseek/deepseek-unpriced-v0") else {
        panic!("refused in words, still a question");
    };
    assert_eq!(key, "model");
    assert!(question.contains("not priced"), "{question}");
    assert!(s.pending_question().is_some(), "the question still waits");
    assert!(matches!(s.turn("mock/echo"), TurnOutcome::Proposal { .. }));
}

/// The compiler's fidelity grammar reaches the human in words: what the
/// draft lost, dropped or invented — never « Candidate 0 is not feasible ».
#[test]
fn a_compiler_reason_is_said_in_the_humans_words() {
    let said = super::authoring::human_reasons(vec![
        "Candidate 0 is not feasible: dropped the recognized operation `draft` (draft a short brief,).".to_owned(),
        "Candidate 0 is not feasible: the path `./tickets.json` is no longer carried by any operation or effect.".to_owned(),
        "Candidate 0 is not feasible: the literal `00` is not in the request.".to_owned(),
        "Unresolved clause: do something clever with it.".to_owned(),
    ]);
    assert_eq!(
        said[0],
        "the draft lost « draft a short brief » (the draft step)"
    );
    assert_eq!(
        said[1],
        "the draft dropped « ./tickets.json »: nothing reads or writes it any more"
    );
    assert_eq!(
        said[2],
        "the draft invented a value (« 00 ») your request never gave"
    );
    assert_eq!(said[3], "Unresolved clause: do something clever with it");
    assert!(said.iter().all(|s| !s.contains("Candidate")), "{said:?}");
}

/// The seat's offer under the model question says when the seat is not
/// priced — before Enter, with the priced models of its provider — and
/// never refuses it: the words are an offer, not a waiting question.
#[test]
fn the_seat_offer_says_before_enter_when_the_seat_is_unpriced() {
    let own = crate::authoring::AuthoringSeat::Provider {
        model: "deepseek/deepseek-unpriced-v0".to_owned(),
    };
    let offer = super::authoring::seat_offer(&own).expect("a provider seat is offered");
    assert!(
        offer
            .starts_with("Enter takes your seat `deepseek/deepseek-unpriced-v0` · or name another"),
        "{offer}"
    );
    assert!(
        offer.contains("`deepseek/deepseek-unpriced-v0` is not priced in Nika's catalog")
            && offer.contains("NIKA-1709")
            && offer.contains("priced for `deepseek`: deepseek/deepseek-flash"),
        "{offer}"
    );
    assert!(
        !offer.contains("the question still waits"),
        "an offer, never a refusal: {offer}"
    );
    let priced = crate::authoring::AuthoringSeat::Provider {
        model: "deepseek/deepseek-flash".to_owned(),
    };
    let offer = super::authoring::seat_offer(&priced).expect("offered");
    assert!(!offer.contains("not priced"), "{offer}");
    let local = crate::authoring::AuthoringSeat::Provider {
        model: "ollama/llama3.1".to_owned(),
    };
    assert!(
        !super::authoring::seat_offer(&local)
            .expect("offered")
            .contains("not priced"),
        "local: unpriced by nature"
    );
    let none = crate::authoring::AuthoringSeat::Deterministic { why: None };
    assert!(super::authoring::seat_offer(&none).is_none());
}

// ---- one decision grammar, local commands, French run lines ---------------

/// Counts every classification: a local command must never reach the route
/// (the classifier stands where the session's model would read the line).
struct Counting(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl TurnClassifier for Counting {
    fn classify(&mut self, _context: &TurnContext, _raw: &str) -> TurnDecision {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        TurnDecision::new(TurnAct::Unknown, RoutingMethod::Model)
    }
}

fn counted() -> (
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    tempfile::TempDir,
    SessionRuntime,
) {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (dir, s) = session_with(Box::new(Counting(std::sync::Arc::clone(&calls))));
    (calls, dir, s)
}

fn classified(calls: &std::sync::Arc<std::sync::atomic::AtomicUsize>) -> usize {
    calls.load(std::sync::atomic::Ordering::SeqCst)
}

/// The Session's cost choice and the Run's cost decision read ONE
/// grammar. A whole-line yes approves (English or French); a refusal may lead
/// a longer line; anything else is unknown and is asked again — never a yes.
#[test]
fn one_decision_grammar_reads_english_and_french_and_never_guesses_a_yes() {
    for yes in ["yes", "YES", "y", "oui", "Oui !", "OUI", "ok", "ok."] {
        assert_eq!(decision_answer(yes), DecisionAnswer::Approve, "{yes}");
    }
    for no in [
        "no",
        "non",
        "NON",
        "n",
        "Non.",
        "cancel",
        "non, finalement pas maintenant",
        "no thanks",
    ] {
        assert_eq!(decision_answer(no), DecisionAnswer::Decline, "{no}");
    }
    for details in ["details", "DETAILS", "/details", "/why", "why?"] {
        assert_eq!(
            decision_answer(details),
            DecisionAnswer::Details,
            "{details}"
        );
    }
    for unknown in [
        "",
        "   ",
        "peut-être",
        "c'est payant ?",
        "yes please",
        "yes?",
        "yes, but only once",
        "yes no",
        "okay",
        "yep",
        "sure",
        "oui oui",
        "oui mais pas maintenant",
        "run it",
        "1",
        "/status",
        "/help",
    ] {
        assert_eq!(
            decision_answer(unknown),
            DecisionAnswer::Unknown,
            "{unknown:?}"
        );
    }
}

/// The French imperative with its object pronoun is the same run
/// verb as the English one; words that merely start alike are not.
#[test]
fn french_imperatives_with_their_pronoun_are_run_verbs() {
    for verb in [
        "run",
        "run:",
        "lance",
        "lance-le",
        "lance-la",
        "lance-les",
        "exécute",
        "exécute-le",
        "exécute-la",
        "relance",
        "relance-le",
        "teste-le",
        "lance-le.",
    ] {
        assert!(super::authoring::is_run_verb(verb), "{verb}");
    }
    for not_run in [
        "lancement",
        "lance-toi",
        "launch",
        "running",
        "le",
        "exécuter",
        "",
    ] {
        assert!(!super::authoring::is_run_verb(not_run), "{not_run}");
    }
}

/// `/help`, `/status`, `/details`, `/why` typed while a proposal
/// waits answer from the session's own facts: the proposal is kept, nothing
/// is written, and the line never reaches the route (the model's seat).
#[test]
fn slash_commands_beside_a_proposal_stay_local_and_keep_it() {
    let (calls, dir, mut s) = counted();
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("proposal");
    };
    let before = classified(&calls);
    assert!(
        matches!(s.consent("/help"), TurnOutcome::Help(ref card) if card.contains("/details")),
        "/help at apply? is the help card"
    );
    assert!(
        matches!(s.consent("/status"), TurnOutcome::Facts(ref t) if t.starts_with("session\n")),
        "/status at apply? is the status card"
    );
    assert!(
        matches!(s.consent("/details"), TurnOutcome::Facts(_)),
        "/details at apply? is the local details card"
    );
    assert!(
        matches!(s.consent("/why"), TurnOutcome::Aside(ref t) if t.contains("the proposal still waits")),
        "/why at apply? explains the proposal"
    );
    assert_eq!(
        classified(&calls),
        before,
        "a local command was routed like open language"
    );
    assert_eq!(
        s.pending_proposal().as_ref(),
        Some(&id),
        "the proposal was lost"
    );
    assert!(
        !dir.path().join(COPY_DEST).exists(),
        "a local command wrote the proposal"
    );
    assert!(
        matches!(s.consent("no"), TurnOutcome::Facts(ref t) if t.contains("discarded")),
        "the proposal still takes its own answer afterwards"
    );
}

/// A slash line typed at a paused human gate is never its answer:
/// it answers locally and the gate keeps waiting for the human's own answer.
#[test]
fn a_slash_command_is_never_the_answer_to_a_gate() {
    const GATE: &str = "nika: gate\npermits: { fs: { read: [\"./draft.md\"], write: [\"./final.md\"] }, tools: [\"nika:read\", \"nika:prompt\", \"nika:write\"] }\ntasks:\n  read_draft:\n    invoke: { tool: \"nika:read\", args: { path: \"./draft.md\" } }\n  approve:\n    invoke: { tool: \"nika:prompt\", args: { mode: confirm, message: \"Write final.md?\" } }\n  write_final:\n    after: { approve: success }\n    with: { go: \"${{ tasks.approve.output }}\", text: \"${{ tasks.read_draft.output }}\" }\n    when: \"${{ with.go == true }}\"\n    invoke: { tool: \"nika:write\", args: { path: \"./final.md\", content: \"${{ with.text }}\" } }\n";
    const PAUSED: &str = "{\"kind\":\"workflow_paused\",\"fields\":[{\"key\":\"task\",\"value\":\"approve\"},{\"key\":\"mode\",\"value\":\"confirm\"},{\"key\":\"message\",\"value\":\"Write final.md?\"}]}\n";
    let (calls, dir, mut s) = counted();
    std::fs::write(dir.path().join("draft.md"), "the draft\n").expect("draft");
    std::fs::write(dir.path().join("gate.nika"), GATE).expect("gate");
    assert!(matches!(
        s.turn("run gate.nika"),
        TurnOutcome::RunRequested { .. }
    ));
    let store = dir.path().join(".nika").join("traces");
    std::fs::create_dir_all(&store).expect("store");
    let trace = store.join("paused.ndjson");
    std::fs::write(&trace, PAUSED).expect("trace");
    assert!(matches!(
        s.observe_run(4, Some(&trace)),
        TurnOutcome::GateAsk { .. }
    ));
    let before = classified(&calls);
    for line in ["/status", "/help", "/details"] {
        let outcome = s.answer_gate(line);
        assert!(
            !matches!(outcome, TurnOutcome::ResumeRequested { .. }),
            "{line} answered the gate: {outcome:?}"
        );
        assert!(
            s.waiting_gate().is_some(),
            "{line}: the gate stopped waiting"
        );
    }
    assert_eq!(classified(&calls), before, "a slash line was routed");
    assert!(
        matches!(s.answer_gate("yes"), TurnOutcome::ResumeRequested { .. }),
        "the human's own answer still resumes the run"
    );
}

/// « lance-le » after a saved workflow is a run line: it reaches
/// the existing run gate (check on disk, the money, then the door's fresh Run
/// decision), exactly like « run it » — never the model, never a conversation.
#[test]
fn lance_le_reaches_the_same_run_gate_as_run_it() {
    let (calls, dir, mut s) = counted();
    let TurnOutcome::Proposal { .. } = s.turn(COPY) else {
        panic!("proposal");
    };
    let _ = s.consent("yes");
    assert!(dir.path().join(COPY_DEST).exists(), "the saved workflow");
    let before = classified(&calls);
    let french = s.turn("lance-le");
    assert!(
        matches!(french, TurnOutcome::RunRequested { .. }),
        "« lance-le » is a run request: {french:?}"
    );
    assert_eq!(classified(&calls), before, "« lance-le » was routed");
}

/// A declined Run is not an observed exit: nothing ran, the status
/// keeps the last real run, and the interrupted exit (130) is named as such.
#[test]
fn a_declined_run_is_typed_not_run_and_130_is_an_interruption() {
    let (_calls, _dir, mut s) = counted();
    let TurnOutcome::Proposal { .. } = s.turn(COPY) else {
        panic!("proposal");
    };
    let _ = s.consent("yes");
    let before = s.status_line();
    let TurnOutcome::Facts(text) = s.observe_declined_run() else {
        panic!("a declined run is a fact");
    };
    assert!(text.starts_with("not run · "), "{text}");
    assert!(text.contains("nothing sent, nothing written"), "{text}");
    assert!(
        !text.contains("exit") && !text.contains("unknown code"),
        "{text}"
    );
    assert_eq!(s.status_line(), before, "a decline changed the run status");
    assert!(matches!(s.turn("run it"), TurnOutcome::RunRequested { .. }));
    let _ = s.observe_run(130, None);
    assert!(
        s.status_line()
            .starts_with("Stopped · the run was interrupted"),
        "{}",
        s.status_line()
    );
}
