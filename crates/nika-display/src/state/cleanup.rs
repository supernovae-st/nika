// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cleanup witnesses belong to their declared unwind row, never the parent.

use super::{Event, EventKind, RunView, TaskState, str_field};

/// One execution attachment; a shared task must retain every parent's issue.
#[derive(Debug)]
pub(super) struct Attachment {
    parent: String,
    gate: String,
    state: TaskState,
    outcome: &'static str,
    detail: String,
}

impl Attachment {
    fn new(parent: String, gate: String) -> Self {
        Self {
            parent,
            gate,
            state: TaskState::Pending,
            outcome: "no outcome recorded",
            detail: String::new(),
        }
    }
}

impl RunView {
    /// Main-lane tasks only; unwind rows have their own best-effort outcomes.
    #[must_use]
    pub fn main_task_count(&self) -> usize {
        self.rows.len().saturating_sub(self.cleanup.len())
    }

    /// Resolve an unwind witness through declaration metadata. Old traces
    /// without that metadata remain readable, but cannot prove attribution.
    #[must_use]
    pub fn cleanup_task(&self, event: &Event) -> Option<&str> {
        if event.kind != EventKind::PermitChecked
            || str_field(event, "plane") != Some("on_finally")
            || !matches!(
                str_field(event, "decision"),
                Some("attempt" | "success" | "failure" | "skipped" | "timeout")
            )
        {
            return None;
        }
        let parent = str_field(event, "task")?;
        let gate = str_field(event, "gate")?;
        let mut matches = self.cleanup.iter().flat_map(|(id, links)| {
            links
                .iter()
                .filter_map(move |a| (a.parent == parent && a.gate == gate).then_some(id.as_str()))
        });
        let id = matches.next()?;
        matches.next().is_none().then_some(id)
    }

    pub(super) fn declare_cleanup(&mut self, event: &Event) {
        let Some(id) = str_field(event, "task") else {
            return;
        };
        let Some(links) = attachments(event).filter(|links| !links.is_empty()) else {
            return;
        };
        self.cleanup.insert(id.to_owned(), links);
        self.refresh_cleanup(id);
    }

    pub(super) fn apply_cleanup(&mut self, event: &Event) {
        let Some(id) = self.cleanup_task(event).map(str::to_owned) else {
            return;
        };
        let Some(links) = self.cleanup.get_mut(&id) else {
            return;
        };
        let Some(link) = links.iter_mut().find(|link| {
            Some(link.parent.as_str()) == str_field(event, "task")
                && Some(link.gate.as_str()) == str_field(event, "gate")
        }) else {
            return;
        };
        (link.state, link.outcome) = match str_field(event, "decision") {
            Some("attempt") => (TaskState::Running, "attempted · no outcome recorded"),
            Some("success") => (TaskState::Ok, "success"),
            Some("failure") => (TaskState::Failed, "failed"),
            Some("skipped") => (TaskState::Skipped, "skipped"),
            Some("timeout") => (TaskState::Failed, "timed out"),
            _ => return,
        };
        if matches!(link.state, TaskState::Failed | TaskState::Skipped) {
            str_field(event, "why")
                .unwrap_or_default()
                .clone_into(&mut link.detail);
        }
        self.refresh_cleanup(&id);
    }

    fn refresh_cleanup(&mut self, id: &str) {
        let (Some(links), Some(&i)) = (self.cleanup.get(id), self.index.get(id)) else {
            return;
        };
        let row = &mut self.rows[i];
        // A later success cannot erase another attachment's failure, skip or
        // absent outcome. The note keeps every issue beside its actual parent.
        row.state = [
            TaskState::Failed,
            TaskState::Running,
            TaskState::Pending,
            TaskState::Skipped,
        ]
        .into_iter()
        .find(|state| links.iter().any(|a| a.state == *state))
        .unwrap_or(TaskState::Ok);
        row.note = links
            .iter()
            .map(|a| format!("cleanup of {} · {}", a.parent, a.outcome))
            .collect::<Vec<_>>()
            .join("; ");
        row.detail = links
            .iter()
            .filter(|a| !a.detail.is_empty())
            .map(|a| format!("{}: {}", a.parent, a.detail))
            .collect::<Vec<_>>()
            .join("; ");
        // Settle-time witnesses never invent execution timing/output/usage.
    }

    pub(crate) fn is_cleanup(&self, id: &str) -> bool {
        self.cleanup.contains_key(id)
    }
}

