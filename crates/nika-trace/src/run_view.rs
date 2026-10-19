// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a run's trace proves — the result, the gate and the proof views,
//! read from the journal's own frames, never from what the run printed.
//!
//! Owned by the flight-recorder reader since 2026-09-24 (moved verbatim
//! from `nika-session`, which read nothing of its own here: frames in,
//! text out, plus the ONE verify door this crate already hosts). The
//! session decides WHEN a view is shown and what it asks; this module says
//! what the trace proves. The public surface is the read-only doors of
//! [`RunFacts`] (`read` · `result` · `gate` · `proof` · `pause_gate`) and,
//! for a paused journal's resume, what it would run again ([`resumed_live`]
//! · [`live_again`]), and whether a kept run's execution ran a component of
//! saved bytes ([`KeptRun::component_run`]); the facts' fields and the
//! per-task, permit, approval, pause and seal facts stay crate-private.
//!
//! One reading (`RunFacts::read`) feeds three views:
//! - RESULT · after a run ended: produced · read · sent · asked · approved
//!   · cost, each line a fact a frame carries (a permit decision, a task
//!   completion, an approval), never an inference from the workflow;
//! - GATE · when a run paused: what ran so far, the question, what a yes
//!   lets happen (from the workflow's own bytes), what a no does;
//! - PROOF · on `/proof`: the chain verdict from the ONE verify door
//!   (`nika trace verify`), the workflow's identity, the boundary the run
//!   exercised, the digests of what it wrote, and what the proof does NOT
//!   cover.
//!
//! Truth labels, said in the views: MEASURED (the engine wrote the frame
//! as it happened) · RE-READ (a file as it is now, not as it was written)
//! · NOT PROVEN (the content's fitness · trust outside this machine).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nika_event::EventKind;
use nika_types::id::ExecutionId;
use nika_types::resource::Value as FieldValue;
use serde_json::Value;

/// How a task's frames settle it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TaskState {
    /// Scheduled, never started (the run ended before it).
    #[default]
    Scheduled,
    /// Started, no terminal frame (a torn journal · a pause elsewhere).
    Running,
    /// Completed.
    Ok,
    /// Failed (the detail rides beside).
    Failed,
    /// Failed, then its `on_error` recovered it.
    Recovered,
    /// Skipped (a `when:` that read false · a skip on error).
    Skipped,
    /// Cancelled by the run's end.
    Cancelled,
    /// Replayed from the cache on a resume: nothing ran again.
    CacheHit,
}

/// One task as its frames settle it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TaskFact {
    pub(crate) id: String,
    /// The started frame's note: `invoke · nika:read` · `infer · <model>`.
    pub(crate) note: String,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) state: TaskState,
    /// The failure's detail, when it failed.
    pub(crate) detail: Option<String>,
    /// The note of the `task_started` frame that opened it, as that frame said it.
    pub(crate) started: Option<String>,
    /// What a `task_completed` frame whose outcome class is `success` returned, when its
    /// `output` is a JSON string (a `nika:write` returns the path it wrote).
    pub(crate) output: Option<String>,
}

/// One permit decision the run recorded (`permit_checked`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PermitFact {
    pub(crate) task: String,
    /// `fs` · `tool` · `net` · `exec` · `env`.
    pub(crate) plane: String,
    /// `permits.fs.write ./out/x.md` · `nika:read` · a host.
    pub(crate) gate: String,
    /// `allow` · `deny`.
    pub(crate) decision: String,
}

/// A human approval the run recorded (`approval_decided`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Approval {
    pub(crate) task: String,
    /// `allow` · `deny`.
    pub(crate) decision: String,
    /// `resume` (the answer came through `--answer`) · `terminal`.
    pub(crate) source: String,
}

/// The pause a run left (`workflow_paused`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Pause {
    pub(crate) task: String,
    pub(crate) message: String,
    pub(crate) mode: String,
    pub(crate) choices: Option<Vec<String>>,
    pub(crate) approval: Option<nika_runtime::approval::ApprovalTicket>,
    /// The values its message shows (`values`) and the keys withheld with why (`withheld`),
    /// each a JSON object; `None` on a frame that carries none.
    pub(crate) shown: (Option<Value>, Option<Value>),
}

/// The seal's covered facts (`run_sealed`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Seal {
    pub(crate) head: String,
    pub(crate) events: Option<u64>,
    pub(crate) key_id: String,
    pub(crate) alg: String,
    pub(crate) declared: Option<u64>,
    pub(crate) exercised: Option<u64>,
    pub(crate) escapes: Option<u64>,
}

/// Everything a trace's frames say about one run. Read-only outside this
/// crate: built by [`RunFacts::read`], said by its three views.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct RunFacts {
    pub(crate) trace: PathBuf,
    pub(crate) workflow: Option<String>,
    pub(crate) workflow_sha256: Option<String>,
    pub(crate) semantic_hash: Option<String>,
    pub(crate) engine_version: Option<String>,
    pub(crate) sandbox: Option<String>,
    /// `succeeded` · `failed` · `cancelled` — the terminal frame's word.
    pub(crate) status: Option<String>,
    /// Why the run ended, as its terminal frame names it (`budget`: its ceiling was reached).
    pub(crate) cause: Option<String>,
    pub(crate) elapsed_ms: Option<u64>,
    pub(crate) priced_calls: Option<u64>,
    pub(crate) unpriced_calls: Option<u64>,
    /// Why calls stayed unpriced: the `cost_unpriced` reasons the task frames name.
    pub(crate) unpriced_reasons: BTreeSet<String>,
    pub(crate) cost_qualifier: Option<String>,
    pub(crate) total_cost_usd: Option<f64>,
    /// The cost ceiling the run was admitted under (`workflow_started` · `budget`).
    pub(crate) ceiling_usd: Option<f64>,
    pub(crate) tasks: Vec<TaskFact>,
    pub(crate) permits: Vec<PermitFact>,
    pub(crate) approvals: Vec<Approval>,
    pub(crate) pause: Option<Pause>,
    /// The gate the FIRST pause asks, as a host answering it reads the frame ([`Self::pause_gate`]);
    /// the result and proof views keep reading the last pause, `pause`.
    pub(crate) gate: Option<Pause>,
    pub(crate) seal: Option<Seal>,
    /// Frames read (lines that parsed as events).
    pub(crate) events: usize,
    /// The executions the frames carry, distinct, first seen first.
    pub(crate) executions: Vec<ExecutionId>,
    /// Frames whose execution is absent or not one (never guessed).
    pub(crate) unidentified: usize,
    /// The source hash each `workflow_started` names, in order (`None`: none named).
    pub(crate) starts: Vec<Option<String>>,
    /// The workflow's declared `outputs:` as its terminal frame recorded them (`outputs`), when
    /// it kept them.
    pub(crate) outputs: Option<serde_json::Map<String, Value>>,
    /// The run this journal continued (`workflow_started` · `resumed_from`), as its trace id.
    pub(crate) resumed_from: Option<String>,
    /// What the run had spent when it stopped at a gate (`workflow_paused`): priced calls,
    /// unpriced calls and the total, as that frame recorded them.
    pub(crate) pause_spend: (Option<u64>, Option<u64>, Option<f64>),
    /// What the legs this journal continued spent, as their own journals recorded it
    /// ([`RunFacts::read`]): a run a gate split is one run, and its cost is every leg's.
    pub(crate) before: Before,
}

/// What the earlier legs of a resumed run spent, summed: how many legs, their priced and
/// unpriced calls, their total when one was recorded, and why calls stayed unpriced.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Before {
    pub(crate) legs: usize,
    pub(crate) priced_calls: u64,
    pub(crate) unpriced_calls: u64,
    pub(crate) total_cost_usd: Option<f64>,
    pub(crate) unpriced_reasons: BTreeSet<String>,
}

/// A failure's detail with its code said once (live gauntlet r04 B14: « NIKA-BUILTIN-JQ-001 ·
/// tool `nika:jq` reported an error: NIKA-BUILTIN-JQ-001 · jq runtime error … »).
fn code_once(detail: &str) -> String {
    let Some((code, rest)) = detail.split_once(" · ") else {
        return detail.to_owned();
    };
    if !code.starts_with("NIKA-") {
        return detail.to_owned();
    }
    let again = format!(": {code} · ");
    format!("{code} · {}", rest.replacen(&again, ": ", 1))
}

/// `a` and `b` summed, a missing total missing only when both are.
fn summed(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a + b),
        (a, b) => a.or(b),
    }
}

