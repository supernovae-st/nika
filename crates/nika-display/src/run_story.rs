// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Pure projection of the existing Run machine stream: the story a busy row
//! shows and a transcript keeps, and the same frames told typed to whoever
//! renders the run ([`RunSink`](crate::run_story::RunSink)): the runtime's own [`Event`](crate::run_story::Event), or the
//! settlement envelope that closes the stream ([`Settled`](crate::run_story::Settled)). A line of the
//! stream that should have been a frame and is not is said to the sink as
//! unread; nothing here fabricates an event from the story's words.
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use serde_json::Value;

pub use nika_event::settlement::OUTPUTS_KEPT;
pub use nika_event::settlement::{RunSettlement, RunState};
pub use nika_event::{Event, EventKind};
pub use nika_types::id::ExecutionId;

/// The most story lines one run keeps; past it a line is counted, not kept.
pub const STORY_KEPT: usize = 2_000;

/// The run's story, folded from the machine lane's frames: one short
/// line per task settle, the header, the summary, the pause — the words
/// the busy row shows and the transcript keeps.
#[derive(Default)]
pub struct RunStory {
    /// Every line said so far, for the block the transcript commits (at most
    /// [`STORY_KEPT`]).
    pub lines: Vec<String>,
    /// The trace the settle frame named.
    pub trace: Option<PathBuf>,
    /// Lines said past [`STORY_KEPT`]: told to the sink, not kept.
    pub untold: usize,
    total: usize,
    done: usize,
}

impl RunStory {
    /// One frame; the line it adds to the story, if any.
    pub fn frame(&mut self, line: &str) -> Option<String> {
        let frame: Value = serde_json::from_str(line).ok()?;
        self.say(&frame)
    }

    /// One frame told to `sink`: the line it adds to the story (the very
    /// line [`Self::frame`] returns), then the frame itself, typed. A line
    /// that is not JSON, or a frame naming a kind this reader cannot type,
    /// is told as unread; a refusal document or a review question has no
    /// typed twin and is not one.
    pub fn tell(&mut self, line: &str, sink: &dyn RunSink) {
        let Ok(frame) = serde_json::from_str::<Value>(line) else {
            if !line.trim().is_empty() {
                sink.unread("a line of the run's stream is not JSON");
            }
            return;
        };
        if let Some(said) = self.say(&frame) {
            sink.said(said);
        }
        match RunFrame::of(frame) {
            Ok(Some(typed)) => sink.frame(typed),
            Ok(None) => {}
            Err(why) => sink.unread(why),
        }
    }

    /// The line one parsed frame adds to the story, if any.
    fn say(&mut self, frame: &Value) -> Option<String> {
        // A run refused before its first frame speaks two other shapes on
        // the same stream: the check verdict document (`clean: false` and
        // its findings) and the error envelope (`{"error": {…}}`). Each
        // is one story line naming the reason — never a silent exit.
        let Some(kind) = frame.get("kind").and_then(serde_json::Value::as_str) else {
            let said = refusal_line(frame)?;
            return Some(self.keep(said));
        };
        let field = |key: &str| -> Option<String> {
            frame
                .get("fields")?
                .as_array()?
                .iter()
                .find(|f| f.get("key").and_then(|k| k.as_str()) == Some(key))?
                .get("value")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
        };
        let said = match kind {
            "workflow_started" => format!("running · {}", field("workflow").unwrap_or_default()),
            "task_scheduled" => {
                self.total += 1;
                return None;
            }
            "task_started" => format!(
                "→ {} · {}",
                field("task").unwrap_or_default(),
                field("note").unwrap_or_default()
            ),
            "task_completed" => {
                self.done += 1;
                format!(
                    "✔ {} · {} ms · {}/{}",
                    field("task").unwrap_or_default(),
                    field("duration_ms").unwrap_or_default(),
                    self.done,
                    self.total
                )
            }
            "task_cache_hit" => {
                self.done += 1;
                format!("↺ {} · from the cache", field("task").unwrap_or_default())
            }
            "task_failed" => format!(
                "✖ {} · {}",
                field("task").unwrap_or_default(),
                field("detail")
                    .unwrap_or_default()
                    .lines()
                    .next()
                    .unwrap_or_default()
            ),
            "task_skipped" => format!("· {} skipped", field("task").unwrap_or_default()),
            "task_cancelled" => format!("· {} cancelled", field("task").unwrap_or_default()),
            "workflow_paused" => format!(
                "◇ paused · `{}` asks you",
                field("task").unwrap_or_default()
            ),
            "workflow_completed" | "workflow_failed" | "workflow_cancelled" => format!(
                "{} · {}/{} tasks · {} ms",
                field("status").unwrap_or_else(|| kind.to_owned()),
                field("tasks_ok").unwrap_or_default(),
                field("tasks_total").unwrap_or_default(),
                field("elapsed_ms").unwrap_or_default()
            ),
            "run_settled" => {
                self.trace = frame
                    .get("receipt")
                    .and_then(|r| r.get("trace_path"))
                    .and_then(|p| p.as_str())
                    .map(PathBuf::from);
                return None;
            }
            _ => return None,
        };
        Some(self.keep(said))
    }

    /// Keep `said` while the story holds fewer than [`STORY_KEPT`] lines,
    /// count it otherwise; the line is said either way.
    fn keep(&mut self, said: String) -> String {
        if self.lines.len() < STORY_KEPT {
            self.lines.push(said.clone());
        } else {
            self.untold += 1;
        }
        said
    }
}

/// Where a lane run speaks while it runs: each line of its story, each
/// machine frame typed, and each line that should have been a frame and is
/// not. A `Sender<String>` takes the story alone, so a caller that renders
/// lines never meets a frame it did not ask for.
pub trait RunSink {
    /// One line of the run's story, as [`RunStory::frame`] says it.
    fn said(&self, line: String);
    /// One machine frame, typed; a sink of lines leaves it.
    fn frame(&self, _frame: RunFrame) {}
    /// A line of the stream that is not a frame this reader can type: never
    /// a fabricated event, at most a counted gap.
    fn unread(&self, _why: &'static str) {}
}

impl RunSink for Sender<String> {
    fn said(&self, line: String) {
        let _ = self.send(line);
    }
}

/// One machine frame of the lane, typed: an engine event (the journal's
/// own type, every kind and field as the runtime wrote them) or the
/// envelope that settles the stream. A refusal document written before any
/// run has no typed twin: its story line says it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RunFrame {
    /// A runtime event.
    Event(Box<Event>),
    /// The `run_settled` envelope.
    Settled(Box<Settled>),
}

