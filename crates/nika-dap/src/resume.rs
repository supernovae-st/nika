// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika run --resume <trace>` — the fold (ADR-099). Descended from
//! `nika-cli`'s run verb 2026-07-22 (compute descends, render stays):
//! a READER of the run journal beside [`crate::recover`] — the same
//! NDJSON bytes, one home.
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
use nika_runtime::approval::{ApprovalTicket, PausedApproval};
use nika_runtime::resume::{PriorSuccess, ResumePlan, fields, referenced_upstreams};
use nika_schema::raw::RawWorkflow;
use nika_types::resource::Value as FieldValue;

/// The `--resume` request as parsed from the CLI surface.
#[derive(Debug, Clone)]
pub struct ResumeRequest {
    /// The prior run's NDJSON trace path — `None` on the answers-only
    /// form (F4 · 2026-07-30): `--answer` WITHOUT `--resume` pre-seeds the
    /// gates of a FRESH run (the answer waits in the map until the gate
    /// asks — the CI one-pass gate the rider always meant to serve);
    /// `Some` folds the skip plan + the paused ticket + the version
    /// judgment as before.
    pub trace: Option<PathBuf>,
    /// `--from <task_id>` — force this task + its transitive downstream
    /// to re-run even on an identity match (ADR-099 §3).
    pub from: Option<String>,
    /// `--answer TASK=VALUE` (repeatable · ADR-099 rider) — binds a
    /// paused prompt's answer at resume; the value parses as JSON when
    /// it parses, else rides as a string (the `--var` convention).
    pub answers: Vec<String>,
    /// `--resume-compat <VERSION>` (F-P21 · NEP-0014 law 4) — the
    /// DECLARED cross-version compatibility: the operator attests the
    /// trace recorded under engine `<VERSION>` may resume under this
    /// one. The token must name the trace's recorded version EXACTLY
    /// (`unrecorded` for a pre-versioning journal) — a blanket force is
    /// precisely the silent degradation the law retires.
    pub compat: Option<String>,
}

/// The compat token a pre-versioning trace names (`--resume-compat
/// unrecorded` declares compat with a journal that carries no
/// `engine_version` — said in the refusal's teaching line).
pub const UNRECORDED_VERSION: &str = "unrecorded";

/// The cross-version judgment (F-P21 · NEP-0014 law 4) — a resume under
/// an engine different from the recording one is judged, never assumed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResumeVersion {
    /// The trace was recorded under THIS engine — the resume is exact.
    Exact,
    /// The trace names a DIFFERENT engine — the refusal names both
    /// versions, or a declared compat (the exact recorded string)
    /// attests the crossing.
    Mismatch {
        /// The version the trace was recorded under.
        recorded: String,
        /// This engine's version.
        current: String,
    },
    /// The trace carries no `engine_version` (a pre-A5 journal) — same
    /// law, the `unrecorded` token.
    Unrecorded {
        /// This engine's version.
        current: String,
    },
}

/// The trace's recorded engine version — the `workflow_started` boot
/// manifest's `engine_version` field (`None` on a pre-A5 journal).
#[must_use]
pub fn trace_engine_version(events: &[Event]) -> Option<String> {
    let started = events
        .iter()
        .find(|e| matches!(e.kind, EventKind::WorkflowStarted))?;
    str_field(started, "engine_version").map(str::to_owned)
}

/// Judge a resume's version crossing (F-P21): the trace's recorded
/// engine against this one. The compat declaration matches ONLY the
/// exact recorded string (or [`UNRECORDED_VERSION`]) — anything else
/// stays a mismatch (the declaration is exact, never a blanket force).
#[must_use]
pub fn judge_version(events: &[Event], current: &str) -> ResumeVersion {
    match trace_engine_version(events) {
        Some(recorded) if recorded == current => ResumeVersion::Exact,
        Some(recorded) => ResumeVersion::Mismatch {
            recorded,
            current: current.to_owned(),
        },
        None => ResumeVersion::Unrecorded {
            current: current.to_owned(),
        },
    }
}

