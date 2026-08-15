// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The permit-parameterization taint (NEP-0004 · LAW-AUTH-0325) — the
//! STATIC twin of the runtime re-gate (`nika-runtime`'s `dispatch/regate.rs`
//! · the `NIKA-SEC-004` run-time half). The category fit
//! ([`super::permits_fit`]) proves `Required ⊆ Declared` on CATEGORIES;
//! this lane binds the VALUES flowing under a PRESENT `permits:` block:
//!
//! - **Law 1 ([`PermitTaintKind::BoundInterpolated`] · `NIKA-AUTH-007`)** —
//!   an interpolation reaching a permit BOUND (a host/glob/program literal
//!   inside `permits:`) is a hard refusal: the bound MUST be a literal,
//!   the boundary would be self-serve, there is nothing left to
//!   canonicalize against. The parser never renders `permits:` (a `${{ }}`
//!   island is a mute literal there today — the silent hole), so the
//!   rejection is this static gate, before any token.
//! - **Law 2 ([`PermitTaintKind::ArgEscapes`] · `NIKA-AUTH-008`)** — an
//!   UNTRUSTED value reaching a permitted verb's ARGUMENT is re-gated on
//!   its RESOLVED, canonical form against the step's permit: `inputs.*`
//!   (the one root resolvable at check, via its declared `default:` —
//!   `config.*` was the second until that authority died with the 9-key
//!   envelope) substitutes, the fs path folds lexically
//!   ([`lexically_normalize`]) before the glob match, the net host reads
//!   through the shared WHATWG extractor ([`super::permits_fit::url_host`]),
//!   and an exec argv tail token of the re-entry class (`--exec` · `-c` ·
//!   `eval`…) is never covered unless the permit lists it.
//! - **Law 4 (deferral)** — an untrusted reference NOT resolvable at
//!   check (no default · a `with:`/`tasks.*` derivation) DEFERS: the file
//!   stays valid and the run-time re-gate is mandatory. Law 4's
//!   informational « deferred re-gates » listing (a SHOULD) is a declared
//!   v1 gap — the hint rail would fire on every ordinary
//!   `${{ tasks.x.output }}` workflow.
//! - **Law 5 (`declassify:`)** — a task-level entry raises its `from:`
//!   binding to trusted HERE (the static twin never re-gates it); the
//!   value is still matched like a literal everywhere else (never a
//!   permit bypass) and the run receipt records the event.
//! - **F-P5 ([`PermitTaintKind::NetWildcard`] · `NIKA-AUTH-010`)** — a
//!   `permits.net.http:` entry carrying the `*.` subdomain wildcard is a
//!   hard refusal: the grant delegates the boundary to the zone operator
//!   (every host under the suffix, present and future — the srt A2
//!   lesson). The BARE `*` stays legal: the explicit allow-all hatch
//!   (`NetPolicy::Allow` — visible, never stealth).
//!
//! Everything is judged against the WORKFLOW's `permits:` block (v1 has
//! no task-level narrowing — the step permit IS the declared block), and
//! only when the block is PRESENT (an absent/null block is NEP-0003's
//! ground — `NIKA-AUTH-006`, the `permits_fit` lane).

use nika_cap::lexically_normalize;
use nika_schema::expression::{NamespaceRef, expr_refs, scan_templates};
use nika_schema::raw::{RawAction, RawCommand, RawWorkflow};
use nika_schema::types::{ExecPermit, Permits, VarDecl};

use super::permits_fit::url_host;

/// The wire code of a bound interpolation (law 1).
pub(crate) const BOUND_CODE: &str = "NIKA-AUTH-007";
/// The wire code of an untrusted-argument escape (law 2).
pub(crate) const REGATE_CODE: &str = "NIKA-AUTH-008";
/// The wire code of a dangerous-floor dead grant in `permits.env:`
/// (NEP-0005 law 3 · LAW-AUTH-0326).
pub(crate) const ENV_DEAD_GRANT_CODE: &str = "NIKA-AUTH-009";
/// The wire code of a `*.` subdomain wildcard in `permits.net.http:`
/// (F-P5 · the wildcard delegates the boundary to the zone operator).
pub(crate) const NET_WILDCARD_CODE: &str = "NIKA-AUTH-010";

/// The fs-read tool set whose `path` arg the static re-gate judges
/// (the reference oracle's table — the engine≡oracle differential law
/// LAW-AUTH-0319 forbids a wider net here; every other dynamic arg
/// defers to the runtime re-gate).
const FS_READ_TOOLS: &[&str] = &["nika:read", "nika:glob", "nika:grep"];
/// The fs-write twin (an `edit`'s untrusted path re-gates on the WRITE
/// direction only — the read half is the runtime boundary's).
const FS_WRITE_TOOLS: &[&str] = &["nika:write", "nika:edit"];
/// The net egress arg for a tool, ASKED of the authoritative table
/// rather than re-typed here.
///
/// The re-gate used to carry its own `NET_TOOLS` list and read `"url"`
/// from every member. `nika:notify`'s URL arg is `target` — a fact
/// `nika-cap` has always known — so the tool was in the list and inert:
/// a tainted `target` escaping the declared hosts passed check with
/// rc=0, while the identical shape on `nika:fetch` was refused with
/// NIKA-AUTH-008 (proven 2026-08-02). A list that names a tool without
/// knowing how to read it is worse than one that omits it, because the
/// name reads as coverage.
///
/// The `Net` arm also encodes notify's channel rule (webhook, present
/// or defaulted), which the flat list could not express at all.
fn net_url_arg(
    tool: &str,
    args: Option<&nika_schema::Spanned<serde_json::Value>>,
) -> Option<&'static str> {
    if let Some(nika_cap::BuiltinEffect::Net { url_arg }) =
        nika_cap::builtin_effect(tool, args.map(|a| &a.value))
    {
        return Some(url_arg);
    }
    // The undecidable channel. `builtin_effect` classifies notify as net
    // only when `channel:` reads literally `webhook`, so an INTERPOLATED
    // channel makes the whole tool unclassifiable and the re-gate went
    // silent — the same shape as the arg-name bug above, through a
    // different door: `channel: "${{ inputs.ch }}"` with a tainted
    // target passed check rc=0 where the literal spelling gives rc=2
    // (2026-08-02).
    //
    // Re-gating on an unknown channel cannot invent a finding. A
    // non-webhook channel's `target` is not a URL (`#general`, an
    // address), `url_host` returns None on it, and the branch below
    // never fires. Only a target that IS a URL escaping the declared
    // hosts speaks — which is a finding whatever channel carries it.
    if tool == "nika:notify" && string_arg(args, "channel").is_some() {
        return Some("target");
    }
    None
}
/// The exec re-entry class (spec 10 §the permit-parameterization taint):
/// a token that re-enters a command interpreter is never covered by a
/// program allowlist unless the permit lists the token itself.
const REENTRY_TOKENS: &[&str] = &["--exec", "--execdir", "-exec", "-execdir", "-c", "eval"];

