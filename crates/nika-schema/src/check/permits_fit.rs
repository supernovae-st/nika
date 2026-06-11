// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Capability-escape detection — does the body FIT the declared `permits:`?
//!
//! Per spec `01-envelope.md` §permits · once `permits:` is present every
//! category is default-deny. This scan flags the **statically-detectable**
//! escapes (`nika check` surface · the runtime `NIKA-SEC-004` catches the
//! dynamic remainder) ·
//!
//! - an `exec:` task under a `false`/omitted permit, or a program outside
//!   the allowlist
//! - an `invoke:`/`agent` tool outside `permits.tools`
//! - a builtin whose **literal** effect escapes the declared `fs`/`net`
//!   boundary — a `nika:fetch` to an unlisted host (`permits.net.http`),
//!   a `nika:read`/`nika:write` to a path outside `permits.fs.{read,write}`.
//!   These are exactly the two escapes spec `01-envelope.md` §permits names
//!   first (`nika:write ./etc/x outside fs.write` · `nika:fetch` to an
//!   unlisted host). A path/host built from a `${{ }}` value is dynamic and
//!   stays the runtime `NIKA-SEC-004` check.

use crate::raw::{RawAction, RawCommand, RawInvokeAction, RawWorkflow};
use crate::types::{ExecPermit, Permits, permits::glob_matches};

/// A statically-detectable effect outside the declared boundary.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct CapabilityEscape {
    /// The offending task.
    pub task: String,
    /// The capability category (`exec`, `tools`).
    pub category: &'static str,
    /// Human detail (the specific tool/program that escaped).
    pub detail: String,
    /// The machine-applicable repair — the exact grant to add to the
    /// `permits:` block (the agent repair loop applies it verbatim).
    pub fix: Option<String>,
}

/// Scan a workflow for capability escapes. Empty when no `permits:` block
/// is declared (absent = today's behavior, nothing to enforce). Walks every
/// task's main action AND its `on_finally:` cleanup actions — a cleanup
/// runs under the same boundary (and ALWAYS runs, so a blind spot there
/// breaks every run, not just the failure path).
#[must_use]
pub(super) fn scan_escapes(wf: &RawWorkflow) -> Vec<CapabilityEscape> {
    let Some(permits) = wf.permits.as_ref().map(|p| &p.value) else {
        return Vec::new();
    };
    let mut escapes = Vec::new();
    for task in &wf.tasks {
        let id = &task.value.id.value;
        check_action(id, &task.value.action, permits, &mut escapes);
        for cleanup in &task.value.on_finally {
            check_action(
                &format!("{id} (on_finally)"),
                &cleanup.value.action,
                permits,
                &mut escapes,
            );
        }
    }
    escapes
}

/// Check one action (a task's main verb OR an `on_finally` cleanup verb)
/// against the boundary.
fn check_action(id: &str, action: &RawAction, permits: &Permits, out: &mut Vec<CapabilityEscape>) {
    match action {
        RawAction::Exec(a) => check_exec(id, &a.command, permits, out),
        RawAction::Invoke(a) => {
            if permits.allows_tool(&a.tool.value) {
                // Tool is granted — but it may still reach a host/path
                // outside the fs/net boundary. Check the literal effect.
                // (A tool OUTSIDE permits.tools is already flagged below;
                // re-flagging its effect would double-count.)
                check_builtin_effect(id, a, permits, out);
            } else {
                escapes_tool(id, "invoke", &a.tool.value, out);
            }
        }
        RawAction::Agent(a) => {
            for tool in &a.tools {
                if !permits.allows_tool(&tool.value) {
                    escapes_tool(id, "agent", &tool.value, out);
                }
            }
        }
        RawAction::Infer(_) => {}
    }
}

/// Record a tool-surface escape (`invoke`/`agent` tool outside the grant).
///
/// When the tool is a `nika:`-prefixed name that is NOT a canonical
/// builtin, the grant fix is withheld — the rename (the `unknown_tools`
/// finding's did-you-mean) owns the repair; recommending a grant for a
/// tool that does not exist would steer the agent loop into a phantom
/// permits entry.
fn escapes_tool(id: &str, surface: &str, tool: &str, out: &mut Vec<CapabilityEscape>) {
    let is_phantom_builtin = tool
        .strip_prefix("nika:")
        .is_some_and(|name| !nika_catalog::all_builtins().iter().any(|b| b.name == name));
    out.push(CapabilityEscape {
        task: id.to_owned(),
        category: "tools",
        detail: format!("{surface} tool `{tool}` is outside permits.tools"),
        fix: (!is_phantom_builtin).then(|| format!("add \"{tool}\" to permits.tools")),
    });
}

