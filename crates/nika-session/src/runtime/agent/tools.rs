// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tools the Session serves to the intelligence that leads its conversation
//! (`nika/author-tools@0`). A tool reads or changes the conversation ([`Conversation`]) and asks
//! the Session's own capabilities ([`Desk`]: the project, the oracle, the judge); it never writes
//! a project file, runs a workflow or grants anything. `ask` ends the turn when it asks; every
//! other reply is text the model reads. The tools live as long as the conversation: Nika's own
//! loop calls them during a turn, an ACP agent reaches them over MCP on the tool server's
//! thread, and between turns no call reaches a capability.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use nika_compile_fidelity::fidelity::resolution::Resolution;
use nika_session_change::outcome::QuestionId;
use nika_session_change::tools::{SessionTools, ToolCall, ToolDef, ToolReply};
use nika_session_change::work::ModelFacts;
use serde_json::{Value, json};

use nika_session_agent::conversation::{Acts, Citations, Conversation};

/// The tools' definitions: names, descriptions and argument schemas (`$defs` inlined).
const DEFINITIONS: &str = include_str!("../../../assets/author_tools.json");

/// The Session's capabilities a tool asks for, beside the conversation.
pub(crate) trait Desk: Send {
    /// A window of a project file (contained, bounded, redacted).
    fn read(
        &mut self,
        path: &str,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<String, String>;
    /// Whether `source` parses strictly; why not, as the model reads it.
    fn parse(&mut self, source: &str) -> Result<(), String>;
    /// The check report (`check`) or the task graph (`inspect`) of `source`.
    fn report(&mut self, source: &str, inspect: bool) -> Result<String, String>;
    /// A diagnostic code explained.
    fn explain(&mut self, code: &str) -> Result<String, String>;
    /// A language page: schema, catalog, examples or template.
    fn language(&mut self, topic: &str, query: Option<&str>) -> Result<String, String>;
    /// The models this Session offers for a role, or the one the person's ask names.
    fn models(&mut self, role: &str, ask: &Value) -> Result<String, String>;
    /// The pinned knowledge release: a search or a skill.
    fn knowledge(&mut self, query: Option<&str>, skill: Option<&str>) -> Result<String, String>;
    /// One public GET, observed.
    fn observe(&mut self, url: &str) -> Result<String, String>;
    /// Document operations applied to `source`: the new source.
    fn compose(&mut self, source: &str, operations: &[Value]) -> Result<String, String>;
    /// The candidate judged whole against what the person `stated`, with its selections
    /// (stated by the author, verified by the Session): the digest of what it may do.
    fn verify(
        &mut self,
        source: &str,
        stated: &str,
        selections: (&[Resolution], &[Resolution]),
    ) -> Result<String, String>;
    /// What Nika judges of `source` with no model: its check findings, then the laws over the
    /// person's words (`stated`) and its selections. Empty when nothing stands against it.
    fn judged_now(
        &mut self,
        source: &str,
        stated: &str,
        selections: (&[Resolution], &[Resolution]),
    ) -> Vec<String>;
    /// What this machine's inventory says of `model` as a run's model; none when it offers none.
    fn model_facts(&mut self, model: &str) -> Option<ModelFacts>;
}

/// What a run decided that the Session acts on after it: a proposal shown, and the acts the
/// person's own words authorized for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Decided {
    /// The candidate revision proposed last, when one was.
    pub(crate) proposed: Option<u64>,
    /// The acts the person's words authorized, when any.
    pub(crate) acts: Option<Acts>,
}

/// A question's identity, from the context it was asked in.
pub(crate) type Mint = dyn FnMut(&str) -> QuestionId + Send;

/// What a call reaching the tools between the Session's turns reads.
const NO_TURN: &str = "No turn is under way: nothing runs until the person writes.";

