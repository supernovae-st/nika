// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Lossless native candidate transport. A line array avoids folded JSON strings;
//! it grants no authority and still goes through the same parser, Check and judge.
//!
//! An answer carries ONE candidate, never a choice between two. `candidate_lines` reads as
//! its elements joined with LF, the only newline normalization: nothing follows the last
//! element (an empty final element is the final newline), and an element holding a YAML line
//! break (LF, CR, NEL, LS or PS) is refused before any comparison. `candidate` reads as sent:
//! no CRLF rewrite, no final newline added or dropped, no trim, no Unicode or HTML
//! normalization. An empty text is an absent representation. Two present texts are one
//! candidate only when byte-identical (a free, deterministic canonicalization); otherwise they
//! conflict, and nothing here may choose. Either way the journal keeps both texts' evidence.

use super::knowledge;
use serde_json::{Value, json};

/// The line breaks a YAML 1.1 reader honors: none may hide inside one physical line.
const LINE_BREAKS: [char; 5] = ['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}'];

pub(in crate::cognition) struct Answer {
    pub(in crate::cognition) candidate: String,
    pub(in crate::cognition) questions: Vec<Question>,
    pub(in crate::cognition) gaps: Vec<String>,
    pub(in crate::cognition) notes: String,
    /// Both texts' evidence when the seat sent the candidate twice, identical; None for one.
    /// Boxed: a rare path that must not grow every round's result.
    pub(in crate::cognition) dual: Option<Box<Dual>>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireAnswer {
    #[serde(default)]
    candidate: String,
    #[serde(default)]
    candidate_lines: Vec<String>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    questions: Vec<Question>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    gaps: Vec<String>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    notes: String,
}

/// Both texts of an answer that carried two: the digest and length of each (the lines as
/// joined), where they first differ and the bytes kept. Evidence of what the seat sent, never
/// of acceptance: the judged source is the round's own `candidate_sha256`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::cognition) struct Dual {
    text_sha256: String,
    text_bytes: usize,
    joined_sha256: String,
    joined_bytes: usize,
    /// The first differing byte offset (the shorter length when one text prefixes the other).
    first_difference: Option<usize>,
    /// Whether the texts differ only in line endings or final newlines: evidence for the
    /// seat's qualification, never a ground to accept either text.
    newlines_only: bool,
}

impl Dual {
    fn of(text: &str, joined: &str) -> Self {
        let first_difference = text
            .bytes()
            .zip(joined.bytes())
            .position(|(a, b)| a != b)
            .or_else(|| (text.len() != joined.len()).then(|| text.len().min(joined.len())));
        let shape = |s: &str| s.replace("\r\n", "\n").trim_end_matches('\n').to_owned();
        Self {
            text_sha256: knowledge::sha256(text),
            text_bytes: text.len(),
            joined_sha256: knowledge::sha256(joined),
            joined_bytes: joined.len(),
            newlines_only: first_difference.is_some() && shape(text) == shape(joined),
            first_difference,
        }
    }

    /// The round journal's `transport` object.
    pub(in crate::cognition) fn record(&self) -> Value {
        let equivalent = self.first_difference.is_none();
        json!({
            "verdict": if equivalent { "EQUIVALENT" } else { "CONFLICTING" },
            "candidate": {"sha256": self.text_sha256, "bytes": self.text_bytes},
            "candidate_lines": {"joined_sha256": self.joined_sha256, "joined_bytes": self.joined_bytes},
            "first_difference": self.first_difference,
            "newlines_only": self.newlines_only,
            "kept_sha256": equivalent.then_some(&self.text_sha256),
        })
    }
}

/// Why a well-formed answer carries no single candidate: a typed transport class, never a
/// reason to choose or to buy another call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Defect {
    /// Both representations carry text and their bytes differ.
    ConflictingCandidates(Dual),
    /// A `candidate_lines` element holds a line break: it is not one physical line.
    CandidateLinePhysicalBreak {
        /// The first such element, from 0.
        element: usize,
        /// The break it holds.
        code_point: char,
    },
}

impl Defect {
    /// The diagnostic class the round journal records.
    pub(super) const fn class(&self) -> &'static str {
        match self {
            Self::ConflictingCandidates(_) => "CONFLICTING_CANDIDATES",
            Self::CandidateLinePhysicalBreak { .. } => "CANDIDATE_LINE_PHYSICAL_BREAK",
        }
    }
}

impl std::fmt::Display for Defect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: ", self.class())?;
        match self {
            Self::ConflictingCandidates(_) => write!(
                f,
                "candidate and candidate_lines carry different texts; neither was chosen"
            ),
            Self::CandidateLinePhysicalBreak {
                element,
                code_point,
            } => write!(
                f,
                "candidate_lines must contain one physical line per element; element {element} holds U+{:04X}",
                u32::from(*code_point)
            ),
        }
    }
}