impl RunFrame {
    /// The typed frame one machine line is, if it is one.
    #[must_use]
    pub fn decode(line: &str) -> Option<Self> {
        Self::of(serde_json::from_str(line).ok()?).ok().flatten()
    }

    /// The typed twin of a parsed frame: `None` for a document without a
    /// kind (a refusal, a review question, the outputs line), an error for
    /// a frame naming a kind this reader cannot type.
    fn of(frame: Value) -> Result<Option<Self>, &'static str> {
        match frame.get("kind").and_then(Value::as_str) {
            None => Ok(None),
            Some("run_settled") => Settled::of(&frame)
                .map(|settled| Some(Self::Settled(Box::new(settled))))
                .ok_or("a settlement this reader cannot type"),
            Some(_) => serde_json::from_value(frame)
                .map(|event| Some(Self::Event(Box::new(event))))
                .map_err(|_| "an event this reader cannot type"),
        }
    }

    /// The execution the frame names, when it names one.
    #[must_use]
    pub fn execution(&self) -> Option<ExecutionId> {
        match self {
            Self::Event(event) => event.execution,
            Self::Settled(settled) => settled.execution,
        }
    }
}

/// The `run_settled` envelope, typed: the runtime's settlement, the
/// execution it settles, what the door says of the evidence the run left
/// (ADR-129: a declaration of the producer, never a verification of the
/// journal), the receipt's identities and the outputs it carried. A field
/// the envelope did not carry stays absent.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Settled {
    /// State, cause, elapsed time, task tally, spend and the failure named.
    pub settlement: RunSettlement,
    /// The execution the envelope settles.
    pub execution: Option<ExecutionId>,
    /// What the run left as evidence, as the producer declared it.
    pub evidence: Option<Evidence>,
    /// The journal the receipt names.
    pub trace: Option<PathBuf>,
    /// The receipt's trace identity.
    pub trace_id: Option<String>,
    /// The journal's chain head, as the receipt names it.
    pub chain_head: Option<String>,
    /// How many events the journal's chain holds, as the receipt says.
    pub chain_len: Option<u64>,
    /// The digest of the execution snapshot the run admitted.
    pub snapshot_digest: Option<String>,
    /// The workflow's outputs as the envelope carried them.
    pub outputs: Outputs,
    /// The trace this leg continued (`--resume`).
    pub resumed_from: Option<String>,
}

impl Settled {
    fn of(frame: &Value) -> Option<Self> {
        let receipt = |key: &str| frame.get("receipt").and_then(|r| r.get(key));
        let text = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
        Some(Self {
            settlement: serde_json::from_value(frame.clone()).ok()?,
            execution: (frame.get("execution").cloned())
                .and_then(|e| serde_json::from_value(e).ok()),
            evidence: frame
                .get("evidence")
                .and_then(Value::as_str)
                .and_then(Evidence::parse),
            trace: text(receipt("trace_path")).map(PathBuf::from),
            trace_id: text(receipt("trace_id")),
            chain_head: text(receipt("chain_head")),
            chain_len: receipt("chain_len").and_then(Value::as_u64),
            snapshot_digest: text(receipt("snapshot_digest")),
            outputs: Outputs::of(frame.get("outputs")),
            resumed_from: text(frame.get("resumed_from")),
        })
    }
}

/// What one run's own frames name of it, folded the one way every host folds them: the execution
/// its first frame binds, the source hash its ONE start names, and the journal and receipt its
/// settlement names. A frame naming another execution or none, or arriving after the settlement,
/// adds nothing; two starts name no source hash. Display-owned: a host maps it to what it keeps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunIdentity {
    execution: Option<ExecutionId>,
    starts: Vec<Option<String>>,
    trace: Option<PathBuf>,
    chain_head: Option<String>,
    chain_len: Option<u64>,
    settled: bool,
}

impl RunIdentity {
    /// Nothing observed yet: the first frame naming an execution binds it.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The identity of `execution`, which a host bound before folding its frames.
    #[must_use]
    pub fn of_execution(execution: ExecutionId) -> Self {
        Self {
            execution: Some(execution),
            ..Self::default()
        }
    }

    /// A settled identity as a host kept it from an earlier observation (its start's source
    /// hash, its journal and receipt): evidence to re-verify, never a frame.
    #[must_use]
    pub fn restored(
        execution: ExecutionId,
        workflow_sha256: Option<String>,
        trace: Option<PathBuf>,
        (chain_head, chain_len): (Option<String>, Option<u64>),
    ) -> Self {
        Self {
            execution: Some(execution),
            starts: vec![workflow_sha256],
            trace,
            chain_head,
            chain_len,
            settled: true,
        }
    }

    /// The identity a journal's own lines name, each folded as the frame it is: a line that is
    /// not a typed frame adds nothing.
    #[must_use]
    pub fn of_journal(raw: &str) -> Self {
        let mut identity = Self::new();
        for frame in raw.lines().filter_map(RunFrame::decode) {
            identity.frame(&frame);
        }
        identity
    }

    /// One typed frame of the run.
    pub fn frame(&mut self, frame: &RunFrame) {
        match frame {
            RunFrame::Event(event) => self.event(event),
            RunFrame::Settled(settled) => self.settle(settled),
        }
    }

    /// One event of the run: its start names the source hash it ran.
    pub fn event(&mut self, event: &Event) {
        if self.binds(event.execution) && event.kind == EventKind::WorkflowStarted {
            (self.starts).push(event.str_field("workflow_sha256").map(str::to_owned));
        }
    }

    /// The run's settlement: the journal and receipt it names. Nothing after it adds.
    pub fn settle(&mut self, settled: &Settled) {
        if self.binds(settled.execution) {
            self.trace.clone_from(&settled.trace);
            self.chain_head.clone_from(&settled.chain_head);
            self.chain_len = settled.chain_len;
            self.settled = true;
        }
    }

    /// Whether a frame of `execution` is this run's: the first execution named binds it.
    fn binds(&mut self, execution: Option<ExecutionId>) -> bool {
        match (execution, self.execution) {
            (None, _) => false,
            _ if self.settled => false,
            (Some(named), None) => {
                self.execution = Some(named);
                true
            }
            (Some(named), Some(bound)) => named == bound,
        }
    }

    /// The execution the run's first frame bound.
    #[must_use]
    pub const fn execution(&self) -> Option<ExecutionId> {
        self.execution
    }

    /// The source hash the run's start named, when exactly one start named one.
    #[must_use]
    pub fn workflow_sha256(&self) -> Option<&str> {
        match self.starts.as_slice() {
            [Some(hash)] => Some(hash),
            _ => None,
        }
    }

