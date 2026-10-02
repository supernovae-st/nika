// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The scoped Session account and proposal boundary over compiler-result and host doubles.
//! File observations are real, confined to test-owned fixtures. No Runtime, provider or socket
//! is used here. Native generation/repair is qualified separately at the compiler entry.

mod revision_tests;

use super::*;
use crate::authoring::{AuthoringError, Reading};
use nika_onboard::compile::copy::Usage;
use nika_onboard::compile::rehearse::{FailureRecord, RecordedCause, Refusal as RoomRefusal};
use nika_onboard::compile::{CompileOutcome, CompileStatus};

#[derive(Clone, Copy)]
enum Mode {
    Passed,
    Failed,
    Stopped,
    NotRun,
    WrongDigest,
    OverBudget,
}

struct Double {
    root: PathBuf,
    mode: Mode,
    calls: Arc<AtomicUsize>,
}

impl Rehearse for Double {
    fn bound(&self) -> Duration {
        ObservedRoom::BOUND
    }
    fn rehearse<'a>(&'a self, source: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(source, inputs, &[])
    }
    fn rehearse_reading<'a>(
        &'a self,
        source: &'a str,
        inputs: &'a [String],
        targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if matches!(self.mode, Mode::NotRun) {
                return RehearsalReport::new(
                    Rehearsal::NotRun {
                        reason: "the synthetic effect is refused".into(),
                    },
                    Attempt::NeverAttempted,
                    EffectCounts::none(),
                    sha256_hex(source.as_bytes()),
                )
                .with_observation(Observation::refused(RoomRefusal::Effect));
            }
            let mut report = answer(&self.root, false, source, inputs, targets);
            match self.mode {
                Mode::Failed => {
                    report.outcome = Rehearsal::Failed {
                        task: "copy".into(),
                        code: "synthetic-failure".into(),
                        message: "failed".into(),
                    };
                    report.observation.failure = Some(FailureRecord::new(
                        "copy",
                        "synthetic-failure",
                        RecordedCause::VerbError,
                    ));
                }
                Mode::Stopped => {
                    report.outcome = Rehearsal::NotRun {
                        reason: "synthetic stop".into(),
                    };
                    report.attempt = Attempt::Stopped { elapsed_ms: 10_000 };
                }
                Mode::WrongDigest => report.candidate_sha256 = "another candidate".into(),
                Mode::OverBudget => report.attempt = Attempt::Completed { elapsed_ms: 60_001 },
                _ => {}
            }
            report
        })
    }
}

fn session(root: &Path, mode: Mode) -> (SessionRuntime, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let (session, _, _) = open_with(root, move |world| {
        Box::new(Double {
            root: world.to_path_buf(),
            mode,
            calls: observed.clone(),
        })
    });
    (session, calls)
}

fn prepared(s: &SessionRuntime) -> CompileOutcome {
    let round = AuthoringRound::new(INTENT);
    let out = compile_in(
        &DETERMINISTIC,
        &s.project_context(),
        &round.request(),
        INTENT,
    )
    .expect("HARNESS_INVALID: deterministic compiler fixture");
    assert_eq!(
        out.status,
        CompileStatus::Ready,
        "HARNESS_INVALID: {out:#?}"
    );
    assert!(
        out.check_preview
            .as_ref()
            .is_some_and(|check| check.report.is_clean())
    );
    out
}

// A revision-shaped result skips the independent Copy selector, so these tests isolate the
// native proof binding. It is a compiler-result double, not a claim that an author revised it.
fn revision(out: &CompileOutcome) -> AuthoringRound {
    let mut round = AuthoringRound::new(INTENT);
    round.edit = Some((
        out.candidate.clone().expect("candidate"),
        "Keep the copy unchanged".into(),
        Some(INTENT.into()),
    ));
    round
}

fn observed(
    s: &mut SessionRuntime,
    round: &AuthoringRound,
    out: CompileOutcome,
    error: bool,
) -> Result<CompileOutcome, AuthoringError> {
    s.rehearse_dispatch(&round.effective_intent(), |_, host| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let _ = runtime.block_on(host.rehearse_reading(
            out.candidate.as_deref().expect("candidate"),
            &["./in/source.txt".into()],
            &["./out/copied.txt".into()],
        ));
        if error {
            Err(AuthoringError::Runtime(
                "synthetic compiler error after host return".into(),
            ))
        } else {
            Ok(out)
        }
    })
}

#[test]
fn native_spending_survives_a_compiler_error() {
    let root = project();
    let (mut s, calls) = session(root.path(), Mode::Passed);
    let out = prepared(&s);
    let round = revision(&out);
    assert!(observed(&mut s, &round, out, true).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(s.rehearsals.turn.attempts, 1);
    assert!(s.rehearsals.native.is_none());
    let out = prepared(&s);
    observed(&mut s, &round, out, false).expect("second dispatch");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        s.rehearsals.turn.attempts, 2,
        "no reset on the next dispatch"
    );
}

#[test]
fn native_and_copy_share_the_same_exhausted_turn() {
    let root = project();
    let (mut s, calls) = session(root.path(), Mode::Passed);
    s.rehearsals.turn = Usage::new(5, 5, 0, 0, 0);
    let out = prepared(&s);
    let round = AuthoringRound::new(INTENT);
    observed(&mut s, &round, out, false).expect("last admitted attempt");
    assert_eq!(s.rehearsals.turn.attempts, 6);
    assert!(s.rehearse_copy(&round).is_err());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "Copy received the carried allowance"
    );
    let out = prepared(&s);
    assert!(
        observed(&mut s, &round, out, false).is_err(),
        "budget NotRun cannot rescue Ready"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(s.rehearsals.turn.attempts, 6);
}

