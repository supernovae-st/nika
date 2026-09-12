// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Recorded gate decisions, without consulting today's workflow or identity.

use nika_event::{Event, EventKind};

/// Keep every decision in journal order, including refusals and older frames.
/// Separate from the width-limited verdict card: audit answers must not be cut
/// off, and a decided gate remains visible even without a terminal verdict.
pub(super) fn lines(events: &[Event]) -> Vec<String> {
    let mut lines = Vec::new();
    for event in events
        .iter()
        .filter(|e| e.kind == EventKind::ApprovalDecided)
    {
        lines.push(format!("  gate {}", recorded(event, "task")));
        for (key, label) in [
            ("decision", "decision"),
            ("source", "source"),
            ("operator", "operator (declared)"),
            ("question", "question"),
            ("answer", "answer"),
        ] {
            lines.push(format!("    {label}: {}", recorded(event, key)));
        }
        if event.field("why").is_some() {
            lines.push(format!("    reason: {}", recorded(event, "why")));
        }
    }
    lines
}

fn recorded(event: &Event, key: &str) -> String {
    // JSON quoting retains the full recorded value while escaping terminal
    // controls. Missing historical fields never borrow the current operator.
    event.field(key).map_or_else(
        || "not recorded".to_owned(),
        |value| {
            serde_json::to_string(value).map_or_else(
                |_| "unreadable value".to_owned(),
                |json| {
                    // JSON escapes C0, but permits DEL and C1 controls.
                    json.chars()
                        .map(|c| {
                            if c.is_control() {
                                format!("\\u{:04x}", u32::from(c))
                            } else {
                                c.to_string()
                            }
                        })
                        .collect()
                },
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_display::demo::bare_event;
    use nika_types::resource::{KeyValue, Value};

    fn decision(fields: &[(&str, &str)]) -> Event {
        let mut event = bare_event(EventKind::ApprovalDecided, 0);
        event.fields = fields
            .iter()
            .map(|(k, v)| KeyValue::new(*k, Value::string(*v)))
            .collect();
        event
    }

    #[test]
    fn gate_read_names_recorded_question_answer_decision_source_and_operator() {
        let event = decision(&[
            ("task", "approve"),
            ("decision", "allow"),
            ("source", "resume"),
            ("operator", "alice-ci"),
            ("question", "ship it?"),
            ("answer", "true"),
        ]);
        assert_eq!(
            lines(&[event]),
            [
                "  gate \"approve\"",
                "    decision: \"allow\"",
                "    source: \"resume\"",
                "    operator (declared): \"alice-ci\"",
                "    question: \"ship it?\"",
                "    answer: \"true\"",
            ]
        );
    }

    #[test]
    fn gate_read_keeps_refusals_and_old_missing_fields_in_journal_order() {
        let events = [
            decision(&[
                ("task", "a"),
                ("decision", "deny"),
                ("why", "approval.answer_malformed"),
            ]),
            bare_event(EventKind::TaskCompleted, 1),
            decision(&[("task", "b"), ("decision", "allow"), ("answer", "yes")]),
        ];
        let rows = lines(&events);
        assert_eq!(rows[0], "  gate \"a\"");
        assert_eq!(rows[1], "    decision: \"deny\"");
        assert_eq!(rows[2], "    source: not recorded");
        assert_eq!(rows[3], "    operator (declared): not recorded");
        assert_eq!(rows[4], "    question: not recorded");
        assert_eq!(rows[5], "    answer: not recorded");
        assert_eq!(rows[6], "    reason: \"approval.answer_malformed\"");
        assert_eq!(rows[7], "  gate \"b\"");
        assert_eq!(rows[12], "    answer: \"yes\"");
    }

    #[test]
    fn gate_read_escapes_terminal_controls_and_preserves_full_answers() {
        let answer = format!("{}\n\u{1b}[2J\r\t\u{7f}\u{9b}2J", "answer ".repeat(80));
        let event = decision(&[
            ("task", "approve\nforged"),
            ("operator", "\u{1b}[31m"),
            ("answer", &answer),
        ]);
        let rows = lines(&[event]);
        assert!(rows.iter().all(|row| !row.chars().any(char::is_control)));
        assert!(rows[0].contains("approve\\nforged"));
        assert!(rows[3].contains("\\u001b[31m"));
        let rendered: String =
            serde_json::from_str(rows[5].strip_prefix("    answer: ").expect("answer label"))
                .expect("quoted answer");
        assert_eq!(rendered, answer);
    }

    #[test]
    fn ordinary_traces_gain_no_gate_rows() {
        assert!(lines(&[]).is_empty());
        assert!(lines(&[bare_event(EventKind::WorkflowCompleted, 0)]).is_empty());
    }
}
