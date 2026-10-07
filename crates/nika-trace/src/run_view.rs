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
//! · [`live_again`]); the facts' fields and the per-task, permit, approval,
//! pause and seal facts stay crate-private.
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

use std::collections::BTreeMap;
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pause {
    pub(crate) task: String,
    pub(crate) message: String,
    pub(crate) mode: String,
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
    pub(crate) elapsed_ms: Option<u64>,
    pub(crate) priced_calls: Option<u64>,
    pub(crate) unpriced_calls: Option<u64>,
    pub(crate) cost_qualifier: Option<String>,
    pub(crate) total_cost_usd: Option<f64>,
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
/// text and not empty, the pause's defaults for the message and the mode.
fn first_gate(frame: &Value) -> Option<Pause> {
    let field = |key: &str| -> Option<String> {
        let row = frame
            .get("fields")?
            .as_array()?
            .iter()
            .find(|r| r.get("key").and_then(Value::as_str) == Some(key))?;
        row.get("value")?.as_str().map(str::to_owned)
    };
    Some(Pause {
        task: field("task").filter(|t| !t.is_empty())?,
        message: field("message").unwrap_or_else(|| "the run awaits your answer".to_owned()),
        mode: field("mode").unwrap_or_else(|| "text".to_owned()),
    })
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
    #[must_use]
    pub fn read(trace: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(trace).ok()?;
        Self::of(trace, &raw)
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
            if kind == "workflow_paused" && !paused {
                paused = true;
                facts.gate = first_gate(&frame);
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
                self.pause = Some(Pause {
                    task: text(f, "task").unwrap_or_default(),
                    message: text(f, "message")
                        .unwrap_or_else(|| "the run awaits your answer".to_owned()),
                    mode: text(f, "mode").unwrap_or_else(|| "text".to_owned()),
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
                self.elapsed_ms = count(f, "elapsed_ms");
                self.priced_calls = count(f, "priced_calls");
                self.unpriced_calls = count(f, "unpriced_calls");
                self.cost_qualifier = text(f, "cost_qualifier");
                self.total_cost_usd = f.get("total_cost_usd").and_then(Value::as_f64);
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

    fn cost_line(&self) -> String {
        let priced = self.priced_calls.unwrap_or(0);
        let unpriced = self.unpriced_calls.unwrap_or(0);
        let mut line = match (self.total_cost_usd, priced, unpriced) {
            (Some(usd), p, _) => format!(
                "cost · ${usd:.4} · {p} priced call(s) · recorded estimate, invoice not verified"
            ),
            (None, 0, 0) => "cost · no model usage recorded".to_owned(),
            (None, 0, u) => format!(
                "cost · UNKNOWN · {u} unpriced call(s): a route with no price table (a local model is unpriced, never free)"
            ),
            (None, p, _) => {
                format!("cost · UNKNOWN · {p} priced call(s) journaled without a total")
            }
        };
        if unpriced > 0 && priced > 0 {
            let _ = write!(
                line,
                " · {unpriced} unpriced call(s): a route with no price table (a local model is unpriced, never free)"
            );
        }
        line
    }

    /// The result view, once a run ended (exit 0 or 1).
    #[must_use]
    pub fn result(&self, root: &Path, workflow: &Path) -> String {
        if self.status.as_deref() == Some("failed")
            || self.tasks.iter().any(|t| t.state == TaskState::Failed)
        {
            return self.failure(workflow);
        }
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
        let sent = self.sent();
        let word = match self.status.as_deref() {
            Some("succeeded") | None => "Done",
            Some("cancelled") => "Cancelled",
            Some(other) => other,
        };
        let mut view = format!("{word} · `{}`", workflow.display());
        if let Some(ms) = self.elapsed_ms {
            let _ = write!(view, " · {}", human_ms(ms));
        }
        let _ = write!(view, " · {ran} task{} ran", plural(ran));
        if cached > 0 {
            let _ = write!(view, " ({cached} from cache)");
        }
        if skipped > 0 {
            let _ = write!(view, " · {skipped} skipped");
        }
        for path in self.fs_paths("write") {
            let size = std::fs::metadata(root.join(&path))
                .ok()
                .filter(std::fs::Metadata::is_file)
                .map_or_else(String::new, |m| format!(" ({})", human_size(m.len())));
            let _ = write!(view, "\n  produced · {path}{size}");
        }
        for path in self.fs_paths("read") {
            let _ = write!(view, "\n  read · {path}");
        }
        for target in &sent {
            let _ = write!(view, "\n  network / notification · {target}");
        }
        for model in self.asked() {
            let _ = write!(view, "\n  asked · {model}");
        }
        for a in &self.approvals {
            let verdict = if a.decision == "allow" {
                "approved · your answer let it go on"
            } else {
                "refused · your answer stopped it there"
            };
            let by = if a.source == "resume" {
                "answered in this session"
            } else {
                &a.source
            };
            let _ = write!(view, "\n  {verdict} · `{}` · {by}", a.task);
        }
        let _ = write!(view, "\n  {}", self.cost_line());
        view.push_str("\n  `/proof` shows the records and their limits · « run it » runs it again");
        view
    }

    /// The failure view: which task failed, what ran before it.
    fn failure(&self, workflow: &Path) -> String {
        let mut view = format!("Failed · `{}`", workflow.display());
        if let Some(t) = self.tasks.iter().find(|t| t.state == TaskState::Failed) {
            let _ = write!(view, " · `{}` failed", t.id);
            if let Some(detail) = &t.detail {
                let _ = write!(view, "\n  {}", first_line(detail));
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
            view.push_str("\n  nothing completed before it");
        } else {
            let _ = write!(view, "\n  ran before it · {}", before.join(" · "));
        }
        let never: Vec<&str> = self
            .tasks
            .iter()
            .filter(|t| matches!(t.state, TaskState::Scheduled | TaskState::Cancelled))
            .map(|t| t.id.as_str())
            .collect();
        if !never.is_empty() {
            let _ = write!(view, "\n  never ran · {}", never.join(" · "));
        }
        let _ = write!(view, "\n  {}", self.cost_line());
        view.push_str(
            "\n  the trace holds the failure · `/proof` reads it · repair the workflow, then « run it » again",
        );
        view
    }

    /// The gate view, when a run paused: so far · the question · what a
    /// yes lets happen (`gated`, from the workflow's bytes) · what a no does.
    #[must_use]
    pub fn gate(&self, workflow: &Path, message: &str, mode: &str, gated: &[String]) -> String {
        let mut view = format!(
            "Paused · `{}` asks you before it goes on",
            workflow.display()
        );
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
        if so_far.is_empty() {
            view.push_str("\n  so far · nothing has run before the gate");
        } else {
            let _ = write!(view, "\n  so far · {}", so_far.join(" · "));
        }
        if let Some(again) = live_again(&self.trace) {
            let _ = write!(view, "\n  {again}");
        }
        let _ = write!(view, "\n  « {message} »");
        if gated.is_empty() {
            view.push_str("\n  a yes lets happen · the tasks after this gate (the workflow's bytes name them)");
        } else {
            let _ = write!(view, "\n  a yes lets happen · {}", gated.join(" · "));
        }
        let how = match mode {
            "confirm" => "yes or no",
            "choice" => "one of the choices, as written",
            _ => "in words",
        };
        let _ = write!(
            view,
            "\n  a no ends the run there · nothing after the gate has happened yet\n  answer {how} · « why? » explains · nothing answers for you"
        );
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
    let Ok(doc) = serde_json::from_str::<Value>(out.text.trim()) else {
        return Attestation::NotAttested;
    };
    let word = |outer: &str, inner: &str| {
        doc.get(outer)
            .and_then(|v| v.get(inner))
            .and_then(Value::as_str)
    };
    if word("chain", "headline") != Some("intact") {
        return Attestation::NotAttested;
    }
    match (
        doc.get("tier").and_then(Value::as_str),
        word("seal", "tier"),
    ) {
        (Some("anchored" | "replayed"), Some("sealed")) => Attestation::Anchored,
        (Some("sealed"), Some("sealed")) => Attestation::Signed,
        (Some("ok"), Some("unsealed")) => Attestation::Unsigned,
        (_, Some("unattributable")) => Attestation::KeyElsewhere,
        _ => Attestation::NotAttested,
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
    let live = resumed_live(trace);
    (!live.is_empty()).then(|| {
        format!(
            "any answer resumes the run, and these completed tasks run again, live · {} (the journal cannot serve them back)",
            live.join(" · ")
        )
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
mod tests;