impl TryFrom<WireAnswer> for Answer {
    type Error = Defect;

    fn try_from(wire: WireAnswer) -> Result<Self, Self::Error> {
        let (candidate, dual) = one_candidate(wire.candidate, &wire.candidate_lines)?;
        Ok(Self {
            candidate,
            questions: wire.questions,
            gaps: wire.gaps,
            notes: wire.notes,
            dual,
        })
    }
}

/// The single candidate the two representations carry, by the rule in the module header, with
/// both texts' evidence when both were present.
fn one_candidate(text: String, lines: &[String]) -> Result<(String, Option<Box<Dual>>), Defect> {
    if lines.is_empty() {
        return Ok((text, None));
    }
    let physical_break = lines.iter().enumerate().find_map(|(element, line)| {
        line.chars()
            .find(|c| LINE_BREAKS.contains(c))
            .map(|code_point| (element, code_point))
    });
    if let Some((element, code_point)) = physical_break {
        return Err(Defect::CandidateLinePhysicalBreak {
            element,
            code_point,
        });
    }
    // Join exactly, without trimming indentation, decoding HTML or adding a newline.
    // An empty final element preserves a requested final newline.
    let joined = lines.join("\n");
    if text.is_empty() {
        return Ok((joined, None));
    }
    if joined.is_empty() {
        return Ok((text, None));
    }
    let dual = Dual::of(&text, &joined);
    if dual.first_difference.is_some() {
        return Err(Defect::ConflictingCandidates(dual));
    }
    Ok((joined, Some(Box::new(dual))))
}

#[derive(serde::Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(in crate::cognition) struct Question {
    pub(in crate::cognition) key: String,
    pub(in crate::cognition) label: String,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    pub(in crate::cognition) answer_type: String,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    pub(in crate::cognition) why: String,
}

#[cfg(test)]
mod tests {
    use super::{Answer, Defect, Dual, WireAnswer, knowledge};
    use serde_json::{Value, json};

    /// The answer a schema-valid payload carries, or the defect that refuses it.
    fn read(payload: &Value) -> Result<Answer, Defect> {
        let wire: WireAnswer = serde_json::from_value(payload.clone()).unwrap();
        Answer::try_from(wire)
    }

    fn one(payload: &Value) -> Result<String, Defect> {
        read(payload).map(|answer| answer.candidate)
    }

    const SOURCE: &str = "nika: example\ntasks:\n  render:\n    value: |\n      café 雪\n\n";

    #[test]
    fn lines_preserve_indentation_blank_lines_unicode_and_final_newline() {
        let lines: Vec<&str> = SOURCE.split('\n').collect();
        let joined = one(&json!({"candidate": "", "candidate_lines": lines})).unwrap();
        assert_eq!(joined.as_bytes(), SOURCE.as_bytes());
        assert_eq!(
            one(&json!({"candidate_lines": ["nika: example", "tasks: {}"]})),
            Ok("nika: example\ntasks: {}".to_owned())
        );
    }

    #[test]
    fn byte_identical_representations_are_one_candidate_with_both_texts_in_evidence() {
        // BUG-A1 (black-box audit, 2026-09-27, 4/4): the seat filled both fields.
        let lines: Vec<&str> = SOURCE.split('\n').collect();
        let Ok(both) = read(&json!({"candidate": SOURCE, "candidate_lines": lines})) else {
            panic!("identical texts are one candidate");
        };
        assert_eq!(both.candidate.as_bytes(), SOURCE.as_bytes());
        let digest = knowledge::sha256(SOURCE);
        let Some(dual) = both.dual else {
            panic!("both texts are kept in evidence");
        };
        assert_eq!(
            dual.record(),
            json!({
                "verdict": "EQUIVALENT",
                "candidate": {"sha256": digest, "bytes": SOURCE.len()},
                "candidate_lines": {"joined_sha256": digest, "joined_bytes": SOURCE.len()},
                "first_difference": null,
                "newlines_only": false,
                "kept_sha256": digest,
            })
        );
        assert_eq!(
            one(&json!({"candidate": "nika: x", "candidate_lines": ["nika: x"]})),
            Ok("nika: x".to_owned())
        );
    }

    #[test]
    fn an_empty_text_is_an_absent_representation() {
        for (payload, candidate) in [
            (
                json!({"candidate": "nika: x", "candidate_lines": []}),
                "nika: x",
            ),
            (
                json!({"candidate": "nika: x", "candidate_lines": [""]}),
                "nika: x",
            ),
            (
                json!({"candidate": "", "candidate_lines": ["nika: x"]}),
                "nika: x",
            ),
            (json!({"candidate": "", "candidate_lines": [""]}), ""),
            (json!({"candidate": "", "candidate_lines": []}), ""),
            (json!({}), ""),
        ] {
            let Ok(answer) = read(&payload) else {
                panic!("{payload}: one text or none");
            };
            assert_eq!(answer.candidate, candidate, "{payload}");
            assert!(answer.dual.is_none(), "{payload}: one text is no dual");
        }
    }

