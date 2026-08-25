// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as check_run_equivalence: this suite's WHOLE JOB is the
// real binary, and the per-entry lines + the ran/green/known-fail/skipped
// tally are the suite's DELIVERABLE (the law: a skip is printed and
// counted, never silent).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::print_stdout
)]

//! The examples FAMILY traversal — every shipped example is LAUNCHED, not
//! just listed.
//!
//! `pack_templates_family.rs` is the sibling for the 14 skeletons; this is
//! the 53-example half. The R1 corpus sweep (2026-08-25) measured the dead
//! angle: 50 of 53 examples were launched by NO gate — and 8 of them were
//! red through the showroom door itself, invisible because nothing ran
//! them. A listing test (`verbs_static.rs` proves `try` NAMES each slug)
//! is not a run; `nika check` green proves the audit, never the run.
//!
//! Every row of `pack_examples_family.manifest.yaml` (checked in beside
//! this file — the pack tree itself is spec-pinned, so the manifest can
//! not live inside it) drives the real binary, hermetically:
//!
//! - `door: try` (default) goes through `nika try <slug>` — the shipped
//!   door, offline mock seat by default, fixtures materialized by the door.
//! - `door: run` stages a scratch cwd per the row's `kit:` and goes through
//!   `nika run --no-progress --max-cost-usd 0.01` — required for the human
//!   gates (`try` carries no `--answer`).
//! - `HOME`/`TMPDIR` are isolated per entry, `NIKA_*` inherited vars are
//!   scrubbed, the only model the gate may name is `mock/echo`.
//! - A `known_fail` row is an XFAIL: on the listed OSes the example RUNS
//!   and must stay red — a green there means the wound healed and the gate
//!   refuses to keep lying about it. A `needs:` row is the honest skip:
//!   launched never, printed and counted always.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

/// The checked-in manifest, compiled in — the test and its contract are
/// one artifact (a manifest edit without the test is nothing; the
/// completeness law below makes the reverse drift a failure too).
const MANIFEST_YAML: &str = include_str!("pack_examples_family.manifest.yaml");

/// Per-entry run ceiling, seconds (worst measured in the sweep: 31 s).
const MAX_TIMEOUT_SECS: u64 = 120;

/// The one model this gate may name — zero keys, zero network, zero
/// keychain. A row naming anything else is a hermeticity breach.
const HERMETIC_MODEL: &str = "mock/echo";

