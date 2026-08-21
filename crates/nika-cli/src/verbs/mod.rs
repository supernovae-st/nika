// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The static verb suite — everything auditable BEFORE a run (spec §2).
//!
//! Every verb here is a pure function `(args, file) → (text, exit code)`
//! over the shipped engine layers (`nika-schema` ladder · `nika-error`
//! registry · `nika-pack` embedded surface). No network, no effects, no
//! runtime — the `run` verb arrives with L3 and is refused honestly until
//! then. The bin (`main.rs`) stays a thin dispatcher so every surface is
//! testable as a library call.

pub mod arm;
pub mod catalog;
pub mod check;
pub mod context;
pub mod examples;
pub mod explain;
pub mod explain_file;
pub mod fix;
pub mod graph;
pub mod guard;
pub mod init;
pub mod inspect;
pub mod key;
pub mod list;
pub mod mcp_pins;
pub mod model;
pub mod new;
pub mod pack_surface;
pub mod run;
pub mod serve;
pub mod session;
pub mod sign;
pub mod test;
pub mod tools;
pub use nika_cli_host::{doctor, probe, welcome, wire};
// The trace-reading plane descended to `nika-trace` 2026-08-11 (the 15k
// prod-LOC wall · D-2026-07-09-N1 one unit, two members · the ADR-110
// cli-host precedent) — re-exported at the historical verbs:: paths so
// every call site, suite and the bin dispatch read unchanged. The
// store/retention shims stay crate-internal (consumers read the
// descended homes: `nika_dap::store` · `nika_cli_host::retention`).
pub(crate) use nika_trace::forecast;
pub use nika_trace::{
    evidence, receipt, trace, trace_anchor, trace_otel, trace_reproduce, trace_verify,
};

use nika_check::CheckReport;
use nika_schema::raw::RawWorkflow;
use nika_schema::{FileId, ParseMode, SchemaError};

pub use nika_cli_host::output::{VerbOutput, exit};
pub(crate) use nika_cli_host::output::{linked_path, truecolor_env};

#[derive(Clone)]
pub(crate) struct RunSource {
    logical_path: std::sync::Arc<str>,
    source: std::sync::Arc<str>,
    repair_target: nika_display::check_render::RepairTarget,
}

impl RunSource {
    pub(crate) fn capture(path: &str) -> Result<Self, VerbOutput> {
        let repair_target = if path == "-" {
            nika_display::check_render::RepairTarget::Stdin
        } else {
            nika_display::check_render::RepairTarget::WorkspaceFile
        };
        Self::capture_with_repair_target(path, repair_target)
    }

    pub(crate) fn capture_with_repair_target(
        path: &str,
        repair_target: nika_display::check_render::RepairTarget,
    ) -> Result<Self, VerbOutput> {
        let bytes = if path == "-" {
            use std::io::Read as _;
            let mut buf = Vec::new();
            std::io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| VerbOutput::env(format!("cannot read stdin: {e}")))?;
            buf
        } else {
            std::fs::read(path).map_err(|e| VerbOutput::env(format!("cannot read {path}: {e}")))?
        };
        Self::from_bytes_with_repair_target(path, bytes, repair_target)
            .map_err(|_| invalid_utf8_refusal())
    }

    pub(crate) fn from_bytes(
        logical_path: impl Into<String>,
        bytes: Vec<u8>,
    ) -> std::io::Result<Self> {
        Self::from_bytes_with_repair_target(
            logical_path,
            bytes,
            nika_display::check_render::RepairTarget::WorkspaceFile,
        )
    }

    fn from_bytes_with_repair_target(
        logical_path: impl Into<String>,
        bytes: Vec<u8>,
        repair_target: nika_display::check_render::RepairTarget,
    ) -> std::io::Result<Self> {
        let source = String::from_utf8(bytes).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error.utf8_error())
        })?;
        Ok(Self::new(logical_path, source, repair_target))
    }

    fn new(
        logical_path: impl Into<String>,
        source: String,
        repair_target: nika_display::check_render::RepairTarget,
    ) -> Self {
        Self {
            logical_path: std::sync::Arc::from(logical_path.into()),
            source: std::sync::Arc::from(source),
            repair_target,
        }
    }

    pub(crate) fn logical_path(&self) -> &str {
        &self.logical_path
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn repair_target(&self) -> nika_display::check_render::RepairTarget {
        self.repair_target
    }
}

/// Read + strict-parse + ladder-check one workflow file. The Unix dash
/// (`-`) reads stdin — the editor wire: a dirty buffer pipes straight
/// in, no tmp-file dance; every verb on this seam (check · graph ·
/// inspect · test) inherits it.
///
/// Failure mapping per spec §4: unreadable = environment (`3`) · parse
/// error = a finding in the FILE (`2`).
pub(crate) fn load_checked(path: &str) -> Result<(RawWorkflow, CheckReport), VerbOutput> {
    let (_, wf, report) = load_checked_with_source(path)?;
    Ok((wf, report))
}

