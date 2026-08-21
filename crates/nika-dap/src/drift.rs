// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Declared-vs-used drift — the `NIKA-DRIFT-001` advisory family.
//!
//! A workflow's envelope DECLARES its contract (`inputs:` · `const:` ·
//! `secrets:` · `permits:`); the body USES it. When the two drift apart
//! the file lies about itself: a declared input nothing reads is smell
//! (the author pruned the body but not the contract). This pass names
//! the dead declaration and suggests the removal. Advisory — a dead
//! declaration breaks nothing (`is_clean` ignores hints; a green exit
//! stays green).
//!
//! ## Why the pass lives HERE (the second move)
//!
//! The natural home was `nika_check::check`, but that crate sits AT
//! the 15,000-LOC cap (vector 24 · zero headroom), so #661 rendered
//! from the check verb — and the SAME wall then pushed the COMPUTE
//! out of `nika-cli` (2026-07-21 · the trust-plane descent: compute
//! descends, render stays). The check verb now renders this scan like
//! the `[inputs]` hint families, unchanged: the terminal HINT block
//! prints `[NIKA-DRIFT-001 · drift]` rows (the PERMITS bracket
//! pattern) and `--json` appends `{kind, task, advice, code}` rows to
//! `hints[]`.
//!
//! ## The boundary vs the HARD diagnostics (one voice, no double report)
//!
//! The reverse direction — USED but UNDECLARED — is already a hard
//! failure everywhere the check surface can see it:
//!
//! - `${{ inputs./const./secrets./with.X }}` with no `X` declared →
//!   `NIKA-VAR-001` (the analyzer's template scan, EVERY island surface) ·
//!   the dead `vars`/`env` reads refuse `NIKA-VALUES-001`/`NIKA-VALUES-002`
//!   and a foreign root `NIKA-VALUES-003` (the C2 family is closed);
//! - `tasks.X` with no task `X` → `NIKA-DAG-002`;
//! - an effect outside the declared `permits:` → `NIKA-SEC-004`
//!   (fires on the CATEGORY regardless of arg dynamism);
//! - a literal URL the always-on SSRF floor refuses → `NIKA-SEC-005`.
//!
//! Nothing is left for a used-but-undeclared hint, so the family mints
//! exactly ONE wire code — `NIKA-DRIFT-001`, declared-but-unused — and
//! no `NIKA-DRIFT-002` (an unemittable code would be dead weight). The
//! no-duplication law is STRUCTURAL: the hard codes name REFERENCES,
//! this pass names DECLARATIONS — the two never fire on the same yaml
//! site. The honest adjacency is the typo pair: `inputs.topik` declared +
//! `${{ inputs.topic }}` used fires VAR-001 on the reference AND DRIFT-001
//! on the declaration — both true; the hint clears when the typo is
//! fixed.
//!
//! ## What counts as a use
//!
//! The SAME surface set the analyzer scans — the drift voice can never
//! contradict the conformance voice. Permits usage is judged only where
//! PROVABLE: per-entry flags need a COMPLETE static used set; a dynamic
//! consumer poisons the set to `None` (no claim, never wrong — a
//! shell-form exec hides its programs · a dynamic URL/path hides the
//! host/path set · an `agent:` whitelist dispatches dynamically, glob
//! ⊆ glob being undecidable). A templated tool name needs no
//! suppression case: the parser refuses it (the closed `nika:`/`mcp:`
//! grammar).
//!
//! The fs models, named so the hint never orders a load-bearing removal:
//! an `exec:` child poisons BOTH path sets (its fs reach is opaque to
//! statics — the sandbox owns it at run · the 2026-07-29 qr-lanes
//! finding: the hint told authors to delete the grants the run needed);
//! `nika:glob`'s walk root IS an fs read (the runtime gate fences that
//! directory — `builtin_effect` deliberately declines the glob ⊆ glob
//! question, but the ROOT is decidable); a `nika:fetch` `multipart:`
//! file part's `path` is an fs read (the defs contract gates it).
//!
//! The net model (the D1 parity, 2026-07-29 run 5): an argv-form exec's
//! LITERAL `http(s)://` tokens ARE net uses — the checker's exec net-fit
//! refuses the same tokens when undeclared, so this pass must never name
//! the corresponding grant dead (the F15 two-lanes contradiction class);
//! a shell-form exec — or a dynamic/unparseable token — poisons the host
//! set (no provable completeness · no claim, never wrong).

use std::collections::BTreeSet;

use nika_cap::BuiltinEffect;
use nika_schema::expression::{NamespaceRef, expr_refs, scan_templates};
use nika_schema::raw::{
    ForEachValue, RawAction, RawCommand, RawInvokeAction, RawWorkflow, VisionInput,
};
use nika_schema::types::{ExecPermit, FsPermits, OnErrorAction, Permits, WhenGate};

/// The family's wire code — registered in the canon (`nika explain` teaches it).
pub const DRIFT_CODE: &str = "NIKA-DRIFT-001";

/// The declared-but-unused advice rows for a workflow, sorted. Every row is
/// the same class (`drift` · [`DRIFT_CODE`] · workflow-level), so both
/// projections shape the constant fields around the advice texts.
#[must_use]
pub fn scan(wf: &RawWorkflow) -> Vec<String> {
    let mut out = Vec::new();
    push_unused_declarations(wf, &UsedNames::collect(wf), &mut out);
    push_unused_permits(wf, &mut out);
    out.sort();
    out
}

