// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A value said in words at an authoring question binds through its typed reading
//! (`runtime/answer.rs`), never as the whole sentence. DIALOG-01 (2026-09-24, f140be40):
//! « Copie entree.txt vers le fichier que je vais choisir. », the native seat asked
//! « Destination file path », and « Écris dans sortie.txt. » became the write path. The
//! intelligence here is a scripted stand-in whose prompts the test reads; the native
//! round is the seat's own record, replayed with zero calls, exactly as an answer round.

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};

use nika_onboard::compile::{CompileOutcome, CompileRequest, compile, intent_sha256};
use serde_json::json;

use super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::reasoner::{NoReasoner, Reply};

const INTENT: &str = "Copie entree.txt vers le fichier que je vais choisir.";
const ORIGINAL: &str = "Écris dans sortie.txt.";

/// The seat's candidate in DIALOG-01's shape: its destination is a declared placeholder,
/// and the write boundary the one narrow form the judge admits while the path is asked.
const SEAT: &str = r#"nika: copy-to-chosen-file
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

/// The chosen intelligence's stand-in: canned readings in order (an `Err` is a failed
/// call), every prompt it received sent where the test reads it.
struct Reader {
    replies: Vec<Result<&'static str, &'static str>>,
    seen: Sender<String>,
}

impl SessionReasoner for Reader {
    fn name(&self) -> String {
        "scripted".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        let _ = self.seen.send(prompt.to_owned());
        let next = if self.replies.is_empty() {
            Ok("")
        } else {
            self.replies.remove(0)
        };
        next.map(|text| Reply {
            text: text.to_owned(),
            usage_observed: false,
        })
        .map_err(|why| ReasonError::Provider(why.to_owned()))
    }
}

fn world() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("root");
    std::fs::write(dir.path().join("entree.txt"), "A\n").expect("entree");
    dir
}

/// A session whose chosen intelligence reads answers (a local engine), authoring
/// deterministically (the stand-in names no authoring model).
fn reading(
    root: &Path,
    replies: Vec<Result<&'static str, &'static str>>,
) -> (SessionRuntime, Receiver<String>) {
    let (seen, prompts) = channel();
    let local = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        model: None,
        locus: DataLocus::Local,
        ready: true,
        why: None,
    };
    let session = SessionRuntime::open(root, local, Box::new(Reader { replies, seen }));
    (session, prompts)
}

/// A session with no intelligence chosen: replies are taken as typed.
fn literal(root: &Path) -> SessionRuntime {
    let none = ResolvedSessionIntelligence {
        kind: IntelligenceKind::None,
        model: None,
        locus: DataLocus::None,
        ready: false,
        why: None,
    };
    SessionRuntime::open(root, none, Box::new(NoReasoner))
}

/// The round waits on the first question the outcome asks.
fn wait_on(s: &mut SessionRuntime, intent: &str, out: &CompileOutcome) -> String {
    let mut round = AuthoringRound::new(intent);
    round.absorb(out);
    let key = round
        .current()
        .map(|q| q.key.clone())
        .expect("a question waits");
    s.authoring = Some(round);
    key
}

/// DIALOG-01's native round: the seat's record replayed (zero calls), its destination asked.
fn at_the_destination(s: &mut SessionRuntime) {
    let record = json!({
        "strategy": "native",
        "intent_sha256": intent_sha256(INTENT),
        "source": SEAT,
        "questions": [{
            "key": "const.destination_path",
            "label": "Destination file path",
            "answer_type": "text",
            "why": "The request says the user will choose the destination file for the copy."
        }],
        "gaps": [],
        "trigger": null,
    });
    let out = compile(&CompileRequest::create(INTENT).with_plan(record)).expect("replays");
    assert_eq!(
        wait_on(s, INTENT, &out),
        "const.destination_path",
        "{out:#?}"
    );
}

/// An exact skeleton's first question, unrelated to DIALOG-01.
fn at_a_skeleton_question(s: &mut SessionRuntime, skeleton: &str) -> String {
    let out = compile(&CompileRequest::create(skeleton)).expect("compiles");
    wait_on(s, skeleton, &out)
}

