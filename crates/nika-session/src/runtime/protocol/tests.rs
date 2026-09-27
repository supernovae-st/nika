// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Session routing truth (V9 P1 · BUG-U6 · S02 · S05b of the 2026-09-27 black-box audit):
//! a command-shaped line is the protocol's — never work, an answer, a consent or a gate's
//! answer; a read-only ask is answered from the machine's own state with nothing routed,
//! read, spent or changed; UNKNOWN binds nothing. Stateful sequences: every read-only line
//! keeps the proposal's identity and bytes and the question's identity; the protocol still
//! takes its real answers afterwards, and a genuine revision still invalidates the old consent.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::tests::{COPY, COPY_DEST, ready, tree};
use super::super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::reasoner::{NoReasoner, ReasonError, Reply, ScriptedReasoner};
use crate::turn::{
    ConservativeFallback, RoutingMethod, TurnAct, TurnClassifier, TurnContext, TurnDecision,
};

/// An intent whose model the compiler asks for (a deterministic question, zero calls).
const DRAFT: &str = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";

/// A workflow that declares a required input: a run asks for it first.
const NEEDS_INPUT: &str = "nika: greet\ninputs:\n  name: { type: string, required: true }\npermits:\n  fs: { write: [\"./hello.txt\"] }\n  tools: [\"nika:write\"]\ntasks:\n  write:\n    invoke: { tool: \"nika:write\", args: { path: \"./hello.txt\", content: \"hello ${{ inputs.name }}\" } }\n";

/// The door's classifier, scripted by sentence and counting every call (a call stands where
/// the model would read the line): UNKNOWN for a sentence it was not told, or a FAILED route.
struct Scripted {
    acts: BTreeMap<&'static str, TurnAct>,
    calls: Arc<AtomicUsize>,
    fail: bool,
}

impl TurnClassifier for Scripted {
    fn classify(&mut self, _context: &TurnContext, raw: &str) -> TurnDecision {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return TurnDecision::failed("the provider did not answer (HTTP 503)");
        }
        let act = self
            .acts
            .get(raw.trim())
            .copied()
            .unwrap_or(TurnAct::Unknown);
        TurnDecision::new(act, RoutingMethod::Model)
    }
}

/// The chosen intelligence, counting every reading and every conversational call.
struct Counted(Arc<AtomicUsize>);

impl SessionReasoner for Counted {
    fn name(&self) -> String {
        "counted".to_owned()
    }

    fn reason(&mut self, _prompt: &str) -> Result<Reply, ReasonError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Reply {
            text: "words".to_owned(),
            usage_observed: false,
        })
    }
}

/// What a test observes: the root, the routes asked of the classifier, the model's calls.
struct World {
    dir: tempfile::TempDir,
    routed: Arc<AtomicUsize>,
    read: Arc<AtomicUsize>,
    s: SessionRuntime,
}

impl World {
    fn routed(&self) -> usize {
        self.routed.load(Ordering::SeqCst)
    }

    fn read(&self) -> usize {
        self.read.load(Ordering::SeqCst)
    }
}

/// A session whose chosen local intelligence answers, its routes scripted by `acts`.
fn world(acts: &[(&'static str, TurnAct)], fail: bool) -> World {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(dir.path().join("notes/brief.md"), "brief\n").expect("brief");
    let routed = Arc::new(AtomicUsize::new(0));
    let read = Arc::new(AtomicUsize::new(0));
    let local = ready(
        IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        DataLocus::Local,
    );
    let mut s = SessionRuntime::open(dir.path(), local, Box::new(Counted(Arc::clone(&read))));
    s.with_classifier(Box::new(Scripted {
        acts: acts.iter().copied().collect(),
        calls: Arc::clone(&routed),
        fail,
    }));
    World {
        dir,
        routed,
        read,
        s,
    }
}

/// The exact bytes of the proposal that waits.
fn bytes(s: &SessionRuntime) -> Vec<String> {
    s.pending.as_ref().map_or_else(Vec::new, |set| {
        set.changes.iter().map(|c| c.content().to_owned()).collect()
    })
}

/// The semantic state no read-only line may write (the P1 mutation digest): the goal, the
/// decisions and open questions, the authoring round, the proposal's identity and bytes, the
/// revision set aside, the money bound to them, the gate, a run's inputs, the activation, the
/// cost review, the question's identity and the last decided proposal — never the transcript,
/// the routes, the activity, the recovery card or the cost observations.
fn semantic(s: &SessionRuntime) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        s.intent,
        s.authoring,
        s.pending_proposal(),
        s.pending,
        s.revising,
        (
            &s.money.pending,
            &s.money.draft,
            &s.money.current,
            &s.money.gate
        ),
        s.pending_gate,
        s.run_inputs,
        s.activation,
        s.cost_choice_details(),
        s.pending_question_id(),
        s.decided,
        s.last_outcome.as_ref().map(|out| &out.candidate),
    )
}