/// Every `inputs./const./secrets.` name the body references (module-doc parity set).
#[derive(Default)]
struct UsedNames {
    inputs: BTreeSet<String>,
    consts: BTreeSet<String>,
    secrets: BTreeSet<String>,
}

impl UsedNames {
    fn collect(wf: &RawWorkflow) -> Self {
        let mut used = Self::default();
        for task in &wf.tasks {
            let t = &task.value;
            used.eat_gate(t.when.as_ref());
            if let Some(for_each) = &t.for_each {
                match &for_each.value {
                    ForEachValue::Expression(expr) => used.eat(expr),
                    ForEachValue::List(list) => used.eat_json(list),
                    _ => {} // a future for_each form joins deliberately (#[non_exhaustive])
                }
            }
            for (_, v) in &t.with {
                used.eat_json(&v.value);
            }
            used.eat_action(&t.action);
            if let Some(on_error) = &t.on_error
                && let OnErrorAction::Recover(value) = &on_error.value.action
            {
                used.eat_json(&value.value);
            }
        }
        for (_, decl) in &wf.outputs {
            used.eat(&decl.value().value);
        }
        if let Some(model) = &wf.model {
            used.eat(&model.value); // the one templated envelope field
        }
        used
    }

    fn eat_gate(&mut self, gate: Option<&nika_schema::source::Spanned<WhenGate>>) {
        if let Some(when) = gate
            && let WhenGate::Expr(expr) = &when.value
        {
            self.eat(expr);
        }
    }

    /// Every namespace ref of one `${{ }}`-bearing text (a broken island yields nothing).
    fn eat(&mut self, text: &str) {
        let Ok(islands) = scan_templates(text) else {
            return;
        };
        for island in &islands {
            for r in expr_refs(&island.expr) {
                match r {
                    NamespaceRef::Inputs(name) => {
                        self.inputs.insert(name);
                    }
                    NamespaceRef::Const(name) => {
                        self.consts.insert(name);
                    }
                    NamespaceRef::Secrets(name) => {
                        self.secrets.insert(name);
                    }
                    _ => {}
                }
            }
        }
    }

    fn eat_json(&mut self, value: &serde_json::Value) {
        match value {
            serde_json::Value::String(s) => self.eat(s),
            serde_json::Value::Array(items) => items.iter().for_each(|i| self.eat_json(i)),
            serde_json::Value::Object(map) => map.values().for_each(|v| self.eat_json(v)),
            _ => {}
        }
    }

    /// The verb-body surfaces, mirroring the analyzer's action scan. The
    /// wildcards are the forward-compat law (render.rs's `verb_of`
    /// precedent): a future verb with templated fields must extend this
    /// walker deliberately — a silent under-read would flag USED names.
    fn eat_action(&mut self, action: &RawAction) {
        match action {
            RawAction::Infer(a) => {
                self.eat(&a.prompt.value);
                for s in [&a.system, &a.model].into_iter().flatten() {
                    self.eat(&s.value);
                }
                for vision in &a.vision {
                    match &vision.value {
                        VisionInput::File { path } => self.eat(&path.value),
                        VisionInput::Url { url } => self.eat(&url.value),
                        _ => {}
                    }
                }
            }
            RawAction::Exec(a) => {
                match &a.command {
                    RawCommand::Shell(c) => self.eat(&c.value),
                    RawCommand::Argv(parts) => parts.iter().for_each(|p| self.eat(&p.value)),
                    _ => {}
                }
                for s in [&a.cwd, &a.stdin].into_iter().flatten() {
                    self.eat(&s.value);
                }
                for (_, v) in &a.env {
                    self.eat(&v.value);
                }
            }
            RawAction::Invoke(a) => {
                if let Some(tool) = a.tool() {
                    self.eat(&tool.value);
                }
                if let Some(args) = &a.args {
                    self.eat_json(&args.value);
                }
            }
            RawAction::Agent(a) => {
                self.eat(&a.prompt.value);
                for s in [&a.system, &a.model].into_iter().flatten() {
                    self.eat(&s.value);
                }
            }
            _ => {}
        }
    }
}

/// Declared `inputs:`/`const:`/`secrets:` names nothing references.
fn push_unused_declarations(wf: &RawWorkflow, used: &UsedNames, out: &mut Vec<String>) {
    let declared: [(&str, Vec<&str>, &BTreeSet<String>, &str); 3] = [
        (
            "inputs",
            wf.inputs.iter().map(|(n, _)| n.value.as_str()).collect(),
            &used.inputs,
            "a dead input drifts from the body it describes",
        ),
        (
            "const",
            wf.consts.iter().map(|(n, _)| n.value.as_str()).collect(),
            &used.consts,
            "a dead constant drifts from the body it describes",
        ),
        (
            "secrets",
            wf.secrets.iter().map(|(n, _)| n.value.as_str()).collect(),
            &used.secrets,
            "an unread store entry is attack surface without a purpose",
        ),
    ];
    for (ns, names, seen, why) in &declared {
        for name in names {
            if !seen.contains(*name) {
                out.push(format!(
                    "`{ns}.{name}` is declared but never referenced — remove the \
                     declaration (or reference it); {why}"
                ));
            }
        }
    }
}

