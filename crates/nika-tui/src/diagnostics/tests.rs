// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The presenter recognises the Session's exact refusals and nothing else.
//! Fixtures come from the Session's own public types: the configuration parser
//! and the strict knowledge door produce the cause, the Session's error type
//! its sentence, and a real runtime its opening banner. What a refused turn
//! adds is pinned to the Session's source, so a change of words there fails
//! here instead of silently painting the whole diagnostic again.

use std::path::{Path, PathBuf};

use nika_cli_host::compile::config::AuthoringSettings;
use nika_cli_host::compile::knowledge::{KnowledgeError, RefusalCode};
use nika_display::front_door::recovery;
use nika_session::authoring::{AuthoringContext, AuthoringContextError, AuthoringError};
use nika_session::{
    IntelligenceCensus, IntelligenceKind, ScriptedReasoner, SessionRuntime,
    UserIntelligencePreference,
};

use super::*;

/// A release root named for these fixtures only: nothing reads it, because the
/// strict door refuses an untrusted release before collecting anything.
pub(crate) const ROOT: &str = "/srv/foundry/release-r3";

/// The exact summary the presenter gives the untrusted-release refusal.
const SUMMARY: Summary = Summary {
    cause: "Nika cannot verify the knowledge release named by NIKA_KNOWLEDGE.",
    scope: "Nothing was sent to the authoring model and nothing was written.",
    next: "Next: quit and restart Nika with NIKA_KNOWLEDGE unset (built-in knowledge) or NIKA_KNOWLEDGE=off.",
    details: "Details: F2",
};

/// The opening banner's shortened knowledge warning, marker included.
pub(crate) const WARNING_LINE: &str = "  ⚠ Knowledge: Nika cannot verify the release named by NIKA_KNOWLEDGE, so authoring with a model will be refused. Details: F2";

/// The configuration a session opens with when its environment names the
/// release at `root` (`NIKA_KNOWLEDGE`), through the parser every door shares.
fn named(root: &str) -> AuthoringContext {
    let mut env = AuthoringSettings::none();
    env.knowledge = Some(PathBuf::from(root));
    AuthoringContext::from_settings(&AuthoringSettings::none(), &env)
}

/// The Session's refusal of a seated turn under `cause`: its error's own words,
/// then what the turn adds (pinned to the Session's source below).
fn refusal_of(cause: AuthoringContextError) -> String {
    format!("{}{NOT_SENT}", AuthoringError::Context(cause))
}

/// The Session's refusal of a seated turn when its environment names `root`.
pub(crate) fn knowledge_refusal(root: &str) -> String {
    let cause = named(root).refusal().cloned();
    refusal_of(cause.expect("a release the environment names carries no trusted identity"))
}

/// The Session's recovery card for a provider that failed under its seat, in
/// the Session's sentences: a failed call may still have reached the model.
pub(crate) fn provider_failure() -> String {
    let reason = AuthoringError::Seat("deepseek answered 503 Service Unavailable".to_owned());
    recovery::card(
        "I couldn't use the authoring seat for this part",
        &reason.to_string(),
        Some("seat: deepseek/deepseek-chat"),
        &[
            "your request: « read notes.md and write a one-paragraph digest to digest.md »"
                .to_owned(),
        ],
        "No workflow output was written or Run requested; the selected model (deepseek/deepseek-chat) may have received this turn's context: a failed call can still have been sent.\n  Preparation: 1 API requests observed · known estimate unknown · 1 unpriced requests · subscription and decision-service invoices unknown · Run has its own budget",
    )
}

/// A temporary project root, removed when the test ends.
struct Room(PathBuf);