/// What the legs a resumed journal continued spent: each journal of `trace`'s directory whose
/// run the one before names as the run it continued (`resumed_from`), followed up to
/// [`crate::lineage::MAX_LINKS`] links. A link no journal there answers, or a loop, ends the
/// walk; nothing is guessed. Each leg's spend is its terminal frame's, or its pause's when it
/// stopped at a gate.
fn before(trace: &Path, resumed_from: Option<&str>) -> Before {
    let mut before = Before::default();
    let Some(dir) = trace.parent() else {
        return before;
    };
    let mut seen = BTreeSet::new();
    let mut next = resumed_from.map(str::to_owned);
    while let Some(run) = next.take() {
        if before.legs >= crate::lineage::MAX_LINKS || !seen.insert(run.clone()) {
            break;
        }
        let Some(leg) = journal_of(dir, &run).and_then(|(path, raw)| RunFacts::of(&path, &raw))
        else {
            break;
        };
        let (priced, unpriced, total) = leg.spend();
        before.legs += 1;
        before.priced_calls += priced;
        before.unpriced_calls += unpriced;
        before.total_cost_usd = summed(before.total_cost_usd, total);
        before.unpriced_reasons.extend(leg.unpriced_reasons);
        next = leg.resumed_from;
    }
    before
}

/// The journal of `dir` whose opening frame carries the run `run` (a trace id, as a
/// continuation names it), and its bytes: the files whose name ends with the run's last four
/// characters first (the store's naming), every other one after. Only a journal's opening frame
/// identifies it.
fn journal_of(dir: &Path, run: &str) -> Option<(PathBuf, String)> {
    let mut names: Vec<PathBuf> = (std::fs::read_dir(dir).ok()?.flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
        .collect();
    let tail = run.get(run.len().saturating_sub(4)..).unwrap_or(run);
    let suffixed = |path: &PathBuf| {
        (path.file_stem().and_then(|stem| stem.to_str())).is_some_and(|stem| stem.ends_with(tail))
    };
    names.sort_by_key(|path| !suffixed(path));
    names.into_iter().find_map(|path| {
        let raw = std::fs::read_to_string(&path).ok()?;
        let opening: Value = serde_json::from_str(raw.lines().next()?).ok()?;
        let execution: ExecutionId =
            serde_json::from_value(opening.get("execution")?.clone()).ok()?;
        (nika_types::id::TraceId::from(execution).to_string() == run).then_some((path, raw))
    })
}

/// The cost ceiling a `workflow_started` frame's `budget` names (`{"max_cost_usd": 0.1}`, an object
/// or its JSON text): what the person and the conversation are told the run was allowed to cost.
/// Live gauntlet, 2026-10-16: asked whether her $0.10 ceiling held, the author answered that the
/// run's summary did not say which ceiling applied.
fn budget_ceiling(f: &BTreeMap<String, Value>) -> Option<f64> {
    let budget = f.get("budget")?;
    let parsed;
    let budget = match budget {
        Value::String(text) => {
            parsed = serde_json::from_str::<Value>(text).ok()?;
            &parsed
        }
        other => other,
    };
    budget.get("max_cost_usd").and_then(Value::as_f64)
}

/// The last run under `root`, read from the store's newest journal (the
/// evidence, never memory): the workflow, every task's outcome, the
/// settlement. Moved from the session's facts, which read nothing of their
/// own here.
#[must_use]
pub fn last_run(root: &Path) -> String {
    let store = root.join(".nika").join("traces");
    let Some(trace) = newest_journal(&store) else {
        return "no run yet under this root (no trace in `.nika/traces/`)".to_owned();
    };
    let Ok(journal) = std::fs::read_to_string(&trace) else {
        return format!("the latest trace `{}` could not be read", trace.display());
    };
    let (mut workflow, mut tasks, mut settled) = (String::new(), Vec::new(), None);
    for frame in journal
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        let f = fields(&frame);
        let field = |key: &str| {
            f.get(key)
                .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                .unwrap_or_default()
        };
        match frame.get("kind").and_then(Value::as_str).unwrap_or("") {
            "workflow_started" => workflow = field("workflow"),
            "task_completed" => tasks.push(format!(
                "✔ {} · {} · {} ms",
                field("task"),
                field("note"),
                field("duration_ms")
            )),
            "task_failed" => tasks.push(format!("✖ {} · {}", field("task"), field("error"))),
            "task_skipped" => tasks.push(format!("○ {} · skipped", field("task"))),
            "workflow_completed" => settled = Some("completed".to_owned()),
            "workflow_failed" => settled = Some(format!("failed · {}", field("error"))),
            "workflow_paused" => settled = Some("paused for a human answer".to_owned()),
            _ => {}
        }
    }
    let settled = settled.unwrap_or_else(|| {
        "no settlement line — the run may still be going, or was cut".to_owned()
    });
    let tasks = if tasks.is_empty() {
        "no task line".to_owned()
    } else {
        tasks.join("\n  ")
    };
    format!(
        "last run · `{workflow}` · {settled} · read from `{}`\n  {tasks}",
        trace.display()
    )
}

/// The newest `.ndjson` under the store by mtime (name tie-break), read raw:
/// an unreadable or cut journal is still the last run, never hidden behind
/// the one before it.
fn newest_journal(store: &Path) -> Option<PathBuf> {
    let mut journals: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(store)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .filter_map(|p| {
            let modified = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            Some((modified, p))
        })
        .collect();
    journals.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    journals.into_iter().next().map(|(_, p)| p)
}

/// A frame's `fields` (`[{key, value}]`) as a map.
fn fields(frame: &Value) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    if let Some(rows) = frame.get("fields").and_then(Value::as_array) {
        for row in rows {
            if let (Some(key), Some(value)) =
                (row.get("key").and_then(Value::as_str), row.get("value"))
            {
                map.insert(key.to_owned(), value.clone());
            }
        }
    }
    map
}

/// A pause frame's gate: the first value of each key (never a later duplicate), a task that is
/// text and not empty, the pause's defaults for the message and the mode, and the values its
/// message shows with the keys withheld, each when the frame carries it as a JSON object.
fn first_gate(frame: &Value) -> Option<Pause> {
    let field = |key: &str| -> Option<String> {
        let row = frame
            .get("fields")?
            .as_array()?
            .iter()
            .find(|r| r.get("key").and_then(Value::as_str) == Some(key))?;
        row.get("value")?.as_str().map(str::to_owned)
    };
    let object = |key: &str| {
        let parsed = field(key).and_then(|text| serde_json::from_str::<Value>(&text).ok());
        parsed.filter(Value::is_object)
    };
    Some(Pause {
        task: field("task").filter(|t| !t.is_empty())?,
        message: field("message").unwrap_or_else(|| "the run awaits your answer".to_owned()),
        mode: field("mode").unwrap_or_else(|| "text".to_owned()),
        shown: (object("values"), object("withheld")),
        choices: field("choices").and_then(|text| serde_json::from_str(&text).ok()),
        approval: pause_approval(frame),
    })
}

/// Display evidence only. Keep first-field semantics and the runtime's TTL law.
fn pause_approval(frame: &Value) -> Option<nika_runtime::approval::ApprovalTicket> {
    let field = |key| {
        frame
            .get("fields")?
            .as_array()?
            .iter()
            .find(|row| row.get("key").and_then(Value::as_str) == Some(key))?
            .get("value")
    };
    let minted = field("approval_minted_at_ms")?.as_i64()?;
    if minted < 0 {
        return None;
    }
    Some(nika_runtime::approval::ApprovalTicket::new(
        field("approval_shown_hash")?.as_str()?.to_owned(),
        field("approval_nonce")?.as_str()?.to_owned(),
        field("task")?.as_str()?.to_owned(),
        minted,
        u32::try_from(field("approval_ttl_seconds")?.as_u64()?).ok()?,
    ))
}

