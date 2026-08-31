// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unknown-builtin + unknown-arg detection — `nika:raed` and `nika:jq`'s
//! `data:` typo caught statically, each with the deterministic « did you
//! mean ___? » (rustc's diagnostic model).
//!
//! The `nika:` namespace is CLOSED (26 canonical builtins · stdlib v0.1 ·
//! the same `nika_catalog::all_builtins()` the codegen enum reads — one
//! source, no drift), and so is each builtin's `args:` key set (the
//! `Builtin::args` vocabulary in the same catalog). A typo'd builtin OR a
//! typo'd arg key parses fine and only bites at runtime — a misspelled arg
//! is the worst case: the runtime silently ignores it (`nika:jq` with
//! `data:` instead of `input:` runs jq over `null` and returns `null`,
//! never an error). This check moves both failures to `nika check`, with
//! the fix attached. The `mcp:` *arg* vocabulary stays OPEN (server-defined);
//! the *server name* does not — a concrete `mcp:<server>/<tool>` call
//! fail-closes unless `.nika/mcp_servers.json` lists that server
//! ([`scan_mcp_against_registry`], the composed lane). Glob grants in an
//! agent whitelist are grants, not calls — skipped.

use std::collections::BTreeSet;

use nika_catalog::{all_builtins, find_builtin};

use nika_schema::raw::{RawAction, RawWorkflow};

use nika_types::suggest::did_you_mean;

/// An invoke/agent tool naming a `nika:` builtin that does not exist.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct UnknownTool {
    /// Where the tool is named (task id · `<id> (on_finally)`).
    pub task: String,
    /// The unknown tool id as written (`nika:raed`).
    pub tool: String,
    /// The nearest canonical builtin, when one is close enough
    /// (`nika:read`) — the machine-applicable fix.
    pub suggestion: Option<String>,
}

/// Scan every invoke tool + agent tool entry (main verbs AND `on_finally`
/// cleanups) for unknown `nika:` builtins.
#[must_use]
pub(super) fn scan_unknown_tools(wf: &RawWorkflow) -> Vec<UnknownTool> {
    let mut findings = Vec::new();
    for task in &wf.tasks {
        let id = &task.value.id.value;
        collect(id, &task.value.action, &mut findings);
    }
    findings
}

/// Check one action's tool names.
fn collect(site: &str, action: &RawAction, out: &mut Vec<UnknownTool>) {
    match action {
        RawAction::Invoke(a) => {
            if let Some(tool) = a.tool() {
                check_tool(site, &tool.value, out);
            }
            // a workflow: target is not a tool name — COMP-001 owns it
        }
        RawAction::Agent(a) => {
            for tool in &a.tools {
                // a glob entry is a grant pattern, not a concrete call —
                // `nika:*` is checked at runtime against the real dispatch
                if !tool.value.contains('*') {
                    check_tool(site, &tool.value, out);
                }
            }
        }
        RawAction::Exec(_) | RawAction::Infer(_) => {}
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        _ => unreachable!("unsupported action"),
    }
}

/// Validate one concrete tool id against the closed `nika:` catalog.
fn check_tool(site: &str, tool: &str, out: &mut Vec<UnknownTool>) {
    let Some(name) = tool.strip_prefix("nika:") else {
        return; // mcp:<server>/<tool> is open — runtime discovery
    };
    if all_builtins().iter().any(|b| b.name == name) {
        return;
    }
    let suggestion = did_you_mean(name, all_builtins().iter().map(|b| b.name))
        .map(|nearest| format!("nika:{nearest}"));
    out.push(UnknownTool {
        task: site.to_owned(),
        tool: tool.to_owned(),
        suggestion,
    });
}