/// An `exec:` task under a `permits:` boundary. A `false`/omitted permit
/// denies any exec; a program allowlist applies to the program — `argv[0]`
/// for the array form (unambiguous), the literal leading token for the
/// shell-string form (dynamic/pipeline heads are a runtime concern).
fn check_exec(id: &str, command: &RawCommand, permits: &Permits, out: &mut Vec<CapabilityEscape>) {
    if !permits.allows_exec() {
        out.push(CapabilityEscape {
            task: id.to_owned(),
            category: "exec",
            detail: "exec task under a boundary that forbids shells".to_owned(),
            fix: static_program(command)
                .map(|p| format!("set permits.exec to a program allowlist including \"{p}\"")),
        });
        return;
    }
    let program = static_program(command);
    if let Some(ExecPermit::Programs(_)) = permits.exec.as_ref()
        && let Some(program) = program
        && !permits.allows_program(program)
    {
        out.push(CapabilityEscape {
            task: id.to_owned(),
            category: "exec",
            detail: format!("program `{program}` is outside permits.exec allowlist"),
            fix: Some(format!("add \"{program}\" to permits.exec")),
        });
    }
}

/// The statically-checkable effect signature of a builtin tool — the ONE
/// classification table both the escape checker and capability inference
/// read, so verification and inference cannot drift. Ground truth: spec
/// `stdlib/builtins-v0.1.md` (File builtins · Network builtins).
///
/// `nika:glob` is deliberately ABSENT: its arg is itself a glob `pattern:`,
/// and glob-pattern ⊆ permits-glob inclusion is not soundly decidable
/// statically — the runtime `NIKA-SEC-004` owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BuiltinEffect {
    /// An HTTP egress whose host comes from a literal URL-shaped arg.
    Net {
        /// The arg key carrying the URL (`url` for fetch · `target` for
        /// webhook notify).
        url_arg: &'static str,
    },
    /// A filesystem effect on a literal `path:` arg.
    Fs {
        /// The effect reads the path (read · grep · edit's find phase).
        reads: bool,
        /// The effect writes the path (write · edit's replace phase).
        writes: bool,
        /// The effect descends under the path (`nika:grep` is a recursive
        /// reader) — inference grants `<path>/**`, not just the path.
        recursive: bool,
    },
}

/// Classify a builtin invoke's statically-checkable effect, `None` for
/// pure-compute builtins (log · jq · hash · …) and for MCP tools (their
/// effects are server-side — the `tools:` grant is the boundary).
pub(super) fn builtin_effect(a: &RawInvokeAction) -> Option<BuiltinEffect> {
    match a.tool.value.as_str() {
        "nika:fetch" => Some(BuiltinEffect::Net { url_arg: "url" }),
        // notify is a net egress ONLY on the webhook channel (a literal
        // `target` URL); other channels ride engine-configured transports,
        // not a workflow-declared host. A dynamic channel is unclassifiable
        // statically (runtime concern).
        "nika:notify" if literal_arg(a, "channel").as_deref() == Some("webhook") => {
            Some(BuiltinEffect::Net { url_arg: "target" })
        }
        "nika:read" => Some(BuiltinEffect::Fs {
            reads: true,
            writes: false,
            recursive: false,
        }),
        "nika:grep" => Some(BuiltinEffect::Fs {
            reads: true,
            writes: false,
            recursive: true,
        }),
        "nika:write" => Some(BuiltinEffect::Fs {
            reads: false,
            writes: true,
            recursive: false,
        }),
        // in-place find/replace reads the bytes, then rewrites the path
        "nika:edit" => Some(BuiltinEffect::Fs {
            reads: true,
            writes: true,
            recursive: false,
        }),
        _ => None,
    }
}