fn text(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// A completed frame's returned string: its `output` field holds the value as JSON, and only an
/// `outcome` whose class is `success` returned it.
fn completed_output(f: &BTreeMap<String, Value>) -> Option<String> {
    let outcome: Value = serde_json::from_str(&text(f, "outcome")?).ok()?;
    (outcome["class"] == "success").then_some(())?;
    serde_json::from_str::<String>(&text(f, "output")?).ok()
}

fn count(map: &BTreeMap<String, Value>, key: &str) -> Option<u64> {
    map.get(key).and_then(Value::as_u64)
}

impl RunFacts {
    /// Read a trace's frames. `None` when the file cannot be read or no
    /// line is an event (the view then falls back to the door's line).
    ///
    /// A resumed journal also reads what the legs it continued spent, from their own journals
    /// in the same directory: a run a gate split costs every leg's calls.
    #[must_use]
    pub fn read(trace: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(trace).ok()?;
        let mut facts = Self::of(trace, &raw)?;
        facts.before = before(trace, facts.resumed_from.as_deref());
        Some(facts)
    }

    /// What this leg spent: its terminal frame's priced calls, unpriced calls and total, or its
    /// pause's when it stopped at a gate.
    fn spend(&self) -> (u64, u64, Option<f64>) {
        if self.status.is_some() {
            let (priced, unpriced) = (self.priced_calls, self.unpriced_calls);
            return (
                priced.unwrap_or(0),
                unpriced.unwrap_or(0),
                self.total_cost_usd,
            );
        }
        let (priced, unpriced, total) = self.pause_spend;
        (priced.unwrap_or(0), unpriced.unwrap_or(0), total)
    }

    /// The frames of journal bytes a host already captured from `trace`:
    /// what [`Self::read`] folds, without reading anything. A fold, never a
    /// verdict: the chain is the verifier's to judge.
    #[must_use]
    pub fn of(trace: &Path, raw: &str) -> Option<Self> {
        let mut facts = Self {
            trace: trace.to_path_buf(),
            ..Self::default()
        };
        let mut paused = false;
        for line in raw.lines() {
            let Ok(frame) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let Some(kind) = frame.get("kind").and_then(Value::as_str) else {
                continue;
            };
            facts.events += 1;
            let execution = frame.get("execution").cloned().map(serde_json::from_value);
            match execution {
                Some(Ok(id)) if !facts.executions.contains(&id) => facts.executions.push(id),
                Some(Ok(_)) => {}
                _ => facts.unidentified += 1,
            }
            let fields = fields(&frame);
            if kind == "workflow_started" {
                facts.starts.push(text(&fields, "workflow_sha256"));
            }
            facts.absorb(kind, &fields);
            if kind == "workflow_paused" {
                if !paused {
                    paused = true;
                    facts.gate = first_gate(&frame);
                } else if let Some(gate) = &mut facts.gate {
                    // Session displays the first pause; runtime folds the last ticket.
                    // Multiple pauses cannot establish one displayed ticket's expiry.
                    gate.approval = None;
                }
            }
        }
        (facts.events > 0).then_some(facts)
    }

    /// The executions the frames carry, distinct, first seen first.
    #[must_use]
    pub fn executions(&self) -> &[ExecutionId] {
        &self.executions
    }

    /// How many frames carry no execution, or one that is not one.
    #[must_use]
    pub fn unidentified(&self) -> usize {
        self.unidentified
    }

    /// The source hash each `workflow_started` frame names, in order: one
    /// start naming one hash is the only shape a single run leaves.
    #[must_use]
    pub fn starts(&self) -> &[Option<String>] {
        &self.starts
    }

    /// The source hash the last `workflow_started` names ([`Self::starts`]
    /// holds every one).
    #[must_use]
    pub fn workflow_sha256(&self) -> Option<&str> {
        self.workflow_sha256.as_deref()
    }

    /// What the run cost in all, the legs a gate split included, as its frames recorded it:
    /// `None` when no total was recorded.
    #[must_use]
    pub fn spent_usd(&self) -> Option<f64> {
        summed(self.total_cost_usd, self.before.total_cost_usd)
    }

    /// What the ceiling this leg was admitted under leaves after what this leg spent (its pause's
    /// spend when it stopped at a gate): the ceiling the run continues under when it is resumed.
    /// `None` when the leg was admitted under no ceiling.
    #[must_use]
    pub fn ceiling_left(&self) -> Option<f64> {
        let ceiling = self.ceiling_usd?;
        Some((ceiling - self.spend().2.unwrap_or(0.0)).max(0.0))
    }

    /// Whether the person refused one of the run's gates (an approval decided otherwise than
    /// `allow`): the run ended where their answer stopped it, never as a plain success.
    #[must_use]
    pub fn refused_at_gate(&self) -> bool {
        (self.approvals.iter()).any(|a| a.decision != "allow")
    }

    /// Whether the run's ceiling stopped it (its terminal frame's cause is `budget`).
    #[must_use]
    pub fn stopped_by_ceiling(&self) -> bool {
        self.cause.as_deref() == Some("budget")
    }

    /// The terminal frame's word (`succeeded` · `failed` · `cancelled`, or
    /// `paused` when the journal ends at a gate); `None` when it holds no
    /// terminal frame, never « done ».
    #[must_use]
    pub fn terminal(&self) -> Option<&str> {
        (self.status.as_deref()).or_else(|| self.pause.as_ref().map(|_| "paused"))
    }

    /// The gate a paused run asks a host to answer (C9): the task, message and mode of its FIRST
    /// `workflow_paused` frame, each the first value its fields give that key, with the pause's
    /// defaults (« the run awaits your answer » · `text`). `None` when the journal records no
    /// pause, or when that first pause names no task (absent, not text, or empty): a later pause
    /// never stands in for it. The gate VIEW ([`Self::gate`]) keeps its own inputs.
    #[must_use]
    pub fn pause_gate(&self) -> Option<(&str, &str, &str)> {
        self.gate
            .as_ref()
            .map(|g| (g.task.as_str(), g.message.as_str(), g.mode.as_str()))
    }

    /// What that first pause's gate shows, structured: the values its message renders by key
    /// (`values`) and the keys withheld with why (`withheld`, a value derived from a secret
    /// among them), each a JSON object; `None` where the frame carries none (an older journal).
    #[must_use]
    pub fn pause_shown(&self) -> (Option<&Value>, Option<&Value>) {
        (self.gate.as_ref()).map_or((None, None), |g| (g.shown.0.as_ref(), g.shown.1.as_ref()))
    }

    /// The exact string options of the first pause, when its first `choices` field
    /// contains a JSON string array. Missing or malformed fields stay unknown;
    /// later fields and pauses never supply replacements. This is display data,
    /// not an approval or a claim that the trace is trusted.
    #[must_use]
    pub fn pause_choices(&self) -> Option<&[String]> {
        self.gate.as_ref()?.choices.as_deref()
    }

    /// Whether the single paused gate's recorded ticket has expired at Unix
    /// milliseconds `now_ms`, using the runtime's exact remaining-seconds law.
    /// Missing/malformed timing, negative time or multiple pauses stay unknown.
    /// This is display evidence, never a trust verdict or resume authority.
    #[must_use]
    pub fn pause_approval_expired(&self, now_ms: i64) -> Option<bool> {
        (now_ms >= 0).then_some(())?;
        Some(self.gate.as_ref()?.approval.as_ref()?.is_expired(now_ms))
    }

    fn task_mut(&mut self, id: &str) -> &mut TaskFact {
        if let Some(i) = self.tasks.iter().position(|t| t.id == id) {
            return &mut self.tasks[i];
        }
        self.tasks.push(TaskFact {
            id: id.to_owned(),
            ..TaskFact::default()
        });
        let last = self.tasks.len() - 1;
        &mut self.tasks[last]
    }

    fn absorb(&mut self, kind: &str, f: &BTreeMap<String, Value>) {
        match kind {
            "workflow_started" => {
                self.workflow = text(f, "workflow");
                self.workflow_sha256 = text(f, "workflow_sha256");
                self.semantic_hash = text(f, "semantic_hash");
                self.engine_version = text(f, "engine_version");
                self.sandbox = text(f, "sandbox");
                self.ceiling_usd = budget_ceiling(f);
                self.resumed_from = text(f, "resumed_from");
            }
            "task_scheduled" | "task_started" | "task_completed" | "task_failed"
            | "task_recovered" | "task_skipped" | "task_cancelled" | "task_cache_hit" => {
                self.absorb_task(kind, f);
            }
            "permit_checked" => self.permits.push(PermitFact {
                task: text(f, "task").unwrap_or_default(),
                plane: text(f, "plane").unwrap_or_default(),
                gate: text(f, "gate").unwrap_or_default(),
                decision: text(f, "decision").unwrap_or_default(),
            }),
            "approval_decided" => self.approvals.push(Approval {
                task: text(f, "task").unwrap_or_default(),
                decision: text(f, "decision").unwrap_or_default(),
                source: text(f, "source").unwrap_or_default(),
            }),
            "workflow_paused" => {
                // What the run had spent at its gate: a continuation adds it to its own.
                self.pause_spend = (
                    count(f, "priced_calls"),
                    count(f, "unpriced_calls"),
                    f.get("total_cost_usd").and_then(Value::as_f64),
                );
                self.pause = Some(Pause {
                    task: text(f, "task").unwrap_or_default(),
                    message: text(f, "message")
                        .unwrap_or_else(|| "the run awaits your answer".to_owned()),
                    mode: text(f, "mode").unwrap_or_else(|| "text".to_owned()),
                    ..Pause::default()
                });
            }
            "workflow_completed" | "workflow_failed" | "workflow_cancelled" => {
                self.status = text(f, "status").or_else(|| {
                    Some(
                        match kind {
                            "workflow_failed" => "failed",
                            "workflow_cancelled" => "cancelled",
                            _ => "succeeded",
                        }
                        .to_owned(),
                    )
                });
                self.cause = text(f, "cause");
                self.elapsed_ms = count(f, "elapsed_ms");
                self.priced_calls = count(f, "priced_calls");
                self.unpriced_calls = count(f, "unpriced_calls");
                self.cost_qualifier = text(f, "cost_qualifier");
                self.total_cost_usd = f.get("total_cost_usd").and_then(Value::as_f64);
                // The terminal frame keeps the outputs as their compact JSON text.
                self.outputs = match f.get("outputs") {
                    Some(Value::String(json)) => serde_json::from_str(json).ok(),
                    Some(Value::Object(map)) => Some(map.clone()),
                    _ => None,
                };
            }
            "run_sealed" => self.absorb_seal(f),
            _ => {}
        }
    }

    fn absorb_task(&mut self, kind: &str, f: &BTreeMap<String, Value>) {
        let Some(id) = text(f, "task") else {
            return;
        };
        let note = text(f, "note");
        let duration = count(f, "duration_ms");
        let detail = text(f, "detail");
        let output = (kind == "task_completed")
            .then(|| completed_output(f))
            .flatten();
        if let Some(reason) = text(f, "cost_unpriced") {
            self.unpriced_reasons.insert(reason);
        }
        let task = self.task_mut(&id);
        if let Some(note) = note
            && kind != "task_cache_hit"
        {
            task.note = note;
        }
        if duration.is_some() {
            task.duration_ms = duration;
        }
        if detail.is_some() {
            task.detail = detail;
        }
        if output.is_some() {
            task.output = output;
        }
        if kind == "task_started" {
            task.started = text(f, "note");
        }
        task.state = match kind {
            "task_started" => TaskState::Running,
            "task_completed" => TaskState::Ok,
            "task_failed" => TaskState::Failed,
            "task_recovered" => TaskState::Recovered,
            "task_skipped" => TaskState::Skipped,
            "task_cancelled" => TaskState::Cancelled,
            "task_cache_hit" => TaskState::CacheHit,
            _ => task.state,
        };
    }

    fn absorb_seal(&mut self, f: &BTreeMap<String, Value>) {
        let covers = text(f, "covers")
            .and_then(|c| serde_json::from_str::<Value>(&c).ok())
            .unwrap_or(Value::Null);
        let effects = covers.get("effects");
        let constant =
            |v: Option<&Value>| v.and_then(|x| x.get("constant")).and_then(Value::as_u64);
        self.seal = Some(Seal {
            head: covers
                .get("head")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            events: covers.get("events").and_then(Value::as_u64),
            key_id: text(f, "key_id").unwrap_or_default(),
            alg: text(f, "alg").unwrap_or_default(),
            declared: constant(effects.and_then(|e| e.get("declared"))),
            exercised: effects
                .and_then(|e| e.get("exercised"))
                .and_then(Value::as_u64),
            escapes: effects
                .and_then(|e| e.get("escapes"))
                .and_then(Value::as_u64),
        });
    }

    /// The paths an allowed `permits.fs.<verb> <path>` decision names.
    fn fs_paths(&self, verb: &str) -> Vec<String> {
        let prefix = format!("permits.fs.{verb} ");
        let mut paths: Vec<String> = self
            .permits
            .iter()
            .filter(|p| p.plane == "fs" && p.decision == "allow")
            .filter_map(|p| p.gate.strip_prefix(&prefix).map(str::to_owned))
            .collect();
        paths.dedup();
        paths
    }

    /// The paths an allowed read checked for a task that failed and was recovered, and that no
    /// task which completed read: nothing of them was read.
    fn recovered_reads(&self) -> Vec<String> {
        let tasks_in = |state: TaskState| -> Vec<&str> {
            (self.tasks.iter())
                .filter(|t| t.state == state)
                .map(|t| t.id.as_str())
                .collect()
        };
        let (recovered, read) = (tasks_in(TaskState::Recovered), tasks_in(TaskState::Ok));
        let gate = |p: &PermitFact| p.gate.strip_prefix("permits.fs.read ").map(str::to_owned);
        let allowed = |p: &&PermitFact| p.plane == "fs" && p.decision == "allow";
        let read_ok: Vec<String> = (self.permits.iter().filter(allowed))
            .filter(|p| read.contains(&p.task.as_str()))
            .filter_map(gate)
            .collect();
        (self.permits.iter().filter(allowed))
            .filter(|p| recovered.contains(&p.task.as_str()))
            .filter_map(gate)
            .filter(|path| !read_ok.contains(path))
            .collect()
    }

    /// Whether an allowed write of `path` was checked for one of the `tasks` named.
    fn written_by(&self, path: &str, tasks: &[String]) -> bool {
        let gate = format!("permits.fs.write {path}");
        self.permits.iter().any(|p| {
            p.plane == "fs" && p.decision == "allow" && p.gate == gate && tasks.contains(&p.task)
        })
    }

    /// The paths this run completed writing: an `invoke · nika:write` task that a start frame
    /// opened and a successful completion settled `Ok` (never recovered, replayed from a cache
    /// or skipped), whose returned path the same task's allowed `permits.fs.write` decision
    /// names, compared without a leading `./`.
    /// A path, not its bytes: what it holds now is the caller's to read again.
    #[must_use]
    pub fn completed_writes(&self) -> Vec<String> {
        let bare = |path: &str| path.strip_prefix("./").unwrap_or(path).to_owned();
        let granted = |task: &str, path: &str| {
            self.permits.iter().any(|p| {
                p.task == task
                    && p.plane == "fs"
                    && p.decision == "allow"
                    && p.gate.strip_prefix("permits.fs.write ").map(bare) == Some(bare(path))
            })
        };
        let mut paths: Vec<String> = (self.tasks.iter())
            .filter(|t| {
                t.started.as_deref() == Some("invoke · nika:write") && t.state == TaskState::Ok
            })
            .filter_map(|t| t.output.as_deref().filter(|out| granted(&t.id, out)))
            .map(str::to_owned)
            .collect();
        paths.dedup();
        paths
    }

    /// Recorded network permissions and notification completions. A permit
    /// is permission, not delivery, and this list does not cover model traffic.
    fn sent(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .permits
            .iter()
            .filter(|p| p.plane == "net" && p.decision == "allow")
            .map(|p| {
                format!(
                    "{} · access allowed (permission, not delivery proof)",
                    p.gate
                )
            })
            .collect();
        for t in &self.tasks {
            if t.state == TaskState::Ok && t.note.contains("nika:notify") {
                out.push(format!(
                    "`{}` · nika:notify · MEASURED: the task completed (the receiver's side is not proven)",
                    t.id
                ));
            }
        }
        out.dedup();
        out
    }

    /// The models asked, from the started frames of `infer` tasks.
    fn asked(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.state,
                    TaskState::Ok | TaskState::Failed | TaskState::Recovered
                )
            })
            .filter_map(|t| t.note.strip_prefix("infer · ").map(str::to_owned))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// What the run cost, its legs a gate split included, and why any call stayed unpriced, said
    /// in French when `french`.
    fn cost_line_in(&self, french: bool) -> String {
        // A run a gate split is one run: its earlier legs' calls count with this one's.
        let before = &self.before;
        let priced = self.priced_calls.unwrap_or(0) + before.priced_calls;
        let unpriced = self.unpriced_calls.unwrap_or(0) + before.unpriced_calls;
        let total = summed(self.total_cost_usd, before.total_cost_usd);
        if french {
            return self.cost_line_fr(total, priced, unpriced);
        }
        let mut line = match (total, priced, unpriced) {
            (Some(usd), p, _) => format!(
                "cost · ${usd:.4} · {p} priced call(s) · recorded estimate, invoice not verified"
            ),
            (None, 0, 0) => "cost · no model usage recorded".to_owned(),
            (None, 0, u) => format!(
                "cost · UNKNOWN · {u} unpriced call(s): {}",
                self.why_unpriced()
            ),
            (None, p, _) => {
                format!("cost · UNKNOWN · {p} priced call(s) journaled without a total")
            }
        };
        if unpriced > 0 && priced > 0 {
            let _ = write!(
                line,
                " · {unpriced} unpriced call(s): {}",
                self.why_unpriced()
            );
        }
        if before.legs > 0 {
            let legs = before.legs + 1;
            let _ = write!(line, " · over its {legs} legs, a gate split the run");
        }
        line
    }

    /// The cost line in French: the same facts, the unpriced reasons as the engine names them.
    fn cost_line_fr(&self, total: Option<f64>, priced: u64, unpriced: u64) -> String {
        let s = |n: u64| if n > 1 { "s" } else { "" };
        let mut line = match (total, priced, unpriced) {
            (Some(usd), p, _) => format!(
                "coût · {} $ · {p} appel{} tarifé{} · estimation enregistrée, facture non vérifiée",
                format!("{usd:.4}").replace('.', ","),
                s(p),
                s(p)
            ),
            (None, 0, 0) => "coût · aucun appel de modèle enregistré".to_owned(),
            (None, 0, u) => format!(
                "coût · INCONNU · {u} appel{} non tarifé{} : {}",
                s(u),
                s(u),
                self.why_unpriced()
            ),
            (None, p, _) => format!(
                "coût · INCONNU · {p} appel{} tarifé{} journalisé{} sans total",
                s(p),
                s(p),
                s(p)
            ),
        };
        if unpriced > 0 && priced > 0 {
            let _ = write!(
                line,
                " · {unpriced} appel{} non tarifé{} : {}",
                s(unpriced),
                s(unpriced),
                self.why_unpriced()
            );
        }
        if self.before.legs > 0 {
            let legs = self.before.legs + 1;
            let _ = write!(line, " · en {legs} parties, une validation a coupé le run");
        }
        line
    }

    /// Why the unpriced calls have no price, from the reasons their frames name. Live gauntlet,
    /// 2026-10-16: every unpriced call read « a local model is unpriced » — said of `OpenRouter`
    /// and Gemini routes, remote and billed.
    fn why_unpriced(&self) -> String {
        let all: BTreeSet<&String> = (self.unpriced_reasons.iter())
            .chain(&self.before.unpriced_reasons)
            .collect();
        if all.is_empty() {
            return "a route with no price table, or usage its provider did not report (never free)"
                .to_owned();
        }
        let reasons: Vec<&str> = (all.into_iter())
            .map(|reason| match reason.as_str() {
                "local_model" => "a local model is unpriced, never free",
                "mock_provider" => "the mock provider is unpriced",
                "missing_catalog_price" => "no verified tariff for its route (never free)",
                "provider_did_not_report_usage" => {
                    "its provider reported no complete usage (spent, never free)"
                }
                "subscription_quota" => "a subscription seat draws quota, not metered dollars",
                "usage_rejected" => {
                    "the usage it reported contradicted the admitted request (spent, never free)"
                }
                other => other,
            })
            .collect();
        reasons.join(" · ")
    }

    /// The result view, once a run ended (exit 0 or 1).
    #[must_use]
    pub fn result(&self, root: &Path, workflow: &Path) -> String {
        self.result_in(root, workflow, false)
    }

    /// [`Self::result`], a stop by the ceiling said in French when `french`.
    #[must_use]
    pub fn result_in(&self, root: &Path, workflow: &Path, french: bool) -> String {
        if self.stopped_by_ceiling() {
            return self.ceiling_stop(workflow, french);
        }
        self.ended(root, workflow, french)
    }

    /// The view of a run that ended on its own: its failure, or what it did and produced, said
    /// in French when `french`.
    fn ended(&self, root: &Path, workflow: &Path, french: bool) -> String {
        if self.status.as_deref() == Some("failed")
            || self.tasks.iter().any(|t| t.state == TaskState::Failed)
        {
            return self.failure(workflow, french);
        }
        let say = |en: &'static str, fr: &'static str| if french { fr } else { en };
        let ran = self
            .tasks
            .iter()
            .filter(|t| t.state == TaskState::Ok)
            .count();
        let cached = self
            .tasks
            .iter()
            .filter(|t| t.state == TaskState::CacheHit)
            .count();
        let skipped = self
            .tasks
            .iter()
            .filter(|t| t.state == TaskState::Skipped)
            .count();
        // A run whose gate the person refused ended where their answer stopped it, never as a
        // plain success (live gauntlet r04 B9: « succeeded » after a « non »).
        let refused = self.refused_at_gate();
        let word = match (self.status.as_deref(), refused) {
            (Some("succeeded") | None, true) => say(
                "Done · stopped at your no",
                "Terminé · arrêté à votre « non »",
            ),
            (Some("succeeded") | None, false) => say("Done", "Terminé"),
            (Some("cancelled"), _) => say("Cancelled", "Annulé"),
            (Some(other), _) => other,
        };
        let mut view = format!("{word} · `{}`", workflow.display());
        if let Some(ms) = self.elapsed_ms {
            let _ = write!(view, " · {}", human_ms(ms));
        }
        if french {
            let s = if ran > 1 { "s" } else { "" };
            let _ = write!(view, " · {ran} étape{s} exécutée{s}");
        } else {
            let _ = write!(view, " · {ran} task{} ran", plural(ran));
        }
        if cached > 0 {
            let _ = write!(view, " ({cached} {})", say("from cache", "depuis le cache"));
        }
        if skipped > 0 {
            if french {
                let s = if skipped > 1 { "s" } else { "" };
                let _ = write!(view, " · {skipped} sautée{s}");
            } else {
                let _ = write!(view, " · {skipped} skipped");
            }
        }
        for line in self.written_lines(root, workflow, french) {
            let _ = write!(view, "\n  {line}");
        }
        for line in self.reached_lines(french) {
            let _ = write!(view, "\n  {line}");
        }
        for line in self.sent_lines(root, workflow, french) {
            let _ = write!(view, "\n  {line}");
        }
        // What the workflow declared as its result, said where the person reads the run (live
        // gauntlet r08 and r03, 2026-10-16: the totals and the report message were only in a
        // side panel's JSON).
        for (key, value) in self.outputs.iter().flatten() {
            let _ = write!(
                view,
                "\n  {} · {key} · {}",
                say("output", "résultat"),
                shown_output(value, french)
            );
        }
        let _ = write!(view, "\n  {}", self.cost_line_in(french));
        if let Some(ceiling) = self.ceiling_usd {
            let stated = nika_display::vocab::stated_usd(ceiling);
            if french {
                let _ = write!(
                    view,
                    "\n  plafond · {} $ · le plus que ce run pouvait coûter",
                    stated.replace('.', ",")
                );
            } else {
                let _ = write!(
                    view,
                    "\n  ceiling · ${stated} · the most this run was allowed to cost"
                );
            }
        }
        view.push_str(say(
            "\n  `/proof` shows the records and their limits · « run it » runs it again",
            "\n  `/proof` montre les preuves et leurs limites · « lance-le » le relance",
        ));
        view
    }

    /// What the run read, sent, asked of a model and was answered at its gates, said in French
    /// when `french`.
    fn reached_lines(&self, french: bool) -> Vec<String> {
        let say = |en: &'static str, fr: &'static str| if french { fr } else { en };
        let mut lines = Vec::new();
        // A read whose task failed and was recovered read nothing (live gauntlet r02: the first
        // run's « read · ./python-version.txt » for a file that did not exist yet).
        let unread = self.recovered_reads();
        for path in self.fs_paths("read") {
            if unread.contains(&path) {
                let said = say(
                    "not there · {path} · nothing was read",
                    "absent · {path} · rien n'a été lu",
                );
                lines.push(said.replace("{path}", &path));
            } else {
                lines.push(format!("{} · {path}", say("read", "lu")));
            }
        }
        for target in &self.sent() {
            lines.push(format!(
                "{} · {target}",
                say("network / notification", "réseau / notification")
            ));
        }
        for model in self.asked() {
            lines.push(format!("{} · {model}", say("asked", "modèle appelé")));
        }
        for a in &self.approvals {
            let verdict = if a.decision == "allow" {
                say(
                    "approved · your answer let it go on",
                    "approuvé · votre réponse l'a laissé continuer",
                )
            } else {
                say(
                    "refused · your answer stopped it there",
                    "refusé · votre réponse l'a arrêté là",
                )
            };
            let by = if a.source == "resume" {
                say("answered in this session", "répondu dans cette session")
            } else {
                &a.source
            };
            lines.push(format!("{verdict} · `{}` · {by}", a.task));
        }
        lines
    }

    /// What became of each step of the workflow that sends beyond this machine (a notification,
    /// a request other than GET): sent when it completed (the receiver's side unproven), or not
    /// sent and why, said in French when `french` (live gauntlet r02 B13, 2026-10-17: the report
    /// never said whether the webhook was pinged).
    fn sent_lines(&self, root: &Path, workflow: &Path, french: bool) -> Vec<String> {
        let say = |en: &'static str, fr: &'static str| if french { fr } else { en };
        let mut lines = Vec::new();
        for id in sending_tasks(root, workflow) {
            let state = (self.tasks.iter()).find(|t| t.id == id).map(|t| t.state);
            let (sent, why) = match state {
                Some(TaskState::Ok | TaskState::Recovered) => (
                    true,
                    say(
                        "the step completed (the receiver's side is not proven)",
                        "l'étape s'est terminée (le côté du destinataire n'est pas prouvé)",
                    ),
                ),
                Some(TaskState::Skipped) => (
                    false,
                    say("its condition was false", "sa condition était fausse"),
                ),
                Some(TaskState::Cancelled) => (
                    false,
                    say(
                        "cancelled: a step before it did not go on",
                        "annulé : une étape avant lui n'a pas continué",
                    ),
                ),
                Some(TaskState::Failed) => (false, say("it failed", "il a échoué")),
                Some(TaskState::CacheHit) => (
                    false,
                    say(
                        "served from the cache: nothing was sent again",
                        "servi depuis le cache : rien n'a été renvoyé",
                    ),
                ),
                _ => (
                    false,
                    say("the run ended before it", "le run s'est arrêté avant lui"),
                ),
            };
            let word = if sent {
                say("sent", "envoyé")
            } else {
                say("not sent", "pas envoyé")
            };
            lines.push(format!("{word} · `{id}` · {why}"));
        }
        lines
    }

    /// What became of each file the run may write: produced (with its size), removed, or not
    /// there now, said in French when `french`.
    fn written_lines(&self, root: &Path, workflow: &Path, french: bool) -> Vec<String> {
        let say = |en: &'static str, fr: &'static str| if french { fr } else { en };
        let mut lines = Vec::new();
        let removals = removal_tasks(root, workflow);
        let written: Vec<String> = (self.completed_writes().iter())
            .map(|path| path.strip_prefix("./").unwrap_or(path).to_owned())
            .collect();
        for path in self.fs_paths("write") {
            let file = std::fs::metadata(root.join(&path))
                .ok()
                .filter(std::fs::Metadata::is_file);
            // A path the run may write but that is not there now was not produced: the run removed
            // it (live gauntlet q05: three inbox files moved away read « produced »), or another
            // step did.
            let produced = say("produced", "produit");
            let line = match file {
                Some(meta) => format!("{produced} · {path} ({})", human_size(meta.len())),
                // The run's own completed write, read on a disk where the file is not.
                None if written
                    .iter()
                    .any(|w| w == path.strip_prefix("./").unwrap_or(&path)) =>
                {
                    format!("{produced} · {path}")
                }
                None if self.written_by(&path, &removals) => {
                    format!("{} · {path}", say("removed", "retiré"))
                }
                None => format!("{} · {path}", say("not there now", "absent maintenant")),
            };
            lines.push(line);
        }
        lines
    }

    /// The view of a run its ceiling stopped: what it spent against the ceiling, what never ran,
    /// and what to do, which is never a repair of the workflow (live gauntlet r09, 2026-10-17:
    /// « repair the workflow, then « run it » again » for a run that only cost more than its
    /// ceiling).
    fn ceiling_stop(&self, workflow: &Path, french: bool) -> String {
        let usd = |cost: f64| format!("{cost:.4}");
        let spent = self.spent_usd().map(usd);
        let ceiling = self.ceiling_usd.map(usd);
        let never: Vec<&str> = (self.tasks.iter())
            .filter(|t| matches!(t.state, TaskState::Scheduled | TaskState::Cancelled))
            .map(|t| t.id.as_str())
            .collect();
        let shown = workflow.display();
        let mut view = match (french, spent, ceiling) {
            (true, Some(spent), Some(ceiling)) => format!(
                "Arrêté · `{shown}` · le plafond est atteint : {} $ dépensés pour un plafond de {} $",
                spent.replace('.', ","),
                ceiling.replace('.', ",")
            ),
            (true, _, _) => format!("Arrêté · `{shown}` · le plafond est atteint"),
            (false, Some(spent), Some(ceiling)) => format!(
                "Stopped · `{shown}` · the ceiling was reached: ${spent} spent for a ${ceiling} ceiling"
            ),
            (false, _, _) => format!("Stopped · `{shown}` · the ceiling was reached"),
        };
        view.push_str(if french {
            "
  l'appel déjà parti a été terminé et compté · les étapes suivantes n'ont pas été lancées"
        } else {
            "
  the call already under way finished and was counted · the steps after it were not started"
        });
        if !never.is_empty() {
            let label = if french {
                "pas lancées"
            } else {
                "not started"
            };
            let _ = write!(
                view,
                "
  {label} · {}",
                never.join(" · ")
            );
        }
        view.push_str(if french {
            "
  le workflow n'est pas en cause · relevez le plafond (« lance-le avec un plafond de … $ ») ou rendez-le moins cher (un modèle moins cher, moins de texte en sortie)"
        } else {
            "
  the workflow is not at fault · raise the ceiling (« run it with a ceiling of $… ») or make it cheaper (a cheaper model, less output)"
        });
        view
    }

    /// The failure view: which task failed, what ran before it, said in French when `french`.
    fn failure(&self, workflow: &Path, french: bool) -> String {
        let say = |en: &'static str, fr: &'static str| if french { fr } else { en };
        let mut view = format!("{} · `{}`", say("Failed", "Échoué"), workflow.display());
        if let Some(t) = self.tasks.iter().find(|t| t.state == TaskState::Failed) {
            let _ = write!(view, " · `{}` {}", t.id, say("failed", "a échoué"));
            if let Some(detail) = &t.detail {
                let _ = write!(view, "\n  {}", code_once(first_line(detail)));
            }
        }
        let before: Vec<String> = self
            .tasks
            .iter()
            .filter(|t| matches!(t.state, TaskState::Ok | TaskState::CacheHit))
            .map(|t| match t.duration_ms {
                Some(ms) => format!("{} ({})", t.id, human_ms(ms)),
                None => t.id.clone(),
            })
            .collect();
        if before.is_empty() {
            view.push_str(say(
                "\n  nothing completed before it",
                "\n  rien n'a abouti avant",
            ));
        } else {
            let _ = write!(
                view,
                "\n  {} · {}",
                say("ran before it", "exécutées avant"),
                before.join(" · ")
            );
        }
        let never: Vec<&str> = self
            .tasks
            .iter()
            .filter(|t| matches!(t.state, TaskState::Scheduled | TaskState::Cancelled))
            .map(|t| t.id.as_str())
            .collect();
        if !never.is_empty() {
            let _ = write!(
                view,
                "\n  {} · {}",
                say("never ran", "jamais lancées"),
                never.join(" · ")
            );
        }
        let _ = write!(view, "\n  {}", self.cost_line_in(french));
        view.push_str(say(
            "\n  the trace holds the failure · `/proof` reads it · repair the workflow if the cause is its own, then « run it » again",
            "\n  la trace garde l'échec · `/proof` la lit · corrigez le workflow si la cause est la sienne, puis « lance-le » à nouveau",
        ));
        view
    }

    /// The gate view, when a run paused: so far · the question · what a
    /// yes lets happen (`gated`, from the workflow's bytes) · what a no does.
    #[must_use]
    pub fn gate(&self, workflow: &Path, message: &str, mode: &str, gated: &[String]) -> String {
        self.gate_in(workflow, message, mode, gated, false)
    }

    /// [`Self::gate`], said in French when `french`: the person's words around the gate's own
    /// question, which stays as the workflow wrote it.
    #[must_use]
    pub fn gate_in(
        &self,
        workflow: &Path,
        message: &str,
        mode: &str,
        gated: &[String],
        french: bool,
    ) -> String {
        let say = |en: &'static str, fr: &'static str| if french { fr } else { en };
        let mut view = if french {
            format!(
                "En pause · `{}` vous demande avant de continuer",
                workflow.display()
            )
        } else {
            format!(
                "Paused · `{}` asks you before it goes on",
                workflow.display()
            )
        };
        let so_far: Vec<String> = self
            .tasks
            .iter()
            .filter(|t| matches!(t.state, TaskState::Ok | TaskState::CacheHit))
            .map(|t| match (t.note.is_empty(), t.duration_ms) {
                (false, Some(ms)) => format!("{} · {} ({})", t.id, t.note, human_ms(ms)),
                (false, None) => format!("{} · {}", t.id, t.note),
                (true, _) => t.id.clone(),
            })
            .collect();
        let so_far_lead = say("so far", "jusqu'ici");
        if so_far.is_empty() {
            let nothing = say(
                "nothing has run before the gate",
                "rien n'a tourné avant la validation",
            );
            let _ = write!(view, "\n  {so_far_lead} · {nothing}");
        } else {
            let _ = write!(view, "\n  {so_far_lead} · {}", so_far.join(" · "));
        }
        if let Some(again) = live_again_in(&self.trace, french) {
            let _ = write!(view, "\n  {again}");
        }
        let _ = write!(view, "\n  « {message} »");
        let after = if mode == "choice" {
            say("after your choice", "après votre choix")
        } else {
            say("a yes lets happen", "un oui permet")
        };
        if gated.is_empty() {
            let tasks = say(
                "the tasks after this gate (the workflow's bytes name them)",
                "les tâches après cette validation (le workflow les nomme)",
            );
            let _ = write!(view, "\n  {after} · {tasks}");
        } else {
            let _ = write!(view, "\n  {after} · {}", gated.join(" · "));
        }
        let how = match mode {
            "confirm" => say("yes or no", "oui ou non"),
            "choice" => say(
                "one of the choices, as written",
                "l'un des choix, tel qu'écrit",
            ),
            _ => say("in words", "en mots"),
        };
        let outcome = if mode == "choice" {
            say(
                "the chosen value resumes the run",
                "la valeur choisie reprend le run",
            )
        } else {
            say("a no ends the run there", "un non arrête le run ici")
        };
        let tail = if french {
            format!(
                "\n  {outcome} · rien après la validation n'a encore eu lieu\n  répondez {how} · « pourquoi ? » explique · rien ne répond à votre place"
            )
        } else {
            format!(
                "\n  {outcome} · nothing after the gate has happened yet\n  answer {how} · « why? » explains · nothing answers for you"
            )
        };
        view.push_str(&tail);
        view
    }

    /// The proof view: the chain verdict from the ONE verify door, the
    /// workflow's identity, the boundary, the digests, the limits.
    #[must_use]
    pub fn proof(&self, root: &Path) -> String {
        let shown = self.trace.strip_prefix(root).unwrap_or(&self.trace);
        let mut view = format!(
            "Proof · `{}` · what this journal records, hash-chained line by line",
            shown.display()
        );
        let _ = write!(
            view,
            "\n  workflow · {} · bytes sha256 {} · meaning {}",
            self.workflow.as_deref().unwrap_or("(unnamed)"),
            self.workflow_sha256
                .as_deref()
                .map_or_else(|| "(absent)".to_owned(), short),
            self.semantic_hash
                .as_deref()
                .map_or_else(|| "(absent)".to_owned(), short)
        );
        let (chain, seal) = chain_verdict(&self.trace);
        let _ = write!(view, "\n  {chain}");
        if !seal.is_empty() {
            let _ = write!(view, "\n  {seal}");
        }
        let allowed = self
            .permits
            .iter()
            .filter(|p| p.decision == "allow")
            .count();
        let denied = self.permits.len() - allowed;
        let mut boundary = String::from("boundary ·");
        if let Some(s) = &self.seal {
            if let (Some(d), Some(e)) = (s.declared, s.exercised) {
                let _ = write!(boundary, " {d} effect(s) declared · {e} exercised");
            }
            if let Some(esc) = s.escapes {
                let _ = write!(boundary, " · {esc} escaped");
            }
            boundary.push_str(" ·");
        }
        let _ = write!(
            boundary,
            " {} permit check(s) · {allowed} allowed · {denied} denied",
            self.permits.len()
        );
        let _ = write!(view, "\n  {boundary}");
        for path in self.fs_paths("write") {
            let line = match std::fs::read(root.join(&path)) {
                Ok(bytes) => format!(
                    "written · {path} · {} · sha256 {} · RE-READ: the file as it is now, not as it was written",
                    human_size(bytes.len() as u64),
                    short(&nika_event::source_id::sha256_hex(&bytes))
                ),
                Err(_) => format!("written · {path} · not on disk now"),
            };
            let _ = write!(view, "\n  {line}");
        }
        for a in &self.approvals {
            let _ = write!(
                view,
                "\n  approval · `{}` · {} · source {}",
                a.task, a.decision, a.source
            );
        }
        if let (Some(engine), Some(sandbox)) = (&self.engine_version, &self.sandbox) {
            let _ = write!(view, "\n  engine · {engine} · sandbox {sandbox}");
        }
        view.push_str(
            "\n  records · task outcomes, permit decisions, durations, and any task input/result hashes present (the journal's encoding, not a file checksum)",
        );
        let _ = write!(view, "\n  {}", attestation(&self.trace).limits());
        view.push_str(
            "\n  `nika trace verify <trace>` re-judges the chain · `nika trace outputs <trace>` prints every output",
        );
        view
    }
}

