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
use nika_runtime::approval::PausedApproval;
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
    pub paused: Option<PausedApproval>,
    /// The F-P21 declared compat (NEP-0014 law 4) — the recorded engine
    /// version the operator allowed the crossing from (`Some` only when
    /// a cross-version resume proceeds under `--resume-compat`).
    pub compat: Option<String>,
    /// The ADR-099 trust attestation (2026-08-08) — `Some` when the run
    /// proceeds WITHOUT a verified chain (the declared opt-out · the
    /// chainless compat), journaled on the boot manifest so no unverified
    /// ancestor launders silently. `None` = the chain verified (or no resume).
    pub unverified: Option<ResumeUnverified>,
    /// The trace this run CONTINUES (#1462) — the resumed journal's own
    /// trace id, attested on the boot manifest and the settlement so a
    /// continuation renders as one. `None` = a fresh run.
    pub resumed_from: Option<String>,
}

impl ResumeSetup {
    /// Attach durable replay protection only after Run monetary admission.
    pub(super) fn bind_durable(mut self, output_json: bool) -> Result<Self, u8> {
        self.paused = durable_paused(self.paused, output_json)?;
        Ok(self)
    }

    /// A fresh run (no trace to fold) — the answers ride in later.
    const fn fresh() -> Self {
        Self {
            plan: None,
            answers: BTreeMap::new(),
            paused: None,
            compat: None,
            unverified: None,
            resumed_from: None,
        }
    }
}

/// The ENV-class refusal, one voice: said on stderr, enveloped on the
/// machine face, and the exit code returned for `?`.
fn refuse_env(message: &str, output_json: bool) -> u8 {
    eprintln!("nika run: {message}");
    epilogue::emit_error_envelope(message, output_json);
    exit::ENV
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
    access: (&nika_providers::ExecutionAccessPlan, Option<&str>),
    output_json: bool,
) -> Result<ResumeSetup, u8> {
    validate_resume(resume, wf, source, model_override, access, output_json)?
        .bind_durable(output_json)
}

/// Read, validate and fold the resume without opening or pruning durable claims.
/// Run can reject a changed access lane before asking for spending authority.
/// The returned setup must bind its durable replay protection before execution.
pub(super) fn validate_resume(
    resume: Option<&ResumeRequest>,
    wf: &RawWorkflow,
    source: &str,
    model_override: Option<&str>,
    access: (&nika_providers::ExecutionAccessPlan, Option<&str>),
    output_json: bool,
) -> Result<ResumeSetup, u8> {
    // The answers-only form (F4): no trace, no plan — the answers below
    // ride into the gate map and wait for the ask.
    let mut setup = match resume.and_then(|req| req.trace.as_deref().map(|t| (req, t))) {
        None => ResumeSetup::fresh(),
        Some((req, trace)) => {
            load_resume_plan(req, trace, wf, source, model_override, access, output_json)?
        }
    };
    let pairs = resume.map_or(&[][..], |r| r.answers.as_slice());
    setup.answers = nika_dap::resume::parse_answers(pairs, wf)
        .map_err(|message| refuse_env(&message, output_json))?;
    // #1067 · a journaled success is a decision. `--answer` on resume
    // used to force the prompt to re-run (ADR-099 F4 "operator intent");
    // that turned a recorded NO into a shipment. Paused gates are not in
    // the plan (they never completed), so they still accept answers.
    if let Some(plan) = &setup.plan {
        nika_dap::resume::refuse_reopened_settled_gates(plan, &setup.answers)
            .map_err(|message| refuse_env(&message, output_json))?;
    }
    Ok(setup)
}

