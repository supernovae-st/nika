// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The showroom lane — `nika try <slug>`: stage the embedded
//! example, pre-display the source (the lesson before the tokens),
//! run it, and hand over the keys (split from `mod.rs` under the
//! 1500-line file law).

use super::{RenderMode, RunVerdict, exit, run_verdict};
use crate::Theme;

/// Stage the rehearsal room: the workflow and the ingredients it
/// reads, in a temp dir of their own, then ENTER it. Returns the
/// staged path and the directory to restore on the way out — the
/// isolation is what makes the storefront's « nothing written » a
/// fact (paths resolve from the RUN's working directory, so an
/// example that reads `examples/fixtures/x` must find it beside
/// itself, and its writes must land here, not in the operator's
/// folder). Split under the 100-line fn law.
///
/// The move is made through the crate's ONE cwd lease (#1192). This site had
/// NO guard at all — it was the third of three chdir sites and the only one
/// that took nothing, so it could move the ground under a budget test that was
/// dutifully holding its own (different) lock. The returned [`crate::cwd::Lease`]
/// IS the restore: dropping it puts the operator's session back, on every path
/// including a panic unwind.
fn stage_room(slug: &str, yaml: &str) -> Result<(std::path::PathBuf, crate::cwd::Lease), u8> {
    let stem = slug.replace('/', "-");
    let room = std::env::temp_dir().join(format!("nika-try-{stem}"));
    let path = room.join(format!("{stem}.nika.yaml"));
    if let Err(e) = std::fs::create_dir_all(&room).and_then(|()| std::fs::write(&path, yaml)) {
        eprintln!("nika run: environment: cannot stage example `{slug}`: {e}");
        return Err(exit::ENV);
    }
    if let Err(e) = nika_onboard::fixtures::materialize(yaml, &path) {
        eprintln!("nika run: environment: cannot stage the ingredients of `{slug}`: {e}");
        return Err(exit::ENV);
    }
    let lease = crate::cwd::enter(&room).map_err(|e| {
        eprintln!("nika run: environment: cannot enter the rehearsal room: {e}");
        exit::ENV
    })?;
    Ok((path, lease))
}

/// The staged try room is `…/nika-try-<stem>/<stem>.nika.yaml`.
/// Display uses this to name the rehearsal (C12 · UX-3) instead of a
/// path the sandbox is about to discard.
pub(super) fn try_rehearsal_slug(path: &str) -> Option<&str> {
    let path = std::path::Path::new(path);
    let parent = path.parent()?.file_name()?.to_str()?;
    let stem = parent.strip_prefix("nika-try-")?;
    let file = path.file_name()?.to_str()?;
    (file.strip_suffix(".nika.yaml") == Some(stem)).then_some(stem)
}

/// UX-3 · every try card: how to own the file.
pub(super) fn try_own_file_line(slug: &str) -> String {
    format!("rehearsal. to own the file: nika new {slug}")
}

/// `nika try <slug>` — execute one EMBEDDED example through the
/// real runtime (the pack ships offline · zero network for the exec/
/// mock-model examples). Stages the embedded YAML to a temp file (the
/// verb reads a path) and runs it.
///
/// `model_override` — `Some(m)` (from `--model m`) swaps the example's
/// envelope model for `m` (so `--model mock/echo` previews offline). On a
/// FAILED run with NO override, a rescue tip keyed on the failure KIND is
/// printed to stderr (#145: the offline-model nudge for infer/provider
/// failures · the real missing dependency for an exec `program not
/// found`) · the original exit code is returned unchanged.
/// Fold `try`'s `--answer` / `--resume` into the same request `run` builds.
/// Relative resume paths pin to the operator cwd BEFORE the rehearsal
/// room chdir, or a `--resume traces/t.ndjson` would look inside the temp
/// staging dir.
fn try_gate_request(
    answers: &[String],
    resume: Option<&std::path::Path>,
    operator_cwd: Option<&std::path::Path>,
) -> Option<nika_dap::resume::ResumeRequest> {
    if resume.is_none() && answers.is_empty() {
        return None;
    }
    Some(nika_dap::resume::ResumeRequest {
        trace: resume.map(|path| {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                operator_cwd.map_or_else(|| path.to_path_buf(), |cwd| cwd.join(path))
            }
        }),
        from: None,
        answers: answers.to_vec(),
        compat: None,
        allow_unverified: false,
    })
}

