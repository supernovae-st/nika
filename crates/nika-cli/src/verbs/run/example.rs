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
fn stage_room(
    slug: &str,
    yaml: &str,
) -> Result<(std::path::PathBuf, Option<std::path::PathBuf>), u8> {
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
    let previous = std::env::current_dir().ok();
    if let Err(e) = std::env::set_current_dir(&room) {
        eprintln!("nika run: environment: cannot enter the rehearsal room: {e}");
        return Err(exit::ENV);
    }
    Ok((path, previous))
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
#[must_use]
#[allow(clippy::fn_params_excessive_bools)] // the run trio, verbatim (two switches)
pub fn example(
    slug: &str,
    model_flag: Option<&str>,
    access_pin: Option<&str>,
    vars: &[String],
    (quiet, no_progress): (bool, bool),
    max_cost_usd: Option<f64>,
    theme: Theme,
) -> u8 {
    // V5 seat law (RAMS-4): the raw --model flag resolves here — bare =
    // the offline mock rehearsal · `self` = the example's own seat.
    let model_override = crate::verbs::examples::rehearsal_seat(model_flag);
    let Some(yaml) = nika_pack::example(slug) else {
        eprintln!("unknown example `{slug}` — bare `nika try` names the embedded set");
        return exit::FILE;
    };
    let (path, previous_dir) = match stage_room(slug, yaml) {
        Ok(pair) => pair,
        Err(code) => return code,
    };
    // The example renders live on a TTY, plain when piped, silent on
    // `--quiet` (the verdict line still lands).
    let mode = if quiet {
        RenderMode::Quiet
    } else if !no_progress && std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        RenderMode::Live
    } else {
        RenderMode::Plain
    };
    if mode == RenderMode::Live {
        example_predisplay(slug, yaml, theme);
    }
    // The interactive duration accents follow the same TTY gate; heat
    // additionally needs colour + the truecolor proof.
    let mut theme = theme;
    theme.accents = mode == RenderMode::Live;
    theme.heat = theme.accents && theme.color && crate::verbs::truecolor_env();
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
        None,
        // No run journal: the example is staged to a TEMP file — `.nika/
        // traces/` belongs to workspace runs (the same drive underneath,
        // deliberately disabled here).
        true,
        // Examples always run whole (tiny by design · no scoping surface).
        None,
        false,
        max_cost_usd,
        // An example runs a TEMP-staged file, not a workspace run — the
        // workspace's trace store is not this invocation's to collect.
        true,
        false, // examples are engine-staged content — unsigned-tolerant
        false, // examples are one-shot runs, not the outer TTY thread
        None,
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
    if verdict.code == exit::OK && mode != RenderMode::Quiet {
        let clean = slug.strip_suffix(".nika.yaml").unwrap_or(slug);
        eprintln!(
            "\n  {}",
            crate::display::vocab::hint(theme, "make it yours", &format!("nika new {clean}"))
        );
    }
    // Leave the room as we found it — the rehearsal is isolated, not
    // a relocation of the operator's session.
    if let Some(dir) = previous_dir {
        let _ = std::env::set_current_dir(dir);
    }
    verdict.code
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
    if verdict.code == exit::OK || override_given {
        return None;
    }
    let failure = verdict.failure.as_ref()?;
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

    /// A failed verdict carrying one typed task error (the policy's input).
    fn failed(code: &str, message: &str) -> RunVerdict {
        RunVerdict {
            code: exit::WORKFLOW,
            failure: Some(nika_runtime::TaskErrorRecord {
                code: code.to_owned(),
                message: message.to_owned(),
                transient: false,
            }),
            paused: None,
            outputs: std::collections::BTreeMap::new(),
            interrupted: false,
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
}