/// The answers the open round has bound so far.
fn answers(s: &SessionRuntime) -> BTreeMap<String, String> {
    s.authoring
        .as_ref()
        .map_or_else(BTreeMap::new, |round| round.answers.clone())
}

fn refused_with(out: &TurnOutcome, says: &str) -> bool {
    matches!(out, TurnOutcome::Refusal(r)
        if r.class == RefusalClass::WrongState && r.text.contains(says) && r.text.contains("nothing was sent"))
}

/// BUG-U6: a slash typo at the prompt never reaches the compiler, a model or a review — an
/// unknown name is refused with the nearest known ones; a known name with words after it,
/// or one that does not apply here, is said so; nothing is sent and nothing changes.
#[test]
fn a_command_shaped_line_is_refused_before_any_route_reading_or_work() {
    let mut w = world(&[], false);
    for (line, says) in [
        ("/bogus", "unknown command `/bogus`"),
        ("/Help", "did you mean `/help`"),
        ("/stat", "did you mean `/status`"),
        ("/meening", "did you mean `/meaning`"),
        ("/why?", "type `/why` alone"),
        ("/meaning please", "type `/meaning` alone"),
        ("/show", "`/show` does not apply here"),
        ("/cancel", "`/cancel` does not apply here"),
        ("/", "needs a command name"),
    ] {
        let out = w.s.turn(line);
        assert!(refused_with(&out, says), "{line}: {out:?}");
        assert!(
            out_text(&out).contains("/help"),
            "{line}: names the card: {out:?}"
        );
    }
    assert_eq!(w.routed(), 0, "a command-shaped line was routed");
    assert_eq!(w.read(), 0, "a command-shaped line reached the model");
    assert!(
        w.s.intent.goal.is_none(),
        "a typo became the automation's goal"
    );
    assert!(w.s.authoring.is_none() && w.s.pending.is_none() && !w.s.waiting_cost_choice());
    assert!(w.s.routes().is_empty() && w.s.recent.is_empty());
    // A path is never a command: the line keeps going where it went before.
    for path in ["/tmp/notes.md", "/notes.md", "/Users/me"] {
        assert!(!refused_with(&w.s.turn(path), "command"), "{path}");
    }
}

/// A command is `/` and a name, its trailing punctuation aside; a path is never one, and a
/// line that does not start with `/` is never one, whatever it carries later.
#[test]
fn a_path_is_never_a_command() {
    for (line, word) in [
        ("/bogus", "/bogus"),
        ("/why?", "/why"),
        (" /meaning please", "/meaning"),
        ("/", "/"),
    ] {
        assert_eq!(super::command_word(line), Some(word), "{line}");
    }
    for line in [
        "/tmp/a.csv",
        "/notes.md",
        "/Users/me",
        "./notes",
        "why",
        "hello /bogus",
        "",
    ] {
        assert_eq!(super::command_word(line), None, "{line}");
        assert_eq!(super::unserved_command(line), None, "{line}");
    }
}

fn out_text(out: &TurnOutcome) -> &str {
    match out {
        TurnOutcome::Refusal(r) => &r.text,
        TurnOutcome::Held { preview, .. } => preview,
        TurnOutcome::Aside(text)
        | TurnOutcome::Facts(text)
        | TurnOutcome::Question { question: text, .. } => text,
        _ => "",
    }
}