#[must_use]
#[allow(clippy::fn_params_excessive_bools)] // the run trio, verbatim (two switches)
pub fn example(
    slug: &str,
    model_flag: Option<&str>,
    access_pin: Option<&str>,
    vars: &[String],
    (quiet, no_progress): (bool, bool),
    max_cost_usd: Option<f64>,
    (answers, resume): (&[String], Option<&std::path::Path>),
    theme: Theme,
) -> u8 {
    // V5 seat law (RAMS-4): the raw --model flag resolves here — bare =
    // the offline mock rehearsal · `self` = the example's own seat.
    let model_override = crate::verbs::examples::rehearsal_seat(model_flag);
    let Some(yaml) = nika_pack::example(slug) else {
        eprintln!("unknown example `{slug}` — bare `nika try` names the embedded set");
        return exit::FILE;
    };
    // Pin `--resume` to the operator cwd before the room chdir.
    let operator_cwd = std::env::current_dir().ok();
    let resume_req = try_gate_request(answers, resume, operator_cwd.as_deref());
    let (path, room_lease) = match stage_room(slug, yaml) {
        Ok(pair) => pair,
        Err(code) => return code,
    };
    let (mode, theme) = example_mode(quiet, no_progress, theme);
    if mode == RenderMode::Live {
        example_predisplay(slug, yaml, theme);
    }
    let verdict = run_verdict(
        &path.to_string_lossy(),
        false,
        None,
        theme,
        mode,
        false,
        model_override,
        access_pin,
        vars,
        resume_req.as_ref(),
        // No run journal: the example is staged to a TEMP file — `.nika/
        // traces/` belongs to workspace runs (the same drive underneath,
        // deliberately disabled here). `--resume` still reads the path
        // the operator named (pinned to their cwd above).
        true,
        // Examples always run whole (tiny by design · no scoping surface).
        None,
        false,
        max_cost_usd,
        // An example runs a TEMP-staged file, not a workspace run — the
        // workspace's trace store is not this invocation's to collect.
        true,
        false, // examples are engine-staged content — unsigned-tolerant
        None,
    );
    // The example's own envelope model — what we suggest overriding when a
    // run fails offline. A parse miss leaves it empty (the infer tip then
    // never fires · the run already surfaced the real finding).
    let model = example_model(yaml);
    if let Some(tip) = example_tip(slug, &verdict, model_override.is_some(), &model) {
        eprintln!("\n  {tip}");
    }
    // The showroom hands over the keys: a green run earns the ONE
    // adoption line (stderr — stdout contracts untouched).
    //
    // Plain is INCLUDED on purpose. It is the piped/CI default, which is
    // to say it is what an agent sees when it runs this for someone —
    // and the kit's whole premise is that an agent runs it for someone.
    // Gating on Live stripped the only next step exactly where the
    // reader had no other guidance (measured 2026-08-03, first-run
    // review: the TTY lane ends on "make it yours", the piped lane ended
    // on "not a real answer" and nothing else). Quiet stays out: it
    // promises the compact verdict card and errors, nothing more.
    if mode != RenderMode::Quiet {
        let clean = slug.strip_suffix(".nika.yaml").unwrap_or(slug);
        eprintln!(
            "\n  {}",
            crate::display::vocab::hint(theme, "rehearsal", &try_own_file_line(clean))
        );
    }
    // Leave the room as we found it — the rehearsal is isolated, not
    // a relocation of the operator's session. Dropping the lease restores
    // the directory AND releases it for the next chdir site; the explicit
    // drop is here so the release is a readable step rather than an
    // end-of-scope accident.
    drop(room_lease);
    verdict.code
}

/// Live / plain / quiet plus the accent/heat pair the TTY lane needs.
fn example_mode(quiet: bool, no_progress: bool, mut theme: Theme) -> (RenderMode, Theme) {
    let mode = if quiet {
        RenderMode::Quiet
    } else if !no_progress && std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        RenderMode::Live
    } else {
        RenderMode::Plain
    };
    theme.accents = mode == RenderMode::Live;
    theme.heat = theme.accents && theme.color && crate::verbs::truecolor_env();
    (mode, theme)
}

/// The pre-display (TTY only): the SOURCE before the run — an example
/// is a teaching artifact, and the lesson reads better before the
/// tokens than after. Dim-framed, verbatim (the comments ARE the
/// curriculum); pipes keep their exact bytes.
fn example_predisplay(slug: &str, yaml: &str, theme: Theme) {
    let file = format!(
        "{}.nika.yaml",
        slug.strip_suffix(".nika.yaml").unwrap_or(slug)
    );
    println!(
        "{} {} {}",
        theme.logo(),
        theme.paint(crate::display::theme::Role::Strong, &file),
        theme.paint(
            crate::display::theme::Role::Dim,
            "— the source, then the run"
        ),
    );
    // Trim the machine boilerplate (SPDX · schema modeline · their
    // trailing blank) — the lesson starts at the title comment.
    let mut started = false;
    for line in yaml.lines() {
        let t = line.trim_start_matches(['#', ' ']);
        if !started
            && (t.starts_with("SPDX") || t.starts_with("yaml-language-server") || t.is_empty())
        {
            continue;
        }
        started = true;
        println!(
            "  {} {line}",
            theme.paint(crate::display::theme::Role::Dim, "│")
        );
    }
    println!();
}

