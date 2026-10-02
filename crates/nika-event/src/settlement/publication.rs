// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Project a terminal message already prepared for publication.

use crate::{Event, EventKind};
use nika_types::id::ExecutionId;

use super::{RunCause, RunSettlement, RunState};

#[derive(Debug)]
enum Close {
    Missing,
    NoError(RunState),
    Message(RunState, String),
    Invalid,
}

/// A single execution's observed publication diagnostic, with no I/O.
/// Feed the already-redacted root stream in order; child streams stay separate.
/// It retains only the supplied diagnostic, not a secret map or a run verdict.
#[derive(Debug)]
#[non_exhaustive]
pub struct TerminalDiagnostic {
    execution: ExecutionId,
    seen: bool,
    opened: bool,
    close: Close,
}

impl TerminalDiagnostic {
    /// Bind a fresh observer to the execution whose terminal will be published.
    #[must_use]
    pub const fn new(execution: ExecutionId) -> Self {
        Self {
            execution,
            seen: false,
            opened: false,
            close: Close::Missing,
        }
    }

    /// Observe one event without altering or emitting it.
    /// Identity mismatch, duplicate opening/close, or incomplete error fields
    /// invalidate the projection; no subsequent event repairs that state.
    pub fn observe(&mut self, event: &Event) {
        if event.execution != Some(self.execution) || !matches!(self.close, Close::Missing) {
            self.close = Close::Invalid;
            return;
        }
        if event.kind == EventKind::WorkflowStarted {
            if self.seen {
                self.close = Close::Invalid;
                return;
            }
            self.opened = true;
        }
        self.seen = true;
        let Some(state) = RunState::from_terminal_kind(event.kind) else {
            return;
        };
        if !self.opened {
            self.close = Close::Invalid;
            return;
        }
        let count = |key| event.fields.iter().filter(|field| field.key == key).count();
        self.close = match (
            count("error_code"),
            count("error_message"),
            count("error_task"),
        ) {
            (0, 0, 0) => Close::NoError(state),
            (1, 1, task_count)
                if task_count <= 1
                    && event.str_field("error_code").is_some()
                    && (task_count == 0 || event.str_field("error_task").is_some()) =>
            {
                event
                    .str_field("error_message")
                    .map_or(Close::Invalid, |message| {
                        Close::Message(state, message.to_owned())
                    })
            }
            _ => Close::Invalid,
        };
    }

