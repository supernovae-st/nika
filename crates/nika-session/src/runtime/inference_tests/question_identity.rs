// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Question identities over real Session → Compiler clarifications: the deterministic
//! compiler's `model` question (zero calls), and a semantic destination question through the
//! loopback seat in DIALOG-11's shape (« Copie entree.txt vers une destination à préciser. »,
//! the destination changed before the old question is answered). The identity is the one
//! the session hands out; an answer naming an old, answered, re-read, re-seated or restarted
//! question is refused before any route, reading, compiler call, record or effect, and the
//! question that waits takes its line. Mechanics only: no live provider, no workflow runs.
use super::*;
use crate::turn::RoutingMethod;
use crate::work::{AnswerAct, Answered, ValueSource};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The deterministic compiler asks the runtime `model` of this request (zero calls).
const DRAFT: &str = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";
/// The same work with another destination.
const REDRAFT: &str =
    "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/other.md";
/// DIALOG-11's request, as the original corpus states it.
const DIALOG_11: &str = "Copie entree.txt vers une destination à préciser.";
/// The human changes the destination while its question waits.
const CHANGE: &str = "Finalement la destination change : je te la redonne tout de suite.";

/// A reasoner that answers every prompt with the route ANSWER and counts the prompts.
struct Counted(Arc<AtomicUsize>);

impl SessionReasoner for Counted {
    fn name(&self) -> String {
        "counted fixture".to_owned()
    }

    fn reason(&mut self, _prompt: &str) -> Result<crate::Reply, crate::ReasonError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(crate::Reply::new("ANSWER".to_owned(), false))
    }
}

/// The door's classifier, scripted by exact line and counted: a route is a dispatch.
struct Routes {
    acts: BTreeMap<&'static str, TurnAct>,
    seen: Arc<AtomicUsize>,
}

impl Routes {
    fn decide(&mut self, context: &TurnContext, raw: &str) -> TurnDecision {
        self.seen.fetch_add(1, Ordering::SeqCst);
        let fallback = if matches!(context.phase, SessionPhase::QuestionPending) {
            TurnAct::Answer
        } else {
            TurnAct::NewWork
        };
        let act = self.acts.get(raw.trim()).copied().unwrap_or(fallback);
        TurnDecision::new(act, RoutingMethod::Model)
    }
}

impl TurnClassifier for Routes {
    fn classify_with_admission(
        &mut self,
        context: &TurnContext,
        raw: &str,
        _account: &nika_providers::InferenceAdmission,
    ) -> TurnDecision {
        self.decide(context, raw)
    }

    fn classify(&mut self, context: &TurnContext, raw: &str) -> TurnDecision {
        self.decide(context, raw)
    }
}

/// A project with the files the requests name.
fn project() -> Result<tempfile::TempDir, String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.path().join("notes")).map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("notes/brief.md"), "brief\n").map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("entree.txt"), "A\n").map_err(|e| e.to_string())?;
    Ok(dir)
}

/// A session over a chosen local intelligence, through the door's own factory: the
/// conversation's reasoner and every route's come from it, each prompt counted.
fn counted(root: &Path, model: &str, calls: &Arc<AtomicUsize>) -> SessionRuntime {
    let mut census = IntelligenceCensus::empty();
    census.locals.push("ollama".to_owned());
    let pref = UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        Some(model.to_owned()),
    );
    let calls = Arc::clone(calls);
    SessionRuntime::open_with(
        root,
        census,
        &pref,
        None,
        Box::new(
            move |_: &ResolvedSessionIntelligence| -> Box<dyn SessionReasoner> {
                Box::new(Counted(Arc::clone(&calls)))
            },
        ),
    )
}

/// Every file under `root` with its bytes: what a refused answer must leave as it was.
fn world(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut files = BTreeMap::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                files.insert(path.display().to_string(), bytes);
            }
        }
    }
    Ok(files)
}

fn refused(out: &TurnOutcome, class: RefusalClass) -> bool {
    matches!(out, TurnOutcome::Refusal(refusal) if refusal.class == class)
}