    #[test]
    fn different_texts_conflict_and_neither_is_chosen() {
        for (text, lines, first_difference, newlines_only) in [
            ("nika: other", vec!["nika: x"], 6, false),
            ("nika: x", vec!["nika: other"], 6, false),
            (" ", vec!["nika: x"], 0, false),
            ("nika: x ", vec!["nika: x"], 7, false),
            ("nika: x", vec!["", ""], 0, false),
            ("café", vec!["cafe\u{301}"], 3, false), // NFC against NFD: no Unicode normalization
            ("nika: x\n", vec!["nika: x"], 7, true), // no final newline is added or dropped
            ("nika: x", vec!["nika: x", ""], 7, true),
            (
                "nika: x\r\ntasks: {}",
                vec!["nika: x", "tasks: {}"],
                7,
                true,
            ), // no CRLF rewrite
        ] {
            let payload = json!({"candidate": text, "candidate_lines": lines});
            let joined = lines.join("\n");
            let Err(Defect::ConflictingCandidates(dual)) = read(&payload) else {
                panic!("{payload}: two different texts are a conflict");
            };
            assert_eq!(dual, Dual::of(text, &joined), "{payload}");
            let record = dual.record();
            assert_eq!(record["verdict"], "CONFLICTING", "{payload}");
            assert_eq!(record["first_difference"], first_difference, "{payload}");
            assert_eq!(record["newlines_only"], newlines_only, "{payload}");
            assert_eq!(
                record["kept_sha256"],
                Value::Null,
                "nothing kept: {payload}"
            );
            assert_eq!(record["candidate"]["sha256"], knowledge::sha256(text));
            assert_eq!(record["candidate"]["bytes"], text.len());
            assert_eq!(
                record["candidate_lines"]["joined_sha256"],
                knowledge::sha256(&joined)
            );
            assert_eq!(record["candidate_lines"]["joined_bytes"], joined.len());
        }
    }

    #[test]
    fn a_line_holding_a_break_is_refused_before_any_comparison() {
        for (payload, element, code_point) in [
            (json!({"candidate_lines": ["nika: x\ntasks: {}"]}), 0, '\n'),
            (json!({"candidate_lines": ["nika: x\r"]}), 0, '\r'),
            (
                json!({"candidate_lines": ["nika: x", "tasks:", "  a: b\rc"]}),
                2,
                '\r',
            ),
            // A YAML 1.1 reader breaks lines at NEL, LS and PS too.
            (
                json!({"candidate_lines": ["nika: x\u{85}tasks: {}"]}),
                0,
                '\u{85}',
            ),
            (
                json!({"candidate_lines": ["nika: x\u{2028}tasks: {}"]}),
                0,
                '\u{2028}',
            ),
            (
                json!({"candidate_lines": ["nika: x\u{2029}tasks: {}"]}),
                0,
                '\u{2029}',
            ),
            // Joined, the element equals the text; it is still not one physical line.
            (
                json!({"candidate": "nika: x\ntasks: {}", "candidate_lines": ["nika: x\ntasks: {}"]}),
                0,
                '\n',
            ),
            (
                json!({"candidate": "nika: x\u{2028}", "candidate_lines": ["nika: x\u{2028}"]}),
                0,
                '\u{2028}',
            ),
        ] {
            assert_eq!(
                one(&payload),
                Err(Defect::CandidateLinePhysicalBreak {
                    element,
                    code_point
                }),
                "{payload}"
            );
        }
        let Err(defect) = one(&json!({"candidate_lines": ["nika: x", "tasks:", "  a: b\rc"]}))
        else {
            panic!("a break is refused");
        };
        assert!(
            defect.to_string().ends_with("element 2 holds U+000D"),
            "{defect}"
        );
    }

    #[test]
    fn a_shape_outside_the_answer_schema_never_decodes() {
        for payload in [
            json!({"candidate_lines": null}),
            json!({"candidate": "nika: x", "candidate_lines": null}),
            json!({"candidate": null, "candidate_lines": ["nika: x"]}),
            json!({"candidate": 7}),
            json!({"candidate_lines": [7]}),
            json!({"candidate_lines": ["nika: x"], "unknown": true}),
        ] {
            assert!(
                serde_json::from_value::<WireAnswer>(payload.clone()).is_err(),
                "{payload}"
            );
        }
    }

    #[test]
    fn legacy_text_answer_remains_byte_identical() {
        let source = "nika: legacy\ntasks: {}\n";
        let wire: WireAnswer =
            serde_json::from_value(json!({"candidate": source, "questions": null})).unwrap();
        let Ok(answer) = Answer::try_from(wire) else {
            panic!("a single text is one candidate");
        };
        assert_eq!(answer.candidate, source);
        assert!(answer.questions.is_empty());
    }
}