    /// The journal the settlement's receipt names.
    #[must_use]
    pub fn trace(&self) -> Option<&std::path::Path> {
        self.trace.as_deref()
    }

    /// The journal's chain head, as the receipt names it.
    #[must_use]
    pub fn chain_head(&self) -> Option<&str> {
        self.chain_head.as_deref()
    }

    /// How many events the journal's chain holds, as the receipt says.
    #[must_use]
    pub const fn chain_len(&self) -> Option<u64> {
        self.chain_len
    }

    /// Whether the settlement was observed.
    #[must_use]
    pub const fn settled(&self) -> bool {
        self.settled
    }
}

/// A settlement's outputs, kept within [`OUTPUTS_KEPT`] bytes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Outputs {
    /// The envelope carried none.
    Absent,
    /// The outputs, whole.
    Kept(Value),
    /// The outputs were larger than [`OUTPUTS_KEPT`]: their size, not them.
    TooLarge {
        /// Their size, serialized.
        bytes: usize,
    },
    /// The journal says the outputs map was withheld whole: neither its
    /// keys nor its values were recorded (and a `***` inside a recorded
    /// value never means this).
    Withheld,
    /// The journal records an outputs map this reader cannot interpret
    /// (two forms, a wrong type, a size within the cap, a broken JSON).
    Unreadable,
}

impl Outputs {
    fn of(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::Absent;
        };
        let bytes = serde_json::to_string(value).map_or(usize::MAX, |s| s.len());
        if bytes > OUTPUTS_KEPT {
            Self::TooLarge { bytes }
        } else {
            Self::Kept(value.clone())
        }
    }

    /// The outputs map a run's terminal frame (`workflow_completed`,
    /// `workflow_failed`) records, read strictly from every field of the
    /// frame: none is [`Self::Absent`] (an older engine, or a close that did
    /// not record it); exactly one well-formed companion is its state;
    /// anything else is [`Self::Unreadable`]. Another frame records none.
    pub(crate) fn from_event(event: &Event) -> Self {
        use nika_event::settlement::{OUTPUTS_BYTES_FIELD, OUTPUTS_FIELD, OUTPUTS_WITHHELD_FIELD};
        use nika_types::resource::Value as FieldValue;
        if !matches!(
            event.kind,
            EventKind::WorkflowCompleted | EventKind::WorkflowFailed
        ) {
            return Self::Absent;
        }
        let named = [OUTPUTS_FIELD, OUTPUTS_BYTES_FIELD, OUTPUTS_WITHHELD_FIELD];
        let found: Vec<_> = (event.fields.iter())
            .filter(|kv| named.contains(&kv.key.as_str()))
            .collect();
        let [kv] = found.as_slice() else {
            return if found.is_empty() {
                Self::Absent
            } else {
                Self::Unreadable
            };
        };
        match (kv.key.as_str(), &kv.value) {
            (OUTPUTS_FIELD, FieldValue::String(json)) if json.len() <= OUTPUTS_KEPT => {
                match serde_json::from_str::<Value>(json) {
                    Ok(map @ Value::Object(_)) => Self::Kept(map),
                    _ => Self::Unreadable,
                }
            }
            (OUTPUTS_BYTES_FIELD, FieldValue::Int(bytes)) => match usize::try_from(*bytes) {
                Ok(bytes) if bytes > OUTPUTS_KEPT => Self::TooLarge { bytes },
                _ => Self::Unreadable,
            },
            (OUTPUTS_WITHHELD_FIELD, FieldValue::Bool(true)) => Self::Withheld,
            _ => Self::Unreadable,
        }
    }
}

/// What a run left as evidence, as its settlement envelope declares it
/// (ADR-129): the run's state is never its evidence, and a declared seal is
/// not a verified journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Evidence {
    /// A journal, and its seal landed.
    Sealed,
    /// A journal, without a seal.
    Unsealed,
    /// A journal whose writer died after the run's effects.
    Lost,
    /// No journal: the run opted out of one.
    NoJournal,
}

impl Evidence {
    fn parse(word: &str) -> Option<Self> {
        match word {
            "sealed" => Some(Self::Sealed),
            "unsealed" => Some(Self::Unsealed),
            "lost" => Some(Self::Lost),
            "none" => Some(Self::NoJournal),
            _ => None,
        }
    }
}

/// How a child run ended, as the row its parent's settle frame carries says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChildOutcome {
    /// The row says `success`.
    Success,
    /// The row says `failure`.
    Failure,
}

/// The child run a task called (`invoke: workflow`), as the task's settle
/// frame names it: an observation of that row, never the whole hierarchy,
/// and never proof that the journal it names is that child's.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChildRun {
    /// The target as written at the call site: words, never a path to read.
    pub target: String,
    /// The child's own journal, by the whole file name its runner gave it.
    pub trace_id: Option<String>,
    /// The row says `success` (the same as `outcome == Some(Success)`).
    pub succeeded: bool,
    /// The head of the child's own chain the parent's frame commits to.
    pub chain_head: Option<String>,
    /// The sha256 of the source bytes the child ran from, as the row names it.
    pub def_hash: Option<String>,
    /// How the row says the child ended; `None` when it says nothing or a
    /// word nobody recognises, never a failure.
    pub outcome: Option<ChildOutcome>,
}

impl ChildRun {
    /// The child row a task's settle frame (`task_completed`, the only frame
    /// the producer writes it on) carries, if it carries one.
    #[must_use]
    pub fn of(event: &Event) -> Option<Self> {
        if event.kind != EventKind::TaskCompleted {
            return None;
        }
        let row: Value = serde_json::from_str(event.str_field("child")?).ok()?;
        let text = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_owned);
        let outcome = match row.get("outcome").and_then(Value::as_str) {
            Some("success") => Some(ChildOutcome::Success),
            Some("failure") => Some(ChildOutcome::Failure),
            _ => None,
        };
        Some(Self {
            target: row.get("target")?.as_str()?.to_owned(),
            trace_id: text("trace_id"),
            succeeded: outcome == Some(ChildOutcome::Success),
            chain_head: text("chain_head"),
            def_hash: text("def_hash"),
            outcome,
        })
    }
}

/// Whether a `workflow_started` event names `bytes` as the source it runs
/// (its `workflow_sha256`): `None` for another event, or one that names no
/// source hash.
#[must_use]
pub fn started_on(event: &Event, bytes: &[u8]) -> Option<bool> {
    if event.kind != EventKind::WorkflowStarted {
        return None;
    }
    let named = event.str_field("workflow_sha256")?;
    Some(named == nika_event::source_id::sha256_hex(bytes))
}