/// The emitted question keeps its identity while it waits; a dropped, answered or re-asked
/// question does not come back, and a new request — the same key, the same words — is
/// another question. The old identity reads nothing; the current one proceeds.
#[test]
fn a_new_request_is_a_new_question_and_an_old_answer_reads_nothing() -> Result<(), String> {
    let dir = project()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut s = counted(dir.path(), "ollama/first", &calls);
    let out = s.turn(DRAFT);
    let TurnOutcome::Question { key, .. } = &out else {
        return Err(format!("the compiler asks: {out:?}"));
    };
    assert_eq!(key, "model");
    let first = s
        .pending_question_id()
        .ok_or("the question has an identity")?;
    assert_eq!(s.pending_question_id().as_ref(), Some(&first));
    assert_eq!((first.as_str().len(), first.to_string().len()), (64, 12));
    assert!(matches!(s.turn("why?"), TurnOutcome::Aside(_)));
    assert_eq!(
        s.pending_question_id().as_ref(),
        Some(&first),
        "an aside keeps it"
    );
    assert!(matches!(s.turn("cancel"), TurnOutcome::Facts(_)));
    let out = s.answer_question_for(&first, "mock/echo");
    assert!(refused(&out, RefusalClass::AlreadyConsumed), "{out:?}");
    let out = s.turn(REDRAFT);
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "model"),
        "{out:?}"
    );
    let current = s
        .pending_question_id()
        .ok_or("the new question has an identity")?;
    assert_ne!(current, first);
    let words = s.pending_question().cloned();
    let untouched = world(dir.path())?;
    let spent = calls.load(Ordering::SeqCst);
    let out = s.answer_question_for(&first, "mock/echo");
    assert!(refused(&out, RefusalClass::StaleRevision), "{out:?}");
    assert_eq!(calls.load(Ordering::SeqCst), spent, "no route, no reading");
    assert_eq!(s.pending_question_id().as_ref(), Some(&current));
    assert_eq!(s.pending_question().cloned(), words);
    assert!(s.pending_proposal().is_none());
    assert_eq!(world(dir.path())?, untouched);
    let out = s.answer_question_for(&current, "mock/echo");
    let TurnOutcome::Proposal { id, .. } = &out else {
        return Err(format!("the current answer proceeds: {out:?}"));
    };
    // A `provider/name` alone answers the model question by its shape: no route, no reading.
    assert_eq!(calls.load(Ordering::SeqCst), spent, "no route, no reading");
    let bound = AnswerAct::Bound {
        key: "model".to_owned(),
        value: "mock/echo".to_owned(),
        reading: ValueSource::AsTyped,
    };
    assert_eq!(
        s.work().answered,
        Some(Answered::new(current.clone(), bound))
    );
    let out = s.answer_question_for(&current, "mock/echo");
    assert!(refused(&out, RefusalClass::AlreadyConsumed), "{out:?}");
    let class = RefusalClass::AlreadyConsumed;
    assert_eq!(
        s.work().answered,
        Some(Answered::new(current.clone(), AnswerAct::Refused { class }))
    );
    assert_eq!(
        s.pending_proposal().as_ref(),
        Some(id),
        "the proposal is untouched"
    );
    assert_eq!(calls.load(Ordering::SeqCst), spent);
    assert!(matches!(s.consent("no"), TurnOutcome::Facts(_)));
    assert_eq!(s.work().answered, None, "a consent is no answer");
    assert!(matches!(s.turn(REDRAFT), TurnOutcome::Question { .. }));
    let again = s.pending_question_id().ok_or("asked again")?;
    assert_eq!(
        s.pending_question().cloned(),
        words,
        "the same key and words"
    );
    assert_ne!(again, current, "asked again is another question");
    // The discarded proposal kept the session's record; no refused answer writes.
    let untouched = world(dir.path())?;
    let out = s.answer_question_for(&current, "mock/echo");
    assert!(refused(&out, RefusalClass::StaleRevision), "{out:?}");
    assert_eq!(world(dir.path())?, untouched);
    assert!(
        untouched.keys().all(|path| {
            !std::path::Path::new(path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("nika"))
        }),
        "nothing was saved: {:?}",
        untouched.keys()
    );
    Ok(())
}

