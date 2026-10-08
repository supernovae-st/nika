// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a workspace card says first when a refusal's words are recognised.
//!
//! A card paints the Session's own words ([`Committed::text`]) unless they are
//! a refusal this presenter recognises: then four parts come first — what
//! stopped the work, what the Session stated did and did not happen, the
//! supported way on, and where the full diagnostic is read. Recognition is the
//! Session's complete sentence around a stable admission code, never a
//! keyword: any other words, or the same words with anything added or missing,
//! are painted as said. The block is never rewritten: its text remains the
//! full diagnostic, and `F2` opens those exact words in a read-only view.
//!
//! A summary keeps the effect scope of the refused authoring request. An
//! older diagnostic establishes no guarantee about earlier model routing.
//! A provider failure says that a failed call may still have reached
//! the model: it keeps its words, and no « nothing was sent » is inferred.
//! Nothing here retries, turns the knowledge off or changes a model: the way
//! on is a step the human takes.

use crate::model::{Committed, Kind};

/// What a workspace card paints of one block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shown {
    /// The Session's words, as said.
    Said,
    /// A recognised refusal: its summary first; the block's text is the detail.
    Refusal(&'static Summary),
    /// The opening banner, its recognised knowledge warning shortened; the
    /// block's text is the detail.
    Banner(String),
}

/// A recognised refusal, in four parts.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Summary {
    /// What stopped the work.
    pub(crate) cause: &'static str,
    /// The supported effect scope, without extending an older diagnostic's claim.
    pub(crate) scope: &'static str,
    /// The supported way on: a step the human takes, never one taken for them.
    pub(crate) next: &'static str,
    /// Where the full diagnostic is read, through a supported view key.
    pub(crate) details: &'static str,
}

/// A knowledge release the session's environment names, refused for want of a
/// trusted identity before the refused authoring request was dispatched. The
/// workspace's Session reads that configuration from the environment only,
/// once, when it opens, and the environment carries no trusted identity: the
/// way on is a restart without it, under this build's own release or with no
/// knowledge. Plain ASCII, so the sentences read the same in every glyph
/// column.
static UNTRUSTED_RELEASE: Summary = Summary {
    cause: "Nika cannot verify the knowledge release named by NIKA_KNOWLEDGE.",
    scope: "This authoring request was not sent; no write. Earlier routing may have reached the model.",
    next: "Next: quit and restart Nika with NIKA_KNOWLEDGE unset (built-in knowledge) or NIKA_KNOWLEDGE=off.",
    details: "Details: F2",
};

/// Old raw words remain readable, but their broader no-model-send claim
/// cannot cover an earlier routing call.
static LEGACY_UNTRUSTED_RELEASE: Summary = Summary {
    scope: "Nothing was written. This older diagnostic does not establish what reached the model.",
    ..UNTRUSTED_RELEASE
};

/// The opening banner's warning for the same release, after the Session's own
/// marker: what any request under a model will meet, and where to read why.
const UNTRUSTED_WARNING: &str = "Knowledge: Nika cannot verify the release named by NIKA_KNOWLEDGE, so authoring with a model will be refused. Details: F2";

/// The Session's refusal of an authoring configuration it cannot honour,
/// before its cause.
const CONFIGURATION: &str = "the authoring configuration cannot be used: ";
/// What the Session adds before the refused authoring call is dispatched:
/// that request's effect scope and the Session's own way on.
const NOT_SENT: &str = " · this workflow-authoring request was not sent, nothing was written · fix or unset the knowledge (NIKA_KNOWLEDGE · NIKA_AUTHORING_STRATEGY) and open the session again";
/// The exact legacy suffix: recognized without inferring a model-send scope.
const LEGACY_NOT_SENT: &str = " · nothing was sent to the authoring model, nothing was written · fix or unset the knowledge (NIKA_KNOWLEDGE · NIKA_AUTHORING_STRATEGY) and open the session again";
/// The strict knowledge door's refusal of a release, up to the release root.
const UNAVAILABLE: &str = "knowledge unavailable: the strict door refused `";
/// Between the root and the admission detail: the stable code of admission's
/// first step, no trusted expected identity for the release.
const UNTRUSTED: &str = "` (ADMISSION_UNTRUSTED: ";
/// The door's own way on, after the admission detail.
const DOOR: &str = ") — name a release this engine admits, or turn the knowledge off (--no-knowledge · NIKA_KNOWLEDGE=off)";
/// The root the door names for the release this build embeds: no setting names
/// it, so unsetting `NIKA_KNOWLEDGE` would not move past that refusal.
const EMBEDDED: &str = "embedded:nika-knowledge-release";
/// The banner's warning marker, kept as the Session wrote it.
const MARKER: &str = "  ⚠ ";
/// The banner's knowledge warning after its marker, up to its cause.
const WARNING: &str = "authoring knowledge: ";

/// What a workspace card paints of `block`.
pub(crate) fn shown(block: &Committed) -> Shown {
    match block.kind {
        Kind::Refusal => untrusted_refusal(&block.text).map_or(Shown::Said, Shown::Refusal),
        Kind::Banner => banner(&block.text).map_or(Shown::Said, Shown::Banner),
        _ => Shown::Said,
    }
}

/// The Session's whole refusal of a turn whose configuration names a release
/// with no trusted identity: its error's words, then exactly what it adds.
fn untrusted_refusal(text: &str) -> Option<&'static Summary> {
    let rest = text.strip_prefix(CONFIGURATION)?;
    let (cause, summary) = if let Some(cause) = rest.strip_suffix(NOT_SENT) {
        (cause, &UNTRUSTED_RELEASE)
    } else {
        (
            rest.strip_suffix(LEGACY_NOT_SENT)?,
            &LEGACY_UNTRUSTED_RELEASE,
        )
    };
    untrusted_cause(cause).then_some(summary)
}

/// The strict door's refusal, on one line, of a release root a setting named,
/// for want of a trusted identity.
fn untrusted_cause(cause: &str) -> bool {
    let named = (cause.strip_prefix(UNAVAILABLE))
        .and_then(|rest| rest.strip_suffix(DOOR))
        .and_then(|rest| rest.split_once(UNTRUSTED));
    named.is_some_and(|(root, detail)| {
        !root.is_empty() && root != EMBEDDED && !detail.is_empty() && !cause.contains('\n')
    })
}

/// The banner with its knowledge warning shortened when that warning is the
/// same refusal; every other line as the Session wrote it.
fn banner(text: &str) -> Option<String> {
    let warned = |line: &str| {
        (line.strip_prefix(MARKER))
            .and_then(|rest| rest.strip_prefix(WARNING))
            .is_some_and(untrusted_cause)
    };
    let at = text.split('\n').position(warned)?;
    let short = format!("{MARKER}{UNTRUSTED_WARNING}");
    let lines: Vec<&str> = (text.split('\n').enumerate())
        .map(|(index, line)| if index == at { short.as_str() } else { line })
        .collect();
    Some(lines.join("\n"))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
#[path = "diagnostics/tests.rs"]
pub(super) mod tests;
