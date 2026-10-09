// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The role a stated literal keeps when Law 1 imposes it (the reader's path literals handed to
//! the path law): a rooted literal the reader states only as a destination is also realized as
//! a route of an endpoint the request states, by a send to exactly that URL ([`routes`]). It
//! reads typed contracts (the reader's `url` bindings, the candidate's sending tasks), never a
//! word list, and grants no path.

use serde_json::Value;

/// The routes the candidate sends to on a URL the request states (`origins`, the reader's `url`
/// bindings): for each sending `nika:fetch` (any method but GET), the part of its URL after one
/// of those origins, its `${{ const.<name> }}` references read from `const:`, up to a query or a
/// fragment. A URL on another origin, or whose origin rides any other expression, sends to no
/// stated route.
pub(super) fn routes(doc: &Value, origins: &[&str]) -> Vec<String> {
    let (effects, _) = super::effect_and_gate_tasks(doc);
    (effects.iter())
        .filter_map(|id| doc.get("tasks")?.get(id)?.get("invoke"))
        .filter(|invoke| invoke["tool"] == "nika:fetch")
        .filter_map(|invoke| invoke.pointer("/args/url")?.as_str())
        .map(|url| expanded(doc, url))
        .filter_map(|url| {
            origins.iter().find_map(|origin| {
                let route = url.strip_prefix(origin.trim_end_matches('/'))?;
                let route = route.split(['?', '#']).next().unwrap_or_default();
                route
                    .starts_with('/')
                    .then(|| route.trim_end_matches('/').to_owned())
            })
        })
        .collect()
}

/// `text` with each `${{ const.<name> }}` it holds replaced by that constant's string value; any
/// other expression stays as written.
fn expanded(doc: &Value, text: &str) -> String {
    let (mut out, mut rest) = (String::new(), text);
    while let Some(open) = rest.find("${{") {
        let Some(close) = rest[open..].find("}}").map(|at| open + at + 2) else {
            break;
        };
        let expression = &rest[open..close];
        let name = (expression.strip_prefix("${{"))
            .and_then(|inner| inner.strip_suffix("}}"))
            .and_then(|inner| inner.trim().strip_prefix("const."));
        let value = name.and_then(|name| doc.get("const")?.get(name.trim())?.as_str());
        out.push_str(&rest[..open]);
        out.push_str(value.unwrap_or(expression));
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::routes;
    use serde_json::json;

    /// Each sending fetch's route after a stated origin, its constants read; a GET, another
    /// origin, an input-borne origin and a non-fetch effect send to no stated route.
    #[test]
    fn a_route_is_what_a_send_reaches_after_a_stated_origin() {
        let doc = json!({"const": {"sink": "http://127.0.0.1:57468"}, "tasks": {
            "whole": {"invoke": {"tool": "nika:fetch", "args": {
                "url": "http://127.0.0.1:57468/a/b/?x=1", "method": "POST"}}},
            "bound": {"invoke": {"tool": "nika:fetch", "args": {
                "url": "${{ const.sink }}/c", "method": "PUT"}}},
            "read": {"invoke": {"tool": "nika:fetch", "args": {
                "url": "http://127.0.0.1:57468/d"}}},
            "elsewhere": {"invoke": {"tool": "nika:fetch", "args": {
                "url": "http://127.0.0.1:9/e", "method": "POST"}}},
            "input": {"invoke": {"tool": "nika:fetch", "args": {
                "url": "${{ inputs.sink }}/f", "method": "POST"}}},
            "write": {"invoke": {"tool": "nika:write", "args": {"path": "/g"}}}
        }});
        let mut found = routes(&doc, &["http://127.0.0.1:57468/"]);
        found.sort();
        assert_eq!(found, ["/a/b", "/c"]);
        assert!(routes(&doc, &[]).is_empty());
    }
}
