// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika run --resume <trace>` — the fold (ADR-099).
//!
//! The trace is the run's own NDJSON journal (`nika run --json > t.ndjson`)
//! — no second store, no daemon: `--resume` is a READER of a file. The
//! fold collects every journaled success that carries a resume identity
//! (`def_hash` + `input_hash` + `output` · additive `task_completed` /
//! `task_cache_hit` fields), the runtime recomputes each task's identity
//! live and skips iff BOTH hashes match (§1). `--from <task_id>` removes
//! the named task and its transitive downstream from the plan (§3).
//!
//! **Honest degradation** (ADR-099 Consequences): a trace with no keys
//! (recorded by an older engine) resumes NOTHING — a notice, never an
//! error. A truncated tail (the crash signature) keeps its valid prefix.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use nika_event::{Event, EventKind};
use nika_runtime::resume::{PriorSuccess, ResumePlan, fields, referenced_upstreams};
use nika_schema::raw::RawWorkflow;
use nika_types::resource::Value as FieldValue;

/// The `--resume` request as parsed from the CLI surface.
#[derive(Debug, Clone)]
pub struct ResumeRequest {
    /// The prior run's NDJSON trace path.
    pub trace: PathBuf,
    /// `--from <task_id>` — force this task + its transitive downstream
    /// to re-run even on an identity match (ADR-099 §3).
    pub from: Option<String>,
    /// `--answer TASK=VALUE` (repeatable · ADR-099 rider) — binds a
    /// paused prompt's answer at resume; the value parses as JSON when
    /// it parses, else rides as a string (the `--var` convention).
    pub answers: Vec<String>,
}

/// Parse + validate the repeatable `--answer TASK=VALUE` pairs: the task
/// must exist AND be a direct `invoke: nika:prompt` (an answer aimed at
/// anything else would silently do nothing — refused instead, the same
/// posture as an unknown `--var` key).
///
/// # Errors
///
/// A human-readable refusal (environment class).
pub(crate) fn parse_answers(
    pairs: &[String],
    wf: &RawWorkflow,
) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let mut answers = BTreeMap::new();
    for pair in pairs {
        let (task_id, raw) = match pair.split_once('=') {
            Some((t, v)) if !t.trim().is_empty() => (t.trim(), v),
            _ => return Err(format!("--answer expects TASK=VALUE, got `{pair}`")),
        };
        let Some(task) = wf.tasks.iter().find(|t| t.value.id.value == task_id) else {
            let ids: Vec<&str> = wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect();
            return Err(format!(
                "--answer {task_id}: unknown task — the workflow declares: {}",
                ids.join(" · ")
            ));
        };
        let is_prompt = matches!(
            &task.value.action,
            nika_schema::raw::RawAction::Invoke(invoke)
                if invoke.tool().map(|t| t.value.as_str()) == Some("nika:prompt")
        );
        if !is_prompt {
            return Err(format!(
                "--answer {task_id}: not a `nika:prompt` task — an answer would do nothing"
            ));
        }
        let value =
            serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_owned()));
        answers.insert(task_id.to_owned(), value);
    }
    Ok(answers)
}

/// The fold's yield — the skip plan plus the honesty counters the
/// notices are built from.
pub(crate) struct PlanFold {
    /// task id → its journaled success identity (last record wins).
    pub plan: ResumePlan,
    /// Successes journaled WITHOUT a resume identity (an older engine ·
    /// a non-eligible task) — drives the honest-degradation notice.
    pub keyless: usize,
    /// Records whose `output` field would not parse back — treated as
    /// keyless (never a guess · ADR-099: an unreadable record re-runs).
    pub unreadable: usize,
}