/// Read + fold the `--resume` trace into the runtime skip plan (ADR-099)
/// plus the F-P4 paused ticket (NEP-0013) plus the F-P21 version verdict
/// (NEP-0014 law 4). The TRUST judgment comes FIRST (ADR-099 trust
/// amendment · 2026-08-08): the tamper-evidence chain is verified BEFORE
/// anything is folded — the forgery class refuses (FILE, one voice with
/// `trace verify`), rides the NAMED `--resume-unverified` opt-out, or
/// proceeds under the chainless compat — both attested on the boot
/// manifest (`resume_unverified`), never a silent default. The
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
    access: (&nika_providers::ExecutionAccessPlan, Option<&str>),
    output_json: bool,
) -> Result<ResumeSetup, u8> {
    let label = trace.display().to_string();
    let refuse = |message: String| refuse_env(&format!("--resume: {message}"), output_json);
    let raw = read_trace(trace, &label, output_json)?;
    // ADR-099 trust amendment — the chain verdict BEFORE the fold (own
    // fn: the 100-line wall, and the judgment belongs to itself).
    let unverified = gate_trust(&raw, &label, req.allow_unverified, output_json)?;
    let recovered = recover_events(&raw, &label).map_err(|e| refuse(e.to_string()))?;
    if let Some(note) = &recovered.truncated_note {
        eprintln!("nika run: {note}");
    }
    // The project judgment FIRST (#1367 · the wave-7 gauntlet): a trace
    // written by another project has nothing else to judge, and no notice
    // below may describe a run that never happens.
    judge_project(&recovered.events, unverified.is_some(), output_json)?;
    // The workflow judgment NEXT (#1586): a journal written by another
    // WORKFLOW is as foreign as another project's — and every notice
    // below (version · seat · access) describes the recording, so a
    // foreign one must not get that far.
    judge_source(&recovered.events, wf, source, &label, output_json)?;
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
        nika_dap::resume::CompatVerdict::Refuse(message) => return Err(refuse(message)),
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and caller ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown compat verdict: {other:?}"),
    };
    judge_seat(&recovered.events, model_override, output_json)?;
    judge_access(&recovered.events, access, &label, output_json)?;
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
    reask_gates_when_unverified(&mut plan, wf, unverified.is_some());
    if let Some(from) = &req.from {
        nika_dap::resume::apply_from(&mut plan, wf, from).map_err(refuse)?;
    }
    Ok(ResumeSetup {
        plan: Some(plan),
        answers: BTreeMap::new(),
        paused: fold.paused,
        compat,
        unverified,
        // #1462 · the continuation link: this leg names the trace it folded.
        resumed_from: nika_dap::resume::trace_run_id(&recovered.events),
    })
}

/// Bind the folded ticket to the durable claim store (`$HOME/.nika/
/// approval-claims` · the replay guard) — and prune the claims that
/// outlived every trace on the way in (#1466): the store is touched only
/// here, so this is where it stays bounded. Pruning is fail-open and
/// speaks exactly one line when anything was removed (D2 · never silent).
fn durable_paused(
    paused: Option<PausedApproval>,
    output_json: bool,
) -> Result<Option<PausedApproval>, u8> {
    let Some(approval) = paused else {
        return Ok(None);
    };
    let home = std::env::home_dir().ok_or_else(|| {
        refuse_env(
            "--resume: HOME is unavailable; the durable approval claim store cannot be opened",
            output_json,
        )
    })?;
    let (cfg, _notes) = nika_dap::retention::RetentionConfig::from_env();
    if let Some(n) = nika_dap::retention::prune_claims(&home, &cfg, std::time::SystemTime::now()) {
        eprintln!("nika run: approval claims gc · removed {n} expired claim(s)");
    }
    approval
        .with_durable_claim_root(&home)
        .map(Some)
        .map_err(|error| {
            refuse_env(
                &format!("--resume: cannot open the durable approval claim store: {error}"),
                output_json,
            )
        })
}

fn read_trace(trace: &std::path::Path, label: &str, output_json: bool) -> Result<String, u8> {
    let refuse = |message: String| refuse_env(&message, output_json);
    // The freeze audit · a run in flight cannot be resumed: its writer
    // holds the journal's lease, and a second execution over a partial
    // journal would re-run and re-spend its in-flight tasks (ADR-129 · the
    // lease is the liveness truth, never a guess).
    if let nika_dap::liveness::Liveness::Alive { pid } = nika_dap::liveness::probe(trace) {
        return Err(refuse(format!(
            "--resume: the writer of {label} is alive (pid {pid} on this host) — a run in flight cannot be resumed: wait for its terminal, or cancel it"
        )));
    }
    let trace_parent = trace
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let trace_name = trace
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| refuse(format!("--resume: invalid trace path {label}")))?;
    let trace_dir =
        nika_fs::OwnedDir::open(trace_parent.unwrap_or_else(|| std::path::Path::new(".")))
            .map_err(|error| {
                refuse(format!(
                    "--resume: cannot open the trace directory for {label}: {error}"
                ))
            })?;
    trace_dir
        .read(trace_name)
        .map_err(|error| refuse(format!("--resume: cannot read {label}: {error}")))
}

