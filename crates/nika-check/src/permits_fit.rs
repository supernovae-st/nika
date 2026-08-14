// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Capability-escape detection — does the body FIT the declared `permits:`?
//!
//! Per spec `01-envelope.md` §permits · once `permits:` is present every
//! category is default-deny. F-O8 « absent = zero authority » (NEP-0003):
//! a MISSING block is judged against the EMPTY boundary — every effect
//! escapes with `undeclared: true` (wire code `NIKA-AUTH-006`), a
//! pure-compute body stays clean. This scan flags the
//! **statically-detectable** escapes (`nika check` surface · the runtime
//! `NIKA-SEC-004` catches the dynamic remainder) ·
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
    /// True when the escape is judged against the ZERO boundary because
    /// no `permits:` block is declared at all (F-O8 « absent = zero
    /// authority » · the wire code is `NIKA-AUTH-006`): the repair is
    /// DECLARING the block — the `fix` idiom still names the exact entry
    /// (`nika check --infer-permits` writes the whole block in one pass).
    /// Mutually exclusive with `floor` (the floor is permits-independent).
    /// Additive (`#[non_exhaustive]`).
    pub undeclared: bool,
}

/// Scan a workflow for capability escapes. F-O8 « absent = zero
/// authority »: a MISSING `permits:` block is judged against the EMPTY
/// boundary (`Permits::new()` · every effect escapes, flagged
/// `undeclared` → `NIKA-AUTH-006`) — absent is never « nothing to
/// enforce » anymore; a pure-compute body escapes nothing and stays
/// clean (the « declare `permits: {}` » hint owns that case). The
/// SSRF-floor parity check runs UNCONDITIONALLY — the
/// floor itself is independent of `permits:` (spec 05-errors
/// `NIKA-SEC-005`), so a literal fetch to a floor-blocked target is a
/// check-time truth with or without a boundary. Walks every task's main
/// action AND its `on_finally:` cleanup actions — a cleanup runs under
/// the same boundary (and ALWAYS runs, so a blind spot there breaks
/// every run, not just the failure path).
#[must_use]
pub fn scan_escapes(wf: &RawWorkflow) -> Vec<CapabilityEscape> {
    let zero = Permits::new();
    let (permits, undeclared) = if let Some(p) = wf.permits.as_ref() {
        (Some(&p.value), false)
    } else {
        // F-O8 · absent = zero authority: judge the body against ∅.
        (Some(&zero), true)
    };
    let consts = ConstStrings::of(wf);
    let mut escapes = Vec::new();
    for task in &wf.tasks {
        let id = &task.value.id.value;
        check_action(
            id,
            &task.value.action,
            permits,
            undeclared,
            &consts,
            &mut escapes,
        );
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
                    undeclared: false,
                });
            }
        }
    }
    if undeclared {
        // F-O8 · every boundary escape above was judged against the ZERO
        // boundary: stamp the class (the wire code maps to NIKA-AUTH-006).
        // Floor escapes keep their own class (NIKA-SEC-005 is
        // permits-independent).
        for e in escapes.iter_mut().filter(|e| !e.floor) {
            e.undeclared = true;
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
/// declared; the boundary half receives the declared block OR the F-O8
/// zero boundary (absent = zero authority · never « nothing to check »).
fn check_action(
    id: &str,
    action: &RawAction,
    permits: Option<&Permits>,
    undeclared: bool,
    consts: &ConstStrings,
    out: &mut Vec<CapabilityEscape>,
) {
    if let RawAction::Invoke(a) = action {
        check_net_floor(id, a, permits, consts, out);
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
            // NEP-0003 law 1 · under an ABSENT block a pure-internal
            // builtin requires nothing (it is the « pure compute » the
            // legal zero admits) — but the exemption belongs to the CALL,
            // not the tool. `nika:decide` is in the class AND carries an
            // fs effect when its `bundle:` is a literal path; asking only
            // the class returned here before the effect was consulted, so
            // `nika:decide { bundle: "/etc/passwd" }` passed clean under
            // zero authority while `nika:read` on the same path was
            // refused. The SSOT already said which was right (« a bundle:
            // path reads like any declared fs.read »).
            let args = a.args.as_ref().map(|s| &s.value);
            if undeclared && nika_cap::is_pure_internal_call(&tool.value, args) {
                return;
            }
            if permits.allows_tool(&tool.value) {
                // Tool is granted — but it may still reach a host/path
                // outside the fs/net boundary. Check the literal effect.
                // (A tool OUTSIDE permits.tools is already flagged below;
                // re-flagging its effect would double-count.)
                check_builtin_effect(id, a, permits, consts, out);
            } else if undeclared {
                // NEP-0003 laws 1+3 · under the absent block only a
                // STATICALLY visible resource is a check-time refusal; a
                // computed one defers to the runtime (NIKA-SEC-004).
                absent_effect_escape(id, a, &tool.value, consts, out);
            } else {
                escapes_tool(id, "invoke", &tool.value, out);
            }
        }
        RawAction::Agent(a) => {
            for tool in &a.tools {
                // NEP-0003 law 1 · under an ABSENT block a pure-internal
                // tool requires nothing (the invoke twin · check≡run).
                if undeclared && nika_cap::is_pure_internal(&tool.value) {
                    continue;
                }
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
    consts: &ConstStrings,
    out: &mut Vec<CapabilityEscape>,
) {
    let Some(BuiltinEffect::Net { url_arg }) = builtin_effect(a) else {
        return;
    };
    let Some(tool_ref) = a.tool() else {
        return; // builtin_effect is None for workflow: — unreachable belt
    };
    if let Some(host) = judgeable_arg(consts, a, url_arg)
        .as_deref()
        .and_then(url_host)
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
            undeclared: false,
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
        undeclared: false,
    });
}

/// NEP-0003 law 1 (with law 3) · the escape of a non-pure tool under an
/// ABSENT block. Only a statically visible resource counts at check time:
/// a literal URL is a `net` escape, a literal path an `fs` one, and every
/// other non-pure tool (chart · image/tts · mcp:* · unknown) is a `tools`
/// escape. A computed url/path is the runtime boundary's (NIKA-SEC-004).
fn absent_effect_escape(
    id: &str,
    a: &RawInvokeAction,
    tool: &str,
    consts: &ConstStrings,
    out: &mut Vec<CapabilityEscape>,
) {
    // THE RESOURCE defers; THE TOOL AUTHORITY does not.
    //
    // Both arms below used to `return` on an unjudgeable argument, which
    // dropped the whole finding — including a conjunct that never depended on
    // the argument. Under an ABSENT `permits:` block F-O8 makes every category
    // empty, so `permits.tools` grants nothing and the invoke cannot run
    // WHATEVER the url or path turns out to be. That is a certainty, not a
    // guess, and it was being waived along with the part that genuinely is one.
    //
    // Measured 2026-07-29, one cell of a four-cell matrix:
    //
    //   literal url            · no permits   check AUTH-006 · run AUTH-006
    //   `${{ const.x }}`       · no permits   check AUTH-006 · run AUTH-006
    //   `${{ const.x }}/tail`  · no permits   check GREEN    · run SEC-004
    //   `${{ const.x }}/tail`  · permits: {}  check SEC-004  · run SEC-004
    //
    // A shipped template sat in the green cell. Interpolating a const into a
    // LARGER string is not a bare reference, so it stays dynamic — correctly —
    // and the tool conjunct went down with it.
    //
    // Per the waiver rule in `lib.rs`: the sub-question that survives is « is
    // this tool granted at all », which is a set-emptiness test. « which host
    // or path » is the part that really does belong to the runtime, and it
    // still does.
    match builtin_effect(a) {
        Some(BuiltinEffect::Net { url_arg }) => {
            if judgeable_arg(consts, a, url_arg).is_none() {
                escapes_tool(id, "invoke", tool, out);
                return;
            }
            out.push(CapabilityEscape {
                task: id.to_owned(),
                category: "net",
                detail: format!(
                    "invoke `{tool}` with a literal URL under an absent `permits:` block"
                ),
                fix: Some(format!(
                    "add \"{tool}\" to permits.tools and the host to permits.net.http"
                )),
                floor: false,
                undeclared: false,
            });
        }
        Some(BuiltinEffect::Fs { path_arg, .. }) => {
            if judgeable_arg(consts, a, path_arg).is_none() {
                escapes_tool(id, "invoke", tool, out); // the decidable conjunct
                return;
            }
            out.push(CapabilityEscape {
                task: id.to_owned(),
                category: "fs",
                detail: format!(
                    "invoke `{tool}` with a literal path under an absent `permits:` block"
                ),
                fix: Some(format!(
                    "add \"{tool}\" to permits.tools and the path to permits.fs"
                )),
                floor: false,
                undeclared: false,
            });
        }
        _ => escapes_tool(id, "invoke", tool, out),
    }
}

/// An `exec:` task under a `permits:` boundary. A `false`/omitted permit
/// denies any exec; a program allowlist verifies `argv[0]` of the ARRAY
/// form only — the shell-string form under an allowlist is an escape by
/// FORM (runtime parity: dispatch refuses that pairing wholesale). Under
/// the ABSENT block (F-O8 · zero authority) EVERY form escapes: NEP-0003
/// law 1 puts the exec CAPABILITY in `Required` the moment an exec task
/// sits in the body, whatever the command form — law 3's runtime deferral
/// owns the dynamic VALUE cases (which program · which host), never the
/// category question, which ∅ decides.
fn check_exec(id: &str, command: &RawCommand, permits: &Permits, out: &mut Vec<CapabilityEscape>) {
    if !permits.allows_exec() {
        // NEP-0003 law 1 · the category refusal is statically decidable
        // under the zero boundary (∅ grants nothing — the run is refused
        // before spawn whatever the command turns out to be), so EVERY
        // form escapes here: argv literal, computed argv, shell string.
        // (The 2026-08-04 probe: the shell spelling passed GREEN while
        // the argv twin was refused — the gate blocked the verifiable
        // door and opened the unverifiable one. The runtime refused
        // both; this restores check≡run.)
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
            undeclared: false,
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
                undeclared: false,
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
                undeclared: false,
            });
        }
    }
    check_exec_net(id, command, permits, out);
}

