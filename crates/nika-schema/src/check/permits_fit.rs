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
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CapabilityEscape {
    /// The offending task.
    pub task: String,
    /// The capability category (`exec`, `tools`).
    pub category: &'static str,
    /// Human detail (the specific tool/program that escaped).
    pub detail: String,
}

/// Scan a workflow for capability escapes. Empty when no `permits:` block
/// is declared (absent = today's behavior, nothing to enforce).
#[must_use]
pub(super) fn scan_escapes(wf: &RawWorkflow) -> Vec<CapabilityEscape> {
    let Some(permits) = wf.permits.as_ref().map(|p| &p.value) else {
        return Vec::new();
    };
    let mut escapes = Vec::new();
    for task in &wf.tasks {
        let id = &task.value.id.value;
        match &task.value.action {
            RawAction::Exec(a) => check_exec(id, &a.command, permits, &mut escapes),
            RawAction::Invoke(a) => {
                if permits.allows_tool(&a.tool.value) {
                    // Tool is granted — but it may still reach a host/path
                    // outside the fs/net boundary. Check the literal effect.
                    // (A tool OUTSIDE permits.tools is already flagged below;
                    // re-flagging its effect would double-count.)
                    check_builtin_effect(id, a, permits, &mut escapes);
                } else {
                    escapes.push(CapabilityEscape {
                        task: id.clone(),
                        category: "tools",
                        detail: format!("invoke tool `{}` is outside permits.tools", a.tool.value),
                    });
                }
            }
            RawAction::Agent(a) => {
                for tool in &a.tools {
                    if !permits.allows_tool(&tool.value) {
                        escapes.push(CapabilityEscape {
                            task: id.clone(),
                            category: "tools",
                            detail: format!("agent tool `{}` is outside permits.tools", tool.value),
                        });
                    }
                }
            }
            RawAction::Infer(_) => {}
        }
    }
    escapes
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
        });
        return;
    }
    let program = match command {
        RawCommand::Argv(_) => command.argv_program(),
        RawCommand::Shell(s) => leading_program(&s.value),
    };
    if let Some(ExecPermit::Programs(_)) = permits.exec.as_ref()
        && let Some(program) = program
        && !permits.allows_program(program)
    {
        out.push(CapabilityEscape {
            task: id.to_owned(),
            category: "exec",
            detail: format!("program `{program}` is outside permits.exec allowlist"),
        });
    }
}

/// Check a builtin invoke's LITERAL fs/net effect against the boundary.
///
/// Only the builtins whose effect is statically knowable from a literal
/// arg are checked · `nika:fetch` (`url` → `net.http` host) ·
/// `nika:read`/`nika:write` (`path` → `fs.read`/`fs.write`). A
/// `${{ }}`-built arg is dynamic → the runtime `NIKA-SEC-004` check.
fn check_builtin_effect(
    id: &str,
    a: &RawInvokeAction,
    permits: &Permits,
    out: &mut Vec<CapabilityEscape>,
) {
    let tool = a.tool.value.as_str();
    match tool {
        "nika:fetch" => {
            if let Some(host) = literal_arg(a, "url").as_deref().and_then(url_host)
                && !host_allowed(permits, &host)
            {
                out.push(CapabilityEscape {
                    task: id.to_owned(),
                    category: "net",
                    detail: format!("`nika:fetch` host `{host}` is outside permits.net.http"),
                });
            }
        }
        "nika:read" | "nika:write" => {
            let writes = tool == "nika:write";
            if let Some(path) = literal_arg(a, "path")
                && !path_allowed(permits, &path, writes)
            {
                let cat = if writes { "fs.write" } else { "fs.read" };
                out.push(CapabilityEscape {
                    task: id.to_owned(),
                    category: "fs",
                    detail: format!("`{tool}` path `{path}` is outside permits.{cat}"),
                });
            }
        }
        _ => {}
    }
}

/// A literal string value of `args.<key>` — `None` when the arg is absent,
/// non-string, or carries a `${{ }}` interpolation (dynamic → runtime).
fn literal_arg(a: &RawInvokeAction, key: &str) -> Option<String> {
    let s = a.args.as_ref()?.value.get(key)?.as_str()?;
    if s.contains("${{") {
        return None; // dynamic value · runtime concern
    }
    Some(s.to_owned())
}

/// The host of a literal URL (`https://api.x.com/p` → `api.x.com`).
/// `None` when there is no parseable host (a relative/garbage value is
/// the engine's problem, not a static-permits one).
fn url_host(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    // strip userinfo + port
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host.split_once(':').map_or(host, |(h, _)| h);
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
