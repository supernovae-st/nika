// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authoring conversation over the ONE compiler, keyless: work reaches
//! the compiler, its question owns the next line, the same plan replays,
//! the Ready candidate is proposed as exact bytes, consent lands them and
//! the real check follows, an explicit run line requests the run. Every
//! pending state gives a `yes` exactly one meaning.

use std::path::Path;

use nika_onboard::compile::{CompileRequest, compile};

use super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::reasoner::ScriptedReasoner;

const DRAFT: &str = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";
const COPY: &str = "Read ./notes/brief.md and write it to ./out/copy.md";
const COPY_FR: &str = "Lis ./notes/brief.md et écris-le dans ./out/copie.md";
const GATED: &str =
    "Read ./draft.md and write it to ./final.md, but a human must approve the write first";

fn world() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(
        dir.path().join("notes/brief.md"),
        "# Brief\n\nThe launch moves to October.\n",
    )
    .expect("brief");
    dir
}

/// A session whose reasoner answers in words only, over a local engine.
fn open(root: &Path, replies: &[&str]) -> SessionRuntime {
    let local = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        model: None,
        locus: DataLocus::Local,
        ready: true,
        why: None,
    };
    SessionRuntime::open(
        root,
        local,
        Box::new(ScriptedReasoner::new(
            replies.iter().map(|r| (*r).to_owned()).collect(),
        )),
    )
}

fn workflows(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .expect("root")
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|n| Path::new(n).extension().is_some_and(|ext| ext == "nika"))
        .collect();
    names.sort();
    names
}

#[test]
fn work_reaches_the_compiler_and_its_question_owns_the_next_line() {
    let root = world();
    let mut s = open(root.path(), &[]);
    let TurnOutcome::Question { key, question } = s.turn(DRAFT) else {
        panic!("a draft needs its model: the compiler asks");
    };
    assert_eq!(key, "model");
    assert!(question.contains("provider/model"), "{question}");
    assert!(
        question.contains("reply on the next line (`model`)"),
        "{question}"
    );
    assert_eq!(s.pending_question().map(|q| q.key.as_str()), Some("model"));
    assert!(s.pending_proposal().is_none());
    assert_eq!(
        s.intent.unresolved.len(),
        1,
        "the open question is recorded"
    );
    assert!(
        workflows(root.path()).is_empty(),
        "nothing is written by a question"
    );

    let TurnOutcome::Proposal { id, preview } = s.turn("mock/echo") else {
        panic!("the answer settles the round: the candidate is proposed");
    };
    assert!(s.pending_question().is_none());
    assert!(s.intent.unresolved.is_empty());
    assert_eq!(s.pending_proposal(), Some(id));
    assert!(
        preview.starts_with("Nika proposes `compiled-workflow.nika`:"),
        "{preview}"
    );
    assert!(preview.contains("· nika:read"), "{preview}");
    assert!(preview.contains("infer · mock/echo"), "{preview}");
    assert!(preview.contains("· nika:write"), "{preview}");
    assert!(
        preview.contains("human approval at run · none"),
        "{preview}"
    );
    assert!(
        preview.contains("creates `compiled-workflow.nika`"),
        "{preview}"
    );
    assert!(preview.contains("check of these bytes"), "{preview}");
    assert!(
        preview.ends_with("nothing is written until you say yes)"),
        "{preview}"
    );
    assert!(
        workflows(root.path()).is_empty(),
        "a proposal writes nothing"
    );

    let TurnOutcome::Facts(report) = s.consent("yes") else {
        panic!("consent lands the set and reports the check");
    };
    assert!(
        report.contains("applied · wrote `compiled-workflow.nika`"),
        "{report}"
    );
    assert!(
        report.contains("check · `compiled-workflow.nika` · clean ✔"),
        "{report}"
    );
    assert!(report.contains("say « run it »"), "{report}");
    assert_eq!(
        workflows(root.path()),
        vec!["compiled-workflow.nika".to_owned()]
    );

    // Adapter parity (the product lane owns it): the bytes the session
    // landed are the bytes the compiler returns for the same request.
    let direct = compile(&CompileRequest::create(DRAFT).answer("model", "\"mock/echo\""))
        .expect("direct compile");
    let landed =
        std::fs::read_to_string(root.path().join("compiled-workflow.nika")).expect("landed");
    assert_eq!(Some(landed.as_str()), direct.candidate.as_deref());

    let TurnOutcome::RunRequested { report, run } = s.turn("run it") else {
        panic!("an explicit run line requests the run of the accepted workflow");
    };
    assert!(report.contains("clean ✔"), "{report}");
    assert_eq!(run.workflow, PathBuf::from("compiled-workflow.nika"));
    assert!(run.vars.is_empty());
    assert!((run.max_cost_usd - DEFAULT_CEILING_USD).abs() < f64::EPSILON);
}

