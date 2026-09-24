// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The value an answer carries, read against the value question it answers.
//!
//! The compiler binds literals only: an answer is one JSON literal under one key,
//! never words it reads. A value said in a sentence — « Écris dans sortie.txt. » to
//! « Destination file path » — is therefore read once before it binds, or the whole
//! sentence becomes the value (the DIALOG-01 refusal of 2026-09-24: the sentence
//! became a write path). The reading is typed and bounded:
//!
//! - a JSON literal or a single token is the value as typed: no call;
//! - several words go to the chosen intelligence once, through the Session's metered
//!   label seat; it can only point at the value by COPYING it, and the copy binds only
//!   when it is verbatim and made of whole tokens of the human's own line — never a piece
//!   cut out of a token (« txt » of « sortie.txt », « user » of « user@example.org »).
//!   That proves every character came from the human and where the copy starts and ends;
//!   it does not prove the copy is the value the human meant. Which whole tokens are the
//!   value is the model's reading, shown beside the outcome and reviewed before consent;
//! - a reply that points nowhere, at several values, at words the human did not type,
//!   or that cannot be read (a failed call, a spending limit) binds nothing: the
//!   question keeps waiting and says why;
//! - without a chosen intelligence the line is the value, as the question says.
//!
//! A reading is never a consent: the candidate it completes is reviewed, and nothing
//! is written before the human's yes.

use nika_onboard::compile::{CompileQuestion, QuestionType};

use super::{SessionRuntime, TurnOutcome};
use crate::authoring::AuthoringRound;
use crate::outcome::{Refusal, RefusalClass};

/// How the next line reaches the value of the question it answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AnswerReading {
    /// The line itself is the value.
    AsTyped,
    /// A verbatim part of the line is the value; the rest was the human's framing.
    Part(String),
    /// Nothing is bound: the question keeps waiting, for this reason.
    Waits(String),
}

/// What a value question says when no intelligence reads its reply.
pub(super) const AS_TYPED_NOTICE: &str =
    "no intelligence reads this reply: it is taken exactly as you type it — say the value alone";

/// A question whose answer is a business value the compiler bakes as a literal. The
/// seat's `model` (provider selection keeps its own door), the replacement request
/// (`intent.clarification`: its answer IS words), a clause's disposition (`gap.N`,
/// answered in words), a rule asked in words and a choice among offered keys are not.
pub(super) fn is_value_question(question: &CompileQuestion) -> bool {
    matches!(
        question.answer_type,
        QuestionType::Text | QuestionType::Literal
    ) && question.key != "model"
        && question.key != "intent.clarification"
        && !question.key.starts_with("gap.")
        && !super::authoring::asks_for_syntax(question)
}

/// The line is its own value without any reading: a JSON literal (`42` · `true` ·
/// `"a quoted value"` · `["a", "b"]`) or one token, nothing around it to leave out.
fn as_typed(line: &str) -> bool {
    let line = line.trim();
    !line.contains(char::is_whitespace) || serde_json::from_str::<serde_json::Value>(line).is_ok()
}

/// The one bounded reading: the question in its own words, the reply verbatim, and one
/// instruction — copy the value, or say NONE.
fn reading_prompt(question: &CompileQuestion, line: &str) -> String {
    let why = if question.why.is_empty() {
        String::new()
    } else {
        format!(" ({})", question.why)
    };
    format!(
        "Nika, an automation tool, asked a human for one value: «{label}»{why}.\nThe human replied: «{reply}».\nCopy that value exactly as it appears in the reply, character for character: only the value, without the words or the sentence punctuation around it; if the whole reply is the value, copy the whole reply. Never correct, translate, complete or add anything. If the reply gives no value, or more than one possible value, answer NONE.\nValue:",
        label = question.label,
        reply = line.trim(),
    )
}

/// The value a reply points at: one line, its own `Value:` cue and one pair of quotes
/// around it removed (both are the model's, never part of the value), kept only when it
/// is made of whole tokens of the human's line (`whole_part`).
fn verbatim_part(reply: &str, line: &str) -> Option<String> {
    let mut lines = reply.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    let value = unquoted(first.strip_prefix("Value:").map_or(first, str::trim));
    (!value.is_empty() && value != "NONE" && whole_part(line.trim(), value))
        .then(|| value.to_owned())
}

