// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! E-diff · the STATIC differential battery — the full `check()` ladder
//! pinned to the ONE reference boundary semantics over a generated
//! permit lattice.
//!
//! The proposition (spec 01 §permits · NIKA-SEC-004): an effect outside
//! the declared boundary is refused. The exec family already has its
//! two-leg differential (`nika-runtime tests/boundary_differential.rs`);
//! this battery extends the STATIC leg to the remaining families —
//! fs · net · tools — through the WHOLE pipeline (YAML parse → permits
//! deserialization → task walk → literal-arg extraction → cap fit), so
//! a divergence here is a plumbing bug the unit tests around `allows_*`
//! cannot see: the reference verdict is computed on a `Permits` BUILT in
//! memory, the ladder verdict on the SAME boundary round-tripped through
//! authored YAML.
//!
//! It also closes the loop on capability INFERENCE (`--infer-permits`):
//!
//! * **sound** — the permits `infer_permits` synthesizes make `check`
//!   clean for the very workflow they were inferred from;
//! * **tight** — dropping ANY single grant reddens `check` again, so
//!   inference never over-grants by a removable entry.
//!
//! Composed: check ⇔ reference boundary ⇔ inference. The runtime legs
//! for fs/net LANDED with the F-O6 lane · `nika-cli/tests/
//! check_run_equivalence.rs` drives the real builtin sinks at the real
//! binary (deny + floor sides · the allow-side dial stays undialed).

use std::fmt::Write as _;

use nika_check::{check, infer_permits};
use nika_schema::types::{ExecPermit, FsPermits, NetPermits, Permits};
use nika_schema::{FileId, ParseMode, parse};
use proptest::prelude::*;

// -- alphabets ---------------------------------------------------------------
// Small on purpose: collisions between grants and attempted effects are what
// find bugs; a huge random alphabet would almost never collide (the same
// philosophy as the exec differential's 5-program alphabet).

/// One literal drawn from a fixed alphabet. `prop_oneof!` over bare string
/// literals would treat them as REGEXES (`.` any-char · `*` repetition) —
/// `Just` keeps every atom byte-exact.
fn one_of(atoms: &'static [&'static str]) -> impl Strategy<Value = String> {
    prop::sample::select(atoms).prop_map(String::from)
}

fn path_atom() -> impl Strategy<Value = String> {
    one_of(&["data/in.txt", "out/report.md", "notes.md"])
}

/// Grants include the literal paths plus directory globs, so the glob
/// half of `allows_path` is exercised too.
fn path_grant() -> impl Strategy<Value = String> {
    one_of(&[
        "data/in.txt",
        "out/report.md",
        "notes.md",
        "data/**",
        "out/**",
    ])
}

/// Public-shaped hosts only — the SSRF floor (`NIKA-SEC-005` · localhost /
/// metadata / literal-IP families) is a DIFFERENT law with `floor: true`
/// escapes; this battery is about the declared boundary (`floor: false`).
fn host_atom() -> impl Strategy<Value = String> {
    one_of(&["api.example.com", "cdn.example.com", "evil.example.com"])
}

fn host_grant() -> impl Strategy<Value = String> {
    one_of(&[
        "api.example.com",
        "cdn.example.com",
        "evil.example.com",
        "*.example.com",
    ])
}

fn tool_grant() -> impl Strategy<Value = String> {
    one_of(&["nika:read", "nika:write", "nika:*", "mcp:browser/*"])
}

// -- YAML rendering ----------------------------------------------------------

fn yaml_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| format!("\"{s}\"")).collect();
    format!("[{}]", quoted.join(", "))
}