/// Check a builtin invoke's LITERAL fs/net effect against the boundary,
/// per the [`builtin_effect`] classification. A `${{ }}`-built arg is
/// dynamic → the runtime `NIKA-SEC-004` check.
fn check_builtin_effect(
    id: &str,
    a: &RawInvokeAction,
    permits: &Permits,
    out: &mut Vec<CapabilityEscape>,
) {
    let tool = a.tool.value.as_str();
    match builtin_effect(a) {
        Some(BuiltinEffect::Net { url_arg }) => {
            if let Some(host) = literal_arg(a, url_arg).as_deref().and_then(url_host)
                && !host_allowed(permits, &host)
            {
                out.push(CapabilityEscape {
                    task: id.to_owned(),
                    category: "net",
                    detail: format!("`{tool}` host `{host}` is outside permits.net.http"),
                    fix: Some(format!("add \"{host}\" to permits.net.http")),
                });
            }
        }
        Some(BuiltinEffect::Fs { reads, writes, .. }) => {
            let Some(path) = literal_arg(a, "path") else {
                return;
            };
            for (active, dir_writes, cat) in [(reads, false, "fs.read"), (writes, true, "fs.write")]
            {
                if active && !path_allowed(permits, &path, dir_writes) {
                    out.push(CapabilityEscape {
                        task: id.to_owned(),
                        category: "fs",
                        detail: format!("`{tool}` path `{path}` is outside permits.{cat}"),
                        fix: Some(format!("add \"{path}\" to permits.{cat}")),
                    });
                }
            }
        }
        None => {}
    }
}

/// A literal string value of `args.<key>` — `None` when the arg is absent,
/// non-string, or carries a `${{ }}` interpolation (dynamic → runtime).
pub(super) fn literal_arg(a: &RawInvokeAction, key: &str) -> Option<String> {
    let s = a.args.as_ref()?.value.get(key)?.as_str()?;
    if s.contains("${{") {
        return None; // dynamic value · runtime concern
    }
    Some(s.to_owned())
}

/// The host of a literal URL (`https://api.x.com/p` → `api.x.com`).
/// `None` when there is no parseable host (a relative/garbage value is
/// the engine's problem, not a static-permits one).
pub(super) fn url_host(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    // strip userinfo + port
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    // IPv6 bracket form `[::1]:8080` — the host is the bracketed literal
    // (a bare first-`:` split would yield `[`); permits are written and
    // matched bracket-free (`::1`), symmetric on both sides of this fn.
    let host = if let Some(rest) = host.strip_prefix('[') {
        rest.split_once(']').map_or(rest, |(h, _)| h)
    } else {
        host.split_once(':').map_or(host, |(h, _)| h)
    };
    (!host.is_empty()).then(|| host.to_owned())
}

/// Whether `host` matches the declared `permits.net.http` allowlist.
/// Default-deny: an omitted `net` block forbids all hosts.
fn host_allowed(permits: &Permits, host: &str) -> bool {
    permits
        .net
        .as_ref()
        .is_some_and(|n| n.http.iter().any(|g| host_glob_matches(g, host)))
}