/// The body's statically-known effect usage. A set is `None` when a
/// dynamic consumer makes it INCOMPLETE: the category stays silent (no
/// claim, never wrong).
#[derive(Default)]
struct BodyUsage {
    /// An exec task exists (main verb or `on_finally`).
    has_exec: bool,
    /// An `agent:` whitelist exists (dynamic dispatch — suppresses tools/net/fs).
    has_agent_whitelist: bool,
    /// Static `argv[0]`s of argv-form execs.
    programs: Option<BTreeSet<String>>,
    /// Static invoke tool names — complete by construction (the parser
    /// refuses templated tool references), hence no `Option`.
    tools: BTreeSet<String>,
    /// Literal hosts from net-effect invokes.
    hosts: Option<BTreeSet<String>>,
    /// Literal fs read paths (`nika:glob`'s walk root · fetch `multipart:`
    /// file parts included) · `None` when a dynamic consumer — ANY `exec:`
    /// task included — hides the set.
    reads: Option<BTreeSet<String>>,
    /// Literal fs write paths (incl. `nika:chart`'s `vega_lite` sibling) ·
    /// poisoned alongside `reads`.
    writes: Option<BTreeSet<String>>,
}

impl BodyUsage {
    fn collect(wf: &RawWorkflow) -> Self {
        let mut u = Self {
            programs: Some(BTreeSet::new()),
            hosts: Some(BTreeSet::new()),
            reads: Some(BTreeSet::new()),
            writes: Some(BTreeSet::new()),
            ..Self::default()
        };
        for task in &wf.tasks {
            u.eat_action(&task.value.action);
        }
        u
    }

    fn eat_action(&mut self, action: &RawAction) {
        match action {
            RawAction::Exec(a) => {
                self.has_exec = true;
                offer(
                    &mut self.programs,
                    static_program(&a.command).map(str::to_owned),
                );
                // An argv's LITERAL URL tokens reach hosts exactly like an
                // invoke's — the checker's exec net-fit judges them (D1 ·
                // run 5, 2026-07-29): this pass must count the same hosts
                // or it orders the removal of the grant the fit lane just
                // required (the F15 two-lanes contradiction class). A
                // SHELL line stays opaque here (no provable completeness —
                // the same law as `programs` above): it poisons the set.
                self.eat_exec_urls(&a.command);
                // An exec child's fs reach is OPAQUE to statics (argv
                // literals carry no path semantics — `qrsmart --db X` is
                // undecidable; the OS sandbox owns the reach at run). The
                // drift pass claims nothing rather than ordering the
                // removal of grants the run requires (the qr-lanes class).
                self.reads = None;
                self.writes = None;
            }
            RawAction::Invoke(a) => {
                if let Some(tool) = a.tool() {
                    self.tools.insert(tool.value.clone());
                }
                match builtin_effect_of(a) {
                    Some(BuiltinEffect::Net { url_arg }) => {
                        offer(
                            &mut self.hosts,
                            literal_arg(a, url_arg).as_deref().and_then(url_host),
                        );
                    }
                    Some(BuiltinEffect::Fs {
                        path_arg,
                        reads,
                        writes,
                        walk_root,
                        ..
                    }) => {
                        // A glob offers its WALK ROOT · the ONE definition
                        // lives in nika-cap beside the effect table, so this
                        // scan, the checker, the inference and the runtime
                        // cannot disagree (the hand-synced second copy that
                        // used to live here is gone · 2026-08-19).
                        let path = literal_arg(a, path_arg).map(|raw| {
                            if walk_root {
                                nika_cap::glob_walk_root(&raw)
                            } else {
                                raw
                            }
                        });
                        if reads {
                            offer(&mut self.reads, path.clone());
                        }
                        if writes {
                            offer(&mut self.writes, path.clone());
                        }
                        // chart's `compile_to: vega_lite` writes a second
                        // literal file beside `out:` — `None` there is NOT
                        // a dynamic path (no poisoning).
                        if let Some(sibling) = chart_vl_sibling_of(a)
                            && let Some(set) = &mut self.writes
                        {
                            set.insert(sibling);
                        }
                    }
                    None => {}
                }
                self.eat_multipart_parts(a);
            }
            RawAction::Agent(a) => {
                if !a.tools.is_empty() {
                    self.has_agent_whitelist = true;
                }
            }
            // Infer is effect-free; a future verb joins deliberately (the
            // forward-compat wildcard, same law as the name walker).
            _ => {}
        }
    }

    /// The exec arm of the net usage (the D1 parity): an argv's literal
    /// `http(s)://` tokens contribute their hosts, so a `net.http` grant
    /// the fit lane required is never named dead here. A token whose host
    /// is dynamic or unparseable poisons the set (`offer(None)` — no
    /// claim, never wrong), as does a leading `${{ }}` that can expand to
    /// a whole URL; a shell line poisons outright (no provable
    /// completeness — the runtime sandbox owns it, the fit lane's own
    /// conservative-read law in the other direction).
    fn eat_exec_urls(&mut self, command: &RawCommand) {
        let RawCommand::Argv(parts) = command else {
            self.hosts = None;
            return;
        };
        for tok in parts.iter().map(|p| p.value.as_str()) {
            if tok.starts_with("http://") || tok.starts_with("https://") {
                offer(&mut self.hosts, url_host(tok));
            } else if tok.starts_with("${{") {
                offer(&mut self.hosts, None);
            }
        }
    }