/// What the session recorded for the line the human answered with.
fn answered(s: &SessionRuntime, line: &str) -> Option<String> {
    s.recent
        .iter()
        .rev()
        .find(|(said, noted)| said == line && noted.starts_with("(answered "))
        .map(|(_, noted)| noted.clone())
}

fn candidate(s: &SessionRuntime) -> String {
    s.last_outcome
        .as_ref()
        .and_then(|out| out.candidate.clone())
        .unwrap_or_default()
}

fn workflows(root: &Path) -> Vec<String> {
    std::fs::read_dir(root)
        .expect("root")
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|n| Path::new(n).extension().is_some_and(|ext| ext == "nika"))
        .collect()
}

/// The original DIALOG-01 reply, before and after: the old binding typed the whole sentence
/// as the path; the reading binds the value the human named, says so beside the proposal,
/// and writes nothing before consent.
#[test]
fn the_original_reply_binds_the_destination_it_names_not_the_sentence() {
    let root = world();
    let (mut s, prompts) = reading(root.path(), vec![Ok("sortie.txt")]);
    at_the_destination(&mut s);
    let question = s
        .pending_question()
        .cloned()
        .expect("the destination waits");
    // Before: the line as the value, the sentence baked as the path.
    assert_eq!(
        crate::authoring::literal_for(&question, ORIGINAL),
        "\"Écris dans sortie.txt.\""
    );
    let TurnOutcome::Proposal { preview, .. } = s.turn(ORIGINAL) else {
        panic!(
            "the destination is bound and the candidate proposed: {:?}",
            s.routes()
        );
    };
    assert!(
        preview.starts_with("read your answer as « sortie.txt » (from « Écris dans sortie.txt. »"),
        "{preview}"
    );
    assert_eq!(
        answered(&s, ORIGINAL).as_deref(),
        Some("(answered const.destination_path · read as « sortie.txt »)")
    );
    let source = candidate(&s);
    assert!(
        source.contains("destination_path: \"sortie.txt\""),
        "{source}"
    );
    assert!(
        source.contains("write: [\"sortie.txt\"]"),
        "the placeholder completes: {source}"
    );
    assert!(
        !source.contains("Écris"),
        "the sentence is never a value: {source}"
    );
    // One bounded reading: the question in its own words, the reply verbatim.
    let sent: Vec<String> = prompts.try_iter().collect();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains("«Destination file path»"), "{}", sent[0]);
    assert!(sent[0].contains("«Écris dans sortie.txt.»"), "{}", sent[0]);
    // Nothing lands before consent, and consent is never a run.
    assert!(workflows(root.path()).is_empty());
    assert!(!root.path().join("sortie.txt").exists());
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(ref t) if t.contains("applied")));
    let saved = workflows(root.path());
    assert_eq!(saved.len(), 1, "{saved:?}");
    let bytes = std::fs::read_to_string(root.path().join(&saved[0])).expect("saved");
    assert!(bytes.contains("\"sortie.txt\""), "{bytes}");
    assert!(
        !root.path().join("sortie.txt").exists(),
        "consent saved, nothing ran"
    );
}