/// Another intelligence reads the answer now: the waiting question is asked again under
/// it, and the identity asked under the first one is refused before any route. While the
/// first screen owns the next line, even the current identity answers nothing.
#[test]
fn another_intelligence_asks_another_question() -> Result<(), String> {
    let dir = project()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut s = counted(dir.path(), "ollama/first", &calls);
    assert!(matches!(s.turn(DRAFT), TurnOutcome::Question { .. }));
    let first = s.pending_question_id().ok_or("asked")?;
    let words = s.pending_question().cloned();
    assert!(matches!(s.turn("/intelligence"), TurnOutcome::Ask(_)));
    let out = s.answer_question_for(&first, "mock/echo");
    assert!(refused(&out, RefusalClass::WrongState), "{out:?}");
    let class = RefusalClass::WrongState;
    assert_eq!(
        s.work().answered,
        Some(Answered::new(first.clone(), AnswerAct::Refused { class }))
    );
    assert!(s.pending_choice(), "the choice still owns the line");
    assert_eq!(s.pending_question_id().as_ref(), Some(&first));
    assert!(matches!(s.choose("3 ollama/second"), TurnOutcome::Facts(_)));
    assert_eq!(s.work().answered, None, "a choice is no answer");
    assert_eq!(s.intelligence.model.as_deref(), Some("ollama/second"));
    let current = s.pending_question_id().ok_or("still asked")?;
    assert_ne!(current, first);
    assert_eq!(
        s.pending_question().cloned(),
        words,
        "the same key and words"
    );
    let untouched = world(dir.path())?;
    let out = s.answer_question_for(&first, "mock/echo");
    assert!(refused(&out, RefusalClass::StaleRevision), "{out:?}");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "refused before any reasoner call"
    );
    assert_eq!(s.pending_question_id().as_ref(), Some(&current));
    assert_eq!(world(dir.path())?, untouched);
    let out = s.answer_question_for(&current, "mock/echo");
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "an identity alone is the answer: no route under the chosen model"
    );
    Ok(())
}

/// A restarted session asks nothing it did not ask: an earlier identity answers nothing
/// when no question waits, and nothing either when the same question in the same words and
/// the same state waits again — with or without the durable conversation history, which
/// records no refused answer.
#[test]
fn a_restarted_session_never_takes_an_earlier_answer() -> Result<(), String> {
    for durable in [false, true] {
        let dir = project()?;
        let home = tempfile::tempdir().map_err(|e| e.to_string())?;
        let calls = Arc::new(AtomicUsize::new(0));
        let mut before_restart = counted(dir.path(), "ollama/first", &calls);
        if durable {
            before_restart
                .enable_history(home.path())
                .map_err(|e| e.to_string())?;
        }
        assert!(matches!(
            before_restart.turn(DRAFT),
            TurnOutcome::Question { .. }
        ));
        let earlier = before_restart.pending_question_id().ok_or("asked")?;
        drop(before_restart);
        let mut s = counted(dir.path(), "ollama/first", &calls);
        if durable {
            s.enable_history(home.path()).map_err(|e| e.to_string())?;
        }
        assert!(s.restore_state().is_none());
        assert!(
            s.pending_question_id().is_none(),
            "no question survives a restart"
        );
        assert_eq!(s.work().answered, None, "no act survives a restart");
        let untouched = (world(dir.path())?, world(home.path())?);
        let out = s.answer_question_for(&earlier, "mock/echo");
        assert!(refused(&out, RefusalClass::WrongState), "{out:?}");
        let class = RefusalClass::WrongState;
        assert_eq!(
            s.work().answered,
            Some(Answered::new(earlier.clone(), AnswerAct::Refused { class }))
        );
        assert_eq!((world(dir.path())?, world(home.path())?), untouched);
        assert!(matches!(s.turn(DRAFT), TurnOutcome::Question { .. }));
        let current = s.pending_question_id().ok_or("asked after the restart")?;
        assert_eq!(
            current.as_str(),
            earlier.as_str(),
            "the same words and state"
        );
        assert_ne!(current, earlier, "asked by another session");
        let untouched = (world(dir.path())?, world(home.path())?);
        let out = s.answer_question_for(&earlier, "mock/echo");
        assert!(refused(&out, RefusalClass::StaleRevision), "{out:?}");
        let class = RefusalClass::StaleRevision;
        assert_eq!(
            s.work().answered,
            Some(Answered::new(earlier.clone(), AnswerAct::Refused { class }))
        );
        assert_eq!(
            (world(dir.path())?, world(home.path())?),
            untouched,
            "no record"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "refused before any reasoner call"
        );
        assert_eq!(s.pending_question_id().as_ref(), Some(&current));
        let out = s.answer_question_for(&current, "mock/echo");
        assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "an identity alone: no route"
        );
    }
    Ok(())
}