/// S05b and the explain gesture: at the consent prompt every read-only line — `why` as
/// `/why`, « what happened? », a command this prompt does not serve — answers from the
/// machine's own state: the proposal's identity and bytes stay, nothing is routed or read,
/// and the proposal still takes its own `yes`.
#[test]
fn read_only_lines_beside_a_proposal_keep_its_identity_and_bytes() {
    let mut w = world(&[], false);
    let TurnOutcome::Proposal { id, .. } = w.s.turn(COPY) else {
        panic!("a proposal");
    };
    let proposed = bytes(&w.s);
    let state = semantic(&w.s);
    for line in [
        "why",
        "why?",
        "explain",
        "pourquoi",
        "what happened?",
        "what happened",
        "/last",
        "/bogus",
        "/cancel",
        "/intelligence",
        "/meaning",
        "/status",
    ] {
        let out = w.s.consent(line);
        assert_eq!(semantic(&w.s), state, "{line} wrote semantic state");
        let refused =
            matches!(&out, TurnOutcome::Refusal(r) if r.class == RefusalClass::WrongState);
        let served = matches!(line, "/meaning" | "/status");
        let beside = matches!(out, TurnOutcome::Aside(_)) || (refused && line.starts_with('/'));
        assert!(
            served || (beside && out_text(&out).contains("still waits")),
            "{line}: {out:?}"
        );
        assert_eq!(w.s.pending_proposal().as_ref(), Some(&id), "{line}");
        assert_eq!(bytes(&w.s), proposed, "{line}: the candidate changed");
    }
    assert_eq!(
        w.routed(),
        0,
        "a read-only line was routed like open language"
    );
    assert_eq!(w.read(), 0, "a read-only line reached the model");
    assert!(w.s.routes().is_empty());
    assert!(!w.dir.path().join(COPY_DEST).exists(), "nothing applied");
    assert!(
        matches!(w.s.consent_to(&id, "yes"), TurnOutcome::Facts(ref t) if t.contains("applied")),
        "the same identity still takes its consent"
    );
    assert!(w.dir.path().join(COPY_DEST).exists());
}

/// S02: at an open question, `why`, « what happened? » and a command bind nothing and are never
/// routed: the question keeps its identity and still takes its answer afterwards — and an
/// answered question answers nothing again.
#[test]
fn read_only_lines_at_a_question_keep_it_and_bind_nothing() {
    let mut w = world(&[("mock/echo", TurnAct::Answer)], false);
    let TurnOutcome::Question { key, .. } = w.s.turn(DRAFT) else {
        panic!("the compiler asks for the model");
    };
    assert_eq!(key, "model");
    let asked = w.s.pending_question_id().expect("the question waits");
    let state = semantic(&w.s);
    for line in [
        "why",
        "why?",
        "what happened?",
        "/last",
        "quoi",
        "/bogus",
        "/show",
        "/meaning",
        "/details",
    ] {
        let out = w.s.turn(line);
        assert_eq!(semantic(&w.s), state, "{line} wrote semantic state");
        assert!(
            !matches!(out, TurnOutcome::Proposal { .. }),
            "{line}: {out:?}"
        );
        assert_eq!(
            w.s.pending_question_id(),
            Some(asked.clone()),
            "{line}: {out:?}"
        );
        assert!(answers(&w.s).is_empty(), "{line} was bound: {out:?}");
    }
    assert_eq!(w.routed(), 0, "a read-only line was routed");
    assert_eq!(w.read(), 0, "a read-only line reached the model");
    assert!(matches!(
        w.s.turn("mock/echo"),
        TurnOutcome::Proposal { .. }
    ));
    assert!(matches!(
        w.s.answer_question_for(&asked, "mock/echo"),
        TurnOutcome::Refusal(ref r) if r.class == RefusalClass::AlreadyConsumed
    ));
}

