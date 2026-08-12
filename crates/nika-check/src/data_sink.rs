// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The data-as-code sink (NEP-0006 · LAW-AUTH-0327) — the STATIC twin of
//! the runtime sink refusal. A `nika:fetch` whose RESOLVED URL path names
//! a code-bearing class (the CLOSED three-class list in
//! [`nika_cap::code_bearing_path_class`] · one predicate for the check
//! twin, the run twin, and the reference oracle) is refused
//! (`NIKA-SEC-008`) unless the task declares the honest door
//! (`inert: "<because>"` · the parser enforces the non-empty because).
//!
//! Resolution at check covers the literal URL and every island over
//! `const.*` or an `inputs.*`/`config.*` entry carrying a declared string
//! default (the NEP-0004 resolution rules extended to the trusted const
//! root — the SINK law classifies the artifact; trust is not the question
//! here). Anything else DEFERS to the run twin (`NIKA-SEC-004` on the
//! resolved URL · NEP-0006 law 3) — never a static refusal.

use nika_schema::Spanned;
use nika_schema::expression::{NamespaceRef, expr_refs, scan_templates};
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::types::VarDecl;

/// The wire code of a code-bearing fetch refusal (NEP-0006 law 1).
pub(crate) const SINK_CODE: &str = "NIKA-SEC-008";

/// One data-as-code sink finding — the check-time half of NEP-0006.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct SinkFinding {
    /// The offending task.
    pub task: String,
    /// The resolved URL whose path named a code-bearing class.
    pub url: String,
    /// The class name (teaching · the closed NEP-0006 list).
    pub class: &'static str,
    /// The matched extension.
    pub ext: &'static str,
    /// The witness sentence (class · extension · the sink it hides).
    pub detail: String,
    /// The two repairs (model as exec · or declare the inert door).
    pub fix: String,
}

impl SinkFinding {
    /// The canonical spec code this finding stamps.
    #[must_use]
    pub fn wire_code(&self) -> &'static str {
        SINK_CODE
    }
}

/// Scan a workflow for code-bearing fetches (NEP-0006 law 1) — the door
/// (law 2) silences a task, the unresolvable defers (law 3).
#[must_use]
pub(crate) fn scan_data_sink(wf: &RawWorkflow) -> Vec<SinkFinding> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        let task = &task.value;
        if task.inert.is_some() {
            continue; // the declared door · law 2 (non-empty by the parser)
        }
        let RawAction::Invoke(a) = &task.action else {
            continue;
        };
        let Some(tool) = a.tool() else {
            continue;
        };
        if tool.value.as_str() != "nika:fetch" {
            continue;
        }
        let Some(template) = url_arg(a.args.as_ref()) else {
            continue;
        };
        let Some(resolved) = resolve_static(template, wf) else {
            continue; // dynamic · the run twin owns it (law 3)
        };
        let Some(path) = url_path(&resolved) else {
            continue; // not a parseable URL · the fetch arg checks own it
        };
        let Some((class, ext)) = nika_cap::code_bearing_path_class(&path) else {
            continue; // the unbounded inert world
        };
        out.push(SinkFinding {
            task: task.id.value.clone(),
            url: resolved.clone(),
            class,
            ext,
            detail: format!(
                "fetches a code-bearing artifact ({class} class · `{ext}`) · {resolved} is a \
                 program, not data: the read hides an execution sink (NEP-0006 law 1)"
            ),
            fix: "model the acquisition as the exec it feeds (exec: + a program permit) · or \
                  declare the read inert on the task (inert: \"<because>\")"
                .to_owned(),
        });
    }
    out
}

/// The `url` argument's raw string — literal or template alike (unlike
/// the taint lane's `string_arg`, which wants templates only).
fn url_arg(args: Option<&Spanned<serde_json::Value>>) -> Option<&str> {
    args?.value.get("url")?.as_str()
}

/// Resolve a fetch URL at check: the literal as-is · every island over
/// `const.*` (the author-fixed string) or `inputs.*`/`config.*` with a
/// declared STRING default. `None` = at least one island stays dynamic.
fn resolve_static(template: &str, wf: &RawWorkflow) -> Option<String> {
    let islands = scan_templates(template).ok()?;
    if islands.is_empty() {
        return Some(template.to_owned());
    }
    let mut resolved = String::with_capacity(template.len());
    let mut cursor = 0_usize;
    for island in &islands {
        let refs = expr_refs(&island.expr);
        let [reference] = refs.as_slice() else {
            return None; // a computed island is not statically decidable
        };
        let value = match reference {
            NamespaceRef::Const(name) => const_string(wf, name)?,
            NamespaceRef::Inputs(name) => declared_string_default(&wf.inputs, name)?,
            NamespaceRef::Config(name) => declared_string_default(&wf.config, name)?,
            _ => return None, // secrets · tasks.* · with.* — dynamic here
        };
        resolved.push_str(&template[cursor..island.start]);
        resolved.push_str(value);
        cursor = island.end;
    }
    resolved.push_str(&template[cursor..]);
    Some(resolved)
}