/// The one line a pre-run refusal document yields: the first finding of
/// a check verdict (with the count of the others), or the envelope's
/// message. `None` for any other kind-less object (the story ignores it).
fn refusal_line(frame: &serde_json::Value) -> Option<String> {
    if let Some(findings) = frame.get("findings").and_then(serde_json::Value::as_array) {
        if frame.get("clean").and_then(serde_json::Value::as_bool) == Some(true) {
            return None;
        }
        let finding = findings.iter().find(|f| f.get("message").is_some())?;
        let message = finding.get("message")?.as_str()?;
        let message = message.lines().next().unwrap_or_default();
        let code = finding
            .get("code")
            .and_then(serde_json::Value::as_str)
            .map_or(String::new(), |c| format!("[{c}] "));
        let more = findings.len().saturating_sub(1);
        return Some(if more == 0 {
            format!("✖ refused before the start · {code}{message}")
        } else {
            format!(
                "✖ refused before the start · {code}{message} · {more} more finding(s) — `nika check` lists them"
            )
        });
    }
    let error = frame.get("error")?;
    let message = error
        .get("message")
        .and_then(serde_json::Value::as_str)
        .or_else(|| error.as_str())
        .map_or_else(
            || error.to_string(),
            |m| m.lines().next().unwrap_or_default().to_owned(),
        );
    Some(format!("✖ refused · {message}"))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::Path;

    /// The story folds the lane's frames into one line each (the header,
    /// a task start, a settle with its count, a pause), keeps the trace
    /// the settle names, and says nothing for a frame it does not tell.
    #[test]
    fn the_story_folds_the_frames_and_keeps_the_trace() {
        let mut story = RunStory::default();
        assert_eq!(
            story.frame(r#"{"kind":"workflow_started","fields":[{"key":"workflow","value":"copy.nika"}]}"#).as_deref(),
            Some("running · copy.nika")
        );
        assert!(
            story
                .frame(r#"{"kind":"task_scheduled","fields":[{"key":"task","value":"a"}]}"#)
                .is_none()
        );
        assert!(
            story
                .frame(r#"{"kind":"task_scheduled","fields":[{"key":"task","value":"b"}]}"#)
                .is_none()
        );
        assert_eq!(
            story.frame(r#"{"kind":"task_started","fields":[{"key":"task","value":"a"},{"key":"note","value":"invoke · nika:read"}]}"#).as_deref(),
            Some("→ a · invoke · nika:read")
        );
        assert_eq!(
            story.frame(r#"{"kind":"task_completed","fields":[{"key":"task","value":"a"},{"key":"duration_ms","value":3}]}"#).as_deref(),
            Some("✔ a · 3 ms · 1/2")
        );
        assert_eq!(
            story
                .frame(r#"{"kind":"workflow_paused","fields":[{"key":"task","value":"approve"}]}"#)
                .as_deref(),
            Some("◇ paused · `approve` asks you")
        );
        assert!(
            story
                .frame(r#"{"kind":"permit_checked","fields":[]}"#)
                .is_none()
        );
        assert!(story.frame("not json at all").is_none());
        assert!(
            story
                .frame(r#"{"kind":"run_settled","receipt":{"trace_path":".nika/traces/t.ndjson"}}"#)
                .is_none()
        );
        assert_eq!(
            story.trace.as_deref(),
            Some(Path::new(".nika/traces/t.ndjson"))
        );
        assert_eq!(story.lines.len(), 4, "{:?}", story.lines);
    }

    /// A run refused before its first frame — the check verdict document
    /// or the error envelope on the same stream — is one story line that
    /// names the reason; a clean document and an unrelated object say nothing.
    #[test]
    fn a_refusal_before_the_start_names_its_reason() {
        let mut story = RunStory::default();
        let check = r#"{"clean":false,"findings":[{"code":"NIKA-AUTH-006","message":"invoke `nika:read` with a literal path under an absent `permits:` block (task `t`) — fix: add \"nika:read\" to permits.tools\nsecond line","severity":"error"},{"code":"NIKA-DRIFT-001","message":"x","severity":"warning"}],"report_version":1}"#;
        assert_eq!(
            story.frame(check).as_deref(),
            Some(
                "✖ refused before the start · [NIKA-AUTH-006] invoke `nika:read` with a literal path under an absent `permits:` block (task `t`) — fix: add \"nika:read\" to permits.tools · 1 more finding(s) — `nika check` lists them"
            )
        );
        let parse = r#"{"clean":false,"findings":[{"gate":"PARSE","kind":"parse","message":"cannot read missing.nika: ENOENT","severity":"error"}],"parse_fatal":true}"#;
        assert_eq!(
            story.frame(parse).as_deref(),
            Some("✖ refused before the start · cannot read missing.nika: ENOENT")
        );
        let envelope = r#"{"error":{"code":"NIKA-1709","message":"NIKA-1709 · refusing to start: the cost floor $0.01 exceeds --max-cost-usd $0.000001"}}"#;
        assert_eq!(
            story.frame(envelope).as_deref(),
            Some(
                "✖ refused · NIKA-1709 · refusing to start: the cost floor $0.01 exceeds --max-cost-usd $0.000001"
            )
        );
        assert!(story.frame(r#"{"clean":true,"findings":[]}"#).is_none());
        assert!(story.frame(r#"{"unrelated":1}"#).is_none());
        assert_eq!(story.lines.len(), 3);
    }

    /// The first leg of a gated mock run exactly as `nika run --json` wrote
    /// it (digest-notes: read, draft, a confirm gate, write · 2026-09-29):
    /// some whole fields cut, nothing else changed.
    const REAL_LEG: &[&str] = &[
        r#"{"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"workflow","value":"digest-notes"}],"id":{"uuid":"01a0ef11-03a1-73d9-a2bc-2548bdab1943"},"kind":"workflow_started","run":null,"timestamp":1790717264801000000}"#,
        r#"{"chain":"bcf41c6138d8fa9233e5dc526559163b67c75ceedf05dd6004ab57cb88c9e19d","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"read_notes"}],"id":{"uuid":"01a0ef11-03a7-74fb-bba0-bfe19b901333"},"kind":"task_scheduled","run":null,"timestamp":1790717264807000000}"#,
        r#"{"chain":"ca2641f82d0f7eda4f3ffd298417edea2625804da6fc2a1661737e6ab7ae32f3","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"draft"}],"id":{"uuid":"01a0ef11-03a7-74fb-bba0-bfe2802136a1"},"kind":"task_scheduled","run":null,"timestamp":1790717264807000000}"#,
        r#"{"chain":"d837e381172632bd47a197cc0e7364427318fdcbbb68cde7d69ebcd8cedcb413","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"approve"}],"id":{"uuid":"01a0ef11-03a7-74fb-bba0-bfe3011d300b"},"kind":"task_scheduled","run":null,"timestamp":1790717264807000000}"#,
        r#"{"chain":"b99377205aa111b51a471ac8634422898285c4f1f48dc55d8d379faa6201366b","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"write_digest"}],"id":{"uuid":"01a0ef11-03a7-74fb-bba0-bfe41c850462"},"kind":"task_scheduled","run":null,"timestamp":1790717264807000000}"#,
        r#"{"chain":"fd6191b5c74cc9047a95060c07738c1a1d120412cbcc2606eb28fcafb4e2f0ce","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"read_notes"},{"key":"note","value":"invoke · nika:read"}],"id":{"uuid":"01a0ef11-03b2-71ee-9ad4-17a43ae77dd4"},"kind":"task_started","run":null,"timestamp":1790717264818000000}"#,
        r#"{"chain":"d7c57c69a81922787c74bb76e6bfc079f517d87def62ea2621c34c1f432b23c3","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"read_notes"},{"key":"plane","value":"tool"},{"key":"gate","value":"nika:read"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}],"id":{"uuid":"01a0ef11-03b2-71ee-9ad4-17a579ccfdc7"},"kind":"permit_checked","run":null,"timestamp":1790717264818000000}"#,
        r#"{"chain":"d95f3c14fc9b3a15432005e057b6c2b9048c22b89f01182b684a5c6f368e863c","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"read_notes"},{"key":"plane","value":"fs"},{"key":"gate","value":"permits.fs.read ./notes/lundi.md"},{"key":"decision","value":"allow"},{"key":"why","value":"the effective identity stays inside the declared set"}],"id":{"uuid":"01a0ef11-03b2-71ee-9ad4-17a67a4356c6"},"kind":"permit_checked","run":null,"timestamp":1790717264818000000}"#,
        r#"{"chain":"099b273272488a2e69617d7e780030b7a7716850f00c6e7171efd38809b61ecf","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"read_notes"},{"key":"note","value":"invoke · nika:read"},{"key":"duration_ms","value":4}],"id":{"uuid":"01a0ef11-03b2-71ee-9ad4-17a755fad3ae"},"kind":"task_completed","run":null,"timestamp":1790717264818000000}"#,
        r#"{"chain":"c3a011d7171cd285d58969f8885dfa167ac395c9abd5128f5c8306adc8cba8fb","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"draft"},{"key":"note","value":"infer · mock/echo"}],"id":{"uuid":"01a0ef11-03b5-73a3-9408-4cbd1de62f04"},"kind":"task_started","run":null,"timestamp":1790717264821000000}"#,
        r#"{"chain":"4a3116d78ac7f7cb9ed57f6abbb385247ef228186121ef52e69ddeef8b1e76dc","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"task","value":"draft"},{"key":"note","value":"infer · mock/echo"},{"key":"duration_ms","value":0},{"key":"tokens","value":29},{"key":"tokens_in","value":27},{"key":"tokens_out","value":29},{"key":"model_served","value":"echo"},{"key":"cost_unpriced","value":"mock_provider"},{"key":"model","value":"mock/echo"},{"key":"provider","value":"mock"},{"key":"access","value":"mock"},{"key":"access_id","value":"mock"},{"key":"billing","value":"local"}],"id":{"uuid":"01a0ef11-03b5-73a3-9408-4cbec8c084b1"},"kind":"task_completed","run":null,"timestamp":1790717264821000000}"#,
        r#"{"chain":"dcffe12b5e1a6292070a56f5801489d7664e82bdb53d0ec5f7c16624e9a48aca","correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"workflow","value":"digest-notes"},{"key":"task","value":"approve"},{"key":"mode","value":"confirm"},{"key":"note","value":"awaiting a `nika:prompt` answer — resume with `--resume <trace> --answer <task>=<value>`"},{"key":"message","value":"Write ./digest.md from the draft?"},{"key":"status","value":"paused"},{"key":"cause","value":"human_gate"},{"key":"elapsed_ms","value":13},{"key":"tasks_total","value":2},{"key":"tasks_ok","value":2},{"key":"tasks_failed","value":0},{"key":"tasks_recovered","value":0},{"key":"tasks_skipped","value":0},{"key":"tasks_cancelled","value":0},{"key":"tasks_never_started","value":0},{"key":"priced_calls","value":0},{"key":"unpriced_calls","value":1},{"key":"cost_qualifier","value":"unpriced"}],"id":{"uuid":"01a0ef11-03b7-72c6-9f23-906232868f7b"},"kind":"workflow_paused","run":null,"timestamp":1790717264823000000}"#,
        r#"{"access_plan":[{"access":"mock","billing":"local","candidates":1,"chosen":"mock","model":"mock/echo","outranked":[],"pinned":false,"provider":"mock","rejected":[],"resolved":true,"trust":"observed"}],"cause":"human_gate","chain":"6df43c3de6d18fa00e53ff1cecce25ea897530684343b0e0b62b7161b807bd80","elapsed_ms":13,"evidence":"unsealed","execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"kind":"run_settled","outputs":{},"receipt":{"chain_head":"6df43c3de6d18fa00e53ff1cecce25ea897530684343b0e0b62b7161b807bd80","chain_len":12,"execution_id":"exe-01a0ef11-0212-70de-a8b3-99de9427fccc","receipt_format":1,"sealed":false,"snapshot_digest":"480c1b9fd2172466665cd8a842533ce9fb6346adb0146b46ae5a0829c483b1e2","trace_id":"01a0ef11021270dea8b399de9427fccc","trace_path":".nika/traces/2026-09-29T21-27-44Z-fccc.ndjson"},"spend":{"priced_calls":0,"qualifier":"unpriced","unpriced_calls":1},"status":"paused","tasks":{"cancelled":0,"failed":0,"never_started":0,"ok":2,"recovered":0,"skipped":0,"total":2}}"#,
    ];

    /// A sink that keeps what it heard.
    #[derive(Default)]
    struct Heard {
        lines: RefCell<Vec<String>>,
        frames: RefCell<Vec<RunFrame>>,
        unread: RefCell<Vec<&'static str>>,
    }

    impl RunSink for Heard {
        fn said(&self, line: String) {
            self.lines.borrow_mut().push(line);
        }

        fn frame(&self, frame: RunFrame) {
            self.frames.borrow_mut().push(frame);
        }

        fn unread(&self, why: &'static str) {
            self.unread.borrow_mut().push(why);
        }
    }

    /// `tell` says exactly the story `frame` says, line for line and byte
    /// for byte (the plain surfaces keep their words, the pause its `◇`),
    /// keeps the same trace, hands every frame the runtime wrote typed, and
    /// says a non-JSON line as unread (a refusal document is not one).
    #[test]
    fn telling_keeps_the_story_and_hands_the_frames_typed() {
        let refusal = r#"{"error":{"code":"NIKA-1709","message":"refusing to start"}}"#;
        let stream: Vec<&str> = REAL_LEG
            .iter()
            .copied()
            .chain(["not json at all", refusal])
            .collect();
        let mut plain = RunStory::default();
        let lines: Vec<String> = stream.iter().filter_map(|l| plain.frame(l)).collect();
        let mut told = RunStory::default();
        let heard = Heard::default();
        for line in &stream {
            told.tell(line, &heard);
        }
        assert_eq!(*heard.lines.borrow(), lines, "the same lines, in order");
        assert_eq!(told.lines, plain.lines);
        assert_eq!(told.trace, plain.trace);
        assert_eq!(
            lines,
            [
                "running · digest-notes",
                "→ read_notes · invoke · nika:read",
                "✔ read_notes · 4 ms · 1/4",
                "→ draft · infer · mock/echo",
                "✔ draft · 0 ms · 2/4",
                "◇ paused · `approve` asks you",
                "✖ refused · refusing to start",
            ]
        );
        assert_eq!(
            *heard.unread.borrow(),
            ["a line of the run's stream is not JSON"]
        );
        let frames = heard.frames.borrow();
        assert_eq!(frames.len(), REAL_LEG.len(), "one typed frame per line");
        let execution = frames[0].execution().expect("the leg's execution");
        assert!(
            frames.iter().all(|f| f.execution() == Some(execution)),
            "every frame names the same execution"
        );
        let kinds: Vec<EventKind> = frames
            .iter()
            .filter_map(|f| match f {
                RunFrame::Event(event) => Some(event.kind),
                RunFrame::Settled(_) => None,
            })
            .collect();
        assert_eq!(kinds.first(), Some(&EventKind::WorkflowStarted));
        assert_eq!(kinds.last(), Some(&EventKind::WorkflowPaused));
        let Some(RunFrame::Settled(settled)) = frames.last() else {
            panic!("the settlement closes the frames: {frames:?}");
        };
        assert_eq!(settled.settlement.state, RunState::Paused);
        assert_eq!(settled.execution, Some(execution));
        assert_eq!(settled.evidence, Some(Evidence::Unsealed));
        assert_eq!(settled.chain_len, Some(12));
        assert_eq!(
            settled.snapshot_digest.as_deref(),
            Some("480c1b9fd2172466665cd8a842533ce9fb6346adb0146b46ae5a0829c483b1e2")
        );
        assert_eq!(
            settled.trace_id.as_deref(),
            Some("01a0ef11021270dea8b399de9427fccc")
        );
        assert_eq!(settled.outputs, Outputs::Kept(serde_json::json!({})));
        assert_eq!(settled.trace, told.trace);
        assert_eq!(settled.resumed_from, None);
    }

    /// A plain busy sender hears the story lines only.
    #[test]
    fn a_line_sender_hears_the_story_alone() {
        let (busy, heard) = std::sync::mpsc::channel();
        let mut story = RunStory::default();
        for line in REAL_LEG {
            story.tell(line, &busy);
        }
        drop(busy);
        let heard: Vec<String> = heard.iter().collect();
        assert_eq!(heard, story.lines);
    }

    /// The decode types only runtime frames: a refusal document, a review
    /// question and noise have no twin; a kind it cannot type is unread,
    /// never a fabricated event; a settlement without its cause is unread.
    #[test]
    fn the_decode_types_only_runtime_frames() {
        assert!(RunFrame::decode("not json").is_none());
        assert!(RunFrame::decode(r#"{"clean":false,"findings":[]}"#).is_none());
        assert!(RunFrame::decode(r#"{"schema":"nika/run-cost-challenge@1"}"#).is_none());
        let heard = Heard::default();
        let mut story = RunStory::default();
        story.tell(r#"{"kind":"task_started","fields":[]}"#, &heard);
        story.tell(r#"{"kind":"run_settled","status":"paused"}"#, &heard);
        story.tell(r#"{"schema":"nika/run-cost-challenge@1"}"#, &heard);
        assert!(heard.frames.borrow().is_empty());
        assert_eq!(
            *heard.unread.borrow(),
            [
                "an event this reader cannot type",
                "a settlement this reader cannot type"
            ]
        );
        for (word, evidence) in [
            ("sealed", Evidence::Sealed),
            ("unsealed", Evidence::Unsealed),
            ("lost", Evidence::Lost),
            ("none", Evidence::NoJournal),
        ] {
            assert_eq!(Evidence::parse(word), Some(evidence));
        }
        assert_eq!(Evidence::parse("sealed "), None);
    }

    /// A settlement's outputs are kept whole within the bound, else only
    /// their size; an envelope without outputs carries none.
    #[test]
    fn the_outputs_are_kept_within_their_bound() {
        assert_eq!(Outputs::of(None), Outputs::Absent);
        let small = serde_json::json!({"said": "one"});
        assert_eq!(Outputs::of(Some(&small)), Outputs::Kept(small.clone()));
        let large = serde_json::json!({"said": "x".repeat(OUTPUTS_KEPT)});
        assert!(matches!(
            Outputs::of(Some(&large)),
            Outputs::TooLarge { bytes } if bytes > OUTPUTS_KEPT
        ));
    }

    /// The child row of a task settle types; no row, no child.
    #[test]
    fn a_child_row_is_an_observation_of_the_settle() {
        let call = r#"{"chain":"c","correlation":null,"fields":[{"key":"task","value":"sub"},{"key":"child","value":"{\"target\":\"./child.nika\",\"trace_id\":\"t-9\",\"chain_head\":\"ab\",\"outcome\":\"success\"}"}],"id":{"uuid":"01a0ef11-03b2-71ee-9ad4-17a755fad3ae"},"kind":"task_completed","run":null,"timestamp":1}"#;
        let Some(RunFrame::Event(event)) = RunFrame::decode(call) else {
            panic!("a task settle decodes");
        };
        assert_eq!(
            ChildRun::of(&event),
            Some(ChildRun {
                target: "./child.nika".to_owned(),
                trace_id: Some("t-9".to_owned()),
                succeeded: true,
                chain_head: Some("ab".to_owned()),
                def_hash: None,
                outcome: Some(ChildOutcome::Success),
            })
        );
        let Some(RunFrame::Event(first)) = RunFrame::decode(REAL_LEG[0]) else {
            panic!("a runtime event decodes");
        };
        assert_eq!(ChildRun::of(&first), None, "no child row, no child");
    }

    /// A start names its bytes by their sha256: the same bytes match, other
    /// bytes do not, and an event that is no start, or names no hash, says
    /// nothing.
    #[test]
    fn a_start_names_the_bytes_it_runs() {
        let bytes = b"nika: probe\n";
        let hash = nika_event::source_id::sha256_hex(bytes);
        let start = format!(
            r#"{{"correlation":null,"fields":[{{"key":"workflow","value":"probe"}},{{"key":"workflow_sha256","value":"{hash}"}}],"id":{{"uuid":"01a0ef11-03a1-73d9-a2bc-2548bdab1943"}},"kind":"workflow_started","run":null,"timestamp":1}}"#
        );
        let Some(RunFrame::Event(start)) = RunFrame::decode(&start) else {
            panic!("a start decodes");
        };
        assert_eq!(started_on(&start, bytes), Some(true));
        assert_eq!(started_on(&start, b"nika: other\n"), Some(false));
        let Some(RunFrame::Event(unhashed)) = RunFrame::decode(REAL_LEG[0]) else {
            panic!("a start decodes");
        };
        assert_eq!(started_on(&unhashed, bytes), None, "no hash, no claim");
        let Some(RunFrame::Event(task)) = RunFrame::decode(REAL_LEG[1]) else {
            panic!("an event decodes");
        };
        assert_eq!(started_on(&task, bytes), None);
    }

    /// Past its bound the story counts the lines it says and keeps no more.
    #[test]
    fn the_story_keeps_a_bounded_transcript() {
        let mut story = RunStory::default();
        let heard = Heard::default();
        let line = r#"{"kind":"task_skipped","fields":[{"key":"task","value":"t"}]}"#;
        for _ in 0..STORY_KEPT + 3 {
            story.tell(line, &heard);
        }
        assert_eq!(story.lines.len(), STORY_KEPT);
        assert_eq!(story.untold, 3);
        assert_eq!(heard.lines.borrow().len(), STORY_KEPT + 3, "all said");
    }

    /// A task settle frame of `kind` whose `child` field holds `row`.
    fn child_frame(kind: &str, row: &str) -> Event {
        let fields = serde_json::json!([
            {"key": "task", "value": "sub"},
            {"key": "child", "value": row},
        ]);
        let line = serde_json::json!({
            "chain": "c", "correlation": null, "fields": fields,
            "id": {"uuid": "01a0ef11-03b2-71ee-9ad4-17a755fad3ae"},
            "kind": kind, "run": null, "timestamp": 1,
        })
        .to_string();
        let Some(RunFrame::Event(event)) = RunFrame::decode(&line) else {
            panic!("a task frame decodes");
        };
        *event
    }

    /// The child row keeps every engagement the producer wrote; an outcome
    /// nobody recognises is unknown, never a failure; only a task's settle
    /// frame (`task_completed`) carries one.
    #[test]
    fn a_child_row_keeps_its_engagements_and_only_a_settle_carries_it() {
        let row = |outcome: &str| {
            serde_json::json!({"target": "./child.nika", "trace_id": "c.ndjson",
                "chain_head": "ab", "def_hash": "cd", "outcome": outcome})
            .to_string()
        };
        let child = ChildRun::of(&child_frame("task_completed", &row("success"))).expect("a child");
        assert_eq!(child.trace_id.as_deref(), Some("c.ndjson"));
        assert_eq!(child.chain_head.as_deref(), Some("ab"));
        assert_eq!(child.def_hash.as_deref(), Some("cd"));
        assert_eq!(child.outcome, Some(ChildOutcome::Success));
        assert!(child.succeeded);
        let failed =
            ChildRun::of(&child_frame("task_completed", &row("failure"))).expect("a child");
        assert_eq!(failed.outcome, Some(ChildOutcome::Failure));
        assert!(!failed.succeeded);
        let unknown = ChildRun::of(&child_frame("task_completed", &row("maybe"))).expect("a child");
        assert_eq!(unknown.outcome, None, "an unknown word is not a failure");
        let bare = ChildRun::of(&child_frame(
            "task_completed",
            r#"{"target":"./child.nika"}"#,
        ))
        .expect("a child");
        assert_eq!(
            (bare.trace_id, bare.chain_head, bare.def_hash, bare.outcome),
            (None, None, None, None)
        );
        for kind in [
            "task_started",
            "task_failed",
            "task_cache_hit",
            "workflow_completed",
        ] {
            assert_eq!(
                ChildRun::of(&child_frame(kind, &row("success"))),
                None,
                "{kind}"
            );
        }
        assert_eq!(
            ChildRun::of(&child_frame("task_completed", "{\"target\": ")),
            None
        );
    }

    /// A terminal frame of `kind` with `fields`.
    fn terminal_with(kind: &str, fields: &serde_json::Value) -> Event {
        let line = serde_json::json!({
            "correlation": null, "fields": fields,
            "id": {"uuid": "01a0ef11-03b2-71ee-9ad4-17a755fad3ae"},
            "kind": kind, "run": null, "timestamp": 1,
        })
        .to_string();
        let Some(RunFrame::Event(event)) = RunFrame::decode(&line) else {
            panic!("a frame decodes");
        };
        *event
    }

    /// The outputs map a terminal frame records is read strictly: each form
    /// alone is its state; none is Absent; two forms, a wrong type, a size
    /// within the cap, `false` or a JSON that is not an object is
    /// Unreadable; another frame's homonym is no workflow output.
    #[test]
    fn the_recorded_outputs_map_is_read_strictly() {
        let kv =
            |key: &str, value: serde_json::Value| serde_json::json!({"key": key, "value": value});
        let read = |kind: &str, fields: Vec<serde_json::Value>| {
            Outputs::from_event(&terminal_with(kind, &serde_json::Value::Array(fields)))
        };
        let big = i64::try_from(OUTPUTS_KEPT + 1).expect("size");
        for kind in ["workflow_completed", "workflow_failed"] {
            assert_eq!(read(kind, vec![]), Outputs::Absent);
            assert_eq!(
                read(
                    kind,
                    vec![kv("outputs", serde_json::json!("{\"total\":5,\"x\":null}"))]
                ),
                Outputs::Kept(serde_json::json!({"total": 5, "x": null}))
            );
            assert_eq!(
                read(kind, vec![kv("outputs", serde_json::json!("{}"))]),
                Outputs::Kept(serde_json::json!({}))
            );
            assert_eq!(
                read(kind, vec![kv("outputs_bytes", serde_json::json!(big))]),
                Outputs::TooLarge {
                    bytes: OUTPUTS_KEPT + 1
                }
            );
            assert_eq!(
                read(kind, vec![kv("outputs_withheld", serde_json::json!(true))]),
                Outputs::Withheld
            );
            assert_eq!(
                read(
                    kind,
                    vec![kv("outputs", serde_json::json!("{\"v\":\"***\"}"))]
                ),
                Outputs::Kept(serde_json::json!({"v": "***"})),
                "a marker in a value is a value"
            );
            for broken in [
                vec![
                    kv("outputs", serde_json::json!("{}")),
                    kv("outputs", serde_json::json!("{}")),
                ],
                vec![
                    kv("outputs", serde_json::json!("{}")),
                    kv("outputs_withheld", serde_json::json!(true)),
                ],
                vec![kv("outputs", serde_json::json!("[1]"))],
                vec![kv("outputs", serde_json::json!("null"))],
                vec![kv("outputs", serde_json::json!("{\"v\":"))],
                vec![kv("outputs", serde_json::json!(5))],
                vec![kv("outputs_bytes", serde_json::json!(10))],
                vec![kv("outputs_bytes", serde_json::json!(-1))],
                vec![kv("outputs_bytes", serde_json::json!("70000"))],
                vec![kv("outputs_withheld", serde_json::json!(false))],
            ] {
                assert_eq!(
                    read(kind, broken.clone()),
                    Outputs::Unreadable,
                    "{broken:?}"
                );
            }
        }
        for kind in ["task_completed", "workflow_cancelled", "run_sealed"] {
            assert_eq!(
                read(kind, vec![kv("outputs", serde_json::json!("{\"x\":1}"))]),
                Outputs::Absent,
                "{kind}"
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod identity_tests {
    use super::*;

    const EXEC: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";
    const OTHER: &str = "01a0ef11-0212-70de-a8b3-99de94270000";

    fn line(exec: &str, n: u32, kind: &str, fields: &str) -> String {
        format!(
            r#"{{"chain":"ab","correlation":null,"execution":{{"uuid":"{exec}"}},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
        )
    }

    fn start(exec: &str, n: u32, hash: &str) -> String {
        let fields = format!(r#"{{"key":"workflow_sha256","value":"{hash}"}}"#);
        line(exec, n, "workflow_started", &fields)
    }

    fn settled(exec: &str) -> RunFrame {
        let line = format!(
            r#"{{"kind":"run_settled","status":"succeeded","cause":"normal","execution":{{"uuid":"{exec}"}},"spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"evidence":"unsealed","receipt":{{"trace_path":".nika/traces/t.ndjson","chain_head":"cd","chain_len":7}}}}"#
        );
        RunFrame::decode(&line).expect("a settlement")
    }

    fn id(uuid: &str) -> ExecutionId {
        serde_json::from_value(serde_json::json!({ "uuid": uuid })).expect("an execution id")
    }

    /// The first execution binds the run: another execution's start and anything after the
    /// settlement add nothing, and the settlement names the journal and receipt.
    #[test]
    fn a_run_is_its_first_execution_until_its_settlement() {
        let mut run = RunIdentity::new();
        for frame in [start(EXEC, 1, "aa"), start(OTHER, 2, "bb")] {
            run.frame(&RunFrame::decode(&frame).expect("an event"));
        }
        run.frame(&settled(EXEC));
        run.frame(&RunFrame::decode(&start(EXEC, 3, "cc")).expect("an event"));
        run.frame(&settled(OTHER));
        assert_eq!(run.execution(), Some(id(EXEC)));
        assert_eq!(run.workflow_sha256(), Some("aa"));
        let trace = run.trace().map(|t| t.display().to_string());
        assert_eq!(trace.as_deref(), Some(".nika/traces/t.ndjson"));
        assert_eq!((run.chain_head(), run.chain_len()), (Some("cd"), Some(7)));
        assert!(run.settled());
    }

    /// Two starts of one execution name no source hash; an event naming no execution binds
    /// nothing.
    #[test]
    fn two_starts_name_no_source_hash_and_an_unnamed_event_binds_nothing() {
        let unnamed = r#"{"correlation":null,"fields":[{"key":"workflow_sha256","value":"zz"}],"id":{"uuid":"01a0ef11-03a7-74fb-bba0-000000000001"},"kind":"workflow_started","run":null,"timestamp":1}"#;
        let mut run = RunIdentity::new();
        run.frame(&RunFrame::decode(unnamed).expect("an event"));
        assert_eq!((run.execution(), run.workflow_sha256()), (None, None));
        for frame in [start(EXEC, 2, "aa"), start(EXEC, 3, "aa")] {
            run.frame(&RunFrame::decode(&frame).expect("an event"));
        }
        assert_eq!(run.execution(), Some(id(EXEC)));
        assert_eq!(run.workflow_sha256(), None);
    }

    /// A journal's own lines fold as the frames they are: its chain field is no frame of its
    /// own, a line that is not a frame adds nothing, and no journal names a settlement.
    #[test]
    fn a_journal_folds_its_own_lines() {
        let completed = line(EXEC, 2, "workflow_completed", "");
        let raw = format!("{}\nnot a frame\n{completed}\n", start(EXEC, 1, "aa"));
        let run = RunIdentity::of_journal(&raw);
        assert_eq!(run.execution(), Some(id(EXEC)));
        assert_eq!(run.workflow_sha256(), Some("aa"));
        assert!(!run.settled());
        assert_eq!((run.trace(), run.chain_head()), (None, None));
        assert_eq!(RunIdentity::of_journal(""), RunIdentity::new());
    }

    /// A bound execution keeps another's frames out; a restored identity is settled and adds
    /// nothing more.
    #[test]
    fn a_bound_or_restored_identity_keeps_its_execution() {
        let mut bound = RunIdentity::of_execution(id(EXEC));
        bound.frame(&RunFrame::decode(&start(OTHER, 1, "bb")).expect("an event"));
        assert_eq!(bound.workflow_sha256(), None);
        let mut kept = RunIdentity::restored(
            id(EXEC),
            Some("aa".to_owned()),
            None,
            (Some("cd".to_owned()), Some(4)),
        );
        kept.frame(&RunFrame::decode(&start(EXEC, 2, "bb")).expect("an event"));
        assert_eq!(kept.workflow_sha256(), Some("aa"));
        assert_eq!((kept.chain_head(), kept.chain_len()), (Some("cd"), Some(4)));
        assert!(kept.settled());
    }
}
