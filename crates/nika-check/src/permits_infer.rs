// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Capability inference — synthesize the TIGHTEST `permits:` (ADR-092 #2).
//!
//! Where `permits_fit` *verifies* a declared boundary, this module *infers*
//! one: it walks every task's literal effect signature (main verb AND
//! `on_finally` cleanup verbs — cleanups run under the same boundary) and
//! computes the minimal capability set the workflow actually needs —
//! object-capability calculus over the DAG. `nika check --infer-permits`
//! writes your security boundary FOR you (no competitor does this).
//!
//! Effect classification is [`builtin_effect`] — the SAME table the escape
//! checker reads, so inference and verification cannot drift: the inferred
//! block re-checks with zero escapes (the round-trip property, tested).
//!
//! The inference is **sound-by-honesty**: a dynamic effect (a `${{ }}`-built
//! path/host/program) cannot be pinned statically, and it is NEVER silently
//! dropped. Per category ·
//!
//! - `exec` — a dynamic program WIDENS the permit to `exec: true` (the
//!   category has an expressible "any"), plus a note.
//! - `net`/`fs` — the permit grammar has no "any host"/"any path" form, so
//!   widening is not expressible; the rendered block stays literal-only and
//!   the dynamic effect becomes an actionable [`InferredPermits::notes`]
//!   entry the operator must resolve before running.
//! - `exec` URLs — an argv (or an admissible shell line) carrying a literal
//!   or const-backed URL names its host like an invoke, read from the SAME
//!   tokens the fit lane's exec net arm judges; a computed authority, or an
//!   HTTP client handed only a computed operand, leaves the net face partial
//!   (issue 1597).

use std::collections::BTreeSet;

use super::permits_fit::{
    BuiltinEffect, ConstStrings, builtin_effect, chart_vl_sibling, exec_url_host, exec_url_tokens,
    judgeable_arg, judgeable_operand, judgeable_program, path_escapes_workspace,
    resolve_const_argv, templated_url_host, url_host,
};
use nika_schema::raw::{RawAction, RawCommand, RawExecAction, RawTask, RawWorkflow};
use nika_schema::types::{ExecPermit, FsPermits, NetPermits, Permits};

/// Which faces of the derivation are INCOMPLETE.
///
/// The honesty notes have always said this in prose. A consumer that
/// wants to ACT on it had to sniff those sentences, which is the shape
/// this codebase already rejected once: `UnknownField` splits a typed
/// `suggestion` from a prose `teaching` for exactly this reason, because
/// a repairer that splices prose splices nonsense.
///
/// Half of it was already typed — `exec_dynamic` has been a bool on the
/// collector since the shell-string case. This completes the pattern
/// rather than inventing one.
///
/// Why it matters, measured 2026-08-20: a workflow with ONE static read
/// and ONE computed read derives `needed.fs.read = ["./data/a.txt"]` and
/// a note about the dynamic one. A consumer reading `needed` as the
/// complete answer would offer a boundary that BREAKS the run. With this
/// flag the same consumer stays silent, by type rather than by care.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
#[allow(
    clippy::struct_excessive_bools,
    reason = "a flag RECORD, not a parameter list — the four fields ARE the JSON               shape consumers read (`permits.partial.fs`), and a bitmask would               serialize as an opaque number, which is the very flattening this               type exists to undo"
)]
pub struct PartialFaces {
    /// An `exec` effect could not be pinned to a program allowlist
    /// (a computed argv, or the shell-string form).
    pub exec: bool,
    /// A `net.http` host could not be pinned — computed, or refused by
    /// the always-on SSRF floor so that no entry could admit it.
    pub net: bool,
    /// An `fs` path could not be pinned (a computed path).
    pub fs: bool,
    /// A composed child workflow owns effects this inference never sees
    /// (the composition lane resolves them · spec 14 law 3/4).
    pub composed: bool,
}