#[test]
fn a_copy_intent_is_ready_at_once_in_french_and_the_reasoner_is_never_asked() {
    let root = world();
    let mut s = open(root.path(), &[]);
    let TurnOutcome::Proposal { .. } = s.turn(COPY_FR) else {
        panic!("an explicit intent is Ready without a seat and without a question");
    };
    let TurnOutcome::Facts(report) = s.consent("oui") else {
        panic!("the consent lands it");
    };
    assert!(report.contains("clean ✔"), "{report}");
    let landed =
        std::fs::read_to_string(root.path().join("compiled-workflow.nika")).expect("landed");
    assert!(landed.contains("./out/copie.md"), "{landed}");
    assert!(landed.contains("nika:write"), "{landed}");
    assert_eq!(s.intent.goal.as_deref(), Some(COPY_FR));
}

#[test]
fn a_second_candidate_never_replaces_the_first() {
    let root = world();
    let mut s = open(root.path(), &[]);
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(_)));
    let TurnOutcome::Proposal { preview, .. } = s.turn(COPY_FR) else {
        panic!("proposal");
    };
    assert!(
        preview.contains("creates `compiled-workflow-2.nika`"),
        "{preview}"
    );
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(_)));
    assert_eq!(
        workflows(root.path()),
        vec![
            "compiled-workflow-2.nika".to_owned(),
            "compiled-workflow.nika".to_owned()
        ]
    );
}

#[test]
fn a_yes_answers_exactly_one_boundary() {
    let root = world();
    let mut s = open(root.path(), &["sure."]);
    // Nothing pending: a consent is the wrong state, never an apply.
    let TurnOutcome::Refusal(why) = s.consent("yes") else {
        panic!("refused");
    };
    assert_eq!(why.class, RefusalClass::WrongState);
    // A question pending: the consent door still has nothing.
    assert!(matches!(s.turn(DRAFT), TurnOutcome::Question { .. }));
    let TurnOutcome::Refusal(why) = s.consent("yes") else {
        panic!("refused");
    };
    assert_eq!(why.class, RefusalClass::WrongState);
    assert!(s.pending_question().is_some(), "the question still waits");
    // `no` is an answer to a question, never an abandonment: it fills the
    // hole (`no` is not a provider/model, so the compiler asks again).
    let outcome = s.turn("no");
    assert!(
        !matches!(outcome, TurnOutcome::Proposal { .. }),
        "a wrong answer is never a candidate: {outcome:?}"
    );
    assert!(workflows(root.path()).is_empty());
}

#[test]
fn cancel_drops_the_round_and_an_empty_line_is_not_an_answer() {
    let root = world();
    let mut s = open(root.path(), &[]);
    assert!(matches!(s.turn(DRAFT), TurnOutcome::Question { .. }));
    let TurnOutcome::Refusal(why) = s.turn("   ") else {
        panic!("an empty line is refused");
    };
    assert_eq!(why.class, RefusalClass::EmptyAnswer);
    assert!(s.pending_question().is_some(), "the question still waits");
    let TurnOutcome::Facts(text) = s.turn("cancel") else {
        panic!("cancel drops the round");
    };
    assert!(text.contains("authoring discarded"), "{text}");
    assert!(s.pending_question().is_none());
    assert!(s.intent.unresolved.is_empty());
    assert!(workflows(root.path()).is_empty());
}