/// Fold a trace's events into the skip plan. BOTH `task_completed` and
/// `task_cache_hit` records count (a resumed run's own trace is itself
/// resumable); the LAST record per task wins (one terminal per run —
/// last-wins also covers a file carrying run + resume appended).
pub(crate) fn fold_plan(events: &[Event]) -> PlanFold {
    let mut fold = PlanFold {
        plan: ResumePlan::new(),
        keyless: 0,
        unreadable: 0,
    };
    for event in events {
        if !matches!(
            event.kind,
            EventKind::TaskCompleted | EventKind::TaskCacheHit
        ) {
            continue;
        }
        let Some(task) = str_field(event, "task") else {
            continue;
        };
        let (Some(def), Some(input)) = (
            str_field(event, fields::DEF_HASH),
            str_field(event, fields::INPUT_HASH),
        ) else {
            fold.keyless += 1;
            continue;
        };
        let Some(output) =
            str_field(event, fields::OUTPUT).and_then(|t| serde_json::from_str(t).ok())
        else {
            fold.unreadable += 1;
            continue;
        };
        fold.plan.insert(
            task.to_owned(),
            PriorSuccess::new(def.to_owned(), input.to_owned(), output),
        );
    }
    fold
}

/// Apply `--from <task_id>` (ADR-099 §3): remove the named task AND its
/// transitive downstream from the plan so they re-run even on an
/// identity match. An unknown id is refused pre-run (the same contract
/// as an unknown `--var` key).
///
/// # Errors
///
/// A human-readable refusal naming the declared task ids.
pub(crate) fn apply_from(
    plan: &mut ResumePlan,
    wf: &RawWorkflow,
    from: &str,
) -> Result<(), String> {
    let ids: Vec<&str> = wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect();
    if !ids.contains(&from) {
        return Err(format!(
            "--from {from}: unknown task — the workflow declares: {}",
            ids.join(" · ")
        ));
    }
    // downstream edges: task → the tasks that observe it (`after:` ∪
    // `tasks.<id>` template references · over-collection is safe).
    let mut downstream: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for task in &wf.tasks {
        for upstream in referenced_upstreams(&task.value) {
            downstream
                .entry(upstream)
                .or_default()
                .push(task.value.id.value.clone());
        }
    }
    let mut forced: BTreeSet<String> = BTreeSet::new();
    let mut queue = vec![from.to_owned()];
    while let Some(id) = queue.pop() {
        if !forced.insert(id.clone()) {
            continue;
        }
        if let Some(consumers) = downstream.get(&id) {
            queue.extend(consumers.iter().cloned());
        }
    }
    for id in &forced {
        plan.remove(id);
    }
    Ok(())
}

/// The post-run summary line (`--resume` only): what skipped, what ran.
pub(crate) fn summary_line(cache_hits: usize, ran_live: usize) -> String {
    format!("resumed · {cache_hits} skipped (cache hit) · {ran_live} ran live")
}

fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event.fields.iter().find(|kv| kv.key == key).and_then(|kv| {
        if let FieldValue::String(s) = &kv.value {
            Some(s.as_str())
        } else {
            None
        }
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use nika_types::id::EventId;
    use nika_types::resource::KeyValue;
    use nika_types::timestamp::Timestamp;
    use uuid::Uuid;

    fn event(kind: EventKind, task: &str, keyed: bool) -> Event {
        let mut e = Event::new(EventId::new(Uuid::nil()), Timestamp::from_unix_ms(0), kind)
            .with_field(KeyValue::new("task", FieldValue::String(task.to_owned())));
        if keyed {
            e = e
                .with_field(KeyValue::new(
                    fields::DEF_HASH,
                    FieldValue::String("d".repeat(64)),
                ))
                .with_field(KeyValue::new(
                    fields::INPUT_HASH,
                    FieldValue::String("i".repeat(64)),
                ))
                .with_field(KeyValue::new(
                    fields::OUTPUT,
                    FieldValue::String("\"out\"".to_owned()),
                ));
        }
        e
    }

    #[test]
    fn fold_collects_keyed_successes_and_counts_keyless() {
        let events = vec![
            event(EventKind::TaskCompleted, "a", true),
            event(EventKind::TaskCompleted, "old", false), // pre-ADR-099 record
            event(EventKind::TaskCacheHit, "b", true),     // a resumed trace resumes
            event(EventKind::TaskFailed, "c", false),      // failures never skip
        ];
        let fold = fold_plan(&events);
        assert_eq!(fold.plan.len(), 2);
        assert!(fold.plan.contains_key("a") && fold.plan.contains_key("b"));
        assert_eq!(fold.keyless, 1, "the old record is counted, not an error");
        assert_eq!(fold.unreadable, 0);
    }

    #[test]
    fn fold_treats_unparseable_output_as_not_skippable() {
        let broken = event(EventKind::TaskCompleted, "a", true).with_fields(vec![
            KeyValue::new("task", FieldValue::String("a".to_owned())),
            KeyValue::new(fields::DEF_HASH, FieldValue::String("d".repeat(64))),
            KeyValue::new(fields::INPUT_HASH, FieldValue::String("i".repeat(64))),
            KeyValue::new(fields::OUTPUT, FieldValue::String("{not json".to_owned())),
        ]);
        let fold = fold_plan(&[broken]);
        assert!(
            fold.plan.is_empty(),
            "an unreadable record re-runs, never guesses"
        );
        assert_eq!(fold.unreadable, 1);
    }

    #[test]
    fn from_removes_the_task_and_its_transitive_downstream() {
        const WF: &str = "nika: v1\nworkflow:\n  id: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n  b:\n    after:\n      a: succeeded\n    exec: { command: [\"true\"] }\n  c:\n    with:\n      prev: ${{ tasks.b.output }}\n    exec: { command: [\"echo\", \"${{ with.prev }}\"] }\n  solo:\n    exec: { command: [\"true\"] }\n";
        let wf = nika_schema::parse(
            WF,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses");
        let mut plan = ResumePlan::new();
        for id in ["a", "b", "c", "solo"] {
            plan.insert(
                id.to_owned(),
                PriorSuccess::new("d".repeat(64), "i".repeat(64), serde_json::json!(null)),
            );
        }
        apply_from(&mut plan, &wf, "a").expect("known id");
        // a forced · b (after a) · c (with: binding on b) — solo stays.
        assert!(!plan.contains_key("a"));
        assert!(!plan.contains_key("b"), "control edge walked");
        assert!(!plan.contains_key("c"), "binding edge walked transitively");
        assert!(
            plan.contains_key("solo"),
            "an untouched sibling still skips"
        );

        let err = apply_from(&mut plan, &wf, "ghost").expect_err("unknown id refused");
        assert!(err.contains("ghost") && err.contains("solo"), "{err}");
    }

    #[test]
    fn summary_line_names_both_sides() {
        assert_eq!(
            summary_line(3, 2),
            "resumed · 3 skipped (cache hit) · 2 ran live"
        );
    }

    #[test]
    fn answers_bind_only_known_prompt_tasks() {
        const WF: &str = "nika: v1\nworkflow:\n  id: t\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"go?\" }\n  build:\n    exec: { command: [\"true\"] }\n";
        let wf = nika_schema::parse(
            WF,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses");
        // JSON-if-parses: a confirm answer lands as a real boolean.
        let answers = parse_answers(&["ask=true".to_owned()], &wf).expect("valid");
        assert_eq!(answers["ask"], serde_json::json!(true));
        // Unknown task → refused with the declared set.
        let err = parse_answers(&["ghost=1".to_owned()], &wf).expect_err("unknown");
        assert!(err.contains("ghost") && err.contains("ask"), "{err}");
        // A non-prompt task → refused (the answer would do nothing).
        let err = parse_answers(&["build=1".to_owned()], &wf).expect_err("not a prompt");
        assert!(err.contains("not a `nika:prompt`"), "{err}");
        // Shape errors are loud.
        assert!(parse_answers(&["noequals".to_owned()], &wf).is_err());
    }
}
