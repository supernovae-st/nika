// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What the observed-room suites share: synthetic worlds, a decoy under the process working
//! directory holding the same relative paths, one catalog of every fixture with what it is
//! expected to be (runnable, screened before any room, or refused at admission), and the checks
//! every report must pass.
//!
//! A world is `project/` (the stated files), `scratch/` (where rooms are made) and `secret.txt`
//! beside both. Every path a fixture names starts with the world's own relative prefix, so a
//! decoy with the same relative paths can sit under the process working directory without two
//! tests ever sharing one. A fixture that does not parse strictly, or a runnable one the
//! admission door refuses, makes every test that uses it harness-invalid, never a semantic RED:
//! each suite runs [`assert_catalog_is_well_formed`] first, through the same door the room
//! admits candidates with, in its own exact invocation; its failure is `HARNESS_INVALID`. The
//! first positive's two programs are made by the real compiler, never written here; jq, convert
//! and `extract:` stay outside what a rehearsal runs, so every fixture holding one is screened
//! before any room.
//!
//! Every real host call goes through [`rehearsed`], which leaves one evidence line before the
//! call (the call is engaged, which proves no run) and one after it, before any assertion:
//! identities, the attempt, counts, digests and coverage by index, never a text, a source, a
//! runtime message or a path. A decoy's root is created exclusively and owned from
//! that instant; a root that already exists is never resumed, overwritten or removed.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use nika_compile::surface::assemble::{CopyLowering, assemble_lowered};
use nika_compile::surface::{self, sha256};
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, HotPolicy, compile};
use nika_compile_fidelity::behavior::{
    Budget, Contract, Limits, Requirement, Run, Usage, Verdict, contract_of_request, judge,
};
use nika_compile_reader::{gates, lexicon, shape};
use nika_onboard::compile::rehearse::{
    Attempt, FinalState, Held, Rehearsal, RehearsalReport, Rehearse,
};
use nika_onboard::compile::room::ObservedRoom;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub(crate) const SALES: &str = "client,amount\na,20\nb,30\nc,10\n";
/// The decoy's rows: a read from the working directory instead of the room shows up as these.
pub(crate) const DECOY_SALES: &str = "client,amount\nx,999\ny,998\nz,997\n";
pub(crate) const HIGHEST_FIRST: &str = ".records | sort_by(.amount | tonumber) | reverse | .[:2]";
pub(crate) const TEXT_PLUS_NUMBER: &str = ".records | sort_by(.amount + 1) | reverse | .[:2]";
pub(crate) const ABSENT_FIELD: &str = ".records | map(.montant | tonumber) | .[:2]";
/// Small, harmless if it ever ran, and outside the bounded jq subset by structure.
pub(crate) const OUTSIDE_THE_JQ_SUBSET: &str = "[range(3)]";
/// A registry unit the admission door reads from the project it admits from whenever a
/// workflow's text mentions `mcp:`; in a world, an ordinary observed data file.
pub(crate) const MCP_SERVERS: &str = ".nika/mcp_servers.json";
/// The first positive's source in world A: plain text.
pub(crate) const ALPHA: &str = "alpha\n";
/// The source in world B: a carriage return, a non-ASCII letter and template text kept literal.
pub(crate) const BETA: &str = "beta\r\ncaf\u{e9} ${{ const.not_code }}\n";
/// The decoy's bytes at the copy's source and target: a read or write that resolved against the
/// working directory instead of the room would show them.
pub(crate) const DECOY_SOURCE: &str = "decoy source\n";
pub(crate) const DECOY_TARGET: &str = "decoy target\n";

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A relative prefix no other test of this process uses.
pub(crate) fn unique_prefix() -> String {
    format!(
        "room-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    )
}

/// Every entry under `dir` by relative path: a file's bytes, a symlink's target, a directory
/// as an empty entry ending in `/`, a special file by name only.
pub(crate) fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    if !dir.exists() {
        return out;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let kind = entry.file_type().unwrap();
            let rel = path
                .strip_prefix(dir)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if kind.is_symlink() {
                let target = std::fs::read_link(&path).unwrap();
                out.insert(rel, target.to_string_lossy().into_owned().into_bytes());
            } else if kind.is_dir() {
                out.insert(format!("{rel}/"), Vec::new());
                stack.push(path);
            } else if kind.is_file() {
                out.insert(rel, std::fs::read(&path).unwrap());
            } else {
                out.insert(format!("{rel}!special"), Vec::new());
            }
        }
    }
    out
}

/// A synthetic world with its own relative prefix.
pub(crate) struct World {
    base: tempfile::TempDir,
    prefix: String,
}

impl World {
    /// A world whose `files` (relative to the prefix) are written under `project/`.
    pub(crate) fn new(files: &[(&str, &str)]) -> Self {
        Self::with_prefix(&unique_prefix(), files)
    }