/// The array is authoritative when present; malformed data cannot fall back
/// to a singleton that would silently drop another attachment. Pre-array
/// singleton traces remain readable, and older unattributed traces stay inert.
fn attachments(event: &Event) -> Option<Vec<Attachment>> {
    if let Some(json) = str_field(event, "cleanup_attachments") {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        return value
            .as_array()?
            .iter()
            .map(|a| {
                Some(Attachment::new(
                    a.get("parent")?.as_str()?.to_owned(),
                    a.get("gate")?.as_str()?.to_owned(),
                ))
            })
            .collect();
    }
    Some(vec![Attachment::new(
        str_field(event, "cleanup_parent")?.to_owned(),
        str_field(event, "cleanup_gate")?.to_owned(),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{demo, render, theme::Theme};
    use nika_types::resource::{KeyValue, Value};

    fn event(kind: EventKind, fields: &[(&str, &str)]) -> Event {
        let mut event = demo::bare_event(kind, 10);
        for (key, value) in fields {
            event = event.with_field(KeyValue::new(*key, Value::String((*value).to_owned())));
        }
        event
    }

    fn declaration(id: &str, parent: &str, gate: &str) -> Event {
        event(
            EventKind::TaskScheduled,
            &[
                ("task", id),
                ("cleanup_parent", parent),
                ("cleanup_gate", gate),
            ],
        )
    }

    fn witness(parent: &str, gate: &str, decision: &str) -> Event {
        event(
            EventKind::PermitChecked,
            &[
                ("task", parent),
                ("plane", "on_finally"),
                ("gate", gate),
                ("decision", decision),
                ("why", "recorded explanation"),
            ],
        )
    }

    #[test]
    fn cleanup_outcomes_fold_and_replay_without_main_settlements() {
        for (decision, state, label) in [
            ("success", TaskState::Ok, "success"),
            ("failure", TaskState::Failed, "failed"),
            ("skipped", TaskState::Skipped, "skipped"),
            ("timeout", TaskState::Failed, "timed out"),
            ("attempt", TaskState::Running, "no outcome recorded"),
        ] {
            let events = [
                event(EventKind::TaskScheduled, &[("task", "main")]),
                declaration("cleanup", "main", "cleanup #0"),
                witness("main", "cleanup #0", "attempt"),
                witness("main", "cleanup #0", decision),
                event(EventKind::TaskCompleted, &[("task", "main")]),
                // Old cancellation logic must not erase the cleanup witness.
                event(
                    EventKind::TaskCancelled,
                    &[("task", "cleanup"), ("note", "cancelled by the operator")],
                ),
                event(EventKind::WorkflowCompleted, &[]),
            ];
            let mut live = RunView::new();
            let mut replay = RunView::new();
            for e in &events {
                live.apply(e);
                let serialized = serde_json::to_string(e).expect("encode trace");
                replay.apply(&serde_json::from_str(&serialized).expect("decode trace"));
            }
            assert_eq!(live.rows()[1].state, state);
            assert!(live.rows()[1].note.contains(label));
            assert!(live.rows()[1].note.contains("cleanup of main"));
            assert_eq!(
                (
                    live.main_task_count(),
                    live.done_count(),
                    live.failed_count(),
                    live.cancelled_count()
                ),
                (1, 1, 0, 0)
            );
            assert_eq!(live.verdict, Some(true));
            assert_eq!(live.rows()[1].wall_ms(), None);
            assert_eq!(live.rows()[1].output_json, None);
            let theme = Theme::new(false, false, false);
            let frame = render::frame(&live, &theme, 0);
            assert_eq!(frame, render::frame(&replay, &theme, 0));
            let text = frame.join("\n");
            assert!(text.contains("↳"), "{text}");
            assert!(text.contains("1/1 done"), "{text}");
            let quiet = render::verdict_frame(&live, &theme).join("\n");
            assert!(quiet.contains(label), "{quiet}");
            assert!(quiet.contains("1 task"), "{quiet}");
            assert!(!text.contains("cancelled by the operator"), "{text}");
            assert!(
                render::stream_settled_line(&live, "cleanup", &theme, false)
                    .expect("line")
                    .contains(label)
            );
        }
    }

    #[test]
    fn attribution_requires_parent_and_gate_and_never_guesses_old_traces() {
        let mut view = RunView::new();
        view.apply(&declaration("a_cleanup", "a", "cleanup #0"));
        view.apply(&declaration("b_cleanup", "b", "cleanup #0"));
        view.apply(&witness("b", "cleanup #0", "failure"));
        assert_eq!(view.rows()[0].state, TaskState::Pending);
        assert_eq!(view.rows()[1].state, TaskState::Failed);
        view.apply(&witness("a", "cleanup #1", "success"));
        assert_eq!(view.rows()[0].state, TaskState::Pending);
        view.apply(&event(EventKind::TaskFailed, &[("task", "b")]));
        view.apply(&event(
            EventKind::TaskCancelled,
            &[("task", "dependent"), ("note", "upstream failed")],
        ));
        let rendered = render::frame(&view, &Theme::new(false, false, false), 0).join("\n");
        assert!(rendered.contains("blocked · b failed"), "{rendered}");
        assert_eq!((view.failed_count(), view.cancelled_count()), (1, 1));
        view.apply(&declaration("ambiguous", "b", "cleanup #0"));
        assert!(
            view.cleanup_task(&witness("b", "cleanup #0", "success"))
                .is_none()
        );
        let mut old = RunView::new();
        old.apply(&event(
            EventKind::TaskScheduled,
            &[("task", "unknown_cleanup")],
        ));
        old.apply(&witness("parent", "cleanup #0", "attempt"));
        old.apply(&witness("parent", "cleanup #0", "success"));
        assert_eq!(old.rows().len(), 1);
        assert_eq!(old.rows()[0].state, TaskState::Pending);
        assert_eq!(old.done_count(), 0);
    }
    #[test]
    fn shared_cleanup_keeps_each_parent_outcome_and_card_main_count() {
        let mut live = RunView::new();
        let mut replay = RunView::new();
        let events = [
            event(EventKind::TaskScheduled, &[("task", "a")]),
            event(EventKind::TaskScheduled, &[("task", "b")]),
            event(
                EventKind::TaskScheduled,
                &[
                    ("task", "shared"),
                    (
                        "cleanup_attachments",
                        r#"[{"parent":"a","gate":"cleanup #0"},{"parent":"b","gate":"cleanup #0"}]"#,
                    ),
                ],
            ),
            declaration("a_tail", "a", "cleanup #1"),
            declaration("b_tail", "b", "cleanup #1"),
            witness("a", "cleanup #0", "failure"),
            witness("a", "cleanup #1", "success"),
            witness("b", "cleanup #0", "success"),
            witness("b", "cleanup #1", "skipped"),
            event(EventKind::TaskCompleted, &[("task", "a")]),
            event(EventKind::TaskCompleted, &[("task", "b")]),
            event(EventKind::WorkflowCompleted, &[]),
        ];
        for e in &events {
            live.apply(e);
            replay.apply(
                &serde_json::from_str(&serde_json::to_string(e).expect("encode")).expect("decode"),
            );
        }
        assert_eq!(live.rows()[2].state, TaskState::Failed);
        assert!(live.rows()[2].note.contains("cleanup of a · failed"));
        assert!(live.rows()[2].note.contains("cleanup of b · success"));
        assert_eq!(live.rows()[3].state, TaskState::Ok);
        assert_eq!(live.rows()[4].state, TaskState::Skipped);
        assert_eq!(
            (
                live.main_task_count(),
                live.done_count(),
                live.failed_count()
            ),
            (2, 2, 0)
        );
        let mut theme = Theme::new(false, false, false);
        theme.accents = true;
        let card = crate::flow::verdict_card(&live, &theme, &[]);
        assert_eq!(card, crate::flow::verdict_card(&replay, &theme, &[]));
        assert!(card.join("\n").contains("2 tasks"), "{card:?}");
        assert!(!card.join("\n").contains("5 tasks"), "{card:?}");
        assert_eq!(
            render::frame(&live, &theme, 0),
            render::frame(&replay, &theme, 0)
        );
    }

    #[test]
    fn final_card_excludes_single_parent_cleanup() {
        let mut view = RunView::new();
        view.apply(&event(EventKind::TaskCompleted, &[("task", "main")]));
        view.apply(&declaration("cleanup", "main", "cleanup #0"));
        view.apply(&witness("main", "cleanup #0", "success"));
        view.apply(&event(EventKind::WorkflowCompleted, &[]));
        let card =
            crate::flow::verdict_card(&view, &Theme::new(false, false, false), &[]).join("\n");
        assert!(card.contains("1 task"), "{card}");
        assert!(!card.contains("2 tasks"), "{card}");
    }
    #[test]
    fn shared_cleanup_waits_for_every_attachment_and_rejects_malformed_metadata() {
        let shared = event(
            EventKind::TaskScheduled,
            &[
                ("task", "shared"),
                (
                    "cleanup_attachments",
                    r#"[{"parent":"a","gate":"cleanup #0"},{"parent":"b","gate":"cleanup #0"}]"#,
                ),
            ],
        );
        let mut view = RunView::new();
        view.apply(&shared);
        view.apply(&witness("b", "cleanup #0", "success"));
        assert_eq!(view.rows()[0].state, TaskState::Pending);
        assert!(
            view.rows()[0]
                .note
                .contains("cleanup of a · no outcome recorded")
        );
        view.apply(&witness("a", "cleanup #0", "attempt"));
        assert_eq!(view.rows()[0].state, TaskState::Running);
        view.apply(&witness("a", "cleanup #0", "timeout"));
        assert_eq!(view.rows()[0].state, TaskState::Failed);
        assert!(view.rows()[0].note.contains("cleanup of a · timed out"));
        let mut malformed = RunView::new();
        malformed.apply(&event(
            EventKind::TaskScheduled,
            &[
                ("task", "bad"),
                ("cleanup_parent", "a"),
                ("cleanup_gate", "cleanup #0"),
                ("cleanup_attachments", "[{}]"),
            ],
        ));
        malformed.apply(&witness("a", "cleanup #0", "success"));
        assert_eq!(malformed.rows()[0].state, TaskState::Pending);
    }
}
