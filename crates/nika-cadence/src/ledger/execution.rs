// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::Value;

use super::json_str;

/// Durable association between one ARM attempt and the shared execution
/// service. The trace identity must be the execution UUID's exact bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExecutionLink {
    execution_id: String,
    trace_id: String,
}

impl ExecutionLink {
    /// Construct a canonical `exe-<uuid>` / 32-lower-hex identity pair.
    #[must_use]
    pub fn new(execution_id: impl Into<String>, trace_id: impl Into<String>) -> Option<Self> {
        let execution_id = execution_id.into();
        let trace_id = trace_id.into();
        pair_is_direct(&execution_id, &trace_id).then_some(Self {
            execution_id,
            trace_id,
        })
    }

    /// Admitted execution identity.
    #[must_use]
    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }

    /// Direct root trace identity.
    #[must_use]
    pub fn trace_id(&self) -> &str {
        &self.trace_id
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
    match (payload.get("execution_id"), payload.get("trace_id")) {
        (None, None) => Some(OptionalLink::Absent),
        (Some(execution_id), Some(trace_id)) => {
            ExecutionLink::new(execution_id.as_str()?, trace_id.as_str()?)
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
        format!(
            ",\"execution_id\":{},\"trace_id\":{}",
            json_str(link.execution_id()),
            json_str(link.trace_id())
        )
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
