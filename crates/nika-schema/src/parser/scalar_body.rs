// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The teaching a scalar verb body is refused with (#1600): the one-line
//! skeleton of the mapping it meant, written so the parser accepts it as
//! taught. Split from `verbs.rs` under its line budget.

/// The longest authored text a refusal reflects back before eliding it.
const REFLECTED_CHARS: usize = 60;

/// A non-mapping `infer:` · `invoke:` · `agent:` body, refused with the one-line skeleton of
/// its mapping (#1600): an authored scalar is reflected into the field it most likely meant.
pub(super) fn scalar_body_teaching(verb: &str, scalar: Option<&str>) -> String {
    let skeleton = match (verb, scalar.map(str::trim)) {
        ("invoke", Some(target)) if names_a_workflow(target) => {
            format!("`invoke: {{ workflow: {target} }}` (a child workflow)")
        }
        ("invoke", target) => {
            let tool = target
                .and_then(tool_reference)
                .unwrap_or_else(|| "nika:<name>".to_owned());
            format!(
                "`invoke: {{ tool: {tool}, args: {{ … }} }}` (a tool), or \
                 `invoke: {{ workflow: ./child.nika }}` (a child workflow)"
            )
        }
        (_, Some(text)) if !text.is_empty() => {
            format!("`{verb}: {{ prompt: {} }}`", reflected(text))
        }
        _ => format!("`{verb}: {{ prompt: \"…\" }}`"),
    };
    format!("`{verb}:` body must be a YAML mapping — write {skeleton}")
}

/// The tool reference a scalar `invoke:` target names, spelled the way `tool:` accepts it: a
/// `nika:` or `mcp:` reference as written, a bare builtin name under its `nika:` prefix
/// (`fetch` → `nika:fetch`). `None` for any other text, which no `tool:` would accept.
fn tool_reference(target: &str) -> Option<String> {
    if target.contains(char::is_whitespace) {
        return None;
    }
    if target
        .strip_prefix("nika:")
        .is_some_and(|name| !name.is_empty())
        || target
            .strip_prefix("mcp:")
            .is_some_and(|name| name.contains('/'))
    {
        return Some(target.to_owned());
    }
    nika_catalog::find_builtin(target).map(|builtin| format!("nika:{}", builtin.name))
}

/// A child workflow's target: a `.nika` path or a pinned `registry:` reference.
fn names_a_workflow(target: &str) -> bool {
    target.starts_with("registry:")
        || std::path::Path::new(target)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("nika"))
}

/// `text` as a double-quoted YAML scalar, elided past [`REFLECTED_CHARS`].
fn reflected(text: &str) -> String {
    let shown: String = if text.chars().count() > REFLECTED_CHARS {
        text.chars()
            .take(REFLECTED_CHARS - 1)
            .chain(std::iter::once('…'))
            .collect()
    } else {
        text.to_owned()
    };
    serde_json::to_string(&shown).unwrap_or_else(|_| "\"…\"".to_owned())
}