/// An `invoke` builtin call that passes an `args:` key the builtin does
/// not declare (the `nika:jq` `data:`-vs-`input:` footgun class).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct UnknownArg {
    /// Where the call is (task id · `<id> (on_finally)`).
    pub task: String,
    /// The builtin id as written (`nika:jq`).
    pub tool: String,
    /// The undeclared arg key as written (`data`).
    pub arg: String,
    /// The nearest declared arg key, when one is close enough (`input`
    /// for `inpit` · edit distance) or unambiguously abbreviated (`expr`
    /// for `expression` · unique-prefix) — the machine-applicable fix.
    /// `None` when no honest guess exists (silence beats a wrong
    /// suggestion) — then [`Self::declared`] is the teaching.
    pub suggestion: Option<String>,
    /// The builtin's FULL declared arg vocabulary (catalog order). The
    /// renderer's fallback teaching when no suggestion is honest: a
    /// wrong-name-entirely miss (`extract` for fetch's `mode` · `left`
    /// for `json_diff`'s `before` — the battery's own author-errors) is
    /// answered by the closed set itself, not a guess. Additive
    /// (`#[non_exhaustive]`).
    pub declared: Vec<String>,
    /// When the defect is a VALUE (not an unknown key) — `channel: slack`
    /// on `nika:notify`. `None` for the undeclared-key class. Additive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_value: Option<String>,
}

/// Scan every `invoke` call (main verbs AND `on_finally` cleanups) for
/// `args:` keys outside the named builtin's declared vocabulary.
///
/// Only KNOWN `nika:` builtins are checked — an unknown builtin is already
/// reported by [`scan_unknown_tools`] (no double finding), `mcp:` tools are
/// the open namespace (server-defined args), and a non-object `args:` is a
/// shape defect the conformance pass owns.
#[must_use]
pub(super) fn scan_unknown_args(wf: &RawWorkflow) -> Vec<UnknownArg> {
    let mut findings = Vec::new();
    for task in &wf.tasks {
        let id = &task.value.id.value;
        collect_args(id, &task.value.action, &mut findings);
        collect_notify_channel(id, &task.value.action, &mut findings);
    }
    findings
}

/// Check one action's `args:` keys (only `invoke` carries an `args:` map).
fn collect_args(site: &str, action: &RawAction, out: &mut Vec<UnknownArg>) {
    let RawAction::Invoke(a) = action else {
        return; // exec/infer/agent have typed fields, not a free args map
    };
    let Some(name) = a.tool().and_then(|t| t.value.strip_prefix("nika:")) else {
        return; // mcp:/workflow: args are not builtin-table keyed
    };
    let Some(builtin) = find_builtin(name) else {
        return; // unknown builtin — scan_unknown_tools owns that finding
    };
    let Some(args) = a.args.as_ref().and_then(|a| a.value.as_object()) else {
        return; // absent or non-object args — nothing to validate here
    };
    for key in args.keys() {
        if builtin.args.contains(&key.as_str()) {
            continue;
        }
        // Suggestion ladder: edit-distance first (typos: `inpit`), then
        // unique-prefix (abbreviations: `expr` → `expression` — distance 6,
        // invisible to the metric, but an unambiguous author intent).
        let suggestion = did_you_mean(key, builtin.args.iter().copied())
            .or_else(|| unique_prefix_match(key, builtin.args))
            .map(str::to_owned);
        out.push(UnknownArg {
            task: site.to_owned(),
            tool: format!("nika:{name}"),
            arg: key.clone(),
            suggestion,
            declared: builtin.args.iter().map(|&s| s.to_owned()).collect(),
            invalid_value: None,
        });
    }
}

/// The one declared key the unknown key unambiguously abbreviates —
/// `Some` only when EXACTLY one candidate starts with it (two matches =
/// ambiguous = silence) and the abbreviation is substantial (≥3 chars:
/// a 1-2 char key prefixes half the vocabulary by accident).
fn unique_prefix_match<'a>(key: &str, declared: &[&'a str]) -> Option<&'a str> {
    if key.chars().count() < 3 {
        return None;
    }
    let mut hits = declared.iter().filter(|d| d.starts_with(key));
    match (hits.next(), hits.next()) {
        (Some(&only), None) => Some(only),
        _ => None,
    }
}

