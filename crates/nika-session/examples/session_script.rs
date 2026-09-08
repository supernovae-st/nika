//! JSONL host over the public [`nika_session::SessionRuntime`].
//!
//! One session at a time, stdin commands, stdout events. Not a second
//! machine and not `nika session`: this process never runs a workflow.
//! `run_requested` is a request; `observe_run` is an observation the
//! caller supplies. Stale preimages are injected on the real filesystem
//! by the outer controller. `reset` drops the runtime and the id cache.
//!
//! Stdin (`op`): `open`/`reset` `{root, replies?, home?}` · `feed` `{text}`
//! (queued until the next open) · `turn` `{text}` · `consent`
//! `{answer, id?}` · `observe_run` `{exit, trace?}` · `answer_gate`
//! `{line, id?}` · `pending` · `quit`.
//!
//! Stdout (`v:1`, `event`): `opened` · `fed` · `proposal`/`held` ·
//! `refusal` · `reply`/`facts`/`help`/`ask` · `run_requested` ·
//! `resume_requested` · `gate_ask` · `pending` · `quit` · `host`.

use std::io::{self, BufRead as _, Write as _};
use std::path::{Path, PathBuf};

use nika_session::RunRequest;
use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference,
};
use nika_session::outcome::{GateId, ProposalId, Refusal};
use nika_session::reasoner::ScriptedReasoner;
use nika_session::runtime::{SessionRuntime, TurnOutcome};
use serde_json::{Value, json};

fn main() -> io::Result<()> {
    let mut host = Host::new();
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match host.handle(&line) {
            Ok(Next::Continue) => {}
            Ok(Next::Quit) => {
                emit(&json!({"v": 1, "event": "quit"}))?;
                return Ok(());
            }
            Err(err) => emit(&host_err(&err))?,
        }
    }
    Ok(())
}

enum Next {
    Continue,
    Quit,
}

struct Host {
    session: Option<SessionRuntime>,
    proposals: Vec<(String, ProposalId)>,
    gates: Vec<(String, GateId)>,
    queued: Vec<String>,
}

impl Host {
    fn new() -> Self {
        Self {
            session: None,
            proposals: Vec::new(),
            gates: Vec::new(),
            queued: Vec::new(),
        }
    }

    fn reset_state(&mut self) {
        *self = Self::new();
    }

    fn handle(&mut self, line: &str) -> Result<Next, String> {
        let v: Value = serde_json::from_str(line).map_err(|e| format!("invalid json: {e}"))?;
        let op = v
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing op".to_owned())?;
        match op {
            "open" | "reset" => self.op_open(&v, op == "reset"),
            "feed" => self.op_feed(&v),
            "turn" => self.op_session(&v, HostOp::Turn),
            "consent" => self.op_session(&v, HostOp::Consent),
            "observe_run" => self.op_session(&v, HostOp::ObserveRun),
            "answer_gate" => self.op_session(&v, HostOp::AnswerGate),
            "pending" => self.op_pending(),
            "quit" => Ok(Next::Quit),
            other => Err(format!("unknown op: {other}")),
        }
    }

    fn op_open(&mut self, v: &Value, reset: bool) -> Result<Next, String> {
        if reset {
            self.reset_state();
        }
        let root = v
            .get("root")
            .and_then(Value::as_str)
            .ok_or_else(|| "open requires root".to_owned())?;
        let mut replies = std::mem::take(&mut self.queued);
        if let Some(arr) = v.get("replies").and_then(Value::as_array) {
            for item in arr {
                let text = item
                    .as_str()
                    .ok_or_else(|| "replies must be strings".to_owned())?;
                replies.push(text.to_owned());
            }
        }
        let intelligence = scripted_intelligence();
        let reasoner = ScriptedReasoner::new(replies);
        let mut session = SessionRuntime::open(Path::new(root), intelligence, Box::new(reasoner));
        let recovery = if let Some(home) = v.get("home").and_then(Value::as_str) {
            session
                .enable_history(Path::new(home))
                .map_err(|why| why.to_string())?
        } else {
            None
        };
        self.proposals.clear();
        self.gates.clear();
        let shown = session.snapshot.root.display().to_string();
        self.session = Some(session);
        emit_ok(&json!({"v": 1, "event": "opened", "root": shown,
            "recovery": recovery, "goal": self.session.as_ref().and_then(|s| s.intent.goal.as_ref())}))?;
        Ok(Next::Continue)
    }

