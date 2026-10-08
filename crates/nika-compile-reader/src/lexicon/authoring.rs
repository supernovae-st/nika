// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The opening request to author a workflow is not an effect its tasks must perform.
//! Only the explicit singular object and an optional simple name are read here. A
//! destination or any other residue remains an ordinary clause; no words are deleted
//! from the request. The caller keeps the whole framing unresolved for cognition and
//! re-enters this same reader on a relative body, if one is present.

/// The relative body after an authoring opening, or the empty suffix when it only
/// names the workflow. `None` leaves the clause's existing reading unchanged.
pub(super) fn body(text: &str) -> Option<&str> {
    let (verb, object) = text.split_once(char::is_whitespace)?;
    if !matches!(verb, "create" | "crée" | "créez" | "créer") {
        return None;
    }
    let object = object.trim_start();
    let mut rest = [
        "a new workflow",
        "a workflow",
        "new workflow",
        "un nouveau workflow",
        "un workflow",
        "workflow",
    ]
    .iter()
    .find_map(|prefix| {
        object
            .strip_prefix(*prefix)
            .filter(|tail| tail.is_empty() || tail.starts_with(char::is_whitespace))
    })?
    .trim_start();
    if let Some(named) = ["named ", "called ", "nommé ", "appelé "]
        .iter()
        .find_map(|prefix| rest.strip_prefix(*prefix))
    {
        let end = named.find(char::is_whitespace).unwrap_or(named.len());
        let name = named.get(..end)?;
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return None;
        }
        rest = named.get(end..)?.trim_start();
    }
    if rest.is_empty() {
        return Some(rest);
    }
    ["that ", "which ", "qui ", "to ", "pour "]
        .iter()
        .find_map(|prefix| rest.strip_prefix(*prefix))
        .map(str::trim_start)
        .filter(|body| !body.is_empty())
}