    /// A `nika:fetch` `multipart:` file part's `path` is fs.read-gated at
    /// run (the defs contract: "path is permits.fs.read-gated") — each
    /// literal part path is a read; a `${{ }}` one poisons the set; a text
    /// part (`{name, value}`) touches no fs.
    fn eat_multipart_parts(&mut self, a: &RawInvokeAction) {
        let Some(parts) = a
            .args
            .as_ref()
            .and_then(|s| s.value.get("multipart"))
            .and_then(serde_json::Value::as_array)
        else {
            return;
        };
        for part in parts {
            match part.get("path").and_then(serde_json::Value::as_str) {
                Some(p) if !p.contains("${{") => offer(&mut self.reads, Some(p.to_owned())),
                Some(_) => offer(&mut self.reads, None),
                None => {}
            }
        }
    }
}

/// Declared `permits:` entries no task effect can use — per category,
/// per entry where the used set is fully static.
fn push_unused_permits(wf: &RawWorkflow, out: &mut Vec<String>) {
    let Some(permits) = wf.permits.as_ref().map(|p| &p.value) else {
        return;
    };
    let usage = BodyUsage::collect(wf);
    push_exec_drift(permits, &usage, out);
    push_tools_drift(permits, &usage, out);
    push_net_drift(permits, &usage, out);
    push_fs_drift(permits, &usage, out);
}

/// `exec:` — `true` is used by ANY exec task; a program allowlist is
/// judged per entry. `exec: false` is a posture, not a grant — never drift.
fn push_exec_drift(permits: &Permits, usage: &BodyUsage, out: &mut Vec<String>) {
    match permits.exec.as_ref() {
        Some(ExecPermit::Any) => {
            if !usage.has_exec {
                out.push(
                    "`permits.exec: true` grants shell reach no task uses — remove it \
                     (no `exec:` task exists)"
                        .to_owned(),
                );
            }
        }
        Some(ExecPermit::Programs(list)) => {
            // `None` = a shell-form/dynamic exec hides its programs —
            // per-entry is undecidable then. (No exec task at all keeps
            // the set complete-and-empty: every entry is provably dead.)
            let Some(used) = &usage.programs else {
                return;
            };
            for program in list {
                if !used.contains(program) {
                    out.push(format!(
                        "`permits.exec` entry `{program}` admits a program no task runs \
                         — remove the entry"
                    ));
                }
            }
        }
        _ => {}
    }
}

/// `tools:` — per entry, through the ONE glob matcher (`nika_cap`).
/// Silent when an agent whitelist dispatches dynamically.
fn push_tools_drift(permits: &Permits, usage: &BodyUsage, out: &mut Vec<String>) {
    let Some(globs) = permits.tools.as_ref() else {
        return;
    };
    if usage.has_agent_whitelist {
        return;
    }
    for glob in globs {
        if !usage.tools.iter().any(|t| nika_cap::glob_matches(glob, t)) {
            out.push(format!(
                "`permits.tools` entry `{glob}` admits no tool the body invokes \
                 — remove the entry"
            ));
        }
    }
}

/// `net.http:` — per entry through the ONE host matcher (the boundary's
/// own). Floor-dead entries are skipped: the `NIKA-SEC-005` escape
/// already names them (one voice, never twice).
fn push_net_drift(permits: &Permits, usage: &BodyUsage, out: &mut Vec<String>) {
    let Some(net) = permits.net.as_ref() else {
        return;
    };
    if usage.has_agent_whitelist {
        return;
    }
    let Some(hosts) = &usage.hosts else {
        return; // a dynamic URL hides the host set — no claim
    };
    for entry in &net.http {
        let floor_dead = !entry.contains('*')
            && nika_types::net::host_is_blocked(entry)
            && !nika_types::net::is_exact_loopback_literal(entry);
        if floor_dead {
            continue;
        }
        if !hosts
            .iter()
            .any(|h| nika_types::net::host_glob_matches(entry, h))
        {
            out.push(format!(
                "`permits.net.http` entry `{entry}` matches no host the body reaches \
                 — remove the entry"
            ));
        }
    }
}

/// `fs.read`/`fs.write` — per entry per direction (`Permits::allows_path` over a single-entry boundary).
fn push_fs_drift(permits: &Permits, usage: &BodyUsage, out: &mut Vec<String>) {
    let Some(fs) = permits.fs.as_ref() else {
        return;
    };
    if usage.has_agent_whitelist {
        return;
    }
    let (Some(reads), Some(writes)) = (&usage.reads, &usage.writes) else {
        return; // a dynamic path hides the path sets — no claim
    };
    for entry in &fs.read {
        if !reads.iter().any(|p| single_entry_admits(entry, p, false)) {
            out.push(format!(
                "`permits.fs.read` entry `{entry}` matches no path the body reads \
                 — remove the entry"
            ));
        }
    }
    for entry in &fs.write {
        if !writes.iter().any(|p| single_entry_admits(entry, p, true)) {
            out.push(format!(
                "`permits.fs.write` entry `{entry}` matches no path the body writes \
                 — remove the entry"
            ));
        }
    }
}

