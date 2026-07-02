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

use super::permits_fit::{BuiltinEffect, builtin_effect, literal_arg, static_program, url_host};
use crate::raw::{RawAction, RawCommand, RawWorkflow};
use crate::types::{ExecPermit, FsPermits, NetPermits, Permits};

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
    pub notes: Vec<String>,
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
    exec_used: bool,
    exec_dynamic: bool,
    programs: BTreeSet<String>,
    tools: BTreeSet<String>,
    hosts: BTreeSet<String>,
    reads: BTreeSet<String>,
    writes: BTreeSet<String>,
    notes: Vec<String>,
}

/// Infer the tightest `permits:` for a workflow.
#[must_use]
pub(super) fn infer(wf: &RawWorkflow) -> InferredPermits {
    let mut c = Collector::default();
    for task in &wf.tasks {
        let id = &task.value.id.value;
        collect_action(&mut c, id, &task.value.action);
        for cleanup in &task.value.on_finally {
            collect_action(&mut c, &format!("{id} (on_finally)"), &cleanup.value.action);
        }
    }

    let exec = if !c.exec_used {
        Some(ExecPermit::No)
    } else if c.exec_dynamic {
        Some(ExecPermit::Any)
    } else {
        Some(ExecPermit::Programs(c.programs.into_iter().collect()))
    };

    let permits = Permits {
        fs: build_fs(c.reads, c.writes),
        net: build_net(c.hosts),
        exec,
        tools: (!c.tools.is_empty()).then(|| c.tools.into_iter().collect()),
    };
    InferredPermits {
        permits,
        notes: c.notes,
    }
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
            }
        }
        RawAction::Invoke(a) => {
            c.tools.insert(a.tool.value.clone());
            collect_builtin_effect(c, id, a);
        }
        RawAction::Agent(a) => {
            for tool in &a.tools {
                c.tools.insert(tool.value.clone());
            }
        }
        RawAction::Infer(_) => {}
    }
}

/// Fold a builtin invoke's literal fs/net effect into the inference state,
/// per the shared [`builtin_effect`] classification.
fn collect_builtin_effect(c: &mut Collector, id: &str, a: &crate::raw::RawInvokeAction) {
    match builtin_effect(a) {
        Some(BuiltinEffect::Net { url_arg }) => {
            match literal_arg(a, url_arg).as_deref().and_then(url_host) {
                Some(host) => {
                    c.hosts.insert(host);
                }
                None => c.notes.push(format!(
                    "task `{id}` reaches a dynamic URL — `net.http` cannot express \
                     'any host'; add the resolved host(s) before running"
                )),
            }
        }
        Some(BuiltinEffect::Fs {
            reads,
            writes,
            recursive,
        }) => match literal_arg(a, "path") {
            Some(path) => {
                // a recursive reader (nika:grep) touches descendants too
                let entry = if recursive {
                    format!("{path}/**")
                } else {
                    path
                };
                if reads {
                    c.reads.insert(entry.clone());
                }
                if writes {
                    c.writes.insert(entry);
                }
            }
            None => c.notes.push(format!(
                "task `{id}` uses a dynamic path — `fs` cannot express 'any path'; \
                 add the resolved path(s) before running"
            )),
        },
        None => {}
    }
}

fn build_fs(reads: BTreeSet<String>, writes: BTreeSet<String>) -> Option<FsPermits> {
    if reads.is_empty() && writes.is_empty() {
        return None;
    }
    Some(FsPermits {
        read: reads.into_iter().collect(),
        write: writes.into_iter().collect(),
    })
}

