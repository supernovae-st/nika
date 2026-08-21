// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika arm fire <label>` — LE TIREUR · the one firer (D2).
//!
//! ONE function applies the on-time window, the miss policy, the
//! overlap lock, the per-tick ceiling and the record — the OS units
//! (W3) and `serve` (W5) both end here, so the law lives exactly once.
//! The beat to fire is the workflow file's radical (D4 —
//! `workflows/doctor.nika.yaml` → `doctor`; a collision takes `-2`,
//! `-3` in file order), the firing state is the sidecar of
//! [`ArmState`] (D3), and the clock is injected at the
//! verb's edge (D5 — `--now`, hidden): the pure decision never reads
//! one, never sleeps, and a replay is deterministic.
//!
//! The order law (W5-bis): the per-beat lock is taken BEFORE the
//! decision and held until AFTER the receipt append. Under the lock
//! the firer reads the state, decides, appends `claimed` (+ fsync),
//! runs, appends the receipt (+ fsync · last.json · watermark), and
//! only then releases. After ANY wait (`chevauchement: file`) the
//! state is re-read and re-decided — the pre-wait slot is stale by
//! law. Ledger appends made WITHOUT the beat lock (the overlap-skip
//! path) are serialized by the inner ledger lock. What this buys is
//! AT-LEAST-ONCE: a crash between the claim and its receipt leaves a
//! visible orphan (`ArmState::unsettled`), never a silent double-fire;
//! exactly-once is never claimed.
//!
//! The stdout contract (D8): a decision prints EXACTLY ONE line —
//! `fired …` · `skipped …` · `paused …` · `failed …`. The in-process
//! run's own fold is turned onto stderr for its duration, so the
//! machine surface stays byte-pure (the launchd/serve log captures
//! both). Two honest exceptions: a registry REFUSAL (exit 2) teaches
//! multi-line, the same voice `nika arm` uses today; and a run that
//! dies ENVIRONMENT (rc 3) rides the bin's env-to-stderr law.
//!
//! Law 4 (N2) binds the pause: every fire is a FRESH run — a paused
//! run is PARKED with its trace (`paused … · trace …`), never resumed,
//! never answered by the firer.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::state::{ArmState, Claim, FireKind, HistoryEntry, LockOutcome, Receipt};
#[cfg(test)]
use jiff::Timestamp;
use jiff::{SignedDuration, Zoned};
use nika_cadence::firing::{self, ArmGeneration, FencingToken, FiringEvent, FiringState, SlotId};
use nika_cadence::registry::{AfterSkip, ArmRegistry, Beat, Cadence, Locus, MissPolicy, Overlap};
use nika_fs::OwnedDir;

mod exit {
    pub(super) const OK: u8 = 0;
    pub(super) const FILE: u8 = 2;
    pub(super) const ENV: u8 = 3;
    pub(super) const PAUSED: u8 = 4;
}

#[cfg(test)]
mod firer_tests;

/// The poll quantum while a queued tick (`chevauchement: file`) waits
/// for the running one to release the lock.
const POLL_MS: i64 = 1_000;

/// The claim deadline's fallback when the cadence names no next slot
/// after now: the v0 crash-detector (a claim past its deadline without
/// a receipt is an orphan — the sweep that re-arms it is W8's). 24h
/// bounds even the weirdest lawful cadence's gap.
const CLAIM_DEADLINE_FALLBACK: SignedDuration = SignedDuration::from_hours(24);

/// The firer's context (D2) — everything the decision needs, injected.
/// `serve` (②) builds the same one; the verb below only parses args.
#[non_exhaustive]
pub struct FireCtx {
    /// The project root (the directory holding `nika.yaml`).
    project_root: PathBuf,
    /// The parsed + validated registry.
    registry: ArmRegistry,
    /// The beat's position in the registry (label resolution is the
    /// caller's — [`labels`]).
    index: usize,
    /// The beat's label (the workflow file radical, D4).
    label: String,
    /// The decision instant — injected (D5); the pure code never reads
    /// a clock.
    now: Zoned,
    /// The firing sidecar (D3).
    state: ArmState,
    /// The firer's own pid, written into the locks it takes (the edges
    /// pass `std::process::id()`; a test can lend a child's).
    pid: u32,
    /// The sleep seam — the queue's wait (`chevauchement: file`) rides
    /// it: the OS firer sleeps, `serve` races the signals, the harness
    /// advances its scripted clock. D5's twin: the firer never sleeps
    /// uninjected.
    wait: WaitSeam,
    /// The run seam — interfaces adapt their execution service here; tests and
    /// simulations can inject a deterministic substitute.
    run: RunSeam,
}

/// A firing context could not bind its registry position to one beat.
#[derive(Debug)]
pub struct FireCtxError {
    registry: ArmRegistry,
    index: usize,
    detail: String,
}

impl FireCtxError {
    /// Recover registry ownership for a resident caller.
    #[must_use]
    pub fn into_registry(self) -> ArmRegistry {
        self.registry
    }
}

impl std::fmt::Display for FireCtxError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.detail.is_empty() {
            write!(
                formatter,
                "arm fire: registry has no beat at index {}",
                self.index
            )
        } else {
            write!(formatter, "arm fire: {}", self.detail)
        }
    }
}

impl std::error::Error for FireCtxError {}

impl FireCtx {
    /// Build one firing transaction. The default wait uses the OS sleeper;
    /// resident interfaces can replace it with [`Self::with_wait`].
    ///
    /// # Errors
    /// The registry has no beat at `index`, the project custody cannot be
    /// opened, or the supplied registry differs from the held `nika.yaml`.
    #[must_use = "an invalid registry index must be handled"]
    pub fn new(
        project_root: PathBuf,
        registry: ArmRegistry,
        index: usize,
        now: Zoned,
        pid: u32,
        run: RunSeam,
    ) -> Result<Self, FireCtxError> {
        let Some(label) = labels(&registry).get(index).cloned() else {
            return Err(FireCtxError {
                registry,
                index,
                detail: String::new(),
            });
        };
        let state = match ArmState::open(&project_root) {
            Ok(state) => state,
            Err(error) => {
                return Err(FireCtxError {
                    registry,
                    index,
                    detail: format!("project custody refused: {error}"),
                });
            }
        };
        if let Err(detail) = verify_registry_custody(&state, &registry) {
            return Err(FireCtxError {
                registry,
                index,
                detail,
            });
        }
        Ok(Self {
            project_root,
            registry,
            index,
            label,
            now,
            state,
            pid,
            wait: Box::new(os_wait),
            run,
        })
    }