/// Which NEP-0004 rule the finding speaks (the wire-code mapping is the
/// ONE match arm every surface — findings · conformance codes · CLI —
/// reads, so the two classes can never swap).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub enum PermitTaintKind {
    /// Law 1 · a permit bound is interpolated, not literal
    /// (`NIKA-AUTH-007`).
    BoundInterpolated,
    /// Law 2 · an untrusted value's canonical resolved form escapes the
    /// step's permit (`NIKA-AUTH-008`).
    ArgEscapes,
    /// NEP-0005 law 3 · a `permits.env:` entry names a dangerous-floor
    /// variable — an inert dead grant, the engine strips the name
    /// unconditionally (`NIKA-AUTH-009` · LAW-AUTH-0326).
    EnvDeadGrant,
    /// F-P5 · a `permits.net.http:` entry carries the `*.` subdomain
    /// wildcard — the grant delegates the boundary to the zone operator,
    /// admitting every host under the suffix, present and future
    /// (`NIKA-AUTH-010`). The BARE `*` stays legal (the explicit
    /// allow-all hatch).
    NetWildcard,
}

/// One permit-taint finding (law 1 or law 2) — the check-time twin of
/// the runtime re-gate's `NIKA-SEC-004` refusal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct PermitTaint {
    /// The offending task (law 2), or `permits` for a bound finding
    /// (law 1 — the bound is its own site).
    pub task: String,
    /// Which law speaks.
    pub kind: PermitTaintKind,
    /// The witness — law 3's shape: the taint path source-first, the
    /// resolved value, its canonical form, and the bound it escaped
    /// (law 1: the interpolated bound's own path).
    pub detail: String,
    /// The machine-applicable repair (the one idiom per class).
    pub fix: Option<String>,
}

impl PermitTaint {
    /// The canonical spec code this finding stamps (one arm · every
    /// surface reads THIS).
    #[must_use]
    pub fn wire_code(&self) -> &'static str {
        match self.kind {
            PermitTaintKind::BoundInterpolated => BOUND_CODE,
            PermitTaintKind::ArgEscapes => REGATE_CODE,
            PermitTaintKind::EnvDeadGrant => ENV_DEAD_GRANT_CODE,
            PermitTaintKind::NetWildcard => NET_WILDCARD_CODE,
        }
    }
}

/// Scan a workflow for the permit-parameterization taint — law 1 (bound
/// literality) and law 2 (the static re-gate) under a PRESENT `permits:`
/// block; empty when no block is declared (NEP-0003 owns that ground).
#[must_use]
pub(crate) fn scan_permit_taint(wf: &RawWorkflow) -> Vec<PermitTaint> {
    let Some(permits) = wf.permits.as_ref() else {
        return Vec::new();
    };
    let permits = &permits.value;
    let mut out = Vec::new();
    scan_bound_literality(permits, &mut out);
    scan_env_dead_grants(permits, &mut out);
    scan_net_wildcards(permits, &mut out);
    scan_regate(wf, permits, &mut out);
    out
}

/// NEP-0005 law 3 (LAW-AUTH-0326) · a `permits.env:` entry naming a
/// dangerous-floor variable is an inert dead grant — the spawn sites
/// strip the name unconditionally (`nika_cap::env::compose_child_env` ·
/// the same list THIS reads, so the check and the composition cannot
/// drift), so the grant can never take effect. Interpolated entries are
/// law 1's ground (`NIKA-AUTH-007` · the bound walk above), never
/// judged here.
fn scan_env_dead_grants(permits: &Permits, out: &mut Vec<PermitTaint>) {
    let Some(env) = permits.env.as_ref() else {
        return;
    };
    for (i, name) in env.iter().enumerate() {
        if name.contains("${{") || !nika_cap::env::is_dangerous_env_name(name) {
            continue;
        }
        out.push(PermitTaint {
            task: "permits".to_owned(),
            kind: PermitTaintKind::EnvDeadGrant,
            detail: format!(
                "permit entry `env[{i}]` names the dangerous-floor variable `{name}` ·                  the engine strips this name unconditionally, the grant can never take                  effect: an inert dead grant (NEP-0005 law 3)"
            ),
            fix: Some(
                "remove the entry · pass authored data through the task env: map, or a                  non-dangerous engine variable by its exact name"
                    .to_owned(),
            ),
        });
    }
}