/// The conversation's tools: the conversation, the person's citations as the tree made them
/// durable, and, while a turn is under way, the Session's capabilities and how a question gets
/// its identity.
pub(crate) struct Toolbox {
    state: Mutex<State>,
}

struct State {
    conversation: Conversation,
    citations: Arc<Mutex<Citations>>,
    turn: Option<(Box<dyn Desk>, Box<Mint>)>,
    decided: Decided,
}

/// One call's view of the tools.
struct Parts<'a> {
    conversation: &'a mut Conversation,
    citations: &'a Arc<Mutex<Citations>>,
    desk: &'a mut dyn Desk,
    mint: &'a mut Mint,
    decided: &'a mut Decided,
}

impl Toolbox {
    /// The tools of `conversation`, whose person's lines `citations` indexes.
    pub(crate) fn new(conversation: Conversation, citations: Arc<Mutex<Citations>>) -> Self {
        let state = State {
            conversation,
            citations,
            turn: None,
            decided: Decided::default(),
        };
        Self {
            state: Mutex::new(state),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A turn begins: the tools reach `desk`, and a question asked gets its identity from
    /// `mint`.
    pub(crate) fn begin(&self, desk: Box<dyn Desk>, mint: Box<Mint>) {
        let mut state = self.state();
        state.turn = Some((desk, mint));
        state.decided = Decided::default();
    }

    /// The turn ended: what it decided. No call reaches a capability until the next turn.
    pub(crate) fn end(&self) -> Decided {
        let mut state = self.state();
        state.turn = None;
        state.decided
    }

    /// Read or change the conversation.
    pub(crate) fn with<R>(&self, act: impl FnOnce(&mut Conversation) -> R) -> R {
        act(&mut self.state().conversation)
    }
}

/// The tools' definitions, `$defs` inlined into each argument schema.
pub(crate) fn definitions() -> Vec<ToolDef> {
    let Ok(document) = serde_json::from_str::<Value>(DEFINITIONS) else {
        return Vec::new();
    };
    let defs = document["$defs"].clone();
    (document["tools"].as_array().into_iter().flatten())
        .map(|tool| {
            let mut schema = tool["input_schema"].clone();
            inline(&mut schema, &defs);
            ToolDef::new(
                tool["name"].as_str().unwrap_or_default(),
                tool["description"].as_str().unwrap_or_default(),
                schema,
                tool["read_only"].as_bool().unwrap_or(false),
            )
        })
        .collect()
}

/// Replace every `{"$ref": "#/$defs/x"}` by the definition `x`, recursively.
fn inline(node: &mut Value, defs: &Value) {
    let name = (node.get("$ref").and_then(Value::as_str))
        .and_then(|r| r.strip_prefix("#/$defs/"))
        .map(str::to_owned);
    if let Some(name) = name {
        let mut resolved = defs[name.as_str()].clone();
        inline(&mut resolved, defs);
        *node = resolved;
        return;
    }
    match node {
        Value::Object(map) => map.values_mut().for_each(|value| inline(value, defs)),
        Value::Array(items) => items.iter_mut().for_each(|value| inline(value, defs)),
        _ => {}
    }
}

fn reply(result: Result<String, String>) -> ToolReply {
    match result {
        Ok(text) => ToolReply::ok(text),
        Err(why) => ToolReply::error(why),
    }
}

impl SessionTools for Toolbox {
    fn tools(&self) -> Vec<ToolDef> {
        definitions()
    }

    fn call(&self, call: ToolCall) -> ToolReply {
        let mut state = self.state();
        let State {
            conversation,
            citations,
            turn,
            decided,
        } = &mut *state;
        let Some((desk, mint)) = turn.as_mut() else {
            return ToolReply::error(NO_TURN);
        };
        let mut parts = Parts {
            conversation,
            citations,
            desk: &mut **desk,
            mint: &mut **mint,
            decided,
        };
        let args = &call.arguments;
        let text = |field: &str| args[field].as_str().map(str::to_owned);
        match call.name.as_str() {
            "read" => {
                let (offset, limit) = (args["offset"].as_u64(), args["limit"].as_u64());
                let path = text("path").unwrap_or_default();
                reply(parts.desk.read(&path, offset, limit))
            }
            "candidate_read" => parts.candidate_read(),
            "candidate_write" => match text("source") {
                Some(source) => parts.write(source, text("summary"), args),
                None => ToolReply::error("candidate_write needs the complete document in `source`"),
            },
            "candidate_edit" => parts.edit(args),
            "check" | "inspect" => parts.report(call.name == "inspect"),
            "explain" => reply(parts.desk.explain(&text("code").unwrap_or_default())),
            "language" => {
                let topic = text("topic").unwrap_or_default();
                reply(parts.desk.language(&topic, text("query").as_deref()))
            }
            "models" => reply(parts.desk.models(&text("role").unwrap_or_default(), args)),
            "knowledge" => {
                let (query, skill) = (text("query"), text("skill"));
                reply(parts.desk.knowledge(query.as_deref(), skill.as_deref()))
            }
            "observe" => reply(parts.desk.observe(&text("url").unwrap_or_default())),
            "compose" => parts.compose(args),
            // The trial runs within the verification: `trial` asks for both.
            "verify" | "trial" => parts.verify(),
            "ask" => parts.ask(args, call.meta.as_deref()),
            "propose" => parts.propose(args),
            "new_request" => {
                let citations = parts.citations();
                parts.conversation.replace(&citations, args)
            }
            other => ToolReply::error(format!("no tool `{other}` in this Session")),
        }
    }
}

impl Parts<'_> {
    fn citations(&self) -> Citations {
        (self.citations.lock())
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn source(&self) -> Option<String> {
        self.conversation.candidate().map(|c| c.source.clone())
    }

    fn candidate_read(&self) -> ToolReply {
        let Some(candidate) = self.conversation.candidate() else {
            return ToolReply::ok(json!({"revision": null}).to_string());
        };
        let rows: Vec<Value> = candidate.rows.iter().map(Resolution::to_json).collect();
        let read = json!({"revision": candidate.number, "source": candidate.source,
            "summary": candidate.summary, "resolutions": rows});
        ToolReply::ok(read.to_string())
    }

    fn write(&mut self, source: String, summary: Option<String>, args: &Value) -> ToolReply {
        let rows = args["resolutions"].as_array().cloned().unwrap_or_default();
        let removed = args["removed"].as_array().cloned().unwrap_or_default();
        let citations = self.citations();
        let desk = &mut *self.desk;
        let texts = (source, summary.unwrap_or_default());
        let reply =
            (self.conversation).write(&citations, texts, (&rows, &removed), &mut |source| {
                desk.parse(source)
            });
        self.with_verdict(reply)
    }

    /// A written candidate's reply with what Nika judges of it with no model, so the author can
    /// propose in the same message: its check findings and its laws (`findings`, empty when
    /// nothing stands against it) and the values it dropped of those the person saw. `propose`
    /// still verifies the whole candidate, its trial and its judge included.
    fn with_verdict(&mut self, mut reply: ToolReply) -> ToolReply {
        let (Some(source), false) = (self.source(), reply.is_error) else {
            return reply;
        };
        let Ok(mut written) = serde_json::from_str::<Value>(&reply.text) else {
            return reply;
        };
        let stated = self.citations().stated(self.conversation.since());
        let (authored, host) = self.conversation.selections();
        let findings = self.desk.judged_now(&source, &stated, (&authored, &host));
        written["findings"] = json!(findings);
        written["dropped"] = json!(self.conversation.dropped());
        reply.text = written.to_string();
        reply
    }

    fn edit(&mut self, args: &Value) -> ToolReply {
        let Some(candidate) = self.conversation.candidate() else {
            return ToolReply::error("no candidate to edit: write one with candidate_write");
        };
        let old = args["old"].as_str().unwrap_or_default();
        let new = args["new"].as_str().unwrap_or_default();
        let found = candidate.source.matches(old).count();
        if old.is_empty() || found != 1 {
            return ToolReply::error(format!(
                "`old` must occur exactly once in the candidate; it occurs {found} times"
            ));
        }
        let source = candidate.source.replacen(old, new, 1);
        let mut args = args.clone();
        if args["resolutions"].is_null() {
            let rows: Vec<Value> = candidate.rows.iter().map(Resolution::to_json).collect();
            args["resolutions"] = Value::Array(rows);
        }
        let summary =
            (args["summary"].as_str()).map_or_else(|| candidate.summary.clone(), str::to_owned);
        self.write(source, Some(summary), &args)
    }

    fn report(&mut self, inspect: bool) -> ToolReply {
        match self.source() {
            Some(source) => reply(self.desk.report(&source, inspect)),
            None => ToolReply::error("no candidate yet: write one with candidate_write"),
        }
    }

    fn compose(&mut self, args: &Value) -> ToolReply {
        let base = self.source().unwrap_or_default();
        let operations = args["operations"].as_array().cloned().unwrap_or_default();
        match self.desk.compose(&base, &operations) {
            Ok(source) => self.write(source, args["summary"].as_str().map(str::to_owned), args),
            Err(why) => ToolReply::error(why),
        }
    }

    /// The candidate judged whole: what it dropped, then the judge. Returns its scope.
    fn judged(&mut self) -> Result<String, String> {
        let source = self
            .source()
            .ok_or("no candidate to judge: write one with candidate_write")?;
        let dropped = self.conversation.dropped();
        if !dropped.is_empty() {
            return Err(dropped.join("\n"));
        }
        let stated = self.citations().stated(self.conversation.since());
        let (authored, host) = self.conversation.selections();
        self.desk.verify(&source, &stated, (&authored, &host))
    }

    fn verify(&mut self) -> ToolReply {
        let revision = self.conversation.candidate().map(|c| c.number);
        match self.judged() {
            Ok(_) => ToolReply::ok(json!({"verdict": "ready", "revision": revision}).to_string()),
            Err(findings) => ToolReply::error(findings),
        }
    }

    fn ask(&mut self, args: &Value, call: Option<&str>) -> ToolReply {
        let citations = self.citations();
        let (mint, desk) = (&mut *self.mint, &mut *self.desk);
        let mut mint = |context: &str| mint(context);
        let mut facts = |model: &str| desk.model_facts(model);
        (self.conversation).ask(&citations, args, call, (&mut mint, &mut facts))
    }

    /// Judge the candidate, then show it; with the person's words, the acts they authorize.
    fn propose(&mut self, args: &Value) -> ToolReply {
        let scope = match self.judged() {
            Ok(scope) => scope,
            Err(findings) => return ToolReply::error(findings),
        };
        let revision = self.conversation.candidate().map(|c| c.number);
        let citations = self.citations();
        let authorized = self.conversation.propose(&citations, args, scope);
        let acts = authorized.as_ref().ok().copied().flatten();
        *self.decided = Decided {
            proposed: revision,
            acts,
        };
        let said = match (&authorized, acts) {
            (_, Some(acts)) if acts.run => {
                "the person's words authorize it: the Session saves it, then asks its run"
            }
            (_, Some(_)) => "the person's words authorize it: the Session saves it",
            (Err(_), None) => "shown to the person for their consent; nothing saved",
            (Ok(_), None) => "shown to the person for review: they save it, or say what to change",
        };
        let mut text = json!({"revision": revision, "proposal": said});
        if let Err(why) = authorized {
            text["why"] = Value::String(why);
        }
        // Shown, the proposal ends the turn: the person's next line answers it.
        ToolReply::ends_turn(text.to_string())
    }
}
