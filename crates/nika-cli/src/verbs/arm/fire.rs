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
//! [`state`](super::state) (D3), and the clock is injected at the
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

use jiff::{SignedDuration, Timestamp, Zoned};
use nika_cadence::firing::{self, ArmGeneration, FencingToken, FiringEvent, FiringState, SlotId};
use nika_cadence::registry::{AfterSkip, ArmRegistry, Beat, Cadence, Locus, MissPolicy, Overlap};
use nika_vocab::project;

use super::args::FireArgs;
use super::state::{ArmState, Claim, FireKind, HistoryEntry, LockOutcome, Receipt};
use crate::verbs::{self, VerbOutput, exit};

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
pub struct FireCtx {
    /// The project root (the directory holding `nika.yaml`).
    pub project_root: PathBuf,
    /// The parsed + validated registry.
    pub registry: ArmRegistry,
    /// The beat's position in the registry (label resolution is the
    /// caller's — [`labels`]).
    pub index: usize,
    /// The beat's label (the workflow file radical, D4).
    pub label: String,
    /// The decision instant — injected (D5); the pure code never reads
    /// a clock.
    pub now: Zoned,
    /// The firing sidecar (D3).
    pub state: ArmState,
    /// The firer's own pid, written into the locks it takes (the edges
    /// pass `std::process::id()`; a test can lend a child's).
    pub pid: u32,
    /// The sleep seam — the queue's wait (`chevauchement: file`) rides
    /// it: the OS firer sleeps, `serve` races the signals, the harness
    /// advances its scripted clock. D5's twin: the firer never sleeps
    /// uninjected.
    pub(crate) wait: WaitSeam,
    /// The run seam — the real in-process run by default ([`prod_run`]);
    /// the tests stub it, W10's sim will mock it here.
    pub(crate) run: RunSeam,
}

/// What a wait returned: the span elapsed whole, or a signal broke it
/// (the firer then skips `serve-stop` — never runs, never blocks).
pub(crate) enum Wait {
    /// The full span elapsed.
    Elapsed,
    /// A stop signal broke the wait (serve's ctrl-c / SIGTERM).
    Interrupted,
}

/// The wait seam's shape: a span in, the outcome out.
pub(crate) type WaitSeam = Box<dyn Fn(SignedDuration) -> Wait>;

pub(crate) struct RunShot {
    pub root: PathBuf,
    pub workflow: String,
    pub source: crate::verbs::RunSource,
    pub generation: ArmGeneration,
    pub plafond: f64,
}

pub(crate) struct RunUpshot {
    pub code: u8,
    pub trace: Option<String>,
}

pub(crate) type RunSeam = Rc<dyn Fn(&RunShot) -> RunUpshot>;

/// What a fire leaves: the ONE stdout line (D8) + the process exit.
pub struct FireVerdict {
    /// The single line — `fired …` · `skipped …` · `paused …` ·
    /// `failed …`.
    pub line: String,
    /// `0` fired/skipped · `2` a refusal · the run's rc (1·2·3·4) when
    /// the run went.
    pub code: u8,
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

/// `nika arm fire <label>` — the verb edge: discover, parse, judge the
/// label, inject the clock (D5), then the one firer. The report's CWD
/// door and refusal voice, unchanged.
#[must_use]
pub fn run(fire: &FireArgs) -> VerbOutput {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let found = match project::discover(&cwd) {
        Ok(found) => found,
        Err(e) => return VerbOutput::file(format!("PROJECT ✗  {e}")),
    };
    let Some((path, _project)) = found else {
        return VerbOutput::file(
            "nothing armed — this project has no `nika.yaml`\n  \
             fix: `nika init --project-file` lays a commented starter"
                .to_owned(),
        );
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => return VerbOutput::env(format!("cannot read {}: {e}", path.display())),
    };
    let registry = match nika_cadence::parse_registry(&text) {
        Ok(registry) => registry,
        Err(e) => return VerbOutput::file(format!("ARM ✗  {e}")),
    };
    let faults: Vec<String> = nika_cadence::validate(&registry)
        .map(|e| format!("  {e}"))
        .collect();
    if !faults.is_empty() {
        return VerbOutput::file(format!(
            "ARM ✗  {} in {}\n{}",
            crate::text::count(faults.len(), "refusal"),
            path.display(),
            faults.join("\n")
        ));
    }
    let labels = labels(&registry);
    let Some(index) = labels.iter().position(|l| l == &fire.label) else {
        return VerbOutput::file(format!(
            "arm fire: unknown beat `{}` — this project arms: {}",
            fire.label,
            labels.join(" · ")
        ));
    };
    let now = match parse_now(fire.now.as_deref()) {
        Ok(now) => now,
        Err(line) => return VerbOutput::file(line),
    };
    let root = path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let ctx = FireCtx {
        state: ArmState::at_project(&root),
        project_root: root,
        registry,
        index,
        label: fire.label.clone(),
        now,
        pid: std::process::id(),
        wait: Box::new(os_wait),
        run: Rc::new(prod_run),
    };
    let verdict = fire_beat(&ctx);
    VerbOutput {
        text: verdict.line,
        code: verdict.code,
    }
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
fn parse_now(raw: Option<&str>) -> Result<Zoned, String> {
    match raw {
        None => Ok(Zoned::now()),
        Some(text) => text
            .parse::<Zoned>()
            .or_else(|_| {
                text.parse::<Timestamp>()
                    .map(|t| t.to_zoned(jiff::tz::TimeZone::UTC))
            })
            .map_err(|_| {
                format!("arm fire: --now `{text}` · RFC 3339 attendu — 2026-08-19T03:02:00Z")
            }),
    }
}

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
    let budget_ms = next_slot(ctx).map_or(0, |next| {
        next.timestamp().as_millisecond() - ctx.now.timestamp().as_millisecond()
    });
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
        let step = (budget_ms - waited_ms).min(POLL_MS);
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
            Wait::Elapsed => waited_ms += step,
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
            repaired += outcome.repaired;
            outcome.seq
        }
        Err(e) => return record_refused(ctx, &e),
    };
    let upshot = (ctx.run)(&RunShot {
        root: ctx.project_root.clone(),
        workflow: beat.workflow.clone(),
        source: pinned.source,
        generation: pinned.generation,
        plafond,
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
        Ok(outcome) => repaired += outcome.repaired,
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
    generation: ArmGeneration,
    source: crate::verbs::RunSource,
}

fn pin_workflow(ctx: &FireCtx, beat: &Beat) -> std::io::Result<PinnedWorkflow> {
    let path = ctx.project_root.join(&beat.workflow);
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC);
    }
    let mut source = options.open(&path)?;
    if !source.metadata()?.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "arm workflow: source is not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    source.read_to_end(&mut bytes)?;
    let generation = ArmGeneration::compute(beat, &bytes);
    let source = crate::verbs::RunSource::from_bytes(beat.workflow.clone(), bytes)?;
    Ok(PinnedWorkflow { generation, source })
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