/// P1: an UNKNOWN route — the classifier could not tell, or its call failed — binds nothing,
/// even through a reading that could have found a value: the question waits with the same
/// identity and names the ways on. An ANSWER still binds (the control).
#[test]
fn an_unknown_or_failed_route_binds_nothing() {
    for fail in [false, true] {
        let mut w = world(&[], fail);
        let TurnOutcome::Question { .. } = w.s.turn(DRAFT) else {
            panic!("the model question");
        };
        let asked = w.s.pending_question_id().expect("waits");
        let out = w.s.turn("hmm, let me think about which one");
        let TurnOutcome::Question { key, question } = &out else {
            panic!("fail={fail}: the question waits: {out:?}");
        };
        assert_eq!(key, "model");
        assert!(question.contains("not an answer I can bind"), "{question}");
        assert_eq!(w.s.pending_question_id(), Some(asked), "fail={fail}");
        assert!(answers(&w.s).is_empty(), "fail={fail}: UNKNOWN was bound");
        let method = if fail {
            RoutingMethod::Failed
        } else {
            RoutingMethod::Model
        };
        assert_eq!(
            w.s.routes().last().map(|r| (r.act, r.method)),
            Some((TurnAct::Unknown, method))
        );
    }
    // A value question with a reader: UNKNOWN never reaches the reading either.
    let mut w = world(&[], false);
    let out = nika_onboard::compile::compile(&nika_onboard::compile::CompileRequest::create(
        "aggregate-by-key",
    ))
    .expect("compiles");
    let mut round = crate::authoring::AuthoringRound::new("aggregate-by-key");
    round.absorb(&out);
    w.s.authoring = Some(round);
    let _ = w.s.turn("Use euros, the code EUR.");
    assert!(
        answers(&w.s).is_empty(),
        "UNKNOWN bound through the reading"
    );
    assert_eq!(w.read(), 0, "UNKNOWN reached the reading");
}

/// Without any intelligence the question's declared protocol answers: the value as typed
/// binds, and the route says so — UNKNOWN (no one could judge), then ANSWER by protocol.
#[test]
fn without_an_intelligence_the_declared_answer_binds_by_protocol() {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(dir.path().join("notes/brief.md"), "brief\n").expect("brief");
    let none = ready(IntelligenceKind::None, DataLocus::None);
    let mut s = SessionRuntime::open(dir.path(), none, Box::new(NoReasoner));
    s.with_classifier(Box::new(ConservativeFallback));
    let TurnOutcome::Question { .. } = s.turn(DRAFT) else {
        panic!("the model question");
    };
    assert!(matches!(s.turn("mock/echo"), TurnOutcome::Proposal { .. }));
    let routes: Vec<(TurnAct, RoutingMethod)> =
        s.routes().iter().map(|r| (r.act, r.method)).collect();
    assert_eq!(
        routes,
        [
            (TurnAct::Unknown, RoutingMethod::Fallback),
            (TurnAct::Answer, RoutingMethod::Protocol)
        ]
    );
    assert!(s.details().contains("ANSWER · Protocol"), "{}", s.details());
}

/// A host that sends a read-only line through `turn` while a proposal waits: it is answered and
/// the proposal keeps its identity (a read-only line is never the proposal's discard).
#[test]
fn a_read_only_turn_never_discards_the_proposal() {
    let mut w = world(&[], false);
    let TurnOutcome::Proposal { id, .. } = w.s.turn(COPY) else {
        panic!("a proposal");
    };
    for line in [
        "/status",
        "/help",
        "/details",
        "/why",
        "/meaning",
        "/proof",
        "/bogus",
        "what happened?",
    ] {
        let _ = w.s.turn(line);
        assert_eq!(w.s.pending_proposal().as_ref(), Some(&id), "{line}");
    }
    assert_eq!(w.routed() + w.read(), 0);
}

/// A slash line is never the answer to a gate (a confirm or a text gate) nor a run's input:
/// both keep waiting and still take the human's own answer.
#[test]
fn a_command_shaped_line_never_answers_a_gate_or_an_input() {
    for mode in ["confirm", "text"] {
        let mut w = world(&[], false);
        w.s.pending_gate = Some(PendingGate {
            workflow: "w.nika".into(),
            trace: w.dir.path().join("paused.ndjson"),
            task: "approve".into(),
            mode: mode.into(),
            message: "Proceed?".into(),
        });
        for line in ["/bogus", "/show", "what happened?"] {
            let out = w.s.answer_gate(line);
            assert!(
                !matches!(out, TurnOutcome::ResumeRequested { .. }),
                "{mode}: {line}: {out:?}"
            );
            assert!(w.s.waiting_gate().is_some(), "{mode}: {line}");
        }
        assert_eq!(w.routed() + w.read(), 0, "{mode}");
        assert!(matches!(
            w.s.answer_gate("yes"),
            TurnOutcome::ResumeRequested { .. }
        ));
    }
    let mut w = world(&[], false);
    std::fs::write(w.dir.path().join("greet.nika"), NEEDS_INPUT).expect("workflow");
    let TurnOutcome::Question { key, .. } = w.s.turn("run greet.nika") else {
        panic!("the input is asked");
    };
    assert_eq!(key, "input.name");
    for line in ["/bogus", "/status please"] {
        let out = w.s.turn(line);
        assert!(
            !matches!(out, TurnOutcome::RunRequested { .. }),
            "{line}: {out:?}"
        );
        assert_eq!(w.s.pending_input(), Some("name"), "{line}");
    }
    assert!(matches!(
        w.s.turn("Nika"),
        TurnOutcome::RunRequested { ref run, .. } if run.vars == ["name=Nika"]
    ));
}