fn build_net(hosts: BTreeSet<String>) -> Option<NetPermits> {
    (!hosts.is_empty()).then(|| NetPermits {
        http: hosts.into_iter().collect(),
    })
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
        None => {}
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
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;
    use proptest::prelude::*;

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
                "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{esc}\" }} }}\n"
            );
            // only proceed if the SOURCE parses (some byte seqs are not
            // valid scalars — that's the parser's domain, not ours)
            let Ok(src_wf) = parse(&yaml, FileId::new(0), ParseMode::Strict) else {
                return Ok(());
            };
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
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: t\n    infer: { prompt: \"hi\", max_tokens: 10 }\n",
        );
        assert_eq!(r.permits.exec, Some(ExecPermit::No));
        assert!(r.permits.fs.is_none() && r.permits.net.is_none());
        assert!(r.permits.tools.is_none());
        assert!(r.notes.is_empty());
    }

    #[test]
    fn literal_effects_infer_a_tight_boundary() {
        let r = infer_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: rd\n    invoke: { tool: \"nika:read\", args: { path: \"./data/in.json\" } }\n  - id: get\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.x.com/v1\" } }\n  - id: build\n    exec: { command: [\"cargo\", \"build\"] }\n",
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
            "nika: v1\nworkflow: w\nvars: { c: \"git\" }\ntasks:\n  - id: t\n    exec: { command: \"${{ vars.c }} status\" }\n",
        );
        assert_eq!(r.permits.exec, Some(ExecPermit::Any), "dynamic → true");
        assert_eq!(r.notes.len(), 1);
        assert!(r.notes[0].contains("shell-string"));
    }

    #[test]
    fn literal_shell_string_never_infers_a_self_refusing_allowlist() {
        // The trap this pins: `command: "git log"` used to infer
        // `exec: ["git"]` — a boundary the runtime then REFUSES for the
        // very task it was inferred from (shell-string under a Programs
        // allowlist is rejected wholesale at dispatch). The sound
        // inference is `exec: true` + a rewrite-to-argv note.
        let r = infer_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { command: \"git log\" }\n",
        );
        assert_eq!(
            r.permits.exec,
            Some(ExecPermit::Any),
            "shell string → true, never a leading-token allowlist"
        );
        assert!(r.notes.iter().any(|n| n.contains("array form")));
    }

    #[test]
    fn dynamic_argv_head_widens_to_any_not_a_garbage_literal() {
        // `["${{ vars.bin }}"]` must NOT be inferred as a literal program
        // named `${{ vars.bin }}` — the head is dynamic → exec: true + note.
        let r = infer_of(
            "nika: v1\nworkflow: w\nvars: { bin: \"git\" }\ntasks:\n  - id: t\n    exec: { command: [\"${{ vars.bin }}\", \"status\"] }\n",
        );
        assert_eq!(r.permits.exec, Some(ExecPermit::Any));
        assert!(r.notes.iter().any(|n| n.contains("dynamic exec")));
    }

    #[test]
    fn dynamic_fetch_url_notes_review() {
        let r = infer_of(
            "nika: v1\nworkflow: w\nvars: { h: \"x.com\" }\ntasks:\n  - id: t\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://${{ vars.h }}/p\" } }\n",
        );
        // host couldn't be pinned → net stays unset, a note flags it
        assert!(r.permits.net.is_none());
        assert!(r.notes.iter().any(|n| n.contains("dynamic URL")));
    }

    #[test]
    fn on_finally_cleanup_effects_are_collected() {
        // The review's PROVEN miss: a cleanup that writes a log file MUST
        // contribute its tool grant + fs.write — it always runs.
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: build\n    exec: { command: [\"cargo\", \"build\"] }\n    on_finally:\n      - invoke: { tool: \"nika:write\", args: { path: \"./out/log.txt\", content: \"done\" } }\n";
        let r = infer_of(yaml);
        assert_eq!(
            r.permits.fs.as_ref().expect("fs").write,
            vec!["./out/log.txt"]
        );
        assert!(
            r.permits
                .tools
                .as_ref()
                .expect("tools")
                .contains(&"nika:write".to_owned())
        );
        assert_round_trips_clean(yaml);
    }

    #[test]
    fn edit_grep_and_webhook_notify_are_classified() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: fix\n    invoke: { tool: \"nika:edit\", args: { path: \"./README.md\", find: \"a\", replace: \"b\" } }\n  - id: scan\n    invoke: { tool: \"nika:grep\", args: { pattern: \"TODO\", path: \"./src\" } }\n  - id: ping\n    invoke: { tool: \"nika:notify\", args: { channel: \"webhook\", target: \"https://hooks.slack.com/x\", message: \"hi\" } }\n";
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
    fn non_webhook_notify_is_not_a_net_effect() {
        let r = infer_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: { tool: \"nika:notify\", args: { channel: \"email\", target: \"ops@x.com\", message: \"hi\" } }\n",
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
            "nika: v1\nworkflow: w\ntasks:\n  - id: rd\n    invoke: { tool: \"nika:read\", args: { path: \"./data/x\" } }\n  - id: build\n    exec: { command: [\"cargo\", \"test\"] }\n",
        );
    }

    #[test]
    fn exec_false_and_exec_true_round_trip() {
        // ExecPermit::No — a pure-invoke workflow renders `exec: false`.
        assert_round_trips_clean(
            "nika: v1\nworkflow: w\ntasks:\n  - id: rd\n    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n",
        );
        // ExecPermit::Any — a dynamic command renders `exec: true`.
        assert_round_trips_clean(
            "nika: v1\nworkflow: w\nvars: { c: \"git\" }\ntasks:\n  - id: t\n    exec: { command: \"${{ vars.c }} status\" }\n",
        );
    }

    #[test]
    fn agent_tool_globs_round_trip() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:fetch\", \"mcp:browser/*\"]\n";
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
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: { tool: \"nika:read\", args: { path: \"data/he said \\\"hi\\\".json\" } }\n";
        assert_round_trips_clean(yaml);
        let r = infer_of(yaml);
        assert!(r.to_yaml().contains(r#"\"hi\""#), "quote is escaped");
    }

    #[test]
    fn empty_permits_renders_the_explicit_empty_mapping() {
        // A bare `permits:` is YAML null and the parser rejects it.
        let rendered = render_yaml(&Permits {
            fs: None,
            net: None,
            exec: None,
            tools: None,
        });
        assert_eq!(rendered, "permits: {}\n");
        let full = format!(
            "nika: v1\nworkflow: w\n{rendered}tasks:\n  - id: t\n    infer: {{ prompt: \"hi\", max_tokens: 5 }}\n"
        );
        assert!(
            parse(&full, FileId::new(0), ParseMode::Strict).is_ok(),
            "the empty mapping form parses"
        );
    }
}