/// The ADR-099 trust gate — the chain verdict BEFORE the fold (the same
/// walk `nika trace verify` runs), mapped to the run's exit classes and
/// the boot-manifest attestation (`Ok(None)` = verified · `Ok(Some(_))`
/// = proceeding WITHOUT a verified chain, attested · `Err` = refused).
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
        // journal): the compat proceeds, SAID on stderr AND attested —
        // the strip-the-chain forgery (delete every `chain` field) lands
        // exactly here, and it never launders silently.
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
        _ => Err(refuse_env(
            &format!(
                "--resume: {label}: unknown chain verdict class — the forensics library is \
                 newer than this CLI"
            ),
            output_json,
        )),
    }
}

/// The resume's ACCESS judgment (One Door · wave 1b · the pack's law
/// « resume cannot switch access silently »): the lanes the trace's
/// boot manifest recorded against the frozen plan THIS resume resolved.
/// Under `--resume-unverified` every recorded human decision re-asks (the
/// wave-7 gauntlet): a decision is a credential, and the chain that bound it
/// to this run is waived. Says which ones.
fn reask_gates_when_unverified(
    plan: &mut nika_runtime::resume::ResumePlan,
    wf: &RawWorkflow,
    unverified: bool,
) {
    if !unverified {
        return;
    }
    let asked = nika_dap::resume::strip_gate_records(plan, wf);
    if !asked.is_empty() {
        eprintln!(
            "nika run: --resume: unverified — {} recorded human decision(s) re-ask ({}) · a \
             decision is a credential and the chain that bound it is waived",
            asked.len(),
            asked.join(" · ")
        );
    }
}

/// The trace's project against this one (#1367): the same fingerprint the
/// composition root stamps (blake3 of the canonical sandbox root · the
/// process cwd for a local run). Another project refuses with the teaching;
/// an older trace with no fingerprint is no claim.
fn judge_project(
    events: &[nika_event::Event],
    unverified: bool,
    output_json: bool,
) -> Result<(), u8> {
    let here = std::env::current_dir()
        .ok()
        .and_then(|cwd| nika_runtime::project_root_fingerprint(&cwd));
    if unverified {
        // The recorded fingerprint is data the waived chain no longer
        // protects: say so before trusting it.
        eprintln!(
            "nika run: --resume: unverified — the trace's project binding is a recorded field the \
             waived chain no longer protects; resume only a trace you wrote"
        );
    }
    match nika_dap::resume::judge_project(events, here.as_deref()) {
        nika_dap::resume::ProjectVerdict::Refuse(message) => {
            Err(refuse_env(&format!("--resume: {message}"), output_json))
        }
        _ => Ok(()),
    }
}

/// The trace's WORKFLOW against this one (#1586): a journal written by
/// another `nika:` id is FOREIGN — refused naming both workflows and both
/// content hashes (« file CHANGED » was the wrong sentence: nothing was
/// edited, and the operator burned a live run they thought was a cache
/// hit). The same id with changed bytes keeps the notice: the current
/// file stays the source of truth (ADR-099 · an edit re-runs, it never
/// serves a stale output). The comparator is the replay session's,
/// content-aware: a CRLF/BOM re-encode is not a change.
fn judge_source(
    events: &[nika_event::Event],
    wf: &RawWorkflow,
    source: &str,
    label: &str,
    output_json: bool,
) -> Result<(), u8> {
    let id = wf.workflow.as_ref().map(|w| w.value.as_str());
    match nika_dap::resume::judge_source(events, id, source) {
        nika_dap::resume::SourceVerdict::Foreign(message) => {
            Err(refuse_env(&format!("--resume: {message}"), output_json))
        }
        nika_dap::resume::SourceVerdict::Changed => {
            eprintln!(
                "nika run: --resume: the workflow file CHANGED since {label} recorded it — the \
                 current bytes are what runs (an edited `model:` moves the seat, and edited tasks \
                 re-run instead of serving the recorded output)"
            );
            Ok(())
        }
        _ => Ok(()),
    }
}