/// [`load_checked`] plus the raw YAML — the painted-diagnostics surface
/// (check) frames findings on the source, so the text it was parsed
/// from rides along instead of being read twice. Carries the Unix-dash
/// contract: `-` reads stdin (the frames then label the origin `-`).
pub(crate) fn load_checked_with_source(
    path: &str,
) -> Result<(String, RawWorkflow, CheckReport), VerbOutput> {
    let source = RunSource::capture(path)?;
    let (wf, report) = load_checked_run_source(&source)?;
    Ok((source.source().to_owned(), wf, report))
}

pub(crate) fn load_checked_run_source(
    source: &RunSource,
) -> Result<(RawWorkflow, CheckReport), VerbOutput> {
    let wf = nika_schema::parse(source.source(), FileId::new(0), ParseMode::Strict)
        .map_err(|error| schema_refusal(&error))?;
    // The composed lane (spec 14): child targets resolve against the
    // file the operator named; the fs edge is the skills reader's twin.
    let mut report = nika_check::check_composed(&wf, source.logical_path(), &mut |p| {
        std::fs::read_to_string(p).map_err(|e| e.to_string())
    });
    stamp_judged_semantic(&wf, &mut report);
    Ok((wf, report))
}

/// The single CLI sink for every schema-facing refusal, whether acquisition
/// rejected the encoding or the parser rejected the decoded workflow.
fn schema_refusal(error: &SchemaError) -> VerbOutput {
    VerbOutput::file(format!("PARSE ✗  {}", error.diagnostic()))
}

fn invalid_utf8_refusal() -> VerbOutput {
    schema_refusal(&SchemaError::YamlSyntax {
        message: "workflow source is not valid UTF-8".to_owned(),
        span: None,
    })
}

/// Stamp the judged-vs-booted binding (F-P2): the report records the
/// semantic hash of the workflow it JUDGED, so the runtime's trust gate
/// refuses a report that describes OTHER bytes — a file edited after
/// the check (same structure) handed its now-stale report refuses
/// NIKA-1707 at boot. An unprojectable workflow stamps `None` (the
/// gate's boundary-lane clause rides alone, today's posture).
pub(crate) fn stamp_judged_semantic(wf: &RawWorkflow, report: &mut CheckReport) {
    report.workflow_semantic =
        nika_runtime::proof::ir::semantic_ir_hash(wf).map(|h| h.as_hex().to_owned());
}

/// The directory a workflow's relative references resolve against — the
/// folder holding the file the operator named. An empty parent (a bare
/// `wf.nika.yaml`) joins to the path itself, which is the CWD-relative
/// form and stays correct.
pub(crate) fn workflow_base(path: &str) -> &std::path::Path {
    std::path::Path::new(path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""))
}

/// The `skills:` fs edge (#473) — the ONE reader check · run · test share.
///
/// `base` is the DIRECTORY of the workflow the operator named. A
/// `skills:` path is relative to the FILE that names it, exactly like a
/// composed child target — `check_composed` has taken that base since
/// spec 14, and this reader, its declared twin, took none. So
/// `nika check sub/wf.nika.yaml` run from the repo root read `./s.md`
/// from the ROOT, not from `sub/`.
///
/// **What deliberately does NOT move: the permits subject.**
/// `resolve_skills` judges the grant on the path AS WRITTEN
/// (`allows_path(key, false)`), and `path_glob_matches` is purely
/// lexical — it normalizes two strings and walks segments, with no
/// filesystem and no CWD. That is what makes check ≡ run decidable
/// without touching a disk, and rebasing the SUBJECT would move a
/// security boundary. Rebasing the READ moves the opposite way: the file
/// actually opened now agrees with the grant the author wrote, instead of
/// depending on where the operator happened to stand. NIKA-SEC-004 was
/// falsified in this exact zone at 0.108.0; the gate stays where it is.
pub(crate) fn resolve_workflow_skills(
    wf: &RawWorkflow,
    base: &std::path::Path,
) -> nika_schema::ResolvedSkills {
    nika_schema::resolve_skills(wf, &mut |p| {
        // An ABSOLUTE skill path joins to itself — it keeps naming the
        // file it names, and the lexical grant still has to admit it.
        std::fs::read_to_string(base.join(p)).map_err(|e| e.to_string())
    })
}

