//! What a run's trace proves — the result, the gate and the proof views,
//! read from the journal's own frames, never from what the run printed.
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

/// Everything a trace's frames say about one run.
#[derive(Clone, Debug, Default)]
pub(crate) struct RunFacts {
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
    pub(crate) seal: Option<Seal>,
    /// Frames read (lines that parsed as events).
    pub(crate) events: usize,
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

fn text(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn count(map: &BTreeMap<String, Value>, key: &str) -> Option<u64> {
    map.get(key).and_then(Value::as_u64)
}

impl RunFacts {
    /// Read a trace's frames. `None` when the file cannot be read or no
    /// line is an event (the view then falls back to the door's line).
    pub(crate) fn read(trace: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(trace).ok()?;
        let mut facts = Self {
            trace: trace.to_path_buf(),
            ..Self::default()
        };
        for line in raw.lines() {
            let Ok(frame) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let Some(kind) = frame.get("kind").and_then(Value::as_str) else {
                continue;
            };
            facts.events += 1;
            facts.absorb(kind, &fields(&frame));
        }
        (facts.events > 0).then_some(facts)
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

    /// What left this machine, as the frames prove it: a `net` permit
    /// decision, or a task that invoked `nika:notify` and completed — each
    /// with the evidence it rests on.
    fn sent(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .permits
            .iter()
            .filter(|p| p.plane == "net" && p.decision == "allow")
            .map(|p| format!("{} · MEASURED: the permit decision that let it out", p.gate))
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
            (Some(usd), p, _) => format!("cost · ${usd:.4} · {p} priced call(s) · MEASURED"),
            (None, 0, 0) => "cost · nothing metered · no model was asked".to_owned(),
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
    pub(crate) fn result(&self, root: &Path, workflow: &Path) -> String {
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
        if sent.is_empty() {
            view.push_str(" · nothing sent elsewhere");
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
            let _ = write!(view, "\n  sent · {target}");
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
        view.push_str("\n  `/proof` shows what this trace proves · « run it » runs it again");
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
    pub(crate) fn gate(
        &self,
        workflow: &Path,
        message: &str,
        mode: &str,
        gated: &[String],
    ) -> String {
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
    pub(crate) fn proof(&self, root: &Path) -> String {
        let mut view = format!(
            "Proof · `{}` · what the engine MEASURED, hash-chained as it happened",
            self.trace.display()
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
            "\n  proves · which tasks ran, what every permit check decided, how long each took, the digests of every input and output\n  does not prove · that the content is right (read it) · that anyone outside this machine trusts the key (`nika trace anchor` notarizes the head)\n  `nika trace verify <trace>` re-judges the chain · `nika trace outputs <trace>` prints every output",
        );
        view
    }
}

/// The chain and seal lines `nika trace verify` says, through the ONE
/// judge — never a second chain walker. (chain line, seal line).
fn chain_verdict(trace: &Path) -> (String, String) {
    let out = nika_trace::trace_verify::verify(&trace.display().to_string());
    let mut lines = out.text.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = shorten_hex(lines.next().unwrap_or(""));
    if out.code != 0 {
        return (
            format!("chain · not judged (verify exit {}) · {first}", out.code),
            String::new(),
        );
    }
    let seal = lines
        .find(|l| {
            l.starts_with("SEALED") || l.starts_with("UNSEALED") || l.starts_with("INCOMPLETE")
        })
        .map(|l| format!("seal · {l}"))
        .unwrap_or_default();
    (format!("chain · {first}"), seal)
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

/// A byte count a human reads (`1.2 KB`, `340 B`).
#[allow(clippy::cast_precision_loss)] // display-only: a size shown to a human, never computed with
fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/traces")
            .join(name)
    }

    /// A real trace of the deterministic copy (engine 0.120.3): two
    /// invoke tasks, four permit decisions, a seal — the result names what
    /// was produced and read from the permit frames, and the cost is
    /// honestly « nothing metered ».
    #[test]
    fn the_copy_trace_reads_as_a_result() {
        let facts = RunFacts::read(&fixture("copy.ndjson")).expect("frames");
        assert_eq!(facts.status.as_deref(), Some("succeeded"));
        assert_eq!(facts.events, 13);
        assert_eq!(facts.tasks.len(), 2);
        assert!(facts.tasks.iter().all(|t| t.state == TaskState::Ok));
        assert_eq!(facts.permits.len(), 4);
        assert!(facts.permits.iter().all(|p| p.decision == "allow"));
        let seal = facts.seal.as_ref().expect("sealed");
        assert!(seal.head.starts_with("5b8e2720"), "{seal:?}");
        assert_eq!(
            (seal.declared, seal.exercised, seal.escapes),
            (Some(2), Some(2), Some(0))
        );
        let root = tempfile::tempdir().expect("tmp");
        std::fs::create_dir_all(root.path().join("out")).expect("out");
        std::fs::write(
            root.path().join("out/copie.md"),
            "# Brief\n\nLe lancement passe en octobre.\n",
        )
        .expect("artefact");
        let view = facts.result(root.path(), Path::new("copy.nika"));
        assert!(
            view.starts_with("Done · `copy.nika` · 11 ms · 2 tasks ran · nothing sent elsewhere"),
            "{view}"
        );
        assert!(
            view.contains("\n  produced · ./out/copie.md (40 B)"),
            "{view}"
        );
        assert!(view.contains("\n  read · ./notes/brief.md"), "{view}");
        assert!(
            view.contains("cost · nothing metered · no model was asked"),
            "{view}"
        );
        assert!(
            view.contains("`/proof` shows what this trace proves"),
            "{view}"
        );
        assert!(!view.contains("approved"), "no approval happened: {view}");
    }

    /// A real paused trace: one task ran, the gate frame carries the
    /// question; the gate view says what ran, asks, and names what a yes
    /// lets happen from the list the workflow's bytes gave.
    #[test]
    fn a_paused_trace_reads_as_a_gate() {
        let facts = RunFacts::read(&fixture("paused.ndjson")).expect("frames");
        let pause = facts.pause.as_ref().expect("a pause");
        assert_eq!(pause.task, "approve");
        assert_eq!(pause.mode, "confirm");
        assert_eq!(pause.message, "Write the copy to ./out/copie.md?");
        assert!(facts.status.is_none(), "a paused run has no terminal word");
        let gated = vec!["write_output · nika:write".to_owned()];
        let view = facts.gate(Path::new("gated.nika"), &pause.message, &pause.mode, &gated);
        assert!(
            view.starts_with("Paused · `gated.nika` asks you before it goes on"),
            "{view}"
        );
        assert!(
            view.contains("\n  so far · read_source · invoke · nika:read (3 ms)"),
            "{view}"
        );
        assert!(
            view.contains("\n  « Write the copy to ./out/copie.md? »"),
            "{view}"
        );
        assert!(
            view.contains("\n  a yes lets happen · write_output · nika:write"),
            "{view}"
        );
        assert!(
            view.contains("a no ends the run there") && view.contains("answer yes or no"),
            "{view}"
        );
        assert!(view.contains("nothing answers for you"), "{view}");
    }

    /// The real resume of that pause: the pre-gate task replays from the
    /// cache, the approval frame says the answer came through the resume,
    /// the write ran — the result says approved, counts the cache hit.
    #[test]
    fn a_resumed_trace_names_the_approval_and_the_cache() {
        let facts = RunFacts::read(&fixture("resumed.ndjson")).expect("frames");
        assert_eq!(facts.status.as_deref(), Some("succeeded"));
        assert_eq!(facts.approvals.len(), 1);
        assert_eq!(facts.approvals[0].decision, "allow");
        assert_eq!(facts.approvals[0].source, "resume");
        let read = facts
            .tasks
            .iter()
            .find(|t| t.id == "read_source")
            .expect("read_source");
        assert_eq!(read.state, TaskState::CacheHit);
        let root = tempfile::tempdir().expect("tmp");
        let view = facts.result(root.path(), Path::new("gated.nika"));
        assert!(
            view.starts_with(
                "Done · `gated.nika` · 49 ms · 2 tasks ran (1 from cache) · nothing sent elsewhere"
            ),
            "{view}"
        );
        assert!(
            view.contains(
                "\n  approved · your answer let it go on · `approve` · answered in this session"
            ),
            "{view}"
        );
        assert!(
            view.contains("\n  produced · ./out/copie.md\n"),
            "the file is not on this disk: no size · {view}"
        );
    }

    /// `/proof` judges the chain through the verify door (never a second
    /// walker), names the workflow's two identities, the boundary the seal
    /// covers, and says what it does not prove. The seal tier depends on
    /// the machine's key custody and is not asserted.
    #[test]
    fn the_proof_reads_the_chain_through_the_verify_door() {
        let facts = RunFacts::read(&fixture("resumed.ndjson")).expect("frames");
        let root = tempfile::tempdir().expect("tmp");
        std::fs::create_dir_all(root.path().join("out")).expect("out");
        std::fs::write(
            root.path().join("out/copie.md"),
            "# Brief\n\nLe lancement passe en octobre.\n",
        )
        .expect("artefact");
        let view = facts.proof(root.path());
        assert!(view.starts_with("Proof · "), "{view}");
        assert!(view.contains("MEASURED"), "{view}");
        assert!(
            view.contains(
                "\n  workflow · gated-copy · bytes sha256 00448c33…6530 · meaning bbbd59cf…9cbd"
            ),
            "{view}"
        );
        assert!(
            view.contains("\n  chain · OK — 15 events · chain intact · head 09ea39d4…2a50"),
            "the judge's own words, the head shortened for the eye: {view}"
        );
        // Three, not five: the resumed run replayed `read_source` from the
        // cache, so its two permit checks were never re-decided.
        assert!(
            view.contains("\n  boundary · 3 effect(s) declared · 3 exercised · 0 escaped · 3 permit check(s) · 3 allowed · 0 denied"),
            "{view}"
        );
        assert!(
            view.contains("\n  written · ./out/copie.md · 40 B · sha256 ")
                && view.contains("RE-READ"),
            "{view}"
        );
        assert!(
            view.contains("\n  approval · `approve` · allow · source resume"),
            "{view}"
        );
        assert!(
            view.contains("engine · 0.120.3 · sandbox seatbelt"),
            "{view}"
        );
        assert!(
            view.contains("does not prove · that the content is right"),
            "{view}"
        );
    }

    /// A failed task (a synthetic journal in the engine's frame shape):
    /// the view names the task, its detail, what ran before, what never ran.
    #[test]
    fn a_failed_task_reads_as_a_failure() {
        let dir = tempfile::tempdir().expect("tmp");
        let trace = dir.path().join("failed.ndjson");
        std::fs::write(
            &trace,
            concat!(
                "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"draft\"}]}\n",
                "{\"kind\":\"task_scheduled\",\"fields\":[{\"key\":\"task\",\"value\":\"read\"}]}\n",
                "{\"kind\":\"task_scheduled\",\"fields\":[{\"key\":\"task\",\"value\":\"draft\"}]}\n",
                "{\"kind\":\"task_scheduled\",\"fields\":[{\"key\":\"task\",\"value\":\"write\"}]}\n",
                "{\"kind\":\"task_started\",\"fields\":[{\"key\":\"task\",\"value\":\"read\"},{\"key\":\"note\",\"value\":\"invoke · nika:read\"}]}\n",
                "{\"kind\":\"task_completed\",\"fields\":[{\"key\":\"task\",\"value\":\"read\"},{\"key\":\"duration_ms\",\"value\":2}]}\n",
                "{\"kind\":\"task_started\",\"fields\":[{\"key\":\"task\",\"value\":\"draft\"},{\"key\":\"note\",\"value\":\"infer · mock/echo\"}]}\n",
                "{\"kind\":\"task_failed\",\"fields\":[{\"key\":\"task\",\"value\":\"draft\"},{\"key\":\"detail\",\"value\":\"NIKA-PROVIDER-002 the route refused\\nsecond line\"}]}\n",
                "{\"kind\":\"workflow_failed\",\"fields\":[{\"key\":\"workflow\",\"value\":\"draft\"},{\"key\":\"priced_calls\",\"value\":0},{\"key\":\"unpriced_calls\",\"value\":1}]}\n",
            ),
        )
        .expect("trace");
        let facts = RunFacts::read(&trace).expect("frames");
        assert_eq!(facts.status.as_deref(), Some("failed"));
        let view = facts.result(dir.path(), Path::new("draft.nika"));
        assert!(
            view.starts_with(
                "Failed · `draft.nika` · `draft` failed\n  NIKA-PROVIDER-002 the route refused\n"
            ),
            "{view}"
        );
        assert!(view.contains("\n  ran before it · read (2 ms)"), "{view}");
        assert!(view.contains("\n  never ran · write"), "{view}");
        assert!(
            view.contains("cost · UNKNOWN · 1 unpriced call(s): a route with no price table"),
            "an unpriced call is never free: {view}"
        );
    }

    /// A file that is not a journal is no reading at all: the door's own
    /// observation line stands alone.
    #[test]
    fn a_non_journal_is_not_read() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("not.ndjson");
        std::fs::write(&path, "hello\n{\"no\":\"kind\"}\n").expect("file");
        assert!(RunFacts::read(&path).is_none());
        assert!(RunFacts::read(&dir.path().join("absent.ndjson")).is_none());
    }

    #[test]
    fn the_human_units_read() {
        assert_eq!(human_ms(11), "11 ms");
        assert_eq!(human_ms(1234), "1.2 s");
        assert_eq!(human_ms(125_000), "2 min 05 s");
        assert_eq!(
            shorten_hex(
                "OK — 13 events · chain intact · head 1cf484e5340c918ce64f6955ed0cd3a54dcaf44572a973eab3fb6e37d0147f01"
            ),
            "OK — 13 events · chain intact · head 1cf484e5…7f01"
        );
        assert_eq!(
            short("5d1bf5915e9fbee4b3d7df30fd517302f1aeab8a25e035a85987bc5f26090730"),
            "5d1bf591…0730"
        );
        assert_eq!(short("abc"), "abc");
    }
}
