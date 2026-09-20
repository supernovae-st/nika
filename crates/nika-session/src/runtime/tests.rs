// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The runtime's conversation tests: turns, facts, the guard, consent,
//! the run request, the gate, the identity a remote host drives by.

use super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::reasoner::{NoReasoner, Reply, ScriptedReasoner};

/// A seat reasoner whose name is the seat itself, as the harness one is.
struct Seat(&'static str);

impl SessionReasoner for Seat {
    fn name(&self) -> String {
        self.0.to_owned()
    }

    fn reason(&mut self, _prompt: &str) -> Result<Reply, crate::reasoner::ReasonError> {
        Ok(Reply {
            text: "seated".to_owned(),
            usage_observed: false,
        })
    }
}

fn tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::write(
        dir.path().join("alpha.nika"),
        "nika: alpha\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: \"sk-live-ABCDEFGH123456\", max_tokens: 10 }\n",
    )
    .expect("a");
    dir
}

fn ready(kind: IntelligenceKind, locus: DataLocus) -> ResolvedSessionIntelligence {
    ResolvedSessionIntelligence {
        kind,
        model: None,
        locus,
        ready: true,
        why: None,
    }
}

/// A chat turn never writes a temp workflow nor a trace: the tree is
/// untouched after three turns.
#[test]
fn a_turn_writes_nothing() {
    let dir = tree();
    let reasoner = ScriptedReasoner::new(vec!["Sure.".to_owned()]);
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(
            IntelligenceKind::Local {
                provider: "ollama".to_owned(),
            },
            DataLocus::Local,
        ),
        Box::new(reasoner),
    );
    let before: Vec<_> = std::fs::read_dir(dir.path())
        .expect("dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    let _ = s.turn("what workflows are here?");
    let _ = s.turn("explain what alpha.nika does");
    assert!(
        matches!(s.turn("/help"), TurnOutcome::Help(ref card) if card.contains("no AI asked") && card.contains("what Nika calls")),
        "the card names the shapes that answer without a model"
    );
    let after: Vec<_> = std::fs::read_dir(dir.path())
        .expect("dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(before, after, "no temp file, no .nika/ tree");
    assert!(!dir.path().join(".nika").exists());
}

/// The reasoner receives only the bundle: the grounding, the facts,
/// the file the human named (redacted), the turn — never the
/// environment, never a file the human did not name.
#[test]
fn the_reasoner_receives_only_the_bundle() {
    let dir = tree();
    std::fs::write(dir.path().join("secret.nika"), "nika: hidden\ntasks: {}\n").expect("hidden");
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(
            IntelligenceKind::Api {
                provider: "mistral".to_owned(),
            },
            DataLocus::Metered {
                provider: "mistral".to_owned(),
            },
        ),
        Box::new(ScriptedReasoner::new(vec!["It reads a file.".to_owned()])),
    );
    let out = s.turn("what does alpha.nika do?");
    assert!(
        matches!(out, TurnOutcome::Reply(ref t) if t.contains("It reads a file.")),
        "{out:?}"
    );
    // reach the scripted reasoner's record through a second session? the
    // reasoner is boxed: assert on the prompt shape via a fresh reasoner
    let mut probe = ScriptedReasoner::new(vec!["x".to_owned()]);
    let snapshot = ProjectSnapshot::observe(dir.path());
    let broker = ContextBroker::new(snapshot.root.clone());
    let bundle = broker.bundle(
        &snapshot,
        Some("goal"),
        &["alpha.nika".to_owned()],
        "metered",
    );
    let prompt = ContextBroker::prompt(&bundle, &[], "what does alpha.nika do?");
    let _ = probe.reason(&prompt);
    let seen = &probe.seen[0];
    assert!(
        seen.contains("Never invent Nika syntax"),
        "the identity core rides"
    );
    assert!(seen.contains("File `alpha.nika`"), "the named file rides");
    assert!(
        !seen.contains("nika: hidden"),
        "an unnamed file never rides"
    );
    assert!(
        !seen.contains("sk-live-ABCDEFGH123456"),
        "the secret never rides"
    );
    assert!(
        !seen.contains("PATH=") && !seen.contains("OPENAI_API_KEY=") && !seen.contains("HOME="),
        "the environment never rides"
    );
}