/// Host glob match — exact, or a LEADING `*.` subdomain wildcard
/// (`*.github.com` matches `api.github.com` AND the bare `github.com`).
/// Distinct from the tool-id trailing-`*` glob.
fn host_glob_matches(glob: &str, host: &str) -> bool {
    if let Some(suffix) = glob.strip_prefix("*.") {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    glob == host
}

/// Whether `path` matches the declared `permits.fs` allowlist for the
/// direction. Default-deny: an omitted `fs` block forbids all paths.
fn path_allowed(permits: &Permits, path: &str, writes: bool) -> bool {
    permits.fs.as_ref().is_some_and(|fs| {
        let globs = if writes { &fs.write } else { &fs.read };
        globs.iter().any(|g| path_glob_matches(g, path))
    })
}

/// Gitignore-style path glob match · supports a trailing `/**` (any
/// descendant) and a single `*` (any tail within a segment). Conservative:
/// when in doubt it does NOT match (default-deny favours flagging).
fn path_glob_matches(glob: &str, path: &str) -> bool {
    if let Some(prefix) = glob.strip_suffix("/**") {
        // `./out/**` matches `./out/x` and `./out/a/b` (and `./out` itself).
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    glob_matches(glob, path)
}

/// The statically-known program of a command, either form. `argv[0]` for
/// the array form WHEN it is a literal (argv is execve-direct — no shell
/// expansion — so only a `${{ }}` island makes it dynamic); the literal
/// leading token for the shell form. `None` when the program is dynamic —
/// a runtime concern, not a static one.
pub(super) fn static_program(command: &RawCommand) -> Option<&str> {
    match command {
        RawCommand::Argv(_) => command.argv_program().filter(|p| !p.contains("${{")),
        RawCommand::Shell(s) => leading_program(&s.value),
    }
}

/// The leading program token of a command string, when it is a literal
/// bare program. `None` when the head is dynamic (`${{ }}`) — a runtime
/// concern, not a static one.
///
/// Leading `NAME=value` environment assignments are NOT the program
/// (`FOO=bar git status` runs `git` with `FOO=bar` in its env) — they are
/// skipped so the allowlist check lands on the real program, not the
/// assignment token.
fn leading_program(command: &str) -> Option<&str> {
    for token in command.split_whitespace() {
        if token.contains('$') || token.contains('{') {
            // a dynamic head/assignment → a runtime concern, give up statically
            return None;
        }
        if is_env_assignment(token) {
            continue; // `NAME=value` prefix · not the program
        }
        return Some(token);
    }
    None
}

/// Whether `token` is a shell `NAME=value` env-assignment prefix
/// (an identifier-shaped name, then `=`). A bare `=foo` or `--flag=x`
/// (no identifier name before `=`) is NOT an assignment.
fn is_env_assignment(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name.bytes().all(|b| b == b'_' || b.is_ascii_alphanumeric())
        && name
            .bytes()
            .next()
            .is_some_and(|b| b == b'_' || b.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn escapes_of(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn no_permits_block_no_escapes() {
        let y = "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { command: \"rm -rf /\" }\n";
        assert!(
            escapes_of(y).is_empty(),
            "absent permits = nothing to enforce"
        );
    }

    #[test]
    fn exec_under_false_permit_escapes() {
        let y = "nika: v1\nworkflow: w\npermits: { exec: false }\ntasks:\n  - id: t\n    exec: { command: \"echo hi\" }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].category, "exec");
    }

    #[test]
    fn exec_outside_program_allowlist_escapes() {
        let y = "nika: v1\nworkflow: w\npermits: { exec: [\"git\", \"cargo\"] }\ntasks:\n  - id: ok\n    exec: { command: \"git status\" }\n  - id: bad\n    exec: { command: \"rm -rf x\" }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "git allowed, rm escapes");
        assert_eq!(e[0].task, "bad");
        assert!(e[0].detail.contains("rm"));
    }

    #[test]
    fn dynamic_argv_head_is_a_runtime_concern_not_a_static_escape() {
        // `["${{ vars.bin }}", "x"]` — the program is template-built. The
        // static check must NOT compare the raw `${{ }}` island against the
        // allowlist (that was a false positive); runtime NIKA-SEC-004 owns it.
        let y = "nika: v1\nworkflow: w\nvars: { bin: \"git\" }\npermits: { exec: [\"git\"] }\ntasks:\n  - id: t\n    exec: { command: [\"${{ vars.bin }}\", \"status\"] }\n";
        assert!(
            escapes_of(y).is_empty(),
            "dynamic argv[0] is not statically checkable"
        );
    }

    #[test]
    fn escape_fixes_are_machine_applicable() {
        // a REAL tool outside the grant → the fix is the grant line;
        // a PHANTOM builtin (typo) → fix withheld (the rename owns it).
        let y = "nika: v1\nworkflow: w\npermits: { tools: [\"nika:read\"], exec: false }\ntasks:\n  - id: real\n    invoke: { tool: \"nika:write\", args: { path: \"x\", content: \"y\" } }\n  - id: typo\n    invoke: { tool: \"nika:wrte\", args: { path: \"x\", content: \"y\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 2);
        assert_eq!(
            e[0].fix.as_deref(),
            Some("add \"nika:write\" to permits.tools")
        );
        assert_eq!(e[1].fix, None, "phantom builtin → rename owns the repair");
    }

    #[test]
    fn on_finally_cleanup_outside_boundary_escapes() {
        // A cleanup verb runs under the same boundary — and ALWAYS runs.
        let y = "nika: v1\nworkflow: w\npermits: { exec: [\"cargo\"] }\ntasks:\n  - id: build\n    exec: { command: [\"cargo\", \"build\"] }\n    on_finally:\n      - invoke: { tool: \"nika:write\", args: { path: \"./log\", content: \"x\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "the cleanup's tool grant is missing");
        assert_eq!(e[0].task, "build (on_finally)");
        assert_eq!(e[0].category, "tools");
    }

    #[test]
    fn edit_requires_both_fs_directions() {
        // in-place find/replace reads the bytes, then rewrites the path —
        // a write-only grant leaves the read side escaping.
        let y = "nika: v1\nworkflow: w\npermits: { tools: [\"nika:edit\"], fs: { write: [\"./README.md\"] }, exec: false }\ntasks:\n  - id: t\n    invoke: { tool: \"nika:edit\", args: { path: \"./README.md\", find: \"a\", replace: \"b\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1);
        assert!(e[0].detail.contains("fs.read"), "detail: {}", e[0].detail);
    }

    #[test]
    fn ipv6_bracket_host_is_extracted_not_mangled() {
        // `https://[::1]:8080/x` — the host is `::1`, not `[` (the first-`:`
        // split bug). Bracket-free in permits, symmetric both sides.
        let ok = "nika: v1\nworkflow: w\npermits: { tools: [\"nika:fetch\"], net: { http: [\"::1\"] }, exec: false }\ntasks:\n  - id: t\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://[::1]:8080/x\" } }\n";
        assert!(escapes_of(ok).is_empty(), "::1 is granted");
        let bad = "nika: v1\nworkflow: w\npermits: { tools: [\"nika:fetch\"], net: { http: [\"api.x.com\"] }, exec: false }\ntasks:\n  - id: t\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://[::1]:8080/x\" } }\n";
        let e = escapes_of(bad);
        assert_eq!(e.len(), 1);
        assert!(e[0].detail.contains("`::1`"), "detail: {}", e[0].detail);
    }

    #[test]
    fn webhook_notify_target_is_checked_as_net() {
        let y = "nika: v1\nworkflow: w\npermits: { tools: [\"nika:notify\"], exec: false }\ntasks:\n  - id: t\n    invoke: { tool: \"nika:notify\", args: { channel: \"webhook\", target: \"https://hooks.x.com/p\", message: \"hi\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "webhook target host needs a net grant");
        assert_eq!(e[0].category, "net");
        // a non-webhook channel rides an engine transport — no host check
        let email = "nika: v1\nworkflow: w\npermits: { tools: [\"nika:notify\"], exec: false }\ntasks:\n  - id: t\n    invoke: { tool: \"nika:notify\", args: { channel: \"email\", target: \"ops@x.com\", message: \"hi\" } }\n";
        assert!(escapes_of(email).is_empty());
    }

    #[test]
    fn invoke_outside_tools_escapes() {
        let y = "nika: v1\nworkflow: w\npermits: { tools: [\"nika:read\"] }\ntasks:\n  - id: t\n    invoke: { tool: \"nika:write\", args: { path: \"x\", content: \"y\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].category, "tools");
        assert!(e[0].detail.contains("nika:write"));
    }

    #[test]
    fn invoke_inside_tools_glob_is_clean() {
        let y = "nika: v1\nworkflow: w\npermits: { tools: [\"mcp:browser/*\"] }\ntasks:\n  - id: t\n    invoke: { tool: \"mcp:browser/navigate\", args: { url: \"x\" } }\n";
        assert!(escapes_of(y).is_empty());
    }

    #[test]
    fn agent_tool_outside_permits_escapes() {
        let y = "nika: v1\nworkflow: w\npermits: { tools: [\"nika:fetch\"] }\ntasks:\n  - id: t\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:fetch\", \"nika:write\"]\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "fetch allowed, write escapes");
        assert!(e[0].detail.contains("nika:write"));
    }

    #[test]
    fn env_assignment_prefix_skips_to_the_real_program() {
        // `FOO=bar git status` runs `git`, not `FOO=bar` — the allowlist
        // check must land on git (allowed), not phantom-flag the assignment.
        let y = r#"nika: v1
workflow: w
permits: { exec: ["git"] }
tasks:
  - id: ok
    exec: { command: "GIT_PAGER=cat git log" }
  - id: bad
    exec: { command: "FOO=1 rm -rf x" }
"#;
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "git allowed past the assignment; rm escapes");
        assert_eq!(e[0].task, "bad");
        assert!(
            e[0].detail.contains("rm"),
            "names rm, not FOO=1: {}",
            e[0].detail
        );
    }

    #[test]
    fn dynamic_program_is_not_statically_flagged() {
        let y = "nika: v1\nworkflow: w\npermits: { exec: [\"git\"] }\nvars: { cmd: \"git\" }\ntasks:\n  - id: t\n    exec: { command: \"${{ vars.cmd }} status\" }\n";
        assert!(escapes_of(y).is_empty(), "dynamic head = runtime check");
    }
}

#[cfg(test)]
mod fs_net_regression {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn escapes(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn fetch_to_unlisted_host_escapes() {
        // The spec's own first named example: a nika:fetch to an unlisted host.
        let y = r#"nika: v1
workflow: w
permits:
  net: { http: ["api.anthropic.com"] }
  tools: ["nika:fetch"]
tasks:
  - id: t
    invoke: { tool: "nika:fetch", args: { url: "https://evil.example.com/exfil" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "evil host escapes net.http");
        assert_eq!(e[0].category, "net");
        assert!(e[0].detail.contains("evil.example.com"));
    }

    #[test]
    fn fetch_to_listed_host_is_clean() {
        let y = r#"nika: v1
workflow: w
permits:
  net: { http: ["*.anthropic.com"] }
  tools: ["nika:fetch"]
tasks:
  - id: t
    invoke: { tool: "nika:fetch", args: { url: "https://api.anthropic.com/v1/x" } }
"#;
        assert!(escapes(y).is_empty(), "glob host match is clean");
    }

    #[test]
    fn write_outside_fs_write_escapes() {
        // The spec's other named example: nika:write ./etc/x outside fs.write.
        let y = r#"nika: v1
workflow: w
permits:
  fs: { write: ["./out/**"] }
  tools: ["nika:write"]
tasks:
  - id: t
    invoke: { tool: "nika:write", args: { path: "/etc/cron.d/x", content: "pwn" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "etc path escapes fs.write");
        assert_eq!(e[0].category, "fs");
        assert!(e[0].detail.contains("/etc/cron.d/x"));
    }

    #[test]
    fn write_inside_fs_write_glob_is_clean() {
        let y = r#"nika: v1
workflow: w
permits:
  fs: { write: ["./out/**"] }
  tools: ["nika:write"]
tasks:
  - id: t
    invoke: { tool: "nika:write", args: { path: "./out/report.md", content: "x" } }
"#;
        assert!(escapes(y).is_empty(), "./out/** matches ./out/report.md");
    }

    #[test]
    fn read_under_write_only_boundary_escapes() {
        // fs declared but only write — a read is default-denied.
        let y = r#"nika: v1
workflow: w
permits:
  fs: { write: ["./out/**"] }
  tools: ["nika:read"]
tasks:
  - id: t
    invoke: { tool: "nika:read", args: { path: "./out/x" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "read is denied when only write is granted");
        assert_eq!(e[0].category, "fs");
    }

    #[test]
    fn dynamic_url_is_a_runtime_concern() {
        let y = r#"nika: v1
workflow: w
vars: { host: "api.anthropic.com" }
permits:
  net: { http: ["api.anthropic.com"] }
  tools: ["nika:fetch"]
tasks:
  - id: t
    invoke: { tool: "nika:fetch", args: { url: "https://${{ vars.host }}/x" } }
"#;
        assert!(escapes(y).is_empty(), "interpolated url = runtime check");
    }
}

#[cfg(test)]
mod argv_program_check {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn escapes(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn argv_program_is_checked_unambiguously() {
        // argv[0] is the program — no shell-split heuristic needed.
        let allowed = r#"nika: v1
workflow: w
permits: { exec: ["git"] }
tasks:
  - id: t
    exec: { command: ["git", "status"] }
"#;
        assert!(escapes(allowed).is_empty(), "git argv allowed");

        let denied = r#"nika: v1
workflow: w
permits: { exec: ["git"] }
tasks:
  - id: t
    exec: { command: ["rm", "-rf", "x"] }
"#;
        let e = escapes(denied);
        assert_eq!(e.len(), 1);
        assert!(
            e[0].detail.contains("rm"),
            "argv[0] rm flagged: {}",
            e[0].detail
        );
    }

    #[test]
    fn argv_with_interpolated_arg_program_still_literal() {
        // The PROGRAM (argv[0]) is literal even when later args interpolate —
        // the whole point of the argv form (injection-safe).
        let y = r#"nika: v1
workflow: w
vars: { x: "y" }
permits: { exec: ["git"] }
tasks:
  - id: t
    exec: { command: ["git", "${{ vars.x }}"] }
"#;
        assert!(escapes(y).is_empty(), "git allowed; the arg is just data");
    }
}