impl PartialFaces {
    /// True when ANY face is incomplete. `needed` is then a FLOOR, never
    /// the answer, and no consumer may present it as the tightest
    /// boundary.
    #[must_use]
    pub const fn any(self) -> bool {
        self.exec || self.net || self.fs || self.composed
    }
}

/// The inferred boundary plus the honesty notes (effects too dynamic to
/// pin statically — the operator must review these).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InferredPermits {
    /// The synthesized minimal `permits:` block.
    pub permits: Permits,
    /// Effects that could not be statically pinned (dynamic path/host/
    /// program). `exec` widened to `true` for its case; `net`/`fs` cannot
    /// widen (no "any" form), so these entries are the operator's todo.
    ///
    /// PROSE, for a human. A consumer that needs to decide anything reads
    /// [`partial`](Self::partial) instead.
    pub notes: Vec<String>,
    /// The machine-readable half of `notes` — which faces are incomplete.
    pub partial: PartialFaces,
}

impl InferredPermits {
    /// Render the inferred boundary as a spec-shaped `permits:` YAML block —
    /// the text the operator pastes into the envelope. Always a valid
    /// `permits:` block (it round-trips through the parser); string items
    /// are YAML-escaped.
    #[must_use]
    pub fn to_yaml(&self) -> String {
        render_yaml(&self.permits)
    }
}

/// The mutable inference state one [`collect_action`] call folds into.
#[derive(Default)]
struct Collector {
    /// The `const:` table, so inference resolves a bare
    /// `${{ const.<name> }}` the way the FIT scan does.
    ///
    /// Without it the two halves of one binary disagreed about one file:
    /// `check` called a const-backed path "a literal path" and judged it,
    /// while `--infer-permits` called the same path "too dynamic to pin
    /// statically" and left it out of the draft. So the block this prints
    /// never round-tripped clean on any file routing a path or url through
    /// `const:` — which is every file the templates teach, and the printed
    /// block is the half an author pastes.
    ///
    /// Empty for [`task_permits`], which is handed one task and never the
    /// workflow, so it cannot see a `const:` block. That path keeps its
    /// prior behaviour: a const-backed value stays unresolved there. The
    /// limit is real and named rather than hidden — closing it means giving
    /// that entry point the workflow.
    consts: ConstStrings,
    exec_used: bool,
    exec_dynamic: bool,
    /// The faces whose derivation could not be completed. `exec` keeps
    /// its own field above because the widening decision reads it; the
    /// rest ride here so the collector gains a record, not three flags.
    partial: PartialFaces,
    programs: BTreeSet<String>,
    tools: BTreeSet<String>,
    hosts: BTreeSet<String>,
    reads: BTreeSet<String>,
    writes: BTreeSet<String>,
    notes: Vec<String>,
}

/// Infer the tightest `permits:` for a workflow.
#[must_use]
// `env:` is NEVER inferred (NEP-0005 law 7 · LAW-AUTH-0326): a
// subprocess's environment reads are opaque to static analysis, so the
// inferred block carries no `env` category — the author declares intent,
// and the undeclared-read failure mode is the child tool's own
// missing-variable error (the repair is one `env: [NAME]` line).
pub(super) fn infer(wf: &RawWorkflow) -> InferredPermits {
    let mut c = Collector {
        consts: ConstStrings::of(wf),
        ..Collector::default()
    };
    for task in &wf.tasks {
        let id = &task.value.id.value;
        collect_action(&mut c, id, &task.value.action);
    }

    let exec = if !c.exec_used {
        Some(ExecPermit::No)
    } else if c.exec_dynamic {
        Some(ExecPermit::Any)
    } else {
        Some(ExecPermit::Programs(c.programs.into_iter().collect()))
    };

    let mut permits = Permits::new();
    permits.fs = build_fs(c.reads, c.writes);
    permits.net = build_net(c.hosts);
    permits.exec = exec;
    permits.tools = (!c.tools.is_empty()).then(|| c.tools.into_iter().collect());
    InferredPermits {
        permits,
        notes: c.notes,
        partial: PartialFaces {
            exec: c.exec_dynamic,
            ..c.partial
        },
    }
}