/// An invented workflow language in the reply is corrected before the
/// human sees it; a claim of ignorance too.
#[test]
fn an_invented_grammar_is_corrected_before_the_human_sees_it() {
    let dir = tree();
    let invented = "Here is your workflow:\n```yaml\nversion: 1\nsteps:\n  - fetch_internet: https://x\n```\nUse `nika:telegram` to notify. I don't know Nika's exact syntax.";
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(
            IntelligenceKind::Local {
                provider: "ollama".to_owned(),
            },
            DataLocus::Local,
        ),
        Box::new(ScriptedReasoner::new(vec![invented.to_owned()])),
    );
    let out = s.turn("make me a workflow that fetches a site and notifies telegram");
    let TurnOutcome::Reply(text) = out else {
        panic!("{out:?}");
    };
    assert!(
        text.contains("grounding (the installed engine disagrees"),
        "{text}"
    );
    assert!(text.contains("`steps` is not a workflow field"), "{text}");
    assert!(text.contains("`nika:telegram` is not a builtin"), "{text}");
    assert!(
        text.contains("the installed engine's canon is available"),
        "{text}"
    );
    assert_eq!(
        s.intent.goal.as_deref(),
        Some("make me a workflow that fetches a site and notifies telegram")
    );
}

/// Without conversational intelligence the facts still answer and a
/// free-text turn is refused with the fix, never routed elsewhere.
#[test]
fn without_intelligence_the_facts_stay_and_free_text_is_refused() {
    let dir = tree();
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(IntelligenceKind::None, DataLocus::None),
        Box::new(NoReasoner),
    );
    assert!(
        matches!(s.turn("which builtins exist?"), TurnOutcome::Facts(ref t) if t.contains("nika:read"))
    );
    assert!(
        matches!(s.turn("write a haiku"), TurnOutcome::Refusal(ref r) if r.text.contains("no conversational intelligence"))
    );
    assert!(matches!(s.turn("/quit"), TurnOutcome::Quit));
    assert!(s.banner().contains("no conversational AI"));
    assert_eq!(
        s.banner().matches("no conversational AI").count(),
        1,
        "the path is named once: {}",
        s.banner()
    );
}

const PROPOSED: &str = "Here it is.\n\n```yaml path=daily.nika\nnika: daily\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\noutputs:\n  said: ${{ tasks.t.output }}\n```\n";

fn ready_with(dir: &Path, replies: Vec<&str>) -> SessionRuntime {
    let seated = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Harness {
            seat: "codex".to_owned(),
        },
        model: None,
        locus: DataLocus::Remote {
            product: "codex".to_owned(),
        },
        ready: true,
        why: None,
    };
    SessionRuntime::open(
        dir,
        seated,
        Box::new(ScriptedReasoner::new(
            replies.into_iter().map(str::to_owned).collect(),
        )),
    )
}