/// The control: read-only lines keep the consent identity, but a genuine revision (here the
/// proposal's ceiling amended) is a new proposal — the old identity is stale and applies
/// nothing; only the revised one lands.
#[test]
fn a_genuine_revision_still_invalidates_the_old_consent() {
    let mut w = world(&[], false);
    let TurnOutcome::Proposal { id: first, .. } = w.s.turn(COPY) else {
        panic!("a proposal");
    };
    let _ = w.s.consent("why");
    let _ = w.s.consent("/bogus");
    assert_eq!(w.s.pending_proposal().as_ref(), Some(&first));
    let TurnOutcome::Proposal { id: revised, .. } = w.s.consent("budget 0.10 USD") else {
        panic!("a money amendment is a new proposal");
    };
    assert_ne!(revised, first);
    assert!(matches!(
        w.s.consent_to(&first, "yes"),
        TurnOutcome::Refusal(ref r) if r.class == RefusalClass::StaleRevision
    ));
    assert!(
        !w.dir.path().join(COPY_DEST).exists(),
        "a stale consent applied"
    );
    assert!(
        matches!(w.s.consent_to(&revised, "yes"), TurnOutcome::Facts(ref t) if t.contains("applied"))
    );
    assert!(Path::new(&w.dir.path().join(COPY_DEST)).exists());
}

/// B1 of the independent review: a seat that leaks its deliberation — « not MODIFY, this is
/// DISCUSS » — names two labels, so the session's own route is UNKNOWN: the proposal is held
/// with its identity, never revised (the first label met used to start a paid revision).
#[test]
fn a_reply_naming_two_labels_never_revises_the_proposal() {
    let World {
        dir: _dir, mut s, ..
    } = world(&[], false);
    s.classifier = None;
    s.factory = Some(Box::new(|_| {
        Box::new(ScriptedReasoner::new(vec![
            "not MODIFY, this is DISCUSS".to_owned(),
        ]))
    }));
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("a proposal");
    };
    let state = semantic(&s);
    let out = s.consent("can you write it to ./out/final.md instead?");
    let TurnOutcome::Held { id: held, preview } = &out else {
        panic!("held: {out:?}");
    };
    assert_eq!(held, &id);
    assert!(!preview.contains("could not revise"), "{preview}");
    assert_eq!(
        s.routes().last().map(|r| (r.act, r.method)),
        Some((TurnAct::Unknown, RoutingMethod::Model))
    );
    assert_eq!(semantic(&s), state);
}

/// The durable door (ADR-133): with the history on, read-only lines through `consent` and
/// `turn` leave the kept draft the very proposal the human saw — after a restart its identity
/// is that proposal's, and no authority comes back (only `/restore` proposes it again, for a
/// fresh consent).
#[test]
fn read_only_lines_keep_the_durable_draft_identity_across_a_restart() {
    let home = tempfile::tempdir().expect("home");
    let World { dir, mut s, .. } = world(&[], false);
    s.enable_history(home.path()).expect("history");
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("a proposal");
    };
    for line in ["why", "what happened?", "/bogus", "/status"] {
        let _ = s.consent(line);
    }
    for line in ["/details", "/meaning", "/bogus"] {
        let _ = s.turn(line);
    }
    assert_eq!(s.pending_proposal().as_ref(), Some(&id));
    drop(s);
    let none = ready(IntelligenceKind::None, DataLocus::None);
    let mut again = SessionRuntime::open(dir.path(), none, Box::new(NoReasoner));
    let notice = again.enable_history(home.path()).expect("restored");
    assert!(
        notice.is_some_and(|n| n.contains("fresh validation")),
        "a restart says what expired"
    );
    assert_eq!(again.restored_draft_id(), Some(id.to_string().as_str()));
    assert!(
        again.pending_proposal().is_none(),
        "a restart restores no authority"
    );
}
