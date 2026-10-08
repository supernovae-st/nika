// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An admitted entry as an author reads it, whatever its kind: its role in the release's
//! ontology and what that role means, its provenance, its whole contract and its whole body. Pure
//! over the admitted row, the source rows it cites and its body file's admitted text; nothing is
//! cut or summarized. A counterexample is presented as a boundary, never as something to reuse;
//! only a block is a component (an author reuses it by instantiating it, never by its text).

use std::fmt::Write as _;

use serde_json::Value;

use super::recall::text_of;
use super::release::canonical::jcs_json;
use super::release::r2;

/// What a role means to an author.
fn meaning(role: &str) -> &'static str {
    match role {
        "component" => "an executable checked block, reused by instantiating it at its holes",
        "case" => "a solved example to learn from, never a component",
        "boundary" => "what fails and why, to be avoided; never a component, never reused",
        "method" => "how to approach a need of this kind",
        "contract" => "what a construct, callable or interface takes, gives and requires",
        "structure" => "a reusable shape: a pattern, a pack or a skeleton",
        "reference" => "a diagnostic and what it teaches",
        "repair" => "how to repair a diagnosed failure",
        "need" => "a family of needs",
        "vocabulary" => "the values of an intent facet",
        "provenance" => "a source the knowledge derives from",
        _ => "knowledge of the release",
    }
}

/// The role of an admitted row's kind, as profile r2 names it (the ontology r1 shares).
#[must_use]
pub fn role(row: &Value) -> Option<&'static str> {
    let kind = row["kind"].as_str()?;
    let profile = r2::profile().ok()?;
    let found = profile.kinds().iter().find(|known| known.name() == kind)?;
    Some(found.role())
}

/// The role line of an admitted row: its role and what it means, when its kind has one.
#[must_use]
pub fn role_line(row: &Value) -> Option<String> {
    role(row).map(|role| format!("role: {role} — {}", meaning(role)))
}

/// The sources an admitted row cites, by id: its provenance in one line.
#[must_use]
pub fn sources_line(row: &Value) -> Option<String> {
    let sources: Vec<&str> = (row["provenance"]["sources"]
        .as_array()
        .into_iter()
        .flatten())
    .filter_map(Value::as_str)
    .collect();
    (!sources.is_empty()).then(|| format!("sources: {}", sources.join(", ")))
}

/// The provenance of an admitted row: each source it cites (its title, licence, ownership and
/// upstream, or that the release holds no such source), the activity, then its trust, status,
/// proof and pin.
#[must_use]
pub fn provenance(row: &Value, sources: &[(&str, Option<&Value>)]) -> String {
    let cited: Vec<String> = sources
        .iter()
        .map(|(id, source)| match source {
            Some(source) => {
                let upstream = text_of(source, &["upstream"]);
                let upstream = if upstream.is_empty() {
                    "none".to_owned()
                } else {
                    upstream
                };
                format!(
                    "{id} ({}; licence {}; {}; upstream {upstream})",
                    text_of(source, &["title"]),
                    text_of(source, &["licence"]),
                    text_of(source, &["ownership"]),
                )
            }
            None => format!("{id} (not in this release)"),
        })
        .collect();
    let mut lines = Vec::new();
    if !cited.is_empty() {
        lines.push(format!("derived from: {}", cited.join(" · ")));
    }
    if let Some(activity) = row["provenance"]["activity"].as_str() {
        lines.push(format!("activity: {activity}"));
    }
    let facts: Vec<String> = [
        ("trust", "/trust"),
        ("status", "/status"),
        ("proof", "/proof_level"),
        ("pin", "/pin/binary"),
        ("spec", "/pin/spec_sha"),
    ]
    .iter()
    .filter_map(|(label, pointer)| Some(format!("{label} {}", row.pointer(pointer)?.as_str()?)))
    .collect();
    if !facts.is_empty() {
        lines.push(facts.join(" · "));
    }
    lines.join("\n")
}

/// A fence longer than any run of backticks in `text`.
fn fence(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

/// An admitted row in full: its role and what it means, its kind, id and title, its provenance,
/// its whole contract (the row as admitted, canonical JSON) and, when its kind keeps its text in
/// a body file, that file's whole text.
#[must_use]
pub fn entry_text(
    row: &Value,
    sources: &[(&str, Option<&Value>)],
    body: Option<(&str, &str)>,
) -> String {
    let kind = text_of(row, &["kind"]);
    let id = text_of(row, &["id"]);
    let title = text_of(row, &["title"]);
    let mut text = format!("{kind} {id} — {title}\n");
    if let Some(role) = role_line(row) {
        text.push_str(&role);
        text.push('\n');
    }
    let provenance = provenance(row, sources);
    if !provenance.is_empty() {
        text.push_str(&provenance);
        text.push('\n');
    }
    let contract = jcs_json(row);
    let fenced = fence(&contract);
    let _ = writeln!(text, "contract:\n{fenced}json\n{contract}\n{fenced}");
    if let Some((file, body)) = body {
        let markdown = std::path::Path::new(file)
            .extension()
            .is_some_and(|extension| extension == "md");
        let language = if markdown { "markdown" } else { "yaml" };
        let fenced = fence(body);
        let _ = writeln!(
            text,
            "{file}:\n{fenced}{language}\n{}\n{fenced}",
            body.trim_end()
        );
    }
    text.trim_end().to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
