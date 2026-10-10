// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The knowledge a Session reads, typed for every host, and the one way on when its
//! configuration names a source the strict door refused: a line that would reach a model is held
//! before any request, exactly as typed; `/knowledge embedded` names the release this build
//! embeds for the conversation and resumes the line once; a line that needs no model is read as
//! before. Keyless: the recorded reasoner keeps every prompt a model would have read.

use std::path::Path;
use std::sync::{Arc, Mutex};

use nika_cli_host::compile::config::AuthoringSettings;
use nika_onboard::knowledge::TrustedIdentity;
use nika_onboard::knowledge::pin::KnowledgePin;

use super::{BANNER, EMBEDDED};
use crate::activity::Phase;
use crate::authoring::{AuthoringContext, AuthoringSeat};
use crate::intelligence::{IntelligenceCensus, IntelligenceKind, UserIntelligencePreference};
use crate::outcome::RefusalClass;
use crate::reasoner::{ReasonError, Reply, ScriptedReasoner, SessionReasoner};
use crate::runtime::restore::KNOWLEDGE_HELP;
use crate::runtime::{HELP, SLASH_COMMANDS, SessionRuntime, TurnOutcome};
use crate::work::{Knowledge, Waiting};

/// Work the deterministic reader cannot settle alone: the seat would read it next.
const WORK: &str = "Read ./a.md and do something clever with it, then write ./b.md";
/// A line only an intelligence answers, in words.
const CHAT: &str = "hello there, how are you today?";
/// A Ready intent the deterministic compiler settles with no model.
const COPY: &str = "Read ./notes/brief.md and write it to ./out/copy.md";
/// An intent whose model the deterministic compiler asks for.
const DRAFT: &str = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";
/// The scripted model's words.
const REPLY: &str = "Fine, thanks — what shall we automate?";

/// The selected model, instrumented: every prompt it is handed, answered by the scripted fake.
/// It names the authoring model, so the route, the conversation and the seat are one model.
struct Recorded(Arc<Mutex<Vec<String>>>, ScriptedReasoner);

impl SessionReasoner for Recorded {
    fn name(&self) -> String {
        "recorded fixture".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        if let Ok(mut prompts) = self.0.lock() {
            prompts.push(prompt.to_owned());
        }
        self.1.reason(prompt)
    }

    fn authoring_model(&self) -> Option<String> {
        Some("mock/echo".to_owned())
    }
}

/// A project with the brief the deterministic intents read.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("project");
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(
        dir.path().join("notes/brief.md"),
        "# Brief\n\nThe launch moves to October.\n",
    )
    .expect("brief");
    dir
}

/// The configuration a session opens with when its environment names the release root `dir`
/// (`NIKA_KNOWLEDGE`), through the parser every door shares: refused, for want of a trusted
/// identity, before anything under `dir` is read.
fn overridden(dir: &Path) -> AuthoringContext {
    let env = AuthoringSettings::none().with_knowledge(dir, None);
    AuthoringContext::from_settings(&AuthoringSettings::none(), &env)
}

/// A session on the `mock/echo` API seat, its reasoner recorded, under `context` and the
/// continuous preparation every door opens with; `home` keeps its history when named.
fn seated(
    root: &Path,
    prompts: &Arc<Mutex<Vec<String>>>,
    context: AuthoringContext,
    home: Option<&Path>,
) -> SessionRuntime {
    let mut census = IntelligenceCensus::empty();
    census.api_keys.push("mock".to_owned());
    let api = IntelligenceKind::Api {
        provider: "mock".to_owned(),
    };
    let pref = UserIntelligencePreference::new(api, Some("mock/echo".to_owned()));
    let recorder = Arc::clone(prompts);
    let mut session = SessionRuntime::open_with(
        root,
        census,
        &pref,
        home,
        Box::new(move |_| {
            let replies = ScriptedReasoner::new(vec![REPLY.to_owned()]);
            Box::new(Recorded(Arc::clone(&recorder), replies))
        }),
    );
    session.set_authoring_context(context);
    session.enable_continuous_preparation();
    session
}

fn prompts_seen(prompts: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    prompts.lock().expect("prompts").clone()
}

/// The issued identity of the release this build embeds, as its own pin states it.
fn embedded_pin() -> KnowledgePin {
    KnowledgePin::embedded(None).expect("the embedded release is admitted")
}