pub(crate) fn prod_run(shot: &RunShot) -> RunUpshot {
    debug_assert!(!shot.workflow.is_empty());
    debug_assert_eq!(shot.generation.as_str().len(), 64);
    let Ok(_room) = enter_room(&shot.root) else {
        return RunUpshot {
            code: exit::ENV,
            trace: None,
        };
    };
    let receipt = run_quietly(|| {
        verbs::run::run_checked_source(
            shot.source.clone(),
            crate::Theme::new(false, true, false),
            shot.plafond,
        )
    });
    RunUpshot {
        code: receipt.code,
        trace: receipt
            .trace
            .map(|path| path.to_string_lossy().into_owned()),
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

struct RoomGuard(Option<PathBuf>);

impl Drop for RoomGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.0 {
            let _ = std::env::set_current_dir(previous);
        }
    }
}

fn enter_room(root: &Path) -> std::io::Result<RoomGuard> {
    let previous = std::env::current_dir().ok();
    std::env::set_current_dir(root)?;
    Ok(RoomGuard(previous))
}

/// Turn stdout onto stderr for the run's duration — D8's one-line
/// machine surface stays byte-pure while the fold narrates to the log
/// (nix's `dup2_stdout` pair; restored on drop, panic included).
#[cfg(unix)]
struct StdoutGuard {
    saved: std::os::fd::OwnedFd,
}

#[cfg(unix)]
impl StdoutGuard {
    fn enter() -> std::io::Result<Self> {
        use std::io::Write as _;
        std::io::stdout().flush()?;
        let saved = nix::unistd::dup(std::io::stdout())?;
        nix::unistd::dup2_stdout(std::io::stderr())?;
        Ok(Self { saved })
    }
}

#[cfg(unix)]
impl Drop for StdoutGuard {
    fn drop(&mut self) {
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
        let _ = nix::unistd::dup2_stdout(&self.saved);
    }
}

#[cfg(unix)]
fn run_quietly<T>(f: impl FnOnce() -> T) -> T {
    match StdoutGuard::enter() {
        Ok(_guard) => f(),
        Err(_) => f(),
    }
}

#[cfg(not(unix))]
fn run_quietly<T>(f: impl FnOnce() -> T) -> T {
    f()
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

    /// `--now`: bare RFC 3339 lands on UTC; garbage teaches.
    #[test]
    fn the_injected_clock_parses_rfc3339_and_refuses_garbage() {
        let now = parse_now(Some("2026-08-19T03:02:00Z")).expect("parses");
        assert_eq!(now.timestamp().to_string(), "2026-08-19T03:02:00Z");
        let zoned = parse_now(Some("2026-08-19T05:02:00+02:00[Europe/Paris]")).expect("parses");
        assert_eq!(zoned.timestamp().to_string(), "2026-08-19T03:02:00Z");
        assert!(parse_now(Some("demain")).is_err());
    }
}
