// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real settlement of a fan-out whose image metadata exceeds 1 MiB in aggregate:
//! every observation rides its own bounded frame and the terminal only counts them.
#![allow(clippy::unwrap_used, clippy::panic)]

use super::*;
use crate::agent_events::StampedAgentEvent;
use nika_kernel::ai::harness::HarnessImage;
use nika_types::resource::Value as Field;
use nika_verb_agent::AgentEvent;

fn field<'a>(event: &'a nika_event::Event, name: &str) -> &'a Field {
    &event.fields.iter().find(|f| f.key == name).unwrap().value
}

/// 256 iterations of 4 KiB reported paths (rows above 1 MiB in aggregate),
/// settling as a success or a failure; returns the run and its expected rows.
fn fan_out(failed: bool) -> (task::RanTask, Vec<serde_json::Value>) {
    let mut expected = Vec::new();
    let agent_events = (0..256_u32)
        .map(|iteration| {
            let mut image = HarnessImage::new(format!("image-{iteration}"));
            image.reported_saved_path = Some("x".repeat(4096));
            expected.push(serde_json::json!({
                "attempt": 1, "iteration": iteration, "image": image.observation()
            }));
            StampedAgentEvent {
                attempt: 1,
                iteration: Some(iteration),
                event: AgentEvent::HarnessImageObserved { image },
            }
        })
        .collect();
    let result = if failed {
        task::RunResult::Failed {
            error: TaskErrorRecord::new("NIKA-EXEC-001", "fixture", false),
            cost_usd: Some(0.125),
            cost_unpriced: None,
            access: None,
            access_refused: None,
        }
    } else {
        task::RunResult::Success {
            value: serde_json::json!(vec!["unchanged text"; 256]),
            tokens: None,
            recovered_from: None,
            warning: None,
            child: None,
            cost_usd: Some(0.125),
            cost_unpriced: None,
            model: None,
            access: None,
        }
    };
    let run = task::RanTask {
        note: "agent fan-out".into(),
        retries: Vec::new(),
        agent_events,
        decisions: Vec::new(),
        cleanup_declassified: Vec::new(),
        evidence: None,
        duration_ms: 1,
        items: None,
        usage: None,
        result,
    };
    (run, expected)
}

#[test]
fn fan_out_media_remains_lossless_without_an_oversized_terminal() {
    for failed in [false, true] {
        let (run, expected) = fan_out(failed);
        assert!(serde_json::to_string(&expected).unwrap().len() > 1024 * 1024);
        let value = serde_json::json!(vec!["unchanged text"; 256]);
        let mut ok = true;
        let mut sink = crate::VecSink::new();
        let record = settle_ran(
            "draw",
            run,
            None,
            &nika_cap::Integrity::trusted(),
            &[],
            None,
            &mut ok,
            &mut crate::DeterministicStamper::new(),
            &mut sink,
        );
        assert_eq!(
            record.harness_media, expected,
            "the embedder keeps every row"
        );
        if !failed {
            assert_eq!(record.output, value, "text output is unchanged");
        }
        assert_eq!(ok, !failed);
        let mut observed = Vec::new();
        for event in sink.events() {
            let line = serde_json::to_vec(event).unwrap().len();
            assert!(
                line < 64 * 1024,
                "every emitted frame stays far below the 1 MiB line"
            );
            if event.kind == EventKind::AgentImageObserved {
                let Field::String(raw) = field(event, "harness_image") else {
                    panic!("image metadata is JSON text");
                };
                let Field::Int(attempt) = field(event, "attempt") else {
                    panic!("attempt");
                };
                let Field::Int(iteration) = field(event, "iteration") else {
                    panic!("iteration");
                };
                let image: serde_json::Value = serde_json::from_str(raw).unwrap();
                assert!(image.get("data").is_none(), "no inline bytes");
                observed.push(serde_json::json!({
                    "attempt": attempt, "iteration": iteration, "image": image
                }));
            }
        }
        assert_eq!(
            observed, expected,
            "every row, in order, with its provenance"
        );
        let terminal = sink.events().last().unwrap();
        let kind = if failed {
            EventKind::TaskFailed
        } else {
            EventKind::TaskCompleted
        };
        assert_eq!(terminal.kind, kind);
        assert!(!terminal.fields.iter().any(|f| f.key == "harness_media"));
        assert_eq!(field(terminal, "harness_media_count"), &Field::Int(256));
        assert_eq!(field(terminal, "cost_usd"), &Field::Float(0.125));
    }
}