/// Does ONE declared fs entry admit this literal path? (`Permits::allows_path` on a synthetic boundary.)
fn single_entry_admits(entry: &str, path: &str, write: bool) -> bool {
    let mut boundary = Permits::new();
    boundary.fs = Some(if write {
        FsPermits::new(Vec::new(), vec![entry.to_owned()])
    } else {
        FsPermits::new(vec![entry.to_owned()], Vec::new())
    });
    boundary.allows_path(path, write)
}

/// Feed one literal into a used set · `None` (dynamic) poisons it.
fn offer(set: &mut Option<BTreeSet<String>>, value: Option<String>) {
    match value {
        Some(v) => {
            if let Some(s) = set {
                s.insert(v);
            }
        }
        None => *set = None,
    }
}

/// The invoke's builtin effect classification — the `nika_cap` oracle.
fn builtin_effect_of(a: &RawInvokeAction) -> Option<BuiltinEffect> {
    let tool = a.tool()?;
    nika_cap::builtin_effect(&tool.value, a.args.as_ref().map(|s| &s.value))
}

/// The invoke's `compile_to: vega_lite` sibling file (the checker fit
/// pass's own law) — `None` for other tools/targets, never a dynamic read.
fn chart_vl_sibling_of(a: &RawInvokeAction) -> Option<String> {
    let tool = a.tool()?;
    nika_cap::chart_vl_sibling(&tool.value, a.args.as_ref().map(|s| &s.value))
}

/// An arg as a STATIC string — `None` when absent, non-string, or
/// `${{ }}`-built (dynamic → the runtime's concern, never a guess).
fn literal_arg(a: &RawInvokeAction, key: &str) -> Option<String> {
    let s = a.args.as_ref()?.value.get(key)?.as_str()?;
    if s.contains("${{") {
        return None;
    }
    Some(s.to_owned())
}

/// The statically-known program of an ARRAY-form command (`argv[0]`
/// when literal) · `None` for the shell-string form.
fn static_program(command: &RawCommand) -> Option<&str> {
    match command {
        RawCommand::Argv(_) => command.argv_program().filter(|p| !p.contains("${{")),
        _ => None,
    }
}