/// Render a `Permits` as the envelope-level `permits:` block — the test's
/// own renderer (the exec differential owns one too), so the property goes
/// through the REAL parser rather than any engine-side render helper.
fn render_permits(p: &Permits) -> String {
    let mut body = String::new();
    if let Some(fs) = &p.fs
        && (!fs.read.is_empty() || !fs.write.is_empty())
    {
        body.push_str("  fs:\n");
        if !fs.read.is_empty() {
            let _ = writeln!(body, "    read: {}", yaml_list(&fs.read));
        }
        if !fs.write.is_empty() {
            let _ = writeln!(body, "    write: {}", yaml_list(&fs.write));
        }
    }
    if let Some(net) = &p.net {
        let _ = writeln!(body, "  net: {{ http: {} }}", yaml_list(&net.http));
    }
    match &p.exec {
        Some(ExecPermit::No) => body.push_str("  exec: false\n"),
        Some(ExecPermit::Any) => body.push_str("  exec: true\n"),
        Some(ExecPermit::Programs(ps)) => {
            let _ = writeln!(body, "  exec: {}", yaml_list(ps));
        }
        _ => {}
    }
    if let Some(tools) = &p.tools {
        let _ = writeln!(body, "  tools: {}", yaml_list(tools));
    }
    if body.is_empty() {
        "permits: {}\n".to_string()
    } else {
        format!("permits:\n{body}")
    }
}

fn workflow(permits_block: &str, tasks_block: &str) -> String {
    format!("nika: ediff\n{permits_block}tasks:\n{tasks_block}")
}

fn checked(yaml: &str) -> nika_check::CheckReport {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("synthesized YAML parses");
    check(&wf)
}

/// Non-floor escapes for one category — the declared-boundary verdict,
/// with the always-on floors (SSRF · blocklist) filtered out.
fn boundary_escapes(report: &nika_check::CheckReport, category: &str) -> usize {
    report
        .capability_escapes
        .iter()
        .filter(|e| !e.floor && e.category == category)
        .count()
}

// -- the fs leg --------------------------------------------------------------

proptest! {
    /// The fs boundary: `check` flags a literal `nika:read`/`nika:write`
    /// path exactly when the reference boundary refuses it.
    #[test]
    fn static_fs_boundary_matches_allows_path(
        read in prop::collection::btree_set(path_grant(), 0..3),
        write in prop::collection::btree_set(path_grant(), 0..3),
        path in path_atom(),
        write_op in any::<bool>(),
    ) {
        let read: Vec<String> = read.into_iter().collect();
        let write: Vec<String> = write.into_iter().collect();
        let mut p = Permits::new();
        p.tools = Some(vec!["nika:*".to_string()]); // isolate the fs variable
        if !read.is_empty() || !write.is_empty() {
            p.fs = Some(FsPermits::new(read, write));
        }
        let task = if write_op {
            format!(
                "  t:\n    invoke:\n      tool: \"nika:write\"\n      args:\n        path: \"{path}\"\n        content: \"x\"\n"
            )
        } else {
            format!(
                "  t:\n    invoke:\n      tool: \"nika:read\"\n      args:\n        path: \"{path}\"\n"
            )
        };
        let report = checked(&workflow(&render_permits(&p), &task));
        let ladder_admits = boundary_escapes(&report, "fs") == 0;
        prop_assert_eq!(
            ladder_admits,
            p.allows_path(&path, write_op),
            "fs divergence: path={:?} write={} permits={:?}",
            path, write_op, p
        );
    }

    /// The net boundary: a literal `nika:fetch` host is flagged exactly
    /// when the reference boundary refuses it (floor filtered out).
    #[test]
    fn static_net_boundary_matches_allows_host(
        grants in prop::collection::btree_set(host_grant(), 0..3),
        host in host_atom(),
    ) {
        let grants: Vec<String> = grants.into_iter().collect();
        let mut p = Permits::new();
        p.tools = Some(vec!["nika:*".to_string()]);
        if !grants.is_empty() {
            p.net = Some(NetPermits::new(grants));
        }
        let task = format!(
            "  t:\n    invoke:\n      tool: \"nika:fetch\"\n      args:\n        url: \"https://{host}/x\"\n"
        );
        let report = checked(&workflow(&render_permits(&p), &task));
        let ladder_admits = boundary_escapes(&report, "net") == 0;
        prop_assert_eq!(
            ladder_admits,
            p.allows_host(&host),
            "net divergence: host={:?} permits={:?}",
            host, p
        );
    }

    /// The tools boundary: with the fs side fully granted, the ONLY
    /// possible non-floor escape is the tool grant — present exactly when
    /// the reference boundary refuses the tool id.
    #[test]
    fn static_tools_boundary_matches_allows_tool(
        grants in prop::collection::btree_set(tool_grant(), 0..3),
        write_op in any::<bool>(),
    ) {
        let grants: Vec<String> = grants.into_iter().collect();
        let path = "data/in.txt".to_string();
        let mut p = Permits::new();
        p.fs = Some(FsPermits::new(vec![path.clone()], vec![path.clone()]));
        if !grants.is_empty() {
            p.tools = Some(grants);
        }
        let tool = if write_op { "nika:write" } else { "nika:read" };
        let task = if write_op {
            format!(
                "  t:\n    invoke:\n      tool: \"{tool}\"\n      args:\n        path: \"{path}\"\n        content: \"x\"\n"
            )
        } else {
            format!(
                "  t:\n    invoke:\n      tool: \"{tool}\"\n      args:\n        path: \"{path}\"\n"
            )
        };
        let report = checked(&workflow(&render_permits(&p), &task));
        let any_boundary_escape = report.capability_escapes.iter().any(|e| !e.floor);
        prop_assert_eq!(
            !any_boundary_escape,
            p.allows_tool(tool),
            "tools divergence: tool={:?} permits={:?}",
            tool, p
        );
    }
}