/// How far the verify door's own typed verdict lets these records be
/// trusted: its `--json` projection (the attained tier, the seal tier, the
/// chain headline), never a reading of its prose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attestation {
    /// Sealed, and the head is notarized or replayed beyond this machine.
    Anchored,
    /// Sealed: the run key signed the journal.
    Signed,
    /// Chain intact, no seal: tamper-evident, not attributable.
    Unsigned,
    /// Chain intact; the seal names a key this machine cannot check.
    KeyElsewhere,
    /// A broken or unchained journal, a forged or buried seal, a torn or
    /// incomplete journal, or no verdict: nothing here is attested.
    NotAttested,
}

impl Attestation {
    /// The trust line and the limits line of the proof view.
    fn limits(self) -> &'static str {
        match self {
            Self::Anchored => {
                "trust · signed by the run key, the head notarized beyond this machine\n  does not prove · that the content is right (read it)"
            }
            Self::Signed => {
                "trust · signed by the run key: a rewritten journal needs that key\n  does not prove · that the content is right (read it) · that anyone outside this machine trusts the key (`nika trace anchor` notarizes the head)"
            }
            Self::Unsigned => {
                "trust · unsigned: the hash chain is internally consistent; a journal rewritten end to end would chain too\n  does not prove · that the content is right (read it) · who wrote this journal (`nika sign` signs future runs)"
            }
            Self::KeyElsewhere => {
                "trust · the chain holds; the seal names a key this machine cannot check\n  does not prove · that the content is right (read it) · who signed it, until that key is checked"
            }
            Self::NotAttested => {
                "trust · the journal does not verify: these records are not attested (`nika trace verify <trace>` says why)\n  does not prove · any of the records above"
            }
        }
    }
}