/// An explicit `--access` names the change (noticed on stderr); silence
/// over a moved lane refuses, naming both paths and the two flags.
fn judge_access(
    events: &[nika_event::Event],
    (plan, pin): (&nika_providers::ExecutionAccessPlan, Option<&str>),
    label: &str,
    output_json: bool,
) -> Result<(), u8> {
    let live: std::collections::BTreeMap<String, nika_dap::resume::LaneCarry> = plan
        .admitted()
        .map(|(model, lane)| {
            let flag =
                nika_dap::resume::pin_flag(&lane.plan.access, Some(lane.plan.chosen.as_str()));
            (model.to_owned(), (lane.plan.access.clone(), flag))
        })
        .collect();
    let recorded = nika_dap::resume::trace_access_lanes(events);
    match nika_dap::resume::judge_access(recorded.as_ref(), &live, pin) {
        nika_dap::resume::AccessVerdict::Proceed { changed } => {
            for (model, was, now) in changed {
                eprintln!(
                    "nika run: --resume: access change declared — {label} ran `{model}` on \
                     `{was}`, this resume runs it on `{now}`"
                );
            }
            Ok(())
        }
        nika_dap::resume::AccessVerdict::Refuse(message) => Err(refuse_env(
            &format!("--resume: {} · {message}", nika_error::codes::NIKA_1807),
            output_json,
        )),
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and caller ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown access verdict: {other:?}"),
    }
}

/// The judgment about WHICH SEAT the resumed legs will run on — the
/// flag (issue 772): a run recorded under `--model` must never SILENTLY
/// resume on the envelope model — the mock-previewed run that comes
/// back on a priced seat. Explicit argv wins; silence REFUSES, naming
/// the recorded seat and the exact flag. (The file half — the envelope
/// `model:` edited between the pause and the resume — is
/// [`judge_source`]'s CHANGED notice.)
fn judge_seat(
    events: &[nika_event::Event],
    model_override: Option<&str>,
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
            Ok(())
        }
        nika_dap::resume::ModelVerdict::Refuse(message) => {
            Err(refuse_env(&format!("--resume: {message}"), output_json))
        }
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and caller ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown model verdict: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_event::{Event, EventKind};
    use nika_types::id::EventId;
    use nika_types::resource::{KeyValue, Value as FieldValue};
    use nika_types::timestamp::Timestamp;

    /// A journal's boot manifest naming `workflow` and the hash of `yaml`.
    fn journal_of(workflow: &str, yaml: &str) -> Vec<Event> {
        let started = Event::new(
            EventId::new(uuid::Uuid::nil()),
            Timestamp::from_unix_ms(0),
            EventKind::WorkflowStarted,
        )
        .with_field(KeyValue::new(
            "workflow",
            FieldValue::String(workflow.to_owned()),
        ))
        .with_field(KeyValue::new(
            "workflow_sha256",
            FieldValue::String(nika_event::source_id::sha256_hex(yaml.as_bytes())),
        ));
        vec![started]
    }

    fn parsed(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("a valid fixture")
    }

    /// #1586 · the CLI face of the workflow judgment: a foreign journal
    /// is the ENV class (exit 3 · the cross-project posture), an edited
    /// file of the same id is a notice (Ok), an untouched one is silent.
    #[test]
    fn a_foreign_journal_refuses_env_and_an_edited_file_only_notices() {
        const HELLO: &str =
            "nika: hello\nmodel: mock/echo\ntasks:\n  greet:\n    infer:\n      prompt: hi\n";
        const BRIEF: &str = "nika: compose-brief\nmodel: mock/echo\ntasks:\n  draft:\n    infer:\n      prompt: hi\n";
        let hello = parsed(HELLO);
        assert_eq!(
            judge_source(
                &journal_of("compose-brief", BRIEF),
                &hello,
                HELLO,
                "t",
                true
            ),
            Err(exit::ENV),
            "another `nika:` id refuses like another project"
        );
        assert_eq!(
            judge_source(&journal_of("hello", BRIEF), &hello, HELLO, "t", true),
            Ok(()),
            "the same id with other bytes is the CHANGED notice, never a refusal"
        );
        assert_eq!(
            judge_source(&journal_of("hello", HELLO), &hello, HELLO, "t", true),
            Ok(())
        );
    }
}