/// The net arm of the exec fit (the 2026-07-29 audit · run 5 · D1): an
/// exec whose argv (or shell line) carries a LITERAL URL judges that
/// host exactly like an invoke's (one boundary, one voice — measured:
/// `exec: ["curl", "https://evil.example.com"]` outside `permits.net.http`
/// passed check clean and died only at the OS sandbox — check-green,
/// run-refused on a statically decidable class). Where the form itself is
/// already refused (a shell line under a program allowlist · SEC-004) the
/// net question is moot and this arm never runs; an admissible shell line
/// is scanned for standalone URL tokens only — a conservative read that
/// can miss obfuscation but never falsely refuses (the fit lane's own
/// law); the runtime sandbox owns everything deeper.
fn check_exec_net(
    id: &str,
    command: &RawCommand,
    permits: &Permits,
    out: &mut Vec<CapabilityEscape>,
) {
    let tokens: Vec<&str> = match command {
        RawCommand::Argv(parts) => parts.iter().map(|p| p.value.as_str()).collect(),
        RawCommand::Shell(line) => line.value.split_whitespace().collect(),
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown exec command form: {other:?}"),
    };
    for tok in tokens {
        if !(tok.starts_with("http://") || tok.starts_with("https://")) {
            continue;
        }
        let Some(host) = url_host(tok) else {
            continue;
        };
        if nika_types::net::host_is_blocked(&host) {
            continue; // the SSRF floor names it already (one voice, never twice)
        }
        if !permits.allows_host(&host) {
            out.push(CapabilityEscape {
                task: id.to_owned(),
                category: "net",
                detail: format!("exec URL host `{host}` is outside permits.net.http"),
                fix: Some(format!("add \"{host}\" to permits.net.http")),
                floor: false,
                undeclared: false,
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
    consts: &ConstStrings,
    out: &mut Vec<CapabilityEscape>,
) {
    let Some(tool_ref) = a.tool() else {
        return; // a workflow: call is the composition lane's (COMP-002)
    };
    let tool = tool_ref.value.as_str();
    match builtin_effect(a) {
        Some(BuiltinEffect::Net { url_arg }) => {
            check_net_effect(id, tool, a, permits, consts, url_arg, out);
        }
        Some(BuiltinEffect::Fs {
            path_arg,
            reads,
            writes,
            ..
        }) => {
            let Some(path) = judgeable_arg(consts, a, path_arg) else {
                return;
            };
            for (active, dir_writes, cat) in [(reads, false, "fs.read"), (writes, true, "fs.write")]
            {
                if active && !permits.allows_path(&path, dir_writes) {
                    out.push(fs_escape(id, tool, &path, cat, permits, dir_writes));
                }
            }
            // The chart vega sibling is a SECOND gated write — same
            // boundary test as the artifact itself.
            if let Some(vl) = chart_vl_sibling(a)
                && !permits.allows_path(&vl, true)
            {
                let mut esc = fs_escape(id, tool, &vl, "fs.write", permits, true);
                esc.detail = format!("`{tool}` vega sibling `{vl}` is outside permits.fs.write");
                out.push(esc);
            }
        }
        None => {}
    }
}

/// One fs boundary escape — and the shovel stays in the shed (user
/// gauntlet 2026-07-31 · G-09: on a deliberate `../../pwned.md` probe
/// the printed fix taught ADDING the escape path to `permits.fs.write`
/// — "the checker hands the junior the shovel along with the hole").
/// A path that ESCAPES the workspace (absolute · `..`-traversal out of
/// the root) carries NO machine fix — the agent repair loop must never
/// auto-widen a boundary toward an escape (the floor-escape precedent:
/// no permits entry is the honest repair) — and the detail teaches the
/// narrow way first, the widening named as the deliberate second.
fn fs_escape(
    id: &str,
    tool: &str,
    path: &str,
    cat: &'static str,
    permits: &Permits,
    dir_writes: bool,
) -> CapabilityEscape {
    if !path_escapes_workspace(path) {
        return CapabilityEscape {
            task: id.to_owned(),
            category: "fs",
            detail: format!("`{tool}` path `{path}` is outside permits.{cat}"),
            fix: Some(format!("add \"{path}\" to permits.{cat}")),
            floor: false,
            undeclared: false,
        };
    }
    let declared = permits
        .fs
        .as_ref()
        .map(|f| if dir_writes { &f.write } else { &f.read })
        .filter(|list| !list.is_empty())
        .map_or_else(
            || format!("declare the {cat} boundary you mean"),
            |list| {
                format!(
                    "keep the path inside the declared {cat} ({})",
                    list.join(" · ")
                )
            },
        );
    CapabilityEscape {
        task: id.to_owned(),
        category: "fs",
        detail: format!(
            "`{tool}` path `{path}` escapes the workspace — {declared} · widening \
             permits.{cat} to an escaping path is a deliberate operator choice, never \
             the default repair"
        ),
        fix: None,
        floor: false,
        undeclared: false,
    }
}

/// Lexically, does the path leave the workflow's root? Absolute paths
/// and `..`-traversals that climb past the anchor do; `a/../b` does
/// not. Purely textual (the check has no filesystem) — symlink escapes
/// stay the runtime gate's, as the PERMITS verdict already states.
fn path_escapes_workspace(path: &str) -> bool {
    if path.starts_with('/') || path.starts_with('\\') {
        return true;
    }
    let mut depth: i32 = 0;
    for seg in path.split(['/', '\\']) {
        match seg {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            _ => depth += 1,
        }
    }
    false
}

/// The net arm of [`check_builtin_effect`] — one boundary escape per
/// judgeable host, and the set-emptiness refusal when no host is granted.
fn check_net_effect(
    id: &str,
    tool: &str,
    a: &RawInvokeAction,
    permits: &Permits,
    consts: &ConstStrings,
    url_arg: &'static str,
    out: &mut Vec<CapabilityEscape>,
) {
    // A floor-blocked host never gets a boundary escape: either it
    // is already flagged by `check_net_floor` (and the grant fix
    // would be a lie — no ordinary entry can admit it), or it is
    // DECLASSIFIED by an exact loopback literal (#395) — and that
    // same entry admits the host at the runtime boundary too
    // (`check_net_allowlist`), so there is nothing to escape.
    if let Some(host) = judgeable_arg(consts, a, url_arg)
        .as_deref()
        .and_then(url_host)
    {
        if !nika_types::net::host_is_blocked(&host) && !permits.allows_host(&host) {
            out.push(CapabilityEscape {
                task: id.to_owned(),
                category: "net",
                detail: format!("`{tool}` host `{host}` is outside permits.net.http"),
                fix: Some(format!("add \"{host}\" to permits.net.http")),
                floor: false,
                undeclared: false,
            });
        }
    } else {
        // WHICH host is unknowable · whether ANY host is granted is not.
        //
        // `permits.net.http` empty (or the whole `net:` block absent)
        // grants zero hosts, so every host lies outside it and the run
        // CANNOT succeed — a set-emptiness test, decidable without ever
        // learning the value. Measured 2026-07-29 on a `nika:notify`
        // whose target is a secret, all three arms of the boundary:
        //
        //   net.http absent      check GREEN · run NIKA-SEC-004
        //   net.http []          check GREEN · run NIKA-SEC-004
        //   net.http [other]     check GREEN · run NIKA-SEC-004
        //
        // Seven shipped showcase files sat in the first two. The third
        // arm stays silent here and MUST: with a non-empty allowlist the
        // static pass genuinely does not know whether the runtime host is
        // in it, and guessing would be the false-refusal this checker
        // must not make either.
        //
        // Per the waiver rule in `lib.rs`, this is the sub-question that
        // survives « the host is inside a secret, so we cannot judge it ».
        let grants_nothing = permits.net.as_ref().is_none_or(|n| n.http.is_empty());
        if grants_nothing {
            out.push(CapabilityEscape {
                task: id.to_owned(),
                category: "net",
                detail: format!(
                    "`{tool}` reaches a host computed at run time, and \
                             permits.net.http grants none — every host is outside \
                             an empty allowlist, so the run is refused whatever \
                             the value turns out to be"
                ),
                fix: Some(
                    "add the host to permits.net.http (a secret-borne \
                             webhook still needs its host named: `egress:` \
                             sanctions the FLOW, permits.net.http grants the \
                             CAPABILITY · spec 01-envelope §③)"
                        .to_owned(),
                ),
                floor: false,
                undeclared: false,
            });
        }
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

/// The `const:` string values, resolved once per scan.
///
/// `const:` is the ONE authority a run cannot move. Measured 2026-07-28,
/// `nika run <file> --var p=X` on a file whose `p` is a const answers with
/// the refusal « this workflow declares no inputs », because `--var`
/// satisfies `inputs:` and nothing else. So a bare `${{ const.<name> }}` has
/// a value that is known at check time and CANNOT differ at run time.
///
/// `inputs:` and `config:` are deliberately NOT resolved here even when they
/// carry a default — the run can supply another value, and a boundary verdict
/// computed against a value the run may replace is exactly the kind of claim
/// this checker must not make.
///
/// A pair list rather than a map: a `const:` block holds a handful of entries,
/// so a linear scan costs nothing and spares the crate a dependency (the house
/// lint forbids `std::collections::HashMap` in favour of `rustc_hash`, which
/// `nika-check` does not otherwise pull in).
#[derive(Debug, Default)]
pub(super) struct ConstStrings(Vec<(String, String)>);

impl ConstStrings {
    pub(super) fn of(wf: &RawWorkflow) -> Self {
        use nika_schema::types::VarDecl;
        Self(
            wf.consts
                .iter()
                .filter_map(|(k, decl)| {
                    let v = match decl {
                        VarDecl::Untyped(v)
                        | VarDecl::Typed {
                            default: Some(v), ..
                        } => v,
                        VarDecl::Typed { default: None, .. } => return None,
                    };
                    Some((k.value.clone(), v.as_str()?.to_owned()))
                })
                .collect(),
        )
    }

    /// A bare `${{ const.<name> }}` resolved to its declared string.
    ///
    /// BARE only: further navigation (`.field` · `[0]`), operators, or any
    /// surrounding text make the result something other than the const, and
    /// this returns `None` so the value stays the runtime's concern. Same
    /// discipline as `cost::static_vars_array_len`, which resolves this
    /// exact shape to bound a `for_each` count — one rung already treats a
    /// const-backed expression as static, and the asymmetry between them is
    /// what let a boundary escape through.
    fn resolve(&self, expr: &str) -> Option<&str> {
        let inner = expr.trim().strip_prefix("${{")?.strip_suffix("}}")?.trim();
        let name = inner.strip_prefix("const.")?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        self.0
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// `args.<key>` as a value this scan can judge — a literal, OR a bare
/// `${{ const.<name> }}` resolved through the const table.
///
/// The security reason this exists (F8, measured 2026-07-28): reading
/// `const:`-backed paths as dynamic made under-declaring both free and
/// profitable. Two files with a byte-identical `tasks:` block, one line of
/// `permits:` apart — the honest one was refused `NIKA-SEC-009`, and the
/// one whose `fs.read` grant was DELETED passed `--native-strict` with
/// `0 hints` and a green audited card, then died at run on its first task
/// with `NIKA-SEC-004`. The gate blocked what worked and passed what could
/// not run, because TRIFECTA reads its private-read leg off `permits:`
/// while the effect stayed in the body. Resolving the const closes it at
/// the source: the under-declared file now fails PERMITS and never reaches
/// TRIFECTA with a short leg.
pub(super) fn judgeable_arg(
    consts: &ConstStrings,
    a: &RawInvokeAction,
    key: &str,
) -> Option<String> {
    let s = a.args.as_ref()?.value.get(key)?.as_str()?;
    if s.contains("${{") {
        return consts.resolve(s).map(str::to_owned);
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
#[path = "permits_fit_tests.rs"]
mod tests;
