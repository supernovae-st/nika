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
/// staged path, cwd lease and temporary-directory owner — the
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
fn stage_room(
    slug: &str,
    yaml: &str,
) -> Result<(std::path::PathBuf, crate::cwd::Lease, tempfile::TempDir), u8> {
    let stem = slug.strip_suffix(".nika").unwrap_or(slug).replace('/', "-");
    let scratch = tempfile::Builder::new()
        .prefix("nika-rehearsal-")
        .tempdir()
        .map_err(|e| {
            eprintln!("nika run: environment: cannot create rehearsal room: {e}");
            exit::ENV
        })?;
    let room = scratch.path().join(format!("nika-try-{stem}"));
    let path = room.join(format!("{stem}.nika"));
    if let Err(e) = std::fs::create_dir_all(&room).and_then(|()| std::fs::write(&path, yaml)) {
        eprintln!("nika run: environment: cannot stage example `{slug}`: {e}");
        return Err(exit::ENV);
    }
    if let Err(e) = nika_onboard::fixtures::materialize(yaml, &path) {
        eprintln!("nika run: environment: cannot stage the ingredients of `{slug}`: {e}");
        return Err(exit::ENV);
    }
    if let Err(e) = nika_onboard::rehearsal::stage(slug, &room) {
        eprintln!("nika run: environment: cannot stage rehearsal kit for `{slug}`: {e}");
        return Err(exit::ENV);
    }
    let lease = crate::cwd::enter(&room).map_err(|e| {
        eprintln!("nika run: environment: cannot enter the rehearsal room: {e}");
        exit::ENV
    })?;
    Ok((path, lease, scratch))
}

/// The staged try room is `…/nika-try-<stem>/<stem>.nika`.
/// Display uses this to name the rehearsal (C12 · UX-3) instead of a
/// path the sandbox is about to discard.
pub(super) fn try_rehearsal_slug(path: &str) -> Option<&str> {
    let path = std::path::Path::new(path);
    let parent = path.parent()?.file_name()?.to_str()?;
    let stem = parent.strip_prefix("nika-try-")?;
    let file = path.file_name()?.to_str()?;
    (file.strip_suffix(".nika") == Some(stem)).then_some(stem)
}

/// The slug `nika try` takes back for a staged room: the room's stem folds
/// a nested slug's `/` into `-` (`snippets/weekly-radar`), so the pack's
/// own list maps it back. `None` when no shipped example stages that stem.
pub(super) fn try_command_slug(stem: &str) -> Option<String> {
    if nika_pack::example(stem).is_some() {
        return Some(stem.to_owned());
    }
    nika_pack::example_slugs()
        .into_iter()
        .find(|slug| slug.replace('/', "-") == stem)
}

/// #1633 · the paused card under `nika try`. The room and its run are
/// discarded when the verb returns, so the card's `nika run --answer …
/// FILE` names a file nobody can reach; `nika try <slug> --answer …`
/// pre-seeds the gate on a fresh rehearsal, the form that finishes it
/// (`nika try --help` · `--answer` without `--resume`). `carry` is the
/// rehearsal's own flags ([`try_carry`]): a door that dropped them would
/// rerun another rehearsal (the mock seat, the inputs' defaults). Rewrites
/// exactly the two answer shapes the card speaks for `task`; any other
/// line, and any run outside a try room, passes through unchanged.
pub(super) fn try_answer_door(
    path: &str,
    task: &str,
    carry: &str,
    lines: Vec<String>,
) -> Vec<String> {
    let Some(slug) = try_rehearsal_slug(path).and_then(try_command_slug) else {
        return lines;
    };
    let doors = [
        (
            format!("nika run --answer {task}=true FILE"),
            format!("nika try {slug}{carry} --answer {task}=true"),
        ),
        (
            format!("nika run --answer {task}=\"your answer\" FILE"),
            format!("nika try {slug}{carry} --answer {task}=\"your answer\""),
        ),
    ];
    lines
        .into_iter()
        .map(|line| {
            doors
                .iter()
                .fold(line, |line, (run, try_door)| line.replace(run, try_door))
        })
        .collect()
}

