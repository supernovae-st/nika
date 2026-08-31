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
    /// `--resume-unverified` (ADR-099 trust amendment · 2026-08-08) —
    /// the NAMED opt-out of the chain precondition: a trace whose
    /// tamper-evidence chain fails [`judge_trust`] is trusted anyway,
    /// the walk's finding journaled on the run's boot manifest
    /// (`resume_unverified: declared`). Never a silent default — and
    /// consumed only when the walk objects: a verified trace journals
    /// no claim.
    pub allow_unverified: bool,
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

/// The trace's recorded `--model` override — the `workflow_started` boot
/// manifest's `model_override` field. `None` when the run carried no
/// override, or on an older journal (absent is honest either way — no
/// claim, never a guess).
#[must_use]
pub fn trace_model_override(events: &[Event]) -> Option<String> {
    let started = events
        .iter()
        .find(|e| matches!(e.kind, EventKind::WorkflowStarted))?;
    str_field(started, "model_override").map(str::to_owned)
}

/// The resume's model verdict (issue 772) — a run recorded under a
/// `--model` override must never SILENTLY resume on the envelope model
/// (the mock-previewed run that resumes onto a priced seat).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModelVerdict {
    /// The resume proceeds — `changed` names `(recorded, declared)` when
    /// an explicit flag moves the seat (noticed on stderr, never silent).
    Proceed {
        /// `Some((recorded, declared))` when the resume's explicit
        /// `--model` differs from the recorded override.
        changed: Option<(String, String)>,
    },
    /// The resume refuses — a flag-less resume of an override run (the
    /// message names the recorded seat + the exact teaching).
    Refuse(String),
}

/// Judge a resume's model carry (issue 772): the trace's recorded
/// override against the live `--model` flag. Explicit argv always wins —
/// what dies is the silent fallback to the envelope model.
#[must_use]
pub fn judge_model(recorded: Option<&str>, declared: Option<&str>) -> ModelVerdict {
    match (recorded, declared) {
        // No recorded override (no claim, or an older journal) — nothing
        // to carry, today's behavior stands.
        (None, _) => ModelVerdict::Proceed { changed: None },
        (Some(rec), Some(dec)) if rec == dec => ModelVerdict::Proceed { changed: None },
        (Some(rec), Some(dec)) => ModelVerdict::Proceed {
            changed: Some((rec.to_owned(), dec.to_owned())),
        },
        (Some(rec), None) => ModelVerdict::Refuse(format!(
            "the trace was recorded under --model {rec} · a flag-less resume would run \
             the workflow's envelope model instead — the model is judged, never assumed: \
             resume with `--model {rec}` to keep the recorded seat, or name the change \
             explicitly"
        )),
    }
}

