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
use nika_runtime::resume::{ResumePlan, ResumeUnverified};
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
    /// The ADR-099 trust amendment attestation (2026-08-08) — the
    /// resume's trust posture when the chain precondition did NOT hold
    /// (the declared `--resume-unverified` opt-out · the chainless
    /// compat), journaled on the run's boot manifest so a trace that
    /// failed — or carried no — verification can never launder into one
    /// that silently passed. `None` = the chain verified (or no resume).
    pub unverified: Option<ResumeUnverified>,
}

/// The folded parts of one `--resume <trace>` (the skip plan · the F-P4
/// ticket · the F-P21 compat · the trust attestation) — a named return:
/// the four-tuple it replaces had grown past readability (the clippy
/// wall agreed).
struct LoadedResume {
    /// The folded skip plan (possibly empty — honest degradation).
    plan: ResumePlan,
    /// The F-P4 paused ticket (`None` on a pause-free or pre-F-P4 trace).
    paused: Option<nika_runtime::approval::PausedApproval>,
    /// The F-P21 declared compat (`Some` on a discharged crossing).
    compat: Option<String>,
    /// The trust-amendment attestation (`Some` when the run proceeds
    /// WITHOUT a verified chain — the declared opt-out past a finding,
    /// or the chainless-capture compat — the posture + finding to
    /// journal).
    unverified: Option<ResumeUnverified>,
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
    source: &str,
    model_override: Option<&str>,
    output_json: bool,
) -> Result<ResumeSetup, u8> {
    let loaded = match resume {
        None => None,
        Some(req) => match req.trace.as_deref() {
            // The answers-only form (F4): no trace, no plan — the answers
            // below ride into the gate map and wait for the ask.
            None => None,
            Some(trace) => Some(load_resume_plan(
                req,
                trace,
                wf,
                source,
                model_override,
                output_json,
            )?),
        },
    };
    let answers =
        nika_dap::resume::parse_answers(resume.map_or(&[][..], |r| r.answers.as_slice()), wf)
            .map_err(|message| {
                eprintln!("nika run: {message}");
                epilogue::emit_error_envelope(&message, output_json);
                exit::ENV
            })?;
    Ok(match loaded {
        Some(l) => ResumeSetup {
            plan: Some(l.plan),
            answers,
            paused: l.paused,
            compat: l.compat,
            unverified: l.unverified,
        },
        None => ResumeSetup {
            plan: None,
            answers,
            paused: None,
            compat: None,
            unverified: None,
        },
    })
}

/// Read + fold the `--resume` trace into the runtime skip plan (ADR-099)
/// plus the F-P4 paused ticket (NEP-0013) plus the F-P21 version verdict
/// (NEP-0014 law 4). The TRUST judgment comes FIRST (ADR-099 trust
/// amendment · 2026-08-08): a resume serves the trace's recorded
/// successes as cache hits and runs live tasks on their values, so the
/// tamper-evidence chain is verified BEFORE anything is folded — the
/// forgery class refuses (the FILE class, one voice with `trace
/// verify`), or rides the NAMED opt-out (`--resume-unverified` ·
/// attested on the run's boot manifest, never a silent default); a
/// chainless trace (a stream capture · a pre-0.96 journal) proceeds
/// under the compat, ATTESTED the same (`resume_unverified: unchained`)
/// so the strip-the-chain forgery never launders silently. The
/// cross-version judgment follows: a resume under an engine different
/// from the recording one is an explicit refusal naming both versions —
/// or rides a declared compat (`--resume-compat`). Honest degradation
/// stays the contract for the KEYS: a keyless trace (older engine)
/// yields an EMPTY plan + a notice — never an error; an unreadable file
/// or an unknown `--from` id is refused loudly (environment class).
///
/// # Errors
///
/// The exit code (already printed + enveloped) — FILE for the tamper
/// class, ENV for every other refusal.
fn load_resume_plan(
    req: &ResumeRequest,
    trace: &std::path::Path,
    wf: &RawWorkflow,
    source: &str,
    model_override: Option<&str>,
    output_json: bool,
) -> Result<LoadedResume, u8> {
    let label = trace.display().to_string();
    let refuse = |message: String| {
        eprintln!("nika run: {message}");
        epilogue::emit_error_envelope(&message, output_json);
        exit::ENV
    };
    let raw = std::fs::read_to_string(trace)
        .map_err(|e| refuse(format!("--resume: cannot read {label}: {e}")))?;
    // ADR-099 trust amendment — the chain verdict BEFORE the fold (own
    // fn: the 100-line wall, and the judgment belongs to itself).
    let unverified = gate_trust(&raw, &label, req.allow_unverified, output_json)?;
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
    judge_seat(
        &recovered.events,
        source,
        model_override,
        &label,
        output_json,
    )?;
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
    Ok(LoadedResume {
        plan,
        paused: fold.paused,
        compat,
        unverified,
    })
}