impl Room {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "nika-tui-diagnostics-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("room");
        Self(path)
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The banner a real session opens with under `context`, no model chosen and
/// none called.
fn banner_under(context: AuthoringContext) -> String {
    let room = Room::new("banner");
    let mut runtime = SessionRuntime::open_with(
        Path::new(&room.0),
        IntelligenceCensus::empty(),
        &UserIntelligencePreference::new(IntelligenceKind::None, None),
        None,
        Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
    );
    runtime.set_authoring_context(context);
    runtime.banner()
}

/// The banner a real session opens with when its environment names `root`.
pub(crate) fn opening_banner(root: &str) -> String {
    banner_under(named(root))
}

/// The refusal the screenshot showed reads first as four plain sentences, and
/// the block keeps the Session's whole sentence as its detail.
#[test]
fn the_untrusted_release_refusal_reads_as_its_exact_summary() {
    let text = knowledge_refusal(ROOT);
    assert!(
        text.starts_with(
            "the authoring configuration cannot be used: knowledge unavailable: the strict door refused `/srv/foundry/release-r3` (ADMISSION_UNTRUSTED: no trusted expected identity"
        ),
        "{text}"
    );
    assert!(
        text.ends_with(" · nothing was sent to the authoring model, nothing was written · fix or unset the knowledge (NIKA_KNOWLEDGE · NIKA_AUTHORING_STRATEGY) and open the session again"),
        "{text}"
    );
    let block = Committed::new(Kind::Refusal, text.clone());
    assert_eq!(shown(&block), Shown::Refusal(&SUMMARY));
    assert_eq!(
        block.text, text,
        "the Session's words stay whole as the detail"
    );
    for sentence in [SUMMARY.cause, SUMMARY.scope, SUMMARY.next, SUMMARY.details] {
        assert!(sentence.is_ascii() && sentence.len() <= 100, "{sentence}");
        for wall in [
            ROOT,
            "ADMISSION_UNTRUSTED",
            "NIKA_AUTHORING_STRATEGY",
            "--no-knowledge",
        ] {
            assert!(!sentence.contains(wall), "{sentence} repeats {wall}");
        }
    }
}

/// Only the Session's complete sentence is summarised: a cause alone states no
/// scope, an addition or another code changes what happened, and the build's
/// own release is not something unsetting `NIKA_KNOWLEDGE` changes.
#[test]
fn any_other_wording_or_cause_is_painted_as_said() {
    let text = knowledge_refusal(ROOT);
    let cause = text
        .strip_suffix(NOT_SENT)
        .expect("the turn's words")
        .to_owned();
    let other = |code, detail: &str| {
        refusal_of(AuthoringContextError::Knowledge(
            KnowledgeError::Unavailable {
                root: PathBuf::from(ROOT),
                code,
                detail: detail.to_owned(),
            },
        ))
    };
    let said = [
        cause,
        format!("{text}\n  then retried"),
        format!("{text}."),
        format!("an earlier line\n{text}"),
        text.replacen(
            "nothing was sent to the authoring model",
            "nothing was sent",
            1,
        ),
        knowledge_refusal(EMBEDDED),
        other(RefusalCode::IdentityMismatch, "manifest bytes differ"),
        other(RefusalCode::Untrusted, ""),
        refusal_of(AuthoringContextError::Changed {
            pinned: "r3".to_owned(),
            found: "r4".to_owned(),
        }),
        refusal_of(AuthoringContextError::Decision {
            seat: "typesafe/jev".to_owned(),
            why: "no key".to_owned(),
        }),
        String::new(),
    ];
    for text in said {
        let block = Committed::new(Kind::Refusal, text.clone());
        assert_eq!(shown(&block), Shown::Said, "{text}");
    }
    // The same words outside a refusal are facts a reply quotes, as said.
    for kind in [
        Kind::Reply,
        Kind::Notice,
        Kind::Question,
        Kind::Report,
        Kind::Human,
    ] {
        assert_eq!(shown(&Committed::new(kind, text.clone())), Shown::Said);
    }
}

/// A provider failure may have sent its call: it keeps its words and its
/// uncertain scope, even when its reason quotes the knowledge refusal.
#[test]
fn a_provider_failure_keeps_its_words_and_its_uncertain_scope() {
    let text = provider_failure();
    assert!(
        text.starts_with("I couldn't use the authoring seat for this part — the authoring seat is unavailable: deepseek answered 503"),
        "{text}"
    );
    assert!(text.contains(
        "may have received this turn's context: a failed call can still have been sent."
    ));
    assert_eq!(shown(&Committed::new(Kind::Refusal, text)), Shown::Said);
    let quoting = recovery::card(
        "I couldn't use the authoring seat for this part",
        &AuthoringError::Seat(knowledge_refusal(ROOT)).to_string(),
        None,
        &[],
        "a failed call can still have been sent.",
    );
    assert_eq!(shown(&Committed::new(Kind::Refusal, quoting)), Shown::Said);
}

/// The words a refused turn adds, and the root of the build's own release, are
/// the producers' own: a change there fails here.
#[test]
fn the_turn_words_are_the_session_sources_own() {
    const SESSION: &str = include_str!("../../../nika-session/src/runtime/authoring.rs");
    const BUNDLED: &str = include_str!("../../../nika-onboard/src/knowledge/bundled.rs");
    assert!(
        SESSION.contains(&format!("\"{{error}}{NOT_SENT}\"")),
        "the Session's refused turn changed its words; the presenter must follow them"
    );
    assert!(BUNDLED.contains(&format!("LABEL: &str = \"{EMBEDDED}\";")));
}

/// The opening banner says the same refusal in one short line; every other
/// line, and the block, stay as the Session wrote them.
#[test]
fn the_opening_banner_warns_in_one_short_line() {
    let said = opening_banner(ROOT);
    let warning = (said.lines())
        .find(|line| line.starts_with("  ⚠ authoring knowledge: "))
        .expect("the Session warns at open")
        .to_owned();
    assert!(
        warning.contains("`/srv/foundry/release-r3` (ADMISSION_UNTRUSTED: "),
        "{said}"
    );
    let block = Committed::new(Kind::Banner, said.clone());
    let expected = said.replacen(&warning, WARNING_LINE, 1);
    assert_eq!(shown(&block), Shown::Banner(expected.clone()));
    assert_eq!(expected.lines().count(), said.lines().count());
    assert!(!expected.contains(ROOT));
    assert_eq!(block.text, said);
    let honoured = banner_under(AuthoringContext::default());
    assert!(!honoured.contains("authoring knowledge"), "{honoured}");
    assert_eq!(shown(&Committed::new(Kind::Banner, honoured)), Shown::Said);
    let embedded = opening_banner(EMBEDDED);
    assert_eq!(shown(&Committed::new(Kind::Banner, embedded)), Shown::Said);
}