/// Whether the CURRENT source differs IN CONTENT from the bytes the
/// recorded run executed — `None` when the trace predates
/// `workflow_sha256` (no claim, never a guess).
///
/// Content, not bytes: an editor re-encoding CRLF↔LF or adding a BOM
/// moved nothing an author would call a change, and the 0.96.0 review
/// proved a raw compare cries wolf on exactly that. Raw match first,
/// then the LF normal forms (against the recorded raw for LF-recorded
/// files, against `workflow_sha256_lf` for CRLF-recorded ones) — only a
/// real content change survives all three.
///
/// ONE comparator, two callers: the replay session's #210 identity
/// check (a drifted file moves breakpoint lines) and the resume path,
/// which owes the operator a word when the file changed under a paused
/// run — the envelope `model:` can be edited between the pause and the
/// resume, and the seat swaps with it (measured 2026-08-03).
#[must_use]
pub fn source_drifted(yaml: &str, events: &[Event]) -> Option<bool> {
    let started = events
        .iter()
        .find(|e| matches!(e.kind, EventKind::WorkflowStarted))?;
    let recorded = str_field(started, "workflow_sha256")?;
    if nika_event::source_id::sha256_hex(yaml.as_bytes()) == recorded {
        return Some(false);
    }
    let lf_sha =
        nika_event::source_id::sha256_hex(nika_event::source_id::lf_normal_form(yaml).as_bytes());
    if lf_sha == recorded || str_field(started, "workflow_sha256_lf") == Some(lf_sha.as_str()) {
        return Some(false);
    }
    Some(true)
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
        // P3 B5 · an `agent:` task is answerable too: on a harness seat
        // its out-of-grants ask pauses the run (NIKA-1806) and this
        // answer is the operator's bound verdict.
        let is_agent = matches!(&task.value.action, nika_schema::raw::RawAction::Agent(_));
        if !is_prompt && !is_agent {
            return Err(format!(
                "--answer {task_id}: not a `nika:prompt` or `agent` task — an answer would do nothing"
            ));
        }
        // Persona 17: confirm prints `[yes/no]`. Bind the same boolean
        // the TTY writes — and only then. `mode: input` (and `choice` /
        // `agent`) keeps `yes` as the string `"yes"`; a boolean does
        // not fill an input gate (the GATED resume e2e).
        let value = match confirm_yes_no(&task.value, raw) {
            Some(flag) => serde_json::Value::Bool(flag),
            None => serde_json::from_str(raw)
                .unwrap_or_else(|_| serde_json::Value::String(raw.to_owned())),
        };
        answers.insert(task_id.to_owned(), value);
    }
    Ok(answers)
}

/// Confirm-mode `nika:prompt` (stdlib default) accepts the TTY's
/// `yes`/`y`/`no`/`n`. Every other answerable task leaves the raw
/// token for JSON-or-string parse.
fn confirm_yes_no(task: &nika_schema::raw::RawTask, raw: &str) -> Option<bool> {
    if !is_confirm_prompt(task) {
        return None;
    }
    match raw.trim().to_ascii_lowercase().as_str() {
        "yes" | "y" => Some(true),
        "no" | "n" => Some(false),
        _ => None,
    }
}

fn is_confirm_prompt(task: &nika_schema::raw::RawTask) -> bool {
    let nika_schema::raw::RawAction::Invoke(invoke) = &task.action else {
        return false;
    };
    if invoke.tool().map(|t| t.value.as_str()) != Some("nika:prompt") {
        return false;
    }
    match invoke
        .args
        .as_ref()
        .and_then(|a| a.value.get("mode"))
        .and_then(serde_json::Value::as_str)
    {
        None | Some("confirm") => true,
        Some(_) => false,
    }
}

/// A `--answer` that names a task already journaled as success in the
/// resume plan would re-open a settled gate (#1067). Fresh runs (`plan`
/// empty) and paused gates (the prompt never completed, so it is not in
/// the plan) still accept answers. A decided gate stays decided.
///
/// # Errors
///
/// A human-readable refusal (environment class).
pub fn refuse_reopened_settled_gates(
    plan: &ResumePlan,
    answers: &BTreeMap<String, serde_json::Value>,
) -> Result<(), String> {
    for id in answers.keys() {
        if plan.contains_key(id) {
            return Err(format!(
                "--answer {id}: this gate already settled in the resumed trace · \
                 a decided gate stays decided. Start a new run (no --resume) to \
                 ask again"
            ));
        }
    }
    Ok(())
}

