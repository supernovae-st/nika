// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tools a session serves to the intelligence that leads its conversation
//! (`nika/author-tools@0`), typed once for every loop that calls them: Nika's own agent loop
//! over a provider, and an ACP agent's loop reaching them over MCP. The session implements
//! [`SessionTools`] against its own state; a loop or a transport lists the definitions, hands
//! each call over and relays the reply. Neither interprets a tool, and no reply grants anything:
//! a save, a run or a consent still goes through the session's own doors.
//!
//! The contract is versioned by [`CONTRACT`]; [`NAMES`] are the tool names it fixes, each a
//! lowercase word that an MCP client mounts unchanged (`mcp__nika__<name>`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The version a loop or a transport checks before relaying these tools.
pub const CONTRACT: &str = "nika/author-tools@0";

/// The tool names the contract fixes, in the order a session lists them.
pub const NAMES: [&str; 17] = [
    "read",
    "candidate_read",
    "candidate_write",
    "candidate_edit",
    "check",
    "inspect",
    "explain",
    "language",
    "models",
    "knowledge",
    "observe",
    "compose",
    "verify",
    "trial",
    "ask",
    "propose",
    "new_request",
];

/// One tool as a session defines it: its name, what it does in the words the intelligence
/// reads, the JSON Schema of its arguments and whether it only reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolDef {
    /// The tool's name, one of [`NAMES`].
    pub name: String,
    /// What the tool does, for the intelligence that chooses it.
    pub description: String,
    /// The JSON Schema its arguments satisfy.
    pub input_schema: Value,
    /// Whether the tool only reads (it changes no candidate, question or proposal).
    pub read_only: bool,
}

impl ToolDef {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        read_only: bool,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            read_only,
        }
    }
}

/// One call the intelligence made: the tool's name, its arguments as sent, and the identity
/// that ties it to the intelligence's own tool call when the loop knows it (over MCP the
/// tool-use id the client names in the request's `_meta`, on Nika's loop the provider's
/// tool-call id).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolCall {
    /// The tool's name, as called.
    pub name: String,
    /// The arguments, as sent.
    pub arguments: Value,
    /// The caller's own identity of this call, when known.
    pub meta: Option<String>,
}

impl ToolCall {
    /// Construct (INV-019): a call with no caller identity.
    #[must_use]
    pub fn new(name: impl Into<String>, arguments: Value) -> Self {
        Self {
            name: name.into(),
            arguments,
            meta: None,
        }
    }

    /// The caller's own identity of this call.
    #[must_use]
    pub fn with_meta(mut self, meta: impl Into<String>) -> Self {
        self.meta = Some(meta.into());
        self
    }
}

/// What a call answers: the text the intelligence reads, whether it reports a failure (the
/// intelligence sees it and may adapt), and whether the turn ends on it (`ask`: Nika's loop
/// parks its run until the person answers; over MCP the text is returned like any other).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolReply {
    /// The text the intelligence reads.
    pub text: String,
    /// The call failed; the text says why.
    pub is_error: bool,
    /// The turn ends on this reply.
    pub ends_turn: bool,
}

impl ToolReply {
    /// A call that succeeded.
    #[must_use]
    pub fn ok(text: impl Into<String>) -> Self {
        Self::of(text.into(), false, false)
    }

    /// A call that failed, its text saying why.
    #[must_use]
    pub fn error(text: impl Into<String>) -> Self {
        Self::of(text.into(), true, false)
    }

    /// A call that succeeded and ends the turn.
    #[must_use]
    pub fn ends_turn(text: impl Into<String>) -> Self {
        Self::of(text.into(), false, true)
    }

    const fn of(text: String, is_error: bool, ends_turn: bool) -> Self {
        Self {
            text,
            is_error,
            ends_turn,
        }
    }
}

/// The tools one session serves. A loop or a transport lists them and hands each call over,
/// on its own thread: [`SessionTools::call`] may block for as long as the tool runs, and the
/// session decides how its state is reached.
pub trait SessionTools: Send + Sync {
    /// The tools this session serves, in a stable order.
    fn tools(&self) -> Vec<ToolDef>;