/// A line a route reads as a cancel drops the round as the cancel word does, and the act names
/// the question it was typed for.
#[test]
fn a_routed_cancel_drops_the_question_it_was_typed_for() -> Result<(), String> {
    const LETGO: &str = "on oublie tout ça pour le moment";
    let dir = project()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut s = counted(dir.path(), "ollama/first", &calls);
    s.with_classifier(Box::new(Routes {
        acts: BTreeMap::from([(LETGO, TurnAct::Cancel)]),
        seen: Arc::new(AtomicUsize::new(0)),
    }));
    assert!(matches!(s.turn(DRAFT), TurnOutcome::Question { .. }));
    let asked = s.pending_question_id().ok_or("asked")?;
    let out = s.turn(LETGO);
    assert!(
        matches!(&out, TurnOutcome::Facts(text) if text.starts_with("authoring discarded")),
        "{out:?}"
    );
    let key = "model".to_owned();
    assert_eq!(
        s.work().answered,
        Some(Answered::new(asked.clone(), AnswerAct::Dropped { key }))
    );
    let out = s.answer_question_for(&asked, "mock/echo");
    assert!(refused(&out, RefusalClass::AlreadyConsumed), "{out:?}");
    Ok(())
}

/// Whether a request the seat received is its round's judge over the bytes that round finished:
/// the closed clause choice (carried · missing) carrying the bound answer.
fn judged_over(body: &serde_json::Value, bound: &str) -> bool {
    let body = body.to_string();
    body.contains("carried") && body.contains("missing") && body.contains(bound)
}

/// Whether a request the seat received is its round's judge of the whole request over the bytes
/// that round finished: the closed whole-request choice over a candidate carrying `bound`.
fn judged_whole_over(body: &serde_json::Value, bound: &str) -> bool {
    super::unjudged::asked(body).is_some_and(|(state, verdicts)| {
        verdicts == ["faithful", "unfaithful", "none"]
            && (state["candidate_nika"].as_str()).is_some_and(|c| c.contains(bound))
    })
}

/// The seat's answers in DIALOG-11, in call order: the document of the request, the document of
/// the request read again, then the answer round's judge of the whole request, faithful (the
/// last reply, which the seat repeats for every later call).
fn dialog_11_script() -> Vec<(u16, serde_json::Value)> {
    let asks = || (200, response(&asks_destination()));
    let says = |text: &str| (200, response(text));
    vec![asks(), asks(), says(JUDGE_APPROVES)]
}