#[test]
fn a_reply_never_becomes_a_file() {
    let root = world();
    let reply = "Here you go.\n\n```yaml path=evil.nika\nnika: evil\npermits: {}\ntasks:\n  t:\n    invoke: { tool: \"nika:log\", args: { message: hi } }\n```\n";
    let mut s = open(root.path(), &[reply]);
    let TurnOutcome::Reply(text) = s.turn("hello there, how are you today?") else {
        panic!("a line that reads as no work is the conversation's");
    };
    assert!(text.contains("evil.nika"), "the words are shown as words");
    assert!(
        s.pending_proposal().is_none(),
        "a reply is never a proposal"
    );
    assert!(workflows(root.path()).is_empty());
    let TurnOutcome::Refusal(why) = s.consent("yes") else {
        panic!("nothing to consent to");
    };
    assert_eq!(why.class, RefusalClass::WrongState);
    assert!(!root.path().join("evil.nika").exists());
}

#[test]
fn an_intent_the_reader_cannot_settle_is_an_honest_incomplete_without_a_seat() {
    let root = world();
    let mut s = open(root.path(), &[]);
    let TurnOutcome::Facts(text) = s.turn(GATED) else {
        panic!("no seat: the deterministic reasons are stated, nothing is invented");
    };
    assert!(
        text.starts_with("I read this as work but cannot settle it"),
        "{text}"
    );
    assert!(text.contains("rephrase"), "{text}");
    assert!(s.pending_question().is_none());
    assert!(s.pending_proposal().is_none());
    assert!(workflows(root.path()).is_empty());
}

#[test]
fn a_stale_destination_refuses_the_apply_and_overwrites_nothing() {
    let root = world();
    let mut s = open(root.path(), &[]);
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    std::fs::write(
        root.path().join("compiled-workflow.nika"),
        "nika: someone-else\n",
    )
    .expect("appeared after the preview");
    let TurnOutcome::Refusal(why) = s.consent("yes") else {
        panic!("a target that appeared is stale");
    };
    assert_eq!(why.class, RefusalClass::StaleRevision, "{why}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("compiled-workflow.nika")).expect("kept"),
        "nika: someone-else\n"
    );
}

#[test]
fn a_run_line_needs_a_workflow_and_a_clean_check() {
    let root = world();
    let mut s = open(root.path(), &[]);
    let TurnOutcome::Refusal(why) = s.turn("run it") else {
        panic!("nothing to run");
    };
    assert_eq!(why.class, RefusalClass::WrongState);
    std::fs::write(
        root.path().join("alpha.nika"),
        "nika: alpha\nmodel: mock/echo\npermits: {}\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\noutputs:\n  said: ${{ tasks.t.output }}\n",
    )
    .expect("alpha");
    std::fs::write(
        root.path().join("bad.nika"),
        "nika: bad\ntasks:\n  w:\n    invoke: { tool: \"nika:write\", args: { path: \"./x.txt\", content: y } }\n",
    )
    .expect("bad");
    let TurnOutcome::RunRequested { run, .. } = s.turn("run alpha.nika with a ceiling of 0.05")
    else {
        panic!("a named clean workflow is requested");
    };
    assert_eq!(run.workflow, PathBuf::from("alpha.nika"));
    assert!((run.max_cost_usd - 0.05).abs() < f64::EPSILON);
    let TurnOutcome::Facts(text) = s.turn("run bad.nika") else {
        panic!("findings stop a run before it starts");
    };
    assert!(
        text.contains("findings ✖ — the run was not started"),
        "{text}"
    );
    assert!(text.contains("NIKA-AUTH-006"), "{text}");
    assert!(!root.path().join("x.txt").exists());
}

#[test]
fn a_bare_greeting_is_the_conversations_never_the_hello_skeleton() {
    let root = world();
    let mut s = open(root.path(), &["hi!", "de rien"]);
    let TurnOutcome::Reply(text) = s.turn("hello") else {
        panic!("a lone greeting is words, not the `hello` lesson compiled");
    };
    assert_eq!(text, "hi!");
    assert!(matches!(s.turn("Merci !"), TurnOutcome::Reply(_)));
    assert!(s.pending_proposal().is_none());
    assert!(workflows(root.path()).is_empty());
}
