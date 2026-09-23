// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Lossless native candidate transport. A line array avoids folded JSON strings;
//! it grants no authority and still goes through the same parser, Check and judge.

#[derive(serde::Deserialize)]
#[serde(try_from = "WireAnswer")]
pub(in crate::cognition) struct Answer {
    pub(in crate::cognition) candidate: String,
    pub(in crate::cognition) questions: Vec<Question>,
    pub(in crate::cognition) gaps: Vec<String>,
    pub(in crate::cognition) notes: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WireAnswer {
    #[serde(default)]
    candidate: String,
    #[serde(default)]
    candidate_lines: Vec<String>,
    #[serde(default, deserialize_with = "nullable_questions")]
    questions: Vec<Question>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_vec")]
    gaps: Vec<String>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_string")]
    notes: String,
}

impl TryFrom<WireAnswer> for Answer {
    type Error = &'static str;

    fn try_from(wire: WireAnswer) -> Result<Self, Self::Error> {
        let candidate = if wire.candidate_lines.is_empty() {
            wire.candidate
        } else {
            if !wire.candidate.is_empty() {
                return Err(
                    "answer carries two candidates; use candidate OR candidate_lines, leave the other empty",
                );
            }
            if wire
                .candidate_lines
                .iter()
                .any(|line| line.contains(['\n', '\r']))
            {
                return Err("candidate_lines must contain one physical line per element");
            }
            // Join exactly, without trimming indentation, decoding HTML or adding a newline.
            // An empty final element preserves a requested final newline.
            wire.candidate_lines.join("\n")
        };
        Ok(Self {
            candidate,
            questions: wire.questions,
            gaps: wire.gaps,
            notes: wire.notes,
        })
    }
}

#[derive(serde::Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(in crate::cognition) struct Question {
    pub(in crate::cognition) key: String,
    pub(in crate::cognition) label: String,
    #[serde(default, deserialize_with = "crate::cognition::nullable_string")]
    pub(in crate::cognition) answer_type: String,
    #[serde(default, deserialize_with = "crate::cognition::nullable_string")]
    pub(in crate::cognition) why: String,
}

pub(in crate::cognition) fn nullable_questions<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<Question>, D::Error> {
    use serde::Deserialize as _;
    Ok(Option::<Vec<Question>>::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::Answer;
    use serde_json::json;

    #[test]
    fn lines_preserve_indentation_blank_lines_unicode_and_final_newline() {
        let source = "nika: example\ntasks:\n  render:\n    value: |\n      café 雪\n\n";
        let answer: Answer = serde_json::from_value(json!({
            "candidate": "", "candidate_lines": source.split('\n').collect::<Vec<_>>()
        }))
        .unwrap();
        assert_eq!(answer.candidate.as_bytes(), source.as_bytes());
        let no_final: Answer =
            serde_json::from_value(json!({"candidate_lines": ["nika: example", "tasks: {}"]}))
                .unwrap();
        assert_eq!(no_final.candidate, "nika: example\ntasks: {}");
    }

    #[test]
    fn ambiguous_or_malformed_line_documents_are_not_silently_rewritten() {
        for payload in [
            json!({"candidate": "nika: other", "candidate_lines": ["nika: x"]}),
            json!({"candidate": "nika: x", "candidate_lines": ["nika: x"]}),
            json!({"candidate_lines": ["nika: x\ntasks: {}"]}),
            json!({"candidate_lines": ["nika: x\r"]}),
            json!({"candidate_lines": null}),
            json!({"candidate_lines": [7]}),
            json!({"candidate_lines": ["nika: x"], "unknown": true}),
        ] {
            assert!(
                serde_json::from_value::<Answer>(payload.clone()).is_err(),
                "{payload}"
            );
        }
    }

    #[test]
    fn legacy_text_answer_remains_byte_identical() {
        let source = "nika: legacy\ntasks: {}\n";
        let answer: Answer =
            serde_json::from_value(json!({"candidate": source, "questions": null})).unwrap();
        assert_eq!(answer.candidate, source);
        assert!(answer.questions.is_empty());
    }
}