    /// A world under a given prefix (two worlds may share one to prove rooms never cross).
    pub(crate) fn with_prefix(prefix: &str, files: &[(&str, &str)]) -> Self {
        let base = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("project").join(prefix)).unwrap();
        std::fs::create_dir_all(base.path().join("scratch")).unwrap();
        std::fs::write(base.path().join("secret.txt"), "TOP-SECRET").unwrap();
        let world = Self {
            base,
            prefix: prefix.to_owned(),
        };
        for (rel, text) in files {
            world.put(rel, text);
        }
        world
    }

    /// Write `text` at `rel` under the prefix, in the project.
    pub(crate) fn put(&self, rel: &str, text: &str) {
        let path = self.project_path(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Write `text` at `rel` at the project root, outside the prefix.
    pub(crate) fn put_at_root(&self, rel: &str, text: &str) {
        let path = self.project().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    pub(crate) fn base(&self) -> &Path {
        self.base.path()
    }

    /// The world's own relative prefix.
    pub(crate) fn prefix(&self) -> &str {
        &self.prefix
    }

    pub(crate) fn project(&self) -> PathBuf {
        self.base.path().join("project")
    }

    /// The absolute path of `rel` under the prefix, in the project.
    pub(crate) fn project_path(&self, rel: &str) -> PathBuf {
        self.project().join(&self.prefix).join(rel)
    }

    /// The relative path a candidate names for `rel`: `./<prefix>/<rel>`.
    pub(crate) fn path(&self, rel: &str) -> String {
        format!("./{}/{rel}", self.prefix)
    }

    pub(crate) fn scratch(&self) -> PathBuf {
        self.base.path().join("scratch")
    }

    pub(crate) fn room(&self) -> ObservedRoom {
        ObservedRoom::new(self.project()).with_scratch_parent(self.scratch())
    }

    /// Everything under the world: the project, the secret and the scratch parent.
    pub(crate) fn files(&self) -> BTreeMap<String, Vec<u8>> {
        snapshot(self.base.path())
    }

    /// A decoy under the process working directory holding the same relative paths as this
    /// world, with other bytes. Its root is created exclusively and owned from that instant, so
    /// only a root this call made is ever removed; one that already exists is `HARNESS_INVALID`.
    pub(crate) fn decoy(&self, files: &[(&str, &str)]) -> Decoy {
        let root = std::env::current_dir().unwrap().join(&self.prefix);
        let owned = std::fs::create_dir(&root);
        assert!(
            owned.is_ok(),
            "HARNESS_INVALID: the decoy root ./{} is not this test's to create: {owned:?}",
            self.prefix
        );
        let decoy = Decoy { root };
        for (rel, text) in files {
            let path = decoy.root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        decoy
    }
}

/// Files under the process working directory at a world's relative paths, under a root this
/// test created and owns: removed when dropped.
pub(crate) struct Decoy {
    root: PathBuf,
}

impl Decoy {
    pub(crate) fn files(&self) -> BTreeMap<String, Vec<u8>> {
        snapshot(&self.root)
    }
}

impl Drop for Decoy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// ─── the fixtures ────────────────────────────────────────────────────────────────────────

/// A candidate reading `read`, transforming with `expression`, writing `write`, granted
/// `read_grant` and `write_grant`.
pub(crate) fn candidate_granted(
    read: &str,
    expression: &str,
    write: &str,
    read_grant: &str,
    write_grant: &str,
) -> String {
    format!(
        r#"nika: top-two
permits:
  tools: ["nika:read", "nika:convert", "nika:jq", "nika:write"]
  fs:
    read: ["{read_grant}"]
    write: ["{write_grant}"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args: {{ path: "{read}" }}
  parse_source:
    with: {{ document: "${{{{ tasks.read_source.output }}}}" }}
    invoke:
      tool: "nika:convert"
      args: {{ input: "${{{{ with.document }}}}", from: csv, to: json }}
  compute:
    with: {{ records: "${{{{ tasks.parse_source.output }}}}" }}
    invoke:
      tool: "nika:jq"
      args:
        input: {{ records: "${{{{ with.records }}}}" }}
        expression: '{expression}'
  write_output:
    with: {{ content: "${{{{ tasks.compute.output }}}}" }}
    invoke:
      tool: "nika:write"
      args: {{ path: "{write}", content: "${{{{ with.content }}}}", overwrite: true, create_dirs: true }}
"#
    )
}

/// A candidate granted exactly the paths it names.
pub(crate) fn candidate(read: &str, expression: &str, write: &str) -> String {
    candidate_granted(read, expression, write, read, write)
}

/// The ranking candidate over the world's sales file.
pub(crate) fn ranking(world: &World, expression: &str) -> String {
    candidate(
        &world.path("data/sales.csv"),
        expression,
        &world.path("out/top.json"),
    )
}

/// A write of a literal note, reading nothing.
pub(crate) fn write_only(world: &World) -> String {
    let note = world.path("out/note.txt");
    format!(
        "nika: note\npermits:\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"{note}\"]\ntasks:\n  put:\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{note}\", content: hello, create_dirs: true }} }}\n"
    )
}

/// The same write with no `permits:` block: zero authority, refused at admission.
pub(crate) fn without_permits(world: &World) -> String {
    let note = world.path("out/note.txt");
    format!(
        "nika: note\ntasks:\n  put:\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{note}\", content: hello }} }}\n"
    )
}

/// A write whose target jq computes at run time, granted `./<prefix>/out/*`.
pub(crate) fn dynamic_target(world: &World, jq_target: &str) -> String {
    let sales = world.path("data/sales.csv");
    let grant = world.path("out/*");
    format!(
        r#"nika: computed-target
permits:
  tools: ["nika:read", "nika:jq", "nika:write"]
  fs:
    read: ["{sales}"]
    write: ["{grant}"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args: {{ path: "{sales}" }}
  target:
    with: {{ text: "${{{{ tasks.read_source.output }}}}" }}
    invoke:
      tool: "nika:jq"
      args: {{ input: {{ text: "${{{{ with.text }}}}" }}, expression: '{jq_target}' }}
  write_output:
    with: {{ target: "${{{{ tasks.target.output }}}}" }}
    invoke:
      tool: "nika:write"
      args: {{ path: "${{{{ with.target }}}}", content: x, create_dirs: true }}
"#
    )
}

/// A copy guarded by a condition the default input makes false.
pub(crate) fn conditional_copy(world: &World) -> String {
    let (sales, copy) = (world.path("data/sales.csv"), world.path("out/copy.csv"));
    format!(
        "nika: maybe-copy\ninputs:\n  publish: {{ type: bool, default: false }}\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"{sales}\"]\n    write: [\"{copy}\"]\ntasks:\n  read_source:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{sales}\" }} }}\n  write_copy:\n    when: \"${{{{ inputs.publish == true }}}}\"\n    with: {{ content: \"${{{{ tasks.read_source.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{copy}\", content: \"${{{{ with.content }}}}\", create_dirs: true }} }}\n"
    )
}

/// A ranking whose failure is recovered by skipping, so the unconditional write that consumes
/// it is skipped too: the run completes, the required output was never written.
pub(crate) fn recovered_then_skipped(world: &World) -> String {
    ranking(world, TEXT_PLUS_NUMBER)
        .replace("  compute:\n", "  compute:\n    on_error: { skip: true }\n")
}

/// A candidate that waits `pause`, then writes `./<prefix>/out/late.txt`.
pub(crate) fn slow(world: &World, pause: &str) -> String {
    let late = world.path("out/late.txt");
    format!(
        "nika: slow\npermits:\n  tools: [\"nika:wait\", \"nika:write\"]\n  fs:\n    write: [\"{late}\"]\ntasks:\n  pause:\n    invoke: {{ tool: \"nika:wait\", args: {{ duration: \"{pause}\" }} }}\n  put:\n    after: {{ pause: success }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{late}\", content: late, create_dirs: true }} }}\n"
    )
}

/// A read of the data file sharing the candidate's logical admission name, copied to an output.
pub(crate) fn reads_the_logical_name(world: &World) -> String {
    let (name, out) = (
        format!("./{}", ObservedRoom::LOGICAL_ROOT),
        world.path("out/copy.txt"),
    );
    format!(
        "nika: reader\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"{name}\"]\n    write: [\"{out}\"]\ntasks:\n  read_it:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{name}\" }} }}\n  copy_it:\n    with: {{ content: \"${{{{ tasks.read_it.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{out}\", content: \"${{{{ with.content }}}}\", create_dirs: true }} }}\n"
    )
}

/// A write of data at the candidate's logical admission name.
pub(crate) fn writes_the_logical_name() -> String {
    let name = format!("./{}", ObservedRoom::LOGICAL_ROOT);
    format!(
        "nika: writer\npermits:\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"{name}\"]\ntasks:\n  put:\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{name}\", content: data, overwrite: true }} }}\n"
    )
}

pub(crate) fn fetch(url: &str) -> String {
    format!(
        "nika: f\npermits:\n  tools: [\"nika:fetch\"]\n  net: {{ http: [\"127.0.0.1\"] }}\ntasks:\n  get:\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{url}\", mode: text }} }}\n"
    )
}

pub(crate) const INFER: &str = "nika: i\nmodel: mock/echo\npermits: {}\ntasks:\n  say:\n    infer: { prompt: hi, max_tokens: 8 }\n";
pub(crate) const AGENT: &str = "nika: a\nmodel: mock/echo\npermits: {}\ntasks:\n  act:\n    agent: { prompt: hi, max_turns: 1 }\n";
pub(crate) const MCP: &str = "nika: m\npermits: { tools: [\"mcp:files/write\"] }\ntasks:\n  put:\n    invoke: { tool: \"mcp:files/write\", args: { path: x } }\n";
pub(crate) const NOTIFY: &str = "nika: n\npermits: { tools: [\"nika:notify\"] }\ntasks:\n  tell:\n    invoke: { tool: \"nika:notify\", args: { message: hi } }\n";
pub(crate) const SECRET: &str = "nika: s\nsecrets:\n  token: { source: env, key: SOME_TOKEN }\npermits: { tools: [\"nika:log\"] }\ntasks:\n  say:\n    invoke: { tool: \"nika:log\", args: { message: \"${{ secrets.token }}\" } }\n";
pub(crate) const UNKNOWN_TOOL: &str = "nika: u\npermits: { tools: [\"nika:frobnicate\"] }\ntasks:\n  go:\n    invoke: { tool: \"nika:frobnicate\", args: {} }\n";
pub(crate) const TEMPLATED_TOOL: &str = "nika: t\ninputs:\n  tool: { type: string, default: \"nika:read\" }\npermits: { tools: [\"nika:read\"] }\ntasks:\n  go:\n    invoke: { tool: \"${{ inputs.tool }}\", args: { path: x } }\n";

pub(crate) fn generate(tool: &str) -> String {
    format!(
        "nika: g\npermits: {{ tools: [\"{tool}\"] }}\ntasks:\n  make:\n    invoke: {{ tool: \"{tool}\", args: {{ prompt: hi }} }}\n"
    )
}

pub(crate) fn exec_touching(sentinel: &Path) -> String {
    format!(
        "nika: e\npermits: {{ exec: [\"touch\"] }}\ntasks:\n  run:\n    exec: {{ command: [\"touch\", \"{}\"] }}\n",
        sentinel.display()
    )
}

pub(crate) fn prompt_gate(world: &World) -> String {
    let ok = world.path("out/ok.txt");
    format!(
        "nika: p\npermits:\n  tools: [\"nika:prompt\", \"nika:write\"]\n  fs:\n    write: [\"{ok}\"]\ntasks:\n  ask:\n    invoke: {{ tool: \"nika:prompt\", args: {{ message: \"Proceed?\" }} }}\n  put:\n    with: {{ ok: \"${{{{ tasks.ask.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{ok}\", content: \"${{{{ with.ok }}}}\", create_dirs: true }} }}\n"
    )
}

pub(crate) fn nested_parent(world: &World) -> String {
    let child = world.path("child.nika");
    format!(
        "nika: parent\npermits: {{}}\ntasks:\n  child:\n    invoke: {{ workflow: \"{child}\" }}\n"
    )
}

/// A clean read followed by `branch`, a second task holding a network effect.
pub(crate) fn with_branch(world: &World, url: &str, branch: &str) -> String {
    let sales = world.path("data/sales.csv");
    format!(
        "nika: b\npermits:\n  tools: [\"nika:read\", \"nika:fetch\"]\n  fs:\n    read: [\"{sales}\"]\n  net: {{ http: [\"127.0.0.1\"] }}\ntasks:\n  a:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{sales}\" }} }}\n  b:\n{branch}    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{url}\", mode: text }} }}\n"
    )
}

/// A read of `from` written verbatim to `to`: no jq, no convert.
pub(crate) fn copy_file(world: &World, from: &str, to: &str) -> String {
    let (from, to) = (world.path(from), world.path(to));
    format!(
        "nika: copy\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"{from}\"]\n    write: [\"{to}\"]\ntasks:\n  read_it:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{from}\" }} }}\n  write_it:\n    with: {{ content: \"${{{{ tasks.read_it.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{to}\", content: \"${{{{ with.content }}}}\", create_dirs: true, overwrite: true }} }}\n"
    )
}

/// A copy of `from` to `to` as given, granted `read_grant` and `write_grant`.
pub(crate) fn copy_granted(from: &str, to: &str, read_grant: &str, write_grant: &str) -> String {
    format!(
        "nika: copy\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"{read_grant}\"]\n    write: [\"{write_grant}\"]\ntasks:\n  read_it:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{from}\" }} }}\n  write_it:\n    with: {{ content: \"${{{{ tasks.read_it.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{to}\", content: \"${{{{ with.content }}}}\", create_dirs: true, overwrite: true }} }}\n"
    )
}

/// A copy of literal paths, granted exactly those.
pub(crate) fn copy_raw(from: &str, to: &str) -> String {
    copy_granted(from, to, from, to)
}

/// The first positive's request over `prefix`.
pub(crate) fn copy_intent(prefix: &str) -> String {
    format!("Copy ./{prefix}/in/source.txt as is to ./{prefix}/out/copied.txt")
}

/// The copy's two programs, each made once by the real compiler.
pub(crate) struct Lowered {
    /// The text lowering: `compile()`'s own candidate.
    pub(crate) text: String,
    /// The byte lowering: the same plan's read as an opaque envelope the write decodes.
    pub(crate) bytes: String,
}

/// The copy's two candidates over `prefix`. The text lowering is `compile()`'s candidate; the
/// byte lowering is the public `assemble_lowered` over the reader's plan, which the strict door
/// admits, and the same plan under `CopyLowering::Text` must equal `compile()`'s candidate byte
/// for byte. Each is Ready with a clean Check, or the reason why not.
pub(crate) fn lowered_copy(prefix: &str) -> Result<Lowered, String> {
    let intent = copy_intent(prefix);
    let compiled =
        compile(&CompileRequest::create(&intent)).map_err(|error| format!("{error:?}"))?;
    let text = ready(&compiled, "compile()")?;
    let folded = lexicon::fold_apostrophes(&intent);
    let mut reading = lexicon::read(&folded);
    gates::backstop(&folded, &mut reading.plan);
    shape::promote_stated_rules(&mut reading.plan, &folded);
    surface::admit_hot(&folded, &reading, HotPolicy::Strict)
        .map_err(|refusal| format!("the strict door refused the copy: {refusal}"))?;
    let assembled = |lowering: CopyLowering| -> Result<CompileOutcome, String> {
        let mut out = surface::initial();
        let request = CompileRequest::create(&folded);
        assemble_lowered(
            &reading.plan,
            &folded,
            &request,
            &[],
            false,
            lowering,
            &mut out,
        )
        .map_err(|error| format!("{error:?}"))?;
        Ok(out)
    };
    if ready(&assembled(CopyLowering::Text)?, "the text lowering")? != text {
        return Err("the text lowering is not compile()'s candidate".to_owned());
    }
    let bytes = ready(&assembled(CopyLowering::Bytes)?, "the byte lowering")?;
    Ok(Lowered { text, bytes })
}

/// The candidate of a Ready outcome whose Check is clean, or why not.
fn ready(out: &CompileOutcome, what: &str) -> Result<String, String> {
    let clean = out
        .check_preview
        .as_ref()
        .is_some_and(|preview| preview.report.is_clean());
    match (&out.status, &out.candidate) {
        (CompileStatus::Ready, Some(candidate)) if clean => Ok(candidate.clone()),
        (status, _) => Err(format!("{what}: {status:?}, check clean {clean}")),
    }
}

/// The copy's request contract over `prefix`, read by the behavioural judge's own reader.
pub(crate) fn copy_contract(prefix: &str) -> Contract {
    contract_of_request(&copy_intent(prefix), &BTreeMap::new())
}

/// Whether `contract` requires the text copy of the copy's source at its target.
pub(crate) fn requires_copy(contract: &Contract, prefix: &str) -> bool {
    let (source, target) = (
        format!("./{prefix}/in/source.txt"),
        format!("./{prefix}/out/copied.txt"),
    );
    contract.obligations.iter().any(|obligation| {
        matches!(&obligation.requirement, Requirement::CopyText { source: from } if *from == source)
            && obligation
                .target
                .as_ref()
                .is_some_and(|written| written.path == target)
    })
}

/// The real judgment of `runs` against `contract`.
pub(crate) fn verdict(contract: &Contract, runs: &[Run]) -> Verdict {
    let mut budget = Budget::new(
        Limits::new(16, 16, 10_000_000, 600_000),
        Limits::new(64, 64, 40_000_000, 2_400_000),
        Usage::default(),
    );
    judge(contract, runs, &mut budget).verdict()
}

/// A reader of the copy's source that publishes nothing.
pub(crate) fn no_op(world: &World) -> String {
    let source = world.path("in/source.txt");
    format!(
        "nika: idle\npermits:\n  tools: [\"nika:read\"]\n  fs:\n    read: [\"{source}\"]\ntasks:\n  read_it:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{source}\" }} }}\n"
    )
}

/// A writer without its own `when` proceeds only after its reader succeeds.
/// The default input skips the reader, so the control edge cancels the writer
/// before it binds the null output. Its required publication remains missing.
pub(crate) fn gated_source(world: &World) -> String {
    let (sales, copy) = (world.path("data/sales.csv"), world.path("out/copy.csv"));
    format!(
        "nika: gated-source\ninputs:\n  publish: {{ type: bool, default: false }}\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"{sales}\"]\n    write: [\"{copy}\"]\ntasks:\n  read_source:\n    when: \"${{{{ inputs.publish == true }}}}\"\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{sales}\" }} }}\n  write_copy:\n    after: {{ read_source: success }}\n    with: {{ content: \"${{{{ tasks.read_source.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{copy}\", content: \"${{{{ with.content }}}}\", create_dirs: true }} }}\n"
    )
}

/// A copy of the world's sales file to a target only the run knows: an input whose default is
/// `target`, under a broad write grant. Check re-gates an untrusted input's resolved default
/// against the step permit (NEP-0004 law 2), so the writing step lifts the taint law on
/// `inputs.target`, here and nowhere else: the value is trusted for that static re-gate only,
/// never a permit bypass. The run starts with the target; the write's own fs boundary and the
/// room must refuse one outside, and nothing may leave.
pub(crate) fn computed_target(world: &World, target: &str) -> String {
    let sales = world.path("data/sales.csv");
    format!(
        "nika: computed-target\ninputs:\n  target: {{ type: string, default: \"{target}\" }}\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"{sales}\"]\n    write: [\"**\"]\ntasks:\n  read_source:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{sales}\" }} }}\n  write_output:\n    with: {{ content: \"${{{{ tasks.read_source.output }}}}\" }}\n    lift: [{{ law: taint, from: inputs.target, because: \"a rehearsal witness: the run meets this target and its fs must refuse it\" }}]\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"${{{{ inputs.target }}}}\", content: \"${{{{ with.content }}}}\", create_dirs: true }} }}\n"
    )
}

/// A note whose text mentions `mcp:` in a plain string: no MCP tool, but the text alone makes
/// the admission door look for a registry in the project it admits from.
pub(crate) fn mentions_mcp(world: &World) -> String {
    let (sales, note) = (world.path("data/sales.csv"), world.path("out/note.txt"));
    format!(
        "nika: mentions\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"{sales}\"]\n    write: [\"{note}\"]\ntasks:\n  read_it:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{sales}\" }} }}\n  note_it:\n    with: {{ rows: \"${{{{ tasks.read_it.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{note}\", content: \"mcp: ${{{{ with.rows }}}}\", create_dirs: true }} }}\n"
    )
}

/// A ranking whose jq expression rides a template instead of a literal.
pub(crate) fn templated_jq(world: &World) -> String {
    ranking(world, HIGHEST_FIRST)
        .replace(
            &format!("expression: '{HIGHEST_FIRST}'"),
            "expression: \"${{ inputs.expr }}\"",
        )
        .replace(
            "nika: top-two\n",
            &format!(
                "nika: top-two\ninputs:\n  expr: {{ type: string, default: \"{HIGHEST_FIRST}\" }}\n"
            ),
        )
}

// ─── the catalog and its harness guard ────────────────────────────────────────────────────

/// What a fixture is expected to be before any room exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Expect {
    /// Parses and passes Check: the room may run it.
    Runs,
    /// Parses; the room's screen refuses it before any room exists.
    Screened,
    /// Parses, and Check refuses it: the room refuses it at admission.
    AdmissionRefused,
    /// Strict parsing refuses it with this spec code, and so does the admission door: the room
    /// refuses it as it reads it, before its screen.
    ParseRefused(&'static str),
}

pub(crate) struct Fixture {
    pub(crate) name: &'static str,
    pub(crate) source: String,
    pub(crate) expect: Expect,
}

fn fixture(name: &'static str, source: String, expect: Expect) -> Fixture {
    Fixture {
        name,
        source,
        expect,
    }
}

/// Every fixture the suites use, built over `world` (the network ones name `url`).
pub(crate) fn catalog(world: &World, url: &str) -> Vec<Fixture> {
    let mut every = copy_fixtures(world);
    every.extend(other_fixtures(world, url));
    every
}

/// The copy's two lowerings and its idle reader, then the copies and literal paths the
/// confinement suite runs or expects screened.
fn copy_fixtures(world: &World) -> Vec<Fixture> {
    use Expect::{Runs, Screened};
    let (text, bytes) = match lowered_copy(world.prefix()) {
        Ok(lowered) => (lowered.text, lowered.bytes),
        Err(why) => (format!("not READY: {why}"), format!("not READY: {why}")),
    };
    let escape = world.base().join("escape.json").display().to_string();
    let sales = world.path("data/sales.csv");
    vec![
        fixture("copy, text lowering", text, Runs),
        fixture("copy, byte lowering", bytes, Runs),
        fixture("no-op reader", no_op(world), Runs),
        fixture(
            "computed target, absolute",
            computed_target(world, &escape),
            Runs,
        ),
        fixture(
            "computed target, traversal",
            computed_target(world, "./out/../../../../escape.json"),
            Runs,
        ),
        fixture(
            "computed target, parent",
            computed_target(world, "../escape.json"),
            Runs,
        ),
        fixture("copy onto itself", copy_raw(&sales, &sales), Runs),
        fixture(
            "granted copy",
            copy_granted(&sales, &world.path("out/top.csv"), "**", "**"),
            Runs,
        ),
        fixture(
            "copy, renamed",
            copy_file(world, "data/sales.csv", "out/copy.csv")
                .replace("nika: copy", "nika: copy-again"),
            Runs,
        ),
        fixture(
            "literal write outside",
            copy_raw(&sales, "../escape.json"),
            Screened,
        ),
        fixture(
            "literal read outside",
            copy_raw("../secret.txt", &world.path("out/top.json")),
            Screened,
        ),
        fixture("absolute literal", copy_raw(&sales, &escape), Screened),
    ]
}

/// The suites' other fixtures (the network ones name `url`).
fn other_fixtures(world: &World, url: &str) -> Vec<Fixture> {
    use Expect::{AdmissionRefused, ParseRefused, Runs, Screened};
    let sentinel = world.base().join("spawned");
    vec![
        fixture("gated source", gated_source(world), Runs),
        fixture(
            "computed target",
            computed_target(world, "./out/x.json"),
            Runs,
        ),
        fixture("ranking", ranking(world, HIGHEST_FIRST), Screened),
        fixture(
            "text plus number",
            ranking(world, TEXT_PLUS_NUMBER),
            Screened,
        ),
        fixture("absent field", ranking(world, ABSENT_FIELD), Screened),
        fixture("write only", write_only(world), Runs),
        fixture("without permits", without_permits(world), AdmissionRefused),
        fixture(
            "dynamic target",
            dynamic_target(world, "\"./out/x.json\""),
            Screened,
        ),
        fixture("conditional copy", conditional_copy(world), Runs),
        fixture(
            "recovered then skipped",
            recovered_then_skipped(world),
            Screened,
        ),
        fixture("slow", slow(world, "2s"), Runs),
        fixture(
            "copy file",
            copy_file(world, "data/sales.csv", "out/copy.txt"),
            Runs,
        ),
        fixture(
            "reads the logical name",
            reads_the_logical_name(world),
            Runs,
        ),
        fixture("writes the logical name", writes_the_logical_name(), Runs),
        fixture("mentions mcp", mentions_mcp(world), Runs),
        fixture(
            "jq outside the subset",
            ranking(world, OUTSIDE_THE_JQ_SUBSET),
            Screened,
        ),
        fixture("templated jq", templated_jq(world), Screened),
        fixture("fetch", fetch(url), Screened),
        fixture("infer", INFER.to_owned(), Screened),
        fixture("agent", AGENT.to_owned(), Screened),
        fixture("exec", exec_touching(&sentinel), Screened),
        fixture("mcp", MCP.to_owned(), Screened),
        fixture("notify", NOTIFY.to_owned(), Screened),
        fixture("image", generate("nika:image_generate"), Screened),
        fixture("speech", generate("nika:tts_generate"), Screened),
        fixture("prompt", prompt_gate(world), Screened),
        fixture("secret", SECRET.to_owned(), Screened),
        fixture("nested", nested_parent(world), Screened),
        fixture("unknown tool", UNKNOWN_TOOL.to_owned(), Screened),
        fixture(
            "templated tool",
            TEMPLATED_TOOL.to_owned(),
            ParseRefused("NIKA-PARSE-019"),
        ),
        fixture(
            "unwind branch",
            with_branch(world, url, "    after: { a: unwind }\n"),
            Screened,
        ),
        fixture(
            "failure branch",
            with_branch(world, url, "    after: { a: failure }\n"),
            Screened,
        ),
        fixture(
            "false condition",
            with_branch(world, url, "    when: \"${{ false }}\"\n"),
            Screened,
        ),
        fixture(
            "fan-out",
            with_branch(world, url, "    for_each: { items: [1, 2] }\n"),
            Screened,
        ),
    ]
}

/// The admission door's verdict on `source`, admitted from its bytes under the room's logical
/// name over a fresh empty project, the way the room admits a candidate: the admitted world's
/// digest, or the refusal in words.
pub(crate) fn door_admits(source: &str) -> Result<String, String> {
    let empty = tempfile::tempdir().map_err(|error| error.to_string())?;
    let project = nika_fs::OwnedDir::open(empty.path()).map_err(|error| error.to_string())?;
    nika_execution::ExecutionService::default()
        .admit_root_bytes(
            &project,
            Path::new(ObservedRoom::LOGICAL_ROOT),
            source.as_bytes(),
        )
        .map(|admitted| admitted.snapshot().digest().to_owned())
        .map_err(|refusal| format!("{refusal:?}"))
}

/// The digest of the world holding `source` alone, as the door admits it.
pub(crate) fn admitted_digest_of(source: &str) -> String {
    door_admits(source).expect("HARNESS_INVALID: a runnable fixture is admitted")
}

/// The harness guard: every fixture parses strictly, except one the parser refuses by its
/// stated code; the admission door admits a runnable one and refuses one the room refuses at
/// admission or as it reads it. A failure here is `HARNESS_INVALID` for the tests using that
/// fixture, never a semantic RED; it calls no runtime.
pub(crate) fn assert_catalog_is_well_formed(world: &World) {
    let mut invalid = Vec::new();
    for item in catalog(world, "http://127.0.0.1:9/") {
        let parsed = nika_schema::parse(
            &item.source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        );
        let admitted = door_admits(&item.source);
        let well_formed = match item.expect {
            Expect::Runs => parsed.is_ok() && admitted.is_ok(),
            Expect::Screened => parsed.is_ok(),
            Expect::AdmissionRefused => parsed.is_ok() && admitted.is_err(),
            Expect::ParseRefused(code) => {
                parsed
                    .as_ref()
                    .is_err_and(|error| error.spec_code().to_string() == code)
                    && admitted.is_err()
            }
        };
        if !well_formed {
            invalid.push(format!(
                "{} ({:?}): parsed {}, admitted {:?}",
                item.name,
                item.expect,
                parsed.is_ok(),
                admitted
            ));
        }
    }
    assert!(invalid.is_empty(), "HARNESS_INVALID fixtures: {invalid:#?}");
}

/// Paths made read-only for one test, their modes restored when dropped, even on a failure,
/// so the world can still be deleted.
pub(crate) struct Protected(Vec<(PathBuf, u32)>);

#[cfg(unix)]
impl Protected {
    /// Make every directory 0o555 and every file 0o444, remembering each mode.
    pub(crate) fn new(paths: &[PathBuf]) -> Self {
        use std::os::unix::fs::PermissionsExt as _;
        let mut kept = Vec::new();
        for path in paths {
            let meta = std::fs::metadata(path).unwrap();
            kept.push((path.clone(), meta.permissions().mode() & 0o7777));
            let mode = if meta.is_dir() { 0o555 } else { 0o444 };
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        Self(kept)
    }
}

#[cfg(unix)]
impl Drop for Protected {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt as _;
        for (path, mode) in self.0.iter().rev() {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(*mode));
        }
    }
}

// ─── the receipts ─────────────────────────────────────────────────────────────────────────

/// One evidence line, captured by the test harness like any test output (`--nocapture` shows
/// it at once).
#[expect(
    clippy::disallowed_macros,
    clippy::print_stdout,
    reason = "the test-only evidence line the qualification runner reads from the test output"
)]
fn receipt(line: &Value) {
    println!("NIKA_REHEARSAL_EVIDENCE_V1 {line}");
}

/// The candidate as a call hands it over: its sha256 and its length, never its text.
fn handed(candidate: &str) -> Value {
    json!({"sha256": sha256(candidate), "bytes": candidate.len()})
}

/// Rehearse `candidate` over `inputs` and `targets` in `room`, as `subrun` of `test`. A `begin`
/// line precedes the call: the call is engaged, which proves no run. A `return` line follows it,
/// before the caller asserts anything: a run is what its attempt says.
pub(crate) async fn rehearsed(
    test: &str,
    subrun: &str,
    room: &ObservedRoom,
    candidate: &str,
    inputs: &[String],
    targets: &[String],
) -> RehearsalReport {
    receipt(&json!({
        "event": "begin", "test": test, "subrun": subrun, "candidate": handed(candidate),
    }));
    let report = room.rehearse_reading(candidate, inputs, targets).await;
    receipt(&json!({
        "event": "return", "test": test, "subrun": subrun, "candidate": handed(candidate),
        "report": projection(&report),
    }));
    report
}

/// What a report shows, by index: no text, source, runtime message or path.
fn projection(report: &RehearsalReport) -> Value {
    let (attempt, elapsed_ms) = match report.attempt {
        Attempt::NeverAttempted => ("never", None),
        Attempt::Completed { elapsed_ms } => ("completed", Some(elapsed_ms)),
        Attempt::Stopped { elapsed_ms } => ("stopped", Some(elapsed_ms)),
        _ => ("unknown", None),
    };
    let observation = &report.observation;
    let (effects, ledger) = (&report.effects, &observation.ledger);
    json!({
        "candidate_sha256": report.candidate_sha256,
        "admitted_digest": (!report.admitted_digest.is_empty()).then_some(&report.admitted_digest),
        "attempt": attempt,
        "elapsed_ms": elapsed_ms,
        "outcome": outcome(&report.outcome),
        "refusal": observation.refusal.map(|refusal| format!("{refusal:?}")),
        "room": {
            "prepared": report.room.prepared, "cleaned": report.room.cleaned,
            "late_refused": report.room.late_refused,
        },
        "denied": {
            "network": effects.network, "provider": effects.provider, "spawn": effects.spawn,
            "prompt": effects.prompt, "secret": effects.secret, "child": effects.child,
        },
        "ledger": {
            "written": ledger.written.len(), "late_refused": ledger.late_refused,
            "leftovers": ledger.leftovers, "panicked": ledger.panicked, "drained": ledger.drained,
        },
        "copies": observation.copies.iter().map(|copy| json!({
            "source": digest(&copy.source), "room": copy.room.as_ref().map(digest),
            "coverage": coverage(&copy.held),
        })).collect::<Vec<_>>(),
        "finals": observation.finals.iter().map(|read| state(&read.state)).collect::<Vec<_>>(),
        "bounds": {
            "time_ms": observation.bounds.time_ms, "room_bytes": observation.bounds.room_bytes,
            "preview_bytes": observation.bounds.preview_bytes,
        },
    })
}

/// The outcome's category, with the failing task and its code, or the outputs by index.
fn outcome(outcome: &Rehearsal) -> Value {
    match outcome {
        Rehearsal::Passed { outputs } => json!({
            "category": "passed",
            "outputs": outputs.iter().map(|output| json!({
                "written": output.written, "truncated": output.truncated,
                "full_bytes": output.full_bytes, "full_sha256": output.full_sha256,
            })).collect::<Vec<_>>(),
        }),
        Rehearsal::Failed { code, task, .. } => {
            json!({"category": "failed", "task": task, "code": code})
        }
        Rehearsal::Missing { outputs } => json!({"category": "missing", "outputs": outputs.len()}),
        Rehearsal::NotRun { .. } => json!({"category": "not_run"}),
        _ => json!({"category": "unknown"}),
    }
}

fn digest(digest: &nika_onboard::compile::rehearse::Digest) -> Value {
    json!({"bytes": digest.bytes, "sha256": digest.sha256})
}

fn coverage(held: &Held) -> &'static str {
    match held {
        Held::Whole(_) => "whole",
        Held::Preview(_) => "preview",
        Held::NotText => "not_text",
        _ => "unknown",
    }
}

fn state(state: &FinalState) -> Value {
    match state {
        FinalState::Absent => json!({"state": "absent"}),
        FinalState::Directory => json!({"state": "directory"}),
        FinalState::File {
            digest: bytes,
            held,
        } => {
            json!({"state": "file", "digest": digest(bytes), "coverage": coverage(held)})
        }
        FinalState::Unreadable => json!({"state": "unreadable"}),
        _ => json!({"state": "unknown"}),
    }
}

// ─── the checks ───────────────────────────────────────────────────────────────────────────

/// The observed input list of a world's sales file.
pub(crate) fn sales_input(world: &World) -> Vec<String> {
    vec![world.path("data/sales.csv")]
}

/// The paths the run published, as the room's ledger recorded them.
pub(crate) fn published(report: &RehearsalReport) -> &[String] {
    &report.observation.ledger.written
}

/// What the room held at `path` once the run ended, when it was text held whole.
pub(crate) fn final_text<'a>(report: &'a RehearsalReport, path: &str) -> Option<&'a str> {
    report
        .observation
        .finals
        .iter()
        .find(|read| read.path == path)
        .and_then(|read| match &read.state {
            FinalState::File {
                held: Held::Whole(text),
                ..
            } => Some(text.as_str()),
            _ => None,
        })
}