/// The workflow with the CLI `--model` swapped into the envelope default
/// (#342) — per-task `model:` keeps winning, mirroring the runtime's
/// precedence. The implementation lives in `nika_check` (the ONE home —
/// the runtime's admission gate prices the same effective model).
pub(crate) fn with_model_override(wf: &RawWorkflow, model: &str) -> RawWorkflow {
    nika_check::with_model_override(wf, model)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `skills:` fs edge lives INSIDE the boundary · the follow-on
    /// #473 never settled (its "permits question" asked whether a skill
    /// could WIDEN the inferred boundary, never whether READING it needed
    /// a grant).
    ///
    /// Falsified at 0.108.0, the published binary: under `permits: {}` ·
    /// the DECLARED ZERO, whose own doc says "this workflow opens no
    /// file" · a skill path of `/etc/hosts` was READ and the engine
    /// reported on its CONTENT (`no YAML frontmatter`) while the PERMITS
    /// and TRIFECTA rungs both stayed green.
    ///
    /// Cost of closing it, measured on this tree: 0 of 94 `.nika.yaml`
    /// files carry `skills:` (92 carry `permits:`, 12 carry `agent:` ·
    /// the census instrument sees things, so the zero is real).
    /// ⚠️ The instrument had to be rebuilt once: a first version pointed at
    /// `/etc/hosts` and went GREEN for the WRONG reason · that file is not a
    /// valid Agent Skill, so it is READ, then rejected at parse, landing in
    /// `findings` and never in `texts`. Both assertions were satisfied by the
    /// MALFORMATION, not by a boundary refusal · a reference that does not
    /// sit outside the measured function agrees with itself.
    ///
    /// ⭐ A `skills:` path is relative to the FILE that names it, never to
    /// wherever the operator happens to stand.
    ///
    /// `check_composed` has taken the workflow's own path as its base since
    /// spec 14, and its own comment calls the skills reader « the fs edge
    /// is the skills reader's twin » — but the twin took NO base, so it
    /// resolved against the process CWD. `nika check sub/wf.nika.yaml` from
    /// the repo root looked for `s.md` in the ROOT.
    ///
    /// The fixture is discriminating by construction: the skill exists ONLY
    /// in the workflow's directory, and the test's CWD is not that
    /// directory. Under the old reader the read misses and `texts` is
    /// empty; only a correctly based read populates it.
    ///
    /// The GRANT is unchanged and still lexical (`s.md` as written) — that
    /// is the half that must not move.
    #[test]
    fn a_skill_path_resolves_against_the_workflow_not_the_cwd() {
        let dir = std::env::temp_dir().join(format!("nika-skill-base-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("fixture dir");
        std::fs::write(
            dir.join("s.md"),
            "---\nname: probe\ndescription: a valid Agent Skill beside its workflow\n---\nbody\n",
        )
        .expect("skill beside the workflow");
        let wf_path = dir.join("wf.nika.yaml");
        let yaml = concat!(
            "nika: w\n",
            "model: mock/echo\n",
            "permits:\n  fs:\n    read: [\"s.md\"]\n",
            "tasks:\n  t:\n    agent:\n",
            "      prompt: p\n",
            "      skills: [\"s.md\"]\n",
        );
        std::fs::write(&wf_path, yaml).expect("workflow beside its skill");

        let wf = nika_schema::parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let named = wf_path.to_string_lossy();
        let resolved = resolve_workflow_skills(&wf, workflow_base(&named));

        assert!(
            resolved.findings.is_empty(),
            "the skill sits beside its workflow and is granted · {:?}",
            resolved.findings
        );
        assert!(
            resolved.texts.contains_key("s.md"),
            "the read must resolve against the workflow's directory, not the CWD"
        );

        // The CWD-based reader is what shipped. Pinned here so the fix is
        // not silently undone: with no base, the same fixture misses.
        let cwd_based = resolve_workflow_skills(&wf, std::path::Path::new(""));
        assert!(
            cwd_based.texts.is_empty(),
            "the fixture must be discriminating — if this populates, the \
             test's CWD happens to hold an `s.md` and proves nothing"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The discriminating fixture is a **valid** skill placed outside the
    /// boundary: only a refusal keeps `texts` empty · a read populates it.
    #[test]
    fn skills_read_outside_the_declared_boundary_is_refused() {
        // A VALID skill, deliberately outside the workflow's tree and outside
        // any grant the file makes.
        let dir = std::env::temp_dir().join("nika-skills-boundary-probe");
        std::fs::create_dir_all(&dir).expect("probe dir");
        let skill = dir.join("valid-skill.md");
        std::fs::write(
            &skill,
            "---\nname: probe\ndescription: a perfectly valid Agent Skill\n---\nbody\n",
        )
        .expect("probe skill");
        let path = skill.to_string_lossy().into_owned();

        let yaml = format!(
            concat!(
                "nika: w\n",
                "model: mock/echo\n",
                "permits: {{}}\n",
                "tasks:\n  t:\n    agent:\n",
                "      prompt: p\n",
                "      skills: [\"{}\"]\n",
            ),
            path
        );
        let wf = nika_schema::parse(&yaml, FileId::new(0), ParseMode::Strict)
            .expect("the fixture parses");

        // The fixture names an ABSOLUTE path, so the base is moot (`join`
        // on an absolute returns it unchanged) — the grant still decides.
        let resolved = resolve_workflow_skills(&wf, std::path::Path::new(""));

        // The whole point: the skill PARSES, so the only thing that can keep
        // it out of `texts` is the boundary refusing the read.
        assert!(
            resolved.texts.is_empty(),
            "a VALID skill outside the declared boundary was READ · \
             the fs edge bypasses permits.fs.read · got {} text(s)",
            resolved.texts.len()
        );
        assert!(
            !resolved.findings.is_empty(),
            "the refusal must surface as a SKILLS finding, never as silence"
        );
    }

    /// The pipe-parity pin: with the `links` capability OFF (every sober
    /// register), `linked_path` returns the path VERBATIM — zero escapes.
    /// With it on, the OSC-8 wrapper carries a `file://` URL and keeps
    /// the printed text unchanged; a path that will not canonicalize
    /// stays plain (a dead link is worse than no link).
    #[test]
    fn linked_path_is_byte_identical_when_links_are_off() {
        let dir = std::env::temp_dir().join("nika-cli-linkedpath-tests");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let file = dir.join("wf.nika.yaml");
        std::fs::write(&file, "nika: v1\n").expect("fixture");
        let path = file.to_str().expect("utf8 path");

        let plain = crate::Theme::new(false, false, false);
        assert_eq!(linked_path(plain, path), path, "sober register: verbatim");

        let mut linked = crate::Theme::new(false, false, false);
        linked.links = true;
        let out = linked_path(linked, path);
        assert!(
            out.starts_with("\x1b]8;;file://") && out.ends_with("\x1b]8;;\x1b\\"),
            "OSC-8 wrapper: {out:?}"
        );
        assert!(out.contains(path), "the printed text stays the path");

        let ghost = dir.join("never-written.yaml");
        let ghost = ghost.to_str().expect("utf8 path");
        assert_eq!(linked_path(linked, ghost), ghost, "no file → no link");
    }

    /// Regression · a parse-stage rejection must surface its spec wire code,
    /// exactly like the CONFORM stage. The multiple-verbs short-circuit used
    /// to render `PARSE ✗ <msg>` with no `[NIKA-PARSE-009]`, so an operator
    /// could not `nika explain` the failure (every other finding shows its
    /// code). `load_checked` now formats `e.spec_code()`.
    #[test]
    fn parse_error_carries_its_code_message_and_next_action_exactly() {
        let path =
            std::env::temp_dir().join(format!("nika-parsecode-{}.nika.yaml", std::process::id(),));
        std::fs::write(
            &path,
            "nika: two-verbs\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: \"x\" }\n    exec: { run: \"echo hi\" }\n",
        )
        .expect("fixture written");
        let err = load_checked(path.to_str().expect("utf-8 tmp path"))
            .expect_err("a task with two verbs must fail to parse");
        std::fs::remove_file(&path).ok();
        assert_eq!(err.code, exit::FILE, "{}", err.text);
        assert_eq!(
            err.text,
            "PARSE ✗  [NIKA-PARSE-009] task `a` has multiple verbs (infer, exec) — exactly one required · → nika explain NIKA-PARSE-009"
        );
    }

    #[test]
    fn invalid_utf8_workflow_is_a_coded_schema_refusal_not_an_environment_error() {
        let path = std::env::temp_dir().join(format!(
            "nika-invalid-utf8-{}.nika.yaml",
            std::process::id(),
        ));
        std::fs::write(&path, [0xff, 0xfe]).expect("invalid UTF-8 fixture written");

        let err = load_checked(path.to_str().expect("UTF-8 tmp path"))
            .expect_err("invalid workflow encoding must refuse before parsing");
        std::fs::remove_file(&path).ok();

        assert_eq!(err.code, exit::FILE, "{}", err.text);
        assert_eq!(
            err.text,
            "PARSE ✗  [NIKA-PARSE-001] YAML parse error: workflow source is not valid UTF-8 · → nika explain NIKA-PARSE-001"
        );
    }

    #[test]
    fn a_missing_workflow_stays_an_environment_error() {
        let path = std::env::temp_dir().join(format!(
            "nika-definitely-missing-{}.nika.yaml",
            std::process::id(),
        ));
        std::fs::remove_file(&path).ok();

        let err = load_checked(path.to_str().expect("UTF-8 tmp path"))
            .expect_err("a missing workflow is an environment failure");
        assert_eq!(err.code, exit::ENV, "{}", err.text);
        assert!(err.text.starts_with("cannot read "), "{}", err.text);
    }
}
