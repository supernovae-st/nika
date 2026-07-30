// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--resume`/`--answer` surface (ADR-099 · F4) — validated and
//! folded BEFORE the runtime composition. Extracted from `mod.rs`
//! 2026-07-30 (the 1500-LOC cap · the same descent pressure as
//! `budget.rs`); the semantics are unchanged.

// The run verb streams live output (the render IS the run) — the same
// sanctioned exemption `mod.rs` carries.
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeMap;

use nika_dap::resume::ResumeRequest;
use nika_runtime::resume::ResumePlan;
use nika_schema::raw::RawWorkflow;
use serde_json::Value;

use super::epilogue;
use super::recover_events;
use crate::verbs::exit;

/// The validated `--resume`/`--answer` inputs the composition consumes.
pub(super) struct ResumeSetup {
    /// The folded skip plan (`None` = no `--resume` requested).
    pub plan: Option<ResumePlan>,
    /// The validated `--answer task=value` map (empty without answers).
    pub answers: BTreeMap<String, Value>,
    /// The F-P4 resume authority (NEP-0013) — the approval ticket folded
    /// from the paused trace (`None` on a fresh run or a pre-F-P4 trace).
    pub paused: Option<nika_runtime::approval::PausedApproval>,
    /// The F-P21 declared compat (NEP-0014 law 4) — the recorded engine
    /// version the operator allowed the crossing from (`Some` only when
    /// a cross-version resume proceeds under `--resume-compat`).
    pub compat: Option<String>,
}

/// Validate + fold the whole `--resume` surface (plan · `--from` ·
/// `--answer`) BEFORE composing — every refusal is the ENV class,
/// already printed + enveloped.
///
/// # Errors
///
/// The exit code to return unchanged.
pub(super) fn resume_setup(
    resume: Option<&ResumeRequest>,
    wf: &RawWorkflow,
    output_json: bool,
) -> Result<ResumeSetup, u8> {
    let (plan, paused, compat) = match resume {
        None => (None, None, None),
        Some(req) => match req.trace.as_deref() {
            // The answers-only form (F4): no trace, no plan — the answers
            // below ride into the gate map and wait for the ask.
            None => (None, None, None),
            Some(trace) => {
                let (plan, paused, compat) = load_resume_plan(req, trace, wf, output_json)?;
                (Some(plan), paused, compat)
            }
        },
    };
    let answers =
        nika_dap::resume::parse_answers(resume.map_or(&[][..], |r| r.answers.as_slice()), wf)
            .map_err(|message| {
                eprintln!("nika run: {message}");
                epilogue::emit_error_envelope(&message, output_json);
                exit::ENV
            })?;
    Ok(ResumeSetup {
        plan,
        answers,
        paused,
        compat,
    })
}

/// Read + fold the `--resume` trace into the runtime skip plan (ADR-099)
/// plus the F-P4 paused ticket (NEP-0013) plus the F-P21 version verdict
/// (NEP-0014 law 4). The cross-version judgment comes FIRST: a resume
/// under an engine different from the recording one is an explicit
/// refusal naming both versions — or rides a declared compat
/// (`--resume-compat` · attested on the run's boot manifest). Honest
/// degradation stays the contract for the KEYS: a keyless trace (older
/// engine) yields an EMPTY plan + a notice — never an error; an
/// unreadable file or an unknown `--from` id is refused loudly
/// (environment class).
///
/// # Errors
///
/// The exit code (already printed + enveloped) — ENV for every refusal.
fn load_resume_plan(
    req: &ResumeRequest,
    trace: &std::path::Path,
    wf: &RawWorkflow,
    output_json: bool,
) -> Result<
    (
        ResumePlan,
        Option<nika_runtime::approval::PausedApproval>,
        Option<String>,
    ),
    u8,
> {
    let label = trace.display().to_string();
    let refuse = |message: String| {
        eprintln!("nika run: {message}");
        epilogue::emit_error_envelope(&message, output_json);
        exit::ENV
    };
    let raw = std::fs::read_to_string(trace)
        .map_err(|e| refuse(format!("--resume: cannot read {label}: {e}")))?;
    let recovered =
        recover_events(&raw, &label).map_err(|message| refuse(format!("--resume: {message}")))?;
    if let Some(note) = &recovered.truncated_note {
        eprintln!("nika run: {note}");
    }
    // F-P21 (NEP-0014 law 4) — the version judgment BEFORE the fold:
    // judged, never assumed (the silent cross-version degradation dies).
    let judgment = nika_dap::resume::judge_version(&recovered.events, env!("CARGO_PKG_VERSION"));
    let compat = match nika_dap::resume::judge_resume(&judgment, req.compat.as_deref()) {
        nika_dap::resume::CompatVerdict::Proceed { compat_with } => {
            if let Some(recorded) = &compat_with {
                eprintln!(
                    "nika run: --resume: cross-version compat declared — the trace was \
                     recorded under engine {recorded}, this engine is {} (attested on \
                     the run's boot manifest)",
                    env!("CARGO_PKG_VERSION")
                );
            }
            compat_with
        }
        nika_dap::resume::CompatVerdict::Refuse(message) => {
            return Err(refuse(format!("--resume: {message}")));
        }
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and caller ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown compat verdict: {other:?}"),
    };
    let fold = nika_dap::resume::fold_plan(&recovered.events);
    if fold.plan.is_empty() {
        // Nothing skippable — an older engine's trace or a run with no
        // journaled successes. The run proceeds fully live (never an error).
        eprintln!("nika run: --resume: {label} carries no resume keys — running everything live");
    } else if fold.keyless + fold.unreadable > 0 {
        eprintln!(
            "nika run: --resume: {} record(s) without a usable resume key — those tasks run live",
            fold.keyless + fold.unreadable
        );
    }
    let mut plan = fold.plan;
    if let Some(from) = &req.from {
        nika_dap::resume::apply_from(&mut plan, wf, from)
            .map_err(|message| refuse(format!("--resume: {message}")))?;
    }
    Ok((plan, fold.paused, compat))
}