#[test]
fn a_failed_stopped_or_invalid_report_cannot_back_ready() {
    for mode in [
        Mode::Failed,
        Mode::Stopped,
        Mode::WrongDigest,
        Mode::OverBudget,
    ] {
        let root = project();
        let (mut s, calls) = session(root.path(), mode);
        let out = prepared(&s);
        let round = revision(&out);
        assert!(observed(&mut s, &round, out, false).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(s.rehearsals.turn.attempts, 1);
        assert!(s.rehearsals.native.is_none());
    }
}

#[test]
fn native_preview_checks_destinations_again_before_save() {
    for drift in [false, true] {
        let root = project();
        let (mut s, _) = session(root.path(), Mode::Passed);
        let out = prepared(&s);
        let round = revision(&out);
        let out = observed(&mut s, &round, out, false).expect("bound report");
        let (id, preview) = proposal(s.settle(round, Reading::Ready(out)));
        assert!(preview.contains("Rehearsed on a copy"), "{preview}");
        assert!(preview.contains("observed result of this candidate"));
        assert!(s.rehearsed_pending(&id));
        if drift {
            write(root.path(), "out/copied.txt", "appeared");
            let refused = refused(s.consent("yes"));
            assert_eq!(refused.class, RefusalClass::StaleRevision);
            assert!(!root.path().join(LANDED).exists());
        } else {
            facts(s.consent("yes"));
            assert!(!root.path().join("out/copied.txt").exists(), "SaveOnly");
        }
        assert_eq!(
            std::fs::read_to_string(root.path().join(SOURCE))
                .expect("synthetic fixture must be available"),
            USER
        );
    }
}

#[test]
fn a_true_not_run_is_explicit_source_only_without_output_or_proof() {
    let root = project();
    let (mut s, calls) = session(root.path(), Mode::NotRun);
    let out = prepared(&s);
    let round = revision(&out);
    let out = observed(&mut s, &round, out, false).expect("safe source-only outcome");
    let (id, preview) = proposal(s.settle(round, Reading::Ready(out)));
    assert!(preview.contains("Rehearsal not run"));
    assert!(preview.contains("synthetic effect is refused"));
    assert!(preview.contains("source-only preview"));
    assert!(!preview.contains("read back"));
    assert!(!s.rehearsed_pending(&id));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(s.rehearsals.turn.attempts, 0);
}

#[test]
fn copy_replacement_discards_native_proof_without_erasing_its_cost() {
    let root = project();
    let (mut s, calls) = session(root.path(), Mode::Passed);
    let mut out = prepared(&s);
    // A harmless authored comment makes byte identity distinct; this is a binding test, not
    // evidence of behavioural diversity between these two sources.
    out.candidate = out
        .candidate
        .map(|source| format!("# authored copy\n{source}"));
    let authored = out
        .candidate
        .clone()
        .expect("synthetic fixture must be available");
    let round = AuthoringRound::new(INTENT);
    let out = observed(&mut s, &round, out, false).expect("native observation");
    let (_, preview) = proposal(s.settle(round, Reading::Ready(out)));
    assert_ne!(pending_bytes(&s), authored);
    assert!(preview.contains("held on every world"));
    assert!(!preview.contains("observed result of this candidate"));
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert_eq!(s.rehearsals.turn.attempts, 4, "native plus Copy, once each");
    assert_eq!(
        s.last_outcome
            .as_ref()
            .expect("synthetic fixture must be available")
            .candidate
            .as_deref(),
        Some(pending_bytes(&s).as_str())
    );
}

#[test]
fn a_live_preview_cannot_follow_changed_returned_bytes() {
    let root = project();
    let (mut s, calls) = session(root.path(), Mode::Passed);
    let mut out = prepared(&s);
    let request = AuthoringRound::new(INTENT);
    let result = s.rehearse_dispatch(&request.effective_intent(), |_, host| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("synthetic fixture must be available");
        let _ = runtime.block_on(
            host.rehearse_reading(
                out.candidate
                    .as_deref()
                    .expect("synthetic fixture must be available"),
                &["./in/source.txt".into()],
                &["./out/copied.txt".into()],
            ),
        );
        out.candidate = out
            .candidate
            .map(|source| format!("# changed after rehearsal\n{source}"));
        Ok(out)
    });
    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(s.rehearsals.turn.attempts, 1);
    assert!(s.rehearsals.native.is_none());
}

#[test]
fn a_live_preview_cannot_follow_another_request() {
    let root = project();
    let (mut s, _) = session(root.path(), Mode::Passed);
    let out = prepared(&s);
    let mut round = revision(&out);
    let out = observed(&mut s, &round, out, false).expect("native observation");
    round
        .edit
        .as_mut()
        .expect("synthetic fixture must be available")
        .1 = "Write another destination instead".into();
    assert!(matches!(
        s.settle(round, Reading::Ready(out)),
        TurnOutcome::Refusal(_)
    ));
    assert!(!root.path().join(LANDED).exists());
}

#[test]
fn a_source_only_dispatch_discards_an_earlier_live_preview_but_keeps_spending() {
    let root = project();
    let (mut s, calls) = session(root.path(), Mode::Passed);
    let out = prepared(&s);
    let round = AuthoringRound::new(INTENT);
    observed(&mut s, &round, out, false).expect("native observation");
    assert!(s.rehearsals.native.is_some());
    s.compile_round(&round, &DETERMINISTIC)
        .expect("source-only dispatch");
    assert!(s.rehearsals.native.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(s.rehearsals.turn.attempts, 1);
}
