// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::json;

use super::*;

/// Tools that answer `read` and fail anything else.
struct Tools;

impl SessionTools for Tools {
    fn tools(&self) -> Vec<ToolDef> {
        vec![ToolDef::new(
            "read",
            "Read a file.",
            json!({"type": "object"}),
            true,
        )]
    }

    fn call(&self, call: ToolCall) -> ToolReply {
        match call.name.as_str() {
            "read" => ToolReply::ok("the file's secret contents"),
            other => ToolReply::error(format!("no tool `{other}`")),
        }
    }
}

fn watched() -> (Observed, Arc<Mutex<Vec<ToolStep>>>) {
    let observed = Observed::new(Arc::new(Tools));
    let steps = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&steps);
    observed.watch(Some(Arc::new(move |step: &ToolStep| {
        kept.lock().unwrap().push(step.clone());
    })));
    (observed, steps)
}

#[test]
fn a_call_reports_its_start_and_its_end_with_the_time_it_took() {
    let (observed, steps) = watched();
    let asked = ToolCall::new("read", json!({"path": "secrets.env"})).with_meta("toolu_1");
    let reply = observed.call(asked);
    assert_eq!(reply.text, "the file's secret contents");
    let steps = steps.lock().unwrap().clone();
    assert_eq!(steps.len(), 2);
    assert_eq!(
        (
            steps[0].call.as_str(),
            steps[0].name.as_str(),
            steps[0].state,
            steps[0].elapsed_ms
        ),
        ("toolu_1", "read", StepState::Started, None)
    );
    assert_eq!(
        (steps[1].call.as_str(), steps[1].state),
        ("toolu_1", StepState::Finished)
    );
    assert!(steps[1].elapsed_ms.is_some());
    let shown = format!("{steps:?}");
    assert!(
        !shown.contains("secrets.env") && !shown.contains("secret contents"),
        "a step carries neither the arguments nor the reply: {shown}"
    );
}

#[test]
fn a_failed_reply_is_a_failed_step_and_an_unnamed_call_gets_a_name() {
    let (observed, steps) = watched();
    let reply = observed.call(ToolCall::new("delete_everything", json!({})));
    assert!(reply.is_error);
    let steps = steps.lock().unwrap().clone();
    assert_eq!(
        steps
            .iter()
            .map(|s| (s.call.as_str(), s.state))
            .collect::<Vec<_>>(),
        [
            ("step-1", StepState::Started),
            ("step-1", StepState::Failed)
        ]
    );
}

#[test]
fn between_turns_no_sink_watches_and_the_call_still_answers() {
    let (observed, steps) = watched();
    observed.watch(None);
    let reply = observed.call(ToolCall::new("read", json!({})));
    assert!(!reply.is_error);
    assert!(steps.lock().unwrap().is_empty());
    assert_eq!(observed.tools().len(), 1);
}