/// F-P5 · a `permits.net.http:` entry carrying the `*.` substring is
/// REFUSED (the srt A2 lesson: the wildcard delegates the permit to the
/// zone operator — `*.github.io` is every user of the shared zone,
/// present and future; the matcher cannot see what the zone will serve
/// tomorrow). The rule is the verbatim substring: the leading form
/// (`*.github.com`) AND a would-be glob like `foo*.com` (which matches
/// nothing by matcher law yet LOOKS like one — the author meant a glob)
/// are both refused. The BARE `*` stays legal: the author's explicit
/// allow-all hatch (the sandbox projection maps it to `NetPolicy::Allow`
/// — visible, never stealth). Interpolated entries are law 1's ground
/// (`NIKA-AUTH-007` · the bound walk above), never judged here.
fn scan_net_wildcards(permits: &Permits, out: &mut Vec<PermitTaint>) {
    let Some(net) = permits.net.as_ref() else {
        return;
    };
    for (i, entry) in net.http.iter().enumerate() {
        if entry.contains("${{") || !entry.contains("*.") {
            continue;
        }
        out.push(PermitTaint {
            task: "permits".to_owned(),
            kind: PermitTaintKind::NetWildcard,
            detail: format!(
                "permit entry `net.http[{i}]` carries the `*.` subdomain wildcard \
                 (`{entry}`) · the grant delegates the boundary to the zone \
                 operator: every host under the suffix, present and future, is \
                 admitted (F-P5)"
            ),
            fix: Some(
                "name the exact hosts · or, when allow-all is genuinely intended, \
                 the bare `*` (the explicit hatch · maps to NetPolicy::Allow)"
                    .to_owned(),
            ),
        });
    }
}

/// Law 1 · every bound string inside the block MUST be a literal — an
/// interpolation reaching the wall itself is a hard refusal (NEP-0004's
/// « the boundary would be self-serve »). Walks the four carrier
/// families (fs globs · net hosts · exec programs · tool grants) and
/// names each interpolated bound by its own path (`net.http[0]`).
fn scan_bound_literality(permits: &nika_schema::types::Permits, out: &mut Vec<PermitTaint>) {
    let mut check = |path: String, bound: &str| {
        if bound.contains("${{") {
            out.push(PermitTaint {
                task: "permits".to_owned(),
                kind: PermitTaintKind::BoundInterpolated,
                detail: format!(
                    "permit bound `{path}` is interpolated, not literal · a bound is \
                     the wall itself: there is nothing left to canonicalize against \
                     (NEP-0004 law 1)"
                ),
                fix: Some(
                    "write the literal host/glob/program and gate the data in the body \
                     (the NIKA-AUTH-008 re-gate checks the value there)"
                        .to_owned(),
                ),
            });
        }
    };
    if let Some(fs) = permits.fs.as_ref() {
        for (i, glob) in fs.read.iter().enumerate() {
            check(format!("fs.read[{i}]"), glob);
        }
        for (i, glob) in fs.write.iter().enumerate() {
            check(format!("fs.write[{i}]"), glob);
        }
    }
    if let Some(net) = permits.net.as_ref() {
        for (i, host) in net.http.iter().enumerate() {
            check(format!("net.http[{i}]"), host);
        }
    }
    if let Some(ExecPermit::Programs(programs)) = permits.exec.as_ref() {
        for (i, program) in programs.iter().enumerate() {
            check(format!("exec[{i}]"), program);
        }
    }
    if let Some(tools) = permits.tools.as_ref() {
        for (i, tool) in tools.iter().enumerate() {
            check(format!("tools[{i}]"), tool);
        }
    }
    // NEP-0005 · env names are bounds too (LAW-AUTH-0325 covers every
    // category · the parser lets the island through so THIS refusal
    // carries the teaching detail).
    if let Some(env) = permits.env.as_ref() {
        for (i, name) in env.iter().enumerate() {
            check(format!("env[{i}]"), name);
        }
    }
}

/// Law 2 · the static re-gate: an untrusted value (`inputs.*` ·
/// `config.*` — the two roots resolvable at check, via their declared
/// defaults) reaching a permitted verb's argument is matched on its
/// RESOLVED, canonical form against the step's permit. The re-gate
/// judges the exact surfaces the reference oracle judges (the engine≡
/// oracle differential forbids a wider net): the fs/net builtin args
/// below, and the argv form of an exec under a program allowlist. Every
/// other dynamic value defers to the runtime re-gate (`NIKA-SEC-004`),
/// never a check error (law 4).
fn scan_regate(wf: &RawWorkflow, permits: &Permits, out: &mut Vec<PermitTaint>) {
    // The boundary the re-gate matches against drops interpolated bounds:
    // those are law 1's refusal, never something to match against (the
    // oracle's `_taint_globbed` rule — a `${{ }}` glob admits nothing).
    let boundary = literal_permits(permits);
    for task in &wf.tasks {
        let task = &task.value;
        // the `taint` doors only — a `data-as-code` lift raises no
        // binding, and `from:` is parser-forbidden on it
        let declassified: Vec<&str> = task
            .taint_lifts()
            .filter_map(|entry| entry.from.as_ref())
            .map(|from| from.value.as_str())
            .collect();
        match &task.action {
            RawAction::Invoke(a) => {
                regate_invoke(task, a, wf, permits, &boundary, &declassified, out);
            }
            RawAction::Exec(a) => {
                regate_exec(task, a, wf, permits, &declassified, out);
            }
            _ => {}
        }
    }
}

