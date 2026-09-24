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
//! A choice among offered keys (a column the request leaves open among the observed
//! ones: « La colonne montant. » to `montant · autre`, DIALOG-03 of 2026-09-24) reads the
//! same way against its OWN offers. An offered key typed alone — or written as its JSON
//! string, the shape the compiler asks — is the answer: no call. Otherwise, under a chosen
//! intelligence, the same one bounded reading is shown the exact keys offered now and
//! binds only a copy that IS one of them, verbatim and made of whole tokens of the human's
//! line; a line that carries no offered key is not read at all. A key the line does not
//! carry, a word that is not offered, a piece of a word, a second answer, an empty or
//! failed reply and a spending limit bind nothing, and the choice keeps waiting. Which of
//! the offered keys the line chooses — one of several, one it rejects — is the reading's,
//! never the first match's, and it is said beside the outcome. Without an intelligence the
//! line is the answer as typed. Either way the compiler admits the answer or asks again.
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
/// answered in words), a rule asked in words and a choice among offered keys are not
/// (a choice reads against its own offers: `is_offered_choice`).
pub(super) fn is_value_question(question: &CompileQuestion) -> bool {
    matches!(
        question.answer_type,
        QuestionType::Text | QuestionType::Literal
    ) && keeps_the_reading(question)
}

/// A choice among the keys the question offers: its answer is one of them, verbatim. The
/// doors that are not a value's keep theirs whatever their shape.
pub(super) fn is_offered_choice(question: &CompileQuestion) -> bool {
    matches!(question.answer_type, QuestionType::Choice)
        && !question.options.is_empty()
        && keeps_the_reading(question)
}

/// Not one of the doors that keep their own reading: the seat's `model`, the replacement
/// request, a clause's disposition, a rule asked in words.
fn keeps_the_reading(question: &CompileQuestion) -> bool {
    question.key != "model"
        && question.key != "intent.clarification"
        && !question.key.starts_with("gap.")
        && !super::authoring::asks_for_syntax(question)
}

/// Whether `text` is one of the keys the question offers, exactly.
fn is_offered(question: &CompileQuestion, text: &str) -> bool {
    question.options.iter().any(|offer| offer.key == text)
}

/// The line is an offered key alone — as written, or as its JSON string (the shape the
/// compiler asks) — spacing aside: its own answer, no reading.
fn names_an_offer_alone(question: &CompileQuestion, line: &str) -> bool {
    let line = line.trim();
    if let Ok(serde_json::Value::String(text)) = serde_json::from_str::<serde_json::Value>(line) {
        return is_offered(question, &text);
    }
    is_offered(question, line)
}

/// Whether the line carries at least one offered key as whole tokens: the only lines a
/// reading could bind from. A necessary condition, never a choice — which offered key the
/// line chooses (one of several, not the one it rejects) is the reading's, then the human's.
fn carries_an_offer(question: &CompileQuestion, line: &str) -> bool {
    question
        .options
        .iter()
        .any(|offer| !offer.key.is_empty() && whole_part(line.trim(), &offer.key))
}