/// A reply carrying a file is a proposal: nothing is written until the
/// consent line says yes; `no` discards; the next yes lands the exact
/// bytes and the real check follows; a new turn discards a pending set.
#[test]
fn a_proposal_lands_only_on_consent() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![PROPOSED, PROPOSED, PROPOSED]);
    let TurnOutcome::Proposal { preview, .. } = s.turn("write me a daily digest workflow") else {
        panic!("a proposal");
    };
    assert!(
        preview.starts_with("Here it is.\n\n"),
        "the prose above: {preview}"
    );
    assert!(
        preview.contains("proposed change · write me a daily digest workflow"),
        "the header names this turn's request: {preview}"
    );
    assert!(
        preview.contains("creates `daily.nika`") && preview.contains("clean ✔"),
        "{preview}"
    );
    assert!(
        !dir.path().join("daily.nika").exists(),
        "nothing written before consent"
    );
    assert!(matches!(s.consent("no"), TurnOutcome::Facts(ref t) if t.contains("discarded")));
    assert!(
        matches!(s.turn("1"), TurnOutcome::Facts(ref t) if t.contains("already chosen")),
        "a bare digit is the first-screen reflex, never a message for the seat"
    );
    assert!(!dir.path().join("daily.nika").exists(), "no means nothing");
    assert!(
        matches!(s.consent("yes"), TurnOutcome::Refusal(ref r) if r.text.contains("nothing is pending"))
    );
    assert!(matches!(
        s.turn("again please"),
        TurnOutcome::Proposal { .. }
    ));
    assert!(
        matches!(s.turn("what workflows are here?"), TurnOutcome::Facts(_)),
        "a new turn"
    );
    assert!(
        matches!(s.consent("yes"), TurnOutcome::Refusal(_)),
        "the new turn discarded the proposal"
    );
    assert!(matches!(s.turn("once more"), TurnOutcome::Proposal { .. }));
    let TurnOutcome::Facts(report) = s.consent("yes") else {
        panic!("applied");
    };
    assert!(
        report.contains("applied · wrote `daily.nika`") && report.contains("clean ✔"),
        "{report}"
    );
    let on_disk = std::fs::read_to_string(dir.path().join("daily.nika")).expect("landed");
    assert!(
        on_disk.starts_with("nika: daily\n") && on_disk.ends_with("${{ tasks.t.output }}\n"),
        "exact bytes"
    );
    assert!(
        s.snapshot
            .workflows
            .iter()
            .any(|w| w.path.ends_with("daily.nika")),
        "the snapshot sees it"
    );
}

/// A question at the consent prompt is answered and the proposal held;
/// the next `yes` still lands it.
#[test]
fn a_question_at_the_consent_prompt_holds_the_proposal() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![PROPOSED]);
    assert!(matches!(
        s.turn("write me a daily digest"),
        TurnOutcome::Proposal { .. }
    ));
    let TurnOutcome::Held { preview: text, .. } = s.consent("what is permits?") else {
        panic!("held");
    };
    assert!(text.contains("boundary"), "{text}");
    let TurnOutcome::Held { preview: text, .. } =
        s.consent("what will this read and write when it runs?")
    else {
        panic!("held");
    };
    assert!(
        text.contains("when it runs:") && text.contains("model mock/echo"),
        "the set's own effects: {text}"
    );
    let TurnOutcome::Held { preview: text, .. } = s.consent("hmm") else {
        panic!("held");
    };
    assert!(
        text.contains("not a consent") && text.contains("still waits"),
        "{text}"
    );
    assert!(!dir.path().join("daily.nika").exists());
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(ref t) if t.contains("applied")));
    assert!(dir.path().join("daily.nika").exists());
}

/// After a run in a git repository, the missing ignore line is named
/// once; a `.gitignore` that keeps the traces out silences it.
#[test]
fn the_trace_hygiene_note_names_the_missing_ignore_line() {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join(".git")).expect("a git root");
    let mut s = ready_with(dir.path(), vec![PROPOSED]);
    assert!(s.snapshot.git_root.is_some(), "a git root");
    let TurnOutcome::Facts(line) = s.observe_run(0, Some(Path::new(".nika/traces/t.ndjson")))
    else {
        panic!("an observation");
    };
    assert!(line.contains("not ignored by git here"), "{line}");
    std::fs::write(dir.path().join(".gitignore"), "target/\n.nika/traces/\n").expect("ignore");
    let TurnOutcome::Facts(line) = s.observe_run(0, Some(Path::new(".nika/traces/t.ndjson")))
    else {
        panic!("an observation");
    };
    assert!(!line.contains("not ignored"), "{line}");
}