/// The example's envelope `model:` string (empty when the YAML has no
/// model or won't parse). Best-effort — drives only the offline-hint
/// decision, never the run itself.
fn example_model(yaml: &str) -> String {
    nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .ok()
    .and_then(|wf| wf.model.map(|m| m.value))
    .unwrap_or_default()
}

/// The rescue tip under a FAILED example, keyed on the failure KIND
/// (#145 · the exit code alone misdirected: an example that carries BOTH
/// a model and exec tasks used to earn the mock-model nudge on a missing
/// binary — a swap that cannot conjure the program). `None` = say
/// nothing: success · pause · pre-run refusals · an explicit `--model`
/// override · failure classes neither a model swap nor an install would
/// rescue. Pure · so the policy is unit-tested without staging or
/// running anything.
#[must_use]
fn example_tip(
    slug: &str,
    verdict: &RunVerdict,
    override_given: bool,
    model: &str,
) -> Option<String> {
    if verdict.code == exit::OK {
        return None;
    }
    let failure = verdict.failure.as_ref()?;
    // B06/C06: the try sandbox is not a cargo tree and not a git repo.
    // `--model` cannot conjure either, so this arm fires before the
    // override short-circuit (a mock rehearsal still hits seatbelt).
    if failure.code == "NIKA-SEC-001"
        && let Some(hint) = nika_pack::try_recover_hint(slug)
    {
        let slug = slug.strip_suffix(".nika.yaml").unwrap_or(slug);
        return Some(format!(
            "tip: try sandbox has no `{}` ({}) · this job is off the first shelf.\n        to own a real workspace: nika new {slug}",
            hint.missing, hint.recovered_as
        ));
    }
    if override_given {
        return None;
    }
    // Infer/provider failures — the "no local model running" case: the
    // offline preview is one flag away (the funnel's highest-intent P0).
    if failure.code.starts_with("NIKA-INFER-") {
        if model.is_empty() || model == "mock/echo" {
            return None;
        }
        return Some(format!(
            "tip: no local model running? preview this example offline →\n        nika try {slug}"
        ));
    }
    // A missing program — name the REAL dependency (the ✖ line above
    // carries the code; this states the way out).
    if failure.code == "NIKA-EXEC-002" {
        let program = failure
            .message
            .split("program not found: ")
            .nth(1)
            .map(str::trim)
            .filter(|p| !p.is_empty());
        return Some(match program {
            Some(p) => format!(
                "tip: this example shells out to `{p}` — not found on this machine;\n        install it, or browse offline-friendly examples → nika try"
            ),
            None => "tip: this example shells out to a program this machine does not \
                     have\n        (the ✖ line names it) — install it, or browse → nika try"
                .to_owned(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_rehearsal_slug_reads_the_staged_room() {
        assert_eq!(
            try_rehearsal_slug("/tmp/nika-try-competitor-radar/competitor-radar.nika.yaml"),
            Some("competitor-radar")
        );
        assert_eq!(
            try_rehearsal_slug("/tmp/nika-try-01-hello/01-hello.nika.yaml"),
            Some("01-hello")
        );
        assert_eq!(
            try_rehearsal_slug("hello.nika.yaml"),
            None,
            "a workspace run is not a try rehearsal"
        );
        assert_eq!(
            try_own_file_line("competitor-radar"),
            "rehearsal. to own the file: nika new competitor-radar"
        );
    }

    #[test]
    fn try_gate_request_is_none_when_neither_flag_is_set() {
        assert!(try_gate_request(&[], None, None).is_none());
    }

    #[test]
    fn try_gate_request_preseeds_answers_without_a_trace() {
        let req = try_gate_request(&["approve=true".into()], None, None)
            .expect("answers-only is a request");
        assert_eq!(req.answers, ["approve=true"]);
        assert!(req.trace.is_none(), "C01 one-pass gate has no --resume");
        assert!(req.from.is_none());
        assert!(req.compat.is_none());
        assert!(!req.allow_unverified);
    }

    #[test]
    fn try_gate_request_pins_a_relative_resume_to_the_operator_cwd() {
        let cwd = std::path::Path::new("/tmp/op");
        let req = try_gate_request(&[], Some(std::path::Path::new("t.ndjson")), Some(cwd))
            .expect("resume is a request");
        assert_eq!(
            req.trace.as_deref(),
            Some(std::path::Path::new("/tmp/op/t.ndjson"))
        );
    }

    #[test]
    fn try_gate_request_keeps_an_absolute_resume() {
        let abs = std::path::Path::new("/abs/t.ndjson");
        let req = try_gate_request(&[], Some(abs), Some(std::path::Path::new("/tmp/op")))
            .expect("resume is a request");
        assert_eq!(req.trace.as_deref(), Some(abs));
    }

    /// A failed verdict carrying one typed task error (the policy's input).
    fn failed(code: &str, message: &str) -> RunVerdict {
        RunVerdict {
            code: exit::WORKFLOW,
            failure: Some(nika_runtime::TaskErrorRecord::new(code, message, false)),
            paused: None,
            trace: None,
        }
    }

    /// The rescue-tip policy (pure · the heart of the UX decision) is
    /// keyed on the failure KIND (#145): only an infer/provider failure
    /// earns the offline-model nudge; a missing program names the real
    /// dependency instead of suggesting a model swap that cannot fix it.
    #[test]
    fn example_tip_keys_on_the_failure_kind() {
        let infer = failed("NIKA-INFER-001", "provider call failed: model not found");
        // FAIL on infer + no override + a local model → the right nudge.
        let tip = example_tip("01-hello", &infer, false, "ollama/llama3.1")
            .expect("the infer failure earns the offline nudge");
        // V5: `try` is offline by default — the rescue line is the bare
        // command again, no `--model` flag to teach.
        assert!(tip.contains("nika try 01-hello"), "{tip}");
        // A clean run never needs the tip.
        let ok = RunVerdict::bare(exit::OK);
        assert!(example_tip("01-hello", &ok, false, "ollama/llama3.1").is_none());
        // The user already overrode the model · suggesting it again is noise.
        assert!(example_tip("01-hello", &infer, true, "ollama/llama3.1").is_none());
        // mock/echo needs no provider · a failure there is a real bug, not
        // a missing local model — so the offline tip would mislead.
        assert!(example_tip("01-hello", &infer, false, "mock/echo").is_none());
        // No envelope model (a parse miss) · the nudge would mislead.
        assert!(example_tip("01-hello", &infer, false, "").is_none());
    }

    /// THE misdirection pin (#145 operator finding): an exec `program not
    /// found` — even on an example that ALSO declares a model — must name
    /// the missing program, never the mock-model swap.
    #[test]
    fn example_tip_exec_failure_names_the_program_not_the_model() {
        let exec = failed("NIKA-EXEC-002", "program not found: cargo test");
        let tip = example_tip("03-exec-pipeline", &exec, false, "ollama/llama3.1")
            .expect("the missing program earns its own tip");
        assert!(tip.contains("`cargo test`"), "{tip}");
        assert!(
            !tip.contains("mock/echo"),
            "no model swap for a missing binary: {tip}"
        );
        // An unparseable exec message still teaches, generically.
        let vague = failed("NIKA-EXEC-002", "spawn refused");
        let tip = example_tip("03-exec-pipeline", &vague, false, "ollama/llama3.1")
            .expect("the exec class still explains itself");
        assert!(tip.contains("nika try"), "{tip}");
        assert!(!tip.contains("mock/echo"), "{tip}");
    }

    /// Failure classes neither a model swap nor an install would rescue
    /// (builtin errors · workflow-level breaches with no failed record)
    /// stay silent — a tip that cannot help is noise.
    #[test]
    fn example_tip_stays_silent_on_unrescuable_classes() {
        let builtin = failed("NIKA-BUILTIN-READ-001", "cannot read ./missing.json");
        assert!(example_tip("01-hello", &builtin, false, "ollama/llama3.1").is_none());
        // A workflow-level failure with no failed task record (typed-output
        // breach) carries nothing to key on — silence, not a guess.
        let bare_fail = RunVerdict::bare(exit::WORKFLOW);
        assert!(example_tip("01-hello", &bare_fail, false, "ollama/llama3.1").is_none());
    }

    /// B06/C06: seatbelt in the try sandbox names cargo/git, even when
    /// `--model mock/echo` was passed (a model swap cannot conjure them).
    #[test]
    fn example_tip_seatbelt_names_the_missing_host_tool() {
        let belt = failed(
            "NIKA-SEC-001",
            "command blocked: seatbelt refused the confined process (status 128)",
        );
        let git = example_tip("standup-digest", &belt, true, "mock/echo")
            .expect("C06 must teach even under --model");
        assert!(git.contains("`git`"), "{git}");
        assert!(git.contains("nika new standup-digest"), "{git}");
        let cargo = example_tip("03-exec-pipeline", &belt, true, "mock/echo")
            .expect("B06 must teach even under --model");
        assert!(cargo.contains("`cargo`"), "{cargo}");
        assert!(
            example_tip("01-hello", &belt, true, "mock/echo").is_none(),
            "first-shelf jobs have no recover hint"
        );
    }
}
