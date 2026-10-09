// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authoring knowledge as the conversation meets it: the typed state every host reads
//! ([`crate::work::Knowledge`]), the line held when the configuration names a source the strict
//! door refused, and the one supported action, `/knowledge embedded`. Under a refused source a
//! line that would reach a model waits before any model request, exactly as typed
//! ([`crate::work::Waiting::KnowledgeChoice`]); a line that needs no model is read as before.
//! The action is explicit: it names the release this build embeds for this conversation alone
//! (its history keeps it, so a reopening keeps it too), never the operator's default, changes no
//! intelligence, effort or draft, and resumes the held line once. Nothing falls back silently:
//! without it the refusal stands, said, and `/details` keeps the whole diagnostic.

use nika_cli_host::compile::config::{KnowledgeChoice, KnowledgeLayer, KnowledgeSource};
use nika_cli_host::compile::knowledge::KnowledgeError;
use nika_onboard::knowledge::pin::{KnowledgeOrigin, short};

use super::decision::local_command_of;
use super::history::Operation;
use super::{SessionRuntime, TurnOutcome};
use crate::authoring::AuthoringContextError;
use crate::outcome::{Refusal, RefusalClass};
use crate::work::Knowledge;

/// The word a conversation's history keeps for its explicit choice of the embedded release.
pub(super) const EMBEDDED: &str = "embedded";

/// The action, as every word of this module offers it.
const ACTION: &str = "`/knowledge embedded` uses the knowledge built into Nika";

/// What the held line waits for, beside anything said at its prompt.
const HELD: &str = "your message waits · `/knowledge embedded` resumes it · `cancel` drops it";

/// The banner's line under a refused source, before its warning: what failed and the way on,
/// nothing of the diagnostic.
const BANNER: &str = "  knowledge override not admitted · `/knowledge embedded` uses the knowledge built into Nika for this conversation · a message that needs a model waits until then";

/// This conversation's knowledge choice and the line it holds, as the runtime keeps them.
#[derive(Default)]
pub(super) struct State {
    /// The line held under a refused knowledge source, exactly as typed: resumed once by
    /// `/knowledge embedded`, dropped on `cancel`.
    pub(super) held: Option<String>,
    /// This conversation's own explicit knowledge choice (`embedded`), kept by its history.
    pub(super) conversation: Option<String>,
}

/// Whether `line` is the knowledge command with words after it (`/knowledge embedded`).
pub(super) fn is_choice(line: &str) -> bool {
    line.trim().starts_with("/knowledge ")
}

impl SessionRuntime {
    /// The authoring knowledge as every host reads it: the release admitted, the source refused,
    /// or why none is read. Reading it reads no file and decides nothing.
    pub(super) fn knowledge_work(&self) -> Knowledge {
        if let Some(refused) = self.knowledge_refused() {
            return refused;
        }
        let context = &self.authoring_context;
        let Some(pin) = context.knowledge() else {
            let why = match context.refusal() {
                Some(_) => "the authoring configuration was refused before its knowledge resolved · `/status` says why".to_owned(),
                None => context.knowledge_choice().words(),
            };
            return Knowledge::Unread { why };
        };
        let by = match context.knowledge_choice() {
            _ if self.knowledge.conversation.is_some()
                && pin.origin == KnowledgeOrigin::Embedded =>
            {
                "conversation"
            }
            KnowledgeChoice::Named { by, .. } => layer(*by),
            _ => "default",
        };
        Knowledge::Admitted {
            source: pin.origin.word(),
            version: pin.version.clone(),
            manifest_sha256: pin.manifest_sha256.clone(),
            by,
        }
    }

    /// The knowledge source the configuration names and the strict door refused, as the typed
    /// state says it (its cause without the host's path); `None` when the knowledge is admitted,
    /// off or unread, or the configuration was refused for another reason.
    fn knowledge_refused(&self) -> Option<Knowledge> {
        let context = &self.authoring_context;
        let (code, cause) = match context.refusal()? {
            AuthoringContextError::Knowledge(KnowledgeError::Unavailable {
                root,
                code,
                detail,
            }) => {
                let root = root.display().to_string();
                (
                    code.as_str().to_owned(),
                    detail.replace(&root, "<release root>"),
                )
            }
            AuthoringContextError::PackForOneRequest { .. } => (
                "PACK_NOT_ADMITTED".to_owned(),
                "a pack composed for one request is bound to no admitted release".to_owned(),
            ),
            _ => return None,
        };
        let (source, by) = match context.knowledge_named()? {
            KnowledgeChoice::Named {
                source: KnowledgeSource::Snapshot { .. },
                by,
            } => ("snapshot", layer(by)),
            KnowledgeChoice::Named {
                source: KnowledgeSource::Pack { .. },
                by,
            } => ("pack", layer(by)),
            KnowledgeChoice::Named { .. } if self.knowledge.conversation.is_some() => {
                ("embedded", "conversation")
            }
            KnowledgeChoice::Named { by, .. } => ("embedded", layer(by)),
            _ => ("embedded", "default"),
        };
        Some(Knowledge::Refused {
            source,
            by,
            code,
            cause,
        })
    }