/// One pair of quotes around a copy, removed.
fn unquoted(text: &str) -> &str {
    for (open, close) in [
        ('"', '"'),
        ('\'', '\''),
        ('`', '`'),
        ('«', '»'),
        ('\u{201c}', '\u{201d}'),
    ] {
        if let Some(inner) = text.strip_prefix(open).and_then(|t| t.strip_suffix(close)) {
            return inner.trim();
        }
    }
    text
}

/// Quotes and brackets that may open a value in a line, those that may close it, and the
/// clause punctuation that may follow it: structure only, never a word.
const OPENING: &[char] = &[
    '"', '\'', '`', '«', '\u{201c}', '\u{2018}', '(', '[', '{', '<',
];
const CLOSING: &[char] = &[
    '"', '\'', '`', '»', '\u{201d}', '\u{2019}', ')', ']', '}', '>',
];
const CLAUSE_END: &[char] = &['.', ',', ';', ':', '!', '?', '\u{2026}'];

/// Whether `part` occurs in `line` as whole whitespace-separated tokens: before it only the
/// line's start or whitespace, past opening quotes or brackets; after it only the line's end
/// or whitespace, past closing quotes, brackets and clause punctuation. So « sortie.txt »
/// ends before the sentence's period, a quoted value binds without its quotes and
/// « dir/rapport final.txt » binds whole, while a piece cut out of a token never does:
/// « txt » of « sortie.txt », « rapport.txt » of « exports/rapport.txt », « file.txt » of
/// « out-file.txt », « b » of « `a_b` », « user » of « user@example.org ». A boundary rule,
/// not a proof of meaning: when several whole-token spans could be the value (« dir/rapport »
/// and « dir/rapport final.txt »), the model's reading chooses and the human reviews it.
fn whole_part(line: &str, part: &str) -> bool {
    let separated = |c: Option<char>| c.is_none_or(char::is_whitespace);
    line.match_indices(part).any(|(at, _)| {
        let before = line[..at].chars().rev().find(|c| !OPENING.contains(c));
        let after = line[at + part.len()..]
            .chars()
            .find(|c| !CLOSING.contains(c) && !CLAUSE_END.contains(c));
        separated(before) && separated(after)
    })
}

/// The bound value said beside the outcome it led to, with the words it was read from.
pub(super) fn disclosed(outcome: TurnOutcome, value: &str, line: &str) -> TurnOutcome {
    let note = format!(
        "read your answer as « {value} » (from « {} » — say the value alone if that is not it)",
        line.trim()
    );
    match outcome {
        TurnOutcome::Proposal { id, preview } => TurnOutcome::Proposal {
            id,
            preview: format!("{note}\n{preview}"),
        },
        TurnOutcome::Question { key, question } => TurnOutcome::Question {
            key,
            question: format!("{note}\n{question}"),
        },
        TurnOutcome::Facts(text) => TurnOutcome::Facts(format!("{note}\n{text}")),
        other => other,
    }
}

impl SessionRuntime {
    /// Whether a chosen intelligence reads replies said in words — the same
    /// availability as the Session's other bounded readings.
    pub(super) fn reads_answers(&self) -> bool {
        self.intelligence.ready && self.chosen && self.reasoner.name() != "none"
    }

