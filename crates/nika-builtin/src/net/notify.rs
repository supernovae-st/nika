// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:notify` receiver shapes. A chat webhook shows the message to
//! people and documents the field it reads: a Slack incoming webhook
//! refuses a body without `text` (`no_text`), a Discord webhook one
//! without `content`. Those two receivers get their own body; every
//! other target keeps the stdlib's `{ message, severity, data? }`, the
//! payload receivers branch on.
//!
//! The message stays TEXT in both media (stdlib §notify injection
//! defense): Slack's three control characters become entities and its
//! formatting is off; Discord's markdown, link and mention openers are
//! escaped and no mention is parsed.

use nika_kernel::ai::provider::ProviderHttpError;
use serde_json::{Value, json};

/// Discord refuses a `content` longer than this (Execute Webhook).
const DISCORD_CONTENT_MAX: usize = 2000;

/// Leading bytes of a refused delivery's body read for its answer.
const ANSWER_SCAN: usize = 4096;

/// Target parts at least this long are withheld from an answer.
const ECHO_MIN: usize = 8;

/// The receivers whose documented webhook contract refuses the stdlib body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Receiver {
    /// Any other target: `{ message, severity, data? }`.
    Generic,
    /// A Slack incoming webhook: `{ text, mrkdwn: false }`.
    Slack,
    /// A Discord webhook: `{ content, allowed_mentions }`.
    Discord,
}

impl Receiver {
    /// Recognize the receiver from the target URL: its host and the path
    /// its vendor documents. A Slack Workflow Builder trigger takes custom
    /// variables and stays generic, as do Discord's `/slack` and `/github`
    /// adapters and every other host.
    pub(super) fn of(target: &str) -> Self {
        let Ok(url) = url::Url::parse(target) else {
            return Self::Generic;
        };
        let host = url.host_str().unwrap_or_default().trim_end_matches('.');
        let segments: Vec<&str> = url
            .path_segments()
            .map(|parts| parts.filter(|part| !part.is_empty()).collect())
            .unwrap_or_default();
        match host {
            "hooks.slack.com" | "hooks.slack-gov.com"
                if matches!(segments.as_slice(), ["services", _, ..]) =>
            {
                Self::Slack
            }
            "discord.com"
            | "discordapp.com"
            | "canary.discord.com"
            | "ptb.discord.com"
            | "canary.discordapp.com"
            | "ptb.discordapp.com"
                if discord_execute(&segments) =>
            {
                Self::Discord
            }
            _ => Self::Generic,
        }
    }

    /// The JSON body this receiver accepts.
    pub(super) fn payload(self, message: &str, severity: &str, data: Option<&Value>) -> Value {
        match self {
            Self::Generic => {
                // `data:` carries structured context so receivers branch on
                // machine fields, never parse the human message (stdlib
                // §notify · the key is ABSENT when not given).
                let mut payload = json!({ "message": message, "severity": severity });
                if let (Some(map), Some(data)) = (payload.as_object_mut(), data) {
                    map.insert("data".to_owned(), data.clone());
                }
                payload
            }
            Self::Slack => json!({
                "text": slack_text(&chat_text(message, severity, data)),
                "mrkdwn": false
            }),
            Self::Discord => {
                let text = discord_text(&chat_text(message, severity, data));
                json!({
                    "content": bound_escaped(&text, DISCORD_CONTENT_MAX),
                    "allowed_mentions": { "parse": [] }
                })
            }
        }
    }
}

/// `api/webhooks/{id}/{token}`, optionally versioned (`api/v10/…`).
fn discord_execute(segments: &[&str]) -> bool {
    match segments {
        ["api", "webhooks", _, _] => true,
        ["api", version, "webhooks", _, _] => version
            .strip_prefix('v')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
        _ => false,
    }
}

/// What a person reads: the severity unless it is the default `info`, the
/// message, then the `data:` context on its own line (a string as written,
/// any other value as compact JSON).
fn chat_text(message: &str, severity: &str, data: Option<&Value>) -> String {
    let mut text = if severity.eq_ignore_ascii_case("info") {
        message.to_owned()
    } else {
        format!("[{severity}] {message}")
    };
    match data {
        None | Some(Value::Null) => {}
        Some(Value::String(context)) => {
            text.push('\n');
            text.push_str(context);
        }
        Some(other) => {
            text.push('\n');
            text.push_str(&other.to_string());
        }
    }
    text
}