/// The invoke arm — the fs/net builtin args the oracle's table judges
/// (a child `workflow:` call has no tool: spec 14 owns its boundary).
fn regate_invoke(
    task: &nika_schema::raw::RawTask,
    a: &nika_schema::raw::RawInvokeAction,
    wf: &RawWorkflow,
    permits: &Permits,
    boundary: &Permits,
    declassified: &[&str],
    out: &mut Vec<PermitTaint>,
) {
    let Some(tool) = a.tool() else {
        return;
    };
    let tool = tool.value.as_str();
    let fs_write = if FS_READ_TOOLS.contains(&tool) {
        Some(false)
    } else if FS_WRITE_TOOLS.contains(&tool) {
        Some(true)
    } else {
        None
    };
    if let Some(write) = fs_write {
        if let Some(template) = string_arg(a.args.as_ref(), "path")
            && let Some((resolved, paths)) = resolve_untrusted(template, wf, declassified)
            && !boundary.allows_path(&resolved, write)
        {
            let cat = if write { "fs.write" } else { "fs.read" };
            let globs = fs_globs(permits, write);
            out.push(regate_finding(
                &task.id.value,
                &paths,
                &format!("args.path ({tool})"),
                &resolved,
                &lexically_normalize(&resolved),
                &format!("{cat} {globs}"),
                "keep the value inside the boundary",
            ));
        }
    } else if let Some(url_arg) = net_url_arg(tool, a.args.as_ref())
        && let Some(template) = string_arg(a.args.as_ref(), url_arg)
        && let Some((resolved, paths)) = resolve_untrusted(template, wf, declassified)
        && let Some(host) = url_host(&resolved)
        && !boundary.allows_host(&host)
    {
        out.push(regate_finding(
            &task.id.value,
            &paths,
            &format!("args.{url_arg} ({tool})"),
            &resolved,
            &host,
            &format!("net.http {:?}", net_globs(permits)),
            "keep the value inside the boundary",
        ));
    }
}

/// The exec arm — the argv form under a program allowlist (a shell
/// string is already refused by FORM; `exec: true` re-gates at run).
fn regate_exec(
    task: &nika_schema::raw::RawTask,
    a: &nika_schema::raw::RawExecAction,
    wf: &RawWorkflow,
    permits: &Permits,
    declassified: &[&str],
    out: &mut Vec<PermitTaint>,
) {
    let (RawCommand::Argv(elements), Some(ExecPermit::Programs(programs))) =
        (&a.command, permits.exec.as_ref())
    else {
        return;
    };
    for (i, element) in elements.iter().enumerate() {
        let template = element.value.as_str();
        if !template.contains("${{") {
            continue;
        }
        let Some((resolved, paths)) = resolve_untrusted(template, wf, declassified) else {
            continue;
        };
        if i == 0 {
            // The program itself is data → re-gate against the
            // (literal) allowlist.
            let allowlist: Vec<&str> = programs
                .iter()
                .filter(|p| !p.contains("${{"))
                .map(String::as_str)
                .collect();
            if !allowlist.contains(&resolved.as_str()) {
                out.push(regate_finding(
                    &task.id.value,
                    &paths,
                    "argv[0] (exec)",
                    &resolved,
                    &resolved,
                    &format!("exec {programs:?}"),
                    "name the literal program in permits.exec",
                ));
            }
        } else if REENTRY_TOKENS.contains(&resolved.as_str()) && !programs.contains(&resolved) {
            out.push(regate_finding(
                &task.id.value,
                &paths,
                &format!("argv[{i}] (exec)"),
                &resolved,
                &resolved,
                &format!("exec {programs:?} · the re-entry class is never covered unless listed"),
                "drop the token",
            ));
        }
    }
}

/// One law-2 finding — the witness speaks law 3's shape: the taint path
/// source-first, the resolved (default) value, its canonical form, and
/// the bound it escaped.
fn regate_finding(
    task: &str,
    paths: &[String],
    sink: &str,
    resolved: &str,
    canonical: &str,
    bound: &str,
    repair: &str,
) -> PermitTaint {
    PermitTaint {
        task: task.to_owned(),
        kind: PermitTaintKind::ArgEscapes,
        detail: format!(
            "task `{task}` passes an untrusted value that escapes the step permit \
             (NEP-0004 law 2) · taint path: {} -> {sink} · resolved (default): \
             {resolved:?} -> canonical {canonical:?} ∉ {bound}",
            paths.join(" , "),
        ),
        fix: Some(format!(
            "{repair} · or declare declassify: on the task (the only door)"
        )),
    }
}

/// Resolve the UNTRUSTED `${{ }}` references of a verb-argument string at
/// check — the reference oracle's substitution semantics, verbatim.
/// Returns the resolved string plus the taint paths (source bindings) when
/// ≥1 untrusted reference resolves and NONE stays dynamic; `None` when no
/// untrusted reference appears (a trusted string — the plain category fit
/// judges it, never this law) or when ANY reference defers (law 4: no
/// default · a `with:`/`tasks.*`/loop-local derivation · a declassified
/// binding — the run-time re-gate is mandatory there). Substituting only
/// the untrusted defaults keeps the honest file green: literal / `const` /
/// `secrets` references never reach this law.
fn resolve_untrusted(
    template: &str,
    wf: &RawWorkflow,
    declassified: &[&str],
) -> Option<(String, Vec<String>)> {
    let islands = scan_templates(template).ok()?;
    if islands.is_empty() {
        return None; // no interpolation at all — a literal, trusted
    }
    let mut resolved = String::with_capacity(template.len());
    let mut cursor = 0_usize;
    let mut paths: Vec<String> = Vec::new();
    for island in &islands {
        // ONE root reference, exactly — a computed island (zero refs · a
        // CEL combination) is not statically decidable → defer (law 4).
        let refs = expr_refs(&island.expr);
        let [reference] = refs.as_slice() else {
            return None;
        };
        let (root, name) = match reference {
            NamespaceRef::Inputs(name) => ("inputs", name),
            _ => return None, // trusted root (const · secrets) or dynamic
        };
        let binding = format!("{root}.{name}");
        if declassified.contains(&binding.as_str()) {
            return None; // the declassify: door — trusted HERE (law 5)
        }
        let default = string_default(wf, root, name)?;
        resolved.push_str(&template[cursor..island.start]);
        resolved.push_str(default);
        cursor = island.end;
        paths.push(binding);
    }
    resolved.push_str(&template[cursor..]);
    Some((resolved, paths))
}

