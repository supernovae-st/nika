// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The bounds of one remote authoring round: a door's operator's, or narrower ones its caller
//! asked for (a configured operator ceiling is never widened: above it is refused, never
//! clamped), and the spelling of the replay token a door issues. Pure law: the door keeps its
//! words and its request authority.

use std::time::Duration;

use super::input::present;

/// A replay token: 32 random bytes, lowercase hex.
pub const TOKEN_HEX: usize = 64;

/// The bounds of one native round: the operator's, or narrower ones a caller asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Bounds {
    /// Output tokens per call.
    pub max_tokens: u32,
    /// First completion's route capacity, bounded by any explicit token limit.
    pub initial_tokens: u32,
    /// The wait for one call.
    pub call_timeout: Duration,
    /// The whole round: calls, judging, assembly.
    pub deadline: Option<Duration>,
    /// Desired repair rounds, bounded separately by explicit request authority.
    pub repairs: Option<u32>,
    /// Explicit request authority, separate from repair preferences.
    pub max_calls: Option<u32>,
    /// The operator or caller that narrowed the request grant.
    pub grant: &'static str,
}

impl Bounds {
    /// A route's own completion bounds under the operator's `grant`: no round deadline, no
    /// repair preference and no request count until the operator states one.
    #[must_use]
    pub const fn route(
        max_tokens: u32,
        initial_tokens: u32,
        call_timeout: Duration,
        grant: &'static str,
    ) -> Self {
        Self {
            max_tokens,
            initial_tokens,
            call_timeout,
            deadline: None,
            repairs: None,
            max_calls: None,
            grant,
        }
    }
}

/// The caller's narrowing of the operator's bounds; each value optional.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    #[serde(default, deserialize_with = "present")]
    max_calls: Option<u32>,
    #[serde(default, deserialize_with = "present")]
    repairs: Option<u32>,
    #[serde(default, deserialize_with = "present")]
    max_tokens: Option<u32>,
    #[serde(default, deserialize_with = "present")]
    call_timeout_ms: Option<u64>,
    #[serde(default, deserialize_with = "present")]
    deadline_ms: Option<u64>,
}

/// The operator's bounds narrowed by the caller's: every value asked must be positive (repairs
/// may be zero). A configured operator ceiling cannot be widened; above is refused, never clamped.
#[must_use]
pub fn narrow(asked: &Limits, operator: Bounds) -> Option<Bounds> {
    let mut bounds = operator;
    if let Some(max_calls) = asked.max_calls {
        bounds.max_calls = Some(
            (max_calls > 0
                && operator
                    .max_calls
                    .is_none_or(|ceiling| max_calls <= ceiling))
            .then_some(max_calls)?,
        );
        bounds.grant = "request: limits.max_calls within operator ceiling";
    }
    if let Some(repairs) = asked.repairs {
        bounds.repairs = Some(
            operator
                .repairs
                .is_none_or(|ceiling| repairs <= ceiling)
                .then_some(repairs)?,
        );
    }
    if let Some(tokens) = asked.max_tokens {
        bounds.max_tokens = (1..=operator.max_tokens)
            .contains(&tokens)
            .then_some(tokens)?;
    }
    let duration = |millis: u64, ceiling: Duration| {
        let asked = Duration::from_millis(millis);
        (millis > 0 && asked <= ceiling).then_some(asked)
    };
    if let Some(millis) = asked.call_timeout_ms {
        bounds.call_timeout = duration(millis, operator.call_timeout)?;
    }
    if let Some(millis) = asked.deadline_ms {
        let asked = Duration::from_millis(millis);
        bounds.deadline = Some(
            (millis > 0
                && operator.deadline.is_none_or(|ceiling| asked <= ceiling)
                && std::time::Instant::now().checked_add(asked).is_some())
            .then_some(asked)?,
        );
    }
    Some(bounds)
}

/// 64 lowercase hexadecimal digits: the only spelling a door issues.
#[must_use]
pub fn is_token(token: &str) -> bool {
    token.len() == TOKEN_HEX
        && token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
