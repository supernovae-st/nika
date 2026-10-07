// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Bounded image sample of a task. Every observation stays in its own journal
//! frame; the fold keeps a few rows, the exact total and the terminal's count.

use super::{BTreeMap, Event, EventKind, int_field, str_field};

/// Rows a task detail shows; the trace keeps the rest.
const SAMPLE: usize = 4;
/// A single observation frame larger than this is malformed evidence.
const FRAME_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Default)]
pub(super) struct Images {
    rows: Vec<serde_json::Value>,
    observed: u64,
    expected: Option<u64>,
    malformed: bool,
    /// The view's JSON: `images` (sample) · `observed` · `expected` · `complete`.
    pub(super) json: String,
}

impl Images {
    fn refresh(&mut self) {
        let complete = !self.malformed && self.expected == Some(self.observed);
        self.json = serde_json::json!({
            "images": self.rows,
            "observed": self.observed,
            "expected": self.expected,
            "complete": complete,
        })
        .to_string();
    }
}

/// Fold one event. A new attempt or cache hit forgets the leg's media; a
/// terminal without `harness_media_count` after frames leaves it incomplete.
pub(super) fn apply(all: &mut BTreeMap<String, Images>, event: &Event) {
    if event.kind == EventKind::WorkflowStarted {
        all.clear();
        return;
    }
    let Some(task) = str_field(event, "task") else {
        return;
    };
    match event.kind {
        EventKind::TaskStarted | EventKind::TaskCacheHit => {
            all.remove(task);
        }
        EventKind::AgentImageObserved => {
            let images = all.entry(task.to_owned()).or_default();
            images.observed = images.observed.saturating_add(1);
            let image = str_field(event, "harness_image")
                .filter(|raw| raw.len() <= FRAME_BYTES)
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                .filter(serde_json::Value::is_object);
            match image {
                Some(image) if images.rows.len() < SAMPLE => images.rows.push(serde_json::json!({
                    "image": image,
                    "attempt": int_field(event, "attempt"),
                    "iteration": int_field(event, "iteration"),
                })),
                Some(_) => {}
                None => images.malformed = true,
            }
            images.refresh();
        }
        EventKind::TaskCompleted | EventKind::TaskFailed | EventKind::TaskSkipped => {
            let expected =
                int_field(event, "harness_media_count").and_then(|n| u64::try_from(n).ok());
            if expected.is_none() && !all.contains_key(task) {
                return;
            }
            let images = all.entry(task.to_owned()).or_default();
            images.expected = expected;
            images.refresh();
        }
        _ => {}
    }
}