/// The resume's chain-trust verdict (ADR-099 trust amendment ·
/// 2026-08-08) — a resume TRUSTS the trace's recorded successes: it
/// serves them as cache hits and runs live tasks on their values. The
/// tamper-evidence walk ([`crate::chain::walk`]) therefore judges the
/// trace BEFORE the fold: the journal is one artifact with two roles
/// (the audit record · the checkpoint), and the second role now
/// CONSULTS the first — enforced, no longer aspirational. The opt-out
/// stays possible but NAMED (`--resume-unverified` · attested on the
/// run's boot manifest), never a silent default.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrustVerdict {
    /// The chain holds over every line — or over every COMPLETE line:
    /// a crash (killed mid-flight · a torn tail) leaves an honest
    /// journal, and crash-resumption is exactly the resume use case.
    Verified,
    /// A pre-chain (pre-0.96) or event-less journal: nothing to verify
    /// and — the fold finds no resume keys on it — nothing to trust.
    /// The honest-degradation law covers it (a notice, never an error).
    Unverifiable,
    /// The chain is broken, unreadable mid-journal, or over the line
    /// bound — the forgery / `DoS` class. The run refuses unless the
    /// operator names the opt-out; `finding` carries the walk's
    /// one-line evidence (sanitized — the journal is untrusted input).
    Tampered {
        /// The walk's one-line finding (line · hashes · bound named).
        finding: String,
    },
}

/// Judge a `--resume` trace's chain BEFORE its records are trusted —
/// the pure half; the caller maps the verdict to its exit class and
/// attests any opt-out on the run's boot manifest. A crash leaves a
/// torn TAIL (trusted); a bad line with good lines after it is a HOLE
/// (the tamper class — no crash writes one).
#[must_use]
pub fn judge_trust(raw: &str) -> TrustVerdict {
    use crate::chain::Verdict;
    match crate::chain::walk(raw) {
        Verdict::Intact { .. } | Verdict::Incomplete { .. } | Verdict::TornTail { .. } => {
            TrustVerdict::Verified
        }
        Verdict::Unchained | Verdict::Empty => TrustVerdict::Unverifiable,
        Verdict::Broken {
            line,
            recorded,
            computed,
        } => TrustVerdict::Tampered {
            finding: format!(
                "chain BROKEN at line {line} — recorded {} · computed {} (edited, inserted, dropped or reordered)",
                short(&recorded),
                short(&computed)
            ),
        },
        Verdict::Unreadable { line } => TrustVerdict::Tampered {
            finding: format!(
                "line {line} is not valid JSON mid-journal — a crash leaves a torn tail, not a hole; the chain is unverifiable"
            ),
        },
        Verdict::LineOverLong { line, got } => TrustVerdict::Tampered {
            finding: format!(
                "line {line} is {got} bytes — over the journal line bound (a journal line is small; an oversized one is the DoS class)"
            ),
        },
    }
}