/// An `invoke` builtin call MISSING an unconditionally-required `args:`
/// key (the `Builtin::required` set · the JSON-Schema `required` the
/// model-facing `ToolDef` declares). Before this, only 5 builtins had a
/// static required-arg check (the hand-coded `builtin_shape` ladder · the
/// rest passed `check {}` and failed at run time).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct MissingArg {
    /// Where the call is (task id · `<id> (on_finally)`).
    pub task: String,
    /// The builtin id as written (`nika:hash`).
    pub tool: String,
    /// The required arg key that is absent (`content`).
    pub arg: String,
}

/// Scan every `invoke` call (main verbs AND `on_finally` cleanups) for a
/// missing unconditionally-required `args:` key.
///
/// Only KNOWN `nika:` builtins are checked (an unknown builtin is
/// [`scan_unknown_tools`]' finding). `mcp:` tools are the open namespace.
/// Builtins with CONDITIONAL requirements (`nika:wait` `duration` XOR
/// `until` · `nika:fetch` mode-dependent + `url`) carry an empty
/// `Builtin::required` — `analyzer::builtin_shape` owns those, so the two
/// checks never double-report. A non-object `args:` (or absent) is treated
/// as « no keys present », so a required-arg builtin called with no args
/// flags each required key.
#[must_use]
pub(super) fn scan_missing_args(wf: &RawWorkflow) -> Vec<MissingArg> {
    let mut findings = Vec::new();
    for task in &wf.tasks {
        let id = &task.value.id.value;
        collect_missing(id, &task.value.action, &mut findings);
    }
    findings
}

/// Check one action's `args:` for the builtin's required keys.
fn collect_missing(site: &str, action: &RawAction, out: &mut Vec<MissingArg>) {
    let RawAction::Invoke(a) = action else {
        return; // only invoke carries a builtin args map
    };
    let Some(name) = a.tool().and_then(|t| t.value.strip_prefix("nika:")) else {
        return; // mcp:/workflow: args are not builtin-table keyed
    };
    let Some(builtin) = find_builtin(name) else {
        return; // unknown builtin — scan_unknown_tools owns that finding
    };
    if builtin.required.is_empty() {
        return; // no flat-required keys (conditional contracts live elsewhere)
    }
    // Absent OR non-object args: → no keys present (each required is missing).
    let args = a.args.as_ref().and_then(|a| a.value.as_object());
    for &req in builtin.required {
        let present = args.is_some_and(|map| map.contains_key(req));
        if !present {
            out.push(MissingArg {
                task: site.to_owned(),
                tool: format!("nika:{name}"),
                arg: req.to_owned(),
            });
        }
    }
}

/// C05 — a literal `nika:notify` `channel:` that is not `webhook` will
/// throw `NIKA-BUILTIN-NOTIFY-001` at run (v0.1 webhook only). Check
/// fail-closes on the same literal so the rung is not theatre.
fn collect_notify_channel(site: &str, action: &RawAction, out: &mut Vec<UnknownArg>) {
    let RawAction::Invoke(a) = action else {
        return;
    };
    if a.tool().is_none_or(|t| t.value != "nika:notify") {
        return;
    }
    let args = a.args.as_ref().map(|spanned| &spanned.value);
    let Some(channel) = literal_notify_channel(args) else {
        return; // missing = webhook default · templated = run's to judge
    };
    if channel == "webhook" {
        return;
    }
    out.push(UnknownArg {
        task: site.to_owned(),
        tool: "nika:notify".to_owned(),
        arg: "channel".to_owned(),
        suggestion: None,
        declared: vec![
            "webhook".to_owned(),
            "v0.1 engines MUST support webhook only".to_owned(),
        ],
        invalid_value: Some(channel.to_owned()),
    });
}