/// The declared STRING default of an `inputs.`/`config.` binding — the
/// one shape the static re-gate can resolve (a non-string or absent
/// default defers: caller-supplied at launch, law 4).
fn string_default<'a>(wf: &'a RawWorkflow, root: &str, name: &str) -> Option<&'a str> {
    // `inputs:` is the only declaring authority left with a `default:`
    // (`config:` died 2026-08-13 · `const:` has no default, it IS the value).
    if root != "inputs" {
        return None;
    }
    let declarations = &wf.inputs;
    let (_, decl) = declarations.iter().find(|(n, _)| n.value == name)?;
    match decl {
        VarDecl::Typed {
            default: Some(serde_json::Value::String(s)),
            ..
        } => Some(s.as_str()),
        _ => None,
    }
}

/// The boundary the re-gate matches against: the declared block minus
/// every interpolated bound (a `${{ }}` bound is law 1's refusal, never
/// something to match against — the oracle's `_taint_globbed` parity).
fn literal_permits(permits: &Permits) -> Permits {
    let mut filtered = permits.clone();
    let literal = |entries: &mut Vec<String>| entries.retain(|e| !e.contains("${{"));
    if let Some(fs) = filtered.fs.as_mut() {
        literal(&mut fs.read);
        literal(&mut fs.write);
    }
    if let Some(net) = filtered.net.as_mut() {
        literal(&mut net.http);
    }
    if let Some(ExecPermit::Programs(programs)) = filtered.exec.as_mut() {
        literal(programs);
    }
    if let Some(tools) = filtered.tools.as_mut() {
        literal(tools);
    }
    filtered
}

/// A string `args.<key>` that carries an interpolation (the re-gate's
/// input shape); `None` when the arg is absent, non-string, or literal
/// (the plain category fit owns literal args).
fn string_arg<'a>(
    args: Option<&'a nika_schema::Spanned<serde_json::Value>>,
    key: &str,
) -> Option<&'a str> {
    let s = args?.value.get(key)?.as_str()?;
    s.contains("${{").then_some(s)
}

/// The declared `fs.{read,write}` globs for a finding's escaped bound
/// (the DECLARED list, verbatim — the author reads their own boundary).
fn fs_globs(permits: &Permits, write: bool) -> String {
    match &permits.fs {
        Some(fs) => {
            let globs = if write { &fs.write } else { &fs.read };
            format!("{globs:?}")
        }
        None => "[]".to_owned(),
    }
}