/// The verify door's typed verdict for one journal (its `--json`
/// projection): the same ONE judge as the prose chain line, read as data.
fn attestation(trace: &Path) -> Attestation {
    let options = crate::trace_verify::VerifyOptions {
        json: true,
        ..Default::default()
    };
    let out = crate::trace_verify::verify_with(&trace.display().to_string(), &options);
    serde_json::from_str::<Value>(out.text.trim())
        .map_or(Attestation::NotAttested, |doc| Attestation::of(&doc))
}

impl Attestation {
    /// The verdict a verify document carries: the CLI's `--json`, or a
    /// door's projection of it (`door_verdict` keeps its `tier`, `chain` and
    /// `seal`). Pure: the one reading every view and every kept run makes.
    fn of(doc: &Value) -> Self {
        let word = |outer: &str, inner: &str| {
            doc.get(outer)
                .and_then(|v| v.get(inner))
                .and_then(Value::as_str)
        };
        if word("chain", "headline") != Some("intact") {
            return Self::NotAttested;
        }
        match (
            doc.get("tier").and_then(Value::as_str),
            word("seal", "tier"),
        ) {
            (Some("anchored" | "replayed"), Some("sealed")) => Self::Anchored,
            (Some("sealed"), Some("sealed")) => Self::Signed,
            (Some("ok"), Some("unsealed")) => Self::Unsigned,
            (_, Some("unattributable")) => Self::KeyElsewhere,
            _ => Self::NotAttested,
        }
    }

