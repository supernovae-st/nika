// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A host that opens the intelligence guide before the first message can still keep
//! that message. Explicit choice syntax remains the choice parser's, including typos.

use super::history::Operation;
use super::{IntelligenceKind, Refusal, RefusalClass, SessionRuntime, TurnOutcome};

impl SessionRuntime {
    /// Keep a message entered at an initial intelligence guide and resume it once after
    /// a usable choice. Slash commands, numeric choice syntax and the API category keep
    /// the ordinary choice door; a malformed explicit choice never becomes a message.
    /// Further messages are appended in order, preserving each message's exact bytes.
    /// Refuses when a usable intelligence is already selected or this guide is not shown.
    pub fn request_before_intelligence(&mut self, line: &str) -> TurnOutcome {
        self.last_answer = None;
        let ready = self.chosen
            && self.intelligence.ready
            && !matches!(self.intelligence.kind, IntelligenceKind::None);
        if !matches!(self.waiting(), crate::work::Waiting::IntelligenceChoice) || ready {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "the initial intelligence guide is no longer waiting; nothing was sent",
            ));
        }
        let first = line.split_whitespace().next().unwrap_or("");
        if line.trim_start().starts_with('/')
            || first.starts_with(|c: char| c.is_ascii_digit())
            || first.eq_ignore_ascii_case("api")
            || crate::authoring::is_cancel(line)
        {
            return self.choose(line);
        }
        self.recorded(Operation::Turn, line, |session| {
            if line.trim().is_empty() {
                return TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::WrongState,
                    "write a message or choose an intelligence; nothing was sent",
                ));
            }
            match &mut session.interrupted {
                Some(kept) => {
                    kept.push_str("\n\n");
                    kept.push_str(line);
                }
                None => session.interrupted = Some(line.to_owned()),
            }
            TurnOutcome::Facts(
                "Your message is kept and will resume once after you choose a usable intelligence."
                    .to_owned(),
            )
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::intelligence::IntelligenceCensus;
    use crate::reasoner::{NoReasoner, ScriptedReasoner};

    #[test]
    fn an_initial_message_survives_bad_choices_and_resumes_once() {
        let room = tempfile::tempdir().expect("room");
        let census = IntelligenceCensus::new(vec![], vec!["deepseek".into()], vec![]);
        let mut session = SessionRuntime::open_unchosen(
            room.path(),
            census,
            None,
            Box::new(|resolved| match &resolved.kind {
                IntelligenceKind::None => Box::new(NoReasoner),
                _ => Box::new(ScriptedReasoner::new(vec!["hello back".into()])),
            }),
        );
        session.enable_continuous_preparation();
        assert!(matches!(session.turn("/intelligence"), TurnOutcome::Ask(_)));
        let message = "  hello there, how are you today?\n  ";
        assert!(matches!(
            session.request_before_intelligence(message),
            TurnOutcome::Facts(_)
        ));
        assert_eq!(session.interrupted.as_deref(), Some(message));
        for invalid in ["2 unknown-provider", "9", "2bad", "API unknown"] {
            assert!(matches!(
                session.request_before_intelligence(invalid),
                TurnOutcome::Refusal(_)
            ));
            assert_eq!(session.interrupted.as_deref(), Some(message));
        }
        assert!(matches!(
            session.request_before_intelligence("/intelligence"),
            TurnOutcome::Ask(_)
        ));
        assert_eq!(session.interrupted.as_deref(), Some(message));
        let TurnOutcome::Resumed { outcome, .. } =
            session.request_before_intelligence("2 deepseek")
        else {
            panic!("the choice resumes the message")
        };
        assert!(
            matches!(*outcome, TurnOutcome::Reply(ref text) if text == "hello back"),
            "the kept chat request resumes: {outcome:?}"
        );
        assert_eq!(session.interrupted, None);
        assert!(!session.pending_choice());
        assert!(matches!(
            session.request_before_intelligence(message),
            TurnOutcome::Refusal(_)
        ));
        assert_eq!(session.interrupted, None);
    }
}