    /// Run one call against the session and answer it. A name the session does not serve or
    /// arguments it refuses answer an error reply, never a panic.
    fn call(&self, call: ToolCall) -> ToolReply;
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;

    /// An MCP client mounts a tool under `mcp__<server>__<name>` after replacing every
    /// character outside `[A-Za-z0-9_-]`: a lowercase word with single underscores reaches it
    /// unchanged, so the name an agent asks for is the name the session serves.
    #[test]
    fn the_fixed_names_are_distinct_lowercase_words_an_mcp_client_keeps() {
        let mut seen = std::collections::BTreeSet::new();
        for name in NAMES {
            assert!(seen.insert(name), "`{name}` is listed twice");
            let first = name.chars().next().expect("a name has a first letter");
            assert!(first.is_ascii_lowercase(), "`{name}` starts with a letter");
            let word = |c: char| c.is_ascii_lowercase() || c == '_';
            assert!(name.chars().all(word), "`{name}` is a lowercase word");
            assert!(!name.contains("__") && !name.ends_with('_'), "`{name}`");
        }
        assert_eq!(seen.len(), 17);
        assert_eq!(CONTRACT, "nika/author-tools@0");
    }

    #[test]
    fn each_reply_says_whether_it_failed_and_whether_the_turn_ends() {
        let cases = [
            (ToolReply::ok("done"), false, false),
            (ToolReply::error("no such file"), true, false),
            (ToolReply::ends_turn("asked"), false, true),
        ];
        for (reply, is_error, ends_turn) in cases {
            assert_eq!((reply.is_error, reply.ends_turn), (is_error, ends_turn));
        }
        assert_eq!(ToolReply::ends_turn("asked").text, "asked");
    }

    /// A session tree journals calls and replies: the wire names are the field names, and a
    /// call without a caller identity says so.
    #[test]
    fn calls_and_replies_round_trip_through_their_wire_names() {
        let call = ToolCall::new("read", json!({"path": "a.nika"})).with_meta("toolu_1");
        let wire = serde_json::to_value(&call).expect("serializes");
        let expected = json!({"name": "read", "arguments": {"path": "a.nika"}, "meta": "toolu_1"});
        assert_eq!(wire, expected);
        let back: ToolCall = serde_json::from_value(wire).expect("deserializes");
        assert_eq!(back, call);
        let anonymous = serde_json::to_value(ToolCall::new("check", json!({})));
        assert_eq!(anonymous.expect("serializes")["meta"], Value::Null);

        let reply = serde_json::to_value(ToolReply::ends_turn("asked")).expect("serializes");
        assert_eq!(
            reply,
            json!({"text": "asked", "is_error": false, "ends_turn": true})
        );
        let def = ToolDef::new(
            "check",
            "Check the candidate.",
            json!({"type": "object"}),
            true,
        );
        let wire = serde_json::to_value(&def).expect("serializes");
        let back: ToolDef = serde_json::from_value(wire.clone()).expect("deserializes");
        assert_eq!((back, wire["read_only"].clone()), (def, json!(true)));
    }

    /// The trait is object safe and shareable: a loop holds one session's tools behind an `Arc`
    /// and calls them from another thread.
    #[test]
    fn a_session_serves_its_tools_behind_a_shared_handle() {
        struct Echo;
        impl SessionTools for Echo {
            fn tools(&self) -> Vec<ToolDef> {
                vec![ToolDef::new("read", "Read a file.", json!({}), true)]
            }
            fn call(&self, call: ToolCall) -> ToolReply {
                match call.name.as_str() {
                    "read" => ToolReply::ok(call.arguments.to_string()),
                    other => ToolReply::error(format!("no tool `{other}`")),
                }
            }
        }
        let tools: std::sync::Arc<dyn SessionTools> = std::sync::Arc::new(Echo);
        let shared = std::sync::Arc::clone(&tools);
        let reply = std::thread::scope(|scope| {
            let worker = scope.spawn(move || shared.call(ToolCall::new("read", json!(1))));
            worker.join().expect("the worker answers")
        });
        assert_eq!(reply, ToolReply::ok("1"));
        assert!(tools.call(ToolCall::new("run", json!({}))).is_error);
        assert_eq!(tools.tools()[0].name, "read");
    }
}