    /// Whether a model may read the next open line: the door's classifier or the chosen
    /// intelligence, cognition not withheld by the money law.
    pub(super) fn routes_by_model(&self) -> bool {
        !self.money_blocks_cognition() && (self.classifier.is_some() || self.reads_answers())
    }

    /// Whether a model may read the answer to the open question: its route, its reading, or the
    /// seat that compiles the round again, cognition not withheld by the money law.
    pub(super) fn answers_by_model(&self) -> bool {
        self.routes_by_model() || (!self.money_blocks_cognition() && self.seat.has_model())
    }

    /// `line` held before any model request, when `model` says one would read it next and the
    /// configuration names a knowledge source the strict door refused that the embedded release
    /// can replace: kept exactly as typed, it waits for `/knowledge embedded`. `None` otherwise:
    /// the line goes on as before.
    pub(super) fn hold_for_knowledge(&mut self, line: &str, model: bool) -> Option<TurnOutcome> {
        let refused = (self.knowledge_refused()).filter(|refused| model && replaceable(refused))?;
        self.knowledge.held = Some(line.to_owned());
        Some(TurnOutcome::Ask(format!(
            "{} · this message reached no model and waits\n  {ACTION} and resumes it · `cancel` drops it · `/details` says why",
            refused_line(&refused)
        )))
    }

    /// The status beside the prompt while a line is held.
    pub(super) fn knowledge_status(&self) -> Option<String> {
        self.knowledge.held.as_ref().map(|_| {
            "Needs the knowledge choice · your message waits · `/knowledge embedded` resumes it"
                .to_owned()
        })
    }

    /// Whether the refused source is one the action replaces: the banner, the help card and the
    /// completions offer it then, and only then.
    pub(super) fn knowledge_offered(&self) -> bool {
        self.knowledge_refused()
            .is_some_and(|refused| replaceable(&refused))
    }

    /// The banner's action line under a refused source the embedded release replaces, else
    /// nothing: said before the warning, which stays the banner's last line as hosts read it.
    pub(super) fn knowledge_banner(&self) -> String {
        if self.knowledge_offered() {
            format!("\n{BANNER}")
        } else {
            String::new()
        }
    }