/// « create and run it » requests the run ONLY after a clean on-disk
/// check; findings stop it; the door's observation becomes a fact.
#[test]
fn a_run_is_requested_only_on_a_clean_check() {
    let dir = tree();
    let dirty = "```yaml path=bad.nika\nnika: bad\ntasks:\n  t:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n```\n";
    let mut s = ready_with(dir.path(), vec![PROPOSED, dirty]);
    assert!(
        matches!(s.turn("create a digest and run it once"), TurnOutcome::Proposal { ref preview, .. } if preview.contains("run `daily.nika` once"))
    );
    let TurnOutcome::RunRequested { report, run } = s.consent("yes") else {
        panic!("a clean check requests the run");
    };
    assert!(report.contains("clean ✔"), "{report}");
    assert_eq!(run.workflow, PathBuf::from("daily.nika"));
    assert!((run.max_cost_usd - DEFAULT_CEILING_USD).abs() < f64::EPSILON);
    assert_eq!(
        ceiling_in("create it and run it once with a ceiling of 0.05"),
        Some(0.05)
    );
    assert_eq!(ceiling_in("run it, cap $0.10 please"), Some(0.10));
    assert_eq!(ceiling_in("run it --max-cost-usd 1"), Some(1.0));
    assert_eq!(ceiling_in("run it --max-cost-usd=0.5"), Some(0.5));
    assert_eq!(ceiling_in("run it once"), None, "no number, the default");
    assert_eq!(
        ceiling_in("write 3 tasks and run it"),
        None,
        "a count is not a ceiling"
    );
    let TurnOutcome::Facts(observed) = s.observe_run(0, Some(Path::new(".nika/traces/t.ndjson")))
    else {
        panic!("an observation");
    };
    assert!(
        observed.contains("exit 0 · succeeded") && observed.contains("t.ndjson"),
        "{observed}"
    );
    assert!(matches!(
        s.turn("make a curl one and run it"),
        TurnOutcome::Proposal { .. }
    ));
    let TurnOutcome::Facts(report) = s.consent("yes") else {
        panic!("findings stop the run");
    };
    assert!(
        report.contains("findings ✖") && report.contains("the run was not started"),
        "{report}"
    );
    assert!(
        dir.path().join("bad.nika").exists(),
        "the bytes landed; the run did not start"
    );
}

/// A paused run returns to the session as a question; the human's line
/// becomes the resume the door runs; nothing answers for them.
#[test]
fn a_paused_run_asks_the_human_and_the_answer_resumes_it() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![PROPOSED]);
    assert!(matches!(
        s.turn("create a digest and run it"),
        TurnOutcome::Proposal { .. }
    ));
    assert!(matches!(s.consent("yes"), TurnOutcome::RunRequested { .. }));
    let store = dir.path().join(".nika").join("traces");
    std::fs::create_dir_all(&store).expect("store");
    let trace = store.join("paused.ndjson");
    std::fs::write(
        &trace,
        "{\"kind\":\"workflow_paused\",\"fields\":[{\"key\":\"task\",\"value\":\"gate\"},{\"key\":\"mode\",\"value\":\"confirm\"},{\"key\":\"message\",\"value\":\"Ship it?\"}]}\n",
    )
    .expect("trace");
    let TurnOutcome::GateAsk { question, .. } = s.observe_run(4, Some(&trace)) else {
        panic!("the gate is asked");
    };
    assert!(
        question.contains("paused for a human answer") && question.contains("Ship it?"),
        "{question}"
    );
    assert!(
        matches!(s.answer_gate(""), TurnOutcome::Refusal(ref r) if r.text.contains("nothing answers for you"))
    );
    let TurnOutcome::ResumeRequested {
        workflow,
        trace: t,
        answer,
    } = s.answer_gate("yes")
    else {
        panic!("the resume");
    };
    assert_eq!(workflow, PathBuf::from("daily.nika"));
    assert_eq!(t, trace);
    assert_eq!(answer, "gate=true");
    assert!(
        matches!(s.answer_gate("yes"), TurnOutcome::Refusal(_)),
        "answered once"
    );
    assert!(
        matches!(s.observe_run(0, Some(&trace)), TurnOutcome::Facts(_)),
        "a completed resume is a fact"
    );
}