/// The same act in English, and a destination with a space — whole, or inside quotes.
#[test]
fn an_english_reply_and_a_path_with_spaces_bind_verbatim() {
    let cases: [(&str, &'static str, &str); 3] = [
        (
            "Please write it to out/report.md",
            "out/report.md",
            "out/report.md",
        ),
        (
            "exports/rapport final.txt",
            "exports/rapport final.txt",
            "exports/rapport final.txt",
        ),
        (
            "Mets la copie dans \"exports/rapport final.txt\".",
            "\"exports/rapport final.txt\"",
            "exports/rapport final.txt",
        ),
    ];
    for (line, reply, value) in cases {
        let root = world();
        let (mut s, _prompts) = reading(root.path(), vec![Ok(reply)]);
        at_the_destination(&mut s);
        let TurnOutcome::Proposal { .. } = s.turn(line) else {
            panic!("{line}: a proposal");
        };
        let source = candidate(&s);
        assert!(
            source.contains(&format!("destination_path: \"{value}\"")),
            "{line}: {source}"
        );
        assert!(
            source.contains(&format!("write: [\"{value}\"]")),
            "{line}: {source}"
        );
        let noted = answered(&s, line).expect("answered");
        if value == line {
            assert_eq!(
                noted, "(answered const.destination_path)",
                "an intentional literal stays whole"
            );
        } else {
            assert!(noted.ends_with(&format!("read as « {value} »)")), "{noted}");
        }
    }
}

/// Questions that are not DIALOG-01's: a literal currency code said in a sentence, an
/// intentional multi-word instruction, and quoted data taken as typed with no reading.
#[test]
fn unrelated_value_questions_read_the_same_way() {
    let root = world();
    let (mut s, prompts) = reading(root.path(), vec![Ok("EUR")]);
    let key = at_a_skeleton_question(&mut s, "aggregate-by-key");
    let line = "Use euros, the code EUR.";
    let _ = s.turn(line);
    assert_eq!(
        answered(&s, line),
        Some(format!("(answered {key} · read as « EUR »)"))
    );
    let sent: Vec<String> = prompts.try_iter().collect();
    assert!(
        sent.len() == 1 && sent[0].contains("«the currency code»"),
        "{sent:?}"
    );

    let instruction = "Summarize the notes in three bullets, oldest first";
    let (mut s, prompts) = reading(root.path(), vec![Ok(instruction)]);
    let key = at_a_skeleton_question(&mut s, "chain");
    let _ = s.turn(instruction);
    assert_eq!(answered(&s, instruction), Some(format!("(answered {key})")));
    assert_eq!(prompts.try_iter().count(), 1);

    let quoted = "\"Keep it short, then list the three dates.\"";
    let (mut s, prompts) = reading(root.path(), vec![Ok("NONE")]);
    let key = at_a_skeleton_question(&mut s, "chain");
    let _ = s.turn(quoted);
    assert_eq!(answered(&s, quoted), Some(format!("(answered {key})")));
    assert_eq!(prompts.try_iter().count(), 0, "a literal is never read");
}

/// Two destinations, an invented path, a fragment of a word: nothing is bound, the question
/// waits and says why; then a value alone binds.
#[test]
fn an_ambiguous_invented_or_partial_value_binds_nothing() {
    let root = world();
    let (mut s, prompts) = reading(
        root.path(),
        vec![Ok("NONE"), Ok("./out/sortie.txt"), Ok("tie.txt")],
    );
    at_the_destination(&mut s);
    for line in [
        "Écris dans sortie.txt ou dans resultat.txt",
        ORIGINAL,
        ORIGINAL,
    ] {
        let TurnOutcome::Question { key, question } = s.turn(line) else {
            panic!("{line}: the question waits");
        };
        assert_eq!(key, "const.destination_path");
        assert!(question.contains("nothing was bound"), "{question}");
        assert!(question.contains("Destination file path"), "{question}");
        assert!(s.pending_proposal().is_none());
        assert_eq!(
            s.pending_question().map(|q| q.key.as_str()),
            Some("const.destination_path")
        );
        assert!(answered(&s, line).is_none(), "{line}");
    }
    assert_eq!(prompts.try_iter().count(), 3);
    assert!(matches!(s.turn("sortie.txt"), TurnOutcome::Proposal { .. }));
    assert_eq!(prompts.try_iter().count(), 0, "one token is its own value");
    assert!(candidate(&s).contains("write: [\"sortie.txt\"]"));
}

/// A copy the model cuts out of a token — a file's extension, the name under its folder —
/// binds nothing, and the whole value binds before the sentence's period, a folder path
/// with a space included. One metered reading per reply, as before.
#[test]
fn a_piece_cut_out_of_a_token_binds_nothing() {
    let root = world();
    let (mut s, prompts) = reading(
        root.path(),
        vec![Ok("txt"), Ok("rapport.txt"), Ok("dir/rapport final.txt")],
    );
    at_the_destination(&mut s);
    for line in [ORIGINAL, "Mets-le dans exports/rapport.txt"] {
        let TurnOutcome::Question { question, .. } = s.turn(line) else {
            panic!("{line}: the question waits");
        };
        assert!(question.contains("nothing was bound"), "{question}");
        assert!(answered(&s, line).is_none(), "{line}");
    }
    let line = "Mets-le dans dir/rapport final.txt.";
    let TurnOutcome::Proposal { preview, .. } = s.turn(line) else {
        panic!("the whole value binds");
    };
    assert!(
        preview.starts_with("read your answer as « dir/rapport final.txt »"),
        "{preview}"
    );
    let source = candidate(&s);
    assert!(
        source.contains("write: [\"dir/rapport final.txt\"]"),
        "{source}"
    );
    assert_eq!(prompts.try_iter().count(), 3);
}

/// A failed reading, and a spending limit that refuses the reading before any call:
/// nothing is bound, never the sentence in its place.
#[test]
fn a_failed_or_refused_reading_leaves_the_question_waiting() {
    let root = world();
    let (mut s, prompts) = reading(root.path(), vec![Err("timed out")]);
    at_the_destination(&mut s);
    let TurnOutcome::Question { question, .. } = s.turn(ORIGINAL) else {
        panic!("the question waits");
    };
    assert!(
        question.starts_with("I could not read your answer"),
        "{question}"
    );
    assert!(
        question.contains("timed out") && question.contains("nothing was bound"),
        "{question}"
    );
    assert_eq!(prompts.try_iter().count(), 1);
    assert!(s.pending_proposal().is_none() && s.pending_question().is_some());

    let (mut s, prompts) = reading(root.path(), vec![Ok("sortie.txt")]);
    at_the_destination(&mut s);
    s.money.reconfirm = true;
    // The turn's own money gate refuses first, unchanged: the round expires, nothing bound…
    let refused = s.turn(ORIGINAL);
    assert!(matches!(refused, TurnOutcome::Refusal(_)), "{refused:?}");
    assert!(s.pending_question().is_none() && s.pending_proposal().is_none());
    // …and the reading itself never calls under a spending limit (the seam, reached directly).
    at_the_destination(&mut s);
    let TurnOutcome::Question { question, .. } = s.answer_question_unrecorded(ORIGINAL) else {
        panic!("the question waits");
    };
    assert!(question.contains("nothing was bound"), "{question}");
    assert_eq!(
        prompts.try_iter().count(),
        0,
        "no call under a spending limit"
    );
    assert!(s.pending_question().is_some());
    assert!(workflows(root.path()).is_empty());
}

/// An opaque identifier: alone it is its own value; said in a sentence, only the identifier
/// binds, exactly as typed.
#[test]
fn an_opaque_identifier_binds_verbatim() {
    let root = world();
    let (mut s, prompts) = reading(root.path(), vec![Ok("customers-2026")]);
    let key = at_a_skeleton_question(&mut s, "deduplicate-records");
    let line = "Label it customers-2026, please";
    let _ = s.turn(line);
    assert_eq!(
        answered(&s, line),
        Some(format!("(answered {key} · read as « customers-2026 »)"))
    );
    assert_eq!(prompts.try_iter().count(), 1);

    let (mut s, prompts) = reading(root.path(), vec![]);
    let key = at_a_skeleton_question(&mut s, "deduplicate-records");
    let _ = s.turn("cust_7Hq-2026");
    assert_eq!(
        answered(&s, "cust_7Hq-2026"),
        Some(format!("(answered {key})"))
    );
    assert_eq!(prompts.try_iter().count(), 0, "one token is its own value");
}

/// Without an intelligence the deterministic law stands — the reply is the value as typed —
/// and a value question says so; the seat's `model`, the replacement request and the
/// value questions of a reading session carry no such notice.
#[test]
fn without_an_intelligence_the_reply_is_taken_as_typed_and_the_question_says_so() {
    let root = world();
    let mut s = literal(root.path());
    let key = at_a_skeleton_question(&mut s, "aggregate-by-key");
    let question = s.pending_question().cloned().expect("waits");
    assert_eq!(
        s.as_typed_notice(&question),
        Some(super::answer::AS_TYPED_NOTICE)
    );
    let line = "Use euros, the code EUR.";
    let _ = s.turn(line);
    assert_eq!(answered(&s, line), Some(format!("(answered {key})")));

    let draft = compile(&CompileRequest::create(
        "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md",
    ))
    .expect("compiles");
    let model = draft
        .questions
        .iter()
        .find(|q| q.key == "model")
        .expect("the model");
    assert_eq!(
        s.as_typed_notice(model),
        None,
        "provider selection keeps its door"
    );
    let (reader, _prompts) = reading(root.path(), vec![]);
    assert_eq!(reader.as_typed_notice(&question), None);
}