/// Whether the declared compat token discharges a judgment (the exact
/// recorded string — or `unrecorded` for a versionless trace).
#[must_use]
pub fn compat_discharges(judgment: &ResumeVersion, compat: Option<&str>) -> bool {
    match (judgment, compat) {
        (ResumeVersion::Exact, _) => true,
        (ResumeVersion::Mismatch { recorded, .. }, Some(declared)) => declared == recorded,
        (ResumeVersion::Unrecorded { .. }, Some(declared)) => declared == UNRECORDED_VERSION,
        _ => false,
    }
}

/// The resume's version verdict (F-P21) — the caller maps Proceed to
/// the fold and Refuse to its exit class (the message names the
/// versions and the teaching, one voice).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompatVerdict {
    /// The resume proceeds — `Some(recorded)` when a DECLARED compat
    /// rides (attested on the run's boot manifest), `None` on an exact
    /// match (no crossing, no claim).
    Proceed {
        /// The recorded version the compat declaration crossed.
        compat_with: Option<String>,
    },
    /// The resume refuses — the named message (both versions · the
    /// exact `--resume-compat` teaching).
    Refuse(String),
}

/// Judge a resume end to end (F-P21): the version judgment × the
/// declared compat token. The declaration matches ONLY the exact
/// recorded string (or [`UNRECORDED_VERSION`]) — a wrong token is its
/// own named refusal, never a blanket force.
#[must_use]
pub fn judge_resume(judgment: &ResumeVersion, compat: Option<&str>) -> CompatVerdict {
    match judgment {
        ResumeVersion::Exact => CompatVerdict::Proceed { compat_with: None },
        ResumeVersion::Mismatch { recorded, current } => match compat {
            Some(declared) if declared == recorded => CompatVerdict::Proceed {
                compat_with: Some(recorded.clone()),
            },
            Some(declared) => CompatVerdict::Refuse(format!(
                "--resume-compat {declared} names the wrong version — the trace was \
                 recorded under engine {recorded}"
            )),
            None => CompatVerdict::Refuse(format!(
                "the trace was recorded under engine {recorded} · this engine is \
                 {current} — a cross-version resume is judged, never assumed (F-P21): \
                 re-run live, or declare the compat with `--resume-compat {recorded}`"
            )),
        },
        ResumeVersion::Unrecorded { current } => match compat {
            Some(declared) if declared == UNRECORDED_VERSION => CompatVerdict::Proceed {
                compat_with: Some(UNRECORDED_VERSION.to_owned()),
            },
            Some(declared) => CompatVerdict::Refuse(format!(
                "--resume-compat {declared} names a version, but the trace records \
                 none — declare `--resume-compat unrecorded`"
            )),
            None => CompatVerdict::Refuse(format!(
                "the trace records no engine version (a pre-versioning journal) · \
                 this engine is {current} — a cross-version resume is judged, never \
                 assumed (F-P21): re-run live, or declare `--resume-compat unrecorded`"
            )),
        },
    }
}