/// ONE task's capability attribution — the graph projector's voice
/// (`graph --format json` node `permits`, the field the projection
/// declared as its contract). The task's effect signature (main action
/// plus its `on_finally` cleanups) flattens to deterministic strings
/// in the fixed family order exec, fs.read, fs.write, net.http, tool
/// (each family sorted — the collector sets are ordered). An effect
/// too dynamic to pin surfaces as the same widened form the boundary
/// inference uses (`exec: true`); dynamic net/fs pin nothing here —
/// the check's escape lane owns that story, a projection never guesses.
#[must_use]
pub(crate) fn task_permits(task: &RawTask) -> Vec<String> {
    let mut c = Collector::default();
    let id = &task.id.value;
    collect_action(&mut c, id, &task.action);
    let mut out = Vec::new();
    if c.exec_used {
        if c.exec_dynamic {
            out.push("exec: true".to_owned());
        } else {
            out.extend(c.programs.into_iter().map(|p| format!("exec: {p}")));
        }
    }
    out.extend(c.reads.into_iter().map(|r| format!("fs.read: {r}")));
    out.extend(c.writes.into_iter().map(|w| format!("fs.write: {w}")));
    out.extend(c.hosts.into_iter().map(|h| format!("net.http: {h}")));
    out.extend(c.tools.into_iter().map(|t| format!("tool: {t}")));
    out
}

/// The read an argv-form `exec:` needs for its own script — `None` when
/// the program is not an interpreter, the argv evals, an operand remains
/// unresolved, or a computed `cwd:` makes the path unknowable.
///
/// The SAME resolution the fit lane judges with
/// ([`super::permits_fit::resolve_against_cwd`]), so an inferred boundary
/// and the finding that would refuse it cannot disagree.
fn interpreter_script_read(a: &RawExecAction, consts: &ConstStrings) -> Option<String> {
    let operands: Vec<_> = resolve_const_argv(&a.command, consts)?.collect();
    let (program, args) = operands.split_first()?;
    if program.contains("${{") || args.iter().any(|s| s.contains("${{")) {
        return None;
    }
    let script = nika_types::exec::interpreter_script_operand(program, args)?;
    super::permits_fit::resolve_against_cwd(script, a.cwd.as_ref().map(|c| c.value.as_str()))
}