    /// The run key signed the journal and its signature verifies here.
    fn signed(self) -> bool {
        matches!(self, Self::Anchored | Self::Signed)
    }
}

/// The chain and seal lines `nika trace verify` says, through the ONE
/// judge — never a second chain walker. (chain line, seal line). The
/// judge's first line is the chain's verdict whatever its exit code: a
/// machine without the signing key (CI, another operator) cannot judge
/// the SEAL and exits 3, while the chain it walked is still intact.
fn chain_verdict(trace: &Path) -> (String, String) {
    let out = crate::trace_verify::verify(&trace.display().to_string());
    let lines: Vec<&str> = out
        .text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let first = shorten_hex(lines.first().copied().unwrap_or(""));
    let chain = if first.is_empty() {
        format!("chain · not judged (verify exit {})", out.code)
    } else {
        format!("chain · {first}")
    };
    let seal = lines
        .iter()
        .find(|l| {
            l.starts_with("SEALED")
                || l.starts_with("UNSEALED")
                || l.starts_with("INCOMPLETE")
                || l.starts_with("BROKEN")
                || l.starts_with("TORN")
        })
        .map_or_else(
            || {
                if out.code == 0 {
                    String::new()
                } else {
                    format!(
                        "seal · not judged on this machine (verify exit {}) · `nika trace verify` says why",
                        out.code
                    )
                }
            },
            |l| format!("seal · {}", shorten_hex(l)),
        );
    (chain, seal)
}