/// A fresh session reads the release this build embeds: the typed state names its identity and
/// manifest digest, chosen by default, and the opening says nothing about knowledge; the action
/// is neither offered nor needed, and asked anyway it changes nothing, said.
#[test]
fn a_default_session_reads_the_embedded_release_and_says_nothing_about_it() {
    let root = project();
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let mut s = seated(root.path(), &prompts, AuthoringContext::default(), None);
    let pin = embedded_pin();
    assert_eq!(
        s.work().knowledge,
        Some(Knowledge::Admitted {
            source: "embedded",
            version: pin.version.clone(),
            manifest_sha256: pin.manifest_sha256.clone(),
            by: "default",
        })
    );
    let profile = pin.identity.as_ref().map(TrustedIdentity::profile);
    assert_eq!(
        profile,
        Some("nika-knowledge-release-profile/r2"),
        "the issued r2 release"
    );
    assert!(!s.banner().contains("knowledge"), "{}", s.banner());
    assert_eq!(s.slash_commands(), SLASH_COMMANDS.to_vec());
    assert_eq!(s.help_card(), HELP);
    let TurnOutcome::Facts(card) = s.turn("/knowledge") else {
        panic!("`/knowledge` answers from the session's own facts");
    };
    assert!(card.starts_with("knowledge: built into Nika · "), "{card}");
    let TurnOutcome::Facts(again) = s.turn("/knowledge embedded") else {
        panic!("a no-op is said");
    };
    assert!(
        again.contains("already in use") && again.ends_with("nothing changed"),
        "{again}"
    );
    assert_eq!(
        s.work()
            .knowledge
            .map(|k| matches!(k, Knowledge::Admitted { by: "default", .. })),
        Some(true)
    );
    assert!(prompts_seen(&prompts).is_empty());
}

/// A historical override names a release root with no trusted identity: the typed state says it
/// was refused (what, by which layer, the door's code and cause, no host path); the opening keeps
/// its warning and names the one action, which the completions and the help card offer too.
#[test]
fn a_refused_override_is_typed_and_offers_one_action() {
    let root = project();
    let release = tempfile::tempdir().expect("an old release root");
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let s = seated(root.path(), &prompts, overridden(release.path()), None);
    let Some(Knowledge::Refused {
        source,
        by,
        code,
        cause,
    }) = s.work().knowledge
    else {
        panic!("refused, typed: {:?}", s.work().knowledge);
    };
    assert_eq!(
        (source, by, code.as_str()),
        ("snapshot", "environment", "ADMISSION_UNTRUSTED")
    );
    assert!(cause.starts_with("no trusted expected identity"), "{cause}");
    let root_words = release.path().display().to_string();
    assert!(!cause.contains(&root_words), "no host path: {cause}");
    let banner = s.banner();
    assert!(
        banner.contains(
            "\n  ⚠ authoring knowledge: knowledge unavailable: the strict door refused `"
        ),
        "the warning stays as a host reads it: {banner}"
    );
    let warned = format!("\n{BANNER}\n  ⚠ authoring knowledge: knowledge unavailable: ");
    assert!(
        banner.contains(&warned),
        "the action, then the warning, last: {banner}"
    );
    assert!(!BANNER.contains(&root_words) && !BANNER.contains("--no-knowledge"));
    assert_eq!(s.slash_commands().first(), Some(&"/knowledge"));
    assert!(s.help_card().contains(KNOWLEDGE_HELP));
    assert_eq!(s.waiting(), Waiting::Free, "nothing waits before a line");
    assert!(prompts_seen(&prompts).is_empty());
}