    /// Replace the default OS wait with a signal-aware resident wait.
    #[must_use]
    pub fn with_wait(mut self, wait: WaitSeam) -> Self {
        self.wait = wait;
        self
    }

    /// Return the registry after a firing transaction has borrowed the
    /// context. Resident loops use it as their next immutable snapshot.
    #[must_use]
    pub fn into_registry(self) -> ArmRegistry {
        self.registry
    }
}

fn verify_registry_custody(state: &ArmState, expected: &ArmRegistry) -> Result<(), String> {
    if let Some(error) = nika_cadence::validate(expected).next() {
        return Err(format!("supplied registry is not lawful: {error}"));
    }
    let (_, mut source) = state
        .open_project_file(Path::new("nika.yaml"))
        .map_err(|error| format!("cannot open held nika.yaml: {error}"))?;
    let mut text = String::new();
    source
        .read_to_string(&mut text)
        .map_err(|error| format!("cannot read held nika.yaml: {error}"))?;
    let actual = nika_cadence::parse_registry(&text)
        .map_err(|error| format!("held nika.yaml is not an ARM registry: {error}"))?;
    if let Some(error) = nika_cadence::validate(&actual).next() {
        return Err(format!(
            "held nika.yaml is not a lawful ARM registry: {error}"
        ));
    }
    if registries_match(expected, &actual) {
        Ok(())
    } else {
        Err("supplied registry does not belong to the held project".to_owned())
    }
}

fn registries_match(left: &ArmRegistry, right: &ArmRegistry) -> bool {
    left.nika == right.nika
        && same_float(left.ceiling, right.ceiling)
        && left.beat_count() == right.beat_count()
        && left
            .beats()
            .zip(right.beats())
            .all(|(left, right)| beats_match(left, right))
}

fn beats_match(left: &Beat, right: &Beat) -> bool {
    left.workflow == right.workflow
        && left.cadence == right.cadence
        && left.ou == right.ou
        && same_float(left.plafond, right.plafond)
        && left.manque == right.manque
        && left.chevauchement == right.chevauchement
        && left.apres_saut == right.apres_saut
        && left.actif == right.actif
        && left.raison == right.raison
        && left.jusqu_au == right.jusqu_au
        && left.tolerance == right.tolerance
        && left.decalage == right.decalage
        && left.par == right.par
}

fn same_float(left: Option<f64>, right: Option<f64>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.to_bits() == right.to_bits(),
        (None, None) => true,
        _ => false,
    }
}

/// What a wait returned: the span elapsed whole, or a signal broke it
/// (the firer then skips `serve-stop` — never runs, never blocks).
pub enum Wait {
    /// The full span elapsed.
    Elapsed,
    /// A stop signal broke the wait (serve's ctrl-c / SIGTERM).
    Interrupted,
}

/// The wait seam's shape: a span in, the outcome out.
pub type WaitSeam = Box<dyn Fn(SignedDuration) -> Wait>;

#[non_exhaustive]
pub struct RunShot {
    project: OwnedDir,
    root: PathBuf,
    workflow: String,
    source: String,
    generation: ArmGeneration,
    ceiling: f64,
}

impl RunShot {
    /// The held project directory capability used for relative execution.
    #[must_use]
    pub fn project(&self) -> &OwnedDir {
        &self.project
    }