    /// Clone the runtime verdict, replacing only its message for publication.
    /// Codes, task identity, state, cause, spend and tally always come from `raw`.
    /// A pre-prologue refusal is the sole no-event case; a missing or malformed
    /// terminal after any event returns `None`, never the raw diagnostic.
    #[must_use]
    pub fn project(&self, raw: &RunSettlement) -> Option<RunSettlement> {
        match &self.close {
            Close::Missing
                if !self.seen
                    && raw.state == RunState::Failed
                    && raw.cause == RunCause::Refused =>
            {
                Some(raw.clone())
            }
            Close::NoError(state) if *state == raw.state && raw.error.is_none() => {
                Some(raw.clone())
            }
            Close::Message(state, message) if *state == raw.state => {
                let mut published = raw.clone();
                published.error.as_mut()?.message.clone_from(message);
                Some(published)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settlement::{SettlementError, TaskTally};
    use nika_types::{
        id::EventId,
        resource::{KeyValue, Value},
        timestamp::Timestamp,
    };
    use uuid::Uuid;

    fn execution() -> ExecutionId {
        ExecutionId::new(Uuid::nil())
    }
    fn event(kind: EventKind) -> Event {
        Event::new(EventId::new(Uuid::nil()), Timestamp::from_unix_ms(0), kind)
            .with_execution(execution())
    }
    fn failed() -> RunSettlement {
        let mut tasks = TaskTally::new();
        tasks.total = 2;
        tasks.failed = 1;
        RunSettlement::new(RunState::Failed, RunCause::TaskFailed)
            .with_tasks(tasks)
            .with_elapsed_ms(10)
            .with_error(Some(SettlementError::new(
                "NIKA-TEST-001",
                "raw resolved value",
                Some("task".to_owned()),
            )))
    }
    fn terminal() -> Event {
        event(EventKind::WorkflowFailed)
            .with_field(KeyValue::new("error_code", Value::string("***")))
            .with_field(KeyValue::new("error_task", Value::string("***")))
            .with_field(KeyValue::new("error_message", Value::string("masked: ***")))
    }
    fn opened() -> TerminalDiagnostic {
        let mut diagnostic = TerminalDiagnostic::new(execution());
        diagnostic.observe(&event(EventKind::WorkflowStarted));
        diagnostic
    }

    #[test]
    fn publication_replaces_only_the_observed_message() {
        let raw = failed();
        let before = raw.clone();
        let mut diagnostic = opened();
        diagnostic.observe(&terminal());
        let projected = diagnostic.project(&raw).expect("complete terminal");
        let mut expected = raw.clone();
        expected.error.as_mut().expect("error").message = "masked: ***".to_owned();
        assert_eq!(projected, expected);
        assert_eq!(raw, before, "downstream data must remain intact");
    }

    #[test]
    fn publication_distinguishes_a_clean_terminal_and_a_preflight_refusal() {
        let mut diagnostic = opened();
        diagnostic.observe(&event(EventKind::WorkflowCompleted));
        let clean = RunSettlement::new(RunState::Succeeded, RunCause::Normal);
        assert_eq!(diagnostic.project(&clean), Some(clean.clone()));
        assert!(diagnostic.project(&failed()).is_none());
        let fresh = TerminalDiagnostic::new(execution());
        let refusal = RunSettlement::new(RunState::Failed, RunCause::Refused).with_error(Some(
            SettlementError::new("NIKA-TEST-001", "preflight refusal", None),
        ));
        assert_eq!(fresh.project(&refusal), Some(refusal.clone()));
        assert!(fresh.project(&failed()).is_none());
        assert!(
            opened().project(&refusal).is_none(),
            "an in-run refusal cannot expose raw content"
        );
    }

    #[test]
    fn publication_refuses_incomplete_or_mismatched_terminal_messages() {
        let complete = terminal();
        let mut missing = complete.clone();
        missing.fields.retain(|field| field.key != "error_message");
        let mut wrong_type = complete.clone();
        wrong_type
            .fields
            .iter_mut()
            .find(|field| field.key == "error_message")
            .expect("message")
            .value = Value::Int(1);
        let duplicate = complete
            .clone()
            .with_field(KeyValue::new("error_message", Value::string("second")));
        let mut no_code = complete.clone();
        no_code.fields.retain(|field| field.key != "error_code");
        let mut wrong_state = complete.clone();
        wrong_state.kind = EventKind::WorkflowCompleted;
        for terminal in [
            missing,
            wrong_type,
            duplicate,
            no_code,
            wrong_state,
            event(EventKind::WorkflowFailed),
        ] {
            let mut diagnostic = opened();
            diagnostic.observe(&terminal);
            assert!(
                diagnostic.project(&failed()).is_none(),
                "unqualified terminal must not select raw content"
            );
        }
        let mut diagnostic = opened();
        diagnostic.observe(&complete);
        assert!(
            diagnostic
                .project(&RunSettlement::new(RunState::Failed, RunCause::TaskFailed))
                .is_none()
        );
    }

    #[test]
    fn publication_binds_identity_opening_and_unique_close() {
        let mut other = terminal();
        other.execution = Some(ExecutionId::new(Uuid::from_u128(1)));
        let mut diagnostic = opened();
        diagnostic.observe(&other);
        assert!(diagnostic.project(&failed()).is_none());
        let mut diagnostic = TerminalDiagnostic::new(execution());
        diagnostic.observe(&terminal());
        assert!(diagnostic.project(&failed()).is_none());
        let mut diagnostic = opened();
        diagnostic.observe(&event(EventKind::WorkflowStarted));
        diagnostic.observe(&terminal());
        assert!(diagnostic.project(&failed()).is_none());
        let mut diagnostic = opened();
        diagnostic.observe(&terminal());
        diagnostic.observe(&terminal());
        assert!(diagnostic.project(&failed()).is_none());
    }
}
