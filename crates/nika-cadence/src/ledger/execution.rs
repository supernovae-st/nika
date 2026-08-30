// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::Value;

use super::{exact_keys, json_str};

/// Durable association between one ARM attempt and the shared execution
/// service. The trace identity must be the execution UUID's exact bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExecutionLink {
    run: Option<String>,
    execution: String,
    trace: String,
}

impl ExecutionLink {
    /// Construct a canonical `exe-<uuid>` / 32-lower-hex identity pair.
    #[must_use]
    pub fn new(execution_id: impl Into<String>, trace_id: impl Into<String>) -> Option<Self> {
        let execution_id = execution_id.into();
        let trace_id = trace_id.into();
        pair_is_direct(&execution_id, &trace_id).then_some(Self {
            run: None,
            execution: execution_id,
            trace: trace_id,
        })
    }

    /// Bind a canonical normal run id to the direct execution identity pair.
    #[must_use]
    pub fn for_run(
        run_id: impl Into<String>,
        execution_id: impl Into<String>,
        trace_id: impl Into<String>,
    ) -> Option<Self> {
        let run_id = run_id.into();
        let mut link = Self::new(execution_id, trace_id)?;
        canonical_run_id(&run_id).then(|| {
            link.run = Some(run_id);
            link
        })
    }

    /// Shared durable run identity, absent on ledgers written before the
    /// resident coordinator existed.
    #[must_use]
    pub fn run_id(&self) -> Option<&str> {
        self.run.as_deref()
    }

    /// Admitted execution identity.
    #[must_use]
    pub fn execution_id(&self) -> &str {
        &self.execution
    }

    /// Direct root trace identity.
    #[must_use]
    pub fn trace_id(&self) -> &str {
        &self.trace
    }
}

pub(super) fn parse(payload: &Value) -> Option<ExecutionLink> {
    match parse_optional(payload)? {
        OptionalLink::Absent => None,
        OptionalLink::Present(link) => Some(link),
    }
}

/// Parse an optional link while distinguishing absence from malformed or
/// one-sided identity fields.
pub(super) fn parse_optional(payload: &Value) -> Option<OptionalLink> {
    match (
        payload.get("run_id"),
        payload.get("execution_id"),
        payload.get("trace_id"),
    ) {
        (None, None, None) => Some(OptionalLink::Absent),
        (None, Some(execution_id), Some(trace_id)) => {
            ExecutionLink::new(execution_id.as_str()?, trace_id.as_str()?)
                .map(OptionalLink::Present)
        }
        (Some(run_id), Some(execution_id), Some(trace_id)) => {
            ExecutionLink::for_run(run_id.as_str()?, execution_id.as_str()?, trace_id.as_str()?)
                .map(OptionalLink::Present)
        }
        _ => None,
    }
}

pub(super) enum OptionalLink {
    Absent,
    Present(ExecutionLink),
}

impl OptionalLink {
    pub(super) fn into_option(self) -> Option<ExecutionLink> {
        match self {
            Self::Absent => None,
            Self::Present(link) => Some(link),
        }
    }
}

pub(super) fn render_fields(link: Option<&ExecutionLink>) -> String {
    link.map_or_else(String::new, |link| {
        let run_id = link.run_id().map_or_else(String::new, |run_id| {
            format!(",\"run_id\":{}", json_str(run_id))
        });
        format!(
            "{run_id},\"execution_id\":{},\"trace_id\":{}",
            json_str(link.execution_id()),
            json_str(link.trace_id())
        )
    })
}

pub(super) fn claimed_keys_valid(object: &serde_json::Map<String, Value>, payload: &Value) -> bool {
    const KEYS: [&str; 4] = ["attempt", "deadline", "fencing", "gen"];
    const EXECUTION_KEYS: [&str; 6] = [
        "attempt",
        "deadline",
        "fencing",
        "gen",
        "execution_id",
        "trace_id",
    ];
    const RUN_KEYS: [&str; 7] = [
        "attempt",
        "deadline",
        "fencing",
        "gen",
        "run_id",
        "execution_id",
        "trace_id",
    ];
    exact_keys(object, &KEYS)
        || ((exact_keys(object, &EXECUTION_KEYS) || exact_keys(object, &RUN_KEYS))
            && parse(payload).is_some())
}

pub(super) fn terminal_keys_valid(
    kind: &str,
    object: &serde_json::Map<String, Value>,
    payload: &Value,
) -> bool {
    const KEYS: [&str; 7] = ["slot", "reason", "trace", "exit", "slots", "fencing", "gen"];
    const EXECUTION_KEYS: [&str; 9] = [
        "slot",
        "reason",
        "trace",
        "exit",
        "slots",
        "fencing",
        "gen",
        "execution_id",
        "trace_id",
    ];
    const RUN_KEYS: [&str; 10] = [
        "slot",
        "reason",
        "trace",
        "exit",
        "slots",
        "fencing",
        "gen",
        "run_id",
        "execution_id",
        "trace_id",
    ];
    const LEGACY_KEYS: [&str; 8] = [
        "slot", "reason", "trace", "exit", "slots", "fencing", "gen", "legacy",
    ];
    let terminal = matches!(kind, "fired" | "paused" | "failed");
    exact_keys(object, &KEYS)
        || (terminal
            && (exact_keys(object, &EXECUTION_KEYS) || exact_keys(object, &RUN_KEYS))
            && parse(payload).is_some())
        || (terminal
            && payload.get("legacy") == Some(&Value::Bool(true))
            && exact_keys(object, &LEGACY_KEYS))
}

fn canonical_run_id(run_id: &str) -> bool {
    run_id.len() == 36
        && run_id.as_bytes().get(8) == Some(&b'-')
        && run_id.as_bytes().get(13) == Some(&b'-')
        && run_id.as_bytes().get(18) == Some(&b'-')
        && run_id.as_bytes().get(23) == Some(&b'-')
        && run_id.as_bytes().get(14) == Some(&b'4')
        && matches!(run_id.as_bytes().get(19), Some(b'8' | b'9' | b'a' | b'b'))
        && run_id.bytes().enumerate().all(|(index, byte)| {
            matches!(index, 8 | 13 | 18 | 23)
                || byte.is_ascii_digit()
                || (b'a'..=b'f').contains(&byte)
        })
}

fn pair_is_direct(execution_id: &str, trace_id: &str) -> bool {
    let Some(uuid) = execution_id.strip_prefix("exe-") else {
        return false;
    };
    if uuid.len() != 36
        || uuid.as_bytes().get(8) != Some(&b'-')
        || uuid.as_bytes().get(13) != Some(&b'-')
        || uuid.as_bytes().get(18) != Some(&b'-')
        || uuid.as_bytes().get(23) != Some(&b'-')
    {
        return false;
    }
    let compact = uuid
        .bytes()
        .filter(|byte| *byte != b'-')
        .collect::<Vec<_>>();
    compact.len() == 32
        && compact
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        && trace_id.as_bytes() == compact.as_slice()
}