/// First 16 chars of a control-stripped string — a journal field is
/// untrusted input: terminal escapes never ride a finding (the
/// `trace verify` voice, one idiom).
fn short(hex: &str) -> String {
    hex.chars().filter(|c| !c.is_control()).take(16).collect()
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
        const WF: &str = "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n  b:\n    after:\n      a: success\n    exec: { command: [\"true\"] }\n  c:\n    with:\n      prev: ${{ tasks.b.output }}\n    exec: { command: [\"echo\", \"${{ with.prev }}\"] }\n  solo:\n    exec: { command: [\"true\"] }\n";
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

    // ── ADR-099 trust amendment · the chain precondition ─────────────

    /// A chained journal the way the sink writes one (the nika-dap
    /// chain-test idiom).
    fn chained(kinds: &[&str]) -> String {
        let mut chain = nika_event::source_id::sha256_hex(crate::chain::CHAIN_GENESIS);
        let mut out = String::new();
        for kind in kinds {
            let mut v = serde_json::json!({
                "id": {"uuid": "01912345-0000-7000-8000-000000000001"},
                "timestamp": 1000, "kind": kind, "run": null,
                "correlation": null, "fields": []
            });
            v["chain"] = serde_json::Value::String(chain.clone());
            let line = serde_json::to_string(&v).expect("test json");
            chain = nika_event::source_id::sha256_hex(line.as_bytes());
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    #[test]
    fn an_intact_or_crashed_journal_is_trusted() {
        assert_eq!(
            judge_trust(&chained(&["workflow_started", "workflow_completed"])),
            TrustVerdict::Verified
        );
        // The crash signatures ARE the resume use case — trusted.
        let killed = chained(&["workflow_started", "task_completed"]);
        assert_eq!(judge_trust(&killed), TrustVerdict::Verified);
        let mut torn = chained(&["workflow_started"]);
        torn.push_str("{\"id\":{\"uuid\":\"half-written-by-sigkill");
        assert_eq!(judge_trust(&torn), TrustVerdict::Verified);
    }

    #[test]
    fn a_pre_chain_journal_is_unverifiable_never_an_error() {
        let raw = "{\"kind\":\"workflow_started\"}\n";
        assert_eq!(judge_trust(raw), TrustVerdict::Unverifiable);
        assert_eq!(judge_trust(""), TrustVerdict::Unverifiable);
    }

    #[test]
    fn one_edited_byte_refuses_the_trust() {
        let raw = chained(&["workflow_started", "task_completed", "workflow_completed"])
            .replace("task_completed", "task_complexed");
        let TrustVerdict::Tampered { finding } = judge_trust(&raw) else {
            panic!("an edited journal is the tamper class");
        };
        assert!(finding.contains("BROKEN at line 3"), "{finding}");
    }

    #[test]
    fn a_mid_journal_hole_is_tampered_not_a_crash() {
        // A crash leaves a torn TAIL — a bad line with good lines after
        // it is the hole class, refused by name.
        let good = chained(&["workflow_started", "workflow_completed"]);
        let mut lines: Vec<&str> = good.lines().collect();
        lines.insert(1, "{corrupt");
        let raw = lines.join("\n");
        let TrustVerdict::Tampered { finding } = judge_trust(&raw) else {
            panic!("a mid-journal hole is not a crash");
        };
        assert!(finding.contains("line 2"), "{finding}");
    }

    #[test]
    fn a_finding_never_rides_terminal_escapes() {
        // The journal is untrusted input: an adversarial `chain` string
        // renders control-stripped and truncated (the verify idiom).
        let mut raw = chained(&["workflow_started", "workflow_completed"]);
        raw = raw.replacen("\"chain\":\"", "\"chain\":\"\u{1b}[2Jevil", 1);
        let TrustVerdict::Tampered { finding } = judge_trust(&raw) else {
            panic!("a forged chain field is the tamper class");
        };
        assert!(!finding.contains('\u{1b}'), "stripped: {finding}");
    }

    #[test]
    fn answers_bind_only_known_prompt_tasks() {
        const WF: &str = "nika: t\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"go?\" }\n  build:\n    exec: { command: [\"true\"] }\n";
        // An agent-task fixture (B5 · items live at scope top, the lint law).
        const AGENT_WF: &str = "nika: t\ntasks:\n  fix:\n    agent: { prompt: \"x\" }\n";
        const INPUT_WF: &str = "nika: t\ntasks:\n  approve:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"input\", message: \"ship it?\" }\n";
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
        // P3 B5 · an agent task IS answerable (the harness gate binds it).
        let agent_wf = nika_schema::parse(
            AGENT_WF,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses");
        let answers = parse_answers(&["fix=false".to_owned()], &agent_wf)
            .expect("an agent task takes the gate answer");
        assert_eq!(answers["fix"], serde_json::json!(false));
        // Shape errors are loud.
        assert!(parse_answers(&["noequals".to_owned()], &wf).is_err());
        // Confirm (and the stdlib default) coerce yes/no; input keeps the
        // string so `--answer approve=yes` fills "shipping yes".
        let yes = parse_answers(&["ask=yes".to_owned()], &wf).expect("confirm yes");
        assert_eq!(yes["ask"], serde_json::json!(true));
        let no = parse_answers(&["ask=n".to_owned()], &wf).expect("confirm n");
        assert_eq!(no["ask"], serde_json::json!(false));
        let input_wf = nika_schema::parse(
            INPUT_WF,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses");
        let typed = parse_answers(&["approve=yes".to_owned()], &input_wf).expect("input yes");
        assert_eq!(typed["approve"], serde_json::json!("yes"));
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

    // ── Issue 772 · the model carry judgment ──────────────────────────

    /// A boot manifest with or without the `model_override` field.
    fn started_with_model(model: Option<&str>) -> Event {
        let mut e = started(Some("0.107.2"));
        if let Some(m) = model {
            e = e.with_field(KeyValue::new(
                "model_override",
                FieldValue::String(m.to_owned()),
            ));
        }
        e
    }

    #[test]
    fn the_model_reader_finds_the_override_and_absence_is_no_claim() {
        let events = vec![started_with_model(Some("mock/override"))];
        assert_eq!(
            trace_model_override(&events).as_deref(),
            Some("mock/override")
        );
        // No field (an override-less run · an older journal) — no claim.
        assert_eq!(trace_model_override(&[started_with_model(None)]), None);
        assert_eq!(trace_model_override(&[]), None);
    }

    #[test]
    fn the_model_judgment_proceeds_on_match_and_notices_a_change() {
        // No recorded override — nothing to carry, whatever argv says.
        assert_eq!(
            judge_model(None, None),
            ModelVerdict::Proceed { changed: None }
        );
        assert_eq!(
            judge_model(None, Some("mock/override")),
            ModelVerdict::Proceed { changed: None }
        );
        // The same seat — proceeds without a claim.
        assert_eq!(
            judge_model(Some("mock/override"), Some("mock/override")),
            ModelVerdict::Proceed { changed: None }
        );
        // An explicit different seat — proceeds, the change named.
        assert_eq!(
            judge_model(Some("mock/override"), Some("mock/other")),
            ModelVerdict::Proceed {
                changed: Some(("mock/override".to_owned(), "mock/other".to_owned())),
            }
        );
    }

    #[test]
    fn a_flagless_resume_of_an_override_run_refuses_with_the_teaching() {
        let ModelVerdict::Refuse(message) = judge_model(Some("mock/override"), None) else {
            panic!("a flag-less resume of an override run refuses");
        };
        assert!(message.contains("mock/override"), "{message}");
        assert!(
            message.contains("--model mock/override"),
            "the teaching names the exact flag: {message}"
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

    #[test]
    fn a_settled_prompt_refuses_a_fresh_answer() {
        let mut plan = ResumePlan::new();
        plan.insert(
            "human".to_owned(),
            PriorSuccess::new("d".repeat(64), "i".repeat(64), serde_json::json!(false)),
        );
        let mut answers = BTreeMap::new();
        answers.insert("human".to_owned(), serde_json::json!(true));
        let err = refuse_reopened_settled_gates(&plan, &answers).expect_err("settled re-open");
        assert!(err.contains("human"), "{err}");
        assert!(err.contains("stays decided"), "{err}");
    }

    #[test]
    fn a_paused_or_fresh_gate_still_accepts_an_answer() {
        let plan = ResumePlan::new();
        let mut answers = BTreeMap::new();
        answers.insert("human".to_owned(), serde_json::json!(true));
        refuse_reopened_settled_gates(&plan, &answers)
            .expect("an unanswered gate is the legitimate --answer path");
    }

    #[test]
    fn answering_a_different_task_than_the_settled_ones_is_fine() {
        let mut plan = ResumePlan::new();
        plan.insert(
            "check_a".to_owned(),
            PriorSuccess::new("d".repeat(64), "i".repeat(64), serde_json::json!("ok")),
        );
        let mut answers = BTreeMap::new();
        answers.insert("human".to_owned(), serde_json::json!(true));
        refuse_reopened_settled_gates(&plan, &answers)
            .expect("the human gate is not in the plan, so it has not settled");
    }
}