/// The keys offered now, in the compiler's order.
fn offered_keys(question: &CompileQuestion) -> String {
    question
        .options
        .iter()
        .map(|offer| offer.key.as_str())
        .collect::<Vec<_>>()
        .join(" · ")
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

/// The one bounded reading of a choice: the question in its own words, the exact keys it
/// offers now, the reply verbatim, and one instruction — copy the offered answer the reply
/// chooses, or say NONE.
fn choice_prompt(question: &CompileQuestion, line: &str) -> String {
    let offers = question
        .options
        .iter()
        .map(|offer| format!("«{}»", offer.key))
        .collect::<Vec<_>>()
        .join(" · ");
    format!(
        "Nika, an automation tool, asked a human to choose one answer: «{label}».\nThe offered answers are exactly: {offers}.\nThe human replied: «{reply}».\nCopy the one offered answer the reply chooses, exactly as it appears in the reply, character for character: only that answer, without the words or the sentence punctuation around it. Never correct, translate, complete or add anything. If the reply chooses none of the offered answers, more than one, or only rejects one, answer NONE.\nValue:",
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

/// The offered key a reply points at: the verbatim whole-token copy (`verbatim_part`), kept
/// only when it IS one of the keys the question offers now — a word the human typed that
/// is not offered, several keys, a key with anything around it bind nothing.
fn offered_part(reply: &str, line: &str, question: &CompileQuestion) -> Option<String> {
    verbatim_part(reply, line).filter(|value| is_offered(question, value))
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
    /// a value question — or a line that is not an offered key alone but carries one at a
    /// choice — under a chosen intelligence.
    fn read_answer(&mut self, question: &CompileQuestion, line: &str) -> AnswerReading {
        let choice = is_offered_choice(question);
        let unread = if choice {
            names_an_offer_alone(question, line)
        } else {
            !is_value_question(question) || as_typed(line)
        };
        if unread || !self.reads_answers() {
            return AnswerReading::AsTyped;
        }
        // No offered key in the line: no copy of it could bind, so nothing is asked of anyone.
        if choice && !carries_an_offer(question, line) {
            return AnswerReading::Waits(format!(
                "none of the offered answers ({}) is in your reply as you typed it — nothing was bound",
                offered_keys(question)
            ));
        }
        self.activity(&crate::activity::Activity::now(
            crate::activity::Phase::Understanding,
            "reading your answer",
        ));
        let prompt = if choice {
            choice_prompt(question, line)
        } else {
            reading_prompt(question, line)
        };
        // The Session's metered label seat: a spending limit refuses before any call.
        match self.reason_with_money(&prompt, true) {
            Ok(reply) => {
                let part = if choice {
                    offered_part(&reply.text, line, question)
                } else {
                    verbatim_part(&reply.text, line)
                };
                match part {
                    Some(value) if value == line.trim() => AnswerReading::AsTyped,
                    Some(value) => AnswerReading::Part(value),
                    None if choice => AnswerReading::Waits(format!(
                        "I did not find one of the offered answers ({}) in your reply as you typed it — nothing was bound",
                        offered_keys(question)
                    )),
                    None => AnswerReading::Waits(format!(
                        "I did not find one value for « {} » in your reply as you typed it — nothing was bound",
                        question.label
                    )),
                }
            }
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
        let asked = self.question_id_of(&round);
        let Some(key) = round.answer_current(&value) else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "no authoring question waits",
            ));
        };
        self.questions.close(asked);
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

    /// A choice as the compiler asks it: an observed-column record (DIALOG-03's shape)
    /// replayed with zero calls.
    fn column_choice() -> CompileQuestion {
        let intent = "Additionne une colonne de ventes.csv dans total.txt.";
        let record = serde_json::json!({
            "strategy": "native",
            "intent_sha256": nika_onboard::compile::intent_sha256(intent),
            "source": "",
            "questions": [{
                "key": "const.sum_column",
                "label": "Quelle colonne additionner ?",
                "answer_type": "choice",
                "why": "La demande ne dit pas quelle colonne.",
                "options": [
                    {"key": "montant", "label": "column `montant` of ventes.csv"},
                    {"key": "autre", "label": "column `autre` of ventes.csv"},
                ],
            }],
            "gaps": [],
            "trigger": null,
        });
        let out = nika_onboard::compile::compile(
            &nika_onboard::compile::CompileRequest::create(intent).with_plan(record),
        )
        .expect("replays");
        out.questions
            .into_iter()
            .next()
            .expect("the column is asked")
    }

    /// An offered key alone is its own answer; a line is read only when it carries one.
    #[test]
    fn an_offered_key_alone_is_its_answer_and_only_a_line_carrying_one_is_read() {
        let choice = column_choice();
        assert_eq!(choice.answer_type, QuestionType::Choice);
        assert!(is_offered_choice(&choice) && !is_value_question(&choice));
        assert_eq!(offered_keys(&choice), "montant · autre");
        for line in ["montant", "  autre ", "\"montant\""] {
            assert!(names_an_offer_alone(&choice, line), "{line}");
        }
        for line in [
            "Montant",
            "montant.",
            "total",
            "\"mont\"",
            "La colonne montant.",
        ] {
            assert!(!names_an_offer_alone(&choice, line), "{line}");
        }
        for line in [
            "La colonne montant.",
            "Use « autre », please",
            "Pas montant, autre.",
        ] {
            assert!(carries_an_offer(&choice, line), "{line}");
        }
        for line in [
            "La colonne des montants.",
            "La colonne mont.",
            "La colonne l'autre.",
            "total",
        ] {
            assert!(!carries_an_offer(&choice, line), "{line}");
        }
    }

    /// A copy binds only when it IS an offered key, verbatim and whole in the human's line.
    #[test]
    fn a_copy_binds_only_as_an_offered_key_the_line_carries_whole() {
        let choice = column_choice();
        let line = "La colonne montant.";
        for reply in ["montant", "Value: montant", "«montant»", "\"montant\""] {
            assert_eq!(
                offered_part(reply, line, &choice).as_deref(),
                Some("montant"),
                "{reply}"
            );
        }
        for reply in [
            "autre",
            "La colonne",
            "mont",
            "montant.",
            "montant\n{\"choice\": \"autre\"}",
            "montant {\"choice\": \"autre\"}",
            "{\"choice\": \"montant\"}",
            "NONE",
            "",
        ] {
            assert_eq!(offered_part(reply, line, &choice), None, "{reply}");
        }
        // Several offered keys, or a rejected one: the reading chooses, never the first match.
        let both = "Pas montant, autre.";
        assert_eq!(
            offered_part("autre", both, &choice).as_deref(),
            Some("autre")
        );
        assert_eq!(offered_part("montant autre", both, &choice), None);
    }

    #[test]
    fn the_choice_prompt_shows_the_exact_offers_and_asks_for_a_copy() {
        let choice = column_choice();
        let prompt = choice_prompt(&choice, "  La colonne montant.  ");
        assert!(
            prompt.contains("«Quelle colonne additionner ?»"),
            "{prompt}"
        );
        assert!(prompt.contains("«montant» · «autre»"), "{prompt}");
        assert!(prompt.contains("«La colonne montant.»"), "{prompt}");
        assert!(
            prompt.contains("answer NONE") && prompt.ends_with("Value:"),
            "{prompt}"
        );
        assert!(
            !prompt.contains("JSON"),
            "a natural request, never a format: {prompt}"
        );
    }

    /// The seat's `model`, the replacement request, a clause's disposition, a rule asked as
    /// code and a choice that offers nothing keep their own doors whatever their shape.
    #[test]
    fn the_doors_that_keep_their_reading_are_never_read_as_a_choice() {
        let choice = column_choice();
        for key in [
            "model",
            "intent.clarification",
            "gap.1",
            "const.sum_expression",
        ] {
            let mut door = choice.clone();
            door.key = key.to_owned();
            assert!(!is_offered_choice(&door), "{key}");
        }
        let mut code = choice.clone();
        code.label = "Which jq expression sums the column?".to_owned();
        assert!(!is_offered_choice(&code));
        let mut nothing = choice;
        nothing.options.clear();
        assert!(!is_offered_choice(&nothing));
    }
}