/// Fold one action (a task's main verb OR an `on_finally` cleanup verb)
/// into the inference state.
fn collect_action(c: &mut Collector, id: &str, action: &RawAction) {
    match action {
        RawAction::Exec(a) => {
            c.exec_used = true;
            match &a.command {
                // argv[0] is the verifiable program — allowlist material.
                RawCommand::Argv(_) => {
                    if let Some(p) = judgeable_program(&a.command, &c.consts) {
                        c.programs.insert(p.to_owned());
                        // An interpreter must OPEN its script before it
                        // runs a line, and the jail admits only what the
                        // boundary declares — so a block that grants the
                        // program and not the script would self-refuse the
                        // very workflow it came from (the vega-sibling law
                        // below, one boundary over). Measured 2026-08-20:
                        // without the read the leg exits 126, empty.
                        if let Some(read) = interpreter_script_read(a, &c.consts) {
                            c.reads.insert(read);
                        }
                    } else {
                        c.exec_dynamic = true;
                        c.notes.push(format!(
                            "task `{id}` runs a dynamic exec command — `exec` widened to `true`"
                        ));
                    }
                }
                // A shell string can never satisfy a Programs allowlist
                // (the runtime refuses that pairing wholesale) — inferring
                // one from its leading token would write a boundary that
                // refuses the very workflow it was inferred from.
                RawCommand::Shell(_) => {
                    c.exec_dynamic = true;
                    c.notes.push(format!(
                        "task `{id}` uses the shell-string exec form — a program \
                         allowlist applies only to the array form; `exec` widened \
                         to `true` (rewrite to the array form for a tighter permit)"
                    ));
                }
                #[allow(
                    clippy::unreachable,
                    reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
                )]
                _ => unreachable!("unknown exec command form"),
            }
            collect_exec_net(c, id, &a.command);
        }
        RawAction::Invoke(a) => match &a.target {
            nika_schema::raw::RawInvokeTarget::Tool(t) => {
                c.tools.insert(t.value.clone());
                collect_builtin_effect(c, id, a);
            }
            nika_schema::raw::RawInvokeTarget::Workflow(w) => {
                c.partial.composed = true;
                c.notes.push(format!(
                    "task `{id}` calls workflow `{}` — the child's effect \
                     boundary is resolved by the composition lane \
                     (NIKA-COMP-002 · spec 14 law 3/4), never inferred here",
                    w.value
                ));
            }
        },
        RawAction::Agent(a) => {
            // A negative whitelist rule cannot contribute a required capability.
            for tool in a.tools.iter().filter(|tool| !tool.value.starts_with('!')) {
                c.tools.insert(tool.value.clone());
            }
            collect_images_save(c, id, a);
        }
        RawAction::Infer(a) => collect_vision_reads(c, id, a),
        // The message never formats the action: it carries prompts, arguments and secret
        // references.
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        _ => unreachable!("an action kind this checker does not know"),
    }
}

/// `images: { save: }` (spec 02 §Harness images): the engine writes the received bytes at a
/// literal path (or a bare const) and its `-n` siblings, so the boundary names every one of
/// them; a rendered path is the runtime's, and names the fs face partial (never an empty write
/// set presented as the answer).
fn collect_images_save(c: &mut Collector, id: &str, a: &nika_schema::raw::RawAgentAction) {
    let Some(save) = a.images.as_ref().and_then(|i| i.value.save.as_ref()) else {
        return;
    };
    let path = if save.value.contains("${{") {
        c.consts.resolve(&save.value).map(str::to_owned)
    } else {
        Some(save.value.clone())
    };
    match path {
        Some(path) if path_escapes_workspace(&path) => {
            c.partial.fs = true;
            c.notes.push(format!(
                "task `{id}` saves its images at `{path}` — that path escapes the workspace, so \
                 no `permits.fs.write` entry is inferred"
            ));
        }
        Some(path) => {
            c.writes
                .extend(nika_types::access::harness_image_paths(&path));
        }
        None => {
            c.partial.fs = true;
            c.notes.push(format!(
                "task `{id}` saves its images at a dynamic path — `fs` cannot express 'any \
                 path'; add the resolved path(s) before running"
            ));
        }
    }
}

