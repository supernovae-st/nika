// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Question identities over real Session → Compiler clarifications: the deterministic
//! compiler's `model` question (zero calls), and a native destination question through the
//! loopback seat in DIALOG-11's shape (« Copie entree.txt vers une destination à préciser. »,
//! the destination changed before the old question is answered). The identity is the one
//! the session hands out; an answer naming an old, answered, re-read, re-seated or restarted
//! question is refused before any route, reading, compiler call, record or effect, and the
//! question that waits takes its line. Mechanics only: no live provider, no workflow runs.
use super::*;
use crate::turn::RoutingMethod;
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

/// The native seat's draft for DIALOG-11: the destination a declared placeholder the seat
/// asks for, the write boundary the one narrow form the judge admits while it is asked.
const DESTINATION_DRAFT: &str = r#"nika: copy-to-chosen-file
const:
  destination_path: ""
permits:
  fs:
    read: ["./entree.txt"]
    write: [""]
  tools: ["nika:read", "nika:write"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args: { path: "./entree.txt" }
  write_destination:
    with: { text: "${{ tasks.read_source.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "${{ const.destination_path }}", content: "${{ with.text }}" }
"#;

/// The seat's answer: the draft, and the destination asked in the same words every time.
fn asks_destination() -> String {
    json!({"candidate": DESTINATION_DRAFT, "questions": [{"key": "const.destination_path",
        "label": "Destination file path", "answer_type": "text",
        "why": "The request leaves the destination to be specified."}],
        "gaps": [], "notes": "copy the exact bytes; the destination is asked"})
    .to_string()
}

/// A reasoner that answers every prompt with the route ANSWER and counts the prompts.
struct Counted(Arc<AtomicUsize>);

impl SessionReasoner for Counted {
    fn name(&self) -> String {
        "counted fixture".to_owned()
    }

    fn reason(&mut self, _prompt: &str) -> Result<crate::Reply, crate::ReasonError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(crate::Reply {
            text: "ANSWER".to_owned(),
            usage_observed: false,
        })
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
    assert_eq!(
        calls.load(Ordering::SeqCst),
        spent + 1,
        "one route, no reading"
    );
    let out = s.answer_question_for(&current, "mock/echo");
    assert!(refused(&out, RefusalClass::AlreadyConsumed), "{out:?}");
    assert_eq!(
        s.pending_proposal().as_ref(),
        Some(id),
        "the proposal is untouched"
    );
    assert_eq!(calls.load(Ordering::SeqCst), spent + 1);
    assert!(matches!(s.consent("no"), TurnOutcome::Facts(_)));
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
    assert!(s.pending_choice(), "the choice still owns the line");
    assert_eq!(s.pending_question_id().as_ref(), Some(&first));
    assert!(matches!(s.choose("3 ollama/second"), TurnOutcome::Facts(_)));
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
        1,
        "one route under the chosen model"
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
        let untouched = (world(dir.path())?, world(home.path())?);
        let out = s.answer_question_for(&earlier, "mock/echo");
        assert!(refused(&out, RefusalClass::WrongState), "{out:?}");
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
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    Ok(())
}

/// DIALOG-11's shape through the real native door: the seat asks the destination; the
/// destination changes before any answer, the request is read again, and the seat asks the
/// SAME key in the SAME words for the revised request. The old answer names the old
/// question: refused with no route, no call and nothing changed; the current answer binds
/// and the recorded plan replays with zero calls; only the durable money record changes
/// before consent, never a workflow or an output file.
#[test]
fn dialog_11_the_same_key_asked_again_is_another_question() -> Result<(), String> {
    let peer = Peer::start(vec![(200, response(&asks_destination()))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut s = open(dir.path());
    let routings = Arc::new(AtomicUsize::new(0));
    s.with_classifier(Box::new(Routes {
        acts: BTreeMap::from([(CHANGE, TurnAct::Modify)]),
        seen: Arc::clone(&routings),
    }));
    // The loopback substitution admits bounded calls only: an explicit Session allowance.
    s.admit_money("budget 2 USD", false)
        .map_err(|out| format!("{out:?}"))?;
    let out = s.turn(DIALOG_11);
    let TurnOutcome::Question { key, .. } = &out else {
        return Err(format!("the seat asks the destination: {out:?}"));
    };
    assert_eq!(key, "const.destination_path");
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
        matches!(&out, TurnOutcome::Question { key, .. } if key == "const.destination_path"),
        "{out:?}"
    );
    assert_eq!(peer.bodies().len(), 2, "the revised request was read again");
    assert_eq!(
        s.pending_question().cloned(),
        words,
        "the same key and words"
    );
    let current = s.pending_question_id().ok_or("asked again")?;
    assert_ne!(current, old, "another revision is another question");
    let untouched = world(dir.path())?;
    let routed = routings.load(Ordering::SeqCst);
    let out = s.answer_question_for(&old, "sortie.txt");
    assert!(refused(&out, RefusalClass::StaleRevision), "{out:?}");
    assert_eq!(
        (peer.bodies().len(), routings.load(Ordering::SeqCst)),
        (2, routed)
    );
    assert_eq!(s.pending_question_id().as_ref(), Some(&current));
    assert_eq!(s.pending_question().cloned(), words);
    assert_eq!(world(dir.path())?, untouched);
    let out = s.answer_question_for(&current, "archive/copie.txt");
    let TurnOutcome::Proposal { id, preview } = &out else {
        return Err(format!("the current answer binds: {out:?}"));
    };
    assert!(preview.contains("archive/copie.txt"), "{preview}");
    assert_eq!(
        (peer.bodies().len(), routings.load(Ordering::SeqCst)),
        (2, routed + 1)
    );
    let out = s.answer_question_for(&current, "autre.txt");
    assert!(refused(&out, RefusalClass::AlreadyConsumed), "{out:?}");
    let out = s.answer_question_for(&old, "sortie.txt");
    assert!(refused(&out, RefusalClass::WrongState), "{out:?}");
    assert_eq!(s.pending_proposal().as_ref(), Some(id));
    assert_eq!(
        (peer.bodies().len(), routings.load(Ordering::SeqCst)),
        (2, routed + 1)
    );
    let state_path = dir
        .path()
        .join(".nika/session-state.json")
        .display()
        .to_string();
    let mut before = untouched;
    let mut after = world(dir.path())?;
    before.remove(&state_path);
    after.remove(&state_path);
    assert_eq!(
        after, before,
        "no workflow or output is written before consent"
    );
    let state = crate::SessionState::load(dir.path())
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