/// Under that override a work line and a chat line each wait before any model request, exactly
/// as typed; the words scope « not sent » to that message, offer the one action and suggest no
/// flag a Session cannot take. What waits answers the read-only commands beside it, refuses
/// anything else and drops the line on `cancel`; no authoring starts and nothing is written.
#[test]
fn a_refused_override_holds_work_and_chat_before_any_model_request() {
    let root = project();
    let release = tempfile::tempdir().expect("an old release root");
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let mut s = seated(root.path(), &prompts, overridden(release.path()), None);
    let phases = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&phases);
    s.on_activity(Arc::new(move |activity| {
        if let Ok(mut seen) = sink.lock() {
            seen.push(activity.phase);
        }
    }));
    let seat = AuthoringSeat::Provider {
        model: "mock/echo".to_owned(),
    };
    assert_eq!(s.authoring_seat(), &seat, "the seat reads the work next");
    let root_words = release.path().display().to_string();
    for line in [WORK, CHAT] {
        let TurnOutcome::Ask(words) = s.submit(line, &Waiting::Free) else {
            panic!("{line}: held, not sent");
        };
        assert!(
            words.starts_with("Knowledge override not admitted (NIKA_KNOWLEDGE · ADMISSION_UNTRUSTED) · this message reached no model and waits"),
            "{words}"
        );
        let action = "`/knowledge embedded` uses the knowledge built into Nika and resumes it";
        assert!(words.contains(action), "{words}");
        for wall in [
            "--no-knowledge",
            "open the session again",
            root_words.as_str(),
        ] {
            assert!(!words.contains(wall), "{line}: {wall} in {words}");
        }
        let held = Waiting::KnowledgeChoice {
            line: line.to_owned(),
        };
        assert_eq!(s.waiting(), held);
        let status = s.status_line();
        assert!(status.contains("/knowledge embedded"), "{status}");
        assert!(
            prompts_seen(&prompts).is_empty(),
            "{line}: no model read it"
        );
        assert!(s.routes().is_empty(), "{line}: no route was asked");
        // Beside it: a read-only command answers, another line is refused, the line waits.
        let status = s.submit("/status", &held);
        assert!(matches!(status, TurnOutcome::Facts(_)), "{status:?}");
        let TurnOutcome::Refusal(refused) = s.submit("what about tomorrow?", &held) else {
            panic!("another line does not replace the held one");
        };
        assert_eq!(refused.class, RefusalClass::WrongState);
        assert!(
            refused.text.contains("this line was not sent"),
            "{refused:?}"
        );
        assert_eq!(s.waiting(), held);
        let TurnOutcome::Facts(dropped) = s.submit("cancel", &held) else {
            panic!("`cancel` drops the held line");
        };
        assert!(dropped.contains("not sent anywhere"), "{dropped}");
        assert_eq!(s.waiting(), Waiting::Free);
    }
    assert!(prompts_seen(&prompts).is_empty());
    let started = phases.lock().expect("activity");
    let authored = |phase: &Phase| matches!(phase, Phase::Authoring | Phase::Repairing);
    assert!(!started.iter().any(authored), "{started:?}");
    let written = std::fs::read_dir(root.path()).expect("project").flatten();
    let workflows = written.filter(|entry| entry.path().extension().is_some_and(|e| e == "nika"));
    assert_eq!(workflows.count(), 0, "nothing was written");
}

/// `/knowledge embedded` names the release this build embeds for the conversation: the typed
/// identity becomes it, the held line resumes once, exactly as typed, under the same intelligence
/// and effort; a second ask is a no-op, said.
#[test]
fn the_embedded_choice_resumes_the_held_line_once_and_changes_nothing_else() {
    let root = project();
    let release = tempfile::tempdir().expect("an old release root");
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let mut s = seated(root.path(), &prompts, overridden(release.path()), None);
    let intelligence = s.work().intelligence;
    assert!(matches!(
        s.submit(CHAT, &Waiting::Free),
        TurnOutcome::Ask(_)
    ));
    let shown = s.waiting();
    let TurnOutcome::Resumed { notice, outcome } = s.submit("/knowledge embedded", &shown) else {
        panic!("the choice resumes the held line");
    };
    assert!(
        notice.starts_with("knowledge: built into Nika · ")
            && notice.ends_with("holds for this conversation"),
        "{notice}"
    );
    assert_eq!(*outcome, TurnOutcome::Reply(REPLY.to_owned()));
    let seen = prompts_seen(&prompts);
    assert_eq!(seen.len(), 1, "one request, the held line's: {seen:#?}");
    assert!(seen[0].contains(CHAT), "{}", seen[0]);
    let pin = embedded_pin();
    assert_eq!(
        s.work().knowledge,
        Some(Knowledge::Admitted {
            source: "embedded",
            version: pin.version.clone(),
            manifest_sha256: pin.manifest_sha256,
            by: "conversation",
        })
    );
    assert_eq!(
        s.work().intelligence,
        intelligence,
        "the same intelligence and effort"
    );
    assert_eq!(s.waiting(), Waiting::Free);
    assert!(!s.banner().contains("knowledge"), "{}", s.banner());
    let TurnOutcome::Facts(again) = s.submit("/knowledge embedded", &Waiting::Free) else {
        panic!("a second choice is a no-op, said");
    };
    assert!(again.contains("already in use"), "{again}");
    assert_eq!(prompts_seen(&prompts).len(), 1, "nothing resent");
    let TurnOutcome::Facts(card) = s.turn("/knowledge") else {
        panic!("the card");
    };
    assert!(card.ends_with("chosen in this conversation"), "{card}");
}

