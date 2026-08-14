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
pub mod mcp_pins;
pub mod model;
pub mod new;
pub mod pack_surface;
pub mod run;
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
use nika_schema::{FileId, ParseMode};

pub use nika_cli_host::output::{VerbOutput, exit};
pub(crate) use nika_cli_host::output::{linked_path, truecolor_env};

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
    let yaml = if path == "-" {
        use std::io::Read as _;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| VerbOutput::env(format!("cannot read stdin: {e}")))?;
        buf
    } else {
        std::fs::read_to_string(path)
            .map_err(|e| VerbOutput::env(format!("cannot read {path}: {e}")))?
    };
    let wf = nika_schema::parse(&yaml, FileId::new(0), ParseMode::Strict)
        .map_err(|e| VerbOutput::file(format!("PARSE ✗  [{}] {e}", e.spec_code())))?;
    // The composed lane (spec 14): child targets resolve against the
    // file the operator named; the fs edge is the skills reader's twin.
    let mut report = nika_check::check_composed(&wf, path, &mut |p| {
        std::fs::read_to_string(p).map_err(|e| e.to_string())
    });
    stamp_judged_semantic(&wf, &mut report);
    Ok((yaml, wf, report))
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

/// The `skills:` fs edge (#473) — the ONE reader check · run · test share.
pub(crate) fn resolve_workflow_skills(wf: &RawWorkflow) -> nika_schema::ResolvedSkills {
    nika_schema::resolve_skills(wf, &mut |p| {
        std::fs::read_to_string(p).map_err(|e| e.to_string())
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

        let resolved = resolve_workflow_skills(&wf);

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
    fn parse_error_carries_its_spec_code() {
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
        assert!(
            err.text.contains("NIKA-PARSE-009"),
            "parse error must carry its spec code · got: {}",
            err.text
        );
    }
}