/// The net face of an `exec:` (issue 1597). An argv's URL reaches its host
/// exactly like an invoke's, and the fit lane's exec net arm judges the
/// SAME tokens ([`exec_url_tokens`]), so the block written here re-checks
/// clean instead of being refused by the rung that reads it. What no static
/// read can pin stays visible as a partial net face: a URL whose authority
/// a template can still move, or an HTTP client whose argv carries no URL
/// but a computed operand (the caller's URL). The run's egress proxy admits
/// only `permits.net.http`, so a block silent on that host checks green and
/// fails at run (measured on 0.124.0).
fn collect_exec_net(c: &mut Collector, id: &str, command: &RawCommand) {
    let urls = exec_url_tokens(command, &c.consts);
    for tok in &urls {
        let templated = tok.contains("${{");
        match exec_url_host(tok) {
            Some(host) if nika_types::net::host_is_blocked(&host) => {
                c.partial.net = true;
                c.notes.push(floor_note(id, &host));
            }
            Some(host) => {
                // A template ahead of the delimiter that closes the
                // authority can still move the host (`u:${{ p }}@a.example`):
                // the parsed host is granted, since the fit lane judges it,
                // and the face stays partial.
                if templated && templated_url_host(tok).is_none() {
                    c.partial.net = true;
                    c.notes.push(computed_host_note(id));
                }
                c.hosts.insert(host);
            }
            None if templated => {
                c.partial.net = true;
                c.notes.push(computed_host_note(id));
            }
            // No parseable host: nothing to grant, and the fit lane skips it.
            None => {}
        }
    }
    if !urls.is_empty() {
        return;
    }
    let RawCommand::Argv(parts) = command else {
        return;
    };
    let Some(program) = judgeable_program(command, &c.consts) else {
        return;
    };
    let base = program.rsplit(['/', '\\']).next().unwrap_or(program);
    let computed = parts
        .iter()
        .skip(1)
        .any(|p| judgeable_operand(&p.value, &c.consts).is_none());
    if computed && nika_cap::HTTP_CLIENT_PROGRAMS.contains(&base) {
        c.partial.net = true;
        c.notes.push(format!(
            "task `{id}` hands `{base}` a computed argument and no literal URL — the \
             host it reaches is resolved at run, and the exec egress proxy admits only \
             `permits.net.http`; add that host before running"
        ));
    }
}

/// The note for an exec URL whose host a template decides.
fn computed_host_note(id: &str) -> String {
    format!(
        "task `{id}` runs a URL whose host is computed — `net.http` cannot express \
         'any host'; add the resolved host(s) before running"
    )
}

/// The note for a host the always-on SSRF floor refuses whatever the
/// boundary says — never inferred into the grants (an inert entry the
/// escape scanner would flag at once: a self-refusing suggestion). A
/// LOOPBACK host is declassifiable (#395), but only by the AUTHOR's hand,
/// so the note teaches the opt-in instead.
fn floor_note(id: &str, host: &str) -> String {
    if nika_types::net::is_exact_loopback_literal(host) {
        format!(
            "task `{id}` fetches `{host}` — the always-on SSRF floor \
             (NIKA-SEC-005) refuses it unless YOU declassify it: \
             writing the exact literal `{host}` into \
             `permits.net.http` is the owner's explicit act, so it \
             is never inferred"
        )
    } else {
        format!(
            "task `{id}` fetches `{host}` — the always-on SSRF floor \
             (NIKA-SEC-005) refuses loopback/private/link-local/metadata \
             targets, so no `permits.net.http` entry can admit it; not \
             inferred (point the task at a public host)"
        )
    }
}

/// Fold a builtin invoke's literal fs/net effect into the inference state,
/// per the shared [`builtin_effect`] classification.
fn collect_builtin_effect(c: &mut Collector, id: &str, a: &nika_schema::raw::RawInvokeAction) {
    match builtin_effect(a) {
        Some(BuiltinEffect::Net { url_arg }) => {
            match judgeable_arg(&c.consts, a, url_arg)
                .as_deref()
                .and_then(url_host)
            {
                // A floor-blocked host is NEVER inferred into the grants:
                // the always-on SSRF floor (NIKA-SEC-005) refuses it
                // regardless of `permits:`, so the entry would be inert —
                // and the escape scanner would immediately flag the block
                // this inference just wrote (a self-refusing suggestion,
                // the same class as suggesting a program allowlist for a
                // shell string). Honesty note instead of a silent drop.
                // A LOOPBACK host is declassifiable (#395) — but only by
                // the AUTHOR's hand: the explicit act stays explicit, so
                // the inference still never writes it; the note teaches
                // the opt-in instead.
                Some(host) if nika_types::net::host_is_blocked(&host) => {
                    c.partial.net = true;
                    c.notes.push(floor_note(id, &host));
                }
                // A documentation host (RFC 2606/6761) is granted like any
                // other boundary member. The runtime allowlist judges the
                // host BEFORE the transport declines to dial it (`nika-http`
                // · `check_net_allowlist`, then `guarded_lookup`), and the
                // PERMITS rung reads that same boundary — so withholding the
                // grant made the inference UNSOUND against its own checker
                // (E-diff `inferred_permits_are_sound_and_tight` refused
                // `Fetch("api.example.com")` at once) and refused the mock
                // rehearsal the hint recommends. The never-dialed fact is the
                // `documentation-host` hint's sentence (wave 3 · persona 04),
                // not a hole in the block.
                Some(host) => {
                    c.hosts.insert(host);
                }
                None => {
                    c.partial.net = true;
                    c.notes.push(format!(
                        "task `{id}` reaches a dynamic URL — `net.http` cannot express \
                         'any host'; add the resolved host(s) before running"
                    ));
                }
            }
        }
        Some(effect @ BuiltinEffect::Fs { .. }) => collect_fs_effect(c, id, a, &effect),
        None => {}
    }
    // The chart vega sibling is a SECOND gated write — inferred alongside
    // the artifact (an exact-path boundary that admits the svg but not
    // its `.vl.json` would self-refuse the very workflow it came from).
    if let Some(vl) = chart_vl_sibling(a) {
        c.writes.insert(vl);
    }
    collect_secondary_reads(c, id, a);
}

