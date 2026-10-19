// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `model` question answered in the person's words (`model_answer.rs`). Live gauntlet of
//! 2026-10-15: on the codex account, « Lis notes/reunion.md et écris dans notes/resume.md un
//! résumé en 3 puces » asked « Which explicit runtime provider/model should run the language
//! steps? », « choisis toi-même » was taken as typed, and the compiler answered « Name the
//! runtime model as <provider>/<model> ». Here the session reads answers through a scripted
//! stand-in, authors deterministically, and holds an Anthropic key.

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};

use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
use nika_types::access::AccessClass;

use super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::reasoner::Reply;
use crate::work::{AnswerAct, Answered, ValueSource};

const DRAFT: &str = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";
const OFFERED: &str = "anthropic/claude-sonnet-5-5";

/// Canned readings in order; every prompt sent where the test reads it.
struct Reader {
    replies: Vec<&'static str>,
    seen: Sender<String>,
}

impl SessionReasoner for Reader {
    fn name(&self) -> String {
        "scripted".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        let _ = self.seen.send(prompt.to_owned());
        let next = if self.replies.is_empty() {
            ""
        } else {
            self.replies.remove(0)
        };
        Ok(Reply::new(next.to_owned(), false))
    }
}

fn world() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(dir.path().join("notes/brief.md"), "Budget validé.\n").expect("brief");
    dir
}

/// A keyed Anthropic route at its vendor endpoint: the catalogue lists its models.
fn anthropic() -> IntelligenceCensus {
    let endpoint = "https://api.anthropic.com/v1/messages";
    let locus = ExecutionLocus::classify(Some(endpoint), endpoint);
    let readiness = ProviderReadiness::new(true, true, None, None, false, locus, AccessClass::Api);
    let probe = ProviderProbe::new(
        "anthropic",
        true,
        true,
        "ANTHROPIC_API_KEY",
        true,
        readiness,
        endpoint,
    );
    let mut census = IntelligenceCensus::empty();
    census.api_keys.push("anthropic".to_owned());
    census.provider_context.push(probe);
    census
}

/// A session that reads answers, authors deterministically and holds an Anthropic key; at the
/// `model` question of [`DRAFT`].
fn at_the_model_question(
    root: &Path,
    replies: Vec<&'static str>,
) -> (SessionRuntime, Receiver<String>, String) {
    let (seen, prompts) = channel();
    let local = ResolvedSessionIntelligence::new(
        IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        None,
        DataLocus::Local,
        true,
        None,
    );
    let mut s = SessionRuntime::open(root, local, Box::new(Reader { replies, seen }));
    s.census = Some(anthropic());
    let TurnOutcome::Question { key, question } = s.turn(DRAFT) else {
        panic!("a draft needs its model: the compiler asks");
    };
    assert_eq!(key, "model");
    (s, prompts, question)
}

fn bound(s: &SessionRuntime, value: &str, reading: ValueSource) -> bool {
    matches!(&s.work().answered, Some(answered) if *answered == Answered::new(
        answered.question.clone(),
        AnswerAct::Bound { key: "model".to_owned(), value: value.to_owned(), reading },
    ))
}

/// With no model of its own, the question offers the configured model this conversation
/// recommends, and an empty line takes it: no `<provider>/<model>` to type.
#[test]
fn the_model_question_offers_a_configured_model_and_enter_takes_it() {
    let root = world();
    let (mut s, prompts, question) = at_the_model_question(root.path(), vec![]);
    assert!(
        question.contains(&format!("Enter takes `{OFFERED}` (recommended here)")),
        "{question}"
    );
    let TurnOutcome::Proposal { preview, .. } = s.turn("") else {
        panic!("the empty line takes the offered model");
    };
    assert!(preview.contains(&format!("infer · {OFFERED}")), "{preview}");
    assert!(
        bound(&s, OFFERED, ValueSource::SeatDefault),
        "{:?}",
        s.work().answered
    );
    assert_eq!(prompts.try_iter().count(), 0, "an empty line reads nothing");
}

/// « choisis toi-même » leaves the model to Nika: the offered model binds as Nika's choice,
/// said beside the proposal, after one reading that offered DELEGATED.
#[test]
fn a_reply_that_leaves_the_model_to_nika_takes_the_offered_one() {
    let root = world();
    let (mut s, prompts, _) = at_the_model_question(root.path(), vec!["DELEGATED"]);
    let TurnOutcome::Proposal { preview, .. } = s.turn("choisis toi-même") else {
        panic!(
            "the delegation binds the offered model: {:?}",
            s.work().answered
        );
    };
    assert!(
        preview.starts_with(&format!(
            "Nika chose « {OFFERED} », the model offered here, for « choisis toi-même »"
        )),
        "{preview}"
    );
    assert!(preview.contains(&format!("infer · {OFFERED}")), "{preview}");
    assert!(
        bound(&s, OFFERED, ValueSource::Delegated),
        "{:?}",
        s.work().answered
    );
    let sent: Vec<String> = prompts.try_iter().collect();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains("answer DELEGATED"), "{}", sent[0]);
    assert!(sent[0].contains(&format!("«{OFFERED}»")), "{}", sent[0]);
}

/// A model named in words is read against the configured models whose name carries a word of
/// the reply: « prends Claude Haiku » binds the Haiku the catalogue lists.
#[test]
fn a_model_named_in_words_binds_the_configured_one_it_designates() {
    let root = world();
    let haiku = "anthropic/claude-haiku-5-5";
    let (mut s, prompts, _) = at_the_model_question(root.path(), vec![haiku]);
    let TurnOutcome::Proposal { preview, .. } = s.turn("prends Claude Haiku") else {
        panic!("the named model binds: {:?}", s.work().answered);
    };
    assert!(preview.contains(&format!("infer · {haiku}")), "{preview}");
    assert!(
        bound(&s, haiku, ValueSource::ModelRead),
        "{:?}",
        s.work().answered
    );
    let sent: Vec<String> = prompts.try_iter().collect();
    assert!(sent[0].contains(&format!("«{haiku}»")), "{}", sent[0]);
}

/// A reading that names no offered model binds nothing: the question waits, naming the model
/// it offers.
#[test]
fn a_reading_outside_the_offered_models_binds_nothing() {
    let root = world();
    for reply in ["openai/gpt-9", "NONE", ""] {
        let (mut s, _, _) = at_the_model_question(root.path(), vec![reply]);
        let TurnOutcome::Question { key, question } = s.turn("un modèle rapide") else {
            panic!("nothing binds: the question waits");
        };
        assert_eq!(key, "model");
        assert!(
            question.contains("I could not tell which model « un modèle rapide » names")
                && question.contains(OFFERED),
            "{reply}: {question}"
        );
        assert!(s.pending_question().is_some_and(|q| q.key == "model"));
    }
}