/// Whether a report is bound to exactly `candidate`.
pub(crate) fn bound_to(report: &RehearsalReport, candidate: &str) -> bool {
    report.candidate_sha256 == sha256(candidate)
}

pub(crate) fn not_run_because(report: &RehearsalReport, word: &str) -> bool {
    matches!(&report.outcome, Rehearsal::NotRun { reason } if reason.contains(word))
}

/// No run began.
pub(crate) fn never_attempted(report: &RehearsalReport) -> bool {
    matches!(report.attempt, Attempt::NeverAttempted)
}

pub(crate) fn completed(report: &RehearsalReport) -> bool {
    matches!(report.attempt, Attempt::Completed { .. })
}

/// Refused before any room: the named reason, no run, no room prepared, nothing left, no
/// effect counted.
pub(crate) fn refused_before_any_room(report: &RehearsalReport, word: &str) -> bool {
    not_run_because(report, word)
        && never_attempted(report)
        && !report.room.prepared
        && report.room.cleaned
        && report.effects.is_none()
}

/// The first output's text parsed as JSON, when the run passed and the preview is whole.
pub(crate) fn first_output(report: &RehearsalReport) -> Option<Value> {
    let Rehearsal::Passed { outputs } = &report.outcome else {
        return None;
    };
    let first = outputs.first()?;
    if first.truncated {
        return None;
    }
    serde_json::from_str(&first.text).ok()
}

/// The rows the ranking contract requires from [`SALES`]: the two highest amounts, highest
/// first, each row whole.
pub(crate) fn two_highest() -> Value {
    serde_json::json!([
        {"client": "b", "amount": "30"},
        {"client": "a", "amount": "20"}
    ])
}
