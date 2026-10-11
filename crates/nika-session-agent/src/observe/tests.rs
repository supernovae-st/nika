// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::json;

use super::*;

/// Tools that answer `read`, fail `compose` naming a key, and fail anything else.
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
            "compose" => ToolReply::error("compose refused:\nkey sk-live-ABCDEFGH12345678 expired"),
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

/// A failed call's step says why, as a host may show it: its reply's excerpt, the secret shapes
/// redacted, on one line. A started or a finished step carries none.
#[test]
fn a_failed_step_carries_its_reply_excerpt_never_a_secret_shape() {
    let (observed, steps) = watched();
    observed.call(ToolCall::new("compose", json!({"block": "feeds"})).with_meta("toolu_c"));
    observed.call(ToolCall::new("read", json!({})).with_meta("toolu_r"));
    let steps = steps.lock().unwrap().clone();
    let said: Vec<_> = (steps.iter())
        .map(|s| (s.call.as_str(), s.state, s.reply.as_deref()))
        .collect();
    assert!(
        said == [
            ("toolu_c", StepState::Started, None),
            (
                "toolu_c",
                StepState::Failed,
                Some("compose refused: key [redacted] expired")
            ),
            ("toolu_r", StepState::Started, None),
            ("toolu_r", StepState::Finished, None),
        ],
        "each step as observed, the failed one's reply redacted"
    );
    assert!(!format!("{steps:?}").contains("ABCDEFGH12345678"));
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

/// A step says when its call began, the same at its start and its end, and, once answered, the
/// size of the reply, never the reply.
#[test]
fn a_step_says_when_its_call_began_and_how_large_its_reply_was() {
    let (observed, steps) = watched();
    observed.call(ToolCall::new("read", json!({"path": "a.nika"})).with_meta("toolu_9"));
    let steps = steps.lock().unwrap().clone();
    assert_eq!(steps[0].at_ms, steps[1].at_ms);
    assert!(steps[0].at_ms > 0);
    assert_eq!(steps[0].reply_bytes, None);
    let size = u64::try_from("the file's secret contents".len()).unwrap();
    assert_eq!(steps[1].reply_bytes, Some(size));
}