    /// The notice a value question carries when its reply is taken as typed.
    pub(super) fn as_typed_notice(&self, question: &CompileQuestion) -> Option<&'static str> {
        (is_value_question(question) && !self.reads_answers()).then_some(AS_TYPED_NOTICE)
    }

    /// The typed reading of `line` as the answer to `question`: as typed, a verbatim
    /// part, or nothing bound. One metered call at most, and only for several words at
    /// a value question under a chosen intelligence.
    fn read_answer(&mut self, question: &CompileQuestion, line: &str) -> AnswerReading {
        if !is_value_question(question) || as_typed(line) || !self.reads_answers() {
            return AnswerReading::AsTyped;
        }
        self.activity(&crate::activity::Activity::now(
            crate::activity::Phase::Understanding,
            "reading your answer",
        ));
        // The Session's metered label seat: a spending limit refuses before any call.
        match self.reason_with_money(&reading_prompt(question, line), true) {
            Ok(reply) => match verbatim_part(&reply.text, line) {
                Some(value) if value == line.trim() => AnswerReading::AsTyped,
                Some(value) => AnswerReading::Part(value),
                None => AnswerReading::Waits(format!(
                    "I did not find one value for « {} » in your reply as you typed it — nothing was bound",
                    question.label
                )),
            },
            Err(error) => AnswerReading::Waits(format!(
                "I could not read your answer ({error}) — nothing was bound"
            )),
        }
    }

    /// Bind the open question to the typed reading of the human's line and read the
    /// round again: the line as typed, a verbatim part of it (said beside the outcome),
    /// or nothing — then the question waits and says why.
    pub(super) fn bind_answer(&mut self, mut round: AuthoringRound, line: &str) -> TurnOutcome {
        let reading = match round.current() {
            Some(question) => self.read_answer(question, line),
            None => AnswerReading::AsTyped,
        };
        let value = match reading {
            AnswerReading::AsTyped => line.to_owned(),
            AnswerReading::Part(value) => value,
            AnswerReading::Waits(why) => return self.answer_waits(round, &why),
        };
        let Some(key) = round.answer_current(&value) else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "no authoring question waits",
            ));
        };
        if value == line {
            self.remember(line, &format!("(answered {key})"));
            return self.compile_again(round);
        }
        self.remember(line, &format!("(answered {key} · read as « {value} »)"));
        let outcome = self.compile_again(round);
        disclosed(outcome, &value, line)
    }

    /// The question keeps waiting, said with the reason nothing was bound.
    fn answer_waits(&mut self, round: AuthoringRound, why: &str) -> TurnOutcome {
        let (key, text) = round.current().map_or_else(
            || (String::new(), why.to_owned()),
            |q| {
                (
                    q.key.clone(),
                    format!(
                        "{why} · say the value alone on the next line\n{}",
                        super::authoring::question_text(q, &round.reasons)
                    ),
                )
            },
        );
        self.authoring = Some(round);
        TurnOutcome::Question {
            key,
            question: text,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_or_one_token_is_its_own_value() {
        for line in [
            "sortie.txt",
            " ./out/x.md ",
            "42",
            "true",
            "\"a b\"",
            "[1, 2]",
            "{\"k\": 1}",
        ] {
            assert!(as_typed(line), "{line}");
        }
        for line in [
            "Écris dans sortie.txt.",
            "the code EUR",
            "my notes/out file.txt",
        ] {
            assert!(!as_typed(line), "{line}");
        }
    }

    #[test]
    fn a_copy_binds_only_as_a_whole_verbatim_part_of_the_line() {
        let line = "Écris dans sortie.txt.";
        assert_eq!(
            verbatim_part("sortie.txt", line).as_deref(),
            Some("sortie.txt")
        );
        assert_eq!(
            verbatim_part("  «sortie.txt»  ", line).as_deref(),
            Some("sortie.txt")
        );
        assert_eq!(
            verbatim_part("Value: sortie.txt", line).as_deref(),
            Some("sortie.txt")
        );
        assert_eq!(
            verbatim_part("`sortie.txt`", line).as_deref(),
            Some("sortie.txt")
        );
        assert_eq!(verbatim_part("NONE", line), None, "no value");
        assert_eq!(verbatim_part("./out/sortie.txt", line), None, "invented");
        assert_eq!(verbatim_part("tie.txt", line), None, "a fragment of a word");
        assert_eq!(
            verbatim_part("sortie.txt\nresultat.txt", line),
            None,
            "two values"
        );
        assert_eq!(verbatim_part("", line), None);
        let quoted = "Mets \"exports/rapport final.txt\" comme destination";
        assert_eq!(
            verbatim_part("\"exports/rapport final.txt\"", quoted).as_deref(),
            Some("exports/rapport final.txt")
        );
    }

    /// A copy cut out of a token — after a dot, a slash, a hyphen, an underscore, an
    /// apostrophe, a sign or a comma, or before an at-sign or a dot that continues the
    /// token — is never the value, whatever the model returned.
    #[test]
    fn a_piece_cut_out_of_a_token_is_never_the_value() {
        for (copy, line) in [
            ("txt", "Écris dans sortie.txt."),
            ("rapport.txt", "Mets-le dans exports/rapport.txt"),
            ("file.txt", "Écris dans out-file.txt"),
            ("b", "La clé est a_b."),
            ("user", "Écris à user@example.org"),
            ("sortie", "Écris dans sortie.txt"),
            ("sortie.txt", "Écris dans sortie.txt.bak"),
            ("2026", "Label it customers-2026, please"),
            ("customers", "Label it customers-2026, please"),
            ("5", "un seuil de 1,5."),
            ("1", "un seuil de 1,5."),
            ("3", "Mets -3."),
            ("archive.txt", "Écris dans l'archive.txt"),
            ("rapport final.txt", "Mets-le dans dir/rapport final.txt."),
        ] {
            assert_eq!(verbatim_part(copy, line), None, "{copy} out of « {line} »");
        }
    }

    /// The whole value binds: before the sentence's period or comma, between quotes or
    /// brackets, with a space inside, as a number, an address or an opaque identifier.
    #[test]
    fn a_whole_token_value_binds_before_punctuation_and_inside_quotes() {
        for (copy, line, value) in [
            ("sortie.txt", "Écris dans sortie.txt.", "sortie.txt"),
            ("sortie.txt", "Écris dans sortie.txt, merci", "sortie.txt"),
            ("sortie.txt", "Écris dans 'sortie.txt'.", "sortie.txt"),
            ("sortie.txt", "(sortie.txt)", "sortie.txt"),
            (
                "\"exports/rapport final.txt\"",
                "Mets \"exports/rapport final.txt\" ici",
                "exports/rapport final.txt",
            ),
            (
                "exports/rapport final.txt",
                "Mets la copie dans « exports/rapport final.txt ».",
                "exports/rapport final.txt",
            ),
            (
                "dir/rapport final.txt",
                "Mets-le dans dir/rapport final.txt.",
                "dir/rapport final.txt",
            ),
            ("42", "Garde 42 lignes.", "42"),
            ("1,5", "un seuil de 1,5.", "1,5"),
            ("-3", "Mets -3.", "-3"),
            (
                "user@example.org",
                "Écris à user@example.org.",
                "user@example.org",
            ),
            (
                "customers-2026",
                "Label it customers-2026, please",
                "customers-2026",
            ),
        ] {
            assert_eq!(
                verbatim_part(copy, line).as_deref(),
                Some(value),
                "{copy} in « {line} »"
            );
        }
    }

    /// Whole tokens bound the copy; they do not choose the value. Two whole-token spans of
    /// one line both pass: which is the value is the model's reading, then the human's.
    #[test]
    fn whole_tokens_bound_the_copy_but_do_not_choose_the_value() {
        let line = "Mets-le dans dir/rapport final.txt.";
        assert_eq!(
            verbatim_part("dir/rapport", line).as_deref(),
            Some("dir/rapport")
        );
        assert_eq!(
            verbatim_part("dir/rapport final.txt", line).as_deref(),
            Some("dir/rapport final.txt")
        );
    }

    #[test]
    fn the_prompt_carries_the_question_and_the_reply_and_asks_for_a_copy() {
        let out = nika_onboard::compile::compile(&nika_onboard::compile::CompileRequest::create(
            "aggregate-by-key",
        ))
        .expect("compiles");
        let question = out.questions.first().expect("one hole");
        let prompt = reading_prompt(question, "  Use euros, the code EUR.  ");
        assert!(
            prompt.contains(&format!("«{}»", question.label)),
            "{prompt}"
        );
        assert!(prompt.contains("«Use euros, the code EUR.»"), "{prompt}");
        assert!(
            prompt.contains("answer NONE") && prompt.ends_with("Value:"),
            "{prompt}"
        );
        assert!(
            !prompt.contains("JSON"),
            "a natural request, never a format: {prompt}"
        );
    }
}