    #[must_use]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    #[must_use]
    pub fn workflow(&self) -> &str {
        &self.workflow
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn generation(&self) -> &ArmGeneration {
        &self.generation
    }

    #[must_use]
    pub fn ceiling(&self) -> f64 {
        self.ceiling
    }
}

#[non_exhaustive]
pub struct RunUpshot {
    code: u8,
    trace: Option<String>,
}

impl RunUpshot {
    #[must_use]
    pub fn new(code: u8, trace: Option<String>) -> Self {
        Self {
            code,
            trace: trace.map(|value| value.replace('\r', "\\r").replace('\n', "\\n")),
        }
    }
}

pub type RunSeam = Rc<dyn Fn(&RunShot) -> RunUpshot>;

/// What a fire leaves: the ONE stdout line (D8) + the process exit.
pub struct FireVerdict {
    /// The single line — `fired …` · `skipped …` · `paused …` ·
    /// `failed …`.
    line: String,
    /// `0` fired/skipped · `2` a refusal · the run's rc (1·2·3·4) when
    /// the run went.
    code: u8,
}

impl FireVerdict {
    /// Consume the verdict into its one output line and process exit code.
    #[must_use]
    pub fn into_parts(self) -> (String, u8) {
        (self.line, self.code)
    }
}

/// The pure decision — the file's own truth, the v0 refusals, the
/// clock's verdict — GIVEN the beat, the injected instant and the last
/// decided slot. Locking and running are the impure halves
/// ([`fire_beat`]).
enum Decision {
    /// A policy said no. `slot` Some ⇒ the decision consumes the slot
    /// (last.json moves); `journal` false ⇒ nothing changed, nothing is
    /// written (a duplicate tick is not a decision).
    Skip {
        /// The full one-line report (D8).
        line: String,
        /// The machine token (`missed:2` · `overlap` · `cloud` · …).
        reason: String,
        /// The slot the decision covers.
        slot: Option<Zoned>,
        /// Journal the decision (history · maybe last.json).
        journal: bool,
    },
    /// v0 refuses with teaching, naming the version it arrives with.
    Refuse {
        /// The teaching line (exit 2).
        line: String,
    },
    /// Fire the slot — `slots` Some(n) is `rattraper-une-fois`: ONE run
    /// answers for the whole silence.
    Fire {
        /// The slot to fire.
        slot: Zoned,
        /// The silence's count for `rattraper-une-fois`.
        slots: Option<u32>,
    },
}

/// The beat labels, in file order (D4): the workflow file's radical —
/// `workflows/doctor.nika.yaml` → `doctor` — a collision taking `-2`,
/// `-3`. The identity lives at L0 since W3 (`nika_cadence::emit::labels`
/// — the OS units name themselves from the same source); this shim keeps
/// the verb's call sites, and its pin below guards the delegation.
#[must_use]
pub fn labels(registry: &ArmRegistry) -> Vec<String> {
    nika_cadence::emit::labels(registry)
}

/// The decision instant: the hidden `--now` (RFC 3339) or the wall
/// clock — the ONE clock read of the verb (D5). A bare RFC 3339 instant
/// lands on UTC; jiff's zoned form (`…+02:00[Europe/Paris]`) keeps its
/// zone. Comparisons ride the instant either way.
/// The decision, pure — every branch pinned by the unit tests below.
/// Order matters: the file's own truth first (inactive · cloud ·
/// expired), then the v0 refusals (they teach even when the beat would
/// be due), then the clock's verdict.
fn decide(
    registry: &ArmRegistry,
    index: usize,
    label: &str,
    now: &Zoned,
    last: Option<&Zoned>,
) -> Decision {
    let Some(beat) = registry.beats().nth(index) else {
        return Decision::Refuse {
            line: "arm fire · engine fault: the label resolved past the registry".to_owned(),
        };
    };
    if !beat.is_active() {
        let why = beat.raison.as_deref().unwrap_or("sans raison");
        return Decision::Skip {
            line: format!("skipped {label} · inactive — {why}"),
            reason: "inactive".to_owned(),
            slot: None,
            journal: true,
        };
    }
    if beat.locus() == Locus::Cloud {
        return Decision::Skip {
            line: format!(
                "skipped {label} · cloud — le cloud exécute, le calendrier demeure au registre"
            ),
            reason: "cloud".to_owned(),
            slot: None,
            journal: true,
        };
    }
    if let Some(expired) = expiry_passed(beat, now) {
        return Decision::Skip {
            line: format!("skipped {label} · expired · jusqu_au {expired}"),
            reason: "expired".to_owned(),
            slot: None,
            journal: true,
        };
    }
    if let Some(line) = v0_refusal(beat) {
        return Decision::Refuse { line };
    }
    let cadence = match Cadence::parse(&beat.cadence) {
        Ok(cadence) => cadence,
        // validate ran first — a cadence that fails here is an ENGINE
        // fault, said as such (the two-readers law).
        Err(e) => {
            return Decision::Refuse {
                line: format!("arm fire · engine fault: a validated cadence refuses — {e}"),
            };
        }
    };
    if matches!(cadence, Cadence::Webhook) {
        return Decision::Skip {
            line: format!(
                "skipped {label} · webhook — le beat tire à l'événement, jamais à l'horloge"
            ),
            reason: "webhook".to_owned(),
            slot: None,
            journal: true,
        };
    }
    decide_by_clock(registry, index, label, now, last, &cadence, beat)
}

/// The clock half of the decision: the planner's silence over
/// `(last, now]`, the on-time window, the miss policy.
fn decide_by_clock(
    registry: &ArmRegistry,
    index: usize,
    label: &str,
    now: &Zoned,
    last: Option<&Zoned>,
    cadence: &Cadence,
    beat: &Beat,
) -> Decision {
    let last_owned = last.cloned();
    // A named binding: the planner borrows the callback for the call.
    let last_of = move |i: usize| {
        if i == index { last_owned.clone() } else { None }
    };
    let dues = match nika_cadence::due(registry, now, &last_of) {
        Ok(dues) => dues,
        // validate ran first — same engine-fault voice as the cadence.
        Err(e) => {
            return Decision::Refuse {
                line: format!("arm fire · engine fault: a validated registry refuses — {e}"),
            };
        }
    };
    match dues.into_iter().find(|d| d.index == index) {
        Some(due) => match (due.kind, beat.manque) {
            (nika_cadence::DueKind::OnTime, _) => Decision::Fire {
                slot: due.slot.at,
                slots: None,
            },
            (nika_cadence::DueKind::Missed { slots }, Some(MissPolicy::Sauter)) => Decision::Skip {
                line: format!(
                    "skipped {label} · missed:{slots} · slot {}",
                    due.slot.at.timestamp()
                ),
                reason: format!("missed:{slots}"),
                slot: Some(due.slot.at),
                journal: true,
            },
            (nika_cadence::DueKind::Missed { slots }, Some(MissPolicy::RattraperUneFois)) => {
                Decision::Fire {
                    slot: due.slot.at,
                    slots: Some(slots),
                }
            }
            // `rattraper` is a v0 refusal upstream and a missing
            // `manqué:` never passes validate — an arrival here is an
            // engine fault, said as such, never an approximation.
            _ => Decision::Refuse {
                line: "arm fire · engine fault: manqué: policy escaped validation".to_owned(),
            },
        },
        // Not due: either this slot was already DECIDED (a duplicate
        // tick changes nothing — and journals nothing), or there is no
        // state and the planner invents no backlog (N2).
        None => match (last, cadence.prev_before(now)) {
            (Some(fired), Some(prev)) if prev.at == *fired => Decision::Skip {
                line: format!("skipped {label} · already · slot {}", prev.at.timestamp()),
                reason: "already".to_owned(),
                slot: None,
                journal: false,
            },
            _ => Decision::Skip {
                line: format!(
                    "skipped {label} · not-due — hors fenêtre, et N2 n'invente pas d'arriéré"
                ),
                reason: "not-due".to_owned(),
                slot: None,
                journal: false,
            },
        },
    }
}

/// The v0 refusals (D6) — each names the version it arrives with. A
/// policy the firer cannot honor must REFUSE, never approximate.
fn v0_refusal(beat: &Beat) -> Option<String> {
    let w = beat.workflow.as_str();
    if beat.chevauchement == Some(Overlap::Remplacer) {
        return Some(format!(
            "arm fire {w} · chevauchement: remplacer — arrive avec serve v0.2 · aujourd'hui: sauter (le défaut) ou file"
        ));
    }
    if beat.apres_saut == Some(AfterSkip::ACompletion) {
        return Some(format!(
            "arm fire {w} · après_saut: à-complétion — arrive avec serve v0.2 · aujourd'hui: prochain-créneau (le défaut)"
        ));
    }
    if beat.manque == Some(MissPolicy::Rattraper) {
        return Some(format!(
            "arm fire {w} · manqué: rattraper — arrive avec serve v0.2 · aujourd'hui: rattraper-une-fois ou sauter"
        ));
    }
    if beat.decalage.is_some() {
        return Some(format!(
            "arm fire {w} · décalage: — arrive avec serve v0.2 · aujourd'hui le créneau tire à l'instant dit"
        ));
    }
    None
}

/// `jusqu_au` strictly before the decision instant's own date ⇒ the
/// suspension is over. v0 judges on the instant's civil date (a bare
/// `--now` rides UTC) — the zone-exact expiry lands with serve.
fn expiry_passed(beat: &Beat, now: &Zoned) -> Option<String> {
    let raw = beat.jusqu_au.as_deref()?;
    let date = raw.parse::<jiff::civil::Date>().ok()?;
    (date < now.date()).then(|| raw.to_owned())
}

/// The one firer (D2), the order law made code (W5-bis): the beat's
/// lock is taken BEFORE the decision and held until AFTER the receipt
/// append; after any wait the state is re-read and re-decided. Always
/// the ONE line (D8).
#[must_use]
pub fn fire_beat(ctx: &FireCtx) -> FireVerdict {
    match ctx.state.acquire_beat_lock(&ctx.label, ctx.pid, &ctx.now) {
        Err(e) => FireVerdict {
            line: format!("failed {} · the lock refused: {e}", ctx.label),
            code: exit::ENV,
        },
        Ok(attempt) => match attempt.outcome {
            LockOutcome::HeldAlive { pid } => overlap_held(ctx, pid),
            LockOutcome::Acquired => {
                let Some(lease) = attempt.lease else {
                    return FireVerdict {
                        line: format!("failed {} · the lock returned no lease", ctx.label),
                        code: exit::ENV,
                    };
                };
                decide_locked(ctx, lease)
            }
        },
    }
}

/// The acquired path: the lock is OURS — the decision happens under it
/// (the law), and the release rides the guard on EVERY exit, engine
/// fault and record refusal included.
fn decide_locked(ctx: &FireCtx, lease: super::state::LockLease) -> FireVerdict {
    let lock = HeldBeatLock { lease };
    let last = match ctx.state.last_fired(&ctx.label) {
        Ok(last) => last,
        Err(error) => return record_refused(ctx, &error),
    };
    match decide(
        &ctx.registry,
        ctx.index,
        &ctx.label,
        &ctx.now,
        last.as_ref(),
    ) {
        Decision::Refuse { line } => FireVerdict {
            line,
            code: exit::FILE,
        },
        Decision::Skip {
            line,
            reason,
            slot,
            journal,
        } => act_on_skip(ctx, line, reason, slot.as_ref(), journal),
        Decision::Fire { slot, slots } => claim_run_receipt(ctx, &lock.lease, &slot, slots),
    }
}

/// The lock is another LIVE firer's — law ⑥ governs. The decision
/// below is an unlocked PEEK, advisory only: it says whether this tick
/// had a slot to fire and which one a skip covers. A run can only ever
/// come from a decision made UNDER the lock (the re-decision law) —
/// the peek alone never runs.
fn overlap_held(ctx: &FireCtx, holder: u32) -> FireVerdict {
    let last = match ctx.state.last_fired(&ctx.label) {
        Ok(last) => last,
        Err(error) => return record_refused(ctx, &error),
    };
    match decide(
        &ctx.registry,
        ctx.index,
        &ctx.label,
        &ctx.now,
        last.as_ref(),
    ) {
        // The tick had nothing to fire — the held lock is not ours to
        // judge: the decision's own line, journaled as it would be
        // under the lock (the inner ledger lock alone serializes the
        // append).
        Decision::Refuse { line } => FireVerdict {
            line,
            code: exit::FILE,
        },
        Decision::Skip {
            line,
            reason,
            slot,
            journal,
        } => act_on_skip(ctx, line, reason, slot.as_ref(), journal),
        Decision::Fire { slot, .. } => match beat_of(ctx).map(Beat::overlap) {
            Some(Overlap::Sauter) => act_on_skip(
                ctx,
                format!(
                    "skipped {} · overlap · pid {holder} tient le créneau · slot {}",
                    ctx.label,
                    slot.timestamp()
                ),
                "overlap".to_owned(),
                Some(&slot),
                true,
            ),
            Some(Overlap::File) => wait_turn(ctx, &slot, holder),
            // `remplacer` is a v0 refusal upstream (the peek refuses
            // before the clock) — an arrival here is an engine fault,
            // said as such, never an approximation.
            _ => FireVerdict {
                line: format!(
                    "failed {} · engine fault: remplacer reached the lock",
                    ctx.label
                ),
                code: exit::FILE,
            },
        },
    }
}

/// `chevauchement: file` — a BOUNDED in-memory wait, never a durable
/// queue: the wait seam carries each quantum (the OS firer sleeps,
/// `serve` races its signals, the harness advances its clock) until
/// the running tick releases the lock or the beat's NEXT slot ends the
/// budget (firing the old one past it would double the new one). On
/// acquisition the state is RE-READ and RE-DECIDED — the pre-wait slot
/// is stale by law.
fn wait_turn(ctx: &FireCtx, slot: &Zoned, holder: u32) -> FireVerdict {
    let budget_ms = wait_budget_ms(ctx);
    let mut waited_ms = 0i64;
    loop {
        if waited_ms >= budget_ms {
            return act_on_skip(
                ctx,
                format!(
                    "skipped {} · overlap-timeout · pid {holder} a tenu jusqu'au créneau suivant · slot {}",
                    ctx.label,
                    slot.timestamp()
                ),
                "overlap-timeout".to_owned(),
                Some(slot),
                true,
            );
        }
        let step = wait_quantum(budget_ms, waited_ms);
        if step <= 0 {
            // A malformed or mutated quantum cannot turn the resident loop
            // into a busy wait. Advancing to the bound preserves the queue's
            // fail-closed timeout semantics.
            waited_ms = budget_ms;
            continue;
        }
        match (ctx.wait)(SignedDuration::from_millis(step)) {
            // A signal broke the wait (serve is stopping): the tick is
            // abandoned — journaled with its slot, never run.
            Wait::Interrupted => {
                return act_on_skip(
                    ctx,
                    format!(
                        "skipped {} · serve-stop · slot {}",
                        ctx.label,
                        slot.timestamp()
                    ),
                    "serve-stop".to_owned(),
                    Some(slot),
                    true,
                );
            }
            Wait::Elapsed => waited_ms = waited_ms.saturating_add(step),
        }
        match ctx.state.acquire_beat_lock(&ctx.label, ctx.pid, &ctx.now) {
            Ok(attempt) if matches!(attempt.outcome, LockOutcome::Acquired) => {
                // The re-decision law: the state may have changed under
                // the wait — decide again, under the lock.
                let Some(lease) = attempt.lease else {
                    return FireVerdict {
                        line: format!("failed {} · the lock returned no lease", ctx.label),
                        code: exit::ENV,
                    };
                };
                return decide_locked(ctx, lease);
            }
            Ok(_) => {}
            Err(e) => {
                return FireVerdict {
                    line: format!("failed {} · the lock refused: {e}", ctx.label),
                    code: exit::ENV,
                };
            }
        }
    }
}

fn wait_budget_ms(ctx: &FireCtx) -> i64 {
    next_slot(ctx).map_or(0, |next| {
        next.timestamp()
            .as_millisecond()
            .saturating_sub(ctx.now.timestamp().as_millisecond())
    })
}

fn wait_quantum(budget_ms: i64, waited_ms: i64) -> i64 {
    budget_ms.saturating_sub(waited_ms).min(POLL_MS)
}

/// Act on a Skip decision: journal it when it bears one (the inner
/// ledger lock serializes the append; the caller's beat lock, when it
/// holds one, outlives this), then the ONE line — the ledger's repair
/// count riding it when the chain had to be healed (D8 stays one line).
fn act_on_skip(
    ctx: &FireCtx,
    line: String,
    reason: String,
    slot: Option<&Zoned>,
    journal: bool,
) -> FireVerdict {
    if !journal {
        return FireVerdict {
            line,
            code: exit::OK,
        };
    }
    let mut entry = HistoryEntry::new(
        slot.map(Zoned::timestamp),
        ctx.now.timestamp(),
        FireKind::Skipped,
    );
    entry.reason = Some(reason);
    entry.exit = Some(exit::OK);
    entry.slot_id = slot.and_then(|s| slot_id_of(ctx, s));
    match ctx.state.record(&ctx.label, &entry) {
        Ok(outcome) => FireVerdict {
            line: with_repair(line, outcome.repaired),
            code: exit::OK,
        },
        Err(e) => record_refused(ctx, &e),
    }
}

/// The Fire branch, under the lock — the order law made code: the
/// CLAIM is appended + fsync'd BEFORE the run (a crash after it leaves
/// a visible orphan — the v0 crash-detector's whole point), the
/// receipt (+ fsync · last.json · watermark) lands AFTER, the release
/// comes last of all (the caller's guard). The receipt fences the
/// claim's seq — that link is what makes an orphan unambiguous.
fn claim_run_receipt(
    ctx: &FireCtx,
    lease: &super::state::LockLease,
    slot: &Zoned,
    slots: Option<u32>,
) -> FireVerdict {
    let Some(beat) = beat_of(ctx) else {
        return FireVerdict {
            line: format!(
                "failed {} · engine fault: the label resolved past the registry",
                ctx.label
            ),
            code: exit::FILE,
        };
    };
    let Some(plafond) = beat.plafond else {
        return FireVerdict {
            line: format!(
                "failed {} · engine fault: plafond absent après validation — à reporter avec le fichier",
                ctx.label
            ),
            code: exit::FILE,
        };
    };
    let pinned = match pin_workflow(ctx, beat) {
        Ok(pinned) => pinned,
        Err(error) => return record_refused(ctx, &error),
    };
    let mut claim = Claim::new(
        SlotId::derive(&beat.workflow, &beat.cadence, slot),
        next_slot(ctx).map_or_else(
            || {
                ctx.now
                    .timestamp()
                    .checked_add(CLAIM_DEADLINE_FALLBACK)
                    .unwrap_or_else(|_| ctx.now.timestamp())
            },
            |next| next.timestamp(),
        ),
        ctx.now.timestamp(),
    );
    claim.generation = Some(pinned.generation.clone());
    let mut repaired = 0u64;
    let fencing = match ArmState::record_claim_with_lease(lease, &claim) {
        Ok(outcome) => {
            repaired = repaired.saturating_add(outcome.repaired);
            outcome.seq
        }
        Err(e) => return record_refused(ctx, &e),
    };
    let upshot = (ctx.run)(&RunShot {
        project: pinned.project,
        root: ctx.project_root.clone(),
        workflow: beat.workflow.clone(),
        source: pinned.source,
        generation: pinned.generation,
        ceiling: plafond,
    });
    let folded = fold_finished_run(&claim, fencing, upshot.code);
    let (kind, line) = verdict_line(
        ctx,
        slot,
        slots,
        folded,
        upshot.code,
        upshot.trace.as_deref(),
    );
    let receipt = Receipt::for_claim(
        &claim,
        FencingToken::new(fencing),
        slot.timestamp(),
        ctx.now.timestamp(),
        upshot.trace,
        upshot.code,
        slots,
    );
    debug_assert_eq!(receipt.kind(), kind);
    match ArmState::record_receipt_with_lease(lease, &receipt) {
        Ok(outcome) => repaired = repaired.saturating_add(outcome.repaired),
        Err(e) => return record_refused(ctx, &e),
    }
    FireVerdict {
        line: with_repair(line, repaired),
        code: upshot.code,
    }
}

/// Fold the run's terminal lifecycle before the receipt speaks its kind.
fn fold_finished_run(claim: &Claim, fencing: u64, code: u8) -> FiringState {
    firing::fold(&[
        FiringEvent::Due,
        FiringEvent::Claimed {
            fencing: FencingToken::new(fencing),
            generation: claim.generation.clone(),
            deadline: claim.deadline,
        },
        FiringEvent::Started {
            fencing: FencingToken::new(fencing),
        },
        FiringEvent::Finished {
            fencing: Some(FencingToken::new(fencing)),
            code,
        },
    ])
}

/// The held beat lock, released on EVERY exit of the scope that took
/// it. The kernel releases the advisory lease even if the process dies.
struct HeldBeatLock {
    lease: super::state::LockLease,
}

/// The beat's next theoretical slot after the decision instant — the
/// queue's budget AND the claim's deadline ride the same planner door.
fn next_slot(ctx: &FireCtx) -> Option<Zoned> {
    beat_of(ctx)
        .and_then(|b| Cadence::parse(&b.cadence).ok())
        .and_then(|c| c.next_after(&ctx.now))
        .map(|s| s.at)
}

/// The slot's canonical identity, computed from the beat's own
/// declaration (the path + the cadence, VERBATIM — never the label).
/// The derivation lives in the cadence machine since W7 (D4).
fn slot_id_of(ctx: &FireCtx, slot: &Zoned) -> Option<SlotId> {
    let beat = beat_of(ctx)?;
    Some(SlotId::derive(&beat.workflow, &beat.cadence, slot))
}

struct PinnedWorkflow {
    project: OwnedDir,
    generation: ArmGeneration,
    source: String,
}

fn pin_workflow(ctx: &FireCtx, beat: &Beat) -> std::io::Result<PinnedWorkflow> {
    let (project, mut source) = ctx.state.open_project_file(Path::new(&beat.workflow))?;
    if !source.metadata()?.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "arm workflow: source is not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    source.read_to_end(&mut bytes)?;
    let generation = ArmGeneration::compute(beat, &bytes);
    let source = String::from_utf8(bytes).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("arm workflow: source is not UTF-8: {error}"),
        )
    })?;
    Ok(PinnedWorkflow {
        project,
        generation,
        source,
    })
}