    fn op_feed(&mut self, v: &Value) -> Result<Next, String> {
        if self.session.is_some() {
            return Err(
                "feed is queued for the next open; a live session uses replies at open".to_owned(),
            );
        }
        let text = v
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| "feed requires text".to_owned())?;
        self.queued.push(text.to_owned());
        emit_ok(&json!({"v": 1, "event": "fed"}))?;
        Ok(Next::Continue)
    }

    fn op_pending(&mut self) -> Result<Next, String> {
        let session = self.session.as_ref().ok_or_else(no_session)?;
        let id = session.pending_proposal().map(|p| p.as_str().to_owned());
        emit_ok(&json!({"v": 1, "event": "pending", "id": id}))?;
        Ok(Next::Continue)
    }

    fn op_session(&mut self, v: &Value, op: HostOp) -> Result<Next, String> {
        let outcome = {
            let session = self.session.as_mut().ok_or_else(no_session)?;
            match op {
                HostOp::Turn => {
                    let text = str_field(v, "text")?;
                    session.turn(text)
                }
                HostOp::Consent => self.consent(v)?,
                HostOp::ObserveRun => {
                    let exit = u8_field(v, "exit")?;
                    let trace = v.get("trace").and_then(Value::as_str).map(PathBuf::from);
                    session.observe_run(exit, trace.as_deref())
                }
                HostOp::AnswerGate => self.answer_gate(v)?,
            }
        };
        self.emit_outcome(outcome)
    }

    fn consent(&mut self, v: &Value) -> Result<TurnOutcome, String> {
        let answer = str_field(v, "answer")?.to_owned();
        let named = match v.get("id").and_then(Value::as_str) {
            None => None,
            Some(id) => Some(
                self.proposals
                    .iter()
                    .find(|(k, _)| k == id)
                    .map(|(_, p)| p.clone())
                    .ok_or_else(|| format!("unknown proposal id: {id}"))?,
            ),
        };
        let session = self.session.as_mut().ok_or_else(no_session)?;
        Ok(match named {
            None => session.consent(&answer),
            Some(pid) => session.consent_to(&pid, &answer),
        })
    }

    fn answer_gate(&mut self, v: &Value) -> Result<TurnOutcome, String> {
        let line = str_field(v, "line")?.to_owned();
        let named = match v.get("id").and_then(Value::as_str) {
            None => None,
            Some(id) => Some(
                self.gates
                    .iter()
                    .find(|(k, _)| k == id)
                    .map(|(_, g)| g.clone())
                    .ok_or_else(|| format!("unknown gate id: {id}"))?,
            ),
        };
        let session = self.session.as_mut().ok_or_else(no_session)?;
        Ok(match named {
            None => session.answer_gate(&line),
            Some(gid) => session.answer_gate_for(&gid, &line),
        })
    }

    fn emit_outcome(&mut self, outcome: TurnOutcome) -> Result<Next, String> {
        let event = match outcome {
            TurnOutcome::Reply(text) => json!({"v": 1, "event": "reply", "text": text}),
            TurnOutcome::Facts(text) => json!({"v": 1, "event": "facts", "text": text}),
            TurnOutcome::Help(text) => json!({"v": 1, "event": "help", "text": text}),
            TurnOutcome::Ask(text) => json!({"v": 1, "event": "ask", "text": text}),
            TurnOutcome::Quit => {
                emit_ok(&json!({"v": 1, "event": "quit"}))?;
                return Ok(Next::Quit);
            }
            TurnOutcome::Refusal(refusal) => refusal_event(&refusal),
            TurnOutcome::Proposal { id, preview } => {
                let hex = remember_proposal(&mut self.proposals, id);
                json!({"v": 1, "event": "proposal", "id": hex, "preview": preview})
            }
            TurnOutcome::Held { id, preview } => {
                let hex = remember_proposal(&mut self.proposals, id);
                json!({"v": 1, "event": "held", "id": hex, "preview": preview})
            }
            TurnOutcome::RunRequested { report, run } => run_requested_event(&report, &run),
            TurnOutcome::ResumeRequested {
                workflow,
                trace,
                answer,
            } => json!({
                "v": 1,
                "event": "resume_requested",
                "workflow": workflow.display().to_string(),
                "trace": trace.display().to_string(),
                "answer": answer
            }),
            TurnOutcome::GateAsk { id, question } => {
                let key = remember_gate(&mut self.gates, id);
                json!({
                    "v": 1,
                    "event": "gate_ask",
                    "id": key,
                    "question": question
                })
            }
            _ => json!({"v": 1, "event": "host", "ok": false, "text": "unknown outcome"}),
        };
        emit_ok(&event)?;
        Ok(Next::Continue)
    }
}

#[derive(Clone, Copy)]
enum HostOp {
    Turn,
    Consent,
    ObserveRun,
    AnswerGate,
}

fn scripted_intelligence() -> ResolvedSessionIntelligence {
    // Kind `None` resolves ready without probing this machine; the
    // scripted reasoner still answers because `turn` calls it whenever
    // `ready` is true. No provider, no key, no paid path.
    let pref = UserIntelligencePreference::new(IntelligenceKind::None, None);
    ResolvedSessionIntelligence::resolve(&pref, &IntelligenceCensus::empty())
}

fn remember_proposal(map: &mut Vec<(String, ProposalId)>, id: ProposalId) -> String {
    let hex = id.as_str().to_owned();
    map.retain(|(k, _)| k != &hex);
    map.push((hex.clone(), id));
    hex
}

fn remember_gate(map: &mut Vec<(String, GateId)>, id: GateId) -> String {
    let key = format!("{}:{}", id.trace.display(), id.task);
    map.retain(|(k, _)| k != &key);
    map.push((key.clone(), id));
    key
}

fn refusal_event(refusal: &Refusal) -> Value {
    json!({
        "v": 1,
        "event": "refusal",
        "class": refusal.class.as_str(),
        "text": refusal.text
    })
}

fn run_requested_event(report: &str, run: &RunRequest) -> Value {
    json!({
        "v": 1,
        "event": "run_requested",
        "report": report,
        "workflow": run.workflow.display().to_string(),
        "vars": run.vars,
        "max_cost_usd": run.max_cost_usd
    })
}

fn str_field<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} must be a string"))
}

fn u8_field(v: &Value, key: &str) -> Result<u8, String> {
    let n = v
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{key} must be an integer"))?;
    u8::try_from(n).map_err(|_| format!("{key} out of range"))
}

fn no_session() -> String {
    "no session — open a root first".to_owned()
}

fn host_err(text: &str) -> Value {
    json!({"v": 1, "event": "host", "ok": false, "text": text})
}

fn emit(v: &Value) -> io::Result<()> {
    let mut out = io::stdout();
    writeln!(out, "{v}")?;
    out.flush()
}

fn emit_ok(v: &Value) -> Result<(), String> {
    emit(v).map_err(|e| e.to_string())
}