/// The READ slots the single `path_arg` leaves out (`nika:image_compose`'s
/// `inputs[]`): every judgeable path is a read the boundary grants — the
/// same seam the escape scan judges, so the printed block never refuses the
/// file it came from — and a hidden one names the fs face partial.
fn collect_secondary_reads(c: &mut Collector, id: &str, a: &nika_schema::raw::RawInvokeAction) {
    let Some(tool) = a.tool() else { return };
    for (_, value) in nika_cap::secondary_read_slots(&tool.value, a.args.as_ref().map(|s| &s.value))
    {
        let path = super::permits_fit::judgeable_value(&c.consts, value);
        collect_read(c, id, path);
    }
}

/// `infer.vision:` files (spec 02 §Vision inputs): each judgeable path is a read the boundary
/// grants, judged by the escape scan through the same seam, so the printed block re-checks
/// clean. A `source: url` needs no grant: the provider fetches it.
fn collect_vision_reads(c: &mut Collector, id: &str, a: &nika_schema::raw::RawInferAction) {
    for path in super::permits_fit::vision_files(a) {
        let path = super::permits_fit::judgeable_str(&c.consts, &path.value);
        collect_read(c, id, path);
    }
}

/// Fold one read into the inference: a workspace path is granted; a path that escapes the
/// workspace is noted, never pasted as a host-file grant; a hidden one names the fs face
/// partial (`None`).
fn collect_read(c: &mut Collector, id: &str, path: Option<String>) {
    match path {
        Some(path) if path_escapes_workspace(&path) => {
            c.partial.fs = true;
            c.notes.push(format!(
                "task `{id}` reads `{path}` — that path escapes the workspace, so no \
                 `permits.fs` entry is inferred (point the task at a workspace-relative \
                 path; widening the boundary toward a host file is a deliberate operator \
                 choice, never the printed repair)"
            ));
        }
        Some(path) => {
            c.reads.insert(path);
        }
        None => {
            c.partial.fs = true;
            c.notes.push(format!(
                "task `{id}` uses a dynamic path — `fs` cannot express 'any path'; \
                 add the resolved path(s) before running"
            ));
        }
    }
}

