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

use std::path::{Path, PathBuf};

use jiff::{Timestamp, Zoned};
use nika_cadence::registry::{AfterSkip, ArmRegistry, Beat, Cadence, Locus, MissPolicy, Overlap};
use nika_vocab::project;

use super::args::FireArgs;
use super::state::{ArmState, FireKind, HistoryEntry, LockOutcome};
use crate::verbs::run::RenderMode;
use crate::verbs::{self, VerbOutput, exit};

/// The poll quantum while a queued tick (`chevauchement: file`) waits
/// for the running one to release the lock.
const POLL_MS: i64 = 1_000;

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
}

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
    };
    let verdict = fire_beat(&ctx);
    VerbOutput {
        text: verdict.line,
        code: verdict.code,
    }
}

/// The beat labels, in file order (D4): the workflow file's radical —
/// `workflows/doctor.nika.yaml` → `doctor` — a collision taking `-2`,
/// `-3`. The OS unit names itself `nika.arm.<radical>` from this list.
#[must_use]
pub fn labels(registry: &ArmRegistry) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for beat in registry.beats() {
        let radical = radical_of(&beat.workflow);
        let mut candidate = radical.clone();
        let mut n = 1u32;
        // Uniqueness is the law (`doctor-2.nika.yaml` arming next to
        // two `doctor` must never hand two beats one label).
        while out.contains(&candidate) {
            n += 1;
            candidate = format!("{radical}-{n}");
        }
        out.push(candidate);
    }
    out
}

/// The radical of a workflow path: the basename minus the `.nika.yaml`
/// the grammar guarantees (a bare `*.yaml` loses its last extension).
fn radical_of(workflow: &str) -> String {
    let base = workflow.rsplit('/').next().unwrap_or(workflow);
    if let Some(radical) = base.strip_suffix(".nika.yaml") {
        return radical.to_owned();
    }
    base.rsplit_once('.')
        .map_or_else(|| base.to_owned(), |(stem, _)| stem.to_owned())
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

/// The one firer (D2): decide, then act — journal the skip, or lock
/// (law ⑥) and run. Always the ONE line (D8).
#[must_use]
pub fn fire_beat(ctx: &FireCtx) -> FireVerdict {
    let last = ctx.state.last_fired(&ctx.label);
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
        } => {
            if journal {
                let verdict = write_record(
                    ctx,
                    &HistoryEntry {
                        slot: slot.as_ref().map(Zoned::timestamp),
                        decided_at: ctx.now.timestamp(),
                        kind: FireKind::Skipped,
                        reason: Some(reason),
                        trace: None,
                        exit: Some(exit::OK),
                        slots: None,
                    },
                );
                if let Some(verdict) = verdict {
                    return verdict;
                }
            }
            FireVerdict {
                line,
                code: exit::OK,
            }
        }
        Decision::Fire { slot, slots } => fire_slot(ctx, &slot, slots),
    }
}

/// The lock, then the shot.
fn fire_slot(ctx: &FireCtx, slot: &Zoned, slots: Option<u32>) -> FireVerdict {
    match ctx.state.try_lock(&ctx.label, std::process::id(), &ctx.now) {
        Err(e) => FireVerdict {
            line: format!("failed {} · the lock refused: {e}", ctx.label),
            code: exit::ENV,
        },
        Ok(LockOutcome::HeldAlive { pid }) => match beat_of(ctx).map(Beat::overlap) {
            Some(Overlap::Sauter) => say_after_journal(
                ctx,
                FireKind::Skipped,
                Some("overlap".to_owned()),
                slot,
                None,
                None,
                exit::OK,
                format!(
                    "skipped {} · overlap · pid {pid} tient le créneau",
                    ctx.label
                ),
            ),
            Some(Overlap::File) => poll_queue(ctx, slot, slots, pid),
            // `remplacer` is a v0 refusal upstream — an arrival here is
            // an engine fault, said as such, never an approximation.
            _ => FireVerdict {
                line: format!(
                    "failed {} · engine fault: remplacer reached the lock",
                    ctx.label
                ),
                code: exit::FILE,
            },
        },
        Ok(LockOutcome::Acquired | LockOutcome::StaleTaken { .. }) => {
            run_and_record(ctx, slot, slots)
        }
    }
}