/// A line that needs no model is read as before under a refused source: a fact, a deterministic
/// proposal and a question the deterministic compiler asks; the answer to that question, which the
/// seat reads next, waits, and resumes under the embedded release into the round it answers.
#[test]
fn lines_that_need_no_model_keep_their_reading_and_an_answer_waits_for_the_seat() {
    let root = project();
    let release = tempfile::tempdir().expect("an old release root");
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let mut s = seated(root.path(), &prompts, overridden(release.path()), None);
    assert!(matches!(
        s.submit("what workflows are here?", &Waiting::Free),
        TurnOutcome::Facts(_)
    ));
    assert!(matches!(
        s.submit(COPY, &Waiting::Free),
        TurnOutcome::Proposal { .. }
    ));
    assert!(matches!(
        s.submit("no", &s.waiting()),
        TurnOutcome::Facts(_)
    ));
    let TurnOutcome::Question { key, .. } = s.submit(DRAFT, &Waiting::Free) else {
        panic!("the deterministic compiler asks the model, no model asked");
    };
    assert_eq!(key, "model");
    assert!(matches!(s.submit("", &s.waiting()), TurnOutcome::Ask(_)));
    assert_eq!(
        s.waiting(),
        Waiting::KnowledgeChoice {
            line: String::new()
        }
    );
    assert!(
        s.pending_question().is_some(),
        "the question still waits under it"
    );
    assert!(prompts_seen(&prompts).is_empty());
    let TurnOutcome::Resumed { outcome, .. } = s.submit("/knowledge embedded", &s.waiting()) else {
        panic!("resumed");
    };
    assert!(
        !matches!(*outcome, TurnOutcome::Ask(_) | TurnOutcome::Refusal(_)),
        "the answer reached its round under the embedded release: {outcome:?}"
    );
    assert!(s.pending_question().is_none(), "the round took the answer");
}

/// The choice is the conversation's: its history keeps it, and reopening the same conversation
/// under the same refused override reads the embedded release with no warning; a session without
/// that history still meets the refusal.
#[test]
fn the_embedded_choice_is_kept_with_the_conversation() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let release = tempfile::tempdir().expect("an old release root");
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let mut s = seated(
        root.path(),
        &prompts,
        overridden(release.path()),
        Some(home.path()),
    );
    s.enable_history(home.path()).expect("history");
    let TurnOutcome::Facts(notice) = s.submit("/knowledge embedded", &Waiting::Free) else {
        panic!("applied, nothing held");
    };
    assert!(notice.ends_with("holds for this conversation"), "{notice}");
    assert_eq!(s.knowledge.conversation.as_deref(), Some(EMBEDDED));
    drop(s);
    let mut again = seated(
        root.path(),
        &prompts,
        overridden(release.path()),
        Some(home.path()),
    );
    again.enable_history(home.path()).expect("history");
    assert!(
        matches!(
            again.work().knowledge,
            Some(Knowledge::Admitted {
                source: "embedded",
                by: "conversation",
                ..
            })
        ),
        "{:?}",
        again.work().knowledge
    );
    assert!(!again.banner().contains("knowledge"), "{}", again.banner());
    drop(again);
    let elsewhere = project();
    let fresh = seated(elsewhere.path(), &prompts, overridden(release.path()), None);
    assert!(matches!(
        fresh.work().knowledge,
        Some(Knowledge::Refused { .. })
    ));
}

/// The action never makes anything worse: where the embedded release cannot be read (the
/// strategy `off` reads no knowledge), it changes nothing and says why; a configuration refused
/// for another reason is no knowledge refusal and holds nothing.
#[test]
fn an_embedded_choice_that_cannot_apply_changes_nothing() {
    let root = project();
    let release = tempfile::tempdir().expect("an old release root");
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let env = AuthoringSettings::none()
        .with_strategy("off")
        .with_knowledge(release.path(), None);
    let unread = AuthoringContext::from_settings(&AuthoringSettings::none(), &env);
    let mut s = seated(root.path(), &prompts, unread, None);
    assert!(matches!(s.work().knowledge, Some(Knowledge::Unread { .. })));
    let before = s.authoring_context().clone();
    let TurnOutcome::Refusal(refused) = s.submit("/knowledge embedded", &Waiting::Free) else {
        panic!("refused, said");
    };
    assert!(
        refused.text.ends_with("nothing changed"),
        "{}",
        refused.text
    );
    assert_eq!(s.authoring_context(), &before, "nothing changed");
    assert!(s.knowledge.conversation.is_none());
    let TurnOutcome::Refusal(other) = s.submit("/knowledge off", &Waiting::Free) else {
        panic!("not a choice");
    };
    assert!(
        other.text.contains("is not a knowledge choice"),
        "{}",
        other.text
    );
    assert!(prompts_seen(&prompts).is_empty());
}