/// The try verb's re-invocation flags (review round 1 · #1633), in the
/// try dialect and paste-safe, the way `nika run` threads `resume_carry`:
/// every `--var` and the RAW `--model` (`self` stays `self` — the resolved
/// seat reads as the mock default), the access pin, the cost ceiling, a
/// `--resume` trace and the answers already given. Display flags
/// (`--plain` · `--quiet` · `--no-progress`) change no outcome and stay
/// out, and so do the kit's supplied vars: the next `nika try` supplies
/// them itself.
fn try_carry(
    model_flag: Option<&str>,
    access_pin: Option<&str>,
    vars: &[String],
    max_cost_usd: Option<f64>,
    (answers, resume): (&[String], Option<&std::path::Path>),
) -> String {
    use nika_cli_host::output::sh_word;
    use std::fmt::Write as _;
    let mut carry = super::epilogue::resume_carry(vars, model_flag);
    // write! to a String is infallible.
    if let Some(access) = access_pin {
        let _ = write!(carry, " --access {}", sh_word(access));
    }
    if let Some(usd) = max_cost_usd {
        let _ = write!(carry, " --max-cost-usd {usd}");
    }
    if let Some(trace) = resume {
        let _ = write!(carry, " --resume {}", sh_word(&trace.to_string_lossy()));
    }
    for answer in answers {
        let _ = write!(carry, " --answer {}", sh_word(answer));
    }
    carry
}

/// UX-3 · every try card: how to own the file.
pub(super) fn try_own_file_line(slug: &str) -> String {
    format!("rehearsal · {slug}. explore authoring skeletons: nika compile --list")
}

/// The hosts a workflow's `permits.net.http` lets it reach, as declared
/// (empty when the block or its `net:` category is absent).
pub(crate) fn declared_hosts(wf: &nika_schema::raw::RawWorkflow) -> &[String] {
    wf.permits
        .as_ref()
        .and_then(|permits| permits.value.net.as_ref())
        .map(|net| net.http.as_slice())
        .unwrap_or_default()
}

/// #1605 · the try card's network line, printed before the run:
/// the mock seat replaces the model, never the network, so a rehearsal
/// whose `permits.net.http` names hosts says which ones it may reach.
/// `None` when the workflow declares no host or the seat is a real model
/// (`--model <provider/model>` · `self` on a real `model:`): nothing there
/// promises otherwise.
pub(super) fn try_reach_line(
    wf: &nika_schema::raw::RawWorkflow,
    model_override: Option<&str>,
) -> Option<String> {
    let hosts = declared_hosts(wf);
    let seat = model_override.or_else(|| wf.model.as_ref().map(|m| m.value.as_str()));
    if hosts.is_empty() || !seat.is_some_and(|m| m.starts_with("mock/")) {
        return None;
    }
    Some(format!(
        "may reach: {} — the mock replaces the model, not the network",
        hosts.join(" · ")
    ))
}

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

