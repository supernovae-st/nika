// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session data model — the two truths a surface may render: the
//! WORKFLOW (what the file declares) and the RUN (what the trace
//! recorded). principles §1 · the file is the interface; a surface never
//! adds state the file does not carry, and every visible number is
//! DERIVED from these (see [`crate::derive`]), never hand-written.
//!
//! The field spellings mirror the studio's JSON exactly — the parity
//! fixtures are the contract, and the contract is read, never reinvented.

use serde::{Deserialize, Serialize};

/// The four verbs — the language's closed set, immutable forever
/// (D-2026-05-22-N18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verb {
    /// LLM generation.
    Infer,
    /// A shell command.
    Exec,
    /// A builtin or MCP tool call.
    Invoke,
    /// A multi-turn agentic loop.
    Agent,
}

impl Verb {
    /// The verb's lowercase spelling — the one wire voice (C-CONV · the
    /// `Touch::permit_key` precedent).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Infer => "infer",
            Self::Exec => "exec",
            Self::Invoke => "invoke",
            Self::Agent => "agent",
        }
    }
}

/// The five permit classes — what a step TOUCHES.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Touch {
    /// Reads the filesystem.
    FsRead,
    /// Writes the filesystem.
    FsWrite,
    /// Reaches the network.
    Net,
    /// Spawns a process.
    Exec,
    /// Invokes a tool.
    Tools,
}

impl Touch {
    /// The permit key a touch REQUIRES — one table, here.
    #[must_use]
    pub const fn permit_key(self) -> &'static str {
        match self {
            Self::FsRead => "fs.read",
            Self::FsWrite => "fs.write",
            Self::Net => "net",
            Self::Exec => "exec",
            Self::Tools => "tools",
        }
    }
}

/// ⭐ THE PROVENANCE — who wrote the code a step executes. The strongest
/// trust signal a surface can give: a builtin lives IN the audited
/// binary, an MCP tool is foreign code, a registry entry is foreign code
/// re-proven in CI. The three do not carry the same trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Origin {
    /// Ships in the engine binary.
    Builtin,
    /// Foreign code over MCP.
    Mcp,
    /// Foreign code re-proven by CI.
    Registry,
    /// A model seat.
    Model,
    /// A shell command.
    Shell,
}

/// The builtin families — their glyphs live in the SSOT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Family {
    /// The core set.
    Core,
    /// File tools.
    File,
    /// Data tools.
    Data,
    /// Network tools.
    Network,
    /// Introspection tools.
    Introspection,
    /// Media tools.
    Media,
}

/// One declared task. `needs` is what makes the waves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Task {
    /// The task id (the file's map key).
    pub id: String,
    /// Its verb.
    pub verb: Verb,
    /// Its glyph (a SSOT token name, carried opaque).
    pub glyph: String,
    /// The tasks this one depends on.
    #[serde(default)]
    pub needs: Vec<String>,
    /// What the step calls — `nika:read` · `mcp:browser/navigate` · a model.
    #[serde(default)]
    pub tool: Option<String>,
    /// Who wrote the called code.
    #[serde(default)]
    pub origin: Option<Origin>,
    /// The builtin family, when builtin.
    #[serde(default)]
    pub family: Option<Family>,
    /// The permit classes the step CONSUMES — the blast radius.
    #[serde(default)]
    pub touches: Option<Vec<Touch>>,
}

impl Task {
    /// The constructor the `#[non_exhaustive]` marker requires (invariant
    /// #19). Only the fields the WIRE requires are arguments; the rest stay
    /// `pub` — set what you mean, leave the rest.
    ///
    /// The marker was deferred once, on the argument that a `publish = false`
    /// crate has no external consumer to protect. The operator corrected the
    /// premise: the design and its functionality are FAR from finished, so
    /// these types will keep gaining fields. That is precisely when the
    /// ratchet earns its cost — it is what lets them grow without breaking a
    /// caller.
    #[must_use]
    pub fn new(id: String, verb: Verb, glyph: String, needs: Vec<String>) -> Self {
        Self {
            id,
            verb,
            glyph,
            needs,
            tool: None,
            origin: None,
            family: None,
            touches: None,
        }
    }
}

/// What the file declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Workflow {
    /// The workflow file name.
    pub file: String,
    /// The engine version that checked it.
    pub engine: String,
    /// The authoring prompt that produced it, when known.
    pub prompt: String,
    /// The declared permit lines (verbatim — the pouvoir lens reads them).
    pub permits: Vec<String>,
    /// What a successful run necessarily declared (the lens verifies).
    pub missing: String,
    /// The declared tasks.
    pub tasks: Vec<Task>,
}