/// Record a builtin fs effect, or an honesty note if the path is dynamic
/// or escapes the workspace (G-09 / persona 7 — never paste a host-file
/// grant the author would apply verbatim).
fn collect_fs_effect(
    c: &mut Collector,
    id: &str,
    a: &nika_schema::raw::RawInvokeAction,
    effect: &BuiltinEffect,
) {
    let BuiltinEffect::Fs {
        path_arg,
        reads,
        writes,
        recursive,
        walk_root,
    } = effect
    else {
        return;
    };
    let Some(path) = judgeable_arg(&c.consts, a, path_arg).map(|raw| {
        // Inference must write a boundary the RUNTIME accepts · for
        // a glob that is the walk root, never the pattern (2026-08-19).
        if *walk_root {
            nika_cap::glob_walk_root(&raw)
        } else {
            raw
        }
    }) else {
        c.partial.fs = true;
        c.notes.push(format!(
            "task `{id}` uses a dynamic path — `fs` cannot express 'any path'; \
             add the resolved path(s) before running"
        ));
        return;
    };
    // a recursive effect (nika:grep reads descendants ·
    // nika:image_generate writes into the dir) touches descendants too
    let entry = if *recursive {
        format!("{path}/**")
    } else {
        path
    };
    if path_escapes_workspace(&entry) {
        c.partial.fs = true;
        c.notes.push(format!(
            "task `{id}` reads or writes `{entry}` — that path \
             escapes the workspace, so no `permits.fs` entry is \
             inferred (point the task at a workspace-relative \
             path; widening the boundary toward a host file is \
             a deliberate operator choice, never the printed \
             repair)"
        ));
        return;
    }
    if *reads {
        c.reads.insert(entry.clone());
    }
    if *writes {
        c.writes.insert(entry);
    }
}

fn build_fs(reads: BTreeSet<String>, writes: BTreeSet<String>) -> Option<FsPermits> {
    if reads.is_empty() && writes.is_empty() {
        return None;
    }
    Some(FsPermits::new(
        reads.into_iter().collect(),
        writes.into_iter().collect(),
    ))
}

fn build_net(hosts: BTreeSet<String>) -> Option<NetPermits> {
    (!hosts.is_empty()).then(|| NetPermits::new(hosts.into_iter().collect()))
}

/// Render an inferred `Permits` as a spec-shaped YAML block (the
/// `--infer-permits` output the operator pastes into the envelope).
fn render_yaml(p: &Permits) -> String {
    if p.fs.is_none() && p.net.is_none() && p.exec.is_none() && p.tools.is_none() {
        // a bare `permits:` is YAML null and the parser rejects it — the
        // empty boundary is the explicit empty mapping
        return String::from("permits: {}\n");
    }
    let mut out = String::from("permits:\n");
    if let Some(fs) = &p.fs {
        out.push_str("  fs:\n");
        if !fs.read.is_empty() {
            push_field(&mut out, "    read: ", &fs.read);
        }
        if !fs.write.is_empty() {
            push_field(&mut out, "    write: ", &fs.write);
        }
    }
    if let Some(net) = &p.net {
        out.push_str("  net: { http: ");
        out.push_str(&yaml_list(&net.http));
        out.push_str(" }\n");
    }
    match &p.exec {
        Some(ExecPermit::No) => out.push_str("  exec: false\n"),
        Some(ExecPermit::Any) => out.push_str("  exec: true\n"),
        Some(ExecPermit::Programs(ps)) => push_field(&mut out, "  exec: ", ps),
        // None + any future non_exhaustive variant render nothing here.
        _ => {}
    }
    if let Some(tools) = &p.tools {
        push_field(&mut out, "  tools: ", tools);
    }
    out
}

/// Append `<label><yaml-list>\n` to the buffer — no intermediate `format!`
/// allocation (the `format_push_string` discipline).
fn push_field(out: &mut String, label: &str, items: &[String]) {
    out.push_str(label);
    out.push_str(&yaml_list(items));
    out.push('\n');
}

/// A YAML flow-sequence of double-quoted scalars (`["a", "b"]`).
fn yaml_list(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| yaml_quote(s)).collect();
    format!("[{}]", inner.join(", "))
}

/// A YAML double-quoted scalar with the escapes a literal path/host/
/// program can require (`"` · `\` · control chars) — an unescaped quote
/// would render a structurally broken block.
fn yaml_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests;