/// The ADR-099 trust gate — the chain verdict BEFORE the fold (the same
/// walk `nika trace verify` runs), mapped to the run's exit classes and
/// the boot-manifest attestation. `Ok(None)` = the chain verified, no
/// claim journaled; `Ok(Some(_))` = the run proceeds WITHOUT a verified
/// chain and the manifest says so; `Err` = the tamper class, refused
/// (the FILE class, one voice with `trace verify`).
fn gate_trust(
    raw: &str,
    label: &str,
    allow_unverified: bool,
    output_json: bool,
) -> Result<Option<ResumeUnverified>, u8> {
    use nika_dap::resume::TrustVerdict;
    match nika_dap::resume::judge_trust(raw) {
        TrustVerdict::Verified => Ok(None),
        // The chainless capture (`--json > t.ndjson` · a pre-0.96
        // journal): nothing to verify — the compat proceeds, but it is
        // SAID on stderr AND attested on the boot manifest. The
        // strip-the-chain forgery (tamper, then delete every `chain`
        // field) converts the walker's Broken into Unchained — it lands
        // here, and it never launders silently.
        TrustVerdict::Unverifiable => {
            eprintln!(
                "nika run: --resume: {label} carries no tamper-evidence chain (a stream \
                 capture or a pre-0.96 journal) — the records are trusted WITHOUT \
                 verification (attested on the run's boot manifest)"
            );
            Ok(Some(ResumeUnverified::Unchained(
                "the trace carries no tamper-evidence chain (a stream capture or a pre-0.96 \
                 journal) — the records were trusted without verification"
                    .to_owned(),
            )))
        }
        TrustVerdict::Tampered { finding } => {
            if allow_unverified {
                eprintln!(
                    "nika run: --resume: {finding} — proceeding under --resume-unverified: \
                     the records are trusted WITHOUT chain verification (attested on the \
                     run's boot manifest)"
                );
                Ok(Some(ResumeUnverified::Declared(finding)))
            } else {
                let message = format!(
                    "--resume: {finding}\n  a resume trusts the trace's recorded successes — \
                     the chain verdict is the resume's precondition (ADR-099)\n  verify: \
                     nika trace verify {label} · or re-run fresh · or resume under \
                     --resume-unverified (attested on the run's boot manifest)"
                );
                eprintln!("nika run: {message}");
                epilogue::emit_error_envelope(&message, output_json);
                Err(exit::FILE)
            }
        }
        // TrustVerdict is #[non_exhaustive]: a class newer than this CLI
        // refuses — fail closed, never a guessed trust (the `trace
        // verify` unknown-verdict posture).
        _ => {
            let message = format!(
                "--resume: {label}: unknown chain verdict class — the forensics library is \
                 newer than this CLI"
            );
            eprintln!("nika run: {message}");
            epilogue::emit_error_envelope(&message, output_json);
            Err(exit::ENV)
        }
    }
}

/// The two judgments about WHICH SEAT the resumed legs will run on —
/// extracted from [`load_resume_plan`] at the 100-line fn wall, and they
/// belong together anyway: both answer "is the model this resume uses
/// the model the recording ran on?", one from the flag and one from the
/// file.
///
/// - **The flag** (issue 772) · a run recorded under `--model` must never
///   SILENTLY resume on the envelope model — the mock-previewed run that
///   comes back on a priced seat. Explicit argv wins; silence REFUSES,
///   naming the recorded seat and the exact flag.
/// - **The file** (adversarial review 2026-08-03) · the flag judgment
///   alone was not enough: the envelope `model:` is one line in a file
///   the operator can edit between the pause and the resume, and the seat
///   moves with it, no flag involved. The file stays the source of truth
///   (ADR-099 · an explicit edit re-runs, it never serves a stale
///   output), so this NOTICES rather than refuses — but it does notice.
///   The comparator is the replay session's, content-aware: a CRLF/BOM
///   re-encode is not a change.
fn judge_seat(
    events: &[nika_event::Event],
    source: &str,
    model_override: Option<&str>,
    label: &str,
    output_json: bool,
) -> Result<(), u8> {
    match nika_dap::resume::judge_model(
        nika_dap::resume::trace_model_override(events).as_deref(),
        model_override,
    ) {
        nika_dap::resume::ModelVerdict::Proceed { changed } => {
            if let Some((recorded, declared)) = changed {
                eprintln!(
                    "nika run: --resume: model change declared — the trace was recorded \
                     under --model {recorded}, this resume runs --model {declared}"
                );
            }
        }
        nika_dap::resume::ModelVerdict::Refuse(message) => {
            let message = format!("--resume: {message}");
            eprintln!("nika run: {message}");
            epilogue::emit_error_envelope(&message, output_json);
            return Err(exit::ENV);
        }
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and caller ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown model verdict: {other:?}"),
    }
    if nika_dap::resume::source_drifted(source, events) == Some(true) {
        eprintln!(
            "nika run: --resume: the workflow file CHANGED since {label} recorded it — the \
             current bytes are what runs (an edited `model:` moves the seat, and edited tasks \
             re-run instead of serving the recorded output)"
        );
    }
    Ok(())
}