/// The host of a literal URL, via the `url` crate — the SAME WHATWG
/// normalization the checker's `url_host` enforces with (a hand-rolled
/// parser is a boundary bypass); duplicated only because the checker's
/// copy sits behind the capped crate's private module (module doc).
fn url_host(raw: &str) -> Option<String> {
    match url::Url::parse(raw).ok()?.host()? {
        url::Host::Domain(d) => Some(d.trim_end_matches('.').to_owned()),
        url::Host::Ipv4(a) => Some(a.to_string()),
        url::Host::Ipv6(a) => Some(a.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::{FileId, ParseMode, parse};

    fn drifted(yaml: &str) -> Vec<String> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        scan(&wf)
    }

    // ─── namespace declarations (vars · env · secrets) ───────────────

    #[test]
    fn unused_var_env_and_secret_are_hinted() {
        let advice = drifted(
            "nika: w\nconst:\n  topic: \"x\"\ninputs:\n  REGION: { type: string, required: false, default: \"eu\" }\nsecrets:\n  k: { source: env, key: K }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        assert_eq!(advice.len(), 3, "{advice:?}");
        assert!(
            advice.iter().any(|a| a.contains("`const.topic`")),
            "{advice:?}"
        );
        assert!(
            advice.iter().any(|a| a.contains("`inputs.REGION`")),
            "{advice:?}"
        );
        assert!(
            advice.iter().any(|a| a.contains("`secrets.k`")),
            "{advice:?}"
        );
    }

    #[test]
    fn used_everything_stays_silent() {
        // every declaration referenced — across DIFFERENT surfaces so
        // each walker arm is pinned: prompt · exec env · with · outputs ·
        // the envelope model: · on_error.recover · when:.
        let advice = drifted(
            "nika: w\nmodel: \"${{ inputs.backend }}\"\ninputs:\n  backend: { type: string, required: true }\n  gate: { type: string, required: true }\n  recovered: { type: string, required: true }\n  REGION: { type: string, required: false, default: \"eu\" }\nconst:\n  topic: \"x\"\nsecrets:\n  k: { source: env, key: K }\ntasks:\n  a:\n    when: ${{ inputs.gate == \"yes\" }}\n    with: { token: \"${{ secrets.k }}\" }\n    on_error: { recover: \"${{ inputs.recovered }}\" }\n    exec:\n      command: [\"echo\", \"${{ const.topic }}\"]\n      env: { R: \"${{ inputs.REGION }}\" }\noutputs:\n  out: \"${{ const.topic }}\"\n",
        );
        assert!(advice.is_empty(), "{advice:?}");
    }

    #[test]
    fn hints_are_sorted_and_deterministic() {
        let first = drifted(
            "nika: w\nconst:\n  zeta: \"x\"\n  alpha: \"y\"\nsecrets:\n  k2: { source: env, key: K2 }\n  k1: { source: env, key: K1 }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        let second = drifted(
            "nika: w\nconst:\n  zeta: \"x\"\n  alpha: \"y\"\nsecrets:\n  k2: { source: env, key: K2 }\n  k1: { source: env, key: K1 }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        assert_eq!(first, second, "same input → same order");
        let mut sorted = first.clone();
        sorted.sort();
        assert_eq!(first, sorted, "emitted sorted: {first:?}");
    }

    // ─── the boundary vs the hard diagnostics ────────────────────────

    #[test]
    fn used_but_undeclared_never_produces_a_drift_hint() {
        // `${{ inputs.ghost }}` with NOTHING declared: the conformance
        // ladder owns it (NIKA-VAR-001) — the drift pass stays silent
        // (the two codes must never fire on the same reference).
        let wf = parse(
            "nika: w\ntasks:\n  a:\n    exec: { command: [\"echo\", \"${{ inputs.ghost }}\"] }\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(
            report.conformance.iter().any(|c| c.code == "NIKA-VAR-001"),
            "the hard diagnostic fires: {:?}",
            report.conformance
        );
        assert!(
            scan(&wf).is_empty(),
            "no drift hint duplicates it: {:?}",
            scan(&wf)
        );
    }

    #[test]
    fn the_typo_pair_names_two_different_sites() {
        // `topik` DECLARED · `topic` USED — the hard code names the
        // reference (VAR-001), the drift hint names the declaration
        // (topik): both true, never the same site, and the hint clears
        // when the typo is fixed.
        let wf = parse(
            "nika: w\ninputs:\n  topik: { type: string, required: true }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"${{ inputs.topic }}\"] }\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(
            report
                .conformance
                .iter()
                .any(|c| c.code == "NIKA-VAR-001" && c.message.contains("inputs.topic")),
            "{:?}",
            report.conformance
        );
        let drift = scan(&wf);
        assert_eq!(drift.len(), 1, "{drift:?}");
        assert!(drift[0].contains("`inputs.topik`"), "{drift:?}");
    }

    // ─── permits: exec ───────────────────────────────────────────────

    #[test]
    fn exec_true_without_any_exec_task_is_hinted() {
        let advice = drifted(
            "nika: w\npermits: { exec: true }\ntasks:\n  a:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n",
        );
        assert!(
            advice.iter().any(|a| a.contains("`permits.exec: true`")),
            "{advice:?}"
        );
    }

    #[test]
    fn exec_true_with_an_exec_task_is_silent() {
        let advice = drifted(
            "nika: w\npermits: { exec: true }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        assert!(
            !advice.iter().any(|a| a.contains("permits.exec")),
            "{advice:?}"
        );
    }

    #[test]
    fn exec_false_is_a_posture_never_a_drift() {
        let advice = drifted(
            "nika: w\npermits: { exec: false }\ntasks:\n  a:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n",
        );
        assert!(
            !advice.iter().any(|a| a.contains("permits.exec")),
            "{advice:?}"
        );
    }

    #[test]
    fn exec_program_list_is_judged_per_entry() {
        // `cargo` is used · `make` is not — ONLY `make` is flagged.
        let advice = drifted(
            "nika: w\npermits: { exec: [\"cargo\", \"make\"] }\ntasks:\n  a:\n    exec: { command: [\"cargo\", \"build\"] }\n",
        );
        assert!(
            advice
                .iter()
                .any(|a| a.contains("`permits.exec` entry `make`")),
            "{advice:?}"
        );
        assert!(
            !advice.iter().any(|a| a.contains("entry `cargo`")),
            "{advice:?}"
        );
    }

    #[test]
    fn exec_program_list_with_no_exec_task_flags_every_entry() {
        let advice = drifted(
            "nika: w\npermits: { exec: [\"cargo\", \"make\"] }\ntasks:\n  a:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n",
        );
        let exec_hints: Vec<&String> = advice
            .iter()
            .filter(|a| a.contains("`permits.exec` entry"))
            .collect();
        assert_eq!(exec_hints.len(), 2, "{advice:?}");
    }

    #[test]
    fn shell_form_exec_suppresses_per_entry_program_flags() {
        // a shell command's program set is opaque — no per-entry claim
        // (the form under an allowlist is the hard lane's business).
        let advice = drifted(
            "nika: w\npermits: { exec: [\"cargo\", \"make\"] }\ntasks:\n  a:\n    exec: { shell: \"cargo build && make test\" }\n",
        );
        assert!(
            !advice.iter().any(|a| a.contains("`permits.exec` entry")),
            "{advice:?}"
        );
    }

    // ─── permits: tools ──────────────────────────────────────────────

    #[test]
    fn tools_entry_nothing_invokes_is_hinted() {
        let advice = drifted(
            "nika: w\npermits: { tools: [\"nika:read\", \"mcp:github/*\"] }\ntasks:\n  a:\n    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n",
        );
        assert!(
            advice
                .iter()
                .any(|a| a.contains("`permits.tools` entry `mcp:github/*`")),
            "{advice:?}"
        );
        assert!(
            !advice.iter().any(|a| a.contains("entry `nika:read`")),
            "{advice:?}"
        );
    }

    #[test]
    fn agent_whitelist_suppresses_tools_entry_flags() {
        // the agent dispatches dynamically; a tools entry may exist to
        // cover its whitelist globs — glob ⊆ glob is undecidable.
        let advice = drifted(
            "nika: w\npermits: { tools: [\"mcp:github/*\"] }\ntasks:\n  a:\n    agent: { prompt: \"triage\", tools: [\"mcp:github/*\"], max_tokens_total: 1000, model: \"mock/echo\" }\n",
        );
        assert!(
            !advice.iter().any(|a| a.contains("`permits.tools` entry")),
            "{advice:?}"
        );
    }

    #[test]
    fn templated_tool_names_are_parse_refused_so_no_suppression_case_exists() {
        // The static tool set is complete BY CONSTRUCTION: the closed
        // `nika:`/`mcp:` grammar refuses a templated reference at parse
        // time. This pins WHY the tools/net/fs passes carry no
        // template-suppression arm — if the grammar ever loosens, this
        // test turns red and the suppression must be (re)added.
        let result = parse(
            "nika: w\nconst:\n  t: \"nika:read\"\ntasks:\n  a:\n    invoke: { tool: \"${{ const.t }}\" }\n",
            FileId::new(0),
            ParseMode::Strict,
        );
        assert!(
            result.is_err(),
            "templated tool reference must stay refused"
        );
    }

    // ─── permits: net ────────────────────────────────────────────────

    #[test]
    fn net_entry_matching_no_literal_host_is_hinted() {
        let advice = drifted(
            "nika: w\npermits:\n  net: { http: [\"api.example.com\", \"other.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/x\" } }\n",
        );
        assert!(
            advice
                .iter()
                .any(|a| a.contains("`permits.net.http` entry `other.example.com`")),
            "{advice:?}"
        );
        assert!(
            !advice.iter().any(|a| a.contains("entry `api.example.com`")),
            "{advice:?}"
        );
    }

    #[test]
    fn net_entries_all_unused_when_no_net_effect_exists() {
        let advice = drifted(
            "nika: w\npermits:\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:read\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n",
        );
        assert!(
            advice
                .iter()
                .any(|a| a.contains("`permits.net.http` entry `api.example.com`")),
            "{advice:?}"
        );
    }

    #[test]
    fn dynamic_url_suppresses_net_entry_flags() {
        let advice = drifted(
            "nika: w\nconst:\n  u: \"https://api.example.com/x\"\npermits:\n  net: { http: [\"other.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"${{ const.u }}\" } }\n",
        );
        assert!(
            !advice
                .iter()
                .any(|a| a.contains("`permits.net.http` entry")),
            "{advice:?}"
        );
    }

    #[test]
    fn an_exec_argv_url_counts_as_a_net_use() {
        // The D1 parity (run 5, 2026-07-29): the checker's exec net-fit
        // REFUSES this exact token undeclared — the drift pass must never
        // name the declared grant dead for the same token (F15: two
        // lanes, one judgment).
        let advice = drifted(
            "nika: w\npermits:\n  exec: [\"curl\"]\n  net: { http: [\"acme.test\"] }\ntasks:\n  crawl:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n",
        );
        assert!(
            !advice
                .iter()
                .any(|a| a.contains("`permits.net.http` entry")),
            "the exec argv URL is a use: {advice:?}"
        );
    }

    #[test]
    fn a_shell_exec_or_dynamic_token_suppresses_net_entry_flags() {
        // No provable completeness: the shell form — and a leading
        // template that can expand to a whole URL — poison the host set
        // rather than order a load-bearing removal.
        let shell = drifted(
            "nika: w\npermits: { exec: true, net: { http: [\"acme.test\"] } }\ntasks:\n  crawl:\n    exec: { shell: \"curl https://acme.test\" }\n",
        );
        assert!(
            !shell.iter().any(|a| a.contains("`permits.net.http` entry")),
            "a shell line hides the host set: {shell:?}"
        );
        let dynamic = drifted(
            "nika: w\nconst:\n  u: \"https://acme.test\"\npermits:\n  exec: [\"curl\"]\n  net: { http: [\"acme.test\"] }\ntasks:\n  crawl:\n    exec: { command: [\"curl\", \"${{ const.u }}\"] }\n",
        );
        assert!(
            !dynamic
                .iter()
                .any(|a| a.contains("`permits.net.http` entry")),
            "a leading template hides the host set: {dynamic:?}"
        );
    }

    #[test]
    fn floor_dead_net_entry_is_not_drift_hinted() {
        // `10.0.0.8` can never take effect — the NIKA-SEC-005 escape
        // already names it (the hard lane); the drift hint must NOT
        // double-report the same entry.
        let wf = parse(
            "nika: w\npermits:\n  net: { http: [\"10.0.0.8\", \"api.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/x\" } }\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(
            report
                .capability_escapes
                .iter()
                .any(|e| e.detail.contains("10.0.0.8")),
            "the floor-dead escape fires: {:?}",
            report.capability_escapes
        );
        assert!(
            !scan(&wf).iter().any(|a| a.contains("10.0.0.8")),
            "no second report for the same entry: {:?}",
            scan(&wf)
        );
    }

    // ─── permits: fs ─────────────────────────────────────────────────

    #[test]
    fn fs_entries_are_judged_per_direction() {
        // read `./in/**` is used · write `./out/**` is not — ONLY the
        // write entry is flagged (a read use never covers a write grant).
        let advice = drifted(
            "nika: w\npermits:\n  fs: { read: [\"./in/**\"], write: [\"./out/**\"] }\n  tools: [\"nika:read\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:read\", args: { path: \"./in/data.txt\" } }\n",
        );
        assert!(
            advice
                .iter()
                .any(|a| a.contains("`permits.fs.write` entry `./out/**`")),
            "{advice:?}"
        );
        assert!(
            !advice.iter().any(|a| a.contains("`permits.fs.read`")),
            "{advice:?}"
        );
    }

    #[test]
    fn fs_write_use_via_chart_sibling_counts() {
        // `compile_to: vega_lite` writes `out` AND its `.vl.json`
        // sibling — both literal writes (the fit pass's own law).
        let advice = drifted(
            "nika: w\npermits:\n  fs: { write: [\"./chart.svg\", \"./chart.vl.json\"] }\n  tools: [\"nika:chart\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:chart\", args: { spec: { mark: \"line\" }, out: \"./chart.svg\", compile_to: \"vega_lite\" } }\n",
        );
        assert!(
            !advice.iter().any(|a| a.contains("`permits.fs.write`")),
            "{advice:?}"
        );
    }

    #[test]
    fn dynamic_path_suppresses_fs_entry_flags() {
        let advice = drifted(
            "nika: w\nconst:\n  p: \"./in/data.txt\"\npermits:\n  fs: { read: [\"./other/**\"] }\n  tools: [\"nika:read\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:read\", args: { path: \"${{ const.p }}\" } }\n",
        );
        assert!(
            !advice.iter().any(|a| a.contains("`permits.fs.read` entry")),
            "{advice:?}"
        );
    }

    #[test]
    fn no_permits_block_means_no_permits_drift() {
        let advice = drifted("nika: w\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n");
        assert!(advice.is_empty(), "{advice:?}");
    }

    // ─── permits: fs — the opaque-exec + glob + multipart models ─────

    #[test]
    fn an_exec_task_poisons_both_fs_sets() {
        // The qr-lanes class (2026-07-29): grants an external binary needs
        // at run — argv literals carry no path semantics, so the hint must
        // NEVER order their removal.
        let advice = drifted(
            "nika: w\npermits:\n  fs: { read: [\"out/smart/**\"], write: [\"out/smart/**\"] }\n  exec: [\"qrt\"]\ntasks:\n  a:\n    exec: { command: [\"qrt\", \"smart\", \"--db\", \"out/smart/smartlink.db\"] }\n",
        );
        assert!(
            !advice.iter().any(|a| a.contains("`permits.fs.")),
            "{advice:?}"
        );
    }

    #[test]
    fn glob_walk_root_counts_as_a_read() {
        // A literal pattern's walk root IS a read: the `./hiring/**` entry
        // is used (silent), the `./other/**` entry is provably dead (flagged).
        let advice = drifted(
            "nika: w\npermits:\n  fs: { read: [\"./hiring/**\", \"./other/**\"] }\n  tools: [\"nika:glob\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:glob\", args: { pattern: \"./hiring/inbox/*.md\" } }\n",
        );
        assert!(
            !advice
                .iter()
                .any(|a| a.contains("`permits.fs.read` entry `./hiring/**`")),
            "{advice:?}"
        );
        assert!(
            advice
                .iter()
                .any(|a| a.contains("`permits.fs.read` entry `./other/**`")),
            "{advice:?}"
        );
    }

    #[test]
    fn a_bare_directory_grant_covers_a_walk_inside_it() {
        // The fanout shape: `read: ["./items"]` + pattern `./items/*.md` —
        // the walk root IS the granted directory.
        let advice = drifted(
            "nika: w\npermits:\n  fs: { read: [\"./items\"] }\n  tools: [\"nika:glob\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:glob\", args: { pattern: \"./items/*.md\" } }\n",
        );
        assert!(
            !advice.iter().any(|a| a.contains("`permits.fs.read` entry")),
            "{advice:?}"
        );
    }

    #[test]
    fn dynamic_glob_pattern_poisons_the_read_set() {
        let advice = drifted(
            "nika: w\nconst:\n  src: \"./items\"\npermits:\n  fs: { read: [\"./items\"] }\n  tools: [\"nika:glob\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:glob\", args: { pattern: \"${{ const.src }}/*.md\" } }\n",
        );
        assert!(
            !advice.iter().any(|a| a.contains("`permits.fs.read` entry")),
            "{advice:?}"
        );
    }

    #[test]
    fn fetch_multipart_file_parts_count_as_reads() {
        // The file part's `path` is fs.read-gated at run (the defs
        // contract) — the `./data/**` entry is used; the text part is no fs.
        let advice = drifted(
            "nika: w\npermits:\n  fs: { read: [\"./data/**\"] }\n  tools: [\"nika:fetch\"]\n  net: { http: [\"api.example.com\"] }\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/up\", method: \"POST\", multipart: [{ name: \"file\", path: \"./data/report.csv\" }, { name: \"note\", value: \"hi\" }] } }\n",
        );
        assert!(
            !advice.iter().any(|a| a.contains("`permits.fs.read` entry")),
            "{advice:?}"
        );
    }

    #[test]
    fn a_dynamic_multipart_part_poisons_the_read_set() {
        let advice = drifted(
            "nika: w\nconst:\n  p: \"./data/report.csv\"\npermits:\n  fs: { read: [\"./data/**\"] }\n  tools: [\"nika:fetch\"]\n  net: { http: [\"api.example.com\"] }\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/up\", method: \"POST\", multipart: [{ name: \"file\", path: \"${{ const.p }}\" }] } }\n",
        );
        assert!(
            !advice.iter().any(|a| a.contains("`permits.fs.read` entry")),
            "{advice:?}"
        );
    }
}