/// The literal channel string, when one is statically visible.
fn literal_notify_channel(args: Option<&serde_json::Value>) -> Option<&str> {
    match args.and_then(|v| v.get("channel")) {
        None => Some("webhook"),
        Some(serde_json::Value::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

/// Project-relative registry path — the same file `nika-mcp` loads at
/// run (`SERVERS_PATH`). Duplicated here so L0 check never depends up.
const MCP_SERVERS_PATH: &str = ".nika/mcp_servers.json";

/// C04 — a concrete `mcp:<server>/<tool>` call whose server is not in
/// `.nika/mcp_servers.json` will throw `NIKA-INVOKE-001` at run. The
/// composed lane injects the reader (this crate stays zero-I/O); a
/// missing or malformed registry is the empty set (fail-closed). No
/// `--allow-unchecked-mcp` door.
pub(super) fn scan_mcp_against_registry(
    wf: &RawWorkflow,
    read: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Vec<UnknownTool> {
    let calls = mcp_call_sites(wf);
    if calls.is_empty() {
        return Vec::new();
    }
    let configured = match read(MCP_SERVERS_PATH) {
        Ok(text) => parse_configured_servers(&text),
        Err(_) => BTreeSet::new(),
    };
    calls
        .into_iter()
        .filter(|(_, tool)| mcp_server_of(tool).is_none_or(|server| !configured.contains(server)))
        .map(|(task, tool)| UnknownTool {
            task,
            tool,
            suggestion: None,
        })
        .collect()
}

fn mcp_call_sites(wf: &RawWorkflow) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        collect_mcp(task.value.id.value.as_str(), &task.value.action, &mut out);
    }
    out
}

fn collect_mcp(site: &str, action: &RawAction, out: &mut Vec<(String, String)>) {
    match action {
        RawAction::Invoke(a) => {
            if let Some(tool) = a.tool() {
                push_mcp_call(site, &tool.value, out);
            }
        }
        RawAction::Agent(a) => {
            for tool in &a.tools {
                push_mcp_call(site, &tool.value, out);
            }
        }
        RawAction::Exec(_) | RawAction::Infer(_) => {}
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        _ => unreachable!("unsupported action"),
    }
}

fn push_mcp_call(site: &str, tool: &str, out: &mut Vec<(String, String)>) {
    if tool.starts_with("mcp:") && !tool.contains('*') {
        out.push((site.to_owned(), tool.to_owned()));
    }
}

fn mcp_server_of(tool: &str) -> Option<&str> {
    let rest = tool.strip_prefix("mcp:")?;
    let (server, name) = rest.split_once('/')?;
    (!server.is_empty() && !name.is_empty()).then_some(server)
}

fn parse_configured_servers(text: &str) -> BTreeSet<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return BTreeSet::new();
    };
    if v.get("mcp_servers_format")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        return BTreeSet::new();
    }
    v.get("servers")
        .and_then(serde_json::Value::as_object)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn findings_of(yaml: &str) -> Vec<UnknownTool> {
        let parsed = parse(yaml, FileId::new(0), ParseMode::Strict);
        assert!(parsed.is_ok(), "fixture must parse");
        let Some(wf) = parsed.ok() else {
            return Vec::new();
        };
        scan_unknown_tools(&wf)
    }

    #[test]
    fn typo_d_builtin_is_caught_with_the_fix() {
        let f = findings_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].tool, "nika:raed");
        assert_eq!(f[0].suggestion.as_deref(), Some("nika:read"));
    }

    #[test]
    fn canonical_builtins_are_clean() {
        let f = findings_of(
            "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n  b:\n    invoke: { tool: \"nika:json_merge_patch\", args: { target: {}, patch: {} } }\n",
        );
        assert!(f.is_empty());
    }

    #[test]
    fn mcp_tools_are_open_namespace() {
        let f = findings_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"mcp:browser/navigate\", args: { url: \"x\" } }\n",
        );
        assert!(f.is_empty(), "server-defined tools are runtime-discovered");
    }

    #[test]
    fn agent_glob_grants_are_skipped_concrete_entries_checked() {
        let f = findings_of(
            "nika: w\ntasks:\n  t:\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:*\", \"nika:fetc\", \"mcp:browser/*\"]\n",
        );
        assert_eq!(f.len(), 1, "only the concrete typo must flag");
        assert_eq!(f[0].suggestion.as_deref(), Some("nika:fetch"));
    }

    #[test]
    fn the_two_loop_only_builtins_are_known_not_flagged() {
        // `nika:done` and `nika:compose` are loop-only `nika:` builtins
        // (ADR-096) — granting them in an agent whitelist must NOT raise an
        // unknown-tool finding (the contract that keeps `nika:compose` in
        // the closed catalog rather than a separate `agent:` namespace).
        let f = findings_of(
            "nika: w\ntasks:\n  t:\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:done\", \"nika:compose\"]\n",
        );
        assert!(f.is_empty(), "loop-only builtins must be catalogued");
    }

    #[test]
    fn far_typo_gets_no_wrong_guess() {
        let f = findings_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:zzzzzzz\", args: {} }\n",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].suggestion, None, "silence beats a wrong suggestion");
    }

    // ── Finding #6 · unknown builtin arg keys ───────────────────────────

    fn arg_findings_of(yaml: &str) -> Vec<UnknownArg> {
        let parsed = parse(yaml, FileId::new(0), ParseMode::Strict);
        assert!(parsed.is_ok(), "fixture must parse");
        let Some(wf) = parsed.ok() else {
            return Vec::new();
        };
        scan_unknown_args(&wf)
    }

    #[test]
    fn jq_data_typo_is_caught_the_silent_null_footgun() {
        // The anchor case: `data:` instead of `input:` — the runtime would
        // ignore it and emit `null`. Caught here. (`data`→`input` is too
        // far for a suggestion, but the unknown-arg finding is the value.)
        let f = arg_findings_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", data: { a: 1 } } }\n",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].tool, "nika:jq");
        assert_eq!(f[0].arg, "data");
        assert_eq!(f[0].task, "t");
    }

    #[test]
    fn near_arg_typo_gets_the_did_you_mean_fix() {
        // `inpit` is one transposition from `input` → suggestion attached.
        let f = arg_findings_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", inpit: 1 } }\n",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].arg, "inpit");
        assert_eq!(f[0].suggestion.as_deref(), Some("input"));
    }

    #[test]
    fn abbreviated_arg_gets_the_unique_prefix_fix() {
        // The battery's own author-error: `expr` for `nika:jq` — edit
        // distance 6 (invisible to the metric) but an unambiguous
        // abbreviation of exactly one declared key.
        let f = arg_findings_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:jq\", args: { expr: \".\", input: 1 } }\n",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].arg, "expr");
        assert_eq!(f[0].suggestion.as_deref(), Some("expression"));
    }

    #[test]
    fn wrong_name_entirely_carries_the_declared_vocabulary() {
        // The other battery author-error class: `left`/`right` for
        // `nika:json_diff` (`before`/`after`) — no lexical similarity, so
        // no guess; the finding carries the closed declared set instead.
        let f = arg_findings_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:json_diff\", args: { left: { a: 1 }, right: { a: 2 } } }\n",
        );
        assert_eq!(f.len(), 2);
        for u in &f {
            assert_eq!(u.suggestion, None, "no honest guess for {}", u.arg);
            assert!(
                u.declared.iter().any(|d| d == "before") && u.declared.iter().any(|d| d == "after"),
                "declared vocabulary must teach the real keys"
            );
        }
    }

    #[test]
    fn prefix_rule_is_unique_and_substantial_only() {
        // Two-candidate ambiguity → silence (never a coin-flip guess).
        assert_eq!(unique_prefix_match("pa", &["path", "pattern"]), None);
        assert_eq!(unique_prefix_match("pat", &["path", "pattern"]), None);
        // Substantial + unique → the fix.
        assert_eq!(
            unique_prefix_match("expr", &["expression", "input"]),
            Some("expression")
        );
        // Too short to assert intent, even when unique.
        assert_eq!(unique_prefix_match("ex", &["expression", "input"]), None);
    }

    #[test]
    fn declared_args_are_clean() {
        let f = arg_findings_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: { a: 1 } } }\n",
        );
        assert!(f.is_empty(), "every key must be declared");
    }

    #[test]
    fn each_undeclared_key_is_its_own_finding() {
        let f = arg_findings_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:read\", args: { path: \"./x\", mode: \"r\", extra: 1 } }\n",
        );
        let mut keys: Vec<&str> = f.iter().map(|u| u.arg.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["extra", "mode"],
            "path is declared, the other two are not"
        );
    }

    #[test]
    fn mcp_args_are_the_open_namespace() {
        let f = arg_findings_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"mcp:browser/navigate\", args: { whatever: 1 } }\n",
        );
        assert!(f.is_empty(), "server-defined args are not validated");
    }

    #[test]
    fn unknown_builtin_args_are_not_double_reported() {
        // `nika:raed` is reported by scan_unknown_tools; its args must NOT
        // also flag here (we can't know a typo'd builtin's vocabulary).
        let f = arg_findings_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:raed\", args: { wat: 1 } }\n",
        );
        assert!(f.is_empty(), "unknown builtin must own its finding");
    }

    // ── F5 · missing required args (extends the 5/22 static check) ───────

    fn missing_of(yaml: &str) -> Vec<MissingArg> {
        let parsed = parse(yaml, FileId::new(0), ParseMode::Strict);
        assert!(parsed.is_ok(), "fixture must parse");
        let Some(wf) = parsed.ok() else {
            return Vec::new();
        };
        scan_missing_args(&wf)
    }

    #[test]
    fn every_builtin_with_a_missing_required_arg_is_caught_at_check_time() {
        // One row per builtin that carries a flat `required` set: an empty
        // (or absent) args: must flag EVERY required key — the « pass
        // check {} then fail at run » gap, closed for all 22.
        use nika_catalog::all_builtins;
        for b in all_builtins() {
            if b.required.is_empty() {
                continue; // conditional contract · builtin_shape owns it
            }
            let yaml = format!(
                "nika: w\ntasks:\n  t:\n    invoke: {{ tool: \"nika:{}\" }}\n",
                b.name
            );
            let f = missing_of(&yaml);
            let mut got: Vec<&str> = f.iter().map(|m| m.arg.as_str()).collect();
            got.sort_unstable();
            let mut want: Vec<&str> = b.required.to_vec();
            want.sort_unstable();
            assert_eq!(got, want, "`nika:{}` missing-required set", b.name);
            assert!(
                f.iter()
                    .all(|m| m.tool == format!("nika:{}", b.name) && m.task == "t"),
                "finding metadata for nika:{}",
                b.name
            );
        }
    }

    #[test]
    fn a_partially_filled_call_flags_only_the_absent_required_keys() {
        // `nika:write` requires path + content · giving only `path` flags
        // exactly `content` (not path), and nothing else.
        let f = missing_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:write\", args: { path: \"./o\" } }\n",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].arg, "content");
        assert_eq!(f[0].tool, "nika:write");
    }

    #[test]
    fn all_required_present_is_clean() {
        let f = missing_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:convert\", args: { input: \"x\", from: \"csv\", to: \"json\" } }\n",
        );
        assert!(f.is_empty(), "every required arg must be present");
    }

    #[test]
    fn optional_only_builtins_never_flag() {
        // `nika:uuid` (version optional) and `nika:done` (no required) never
        // raise a missing-required finding even with empty args.
        for name in ["uuid", "done"] {
            let yaml = format!("nika: w\ntasks:\n  t:\n    invoke: {{ tool: \"nika:{name}\" }}\n");
            assert!(missing_of(&yaml).is_empty(), "nika:{name} has no required");
        }
    }

    #[test]
    fn conditional_builtins_are_left_to_builtin_shape() {
        // `nika:wait` (duration XOR until) and `nika:fetch` (url + mode
        // pairings) carry NO flat required — the missing-required scan must
        // stay silent on them (builtin_shape owns the double-report-free
        // contract).
        let wait = missing_of("nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:wait\" }\n");
        assert!(wait.is_empty(), "wait is conditional, not flat-required");
        let fetch = missing_of("nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\" }\n");
        assert!(fetch.is_empty(), "fetch contract is builtin_shape's");
    }

    #[test]
    fn unknown_builtin_missing_args_not_double_reported() {
        // A typo'd builtin is scan_unknown_tools' finding — the
        // missing-required scan can't know its vocabulary, stays silent.
        let f = missing_of("nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:hsah\" }\n");
        assert!(f.is_empty(), "unknown builtin must own its finding");
    }
}