/// The repair round: a dirty apply, then « fix it » — the reasoner's
/// repaired file is a witnessed update, consented, checked clean.
#[test]
fn a_repair_round_updates_the_witnessed_file_to_clean() {
    let dir = tree();
    let dirty = "```yaml path=bad.nika\nnika: bad\ntasks:\n  t:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n```\n";
    let repaired = "Adding the boundary.\n\n```yaml path=bad.nika\nnika: bad\npermits: { exec: [\"curl\"], net: { http: [\"example.com\"] } }\ntasks:\n  t:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n```\n";
    let mut s = ready_with(dir.path(), vec![dirty, repaired]);
    assert!(matches!(
        s.turn("make a curl one"),
        TurnOutcome::Proposal { .. }
    ));
    let TurnOutcome::Facts(report) = s.consent("yes") else {
        panic!("applied");
    };
    assert!(
        report.contains("findings ✖") && report.contains("NIKA-AUTH-006"),
        "{report}"
    );
    let TurnOutcome::Proposal { preview, .. } = s.turn("fix it") else {
        panic!("a repair proposal");
    };
    assert!(
        preview.contains("replaces `bad.nika` whole") && preview.contains("clean ✔"),
        "{preview}"
    );
    let TurnOutcome::Facts(report) = s.consent("yes") else {
        panic!("applied");
    };
    assert!(
        report.contains("clean ✔") && !report.contains("findings ✖"),
        "{report}"
    );
    assert!(
        std::fs::read_to_string(dir.path().join("bad.nika"))
            .expect("landed")
            .contains("permits:")
    );
}

/// A reply proposing a path outside the root is refused before any preview.
#[test]
fn a_path_outside_the_root_is_refused_before_preview() {
    let dir = tree();
    let evil = "```yaml path=../evil.nika\nnika: evil\n```\n";
    let mut s = ready_with(dir.path(), vec![evil]);
    assert!(
        matches!(s.turn("write one"), TurnOutcome::Refusal(ref r) if r.text.contains("not a path inside the project root"))
    );
    assert!(
        matches!(s.consent("yes"), TurnOutcome::Refusal(_)),
        "nothing pending"
    );
}

/// `/intelligence` asks the first screen again in-session; the next
/// line is the answer, kept under the home, the reasoner rebuilt; an
/// unserved pick is refused and the previous choice stands.
#[test]
fn the_intelligence_can_be_rechosen_in_session() {
    let dir = tree();
    let home = tempfile::tempdir().expect("home");
    let census = IntelligenceCensus {
        seats: vec![crate::intelligence::SeatSeen {
            id: "codex".to_owned(),
            product_present: true,
            configured: true,
        }],
        api_keys: vec![],
        locals: vec![],
    };
    let pref = UserIntelligencePreference::new(IntelligenceKind::None, None);
    let factory: ReasonerFactory = Box::new(|resolved| match &resolved.kind {
        IntelligenceKind::None => Box::new(NoReasoner),
        _ => Box::new(ScriptedReasoner::new(vec!["seated".to_owned()])),
    });
    let mut s = SessionRuntime::open_with(dir.path(), census, &pref, Some(home.path()), factory);
    assert!(matches!(s.turn("hello"), TurnOutcome::Refusal(_)));
    let TurnOutcome::Ask(screen) = s.turn("/intelligence") else {
        panic!("asks");
    };
    assert!(screen.contains("Choose which AI"), "{screen}");
    assert!(
        matches!(s.choose("2"), TurnOutcome::Refusal(ref r) if r.text.contains("previous choice stands"))
    );
    assert!(
        matches!(s.turn("hello"), TurnOutcome::Refusal(_)),
        "still none"
    );
    assert!(
        matches!(s.choose("1"), TurnOutcome::Facts(ref t) if t.contains("codex") && t.contains("kept"))
    );
    assert!(matches!(s.turn("hello"), TurnOutcome::Reply(ref t) if t.contains("seated")));
    let back = UserIntelligencePreference::load(home.path()).expect("kept under the home");
    assert_eq!(
        back.kind,
        IntelligenceKind::Harness {
            seat: "codex".to_owned()
        }
    );
}