impl Workflow {
    /// The constructor the `#[non_exhaustive]` marker requires (invariant
    /// #19). Only the fields the WIRE requires are arguments; the rest stay
    /// `pub` — set what you mean, leave the rest.
    ///
    /// The marker was deferred once, on the argument that a `publish = false`
    /// crate has no external consumer to protect. The operator corrected the
    /// premise: the design and its functionality are FAR from finished, so
    /// these types will keep gaining fields. That is precisely when the
    /// ratchet earns its cost — it is what lets them grow without breaking a
    /// caller.
    #[must_use]
    pub const fn new(
        file: String,
        engine: String,
        prompt: String,
        permits: Vec<String>,
        missing: String,
        tasks: Vec<Task>,
    ) -> Self {
        Self {
            file,
            engine,
            prompt,
            permits,
            missing,
            tasks,
        }
    }
}

/// A recorded failure — the code and its reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Failure {
    /// The NIKA code.
    pub code: String,
    /// The engine's one-line reason.
    pub why: String,
}

impl Failure {
    /// The constructor the `#[non_exhaustive]` marker requires (invariant #19).
    #[must_use]
    pub const fn new(code: String, why: String) -> Self {
        Self { code, why }
    }
}

/// One recorded step of a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Step {
    /// The task id it ran.
    pub id: String,
    /// Seconds since the run began.
    pub start: f64,
    /// Its duration in seconds.
    pub dur: f64,
    /// Its spend, when priced.
    #[serde(default)]
    pub cost: Option<f64>,
    /// Its tokens, when metered.
    #[serde(default)]
    pub tokens: Option<u64>,
    /// Set when the step failed.
    #[serde(default)]
    pub failed: Option<Failure>,
    /// Never started — a dependency broke.
    #[serde(default)]
    pub never_born: Option<bool>,
    /// The CULPABLE dependency — the trace says `blocked_by`; `needs[0]`
    /// can lie.
    #[serde(default)]
    pub blocked_by: Option<String>,
    /// The engine said `task_skipped` (a `when:` gate or an `on_error:
    /// skip`) — the step exists in the DAG and never ran. Absent from the
    /// studio's own data (the field is the engine-side enrichment); a run
    /// without skips never carries the key at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<bool>,
}

impl Step {
    /// The constructor the `#[non_exhaustive]` marker requires (invariant
    /// #19). Only the fields the WIRE requires are arguments; the rest stay
    /// `pub` — set what you mean, leave the rest.
    ///
    /// The marker was deferred once, on the argument that a `publish = false`
    /// crate has no external consumer to protect. The operator corrected the
    /// premise: the design and its functionality are FAR from finished, so
    /// these types will keep gaining fields. That is precisely when the
    /// ratchet earns its cost — it is what lets them grow without breaking a
    /// caller.
    #[must_use]
    pub const fn new(id: String, start: f64, dur: f64) -> Self {
        Self {
            id,
            start,
            dur,
            cost: None,
            tokens: None,
            failed: None,
            never_born: None,
            blocked_by: None,
            skipped: None,
        }
    }
}

/// What the trace recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Run {
    /// The trace id.
    pub trace: String,
    /// When it ran — or the declared sentinel (`never ran` · `synthetic`).
    pub when: String,
    /// The run's final output, summarized.
    pub output: String,
    /// The recorded steps.
    pub steps: Vec<Step>,
}

impl Run {
    /// The constructor the `#[non_exhaustive]` marker requires (invariant
    /// #19). Only the fields the WIRE requires are arguments; the rest stay
    /// `pub` — set what you mean, leave the rest.
    ///
    /// The marker was deferred once, on the argument that a `publish = false`
    /// crate has no external consumer to protect. The operator corrected the
    /// premise: the design and its functionality are FAR from finished, so
    /// these types will keep gaining fields. That is precisely when the
    /// ratchet earns its cost — it is what lets them grow without breaking a
    /// caller.
    #[must_use]
    pub const fn new(trace: String, when: String, output: String, steps: Vec<Step>) -> Self {
        Self {
            trace,
            when,
            output,
            steps,
        }
    }
}

/// ⭐⭐ THE FAN-OUT — one line in the display is either a single task or
/// a group of tasks sharing verb, tool AND dependencies (the exact
/// signature of a declared `for_each`). Derived, never annotated.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Group {
    /// A task with no twins.
    Single(Task),
    /// Three or more tasks with one signature — the group's `name` is the
    /// common prefix (what the `for_each` named once).
    Fanout {
        /// The common prefix.
        name: String,
        /// The grouped members, in declared order.
        members: Vec<Task>,
    },
}

impl Group {
    /// The group marker (the studio's `isGroup`).
    #[must_use]
    pub const fn is_fanout(&self) -> bool {
        matches!(self, Self::Fanout { .. })
    }
}

/// The wire vocabulary (`{task: id}` · `{group, members: [ids]}`) lives
/// HERE, once — the studio's gen-parity emits it, the parity test reads
/// it, and the wasm door serves it. Members serialize as IDS (the full
/// tasks are the caller's already).
impl serde::Serialize for Group {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Single(t) => serde_json::json!({ "task": t.id }),
            Self::Fanout { name, members } => serde_json::json!({
                "group": name,
                "members": members.iter().map(|m| &m.id).collect::<Vec<_>>(),
            }),
        }
        .serialize(ser)
    }
}