/// The declared `net.http` globs (same verbatim honesty).
fn net_globs(permits: &Permits) -> &[String] {
    match &permits.net {
        Some(net) => &net.http,
        None => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn taints_of(yaml: &str) -> Vec<PermitTaint> {
        scan_permit_taint(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn an_interpolated_bound_is_a_hard_refusal() {
        // Conformance fixture core/authority/010 — the default even
        // MATCHES the intended host: irrelevant, the boundary would be
        // self-serve.
        let y = r#"nika: w
permits:
  net: { http: ["${{ inputs.host }}"] }
  tools: ["nika:fetch"]
inputs:
  host: { type: string, required: false, default: "api.example.com" }
tasks:
  grab:
    invoke:
      tool: nika:fetch
      args: { url: "https://${{ inputs.host }}/data" }
"#;
        let taints = taints_of(y);
        assert!(
            taints
                .iter()
                .any(|t| t.kind == PermitTaintKind::BoundInterpolated
                    && t.detail.contains("net.http[0]")),
            "law 1 names the bound's own path: {taints:?}"
        );
    }

    #[test]
    fn literal_bounds_and_an_absent_block_are_silent() {
        // Law 1 judges a PRESENT block only — absent is NEP-0003's ground.
        assert!(taints_of("nika: w\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n").is_empty());
        // …and literal bounds never fire (every honest file).
        let y = r#"nika: w
permits:
  fs: { read: ["datasets/**"] }
  net: { http: ["api.example.com"] }
  exec: ["find"]
  tools: ["nika:read"]
tasks:
  t:
    invoke: { tool: "nika:read", args: { path: "datasets/q3.csv" } }
"#;
        assert!(taints_of(y).is_empty(), "{:?}", taints_of(y));
    }

    #[test]
    fn every_bound_family_is_walked() {
        let y = r#"nika: w
permits:
  fs: { read: ["${{ inputs.a }}"], write: ["out/**"] }
  net: { http: ["${{ inputs.b }}"] }
  exec: ["${{ inputs.c }}"]
  tools: ["${{ inputs.d }}"]
  env: ["${{ inputs.e }}"]
tasks:
  t:
    exec: { command: ["true"] }
"#;
        let paths: Vec<String> = taints_of(y).iter().map(|t| t.detail.clone()).collect();
        for needle in ["fs.read[0]", "net.http[0]", "exec[0]", "tools[0]", "env[0]"] {
            assert!(
                paths.iter().any(|d| d.contains(needle)),
                "{needle} flagged: {paths:?}"
            );
        }
        // the literal write glob is NOT flagged.
        assert!(!paths.iter().any(|d| d.contains("fs.write[0]")));
    }

    // ── NEP-0005 law 3 · the env dead grant (NIKA-AUTH-009) — the
    //    conformance fixtures core/authority/014-017, mirrored ──

    #[test]
    fn fixture_015_env_dangerous_dead_grant_is_refused() {
        let y = r#"nika: t-env-dangerous-dead-grant
permits:
  exec: true
  env: ["LD_PRELOAD"]
tasks:
  build:
    exec: { shell: "echo ok" }
"#;
        let taints = taints_of(y);
        assert_eq!(taints.len(), 1, "{taints:?}");
        assert_eq!(taints[0].kind, PermitTaintKind::EnvDeadGrant);
        assert_eq!(taints[0].wire_code(), "NIKA-AUTH-009");
        assert!(taints[0].detail.contains("env[0]"), "{}", taints[0].detail);
        assert!(taints[0].detail.contains("LD_PRELOAD"));
    }

    #[test]
    fn fixtures_016_017_declared_and_zero_env_pass_clean() {
        for y in [
            r#"nika: t-env-passthrough-declared
permits:
  exec: true
  env: ["CI_COMMIT_SHA"]
tasks:
  stamp:
    exec: { shell: "echo ok" }
"#,
            r#"nika: t-env-declared-zero
permits:
  exec: true
  env: []
tasks:
  hermetic:
    exec: { shell: "echo ok" }
"#,
        ] {
            let taints = taints_of(y);
            assert!(taints.is_empty(), "{taints:?}");
        }
    }

    // ── F-P5 · the net wildcard refusal (NIKA-AUTH-010) ──

    #[test]
    fn a_subdomain_wildcard_in_net_http_is_a_hard_refusal() {
        // The matcher ADMITS the task's host through the wildcard (no
        // escape) — the refusal is entry-level: the grant itself is the
        // hole, whatever the body does.
        let y = r#"nika: t-wildcard
permits:
  net: { http: ["*.github.com"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://api.github.com/x" } }
"#;
        let taints = taints_of(y);
        assert_eq!(taints.len(), 1, "{taints:?}");
        assert_eq!(taints[0].kind, PermitTaintKind::NetWildcard);
        assert_eq!(taints[0].wire_code(), "NIKA-AUTH-010");
        assert_eq!(taints[0].task, "permits");
        assert!(
            taints[0].detail.contains("net.http[0]"),
            "{}",
            taints[0].detail
        );
        assert!(taints[0].detail.contains("*.github.com"));
        // …and every wildcard entry is named, exact neighbours aside.
        // `foo*.com` carries the `*.` substring (a would-be glob that
        // matches nothing yet LOOKS like one) — refused with the rest.
        let mixed = y.replace(
            "\"*.github.com\"",
            "\"api.example.com\", \"*.github.io\", \"foo*.com\"",
        );
        let taints = taints_of(&mixed);
        assert_eq!(taints.len(), 2, "{taints:?}");
        assert!(
            taints
                .iter()
                .all(|t| t.kind == PermitTaintKind::NetWildcard)
        );
        assert!(
            taints[0].detail.contains("net.http[1]"),
            "{}",
            taints[0].detail
        );
        assert!(
            taints[1].detail.contains("net.http[2]"),
            "{}",
            taints[1].detail
        );
    }

    #[test]
    fn the_bare_star_and_exact_hosts_stay_legal() {
        // The bare `*` is the explicit allow-all hatch (NetPolicy::Allow —
        // the author typed it, visible, never stealth): NOT this law's
        // ground (it carries no `*.` substring). Exact hosts neither.
        for entries in ["\"*\"", "\"api.github.com\", \"github.com\""] {
            let y = format!(
                "nika: w\npermits:\n  net: {{ http: [{entries}] }}\n  \
                 tools: [\"nika:fetch\"]\ntasks:\n  t:\n    \
                 invoke: {{ tool: \"nika:fetch\", args: {{ url: \"https://api.github.com/x\" }} }}\n"
            );
            assert!(
                taints_of(&y).is_empty(),
                "{entries} must stay legal: {:?}",
                taints_of(&y)
            );
        }
    }

    #[test]
    fn an_interpolated_wildcard_bound_is_law1s_ground_only() {
        // `${{ inputs.h }}` containing `*.` never double-fires: law 1
        // (BoundInterpolated) owns the interpolated bound.
        let y = r#"nika: w
permits:
  net: { http: ["${{ inputs.h }}"] }
inputs:
  h: { type: string, required: false, default: "*.github.com" }
tasks:
  t:
    exec: { command: ["true"] }
"#;
        let taints = taints_of(y);
        assert_eq!(taints.len(), 1, "{taints:?}");
        assert_eq!(taints[0].kind, PermitTaintKind::BoundInterpolated);
    }

    // ── Law 2 · the static re-gate (NIKA-AUTH-008) — the conformance
    //    fixtures core/authority/007..013, mirrored one test each ──

    #[test]
    fn fixture_007_untrusted_traversal_under_fs_read_is_refused() {
        let y = r#"nika: t
permits:
  fs: { read: ["datasets/**"] }
  tools: ["nika:read"]
inputs:
  p: { type: string, required: false, default: "../../etc/passwd" }
tasks:
  load:
    invoke:
      tool: nika:read
      args: { path: "${{ inputs.p }}" }
"#;
        let taints = taints_of(y);
        assert_eq!(taints.len(), 1, "{taints:?}");
        let t = &taints[0];
        assert_eq!(t.kind, PermitTaintKind::ArgEscapes);
        assert_eq!(t.wire_code(), "NIKA-AUTH-008");
        assert!(
            t.detail.contains("inputs.p -> args.path (nika:read)")
                && t.detail
                    .contains("resolved (default): \"../../etc/passwd\"")
                && t.detail.contains("∉ fs.read [\"datasets/**\"]"),
            "taint path source-first + canonical + the escaped bound: {}",
            t.detail
        );
    }

    #[test]
    fn fixture_008_untrusted_exec_reentry_flag_is_refused() {
        let y = r#"nika: t
permits:
  exec: ["find"]
inputs:
  extra: { type: string, required: false, default: "--exec" }
tasks:
  search:
    exec: { command: ["find", ".", "-name", "report", "${{ inputs.extra }}"] }
"#;
        let taints = taints_of(y);
        assert_eq!(taints.len(), 1, "{taints:?}");
        assert!(
            taints[0].detail.contains("inputs.extra -> argv[4] (exec)")
                && taints[0].detail.contains("re-entry"),
            "the re-entry class speaks: {}",
            taints[0].detail
        );
        // …while a DATA value (not the re-entry class) defers to the
        // runtime re-gate — the static twin refuses only the named class.
        let data = y.replace("default: \"--exec\"", "default: \"-empty\"");
        assert!(
            taints_of(&data).is_empty(),
            "a non-reentry token defers (law 4): {:?}",
            taints_of(&data)
        );
    }

    /// The fixture above proves `--exec`. The other five rode on its
    /// reputation: each names a way back into an interpreter, and each
    /// must be refused by its own name.
    #[test]
    fn every_reentry_token_is_refused_by_name() {
        for token in REENTRY_TOKENS {
            let y = format!(
                "nika: t\npermits:\n  exec: [\"find\"]\ninputs:\n  \
                 extra: {{ type: string, required: false, default: \"{token}\" }}\ntasks:\n  search:\n    \
                 exec: {{ command: [\"find\", \".\", \"${{{{ inputs.extra }}}}\"] }}\n"
            );
            let taints = taints_of(&y);
            assert_eq!(taints.len(), 1, "{token:?} was not refused: {taints:?}");
            assert!(
                taints[0].detail.contains("re-entry"),
                "{token:?} refused for the wrong reason: {}",
                taints[0].detail
            );
        }
    }

    /// Iterating cannot see a deletion. This names each token and the door
    /// it closes, so removing one fails here instead of passing silently.
    #[test]
    fn the_reentry_floor_never_loses_a_token() {
        const FLOOR: &[(&str, &str)] = &[
            ("--exec", "find/xargs run an arbitrary program per match"),
            ("--execdir", "the same, from the match's directory"),
            ("-exec", "the single-dash spelling find also accepts"),
            ("-execdir", "the single-dash spelling of --execdir"),
            ("-c", "sh/bash/python read a program from the argument"),
            ("eval", "the shell builtin that re-parses its argument"),
        ];
        for (token, why) in FLOOR {
            assert!(
                REENTRY_TOKENS.contains(token),
                "{token:?} left the re-entry class · it re-opens: {why}"
            );
        }
    }

    #[test]
    fn fixture_009_untrusted_host_under_fetch_permit_is_refused() {
        let y = r#"nika: t
permits:
  net: { http: ["api.example.com"] }
  tools: ["nika:fetch"]
inputs:
  host: { type: string, required: false, default: "evil.example.com" }
tasks:
  grab:
    invoke:
      tool: nika:fetch
      args: { url: "https://${{ inputs.host }}/data" }
"#;
        let taints = taints_of(y);
        assert_eq!(taints.len(), 1, "{taints:?}");
        assert!(
            taints[0]
                .detail
                .contains("inputs.host -> args.url (nika:fetch)")
                && taints[0]
                    .detail
                    .contains("canonical \"evil.example.com\" ∉ net.http"),
            "the canonical host + the escaped bound: {}",
            taints[0].detail
        );
    }

    #[test]
    fn fixture_010_interpolated_bound_refuses_law1_and_the_regate_defers_nothing() {
        // The oracle's exact behavior on fixture 010: law 1 fires on the
        // bound AND law 2 fires on the task — the interpolated bound is
        // never something to match against, so the (default-resolved)
        // host escapes the EMPTY literal boundary.
        let y = r#"nika: t
permits:
  net: { http: ["${{ inputs.host }}"] }
  tools: ["nika:fetch"]
inputs:
  host: { type: string, required: false, default: "api.example.com" }
tasks:
  grab:
    invoke:
      tool: nika:fetch
      args: { url: "https://${{ inputs.host }}/data" }
"#;
        let kinds: Vec<PermitTaintKind> = taints_of(y).iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                PermitTaintKind::BoundInterpolated,
                PermitTaintKind::ArgEscapes
            ],
            "law 1 then law 2, oracle-exact: {kinds:?}"
        );
    }

    #[test]
    fn fixture_011_declassify_declared_opens_the_door() {
        let y = r#"nika: t
permits:
  fs: { read: ["datasets/**", "vendor/**"] }
  tools: ["nika:read"]
inputs:
  p: { type: string, required: false, default: "vendor/q3.csv" }
tasks:
  load:
    invoke:
      tool: nika:read
      args: { path: "${{ inputs.p }}" }
    lift:
      - law: taint
        from: inputs.p
        because: "vendor inventory path, deployment-controlled, reviewed at release time"
"#;
        assert!(
            taints_of(y).is_empty(),
            "the declared door lifts the taint law: {:?}",
            taints_of(y)
        );
        // …and per the oracle the declassified binding DEFERS the static
        // re-gate outright — even an escaping default: law 6's literal
        // match is the runtime boundary's (the door is receipt-recorded,
        // never a silent bypass).
        let escaping = y.replace(
            "default: \"vendor/q3.csv\"",
            "default: \"../../etc/passwd\"",
        );
        assert!(
            taints_of(&escaping).is_empty(),
            "oracle parity: a declassified binding defers statically: {:?}",
            taints_of(&escaping)
        );
    }

    #[test]
    fn fixture_012_trusted_value_passes() {
        // `const.p` is author-baked (Integ=trusted): the static match
        // against the boundary succeeds, no re-gate needed.
        let y = r#"nika: t
permits:
  fs: { read: ["datasets/**"] }
  tools: ["nika:read"]
const:
  p: "datasets/q3.csv"
tasks:
  load:
    invoke:
      tool: nika:read
      args: { path: "${{ const.p }}" }
"#;
        assert!(taints_of(y).is_empty(), "{:?}", taints_of(y));
    }

    #[test]
    fn fixture_013_canonical_form_back_inside_boundary_passes() {
        // The RAW string spells `..` yet canonicalizes INSIDE the bound —
        // the canonicalize-first pivot a prefix matcher false-positives on.
        let y = r#"nika: t
permits:
  fs: { read: ["datasets/**"] }
  tools: ["nika:read"]
inputs:
  p: { type: string, required: false, default: "datasets/../datasets/q3.csv" }
tasks:
  load:
    invoke:
      tool: nika:read
      args: { path: "${{ inputs.p }}" }
"#;
        assert!(taints_of(y).is_empty(), "{:?}", taints_of(y));
    }

    #[test]
    fn law4_the_unresolvable_defers_never_a_check_error() {
        // No default → caller-supplied at launch: the file stays valid,
        // the runtime re-gate is mandatory (NIKA-SEC-004).
        let y = r#"nika: t
permits:
  fs: { read: ["datasets/**"] }
  tools: ["nika:read"]
inputs:
  p: { type: string }
tasks:
  load:
    invoke:
      tool: nika:read
      args: { path: "${{ inputs.p }}" }
"#;
        assert!(taints_of(y).is_empty(), "{:?}", taints_of(y));
        // …and a with:/tasks.* derivation defers too (the runtime re-gate
        // owns the chain — the static twin resolves the direct roots only).
        let chained = r#"nika: t
permits:
  fs: { read: ["datasets/**"] }
  tools: ["nika:read"]
inputs:
  p: { type: string, required: false, default: "../../etc/passwd" }
tasks:
  load:
    with: { q: "${{ inputs.p }}" }
    invoke:
      tool: nika:read
      args: { path: "${{ with.q }}" }
"#;
        assert!(taints_of(chained).is_empty(), "{:?}", taints_of(chained));
    }

    #[test]
    fn input_defaults_are_an_untrusted_root_too() {
        // NEP-0004's Integ table: a deployment-supplied value comes from
        // outside the file — the file cannot vouch for what it does not
        // contain. It rides `inputs:` now; `config:` is gone.
        let y = r#"nika: t
permits:
  fs: { read: ["datasets/**"] }
  tools: ["nika:read"]
inputs:
  p: { type: string, required: false, default: "../../etc/passwd" }
tasks:
  load:
    invoke:
      tool: nika:read
      args: { path: "${{ inputs.p }}" }
"#;
        let taints = taints_of(y);
        assert_eq!(taints.len(), 1, "{taints:?}");
        assert!(
            taints[0].detail.contains("inputs.p -> args.path"),
            "{}",
            taints[0].detail
        );
    }

    #[test]
    fn exec_argv0_data_is_regated_against_the_allowlist() {
        let y = r#"nika: t
permits:
  exec: ["find"]
inputs:
  bin: { type: string, required: false, default: "rm" }
tasks:
  t:
    exec: { command: ["${{ inputs.bin }}", "."] }
"#;
        let taints = taints_of(y);
        assert_eq!(taints.len(), 1, "{taints:?}");
        assert!(
            taints[0].detail.contains("inputs.bin -> argv[0] (exec)"),
            "{}",
            taints[0].detail
        );
        // an allowlisted resolution runs.
        let ok = y.replace("default: \"rm\"", "default: \"find\"");
        assert!(taints_of(&ok).is_empty(), "{:?}", taints_of(&ok));
    }

    /// The emitted⊆registered ratchet, permit-taint tier (the
    /// `composition.rs` pattern): every wire code this lane stamps must
    /// exist in the vendored canon registry — an unregistered refusal
    /// 404s the `docs_url` every finding carries.
    #[test]
    fn every_permit_taint_code_is_registered_in_the_canon() {
        let registered: std::collections::BTreeSet<String> = nika_pack::error_codes()
            .into_iter()
            .map(|row| row.code.to_string())
            .collect();
        for code in [BOUND_CODE, REGATE_CODE, NET_WILDCARD_CODE] {
            assert!(
                registered.contains(code),
                "`{code}` is not in the canon registry (spec/05-errors.md SSOT)"
            );
        }
    }
}

#[cfg(test)]
mod net_arg_tests {
    use super::net_url_arg;

    /// Every net builtin's URL arg comes from the authoritative table.
    /// `notify` was in the old flat list and inert, because the list
    /// knew the tool but not that its arg is `target` — a tainted
    /// `target` escaping the declared hosts passed check with rc=0
    /// while the same shape on `fetch` was refused (2026-08-02).
    #[test]
    fn each_net_builtin_names_its_own_url_arg() {
        let webhook = serde_json::json!({ "channel": "webhook", "target": "https://x.test" });
        let spanned = nika_schema::Spanned::new(webhook, nika_schema::Span::default());
        assert_eq!(net_url_arg("nika:fetch", None), Some("url"));
        assert_eq!(net_url_arg("nika:notify", Some(&spanned)), Some("target"));
        // Absent channel rides the webhook default — the same contract
        // `nika-cap` encodes, which a flat list could not express.
        let bare = serde_json::json!({ "target": "https://x.test" });
        let bare = nika_schema::Spanned::new(bare, nika_schema::Span::default());
        assert_eq!(net_url_arg("nika:notify", Some(&bare)), Some("target"));
    }

    /// An INTERPOLATED channel keeps the target under the re-gate.
    ///
    /// `builtin_effect` classifies notify as net only on a literal
    /// `webhook`, so a templated channel made the whole tool
    /// unclassifiable and the re-gate went silent: the same payload
    /// passed check rc=0 with `channel: "${{ inputs.ch }}"` and rc=2
    /// with `channel: webhook` (2026-08-02).
    #[test]
    fn an_interpolated_channel_still_re_gates_the_target() {
        let templated = serde_json::json!({
            "channel": "${{ inputs.ch }}",
            "target": "${{ inputs.t }}",
        });
        let spanned = nika_schema::Spanned::new(templated, nika_schema::Span::default());
        assert_eq!(net_url_arg("nika:notify", Some(&spanned)), Some("target"));
    }

    /// And re-gating an unknown channel cannot invent a finding: a
    /// non-webhook target is not a URL, so the host extraction yields
    /// nothing and the branch never speaks. Pinned at the seam that
    /// makes that true.
    #[test]
    fn a_non_url_target_has_no_host_to_judge() {
        assert_eq!(super::url_host("#general"), None);
        assert_eq!(super::url_host("ops@example.com"), None);
        assert_eq!(
            super::url_host("https://evil.example.com/hook").as_deref(),
            Some("evil.example.com")
        );
    }

    /// A non-net tool has no URL arg — the re-gate must not invent one.
    #[test]
    fn a_non_net_tool_has_no_url_arg() {
        for tool in ["nika:read", "nika:write", "nika:jq", "nika:decide", "mcp:x"] {
            assert_eq!(net_url_arg(tool, None), None, "{tool} is not a net egress");
        }
    }
}