/// The repair suffix — the ledger's self-healing is SAID on the
/// decision line (D8 stays one line).
fn with_repair(line: String, repaired: u64) -> String {
    if repaired == 0 {
        line
    } else {
        format!("{line} · ledger réparé (-{repaired})")
    }
}

/// The record's refusal: the failure line REPLACES the decision's (a
/// fire without its record is a fire that re-fires) — exit ENV, and
/// the caller's guard still releases the lock.
fn record_refused(ctx: &FireCtx, e: &std::io::Error) -> FireVerdict {
    FireVerdict {
        line: format!("failed {} · the record refused: {e}", ctx.label),
        code: exit::ENV,
    }
}

/// The OS firer's wait: the process sleeps the span, whole — its stop
/// is the OS unit's own kill, no signal is scrutinized here (②'s
/// signal-aware seam is serve's).
fn os_wait(span: SignedDuration) -> Wait {
    std::thread::sleep(std::time::Duration::try_from(span).unwrap_or_default());
    Wait::Elapsed
}

/// The line + the kind for a run that went (rc 0 · 1 · 2 · 3 · 4).
/// The kind FOLLOWS the machine's fold (D5) — the line's shape is
/// unchanged (D8 stays byte-identical).
fn verdict_line(
    ctx: &FireCtx,
    slot: &Zoned,
    slots: Option<u32>,
    state: FiringState,
    code: u8,
    trace: Option<&str>,
) -> (FireKind, String) {
    let kind = match state {
        FiringState::Succeeded => FireKind::Fired,
        FiringState::Cancelled => FireKind::Paused,
        // FailedRetryable · FailedPermanent — and the defensive floor:
        // the fold of [due · claimed · started · finished] lands
        // nowhere else, so this arm doubles as the engine-fault-safe one.
        _ => FireKind::Failed,
    };
    let label = ctx.label.as_str();
    let slot_rfc = slot.timestamp().to_string();
    let catchup = slots.map_or(String::new(), |n| format!(" · rattrapage ×{n}"));
    let via_trace = trace.map_or(String::new(), |t| format!(" · trace {t}"));
    match code {
        exit::OK => (
            kind,
            format!("fired {label} · slot {slot_rfc}{catchup} · exit 0{via_trace}"),
        ),
        exit::PAUSED => (
            kind,
            format!(
                "paused {label} · slot {slot_rfc}{via_trace} — garé (N2: jamais repris, jamais répondu)"
            ),
        ),
        _ => (
            kind,
            format!("failed {label} · slot {slot_rfc} · exit {code}{via_trace}"),
        ),
    }
}