/// Slack's escaping rule: `&`, `<` and `>` open links and mentions
/// (`<!channel>`, `<@U…>`), so plain text carries them as entities.
fn slack_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Discord renders `content` as markdown and escapes any punctuation
/// behind a backslash. Every character that opens emphasis, code, a
/// spoiler, a masked link or a mention is escaped, as is a line's opening
/// `#`, `-` or `>` (heading, list, quote). A bare URL is copied as is: an
/// escape inside it would change the link. Every `\` of the result opens
/// an escape pair, which [`bound_escaped`] relies on.
fn discord_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + text.len() / 8);
    let mut line_start = true;
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        if let Some(len) = bare_url_len(rest) {
            escaped.push_str(&rest[..len]);
            rest = &rest[len..];
            line_start = false;
            continue;
        }
        let opener = matches!(
            ch,
            '\\' | '*' | '_' | '~' | '`' | '|' | '[' | ']' | '<' | '>'
        ) || (line_start && matches!(ch, '#' | '-'));
        if opener {
            escaped.push('\\');
        }
        escaped.push(ch);
        line_start = ch == '\n' || (line_start && matches!(ch, ' ' | '\t'));
        rest = &rest[ch.len_utf8()..];
    }
    escaped
}

/// Length of the bare `http://` or `https://` URL opening `rest`, up to
/// whitespace, `<` (Discord's autolink stops there) or `\`.
fn bare_url_len(rest: &str) -> Option<usize> {
    let scheme = ["https://", "http://"].into_iter().find(|scheme| {
        rest.get(..scheme.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
    })?;
    let tail = &rest[scheme.len()..];
    let end = tail
        .find(|c: char| c.is_whitespace() || matches!(c, '<' | '\\'))
        .unwrap_or(tail.len());
    (end > 0).then_some(scheme.len() + end)
}

/// `text` within `max` UTF-16 units, ending `…` when cut. UTF-16 units
/// never number fewer than characters, so the bound holds whichever unit
/// the receiver counts. An escape pair (`\` and its character) is never
/// split.
fn bound_escaped(text: &str, max: usize) -> String {
    if text.encode_utf16().count() <= max {
        return text.to_owned();
    }
    let budget = max.saturating_sub(1);
    let mut bounded = String::with_capacity(max);
    let mut used = 0;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        let escaped = if ch == '\\' { chars.next() } else { None };
        let units = ch.len_utf16() + escaped.map_or(0, char::len_utf16);
        if used + units > budget {
            break;
        }
        used += units;
        bounded.push(ch);
        bounded.extend(escaped);
    }
    bounded.push('…');
    bounded
}

/// The receiver's own answer to a refused delivery (Slack answers
/// `no_text`, Discord a JSON `message`): the actionable cause a person or
/// a repair loop reads. It passes the provider seam's reviewed filter —
/// one bounded line, invisible and control characters removed,
/// credential-shaped words withheld — and every part of the target is
/// withheld as well: a webhook URL carries its credential in its path or
/// query, and an error page may echo the request.
pub(super) fn answer(status: u16, body: &[u8], target: &str) -> Option<String> {
    let head = String::from_utf8_lossy(&body[..body.len().min(ANSWER_SCAN)]);
    let parts = target_parts(target);
    let withheld: Vec<&str> = parts.iter().map(String::as_str).collect();
    ProviderHttpError::new(status, None, None, None)
        .with_message(&head, &withheld)
        .message()
        .map(str::to_owned)
}

/// The target, its path and query, its password, and each path segment
/// or query value of [`ECHO_MIN`] characters or more, longest first so a
/// whole echo is withheld before its parts.
fn target_parts(target: &str) -> Vec<String> {
    let mut parts = vec![target.to_owned()];
    if let Ok(url) = url::Url::parse(target) {
        parts.push(url.path().to_owned());
        parts.extend(url.query().map(str::to_owned));
        parts.extend(url.password().map(str::to_owned));
        if let Some(segments) = url.path_segments() {
            parts.extend(segments.map(str::to_owned));
        }
        parts.extend(url.query_pairs().map(|(_, value)| value.into_owned()));
    }
    parts.retain(|part| part.chars().count() >= ECHO_MIN);
    parts.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    parts.dedup();
    parts
}

#[cfg(test)]
mod tests;