/// `chevauchement: file` — poll the lock each second until the running
/// tick releases it, or until the beat's NEXT slot (firing the old one
/// past it would double the new one).
fn poll_queue(ctx: &FireCtx, slot: &Zoned, slots: Option<u32>, holder: u32) -> FireVerdict {
    let budget_ms = beat_of(ctx)
        .and_then(|b| Cadence::parse(&b.cadence).ok())
        .and_then(|c| c.next_after(&ctx.now))
        .map_or(0, |next| {
            next.at.timestamp().as_millisecond() - ctx.now.timestamp().as_millisecond()
        });
    let mut waited_ms = 0i64;
    loop {
        if waited_ms >= budget_ms {
            return say_after_journal(
                ctx,
                FireKind::Skipped,
                Some("overlap-timeout".to_owned()),
                slot,
                None,
                None,
                exit::OK,
                format!(
                    "skipped {} · overlap-timeout · pid {holder} a tenu jusqu'au créneau suivant",
                    ctx.label
                ),
            );
        }
        let step = (budget_ms - waited_ms).min(POLL_MS);
        std::thread::sleep(std::time::Duration::from_millis(
            u64::try_from(step).unwrap_or(0),
        ));
        waited_ms += step;
        match ctx.state.try_lock(&ctx.label, std::process::id(), &ctx.now) {
            Ok(LockOutcome::Acquired | LockOutcome::StaleTaken { .. }) => {
                return run_and_record(ctx, slot, slots);
            }
            Ok(LockOutcome::HeldAlive { .. }) => {}
            Err(e) => {
                return FireVerdict {
                    line: format!("failed {} · the lock refused: {e}", ctx.label),
                    code: exit::ENV,
                };
            }
        }
    }
}

/// The shot: enter the project (traces and workflow-relative paths
/// resolve at the root), turn stdout onto stderr for the run's
/// duration (D8), run IN-PROCESS with the `nika run` CLI defaults —
/// the per-tick ceiling ALWAYS set (law 7) — then record, release, and
/// say the ONE line.
fn run_and_record(ctx: &FireCtx, slot: &Zoned, slots: Option<u32>) -> FireVerdict {
    let beat = beat_of(ctx);
    let Some(plafond) = beat.and_then(|b| b.plafond) else {
        return FireVerdict {
            line: format!(
                "failed {} · engine fault: plafond absent après validation — à reporter avec le fichier",
                ctx.label
            ),
            code: exit::FILE,
        };
    };
    let workflow = beat.map_or_else(String::new, |b| b.workflow.clone());
    let before = trace_set(&ctx.project_root);
    let Ok(_room) = enter_room(&ctx.project_root) else {
        // The lock we hold never outlives a door we cannot walk through.
        let _ = ctx.state.release(&ctx.label);
        return FireVerdict {
            line: format!("failed {} · cannot enter the project root", ctx.label),
            code: exit::ENV,
        };
    };
    let code = run_quietly(|| {
        verbs::run::run(
            &workflow,
            false, // json
            None,  // output
            crate::Theme::new(false, true, false),
            RenderMode::Plain, // the piped `nika run` defaults
            false,             // dry_run
            None,              // model_override
            None,              // access_pin
            &[],               // vars
            None,              // resume — NEVER (law 4 · N2)
            false,             // no_trace_file — the trace is load-bearing
            None,              // task_filter
            false,             // no_outputs
            Some(plafond),     // the per-tick ceiling, ALWAYS (law 7)
            false,             // no_gc
            false,             // require_signature — ② (serve) verifies
        )
    });
    let trace = new_trace(&ctx.project_root, &before);
    let _ = ctx.state.release(&ctx.label);
    let (kind, line) = verdict_line(ctx, slot, slots, code, trace.as_deref());
    say_after_journal(ctx, kind, None, slot, slots, trace, code, line)
}