/// The beat at the context's index (validated: always present).
fn beat_of(ctx: &FireCtx) -> Option<&Beat> {
    ctx.registry.beats().nth(ctx.index)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// A one-beat registry (validated green) — the `decide` fixture.
    fn registry_with(body: &str) -> ArmRegistry {
        let text = format!(
            "nika: v1\narm:\n  - workflow: workflows/doctor.nika.yaml\n    cadence: \"TZ=UTC 0 3 * * *\"\n    plafond: 0.25\n{body}"
        );
        let registry = nika_cadence::parse_registry(&text).expect("parse");
        assert!(
            nika_cadence::validate(&registry).next().is_none(),
            "the fixture must be lawful"
        );
        registry
    }

    fn test_context(body: &str, now: &str) -> FireCtx {
        FireCtx {
            project_root: PathBuf::from("/project"),
            registry: registry_with(body),
            index: 0,
            label: "doctor".to_owned(),
            now: at(now),
            state: ArmState::at_project(Path::new("/project")),
            pid: 7,
            wait: Box::new(os_wait),
            run: Rc::new(|_| RunUpshot::new(exit::OK, None)),
        }
    }

    const BASE: &str = "    manqué: sauter\n";

    fn at(text: &str) -> Zoned {
        text.parse::<Timestamp>()
            .expect("ts")
            .to_zoned(jiff::tz::TimeZone::UTC)
    }

    fn decide_base(now: &Zoned, last: Option<&Zoned>) -> Decision {
        let registry = registry_with(BASE);
        decide(&registry, 0, "doctor", now, last)
    }

    #[test]
    fn public_run_and_verdict_projections_preserve_every_value() {
        let project = tempfile::tempdir().expect("project");
        let registry = registry_with(BASE);
        let generation = ArmGeneration::compute(
            registry.beats().next().expect("beat"),
            b"schema: nika/workflow@0.12\ntasks: {}\n",
        );
        let shot = RunShot {
            project: OwnedDir::open(project.path()).expect("project capability"),
            root: project.path().to_path_buf(),
            workflow: "workflows/doctor.nika.yaml".to_owned(),
            source: "schema: nika/workflow@0.12\ntasks: {}\n".to_owned(),
            generation: generation.clone(),
            ceiling: 0.25,
        };
        assert_eq!(shot.root(), project.path());
        assert_eq!(shot.workflow(), "workflows/doctor.nika.yaml");
        assert_eq!(shot.source(), "schema: nika/workflow@0.12\ntasks: {}\n");
        assert_eq!(shot.generation(), &generation);
        assert_eq!(shot.ceiling().to_bits(), 0.25f64.to_bits());

        let (line, code) = FireVerdict {
            line: "paused doctor".to_owned(),
            code: exit::PAUSED,
        }
        .into_parts();
        assert_eq!(line, "paused doctor");
        assert_eq!(code, exit::PAUSED);
    }

    #[test]
    fn context_derives_the_label_and_rejects_an_invalid_index() {
        let registry = registry_with(BASE);
        let Err(error) = FireCtx::new(
            PathBuf::from("/project"),
            registry,
            1,
            at("2026-08-19T03:02:00Z"),
            7,
            Rc::new(|_| RunUpshot::new(exit::OK, None)),
        ) else {
            panic!("one-beat registry has no index one");
        };
        assert_eq!(error.index, 1);
        assert_eq!(error.into_registry().beat_count(), 1);
    }

    #[test]
    fn trace_projection_cannot_break_the_one_line_contract() {
        let upshot = RunUpshot::new(exit::PAUSED, Some("first\r\nsecond".to_owned()));
        assert_eq!(upshot.trace.as_deref(), Some("first\\r\\nsecond"));
    }

    /// On time, never fired: the window alone decides (N2) — FIRE.
    #[test]
    fn an_on_time_slot_without_state_fires() {
        match decide_base(&at("2026-08-19T03:02:00Z"), None) {
            Decision::Fire { slot, slots } => {
                assert_eq!(slot.timestamp().to_string(), "2026-08-19T03:00:00Z");
                assert_eq!(slots, None);
            }
            _ => panic!("on-time without state fires"),
        }
    }

    /// Never fired and the window long gone: no state invents no
    /// backlog (N2) — skipped, journaled NOWHERE.
    #[test]
    fn a_first_contact_beyond_the_window_skips_without_a_record() {
        match decide_base(&at("2026-08-19T10:00:00Z"), None) {
            Decision::Skip {
                reason, journal, ..
            } => {
                assert_eq!(reason, "not-due");
                assert!(!journal, "N2 writes nothing");
            }
            _ => panic!("hors fenêtre sans état saute"),
        }
    }

    /// The slot already DECIDED: a duplicate tick is a no-op.
    #[test]
    fn an_already_decided_slot_skips_without_a_record() {
        let fired = at("2026-08-19T03:00:00Z");
        match decide_base(&at("2026-08-19T03:01:00Z"), Some(&fired)) {
            Decision::Skip {
                reason, journal, ..
            } => {
                assert_eq!(reason, "already");
                assert!(!journal);
            }
            _ => panic!("déjà décidé saute"),
        }
    }

    #[test]
    fn a_different_prior_slot_is_not_mislabeled_already() {
        let older = at("2026-08-17T03:00:00Z");
        match decide_base(&at("2026-08-19T10:00:00Z"), Some(&older)) {
            Decision::Skip {
                reason, journal, ..
            } => {
                assert_eq!(reason, "missed:2");
                assert!(journal);
            }
            _ => panic!("the planner owns the two missed slots"),
        }

        let future = at("2026-08-20T03:00:00Z");
        match decide_base(&at("2026-08-19T10:00:00Z"), Some(&future)) {
            Decision::Skip {
                reason, journal, ..
            } => {
                assert_eq!(reason, "not-due");
                assert!(!journal);
            }
            _ => panic!("a non-matching state is never called already"),
        }
    }

    /// A missed slot under `manqué: sauter`: skipped, journaled WITH
    /// the slot (a skip consumes it).
    #[test]
    fn a_missed_slot_under_sauter_is_journaled_and_consumed() {
        let fired = at("2026-08-18T03:00:00Z");
        match decide_base(&at("2026-08-19T10:00:00Z"), Some(&fired)) {
            Decision::Skip {
                reason,
                slot,
                journal,
                line,
            } => {
                assert_eq!(reason, "missed:1");
                assert!(journal);
                assert_eq!(
                    slot.expect("the consumed slot").timestamp().to_string(),
                    "2026-08-19T03:00:00Z"
                );
                assert!(line.starts_with("skipped doctor · missed:1"), "{line}");
            }
            _ => panic!("un créneau raté saute"),
        }
    }

    /// `rattraper-une-fois`: ONE fire answers for the whole silence.
    #[test]
    fn rattraper_une_fois_fires_once_for_the_whole_silence() {
        let registry = registry_with("    manqué: rattraper-une-fois\n");
        let fired = at("2026-08-17T03:00:00Z");
        match decide(
            &registry,
            0,
            "doctor",
            &at("2026-08-19T03:02:00Z"),
            Some(&fired),
        ) {
            Decision::Fire { slot, slots } => {
                assert_eq!(slot.timestamp().to_string(), "2026-08-19T03:00:00Z");
                assert_eq!(slots, Some(2), "the 18th AND the 19th");
            }
            _ => panic!("un seul tir pour tout le silence"),
        }
    }

    /// The file's own truth, judged before the clock.
    #[test]
    fn inactive_cloud_and_expired_beats_skip_with_their_reason() {
        let registry = registry_with(concat!(
            "    manqué: sauter\n",
            "    actif: false\n",
            "    raison: \"pause estivale\"\n",
            "    jusqu_au: \"2099-12-31\"\n",
        ));
        match decide(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
            Decision::Skip { reason, line, .. } => {
                assert_eq!(reason, "inactive");
                assert!(line.contains("pause estivale"), "{line}");
            }
            _ => panic!("un beat inactif saute"),
        }

        let registry = registry_with("    manqué: sauter\n    où: cloud\n");
        match decide(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
            Decision::Skip { reason, .. } => assert_eq!(reason, "cloud"),
            _ => panic!("un beat cloud saute"),
        }

        let registry = registry_with("    manqué: sauter\n    jusqu_au: \"2026-01-01\"\n");
        match decide(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
            Decision::Skip { reason, .. } => assert_eq!(reason, "expired"),
            _ => panic!("un beat expiré saute"),
        }
    }

    #[test]
    fn expiry_is_strictly_before_the_decision_date() {
        let registry = registry_with("    manqué: sauter\n    jusqu_au: \"2026-08-19\"\n");
        let beat = registry.beats().next().expect("beat");
        assert_eq!(expiry_passed(beat, &at("2026-08-19T23:59:59Z")), None);
        assert_eq!(
            expiry_passed(beat, &at("2026-08-20T00:00:00Z")).as_deref(),
            Some("2026-08-19")
        );
    }

    #[test]
    fn paused_run_state_projects_a_paused_kind_and_line() {
        let ctx = test_context(BASE, "2026-08-19T03:02:00Z");
        let (kind, line) = verdict_line(
            &ctx,
            &at("2026-08-19T03:00:00Z"),
            None,
            FiringState::Cancelled,
            exit::PAUSED,
            Some("trace.ndjson"),
        );
        assert_eq!(kind, FireKind::Paused);
        assert_eq!(
            line,
            "paused doctor · slot 2026-08-19T03:00:00Z · trace trace.ndjson — garé (N2: jamais repris, jamais répondu)"
        );
    }

    #[test]
    fn queue_budget_and_quantum_are_bounded_subtractions() {
        let ctx = test_context(BASE, "2026-08-19T03:02:00Z");
        assert_eq!(wait_budget_ms(&ctx), 86_280_000);
        assert_eq!(wait_quantum(2_500, 0), 1_000);
        assert_eq!(wait_quantum(2_500, 1_000), 1_000);
        assert_eq!(wait_quantum(2_500, 2_000), 500);
        assert_eq!(wait_quantum(500, 500), 0);
    }

    /// A webhook beat fires on its event, never on the clock.
    #[test]
    fn a_webhook_beat_skips_the_clock() {
        let text = concat!(
            "nika: v1\n",
            "arm:\n",
            "  - workflow: workflows/doctor.nika.yaml\n",
            "    cadence: \"on-webhook\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
        );
        let registry = nika_cadence::parse_registry(text).expect("parse");
        match decide(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
            Decision::Skip { reason, .. } => assert_eq!(reason, "webhook"),
            _ => panic!("un webhook saute l'horloge"),
        }
    }

    /// The v0 refusals (D6) — each names the version it arrives with.
    #[test]
    fn the_v0_unsupported_policies_refuse_with_teaching() {
        for (extra, named) in [
            ("    chevauchement: remplacer\n", "chevauchement: remplacer"),
            (
                "    chevauchement: sauter\n    après_saut: à-complétion\n",
                "après_saut: à-complétion",
            ),
            ("    décalage: hash\n", "décalage:"),
        ] {
            let registry = registry_with(&format!("    manqué: sauter\n{extra}"));
            match decide(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
                Decision::Refuse { line } => {
                    assert!(line.contains(named), "{line}");
                    assert!(line.contains("serve v0.2"), "names the version: {line}");
                }
                _ => panic!("{named} doit refuser"),
            }
        }
        let registry = registry_with("    manqué: rattraper\n");
        match decide(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
            Decision::Refuse { line } => {
                assert!(line.contains("manqué: rattraper"), "{line}");
                assert!(line.contains("serve v0.2"), "{line}");
            }
            _ => panic!("rattraper doit refuser"),
        }
    }

    /// D4: the label is the workflow file's radical; a collision takes
    /// `-2`, `-3` in file order.
    #[test]
    fn labels_are_the_radicals_with_collision_suffixes() {
        let text = concat!(
            "nika: v1\n",
            "arm:\n",
            "  - workflow: a/doctor.nika.yaml\n",
            "    cadence: \"TZ=UTC 0 3 * * *\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
            "  - workflow: b/doctor.nika.yaml\n",
            "    cadence: \"TZ=UTC 0 4 * * *\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
            "  - workflow: c/doctor.nika.yaml\n",
            "    cadence: \"TZ=UTC 0 5 * * *\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
            "  - workflow: nightly.nika.yaml\n",
            "    cadence: \"TZ=UTC 0 6 * * *\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
        );
        let registry = nika_cadence::parse_registry(text).expect("parse");
        assert_eq!(
            labels(&registry),
            vec!["doctor", "doctor-2", "doctor-3", "nightly"]
        );
    }
}