// -- the inference round-trip ------------------------------------------------

#[derive(Debug, Clone)]
enum Mv {
    Read(String),
    Write(String),
    Fetch(String),
    Exec(String),
}

fn gen_moves() -> impl Strategy<Value = Vec<Mv>> {
    prop::collection::vec(
        prop_oneof![
            path_atom().prop_map(Mv::Read),
            path_atom().prop_map(Mv::Write),
            host_atom().prop_map(Mv::Fetch),
            one_of(&["git", "ls", "rm"]).prop_map(Mv::Exec),
        ],
        1..5,
    )
}

fn tasks_for(moves: &[Mv]) -> String {
    let mut out = String::new();
    for (i, m) in moves.iter().enumerate() {
        match m {
            Mv::Read(p) => {
                let _ = write!(
                    out,
                    "  t{i}:\n    invoke:\n      tool: \"nika:read\"\n      args:\n        path: \"{p}\"\n"
                );
            }
            Mv::Write(p) => {
                let _ = write!(
                    out,
                    "  t{i}:\n    invoke:\n      tool: \"nika:write\"\n      args:\n        path: \"{p}\"\n        content: \"x\"\n"
                );
            }
            Mv::Fetch(h) => {
                let _ = write!(
                    out,
                    "  t{i}:\n    invoke:\n      tool: \"nika:fetch\"\n      args:\n        url: \"https://{h}/x\"\n"
                );
            }
            Mv::Exec(prog) => {
                let _ = write!(
                    out,
                    "  t{i}:\n    exec:\n      command: [\"{prog}\", \"--version\"]\n"
                );
            }
        }
    }
    out
}