/// `nika try <slug>` — execute one EMBEDDED example through the real
/// runtime. The default seat is the mock model (zero keys · no model
/// call); every other effect stays real, so a step that fetches still
/// reaches its host and the try card names those hosts before the run
/// (#1605). Stages the embedded YAML to a temp file (the verb reads a
/// path) and runs it.
///
/// `model_override` — `Some(m)` (from `--model m`) swaps the example's
/// envelope model for `m` (so `--model mock/echo` previews with no model
/// call). On a FAILED run with NO override, a rescue tip keyed on the
/// failure KIND is printed to stderr (#145: the mock-preview nudge for
/// infer/provider failures · the real missing dependency for an exec
/// `program not found`) · the original exit code is returned unchanged.
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
    // the mock rehearsal (no model call · other effects stay real) ·
    // `self` = the example's own seat.
    let model_override = crate::verbs::examples::rehearsal_seat(model_flag);
    let Some(yaml) = nika_pack::example(slug) else {
        eprintln!("unknown example `{slug}` — bare `nika try` names the embedded set");
        return exit::FILE;
    };
    // Pin `--resume` to the operator cwd before the room chdir.
    let operator_cwd = std::env::current_dir().ok();
    let resume_req = try_gate_request(answers, resume, operator_cwd.as_deref());
    let (path, room_lease, scratch) = match stage_room(slug, yaml) {
        Ok(pair) => pair,
        Err(code) => return code,
    };
    let (mode, theme) = example_mode(quiet, no_progress, theme);
    if mode == RenderMode::Live {
        example_predisplay(slug, yaml, theme);
    }
    let mut run_vars = vars.to_vec();
    run_vars.extend(nika_onboard::rehearsal::supplied_vars(slug, vars));
    let carry = try_carry(
        model_flag,
        access_pin,
        vars,
        max_cost_usd,
        (answers, resume),
    );
    let verdict = run_verdict(
        &path.to_string_lossy(),
        false,
        None,
        theme,
        mode,
        false,
        model_override,
        access_pin,
        super::inputs::InputBindings::Rehearsal(&run_vars, &carry),
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
        let clean = slug.strip_suffix(".nika").unwrap_or(slug);
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
    drop(scratch);
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
    let file = format!("{}.nika", slug.strip_suffix(".nika").unwrap_or(slug));
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
/// model or won't parse). Best-effort — drives only the mock-preview hint
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
    // A confinement refusal does not establish that a tool is absent.
    // Keep the original finding and name only the example dependency.
    if failure.code == "NIKA-SEC-001"
        && let Some(hint) = nika_pack::try_recover_hint(slug)
    {
        let slug = slug.strip_suffix(".nika").unwrap_or(slug);
        return Some(format!(
            "tip: this example calls `{}` inside its declared sandbox; inspect the refusal above.\n        authoring skeletons: nika compile --list (example: {slug})",
            hint.missing
        ));
    }
    if override_given {
        return None;
    }
    // Infer/provider failures — the "no local model running" case: the
    // mock preview is the bare `nika try` (the funnel's highest-intent P0).
    // It replaces the model only, so the tip never promises « offline »
    // (#1605 · a step that fetches still reaches its host).
    if failure.code.starts_with("NIKA-INFER-") {
        if model.is_empty() || model == "mock/echo" {
            return None;
        }
        return Some(format!(
            "tip: no local model running? preview this example on the mock model →\n        nika try {slug}"
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
                "tip: this example shells out to `{p}` — not found on this machine;\n        install it, or browse the other examples → nika try"
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
            try_rehearsal_slug("/tmp/nika-try-competitor-radar/competitor-radar.nika"),
            Some("competitor-radar")
        );
        assert_eq!(
            try_rehearsal_slug("/tmp/nika-try-01-hello/01-hello.nika"),
            Some("01-hello")
        );
        assert_eq!(
            try_rehearsal_slug("hello.nika"),
            None,
            "a workspace run is not a try rehearsal"
        );
        assert_eq!(
            try_own_file_line("competitor-radar"),
            "rehearsal · competitor-radar. explore authoring skeletons: nika compile --list"
        );
    }

    /// #1633 · a paused rehearsal teaches the try door it can take: the
    /// card's `nika run --answer … FILE` named a discarded room. Both
    /// answer shapes move, a nested slug keeps its `/`, and a workspace
    /// run keeps the run door.
    #[test]
    fn a_paused_try_rehearsal_answers_on_the_try_door() {
        let theme = crate::Theme::new(false, false, false);
        let card = |events: Vec<nika_event::Event>| {
            let mut view = crate::RunView::new();
            for event in events {
                view.apply(&event);
            }
            crate::display::render::stream_summary(&view, &theme, &[])
        };
        let staged = "/tmp/nika-try-release-notes/release-notes.nika";
        let confirm =
            try_answer_door(staged, "summarize", "", card(crate::demo::paused())).join("\n");
        assert!(
            confirm.contains(
                "answer: nika try release-notes --answer summarize=true  \
                 (boolean true/false, not yes)"
            ),
            "{confirm}"
        );
        assert!(
            !confirm.contains("nika run --answer") && !confirm.contains("FILE"),
            "{confirm}"
        );
        let input_gate = {
            use nika_types::resource::{KeyValue, Value};
            let mut events = crate::demo::paused();
            events.pop();
            events.push(
                crate::demo::bare_event(nika_event::EventKind::WorkflowPaused, 4400)
                    .with_field(KeyValue::new("task", Value::String("approve".to_owned())))
                    .with_field(KeyValue::new("mode", Value::String("input".to_owned()))),
            );
            events
        };
        let input = try_answer_door(staged, "approve", "", card(input_gate)).join("\n");
        assert!(
            input.contains("answer: nika try release-notes --answer approve=\"your answer\""),
            "{input}"
        );
        assert!(!input.contains("FILE"), "{input}");
        let nested = "/tmp/nika-try-snippets-weekly-radar/snippets-weekly-radar.nika";
        let nested =
            try_answer_door(nested, "summarize", "", card(crate::demo::paused())).join("\n");
        assert!(
            nested.contains("nika try snippets/weekly-radar --answer summarize=true"),
            "the slug the verb accepts, not the room's stem: {nested}"
        );
        let owned = try_answer_door("gate.nika", "summarize", "", card(crate::demo::paused()));
        assert_eq!(
            owned,
            card(crate::demo::paused()),
            "a workspace run keeps the run door"
        );
    }

    /// Review round 1 · `nika try release-notes --model self --var
    /// announce=true` paused on `approve`, and its door dropped both flags:
    /// pasted, it reran on the mock seat with `announce` back to false. The
    /// door carries the try verb's own flags — `self` as typed, never the
    /// resolved seat.
    #[test]
    fn the_try_door_carries_the_rehearsal_flags() {
        let carry = try_carry(
            Some("self"),
            None,
            &["announce=true".to_owned()],
            None,
            (&[], None),
        );
        assert_eq!(carry, " --var announce=true --model self");
        let theme = crate::Theme::new(false, false, false);
        let mut view = crate::RunView::new();
        for event in crate::demo::paused() {
            view.apply(&event);
        }
        let card = crate::display::render::stream_summary(&view, &theme, &[]);
        let door = try_answer_door(
            "/tmp/nika-try-release-notes/release-notes.nika",
            "summarize",
            &carry,
            card,
        )
        .join("\n");
        assert!(
            door.contains(
                "answer: nika try release-notes --var announce=true --model self \
                 --answer summarize=true"
            ),
            "{door}"
        );
        let every = try_carry(
            Some("openai/gpt-5.2"),
            Some("api"),
            &["topic=two words".to_owned()],
            Some(0.05),
            (
                &["first=yes".to_owned()],
                Some(std::path::Path::new("t.ndjson")),
            ),
        );
        assert_eq!(
            every,
            " --var 'topic=two words' --model openai/gpt-5.2 --access api \
             --max-cost-usd 0.05 --resume t.ndjson --answer first=yes"
        );
        assert_eq!(try_carry(None, None, &[], None, (&[], None)), "");
        assert_eq!(
            super::super::epilogue::unsafe_to_paste(&format!("nika try x{every}")),
            None,
            "every carried word is paste-safe"
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
    /// earns the mock-preview nudge; a missing program names the real
    /// dependency instead of suggesting a model swap that cannot fix it.
    #[test]
    fn example_tip_keys_on_the_failure_kind() {
        let infer = failed("NIKA-INFER-001", "provider call failed: model not found");
        // FAIL on infer + no override + a local model → the right nudge.
        let tip = example_tip("01-hello", &infer, false, "ollama/llama3.1")
            .expect("the infer failure earns the mock-preview nudge");
        // V5: `try` rehearses on the mock model by default — the rescue
        // line is the bare command again, no `--model` flag to teach.
        assert!(tip.contains("nika try 01-hello"), "{tip}");
        // #1605 · the mock replaces the model, not the network.
        assert!(
            tip.contains("preview this example on the mock model"),
            "{tip}"
        );
        assert!(!tip.contains("offline"), "{tip}");
        // A clean run never needs the tip.
        let ok = RunVerdict::bare(exit::OK);
        assert!(example_tip("01-hello", &ok, false, "ollama/llama3.1").is_none());
        // The user already overrode the model · suggesting it again is noise.
        assert!(example_tip("01-hello", &infer, true, "ollama/llama3.1").is_none());
        // mock/echo needs no provider · a failure there is a real bug, not
        // a missing local model — so the mock-preview tip would mislead.
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
        // #1605 · the shelf holds examples that fetch: none is
        // « offline-friendly » by being on it.
        assert!(
            tip.contains("install it, or browse the other examples → nika try"),
            "{tip}"
        );
        assert!(!tip.contains("offline"), "{tip}");
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

    /// A sandbox refusal must not invent a missing binary or exit status.
    #[test]
    fn example_tip_seatbelt_preserves_the_actual_cause() {
        let belt = failed(
            "NIKA-SEC-001",
            "command blocked: seatbelt refused the confined process (status 128)",
        );
        let git = example_tip("standup-digest", &belt, true, "mock/echo")
            .expect("C06 must teach even under --model");
        assert!(git.contains("`git`"), "{git}");
        assert!(git.contains("nika compile --list"), "{git}");
        assert!(!git.contains("has no"), "{git}");
        assert!(!git.contains("exit 127"), "{git}");
        assert!(!git.contains("empty log"), "{git}");
        assert!(example_tip("03-exec-pipeline", &belt, true, "mock/echo").is_none());
        assert!(example_tip("snippets/run", &belt, true, "mock/echo").is_none());
        assert!(
            example_tip("01-hello", &belt, true, "mock/echo").is_none(),
            "first-shelf jobs have no recover hint"
        );
    }

    fn parsed(yaml: &str) -> nika_schema::raw::RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    /// #1605 · the mock seat replaces the model, never the
    /// network: the try card names every `permits.net.http` host, in the
    /// declared order, and says nothing when no host is declared or the
    /// seat is a real model.
    #[test]
    fn the_try_card_names_the_hosts_a_mock_rehearsal_reaches() {
        let fetch = parsed(
            "nika: two-hosts\npermits:\n  tools: [\"nika:fetch\"]\n  net: { http: [\"nika.sh\", \"docs.nika.sh\"] }\ntasks:\n  page:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://nika.sh\" } }\n",
        );
        assert_eq!(declared_hosts(&fetch), ["nika.sh", "docs.nika.sh"]);
        assert_eq!(
            try_reach_line(&fetch, Some("mock/echo")).as_deref(),
            Some(
                "may reach: nika.sh · docs.nika.sh — the mock replaces the model, not the network"
            )
        );
        assert_eq!(
            try_reach_line(&fetch, Some("ollama/qwen3")),
            None,
            "a real seat promises nothing offline"
        );
        assert_eq!(
            try_reach_line(&fetch, None),
            None,
            "`--model self` on a file with no model is no mock rehearsal"
        );

        let own_mock = parsed(
            "nika: own-mock\nmodel: mock/echo\npermits:\n  tools: [\"nika:fetch\"]\n  net: { http: [\"nika.sh\"] }\ntasks:\n  page:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://nika.sh\" } }\n",
        );
        assert_eq!(
            try_reach_line(&own_mock, None).as_deref(),
            Some("may reach: nika.sh — the mock replaces the model, not the network"),
            "`--model self` on a mock `model:` is still a mock rehearsal"
        );

        for quiet in [
            "nika: no-permits\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
            "nika: no-net\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
            "nika: empty-net\npermits:\n  net: { http: [] }\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
        ] {
            let wf = parsed(quiet);
            assert!(declared_hosts(&wf).is_empty(), "{quiet}");
            assert_eq!(try_reach_line(&wf, Some("mock/echo")), None, "{quiet}");
        }
    }

    /// The same law over the shipped shelf: every embedded example's try
    /// card names exactly its declared hosts under the default seat, and
    /// the measured #1605 cases read what they reach.
    #[test]
    fn every_shelf_example_names_exactly_its_declared_hosts() {
        let seat = crate::verbs::examples::rehearsal_seat(None);
        let mut named = 0;
        for slug in nika_pack::example_slugs() {
            let yaml = nika_pack::example(&slug).expect("listed slug embeds");
            let wf = parsed(yaml);
            let hosts = declared_hosts(&wf);
            match try_reach_line(&wf, seat) {
                Some(line) => {
                    named += 1;
                    assert_eq!(
                        line,
                        format!(
                            "may reach: {} — the mock replaces the model, not the network",
                            hosts.join(" · ")
                        ),
                        "{slug}"
                    );
                }
                None => assert!(hosts.is_empty(), "{slug} declares {hosts:?} but names none"),
            }
        }
        assert!(named > 0, "the shelf holds examples that fetch");
        for (slug, want) in [
            ("snippets/use-a-tool", "may reach: nika.sh"),
            ("snippets/weekly-radar", "may reach: hnrss.org"),
            (
                "ceo-monday-brief",
                "may reach: hn.algolia.com · hooks.slack.com",
            ),
        ] {
            let wf = parsed(nika_pack::example(slug).expect("measured example ships"));
            let line = try_reach_line(&wf, seat).expect("a fetching rehearsal names its hosts");
            assert!(line.starts_with(&format!("{want} — ")), "{slug}: {line}");
        }
        let hello = parsed(nika_pack::example("01-hello").expect("hello ships"));
        assert_eq!(
            try_reach_line(&hello, seat),
            None,
            "01-hello declares no host"
        );
    }
}