// ── The manifest schema ───────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    /// class → issue token (the tracker row the known-fail is bound to).
    issues: BTreeMap<String, String>,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    slug: String,
    /// `try` (the shipped showroom door) or `run` (staged scratch cwd).
    #[serde(default)]
    door: Door,
    /// What the harness stages for `door: run`.
    #[serde(default)]
    kit: Vec<Kit>,
    /// Pre-seeded gate answers (`--answer approve=true`).
    #[serde(default)]
    answers: BTreeMap<String, String>,
    /// Workflow inputs (`--var k=v`).
    #[serde(default)]
    vars: BTreeMap<String, String>,
    /// Only ever `mock/echo` — enforced below.
    model: Option<String>,
    /// Seconds; ceiling [`MAX_TIMEOUT_SECS`].
    timeout: Option<u64>,
    expect: Expect,
    /// XFAIL: on these OSes the run must stay red (green = healed = fail).
    known_fail: Option<KnownFail>,
    /// The honest skip — never launched, always counted.
    needs: Option<Need>,
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Door {
    #[default]
    Try,
    Run,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Kit {
    Fixtures,
    GitRepo,
    CargoProject,
    SpecRepo,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    rc: Option<i32>,
    finding: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KnownFail {
    /// Platform names — the sweep's `darwin` (Rust's `macos` `target_os`)
    /// · `linux`.
    on: Vec<String>,
    /// The measured failure class (a key of `issues:`).
    class: String,
    /// The typed code the red run must carry (absent = any red holds).
    finding: Option<String>,
    /// The tracker token — must be the `issues:` value for `class`.
    issue: String,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Need {
    /// Benches/serves real local models the mock plane cannot impersonate.
    OllamaModel,
    /// Drives a repo shape this harness does not stage.
    Manual,
}

fn manifest() -> Manifest {
    let m: Manifest =
        serde_yaml_bw::from_str(MANIFEST_YAML).expect("the checked-in manifest parses");
    validate(&m);
    m
}

/// The manifest's internal laws — every one fails HERE, at schema level,
/// never as a surprise mid-traversal.
fn validate(m: &Manifest) {
    assert!(!m.entries.is_empty(), "the manifest names examples");
    for e in &m.entries {
        assert!(
            e.expect.rc.is_some() ^ e.expect.finding.is_some(),
            "`{}`: expect is exactly one of rc / finding",
            e.slug
        );
        if let Some(model) = &e.model {
            assert_eq!(
                model, HERMETIC_MODEL,
                "`{}`: the gate names no model but {HERMETIC_MODEL}",
                e.slug
            );
        }
        if let Some(t) = e.timeout {
            assert!(
                t <= MAX_TIMEOUT_SECS,
                "`{}`: timeout {t}s exceeds the {MAX_TIMEOUT_SECS}s ceiling",
                e.slug
            );
        }
        assert!(
            e.known_fail.is_none() || e.needs.is_none(),
            "`{}`: known_fail and needs are mutually exclusive — one row, one honesty",
            e.slug
        );
        if e.door == Door::Try {
            assert!(
                e.answers.is_empty(),
                "`{}`: `try` carries no --answer flag — a gated row takes `door: run`",
                e.slug
            );
            assert!(
                e.kit.is_empty(),
                "`{}`: `try` stages its own room — kits belong to `door: run`",
                e.slug
            );
        }
        if let Some(kf) = &e.known_fail {
            assert!(!kf.on.is_empty(), "`{}`: known_fail names its OSes", e.slug);
            match m.issues.get(&kf.class) {
                Some(token) => assert_eq!(
                    &kf.issue, token,
                    "`{}`: the issue token is the `issues:` entry for its class",
                    e.slug
                ),
                None => panic!(
                    "`{}`: class `{}` has no `issues:` token — a known-fail with no tracker is a silent skip",
                    e.slug, kf.class
                ),
            }
        }
    }
}

// ── The hermetic launch ─────────────────────────────────────────────────────

/// One entry's sandbox: per-slug scratch holding the run cwd, the isolated
/// HOME, the per-door TMPDIR, and the captured output logs.
struct Room {
    root: PathBuf,
    cwd: PathBuf,
}

impl Room {
    fn enter(slug: &str) -> Self {
        let stem = slug.replace('/', "-");
        let root = std::env::temp_dir().join(format!(
            "nika-pack-examples-family-{}-{stem}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let cwd = root.join("cwd");
        std::fs::create_dir_all(cwd.join("home")).expect("scratch home");
        std::fs::create_dir_all(root.join("tmp")).expect("scratch tmp");
        Room { root, cwd }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

/// The child environment law: the operator's `NIKA_*` exports never leak
/// into a corpus run; HOME and TMPDIR are the entry's own; PATH survives
/// (an `exec` example must find the same programs the operator does).
fn hermetic(cmd: &mut Command, room: &Room) {
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("NIKA_") {
            cmd.env_remove(&key);
        }
    }
    cmd.env("HOME", room.cwd.join("home"))
        .env("TMPDIR", room.path("tmp"))
        .env("NO_COLOR", "1")
        .current_dir(&room.cwd);
}

struct Outcome {
    code: Option<i32>,
    timed_out: bool,
    out: String,
    err: String,
    elapsed: Duration,
}

fn nika() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
}

/// Spawn and wait with the row's ceiling — stdout/stderr land in FILES
/// (a piping poll loop deadlocks the day an example outtalks the pipe
/// buffer; the file form has no such size).
fn launch(mut cmd: Command, room: &Room, timeout: u64) -> Outcome {
    let out_path = room.path("out.log");
    let err_path = room.path("err.log");
    let out_file = std::fs::File::create(&out_path).expect("out log");
    let err_file = std::fs::File::create(&err_path).expect("err log");
    cmd.stdout(Stdio::from(out_file))
        .stderr(Stdio::from(err_file));
    hermetic(&mut cmd, room);
    let start = Instant::now();
    let mut child = cmd.spawn().expect("the nika binary spawns");
    let mut timed_out = false;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if start.elapsed() > Duration::from_secs(timeout) => {
                let _ = child.kill();
                timed_out = true;
                break child.wait().ok().and_then(|s| s.code());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => panic!("wait on the corpus child: {e}"),
        }
    };
    let read = |p: &Path| {
        let mut s = String::new();
        let _ = std::fs::File::open(p)
            .expect("log reopens")
            .read_to_string(&mut s);
        s
    };
    Outcome {
        code,
        timed_out,
        out: read(&out_path),
        err: read(&err_path),
        elapsed: start.elapsed(),
    }
}

/// `door: run` staging — the kits are the declarative answer to the sweep's
/// FIXTURE-CWD / repo-shape classes.
fn stage_run_room(room: &Room, entry: &Entry, body: &str) -> PathBuf {
    let stem = entry.slug.replace('/', "-");
    let path = room.cwd.join(format!("{stem}.nika.yaml"));
    std::fs::write(&path, body).expect("plant the example");
    for kit in &entry.kit {
        match kit {
            // The same materializer `try`/`new` use — ONE implementation
            // for every taking door (nika-onboard's own law).
            Kit::Fixtures => {
                nika_onboard::fixtures::materialize(body, &path).expect("the fixtures kit stages");
            }
            Kit::GitRepo => stage_git_repo(&room.cwd),
            Kit::CargoProject => {
                std::fs::create_dir_all(room.cwd.join("src")).expect("cargo src");
                std::fs::write(
                    room.cwd.join("Cargo.toml"),
                    "[package]\nname = \"x\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
                )
                .expect("cargo manifest");
                std::fs::write(room.cwd.join("src/main.rs"), "fn main() {}\n").expect("cargo main");
            }
            Kit::SpecRepo => {
                std::fs::create_dir_all(room.cwd.join("schemas")).expect("spec schemas");
                std::fs::write(room.cwd.join("VERSION"), "0.114.0\n").expect("spec VERSION");
                std::fs::write(
                    room.cwd.join("CHANGELOG.md"),
                    "# Changelog\n\n## [Unreleased]\n",
                )
                .expect("spec CHANGELOG");
                std::fs::write(
                    room.cwd.join("schemas/workflow.schema.json"),
                    nika_pack::schema_json(),
                )
                .expect("spec schema");
            }
        }
    }
    path
}

fn stage_git_repo(dir: &Path) {
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("HOME", dir.join("home"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git is on PATH (CI runners ship it)");
        assert!(status.success(), "the git-repo kit stages: git {args:?}");
    };
    std::fs::create_dir_all(dir.join("src")).expect("kit src dir");
    std::fs::write(dir.join("src/change.txt"), "the git-repo kit's one file\n")
        .expect("kit src file");
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.email=gate@example.com",
        "-c",
        "user.name=gate",
        "commit",
        "-qm",
        "init",
    ]);
}

/// Assemble the row's command — `try` speaks the showroom dialect (no
/// `--model`: the offline mock rehearsal IS the default seat), `run`
/// carries the trio plus the row's model/answers/vars.
fn entry_command(entry: &Entry, room: &Room, body: &str) -> Command {
    let mut cmd = nika();
    match entry.door {
        Door::Try => {
            cmd.arg("try")
                .arg(&entry.slug)
                .args(["--no-progress", "--max-cost-usd", "0.01"]);
        }
        Door::Run => {
            let path = stage_run_room(room, entry, body);
            cmd.arg("run")
                .arg(&path)
                .args(["--no-progress", "--max-cost-usd", "0.01"]);
            if let Some(model) = &entry.model {
                cmd.args(["--model", model]);
            }
            for (task, value) in &entry.answers {
                cmd.args(["--answer", &format!("{task}={value}")]);
            }
        }
    }
    for (key, value) in &entry.vars {
        cmd.args(["--var", &format!("{key}={value}")]);
    }
    cmd
}

// ── The verdicts ────────────────────────────────────────────────────────────

fn tail(text: &str) -> String {
    text.lines()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

/// Judge one launched row against its expectation. `None` = green; the
/// string is the failure's evidence.
fn judge(entry: &Entry, o: &Outcome) -> Option<String> {
    if o.timed_out {
        return Some(format!(
            "timed out past {}s",
            entry.timeout.unwrap_or(MAX_TIMEOUT_SECS)
        ));
    }
    match (&entry.expect.rc, &entry.expect.finding) {
        (Some(rc), None) => {
            if o.code == Some(*rc) {
                None
            } else {
                Some(format!("rc {:?} ≠ expected {rc}", o.code))
            }
        }
        (None, Some(code)) => {
            if o.code == Some(0) {
                Some(format!("rc 0 but the row expects the finding {code}"))
            } else if o.out.contains(code.as_str()) || o.err.contains(code.as_str()) {
                None
            } else {
                Some(format!("rc {:?} without the expected {code}", o.code))
            }
        }
        _ => Some("expect carries neither rc nor finding".to_owned()),
    }
}

// ── The gates ───────────────────────────────────────────────────────────────

/// The completeness ratchet: the manifest and the pack are the SAME set —
/// an example with no row never re-enters the dead angle, and a row naming
/// no example fails instead of rotting.
#[test]
fn the_manifest_covers_every_shipped_example() {
    let m = manifest();
    let shipped: BTreeSet<String> = nika_pack::example_slugs().into_iter().collect();
    let named: BTreeSet<String> = m.entries.iter().map(|e| e.slug.clone()).collect();
    assert_eq!(
        named.len(),
        m.entries.len(),
        "a slug is named twice in the manifest"
    );
    assert_eq!(
        named,
        shipped,
        "manifest Δ pack — missing rows: {:?} · stray rows: {:?}",
        shipped.difference(&named).collect::<Vec<_>>(),
        named.difference(&shipped).collect::<Vec<_>>(),
    );
    for e in &m.entries {
        assert!(
            nika_pack::example(&e.slug).is_some(),
            "`{}` resolves in the embedded pack",
            e.slug
        );
    }
}

/// The traversal: every example launched per its row, every skip explicit
/// and counted, the tally printed — ran / green / known-fail / skipped.
#[test]
fn every_shipped_example_runs_per_its_manifest_row() {
    let m = manifest();
    // The manifest speaks the sweep's platform names: `darwin` for the OS
    // Rust calls `macos` (target_os / env::consts::OS), the rest verbatim.
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let mut tally = (0usize, 0usize, 0usize, 0usize); // ran · green · known-fail · skipped
    let mut failures: Vec<String> = Vec::new();

    for entry in &m.entries {
        let row = &entry.slug;
        if let Some(need) = &entry.needs {
            tally.3 += 1;
            println!("SKIP       {row} (needs: {})", need_name(*need));
            continue;
        }
        let body = nika_pack::example(row).expect("completeness gate ran first");
        let room = Room::enter(row);
        let cmd = entry_command(entry, &room, body);
        let outcome = launch(cmd, &room, entry.timeout.unwrap_or(MAX_TIMEOUT_SECS));
        tally.0 += 1;

        match entry
            .known_fail
            .as_ref()
            .filter(|kf| kf.on.iter().any(|o| o == os))
        {
            // The XFAIL leg: the run must STAY red on this OS, carrying the
            // class's finding when the row names one. Green = healed = the
            // manifest is now lying — fail and ask for the promotion.
            Some(kf) => {
                let healed = !outcome.timed_out && outcome.code == Some(0);
                let finding_seen = kf.finding.as_ref().is_none_or(|code| {
                    outcome.out.contains(code.as_str()) || outcome.err.contains(code.as_str())
                });
                if healed {
                    failures.push(format!(
                        "{row}: known-fail ({}) came back GREEN — the wound healed; \
                         promote the row to a plain expectation and close {}",
                        kf.class, kf.issue
                    ));
                } else if !finding_seen {
                    failures.push(format!(
                        "{row}: red but without {} — the class moved:\n{}\n{}",
                        kf.finding.as_deref().unwrap_or("?"),
                        tail(&outcome.out),
                        tail(&outcome.err)
                    ));
                } else {
                    tally.2 += 1;
                    println!(
                        "KNOWN-FAIL {row} ({} · {} · {} · {:.0?})",
                        kf.class, kf.issue, os, outcome.elapsed
                    );
                }
            }
            None => match judge(entry, &outcome) {
                None => {
                    tally.1 += 1;
                    let door = match entry.door {
                        Door::Try => "try",
                        Door::Run => "run",
                    };
                    println!(
                        "GREEN      {row} ({door} · rc={} · {:.0?})",
                        outcome.code.unwrap_or(-1),
                        outcome.elapsed
                    );
                }
                Some(why) => failures.push(format!(
                    "{row}: {why}\n--- stdout tail ---\n{}\n--- stderr tail ---\n{}",
                    tail(&outcome.out),
                    tail(&outcome.err)
                )),
            },
        }
    }

    println!(
        "pack examples family · {} entries: {} ran · {} green · {} known-fail · {} skipped",
        m.entries.len(),
        tally.0,
        tally.1,
        tally.2,
        tally.3
    );
    assert_eq!(
        tally.0 + tally.3,
        m.entries.len(),
        "every row is either launched or counted as skipped — no silent third state"
    );
    assert!(
        failures.is_empty(),
        "{} example(s) failed their manifest row:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

fn need_name(need: Need) -> &'static str {
    match need {
        Need::OllamaModel => "ollama-model",
        Need::Manual => "manual",
    }
}