/// The line + the kind for a run that went (rc 0 · 1 · 2 · 3 · 4).
fn verdict_line(
    ctx: &FireCtx,
    slot: &Zoned,
    slots: Option<u32>,
    code: u8,
    trace: Option<&str>,
) -> (FireKind, String) {
    let label = ctx.label.as_str();
    let slot_rfc = slot.timestamp().to_string();
    let catchup = slots.map_or(String::new(), |n| format!(" · rattrapage ×{n}"));
    let via_trace = trace.map_or(String::new(), |t| format!(" · trace {t}"));
    match code {
        exit::OK => (
            FireKind::Fired,
            format!("fired {label} · slot {slot_rfc}{catchup} · exit 0{via_trace}"),
        ),
        exit::PAUSED => (
            FireKind::Paused,
            format!(
                "paused {label} · slot {slot_rfc}{via_trace} — garé (N2: jamais repris, jamais répondu)"
            ),
        ),
        _ => (
            FireKind::Failed,
            format!("failed {label} · slot {slot_rfc} · exit {code}{via_trace}"),
        ),
    }
}

/// Journal the decision, then the line. A fire without its record is a
/// fire that re-fires — a record failure is said, never swallowed.
#[allow(clippy::too_many_arguments)] // the decision's full facts
fn say_after_journal(
    ctx: &FireCtx,
    kind: FireKind,
    reason: Option<String>,
    slot: &Zoned,
    slots: Option<u32>,
    trace: Option<String>,
    code: u8,
    line: String,
) -> FireVerdict {
    let entry = HistoryEntry {
        slot: Some(slot.timestamp()),
        decided_at: ctx.now.timestamp(),
        kind,
        reason,
        trace,
        exit: Some(code),
        slots,
    };
    if let Some(verdict) = write_record(ctx, &entry) {
        return verdict;
    }
    FireVerdict { line, code }
}

/// The record write — `Some(verdict)` when it failed (the failure
/// line REPLACES the decision's: an unrecorded fire re-fires).
fn write_record(ctx: &FireCtx, entry: &HistoryEntry) -> Option<FireVerdict> {
    ctx.state
        .record(&ctx.label, entry)
        .err()
        .map(|e| FireVerdict {
            line: format!("failed {} · the record refused: {e}", ctx.label),
            code: exit::ENV,
        })
}

/// The beat at the context's index (validated: always present).
fn beat_of(ctx: &FireCtx) -> Option<&Beat> {
    ctx.registry.beats().nth(ctx.index)
}

/// The trace files present under the project (`.nika/traces/*.ndjson`).
fn trace_set(root: &Path) -> Vec<String> {
    let dir = root.join(nika_dap::store::TRACE_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .filter_map(std::result::Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".ndjson"))
        .collect()
}

/// The trace THIS run wrote: the `.ndjson` present now that was not
/// there before (several = the alphabetically last — the freshest
/// timestamp wins the name).
fn new_trace(root: &Path, before: &[String]) -> Option<String> {
    let mut fresh: Vec<String> = trace_set(root)
        .into_iter()
        .filter(|n| !before.contains(n))
        .collect();
    fresh.sort_unstable();
    fresh
        .last()
        .map(|name| format!("{}/{name}", nika_dap::store::TRACE_DIR))
}

/// Enter the project root for the run's duration — the fold's trace
/// sink (`.nika/traces/`) and the workflow's relative paths read the
/// CWD (the `try` verb's rehearsal-room precedent).
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

/// The run behind the stdout guard (unix) — a guard failure degrades
/// to the unguarded run (the line still prints; the fold may precede
/// it), never blocks the shot.
#[cfg(unix)]
fn run_quietly(f: impl FnOnce() -> u8) -> u8 {
    match StdoutGuard::enter() {
        Ok(_guard) => f(),
        Err(_) => f(),
    }
}

/// No fd surface off-unix: the run speaks (the ship targets are unix).
#[cfg(not(unix))]
fn run_quietly(f: impl FnOnce() -> u8) -> u8 {
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