/// The judge's line with every 64-hex digest shortened for the eye
/// (`nika trace verify <trace>` prints them whole).
fn shorten_hex(line: &str) -> String {
    line.split(' ')
        .map(|word| {
            if word.len() == 64 && word.chars().all(|c| c.is_ascii_hexdigit()) {
                short(word)
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `5d1bf591…0730` — a hex digest a human can compare by eye.
fn short(hex: &str) -> String {
    if hex.len() > 14 {
        format!("{}…{}", &hex[..8], &hex[hex.len() - 4..])
    } else {
        hex.to_owned()
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// `11 ms` · `1.2 s` · `2 min 05 s`.
#[allow(clippy::cast_precision_loss)] // display-only: a duration shown to a human
fn human_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", ms as f64 / 1000.0)
    } else {
        format!("{} min {:02} s", ms / 60_000, (ms % 60_000) / 1000)
    }
}

/// The completed tasks a resume of the paused journal `trace` is sure to run again, live (C10 ·
/// Q8): each completion its resume plan does not carry (no resume identity, or an output that
/// does not read back), judged by the resume's own fold ([`nika_dap::resume::fold_plan`]), in
/// journal order, each named once. Empty when the plan carries every completion, or when the
/// journal cannot be recovered at all (a resume refuses it then). Empty promises nothing more:
/// the run serves a carried completion only while its definition and inputs are unchanged.
#[must_use]
pub fn resumed_live(trace: &Path) -> Vec<String> {
    let label = trace.display().to_string();
    let Some(recovered) = std::fs::read_to_string(trace)
        .ok()
        .and_then(|raw| nika_dap::recover::recover_events(&raw, &label).ok())
    else {
        return Vec::new();
    };
    let plan = nika_dap::resume::fold_plan(&recovered.events).plan;
    let mut live: Vec<String> = Vec::new();
    for event in &recovered.events {
        if let (EventKind::TaskCompleted | EventKind::TaskCacheHit, Some(FieldValue::String(task))) =
            (&event.kind, event.field("task"))
            && !plan.contains_key(task)
            && !live.contains(task)
        {
            live.push(task.clone());
        }
    }
    live
}

/// The line a host says before the answer that resumes `trace`, naming [`resumed_live`]:
/// `None` when the plan carries every completion (nothing is said then, and nothing promised).
#[must_use]
pub fn live_again(trace: &Path) -> Option<String> {
    live_again_in(trace, false)
}

/// [`live_again`], said in French when `french`.
#[must_use]
pub fn live_again_in(trace: &Path, french: bool) -> Option<String> {
    let live = resumed_live(trace);
    (!live.is_empty()).then(|| {
        if french {
            format!(
                "toute réponse reprend le run, et ces tâches terminées tournent à nouveau, en direct · {} (le journal ne peut pas les resservir)",
                live.join(" · ")
            )
        } else {
            format!(
                "any answer resumes the run, and these completed tasks run again, live · {} (the journal cannot serve them back)",
                live.join(" · ")
            )
        }
    })
}

/// A byte count a human reads (`1.2 KB`, `340 B`).
#[allow(clippy::cast_precision_loss)] // display-only: a size shown to a human, never computed with
#[must_use]
pub fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// What a green run of `workflow` left behind under `root`: the files the
/// workflow's own boundary lets it write (`permits.fs.write`, literal paths
/// only) that exist there now, with their sizes. The boundary is the claim;
/// the file on disk is the evidence; a glob is not a file. Read now, never the
/// bytes the run wrote.
#[must_use]
pub fn produced(root: &Path, workflow: &Path) -> Option<String> {
    let source = std::fs::read_to_string(root.join(workflow)).ok()?;
    let wf = nika_schema::parse(
        &source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .ok()?;
    let writes = wf.permits.as_ref()?.value.fs.as_ref()?.write.clone();
    let mut produced = Vec::new();
    for path in writes {
        if path.contains(['*', '?', '[']) {
            continue;
        }
        let Ok(meta) = std::fs::metadata(root.join(&path)) else {
            continue;
        };
        if meta.is_file() {
            produced.push(format!("{path} ({})", human_size(meta.len())));
        }
    }
    (!produced.is_empty()).then(|| format!("produced · {}", produced.join(" · ")))
}

/// A declared output as the run report says it: a string's one line, or its first and its last
/// line with their count when it holds more (a journal a run appends to ends with what this run
/// wrote: live gauntlet wave 6 r08 B2, 2026-10-18, three runs read « résultat · journal · » the
/// first run's line), any other value as its compact JSON; each part cut with « … ».
fn shown_output(value: &Value, french: bool) -> String {
    let cut = |text: &str, at: usize| match text.char_indices().nth(at) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    };
    let Value::String(text) = value else {
        return cut(&value.to_string(), 160);
    };
    let lines: Vec<&str> = (text.lines())
        .filter(|line| !line.trim().is_empty())
        .collect();
    match lines.as_slice() {
        [] => String::new(),
        [only] => cut(only, 160),
        [first, .., last] => {
            let count = lines.len();
            let counted = if french {
                format!("{count} lignes")
            } else {
                format!("{count} lines")
            };
            format!("{} … {} ({counted})", cut(first, 80), cut(last, 80))
        }
    }
}

/// The tasks of `workflow` that remove a file (`nika:remove_file`), read from its bytes now.
fn removal_tasks(root: &Path, workflow: &Path) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(root.join(workflow)) else {
        return Vec::new();
    };
    let Ok(wf) = nika_schema::parse(
        &source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    ) else {
        return Vec::new();
    };
    (wf.tasks.iter())
        .filter(|task| match &task.value.action {
            nika_schema::raw::RawAction::Invoke(invoke) => invoke
                .tool()
                .is_some_and(|tool| tool.value == "nika:remove_file"),
            _ => false,
        })
        .map(|task| task.value.id.value.clone())
        .collect()
}

/// The tasks of the workflow at `root`/`workflow` that send beyond this machine, in its order: a
/// `nika:notify`, and a `nika:fetch` whose literal method is not GET.
fn sending_tasks(root: &Path, workflow: &Path) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(root.join(workflow)) else {
        return Vec::new();
    };
    let Ok(wf) = nika_schema::parse(
        &source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    ) else {
        return Vec::new();
    };
    (wf.tasks.iter())
        .filter(|task| {
            let nika_schema::raw::RawAction::Invoke(invoke) = &task.value.action else {
                return false;
            };
            let Some(tool) = invoke.tool() else {
                return false;
            };
            let method = (invoke.args.as_ref())
                .and_then(|args| args.value.get("method"))
                .and_then(Value::as_str);
            tool.value == "nika:notify"
                || (tool.value == "nika:fetch"
                    && method.is_some_and(|method| !method.eq_ignore_ascii_case("GET")))
        })
        .map(|task| task.value.id.value.clone())
        .collect()
}

/// In a git work tree (`git_root`) whose `.gitignore` does not keep
/// `.nika/traces/` out, a run's trace (model outputs · file contents · 0600)
/// would be one `git add` away from a commit: the note that says so.
#[must_use]
pub fn hygiene_note(git_root: Option<&Path>) -> Option<String> {
    let ignored = std::fs::read_to_string(git_root?.join(".gitignore"))
        .map(|text| {
            text.lines().any(|l| {
                l.trim().contains(".nika/traces") || l.trim() == ".nika" || l.trim() == ".nika/"
            })
        })
        .unwrap_or(false);
    (!ignored).then(|| {
        "runs write `.nika/traces/` (model outputs · file contents · mode 0600) — not ignored by git here · `nika init` adds the line, or add `.nika/traces/` to `.gitignore`".to_owned()
    })
}

mod join;
pub use join::{ComponentRun, ComponentTask};

mod kept;
pub use kept::KeptRun;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod identity_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod completed_write_tests;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod join_tests;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