/// Parse + validate the repeatable `--answer TASK=VALUE` pairs: the task
/// must exist AND be a direct `invoke: nika:prompt` (an answer aimed at
/// anything else would silently do nothing — refused instead, the same
/// posture as an unknown `--var` key).
///
/// # Errors
///
/// A human-readable refusal (environment class).
pub fn parse_answers(
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
#[non_exhaustive]
pub struct PlanFold {
    /// task id → its journaled success identity (last record wins).
    pub plan: ResumePlan,
    /// Successes journaled WITHOUT a resume identity (an older engine ·
    /// a non-eligible task) — drives the honest-degradation notice.
    pub keyless: usize,
    /// Records whose `output` field would not parse back — treated as
    /// keyless (never a guess · ADR-099: an unreadable record re-runs).
    pub unreadable: usize,
    /// The F-P4 resume authority (NEP-0013): the approval ticket the
    /// LAST `workflow_paused` frame serialized (`approval_*` fields)
    /// plus the trace's own run identity (its `workflow_started` event
    /// id — the cross-run check lives in the trace's bytes). `None`
    /// when the trace carries no pause, or a pre-F-P4 one (no shown
    /// hash to bind against — the `--answer` then binds unvalidated,
    /// the ADR-099 contract unchanged).
    pub paused: Option<PausedApproval>,
}

/// Fold a trace's events into the skip plan. BOTH `task_completed` and
/// `task_cache_hit` records count (a resumed run's own trace is itself
/// resumable); the LAST record per task wins (one terminal per run —
/// last-wins also covers a file carrying run + resume appended). The
/// F-P4 paused ticket folds with the same last-wins law.
#[must_use]
pub fn fold_plan(events: &[Event]) -> PlanFold {
    let mut fold = PlanFold {
        plan: ResumePlan::new(),
        keyless: 0,
        unreadable: 0,
        paused: fold_paused_approval(events),
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

/// The F-P4 fold: read the LAST `workflow_paused` frame's `approval_*`
/// fields back into the ticket, and the trace's run identity from its
/// own `workflow_started` frame. Any absent or malformed piece yields
/// `None` — never a guess (an unreadable ticket simply cannot validate,
/// so the resume falls back to the unvalidated ADR-099 binding).
fn fold_paused_approval(events: &[Event]) -> Option<PausedApproval> {
    let paused = events
        .iter()
        .rev()
        .find(|e| e.kind == EventKind::WorkflowPaused)?;
    let task = str_field(paused, "task")?;
    let shown_hash = str_field(paused, "approval_shown_hash")?;
    let nonce = str_field(paused, "approval_nonce")?;
    let minted_at_ms = int_field(paused, "approval_minted_at_ms")?;
    let ttl_seconds = u32::try_from(int_field(paused, "approval_ttl_seconds")?).ok()?;
    let trace_nonce = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .map(|e| e.id.uuid.to_string())?;
    let ticket = ApprovalTicket::new(
        shown_hash.to_owned(),
        nonce.to_owned(),
        task.to_owned(),
        minted_at_ms,
        ttl_seconds,
    );
    Some(PausedApproval::new(ticket, trace_nonce))
}

/// Apply `--from <task_id>` (ADR-099 §3): remove the named task AND its
/// transitive downstream from the plan so they re-run even on an
/// identity match. An unknown id is refused pre-run (the same contract
/// as an unknown `--var` key).
///
/// # Errors
///
/// A human-readable refusal naming the declared task ids.
pub fn apply_from(plan: &mut ResumePlan, wf: &RawWorkflow, from: &str) -> Result<(), String> {
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
#[must_use]
pub fn summary_line(cache_hits: usize, ran_live: usize) -> String {
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

/// The int twin of [`str_field`] (the F-P4 `approval_*` mint/TTL fields).
fn int_field(event: &Event, key: &str) -> Option<i64> {
    event.fields.iter().find(|kv| kv.key == key).and_then(|kv| {
        if let FieldValue::Int(v) = &kv.value {
            Some(*v)
        } else {
            None
        }
    })
}

#[cfg(test)]
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
        const WF: &str = "nika: v1\nworkflow:\n  id: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n  b:\n    after:\n      a: success\n    exec: { command: [\"true\"] }\n  c:\n    with:\n      prev: ${{ tasks.b.output }}\n    exec: { command: [\"echo\", \"${{ with.prev }}\"] }\n  solo:\n    exec: { command: [\"true\"] }\n";
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

    // ── F-P21 · the cross-version judgment (NEP-0014 law 4) ───────────

    /// A `workflow_started` boot manifest, with or without the
    /// `engine_version` field (a pre-A5 journal carries none).
    fn started(version: Option<&str>) -> Event {
        let mut e = Event::new(
            EventId::new(Uuid::nil()),
            Timestamp::from_unix_ms(0),
            EventKind::WorkflowStarted,
        );
        if let Some(v) = version {
            e = e.with_field(KeyValue::new(
                "engine_version",
                FieldValue::String(v.to_owned()),
            ));
        }
        e
    }

    #[test]
    fn the_version_judgment_discriminates_exact_mismatch_and_unrecorded() {
        // Exact — the trace's engine IS this one.
        let events = vec![started(Some("0.106.0"))];
        assert_eq!(judge_version(&events, "0.106.0"), ResumeVersion::Exact);
        assert_eq!(trace_engine_version(&events).as_deref(), Some("0.106.0"));
        // Mismatch — both versions named.
        assert_eq!(
            judge_version(&events, "0.107.0"),
            ResumeVersion::Mismatch {
                recorded: "0.106.0".to_owned(),
                current: "0.107.0".to_owned(),
            }
        );
        // Unrecorded — a pre-versioning journal (no field · no started).
        assert_eq!(
            judge_version(&[started(None)], "0.107.0"),
            ResumeVersion::Unrecorded {
                current: "0.107.0".to_owned(),
            }
        );
        assert_eq!(
            judge_version(&[], "0.107.0"),
            ResumeVersion::Unrecorded {
                current: "0.107.0".to_owned(),
            }
        );
    }

    #[test]
    fn an_exact_resume_proceeds_without_a_claim() {
        let verdict = judge_resume(&ResumeVersion::Exact, None);
        assert_eq!(verdict, CompatVerdict::Proceed { compat_with: None });
        // A compat token on an EXACT match is harmless (nothing crossed).
        let verdict = judge_resume(&ResumeVersion::Exact, Some("0.106.0"));
        assert_eq!(verdict, CompatVerdict::Proceed { compat_with: None });
    }

    #[test]
    fn a_mismatch_refuses_naming_both_versions_and_the_teaching() {
        let judgment = ResumeVersion::Mismatch {
            recorded: "0.105.0".to_owned(),
            current: "0.106.1".to_owned(),
        };
        let CompatVerdict::Refuse(message) = judge_resume(&judgment, None) else {
            panic!("no compat = a refusal");
        };
        assert!(message.contains("0.105.0"), "{message}");
        assert!(message.contains("0.106.1"), "{message}");
        assert!(
            message.contains("--resume-compat 0.105.0"),
            "the teaching names the exact token: {message}"
        );
    }

    #[test]
    fn a_declared_compat_proceeds_attested_but_a_wrong_token_refuses() {
        let judgment = ResumeVersion::Mismatch {
            recorded: "0.105.0".to_owned(),
            current: "0.106.1".to_owned(),
        };
        // The exact recorded string discharges — attested for the journal.
        assert_eq!(
            judge_resume(&judgment, Some("0.105.0")),
            CompatVerdict::Proceed {
                compat_with: Some("0.105.0".to_owned()),
            }
        );
        // A wrong token is its own named refusal (never a blanket force).
        let CompatVerdict::Refuse(message) = judge_resume(&judgment, Some("0.104.0")) else {
            panic!("a wrong token refuses");
        };
        assert!(
            message.contains("0.104.0") && message.contains("0.105.0"),
            "{message}"
        );
    }

    #[test]
    fn an_unrecorded_trace_rides_the_unrecorded_token_only() {
        let judgment = ResumeVersion::Unrecorded {
            current: "0.106.1".to_owned(),
        };
        // No compat → refusal teaching the `unrecorded` token.
        let CompatVerdict::Refuse(message) = judge_resume(&judgment, None) else {
            panic!("no compat = a refusal");
        };
        assert!(message.contains("--resume-compat unrecorded"), "{message}");
        // The token discharges; a version string never covers the void.
        assert_eq!(
            judge_resume(&judgment, Some(UNRECORDED_VERSION)),
            CompatVerdict::Proceed {
                compat_with: Some(UNRECORDED_VERSION.to_owned()),
            }
        );
        let CompatVerdict::Refuse(message) = judge_resume(&judgment, Some("0.95.0")) else {
            panic!("a version token never covers a versionless trace");
        };
        assert!(message.contains("records none"), "{message}");
    }
}
