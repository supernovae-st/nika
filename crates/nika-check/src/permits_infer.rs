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

use std::collections::BTreeSet;

use super::permits_fit::{
    BuiltinEffect, ConstStrings, builtin_effect, chart_vl_sibling, judgeable_arg,
    path_escapes_workspace, static_program, url_host,
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
/// the program is not an interpreter, the argv evals, the argv is
/// templated, or a computed `cwd:` makes the path unknowable.
///
/// The SAME resolution the fit lane judges with
/// ([`super::permits_fit::resolve_against_cwd`]), so an inferred boundary
/// and the finding that would refuse it cannot disagree.
fn interpreter_script_read(a: &RawExecAction) -> Option<String> {
    let RawCommand::Argv(parts) = &a.command else {
        return None;
    };
    let mut elements = parts.iter().map(|p| p.value.as_str());
    let program = elements.next()?;
    let args: Vec<&str> = elements.collect();
    if program.contains("${{") || args.iter().any(|s| s.contains("${{")) {
        return None;
    }
    let script = nika_types::exec::interpreter_script_operand(program, &args)?;
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
                    if let Some(p) = static_program(&a.command) {
                        c.programs.insert(p.to_owned());
                        // An interpreter must OPEN its script before it
                        // runs a line, and the jail admits only what the
                        // boundary declares — so a block that grants the
                        // program and not the script would self-refuse the
                        // very workflow it came from (the vega-sibling law
                        // below, one boundary over). Measured 2026-08-20:
                        // without the read the leg exits 126, empty.
                        if let Some(read) = interpreter_script_read(a) {
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
                other => unreachable!("unknown exec command form: {other:?}"),
            }
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
            for tool in &a.tools {
                c.tools.insert(tool.value.clone());
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
                    let note = if nika_types::net::is_exact_loopback_literal(&host) {
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
                    };
                    c.partial.net = true;
                    c.notes.push(note);
                }
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
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;
    use proptest::prelude::*;

    /// The round trip that keeps the tool's own advice honest: a boundary
    /// this module WRITES must satisfy the lane that judges it. An
    /// interpreter needs its script readable, so a block granting the
    /// program and not the script would self-refuse the very workflow it
    /// came from — the chart/tts sibling law, one boundary over.
    ///
    /// MEASURED 2026-08-20 · without the read the leg exits 126, empty
    /// stdout, and the run still renders it as a completed task.
    #[test]
    fn an_inferred_boundary_grants_the_script_its_interpreter_opens() {
        let wf = parse(
            "\
nika: t
tasks:
  leg:
    exec: { command: [\"bash\", \"scripts/deploy.sh\"] }
",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("parses");
        let inferred = infer(&wf);
        let fs = inferred.permits.fs.as_ref().expect("an fs block");
        assert!(
            fs.read.iter().any(|r| r == "scripts/deploy.sh"),
            "the script must be granted: {:?}",
            fs.read
        );
        // and the boundary it wrote must survive the lane that judges it
        assert!(
            inferred.permits.jail_admits_read("scripts/deploy.sh"),
            "an inferred boundary must not self-refuse"
        );
    }

    /// A `cwd:` re-anchors the script; a COMPUTED one makes it unknowable,
    /// and a guess there would write a grant the author never meant.
    #[test]
    fn the_inferred_script_read_follows_a_literal_cwd_and_stops_at_a_computed_one() {
        let literal = parse(
            "\
nika: t
tasks:
  leg:
    exec: { command: [\"bash\", \"inner.sh\"], cwd: \"sub\" }
",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("parses");
        let fs = infer(&literal).permits.fs.expect("an fs block");
        assert!(fs.read.iter().any(|r| r == "sub/inner.sh"), "{:?}", fs.read);

        let computed = parse(
            "\
nika: t
const:
  where: \"sub\"
tasks:
  leg:
    exec: { command: [\"bash\", \"inner.sh\"], cwd: \"${{ const.where }}\" }
",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("parses");
        let reads = infer(&computed)
            .permits
            .fs
            .map(|f| f.read)
            .unwrap_or_default();
        assert!(
            reads.is_empty(),
            "a computed cwd must pin nothing: {reads:?}"
        );
    }

    /// The silences · a non-interpreter positional is the program's own
    /// business, and inline eval opens no file.
    #[test]
    fn the_inferred_script_read_claims_nothing_it_cannot_know() {
        for yaml in [
            "nika: t\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi.txt\"] }\n",
            "nika: t\ntasks:\n  t:\n    exec: { command: [\"python3\", \"-m\", \"unittest\"] }\n",
            "nika: t\ntasks:\n  t:\n    exec: { shell: \"bash leg.sh\" }\n",
        ] {
            let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
            let reads = infer(&wf).permits.fs.map(|f| f.read).unwrap_or_default();
            assert!(reads.is_empty(), "silence expected for {yaml}: {reads:?}");
        }
    }

    /// The inference covers chart + tts (they were invisible — the
    /// boundary it wrote refused the very run it came from) and the
    /// chart vega sibling rides along.
    #[test]
    fn chart_and_tts_infer_their_writes() {
        let wf = nika_schema::parser::parse(
            "\
nika: t
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
  s:
    invoke:
      tool: \"nika:tts_generate\"
      args:
        text: \"hi\"
        output_dir: \"audio\"
",
            nika_schema::source::FileId::new(0),
            nika_schema::parser::ParseMode::Strict,
        )
        .expect("parse");
        let inferred = infer(&wf);
        let yaml = inferred.to_yaml();
        assert!(
            yaml.contains("out/c.svg"),
            "chart artifact inferred: {yaml}"
        );
        assert!(
            yaml.contains("out/c.vl.json"),
            "vega sibling inferred: {yaml}"
        );
        assert!(
            yaml.contains("audio/**"),
            "tts dir inferred recursive: {yaml}"
        );
    }

    /// The per-task projector: each family flattens deterministically,
    /// dynamic exec widens, unpinnable effects project NOTHING, and a
    /// bare infer task is empty (the wire's []).
    #[test]
    fn task_permits_attributes_each_family_deterministically() {
        let yaml = "\
nika: t
model: mock/echo
tasks:
  fetcher:
    invoke:
      tool: nika:fetch
      args:
        url: https://api.example.org/items
  writer:
    invoke:
      tool: \"nika:write\"
      args:
        path: out/report.md
        content: hi
  lister:
    exec:
      command: [\"ls\", \"-la\"]
  sheller:
    exec:
      shell: \"echo hi && ls\"
  thinker:
    infer:
      prompt: p
  looper:
    agent:
      prompt: g
      tools: [\"nika:done\", \"nika:log\"]
      max_turns: 2
";
        let wf = nika_schema::parser::parse(
            yaml,
            nika_schema::source::FileId::new(0),
            nika_schema::parser::ParseMode::Strict,
        )
        .expect("fixture parses");
        let by_id = |id: &str| {
            let t = wf
                .tasks
                .iter()
                .find(|t| t.value.id.value == id)
                .expect("task exists");
            task_permits(&t.value)
        };

        assert_eq!(
            by_id("fetcher"),
            vec!["net.http: api.example.org", "tool: nika:fetch"],
        );
        assert_eq!(
            by_id("writer"),
            vec!["fs.write: out/report.md", "tool: nika:write"],
        );
        assert_eq!(by_id("lister"), vec!["exec: ls"]);
        // Shell strings can never satisfy a program allowlist — widened.
        assert_eq!(by_id("sheller"), vec!["exec: true"]);
        // A bare infer has no pinnable effect — the wire's empty list.
        assert!(by_id("thinker").is_empty());
        // Agent tools are boundary vocabulary, BTree-ordered.
        assert_eq!(by_id("looper"), vec!["tool: nika:done", "tool: nika:log"]);
    }

    fn infer_of(yaml: &str) -> InferredPermits {
        infer(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    proptest! {
        // The security-critical property: ANY literal `nika:read` path —
        // including quotes, backslashes, control chars marked-yaml admits
        // in a double-quoted scalar — renders to a `permits:` block that
        // PARSES and admits the workflow. A broken escape would either
        // fail to parse (the operator can't paste it) or silently shift
        // the boundary (the grant no longer matches the path).
        #[test]
        fn inferred_block_round_trips_for_arbitrary_literal_paths(
            path in "[a-z0-9 ./\"\\\\\t]{1,40}",
        ) {
            // build a read on this literal path via a JSON-escaped scalar
            let esc = path.replace('\\', "\\\\").replace('"', "\\\"").replace('\t', "\\t");
            let yaml = format!(
                "nika: w\ntasks:\n  t:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{esc}\" }} }}\n"
            );
            // only proceed if the SOURCE parses (some byte seqs are not
            // valid scalars — that's the parser's domain, not ours)
            let Ok(src_wf) = parse(&yaml, FileId::new(0), ParseMode::Strict) else {
                return Ok(());
            };
            // Escaping paths are never inferred (persona 7 · G-09 shovel);
            // the round-trip claim is for workspace-relative grants.
            if path_escapes_workspace(&path) {
                return Ok(());
            }
            let r = infer(&src_wf);
            // the rendered block must itself parse + admit the workflow
            let (head, tail) = yaml.split_once("tasks:").expect("has tasks");
            let spliced = format!("{head}{}tasks:{tail}", r.to_yaml());
            let wf = parse(&spliced, FileId::new(0), ParseMode::Strict)
                .expect("inferred block must parse");
            prop_assert!(wf.permits.is_some());
            prop_assert!(
                super::super::permits_fit::scan_escapes(&wf).is_empty(),
                "boundary must admit the path it was inferred from"
            );
        }
    }

    /// Splice the inferred block into the workflow and assert the re-check
    /// reports ZERO capability escapes — the round-trip property.
    fn assert_round_trips_clean(yaml: &str) {
        let r = infer_of(yaml);
        let (head, tail) = yaml
            .split_once("tasks:")
            .expect("test workflow has a tasks: block");
        let spliced = format!("{head}{}tasks:{tail}", r.to_yaml());
        let wf = parse(&spliced, FileId::new(0), ParseMode::Strict)
            .expect("inferred permits block parses");
        assert!(wf.permits.is_some(), "the rendered block is a permits:");
        let escapes = super::super::permits_fit::scan_escapes(&wf);
        assert!(
            escapes.is_empty(),
            "inferred boundary must admit the workflow, got: {escapes:?}"
        );
    }

    #[test]
    fn pure_compute_workflow_infers_empty_boundary() {
        // infer-only, no effects → exec:false, no fs/net/tools.
        let r = infer_of(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 10 }\n",
        );
        assert_eq!(r.permits.exec, Some(ExecPermit::No));
        assert!(r.permits.fs.is_none() && r.permits.net.is_none());
        assert!(r.permits.tools.is_none());
        assert!(r.notes.is_empty());
    }

    #[test]
    fn literal_effects_infer_a_tight_boundary() {
        let r = infer_of(
            "nika: w\ntasks:\n  rd:\n    invoke: { tool: \"nika:read\", args: { path: \"./data/in.json\" } }\n  get:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.x.com/v1\" } }\n  build:\n    exec: { command: [\"cargo\", \"build\"] }\n",
        );
        assert_eq!(
            r.permits.exec,
            Some(ExecPermit::Programs(vec!["cargo".to_owned()]))
        );
        assert_eq!(
            r.permits.fs.as_ref().expect("fs").read,
            vec!["./data/in.json"]
        );
        assert_eq!(r.permits.net.as_ref().expect("net").http, vec!["api.x.com"]);
        let tools = r.permits.tools.as_ref().expect("tools");
        assert!(tools.contains(&"nika:read".to_owned()));
        assert!(tools.contains(&"nika:fetch".to_owned()));
        assert!(r.notes.is_empty(), "all effects were literal");
    }

    #[test]
    fn dynamic_exec_widens_to_any_with_a_note() {
        // A dynamic SHELL string rides the shell-string arm (the form
        // decides before the head is even looked at).
        let r = infer_of(
            "nika: w\nconst: { c: \"git\" }\ntasks:\n  t:\n    exec: { shell: \"${{ const.c }} status\" }\n",
        );
        assert_eq!(r.permits.exec, Some(ExecPermit::Any), "dynamic → true");
        assert_eq!(r.notes.len(), 1);
        assert!(r.notes[0].contains("shell-string"));
    }

    #[test]
    fn literal_shell_string_never_infers_a_self_refusing_allowlist() {
        // The trap this pins: `shell: "git log"` used to infer
        // `exec: ["git"]` — a boundary the runtime then REFUSES for the
        // very task it was inferred from (shell-string under a Programs
        // allowlist is rejected wholesale at dispatch). The sound
        // inference is `exec: true` + a rewrite-to-argv note.
        let r = infer_of("nika: w\ntasks:\n  t:\n    exec: { shell: \"git log\" }\n");
        assert_eq!(
            r.permits.exec,
            Some(ExecPermit::Any),
            "shell string → true, never a leading-token allowlist"
        );
        assert!(r.notes.iter().any(|n| n.contains("array form")));
    }

    #[test]
    fn dynamic_argv_head_widens_to_any_not_a_garbage_literal() {
        // `["${{ inputs.bin }}"]` must NOT be inferred as a literal program
        // named `${{ inputs.bin }}` — the head is dynamic → exec: true + note.
        let r = infer_of(
            "nika: w\nconst: { bin: \"git\" }\ntasks:\n  t:\n    exec: { command: [\"${{ const.bin }}\", \"status\"] }\n",
        );
        assert_eq!(r.permits.exec, Some(ExecPermit::Any));
        assert!(r.notes.iter().any(|n| n.contains("dynamic exec")));
    }

    #[test]
    fn dynamic_fetch_url_notes_review() {
        let r = infer_of(
            "nika: w\nconst: { h: \"x.com\" }\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://${{ const.h }}/p\" } }\n",
        );
        // host couldn't be pinned → net stays unset, a note flags it
        assert!(r.permits.net.is_none());
        assert!(r.notes.iter().any(|n| n.contains("dynamic URL")));
    }

    #[test]
    fn edit_grep_and_webhook_notify_are_classified() {
        let yaml = "nika: w\ntasks:\n  fix:\n    invoke: { tool: \"nika:edit\", args: { path: \"./README.md\", find: \"a\", replace: \"b\" } }\n  scan:\n    invoke: { tool: \"nika:grep\", args: { pattern: \"TODO\", path: \"./src\" } }\n  ping:\n    invoke: { tool: \"nika:notify\", args: { channel: \"webhook\", target: \"https://hooks.slack.com/x\", message: \"hi\" } }\n";
        let r = infer_of(yaml);
        let fs = r.permits.fs.as_ref().expect("fs");
        // edit reads then rewrites the same path
        assert_eq!(fs.read, vec!["./README.md", "./src/**"]);
        assert_eq!(fs.write, vec!["./README.md"]);
        // webhook notify is a net egress to the target host
        assert_eq!(
            r.permits.net.as_ref().expect("net").http,
            vec!["hooks.slack.com"]
        );
        assert_round_trips_clean(yaml);
    }

    #[test]
    fn image_generate_infers_a_recursive_write_grant() {
        // Assets + the manifest land INSIDE output_dir — the inference
        // grants `<dir>/**` on fs.write (the grep-recursive analog, write
        // side), and the provider egress is deliberately NOT a net grant
        // (the image plane rides engine transport, like `infer:`).
        let yaml = "nika: w\ntasks:\n  og:\n    invoke: { tool: \"nika:image_generate\", args: { prompt: \"hero\", output_dir: \"./assets/og\" } }\n";
        let r = infer_of(yaml);
        let fs = r.permits.fs.as_ref().expect("fs");
        assert!(fs.read.is_empty(), "generation reads nothing");
        assert_eq!(fs.write, vec!["./assets/og/**"]);
        assert!(
            r.permits.net.is_none(),
            "provider egress ≠ permits.net.http"
        );
        assert!(
            r.permits
                .tools
                .as_ref()
                .expect("tools")
                .contains(&"nika:image_generate".to_owned())
        );
        assert!(r.notes.is_empty(), "literal dir → no review note");
        assert_round_trips_clean(yaml);
    }

    /// A `const:`-backed dir RESOLVES · a runtime-supplied one does not.
    ///
    /// This test used to call the const case "dynamic" and assert that
    /// inference could not pin it. That premise was wrong on the same
    /// point the FIT scan was wrong on: `--var` satisfies `inputs:` and
    /// nothing else (measured — on a file whose `p` is a const, `nika run
    /// --var p=X` answers « this workflow declares no inputs »), so a
    /// const cannot move between check and run and is therefore statically
    /// known.
    ///
    /// The consequence of the old reading was a tool contradicting itself:
    /// `check` called a const-backed path a literal path and judged it,
    /// while `--infer-permits` called the same path too dynamic to pin and
    /// omitted it — so the block it printed never round-tripped clean on
    /// any file routing a path through `const:`, which is every file the
    /// templates teach.
    ///
    /// THE AUTHORITY IS THE BOUNDARY, and this pins it in both directions.
    #[test]
    fn a_const_dir_resolves_and_a_runtime_one_notes_review() {
        let konst = "nika: w\nconst: { dir: \"./assets\" }\ntasks:\n  og:\n    invoke: { tool: \"nika:image_generate\", args: { prompt: \"hero\", output_dir: \"${{ const.dir }}\" } }\n";
        let r = infer_of(konst);
        assert!(
            r.permits.fs.is_some(),
            "a const-backed dir is statically known: {:?}",
            r.permits.fs
        );
        assert_round_trips_clean(konst);

        // `inputs:` is genuinely dynamic even WITH a default — the run can
        // supply another value, and a boundary drafted against a value the
        // run may replace is exactly the claim this tool must not make.
        let runtime = "nika: w\ninputs:\n  dir:\n    type: string\n    default: \"./assets\"\ntasks:\n  og:\n    invoke: { tool: \"nika:image_generate\", args: { prompt: \"hero\", output_dir: \"${{ inputs.dir }}\" } }\n";
        let r = infer_of(runtime);
        assert!(r.permits.fs.is_none(), "a runtime dir cannot be pinned");
        assert!(r.notes.iter().any(|n| n.contains("dynamic path")));
    }

    #[test]
    fn non_webhook_notify_is_not_a_net_effect() {
        let r = infer_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:notify\", args: { channel: \"email\", target: \"ops@x.com\", message: \"hi\" } }\n",
        );
        assert!(
            r.permits.net.is_none(),
            "email rides an engine transport, not a workflow host grant"
        );
    }

    #[test]
    fn rendered_yaml_round_trips_through_the_parser() {
        // The inferred block must itself parse AND admit the workflow.
        assert_round_trips_clean(
            "nika: w\ntasks:\n  rd:\n    invoke: { tool: \"nika:read\", args: { path: \"./data/x\" } }\n  build:\n    exec: { command: [\"cargo\", \"test\"] }\n",
        );
    }

    #[test]
    fn exec_false_and_exec_true_round_trip() {
        // ExecPermit::No — a pure-invoke workflow renders `exec: false`.
        assert_round_trips_clean(
            "nika: w\ntasks:\n  rd:\n    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n",
        );
        // ExecPermit::Any — a dynamic command renders `exec: true`.
        assert_round_trips_clean(
            "nika: w\nconst: { c: \"git\" }\ntasks:\n  t:\n    exec: { shell: \"${{ const.c }} status\" }\n",
        );
    }

    #[test]
    fn agent_tool_globs_round_trip() {
        let yaml = "nika: w\ntasks:\n  t:\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:fetch\", \"mcp:browser/*\"]\n";
        let r = infer_of(yaml);
        let tools = r.permits.tools.as_ref().expect("tools");
        assert!(tools.contains(&"mcp:browser/*".to_owned()));
        assert_eq!(r.permits.exec, Some(ExecPermit::No), "no exec task");
        assert_round_trips_clean(yaml);
    }

    #[test]
    fn quotes_and_backslashes_in_paths_render_valid_yaml() {
        // An unescaped `"` would render a structurally broken block — the
        // review's PROVEN parse failure.
        let yaml = "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:read\", args: { path: \"data/he said \\\"hi\\\".json\" } }\n";
        assert_round_trips_clean(yaml);
        let r = infer_of(yaml);
        assert!(r.to_yaml().contains(r#"\"hi\""#), "quote is escaped");
    }

    #[test]
    fn empty_permits_renders_the_explicit_empty_mapping() {
        // A bare `permits:` is YAML null and the parser rejects it.
        let rendered = render_yaml(&Permits::new());
        assert_eq!(rendered, "permits: {}\n");
        let full = format!(
            "nika: w\n{rendered}tasks:\n  t:\n    infer: {{ prompt: \"hi\", max_tokens: 5 }}\n"
        );
        assert!(
            parse(&full, FileId::new(0), ParseMode::Strict).is_ok(),
            "the empty mapping form parses"
        );
    }

    #[test]
    fn floor_blocked_host_is_never_inferred_into_the_grants() {
        // A loopback fetch must NOT synthesize `net.http: ["127.0.0.1"]`
        // — even though the exact literal WOULD declassify the floor
        // (#395): the explicit act must stay the author's, so the
        // inference keeps its hands off and the note TEACHES the opt-in.
        // Public hosts still infer.
        let r = infer_of(
            "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://127.0.0.1:9/x\" } }\n  b:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/x\" } }\n",
        );
        let net = r.permits.net.as_ref().expect("public host infers net");
        assert_eq!(net.http, vec!["api.example.com".to_owned()]);
        assert_eq!(r.notes.len(), 1, "{:?}", r.notes);
        assert!(r.notes[0].contains("SSRF floor"), "{}", r.notes[0]);
        assert!(r.notes[0].contains("`127.0.0.1`"), "{}", r.notes[0]);
        assert!(
            r.notes[0].contains("explicit act"),
            "the loopback note teaches the opt-in: {}",
            r.notes[0]
        );
    }

    #[test]
    fn an_escaping_host_path_is_never_inferred_into_the_grants() {
        // Persona 7 · 2026-08-22: `--infer-permits` printed
        // `fs.read: ["/etc/passwd"]`, the author pasted it, check+run
        // greened. Absolute / home-relative paths stay a note.
        let r = infer_of(
            "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:read\", args: { path: \"/etc/passwd\" } }\n  b:\n    invoke: { tool: \"nika:read\", args: { path: \"./notes.md\" } }\n",
        );
        let fs = r.permits.fs.as_ref().expect("workspace path infers fs");
        assert_eq!(fs.read, vec!["./notes.md".to_owned()]);
        assert!(
            !fs.read.iter().any(|p| p.contains("passwd")),
            "host file must not be in the paste block: {:?}",
            fs.read
        );
        assert!(r.partial.fs, "the escaping face is incomplete");
        assert!(
            r.notes
                .iter()
                .any(|n| n.contains("/etc/passwd") && n.contains("never the printed repair")),
            "{:?}",
            r.notes
        );
        let home = infer_of(
            "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:read\", args: { path: \"~/.ssh/id_rsa\" } }\n",
        );
        assert!(
            home.permits.fs.as_ref().is_none_or(|f| f.read.is_empty()),
            "home-relative secrets are not inferred: {:?}",
            home.permits.fs
        );
        assert!(home.partial.fs);
    }

    #[test]
    fn non_loopback_floor_note_never_teaches_the_opt_in() {
        // The never-list keeps the pre-#395 wording: no entry can admit a
        // metadata/RFC1918 target, so the note must NOT hint one.
        let r = infer_of(
            "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://169.254.169.254/latest/meta-data/\" } }\n",
        );
        assert_eq!(r.notes.len(), 1, "{:?}", r.notes);
        assert!(
            r.notes[0].contains("no `permits.net.http` entry can admit it"),
            "{}",
            r.notes[0]
        );
        assert!(
            !r.notes[0].contains("explicit act"),
            "never-list targets get no opt-in hint: {}",
            r.notes[0]
        );
    }
}