/// An explicit choice this machine cannot serve refuses every
/// free-text turn with its fix — the facts still answer.
#[test]
fn an_unserved_choice_refuses_with_its_fix() {
    let dir = tree();
    let unserved = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Harness {
            seat: "claude-code".to_owned(),
        },
        model: None,
        locus: DataLocus::Remote {
            product: "claude-code".to_owned(),
        },
        ready: false,
        why: Some("`claude-code` is not installed on this machine — install it".to_owned()),
    };
    let mut s = SessionRuntime::open(dir.path(), unserved, Box::new(Seat("claude-code")));
    assert!(s.banner().contains("⚠ `claude-code` is not installed"));
    assert!(
        s.banner().contains("intelligence: claude-code · uses")
            && !s.banner().contains("claude-code · claude-code"),
        "the seat is named once: {}",
        s.banner()
    );
    assert!(
        matches!(s.turn("hello"), TurnOutcome::Refusal(ref r) if r.text.contains("not installed"))
    );
    assert!(matches!(
        s.turn("what workflows are here?"),
        TurnOutcome::Facts(_)
    ));
}

/// The freeze audit · a stale apply (the file appeared on disk after the
/// preview) leaves the proposal UNDECIDED: nothing was written, and a
/// retry by identity reads `wrong_state`, never `already_consumed` —
/// « its effect happened once » would be a lie.
#[test]
fn a_stale_apply_leaves_the_proposal_undecided() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![PROPOSED]);
    let TurnOutcome::Proposal { id, .. } = s.turn("write me a daily digest workflow") else {
        panic!("a proposal");
    };
    std::fs::write(dir.path().join("daily.nika"), "nika: raced\n").expect("the race");
    let TurnOutcome::Refusal(stale) = s.consent_to(&id, "yes") else {
        panic!("stale");
    };
    assert_eq!(stale.class, RefusalClass::StaleRevision, "{stale}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("daily.nika")).expect("still there"),
        "nika: raced\n",
        "nothing was applied"
    );
    let TurnOutcome::Refusal(again) = s.consent_to(&id, "yes") else {
        panic!("wrong state");
    };
    assert_eq!(
        again.class,
        RefusalClass::WrongState,
        "undecided, never consumed: {again}"
    );
    assert!(s.pending_proposal().is_none());
}

/// A remote host judges by identity (ADR-133): a consent naming a
/// proposal that is not the one waiting is stale and applies nothing;
/// the same proposal consents once; the same gate answers once.
#[test]
fn a_remote_host_drives_the_machine_by_identity() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![PROPOSED, PROPOSED]);
    let TurnOutcome::Proposal { id, .. } = s.turn("write me a daily digest workflow") else {
        panic!("a proposal");
    };
    assert_eq!(s.pending_proposal().as_ref(), Some(&id));
    let other = ProposalId::of("another preview");
    let TurnOutcome::Refusal(stale) = s.consent_to(&other, "yes") else {
        panic!("stale");
    };
    assert_eq!(stale.class, RefusalClass::StaleRevision, "{stale}");
    assert_eq!(
        s.pending_proposal().as_ref(),
        Some(&id),
        "a stale consent leaves the proposal waiting"
    );
    assert!(
        !dir.path().join("daily.nika").exists(),
        "nothing was applied"
    );
    assert!(matches!(
        s.consent_to(&id, "yes"),
        TurnOutcome::Facts(_) | TurnOutcome::RunRequested { .. }
    ));
    let TurnOutcome::Refusal(again) = s.consent_to(&id, "yes") else {
        panic!("consumed");
    };
    assert_eq!(again.class, RefusalClass::AlreadyConsumed, "{again}");
    assert!(s.pending_proposal().is_none());
    let TurnOutcome::Refusal(none) = s.consent("yes") else {
        panic!("nothing pending");
    };
    assert!(none.text.contains("nothing is pending"), "{none}");
    let TurnOutcome::Refusal(foreign) = s.consent_to(&other, "yes") else {
        panic!("wrong state");
    };
    assert_eq!(foreign.class, RefusalClass::WrongState, "{foreign}");
    assert!(
        foreign.text.contains(&other.to_string()) && !foreign.text.contains(&id.to_string()),
        "the refusal names the caller's id, never the last decided one: {foreign}"
    );
    assert!(s.waiting_gate().is_none());
    let gate = GateId::new(Path::new("never.ndjson"), "gate");
    let TurnOutcome::Refusal(no_gate) = s.answer_gate_for(&gate, "yes") else {
        panic!("no gate");
    };
    assert_eq!(no_gate.class, RefusalClass::WrongState, "{no_gate}");
}