/// A `const.<name>` string value — the bare literal, or the typed
/// constant whose `value:` rides `default` (nika-vocab `VarDecl`).
fn const_string<'a>(wf: &'a RawWorkflow, name: &str) -> Option<&'a str> {
    let (_, decl) = wf.consts.iter().find(|(n, _)| n.value == name)?;
    match decl {
        VarDecl::Untyped(serde_json::Value::String(s))
        | VarDecl::Typed {
            default: Some(serde_json::Value::String(s)),
            ..
        } => Some(s.as_str()),
        _ => None,
    }
}

/// A declared STRING default of an `inputs:`/`config:` entry.
fn declared_string_default<'a>(
    declarations: &'a [(Spanned<String>, VarDecl)],
    name: &str,
) -> Option<&'a str> {
    let (_, decl) = declarations.iter().find(|(n, _)| n.value == name)?;
    match decl {
        VarDecl::Typed {
            default: Some(serde_json::Value::String(s)),
            ..
        } => Some(s.as_str()),
        _ => None,
    }
}

/// The URL's PATH (the classification surface · query and fragment carry
/// no verdict) — the same WHATWG parser every other URL judgment reads.
fn url_path(url: &str) -> Option<String> {
    url::Url::parse(url).ok().map(|u| u.path().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::{FileId, ParseMode, parse};

    fn findings_of(yaml: &str) -> Vec<SinkFinding> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        scan_data_sink(&wf)
    }

    const BASE: &str =
        "nika: w\npermits:\n  net: { http: [\"data.example.com\"] }\n  tools: [\"nika:fetch\"]\n";

    // ── the conformance fixtures 018-022, mirrored one test each ──

    #[test]
    fn fixture_018_serialized_class_is_refused() {
        let y = format!(
            "{BASE}tasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://data.example.com/models/legacy.pkl\" }}\n"
        );
        let f = findings_of(&y);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].wire_code(), "NIKA-SEC-008");
        assert_eq!(f[0].class, "serialized-executable");
        assert_eq!(f[0].ext, ".pkl");
        assert_eq!(f[0].task, "grab");
    }

    #[test]
    fn fixture_019_script_class_is_refused() {
        let y = format!(
            "{BASE}tasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://data.example.com/setup.sh\" }}\n"
        );
        let f = findings_of(&y);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].class, "script/interpreter");
    }

    #[test]
    fn fixture_020_inert_data_passes() {
        let y = format!(
            "{BASE}tasks:\n  rows:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://data.example.com/q3/rows.csv\" }}\n  feed:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://data.example.com/feed.json\" }}\n"
        );
        assert!(findings_of(&y).is_empty());
    }

    #[test]
    fn fixture_021_the_declared_door_silences() {
        let y = format!(
            "{BASE}tasks:\n  archive:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://data.example.com/models/legacy.pkl\" }}\n    inert: \"archived for provenance · never loaded by any child of this workflow\"\n"
        );
        assert!(findings_of(&y).is_empty());
    }

    #[test]
    fn dynamic_url_defers_and_resolvable_ones_classify() {
        // no default → the run twin owns it (law 3)
        let dynamic = format!(
            "{BASE}inputs:\n  artifact: {{ type: string }}\ntasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"${{{{ inputs.artifact }}}}\" }}\n"
        );
        assert!(findings_of(&dynamic).is_empty());
        // a declared string default resolves · classify
        let defaulted = format!(
            "{BASE}inputs:\n  artifact: {{ type: string, default: \"https://data.example.com/a.pkl\" }}\ntasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"${{{{ inputs.artifact }}}}\" }}\n"
        );
        assert_eq!(findings_of(&defaulted).len(), 1);
        // the trusted const root resolves too (the oracle parity)
        let via_const = format!(
            "{BASE}const:\n  artifact: \"https://data.example.com/a.pkl\"\ntasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"${{{{ const.artifact }}}}\" }}\n"
        );
        assert_eq!(findings_of(&via_const).len(), 1);
    }

    #[test]
    fn the_query_carries_no_verdict() {
        let y = format!(
            "{BASE}tasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://data.example.com/render?file=x.py\" }}\n"
        );
        assert!(findings_of(&y).is_empty(), "NEP-0006 law 4 · path only");
    }
}