/// Every boundary with exactly ONE grant removed — the tightness probes.
/// A category emptied by the drop is normalized to its absent form so the
/// rendered YAML stays parseable (`fs:` with no lists is YAML null).
fn drop_one_grant(p: &Permits) -> Vec<Permits> {
    let mut out = Vec::new();
    if let Some(fs) = &p.fs {
        for i in 0..fs.read.len() {
            let mut q = p.clone();
            let mut read = fs.read.clone();
            read.remove(i);
            q.fs = if read.is_empty() && fs.write.is_empty() {
                None
            } else {
                Some(FsPermits::new(read, fs.write.clone()))
            };
            out.push(q);
        }
        for i in 0..fs.write.len() {
            let mut q = p.clone();
            let mut write = fs.write.clone();
            write.remove(i);
            q.fs = if fs.read.is_empty() && write.is_empty() {
                None
            } else {
                Some(FsPermits::new(fs.read.clone(), write))
            };
            out.push(q);
        }
    }
    if let Some(net) = &p.net {
        for i in 0..net.http.len() {
            let mut q = p.clone();
            let mut http = net.http.clone();
            http.remove(i);
            q.net = if http.is_empty() {
                None
            } else {
                Some(NetPermits::new(http))
            };
            out.push(q);
        }
    }
    if let Some(ExecPermit::Programs(ps)) = &p.exec {
        for i in 0..ps.len() {
            let mut q = p.clone();
            let mut list = ps.clone();
            list.remove(i);
            // an emptied list stays `exec: []` — category enabled, no program
            q.exec = Some(ExecPermit::Programs(list));
            out.push(q);
        }
    }
    if let Some(tools) = &p.tools {
        for i in 0..tools.len() {
            let mut q = p.clone();
            let mut list = tools.clone();
            list.remove(i);
            q.tools = Some(list); // `tools: []` = no tool grants
            out.push(q);
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// `--infer-permits` round-trip: the inferred boundary makes `check`
    /// clean (sound) and loses that cleanliness the moment ANY single
    /// grant is dropped (tight).
    #[test]
    fn inferred_permits_are_sound_and_tight(moves in gen_moves()) {
        let tasks = tasks_for(&moves);

        // Infer from the bare workflow (no permits block at all).
        let bare = workflow("", &tasks);
        let wf = parse(&bare, FileId::new(0), ParseMode::Strict).expect("bare parses");
        let inferred = infer_permits(&wf);

        // Sound: the inferred block, round-tripped through its own YAML
        // renderer and the real parser, admits every declared effect.
        let with = workflow(&inferred.to_yaml(), &tasks);
        let report = checked(&with);
        let escapes: Vec<String> = report
            .capability_escapes
            .iter()
            .filter(|e| !e.floor)
            .map(|e| e.detail.clone())
            .collect();
        prop_assert!(
            escapes.is_empty(),
            "inference must be sound · moves={:?} · escapes={:?}\ninferred:\n{}",
            moves, escapes, inferred.to_yaml()
        );

        // Tight: no single grant is removable.
        for reduced in drop_one_grant(&inferred.permits) {
            let y = workflow(&render_permits(&reduced), &tasks);
            let r = checked(&y);
            let still_clean = !r.capability_escapes.iter().any(|e| !e.floor);
            prop_assert!(
                !still_clean,
                "inference not tight: a grant was removable · moves={:?}\nreduced:\n{}",
                moves, render_permits(&reduced)
            );
        }
    }
}

// -- the env category is never inferred --------------------------------------

/// The bait an `env:` inference would take: a task whose `exec:` verb carries
/// its OWN `env:` map. That map is AUTHORED VALUES handed to one subprocess —
/// a different plane from `permits.env`, which grants passthrough of the
/// ENGINE's environment. Spec 01 §permits composes a child environment from
/// the two as distinct sources, and spec 02 §exec says it outright: « They are
/// NOT auto-connected ». Reading task env keys as a passthrough need is the
/// most natural wrong move available here, so the property carries it.
const ENV_BAIT_TASK: &str = concat!(
    "  envy:\n",
    "    exec:\n",
    "      command: [\"git\", \"status\"]\n",
    "      env:\n",
    "        CI: \"1\"\n",
    "        NIKA_LOG: \"debug\"\n",
);

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// `env:` is NEVER inferred — spec 01 §permits, normative: « permit
    /// inference MUST NOT invent the list » (NEP-0005 law 7 · LAW-AUTH-0326).
    /// `permits_infer` states the law in a comment above `infer`; until this
    /// property, nothing asserted it, so the first refactor to wire task env
    /// keys into inference would have kept every test green.
    ///
    /// The workflow exercises effects across every INFERABLE family — fs read,
    /// fs write, net, exec, tools — and adds [`ENV_BAIT_TASK`] on top, so the
    /// inferred block is non-trivial in four categories and must still be
    /// silent in the fifth.
    ///
    /// The assertion reads the STRUCT, never the rendered YAML: `render_yaml`
    /// has no `env` arm at all, so a check on its text could not fail whatever
    /// `permits.env` held — a reference that cannot move with the mutant.
    #[test]
    fn inference_never_invents_an_env_category(moves in gen_moves()) {
        let tasks = format!("{}{}", tasks_for(&moves), ENV_BAIT_TASK);
        let wf = parse(&workflow("", &tasks), FileId::new(0), ParseMode::Strict)
            .expect("bait workflow parses");
        let inferred = infer_permits(&wf);
        prop_assert!(
            inferred.permits.env.is_none(),
            "inference invented an env category ({:?}) · the author would paste \
             an environment passthrough they never asked for · moves={:?}",
            inferred.permits.env, moves
        );
    }
}