    /// `/knowledge`: the knowledge in use, or why none is read, and the one choice beside it;
    /// `/knowledge embedded`: that choice applied. Any other word changes nothing.
    pub(super) fn knowledge_command(&mut self, line: &str) -> TurnOutcome {
        match line.trim() {
            "/knowledge" => TurnOutcome::Facts(self.knowledge_card()),
            "/knowledge embedded" => self.use_embedded_knowledge(),
            other => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!(
                    "`{other}` is not a knowledge choice — {ACTION} · `/knowledge` shows the knowledge in use · nothing changed"
                ),
            )),
        }
    }

    /// The release this build embeds, named explicitly for this conversation: admitted and pinned
    /// now, recorded with the conversation, and the held line resumed once, exactly as typed. The
    /// release already in use is a no-op, said; a release that cannot be read here changes nothing.
    fn use_embedded_knowledge(&mut self) -> TurnOutcome {
        let embedded = |origin: &KnowledgeOrigin| *origin == KnowledgeOrigin::Embedded;
        if let Some(pin) = self.authoring_context.knowledge()
            && embedded(&pin.origin)
        {
            return TurnOutcome::Facts(format!(
                "knowledge: built into Nika, already in use · {} · nothing changed",
                identity(pin.version.as_deref(), &pin.manifest_sha256)
            ));
        }
        let context = self.authoring_context.with_embedded_knowledge();
        let Some(pin) = context
            .knowledge()
            .filter(|pin| embedded(&pin.origin))
            .cloned()
        else {
            let why = context
                .refusal()
                .map_or_else(String::new, |why| format!(": {why}"));
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!("the knowledge built into Nika cannot be used here{why} · nothing changed"),
            ));
        };
        self.set_authoring_context(context);
        self.knowledge.conversation = Some(EMBEDDED.to_owned());
        let notice = format!(
            "knowledge: built into Nika · {} · holds for this conversation",
            identity(pin.version.as_deref(), &pin.manifest_sha256)
        );
        match self.knowledge.held.take() {
            Some(line) => TurnOutcome::Resumed {
                notice,
                outcome: Box::new(self.turn_unrecorded(&line)),
            },
            None => TurnOutcome::Facts(notice),
        }
    }

    /// `/knowledge`, in a few words: the release in use and who chose it, or the refusal and its
    /// way on, or why none is read.
    pub(super) fn knowledge_card(&self) -> String {
        match self.knowledge_work() {
            Knowledge::Admitted {
                source,
                version,
                manifest_sha256,
                by,
            } => {
                let seen = identity(version.as_deref(), &manifest_sha256);
                match (source, by) {
                    ("embedded", "conversation") => {
                        format!("knowledge: built into Nika · {seen} · chosen in this conversation")
                    }
                    ("embedded", _) => format!("knowledge: built into Nika · {seen}"),
                    _ => format!(
                        "knowledge: {seen} · named by the {by}\n  {ACTION} instead, for this conversation"
                    ),
                }
            }
            refused @ Knowledge::Refused { .. } if self.knowledge_offered() => format!(
                "{} · nothing of it is read\n  {ACTION} for this conversation · `/details` says why",
                refused_line(&refused)
            ),
            refused @ Knowledge::Refused { .. } => {
                format!("{} · `/details` says why", refused_line(&refused))
            }
            Knowledge::Unread { why } => format!("knowledge: none read · {why}"),
            _ => "knowledge: not stated".to_owned(),
        }
    }

    /// The answer to the held line's prompt, through the same durable boundary as a choice.
    pub fn choose_knowledge(&mut self, answer: &str) -> TurnOutcome {
        self.last_answer = None;
        self.recorded(Operation::Choice, answer, |s| {
            s.knowledge_choice_unrecorded(answer)
        })
    }

    /// `/knowledge` and `/knowledge embedded` typed as a turn: recorded as a choice.
    pub(super) fn knowledge_turn(&mut self, line: &str) -> TurnOutcome {
        self.recorded(Operation::Choice, line, |s| s.knowledge_command(line))
    }

    /// The held line's prompt: the action resumes it, `cancel` drops it (sent nowhere), leaving
    /// stays one line away, the read-only commands answer beside it; anything else keeps it.
    fn knowledge_choice_unrecorded(&mut self, answer: &str) -> TurnOutcome {
        let answer = answer.trim();
        if super::is_quit(answer) {
            self.knowledge.held = None;
            return TurnOutcome::Quit;
        }
        if crate::authoring::is_cancel(answer) {
            self.knowledge.held = None;
            return TurnOutcome::Facts(
                "dropped · your message was not sent anywhere · the knowledge is unchanged"
                    .to_owned(),
            );
        }
        if answer == "/knowledge" || is_choice(answer) {
            return self.knowledge_command(answer);
        }
        if let Some(command) = local_command_of(answer) {
            return self.answer_locally(command);
        }
        if let Some(outcome) = self.beside(answer, HELD) {
            return outcome;
        }
        TurnOutcome::Refusal(Refusal::new(
            RefusalClass::WrongState,
            format!("this line was not sent · {HELD}"),
        ))
    }

    /// The conversation's kept knowledge choice, applied when its history reopens: the embedded
    /// release named for it again, silently; a word this engine does not read is kept unchanged.
    pub(super) fn restore_knowledge(&mut self, kept: Option<String>) {
        if kept.as_deref() == Some(EMBEDDED) {
            let context = self.authoring_context.with_embedded_knowledge();
            if context.knowledge().is_some() {
                self.set_authoring_context(context);
            }
        }
        self.knowledge.conversation = kept;
    }
}

/// Whether the embedded release can replace a refused source: not when it is the one refused.
fn replaceable(refused: &Knowledge) -> bool {
    !matches!(
        refused,
        Knowledge::Refused {
            source: "embedded",
            ..
        }
    )
}

/// The layer that named a source, in the typed state's word.
fn layer(by: KnowledgeLayer) -> &'static str {
    match by {
        KnowledgeLayer::Environment => "environment",
        _ => "host",
    }
}

/// A release in a few words: its version and its manifest digest, cut at twelve.
fn identity(version: Option<&str>, manifest_sha256: &str) -> String {
    format!(
        "{} · snapshot {}",
        version.unwrap_or("unversioned"),
        short(manifest_sha256)
    )
}

/// What was refused, in one clause: the setting that named it and the door's stable code.
fn refused_line(refused: &Knowledge) -> String {
    let Knowledge::Refused {
        source, by, code, ..
    } = refused
    else {
        return "knowledge not admitted".to_owned();
    };
    let named = match (*source, *by) {
        ("snapshot", "environment") => "NIKA_KNOWLEDGE",
        ("pack", "environment") => "NIKA_KNOWLEDGE_PACK",
        ("embedded", _) => "the release built into Nika",
        _ => "the host's knowledge",
    };
    format!("Knowledge override not admitted ({named} · {code})")
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod knowledge_tests;