/// DIALOG-11's shape through semantic CREATE: the document leaves the destination open and asks
/// it; the destination changes before any answer, the request is read again,
/// and the SAME key is asked in the SAME words for the revised request. The old answer names the
/// old question: refused with no route, no call and nothing changed; the current answer binds
/// and the recorded document replays, its calls the judge the seat is permitted as when the
/// round finishes, over the bound bytes: the whole request, whose faithful verdict is weighed
/// against the round's trial run of those bytes, each part asked over it; only the durable money
/// record changes before consent, never a workflow or an output file.
#[test]
fn dialog_11_the_same_key_asked_again_is_another_question() -> Result<(), String> {
    let peer = Peer::start(dialog_11_script());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut s = open(dir.path());
    let routings = Arc::new(AtomicUsize::new(0));
    s.with_classifier(Box::new(Routes {
        acts: BTreeMap::from([(CHANGE, TurnAct::Modify)]),
        seen: Arc::clone(&routings),
    }));
    // The loopback substitution admits bounded calls only: an explicit Session allowance.
    s.admit_money("budget 2 USD", false, false)
        .map_err(|out| format!("{out:?}"))?;
    let out = s.turn(DIALOG_11);
    let TurnOutcome::Question { key, .. } = &out else {
        return Err(format!("the compiler asks the destination: {out:?}"));
    };
    assert_eq!(key, "const.output_path");
    let old = s
        .pending_question_id()
        .ok_or("the emitted question has an identity")?;
    assert_eq!(peer.bodies().len(), 1);
    assert!(
        peer.bodies()[0].to_string().contains(DIALOG_11),
        "the request, verbatim"
    );
    let words = s.pending_question().cloned();
    let out = s.turn(CHANGE);
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "const.output_path"),
        "{out:?}"
    );
    assert_eq!(peer.bodies().len(), 2, "the revised request was read again");
    let key = || "const.output_path".to_owned();
    assert_eq!(
        s.work().answered,
        Some(act(&old, AnswerAct::Restated { key: key() }))
    );
    assert_eq!(
        s.pending_question().cloned(),
        words,
        "the same key and words"
    );
    let current = s.pending_question_id().ok_or("asked again")?;
    assert_ne!(current, old, "another revision is another question");
    let untouched = world(dir.path())?;
    // The requests the seat received and the turns routed so far.
    let calls = || (peer.bodies().len(), routings.load(Ordering::SeqCst));
    let routed = calls().1;
    let out = s.answer_question_for(&old, "sortie.txt");
    assert!(refused(&out, RefusalClass::StaleRevision), "{out:?}");
    assert_eq!(calls(), (2, routed));
    assert_eq!(s.pending_question_id().as_ref(), Some(&current));
    assert_eq!(s.pending_question().cloned(), words);
    assert_eq!(world(dir.path())?, untouched);
    let out = s.answer_question_for(&current, "archive/copie.txt");
    let TurnOutcome::Proposal { id, preview } = &out else {
        return Err(format!("the current answer binds: {out:?}"));
    };
    assert!(preview.contains("archive/copie.txt"), "{preview}");
    written_whole(&s, preview)?;
    let bound = AnswerAct::Bound {
        key: key(),
        value: "archive/copie.txt".to_owned(),
        reading: ValueSource::AsTyped,
    };
    assert_eq!(s.work().answered, Some(act(&current, bound)));
    assert_eq!(calls(), (5, routed + 1));
    assert!(
        judged_whole_over(&peer.bodies()[2], "archive/copie.txt"),
        "the round's judge of the whole request"
    );
    let over_the_run = |body: &serde_json::Value| body.to_string().contains("unexercised");
    assert!(
        peer.bodies()[3..].iter().all(over_the_run),
        "its parts over the trial run"
    );
    assert!(
        (peer.bodies()[3..].iter()).all(|body| judged_over(body, "archive/copie.txt")),
        "each part judged over the bound bytes"
    );
    let out = s.answer_question_for(&current, "autre.txt");
    assert!(refused(&out, RefusalClass::AlreadyConsumed), "{out:?}");
    let out = s.answer_question_for(&old, "sortie.txt");
    assert!(refused(&out, RefusalClass::WrongState), "{out:?}");
    assert_eq!(s.pending_proposal().as_ref(), Some(id));
    assert_eq!(calls(), (5, routed + 1));
    only_money_is_durable(dir.path(), untouched, &s)
}

/// What a line did to the question it was typed for, as the work snapshot carries it.
fn act(question: &crate::outcome::QuestionId, act: AnswerAct) -> Answered {
    Answered::new(question.clone(), act)
}

/// The CREATE round's own record describes the proposed bytes: written whole, no base.
fn written_whole(s: &SessionRuntime, preview: &str) -> Result<(), String> {
    assert!(preview.contains("created · written"), "{preview}");
    let created = (s.work().candidate)
        .and_then(|c| c.revision)
        .ok_or("the creation record binds the proposed bytes")?;
    assert_eq!(
        (created.mode.as_str(), created.base_sha256.as_deref()),
        ("written", None)
    );
    assert!(created.components.is_empty() && created.changed.is_empty());
    Ok(())
}

/// Before consent only the durable money record changes: no workflow or output is written, and
/// the paid-call observation is kept with no dispatch decision.
fn only_money_is_durable(
    root: &Path,
    mut before: BTreeMap<String, Vec<u8>>,
    s: &SessionRuntime,
) -> Result<(), String> {
    let state_path = root.join(".nika/session-state.json").display().to_string();
    let mut after = world(root)?;
    before.remove(&state_path);
    after.remove(&state_path);
    assert_eq!(
        after, before,
        "no workflow or output is written before consent"
    );
    let state = crate::SessionState::load(root)
        .map_err(|e| e.to_string())?
        .ok_or("the paid-call observation is durable")?;
    assert_eq!(state.inference_observations, s.cost_observations());
    assert!(
        !state
            .decisions
            .iter()
            .any(|line| line.starts_with(super::super::inference::DISPATCH_PREFIX))
    );
    Ok(())
}
