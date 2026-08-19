// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika serve` — LE TIREUR RÉSIDENT (②) · the same firer, resident.
//!
//! Where no launchd/systemd answers (a container · a VPS · the cloud
//! ③), the ONE firer of `arm fire` (W2) lives long: this loop reads the
//! clock, asks the planner what is due, and hands each named beat to
//! `fire_beat` — the on-time window, the miss policy, the overlap lock,
//! the per-tick ceiling and the record live there exactly once, and
//! serve adds no second law. A beat the planner does not name prints
//! nothing: the resident log is the fires, not the silence.
//!
//! The loop law (plan §W5 — one law, two edges):
//!
//! - **The clock is the edge** (D5) — production reads `Zoned::now`;
//!   the hidden `--now` injects the start of a deterministic replay
//!   whose waits ADVANCE the scripted instant instead of sleeping, and
//!   `--until` bounds the replay (checked when the clock is read, BEFORE
//!   anything fires). A scripted loop with no bound refuses at the edge:
//!   it would spin, never serve.
//! - **The file proposes — re-read, never cached** — `nika.yaml` is
//!   re-parsed and re-validated whenever its mtime moves. A reload that
//!   refuses (the grammar's law) POISONS the served set until the file
//!   reads again: a beat the operator has just disarmed must never fire
//!   from memory, and the refusal is said (stderr).
//! - **Server exit convention** — `0` on a clean stop (`--once` done ·
//!   `--until` reached · SIGINT/SIGTERM — the fire in flight, which is
//!   synchronous, finishes first), `1` on serve's own fault (no project
//!   · a registry refusing at boot · a bad clock). A beat's own failure
//!   is recorded in its history and never moves serve's exit: the
//!   daemon is healthy, the beat is not — `nika arm` reads it.
//!
//! Input trust (Gate 1 · P0): serve reads ONLY `nika.yaml` (judged by
//! the vocab's shape and the cadence grammar BEFORE any firing) and its
//! own `.nika/arm/` sidecar — no socket, no port, no network read, no
//! external argument. `--once`/`--dry` are the whole public surface
//! (`--now`/`--until` stay hidden replay hooks). Who fires a beat from
//! a distance (`serve_tokens`) is NOT v0 — the cloud (③) will carry it;
//! ADR-116 records the three answers (`max_retries` · the state enum ·
//! `serve_tokens`).

#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]
// The resident firer prints the firer's ONE line per decision to stdout
// (D8's machine surface) and its own lifecycle to stderr — the same
// carve-out class as the run verb's fold (verbs/run/mod.rs).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jiff::{Span, Timestamp, Zoned};
use nika_cadence::registry::ArmRegistry;
use nika_vocab::project;

use super::arm::fire::{self, FireCtx};
use super::arm::state::ArmState;
use super::exit;

/// The longest the loop ever waits between two looks at the file — the
/// re-read law needs a bounded horizon even when the next slot is hours
/// out (a moved `nika.yaml` is picked up within the minute).
const SLEEP_CAP_S: i64 = 60;

/// `nika serve` — the resident firer's args.
#[derive(Debug, clap::Args)]
pub struct ServeArgs {
    /// Fire what is due once, then exit — the doctor's probe.
    #[arg(long)]
    pub once: bool,
    /// Say what WOULD fire — the REAL decision per due beat, printed;
    /// nothing locked, nothing run, nothing recorded.
    #[arg(long)]
    pub dry: bool,
    /// Inject the clock's start (RFC 3339) instead of the wall — D5: a
    /// replay is deterministic. With `--now` the waits ADVANCE the
    /// scripted instant instead of sleeping, so the loop then needs a
    /// bound (`--once` or `--until`).
    #[arg(long, hide = true, value_name = "RFC3339")]
    pub now: Option<String>,
    /// Stop the loop once the clock reaches this instant (RFC 3339) —
    /// the replay's bound.
    #[arg(long, hide = true, value_name = "RFC3339")]
    pub until: Option<String>,
}

/// How a wait ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wake {
    /// The span elapsed.
    Elapsed,
    /// SIGINT/SIGTERM landed — the loop stops clean.
    Signaled,
}

/// Why the loop stopped (the report and the tests read it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    /// `--once`: one pass done.
    Once,
    /// `--until`: the clock reached the bound.
    Until,
    /// SIGINT/SIGTERM landed.
    Signaled,
}

/// What a re-read found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reread {
    /// The file did not move.
    Unmoved,
    /// The file moved and reads.
    Reloaded,
    /// The file moved and refuses.
    Refused,
}

/// What the loop did. The unit tests read it — their registries arm
/// cloud-only beats, so nothing ever fires in-process (the room the
/// firer enters stays the suite's own CWD, never a tempdir's).
#[derive(Debug)]
struct LoopReport {
    /// Passes over the registry.
    passes: usize,
    /// Verdict lines printed (fires · skips · dry would-dos).
    decisions: usize,
    /// Successful re-reads after boot (the file moved and reads).
    reloads: usize,
    /// Refused re-reads (the file moved and broke — the set poisons).
    refusals: usize,
    /// How it ended.
    stop: Stop,
}

/// The loop's clock and wait (the plan's two doors): production reads
/// the wall and waits in tokio (SIGINT/SIGTERM outrun the timer); a
/// replay holds a scripted instant the waits advance. `sleep` is
/// `FnMut`: tokio's SIGTERM stream borrows mutably across waits.
struct Clock {
    /// Read the current instant.
    now: Box<dyn Fn() -> Zoned>,
    /// Wait one span (or until a signal) — a replay advances instead.
    sleep: Box<dyn FnMut(Span) -> Wake>,
}

/// What the loop serves right now: the registry (`None` only while the
/// firer holds it for one pass), its labels, the file's mtime at the
/// last read, and the poison flag.
struct Served {
    /// The parsed + validated registry.
    registry: Option<ArmRegistry>,
    /// The beat labels, in file order (the W2 firer's own derivation).
    names: Vec<String>,
    /// The file's mtime at the last read — the re-read trigger.
    mtime: Option<std::time::SystemTime>,
    /// The file moved and refused: NOTHING is served until it reads
    /// again (a beat never fires from memory).
    poisoned: bool,
}

impl Served {
    /// Load what the file proposes right now (both judges: the parse,
    /// then the law) + the labels + the mtime the trigger compares.
    fn load(path: &Path) -> Result<Self, String> {
        let registry = load_registry(path)?;
        Ok(Self {
            names: fire::labels(&registry),
            registry: Some(registry),
            mtime: mtime_of(path),
            poisoned: false,
        })
    }

    /// Re-read the file when its mtime moved — « le fichier propose »,
    /// never cached. A refusal poisons the served set until the file
    /// reads again, and is said on stderr.
    fn reread(&mut self, path: &Path) -> Reread {
        let seen = mtime_of(path);
        if seen == self.mtime {
            return Reread::Unmoved;
        }
        self.mtime = seen;
        match load_registry(path) {
            Ok(registry) => {
                self.names = fire::labels(&registry);
                self.registry = Some(registry);
                self.poisoned = false;
                eprintln!(
                    "serve · registry re-read — {}",
                    crate::text::count(self.names.len(), "beat")
                );
                Reread::Reloaded
            }
            Err(line) => {
                self.poisoned = true;
                eprintln!("serve · reload refused — nothing served until the file reads\n{line}");
                Reread::Refused
            }
        }
    }
}

/// `nika serve` — the verb edge: discover the project, load the
/// registry, build the clock, loop. Server convention: `0` clean stop ·
/// `1` serve's own fault.
#[must_use]
pub fn run(args: &ServeArgs) -> u8 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let found = match project::discover(&cwd) {
        Ok(found) => found,
        Err(e) => {
            eprintln!("serve ✗  {e}");
            return 1;
        }
    };
    let Some((path, _project)) = found else {
        eprintln!(
            "serve · nothing armed — this project has no `nika.yaml`\n  \
             fix: `nika init --project-file` lays a commented starter"
        );
        return 1;
    };
    let mut served = match Served::load(&path) {
        Ok(served) => served,
        Err(line) => {
            eprintln!("{line}");
            return 1;
        }
    };
    let (start, until) = match hooks(args) {
        Ok(hooks) => hooks,
        Err(line) => {
            eprintln!("{line}");
            return 1;
        }
    };
    let mut clock = match start {
        Some(start) => scripted(start),
        None => match production() {
            Ok(clock) => clock,
            Err(line) => {
                eprintln!("{line}");
                return 1;
            }
        },
    };
    let root = path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let now = (clock.now)();
    eprintln!("{}", boot_line(&served, &now));
    let report = serve_loop(
        &root,
        &path,
        &mut served,
        args,
        until.map(|z| z.timestamp()),
        &mut clock,
    );
    eprintln!("{}", stop_line(&report));
    exit::OK
}

/// The two hidden clock hooks, parsed (D5) — and the replay's law: a
/// scripted clock with no bound would spin forever, so `--now` wants
/// `--once` or `--until` beside it.
fn hooks(args: &ServeArgs) -> Result<(Option<Zoned>, Option<Zoned>), String> {
    let start = parse_hook("--now", args.now.as_deref())?;
    let until = parse_hook("--until", args.until.as_deref())?;
    if start.is_some() && !args.once && until.is_none() {
        return Err(
            "serve · --now without --once or --until would spin forever — a replay needs a bound"
                .to_owned(),
        );
    }
    Ok((start, until))
}

/// The loop (plan §W5): read the clock · stop at the bound BEFORE
/// anything fires · re-read the file when it moved · fire what is due ·
/// wait for the next slot, capped — and SIGINT/SIGTERM stop it clean.
/// Owns no clock and no wait itself: both arrive behind the [`Clock`]
/// doors, so the replay and production run the SAME body.
fn serve_loop(
    root: &Path,
    path: &Path,
    served: &mut Served,
    args: &ServeArgs,
    until: Option<Timestamp>,
    clock: &mut Clock,
) -> LoopReport {
    let (mut passes, mut decisions, mut reloads, mut refusals) = (0, 0, 0, 0);
    let stop = 'passes: loop {
        passes += 1;
        let now = (clock.now)();
        if until.is_some_and(|bound| now.timestamp() >= bound) {
            break 'passes Stop::Until;
        }
        match served.reread(path) {
            Reread::Unmoved => {}
            Reread::Reloaded => reloads += 1,
            Reread::Refused => refusals += 1,
        }
        decisions += fire_pass(root, served, &now, args.dry);
        if args.once {
            break 'passes Stop::Once;
        }
        if let Wake::Signaled = (clock.sleep)(wait_span(served, &now)) {
            break 'passes Stop::Signaled;
        }
    };
    LoopReport {
        passes,
        decisions,
        reloads,
        refusals,
        stop,
    }
}

/// One pass over what the planner names: `due` pre-filters (the
/// resident log is the fires, not the silence), then each named beat
/// goes through the W2 firer's own decision — rehearsed (`--dry`) or
/// fired. Returns how many verdict lines were printed.
fn fire_pass(root: &Path, served: &mut Served, now: &Zoned, dry: bool) -> usize {
    if served.poisoned {
        return 0;
    }
    let Some(registry) = served.registry.as_ref() else {
        return 0;
    };
    let side = ArmState::at_project(root);
    let last_of = |i: usize| served.names.get(i).and_then(|label| side.last_fired(label));
    let dues = match nika_cadence::due(registry, now, &last_of) {
        Ok(dues) => dues,
        // validate ran at load — a cadence refusing here is an ENGINE
        // fault, said as such (the two-readers law); the pass serves
        // nothing rather than approximate.
        Err(e) => {
            eprintln!("serve · engine fault: a validated registry refuses — {e}");
            return 0;
        }
    };
    let indices: Vec<usize> = dues.map(|d| d.index).collect();
    if indices.is_empty() {
        return 0;
    }
    if dry {
        return dry_lines(served, &side, now, &indices);
    }
    fire_lines(root, served, now, &indices)
}

/// The rehearsal: the REAL decision per due beat, printed — nothing
/// locked, nothing run, nothing recorded.
fn dry_lines(served: &Served, side: &ArmState, now: &Zoned, indices: &[usize]) -> usize {
    let mut printed = 0;
    let Some(registry) = served.registry.as_ref() else {
        return 0;
    };
    for &index in indices {
        let Some(label) = served.names.get(index) else {
            continue;
        };
        let last = side.last_fired(label);
        let line = match fire::decide(registry, index, label, now, last.as_ref()) {
            fire::Decision::Fire { slot, slots } => {
                let catchup = slots.map_or(String::new(), |n| format!(" · rattrapage ×{n}"));
                format!(
                    "dry {label} · slot {} · would fire{catchup}",
                    slot.timestamp()
                )
            }
            fire::Decision::Skip { reason, .. } => {
                format!("dry {label} · would skip · {reason}")
            }
            fire::Decision::Refuse { line } => format!("dry {label} · would refuse — {line}"),
        };
        println!("{line}");
        printed += 1;
    }
    printed
}

/// The wet pass: the firer OWNS the registry for the pass's duration
/// (its context moves it in; the pass moves it back out), and every
/// verdict line is the firer's own (D8).
fn fire_lines(root: &Path, served: &mut Served, now: &Zoned, indices: &[usize]) -> usize {
    let mut printed = 0;
    let Some(registry) = served.registry.take() else {
        return 0;
    };
    let mut ctx = FireCtx {
        project_root: root.to_path_buf(),
        registry,
        index: 0,
        label: String::new(),
        now: now.clone(),
        state: ArmState::at_project(root),
    };
    for &index in indices {
        let Some(label) = served.names.get(index) else {
            continue;
        };
        ctx.index = index;
        ctx.label.clone_from(label);
        let verdict = fire::fire_beat(&ctx);
        println!("{}", verdict.line);
        printed += 1;
    }
    let FireCtx { registry, .. } = ctx;
    served.registry = Some(registry);
    printed
}

/// How long to wait: the gap to the earliest next slot, floored at one
/// tick (a slot AT now never busy-spins the loop) and capped at
/// [`SLEEP_CAP_S`] so a moved file is re-read within the minute. Nothing
/// armed (or a poisoned set) waits the cap.
fn wait_span(served: &Served, now: &Zoned) -> Span {
    let registry = if served.poisoned {
        None
    } else {
        served.registry.as_ref()
    };
    let next = registry.and_then(|reg| match nika_cadence::earliest_next(reg, now) {
        Ok(next) => next,
        Err(e) => {
            eprintln!("serve · engine fault: a validated registry refuses — {e}");
            None
        }
    });
    let gap = next.map_or(SLEEP_CAP_S, |(_i, slot)| {
        slot.at.timestamp().as_second() - now.timestamp().as_second()
    });
    Span::new().seconds(gap.clamp(1, SLEEP_CAP_S))
}

/// The replay doors (D5): a scripted instant the waits advance — the
/// `VirtualClock` trap (a sleeping loop whose clock never moves spins
/// forever) closed by construction.
fn scripted(start: Zoned) -> Clock {
    let cell = Rc::new(RefCell::new(start));
    let reader = Rc::clone(&cell);
    Clock {
        now: Box::new(move || cell.borrow().clone()),
        sleep: Box::new(move |span: Span| {
            let mut at = reader.borrow_mut();
            match at.checked_add(span) {
                Ok(next) => *at = next,
                // A one-minute step never overflows jiff's range — the
                // unreachable arm stops the replay rather than spin on a
                // frozen clock.
                Err(_) => return Wake::Signaled,
            }
            Wake::Elapsed
        }),
    }
}

/// The production doors: the wall clock, and a wait SIGINT/SIGTERM
/// outrun (tokio's current-thread runtime — the run verb's own shape).
/// The signal streams arm ONCE: a signal landing mid-fire stays pending
/// and is observed at the next wait, so the fire in flight — synchronous
/// — always finishes first.
#[cfg(unix)]
fn production() -> Result<Clock, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("serve · cannot start the timer: {e}"))?;
    // The signal stream's registration needs the reactor's context —
    // create it INSIDE one block_on, then drive it with the waits'.
    let mut term = rt
        .block_on(async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        })
        .map_err(|e| format!("serve · cannot arm SIGTERM: {e}"))?;
    Ok(Clock {
        now: Box::new(Zoned::now),
        sleep: Box::new(move |span: Span| {
            rt.block_on(async {
                tokio::select! {
                    () = tokio::time::sleep(std_duration(span)) => Wake::Elapsed,
                    _ = tokio::signal::ctrl_c() => Wake::Signaled,
                    _ = term.recv() => Wake::Signaled,
                }
            })
        }),
    })
}

/// The production doors off-unix: SIGINT only (the ship targets are
/// unix — the firer's own fd fold makes the same call).
#[cfg(not(unix))]
fn production() -> Result<Clock, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("serve · cannot start the timer: {e}"))?;
    Ok(Clock {
        now: Box::new(Zoned::now),
        sleep: Box::new(move |span: Span| {
            rt.block_on(async {
                tokio::select! {
                    () = tokio::time::sleep(std_duration(span)) => Wake::Elapsed,
                    _ = tokio::signal::ctrl_c() => Wake::Signaled,
                }
            })
        }),
    })
}

/// The wait as std's duration (absolute ticks; a negative or
/// overflowing span waits zero — the next pass re-judges).
fn std_duration(span: Span) -> std::time::Duration {
    std::time::Duration::from_secs(u64::try_from(span.get_seconds()).unwrap_or(0))
}

/// Read + parse + validate the registry — both judges (the grammar,
/// then the law) BEFORE any firing. The refusal's voice is `arm`'s own.
fn load_registry(path: &Path) -> Result<ArmRegistry, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("serve · cannot read {}: {e}", path.display()))?;
    let registry = nika_cadence::parse_registry(&text).map_err(|e| format!("serve ✗  {e}"))?;
    let faults: Vec<String> = nika_cadence::validate(&registry)
        .map(|e| format!("  {e}"))
        .collect();
    if faults.is_empty() {
        Ok(registry)
    } else {
        Err(format!(
            "serve ✗  {} in {}\n{}",
            crate::text::count(faults.len(), "refusal"),
            path.display(),
            faults.join("\n")
        ))
    }
}

/// The file's mtime — `None` when it cannot be stated (a file deleted
/// mid-serve reads as « moved »: the re-read then refuses and the set
/// poisons).
fn mtime_of(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// One hidden clock hook (RFC 3339) — a bare instant lands on UTC, the
/// zoned form keeps its zone (the `arm fire` parser's own law, D5).
fn parse_hook(flag: &str, raw: Option<&str>) -> Result<Option<Zoned>, String> {
    raw.map(|text| {
        text.parse::<Zoned>()
            .or_else(|_| {
                text.parse::<Timestamp>()
                    .map(|t| t.to_zoned(jiff::tz::TimeZone::UTC))
            })
            .map_err(|_| {
                format!("serve: {flag} `{text}` · RFC 3339 attendu — 2026-08-19T03:02:00Z")
            })
    })
    .transpose()
}

/// The startup line (stderr — the lifecycle never dirties the D8
/// stdout): how many beats serve, and what fires next.
fn boot_line(served: &Served, now: &Zoned) -> String {
    let next = served
        .registry
        .as_ref()
        .and_then(|reg| nika_cadence::earliest_next(reg, now).ok().flatten())
        .map_or_else(
            || "—".to_owned(),
            |(_i, slot)| slot.at.timestamp().to_string(),
        );
    format!(
        "serve · {} · next {next}",
        crate::text::count(served.names.len(), "beat")
    )
}

/// The stop line (stderr): why it ended, and what the loop did.
fn stop_line(report: &LoopReport) -> String {
    let why = match report.stop {
        Stop::Once => "once done",
        Stop::Until => "until reached",
        Stop::Signaled => "signal — clean stop",
    };
    format!(
        "serve · {why} · {} · {} · {} · {}",
        crate::text::count(report.passes, "pass"),
        crate::text::count(report.decisions, "decision"),
        crate::text::count(report.reloads, "reload"),
        crate::text::count(report.refusals, "refused reload"),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn at(text: &str) -> Zoned {
        text.parse::<Timestamp>()
            .expect("ts")
            .to_zoned(jiff::tz::TimeZone::UTC)
    }

    fn serve_args(once: bool, dry: bool) -> ServeArgs {
        ServeArgs {
            once,
            dry,
            now: None,
            until: None,
        }
    }

    /// A registry whose beats the planner NEVER names in-process
    /// (`où: cloud`) — the loop's mechanics tested without the firer
    /// entering a tempdir room (the suite's CWD never moves).
    fn cloud_project(tag: &str, beats: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::Builder::new()
            .prefix(&format!("nika-serve-{tag}-"))
            .tempdir()
            .expect("tmp dir");
        let path = dir.path().join("nika.yaml");
        std::fs::write(&path, format!("nika: v1\narm:\n{beats}")).expect("registry");
        (dir, path)
    }

    const CLOUD_DOCTOR: &str = concat!(
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 3 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
        "    où: cloud\n",
    );

    /// The replay door: a wait ADVANCES the scripted instant.
    #[test]
    fn scripted_waits_advance_the_instant() {
        let mut clock = scripted(at("2026-08-19T03:00:00Z"));
        let wake = (clock.sleep)(Span::new().seconds(60));
        assert_eq!(wake, Wake::Elapsed);
        assert_eq!(
            (clock.now)().timestamp().to_string(),
            "2026-08-19T03:01:00Z"
        );
    }

    /// `--once`: one pass, nothing due (the cloud beat is the planner's
    /// own refusal), stop said.
    #[test]
    fn once_runs_one_pass_and_stops() {
        let (dir, path) = cloud_project("once", CLOUD_DOCTOR);
        let mut served = Served::load(&path).expect("loads");
        let mut clock = scripted(at("2026-08-19T03:02:00Z"));
        let report = serve_loop(
            dir.path(),
            &path,
            &mut served,
            &serve_args(true, false),
            None,
            &mut clock,
        );
        assert_eq!(report.stop, Stop::Once);
        assert_eq!(report.passes, 1);
        assert_eq!(report.decisions, 0);
        assert_eq!(report.reloads, 0);
    }

    /// The bound: the replay stops the first pass the clock reaches it,
    /// before anything fires.
    #[test]
    fn until_bounds_the_replay() {
        let (dir, path) = cloud_project("until", CLOUD_DOCTOR);
        let mut served = Served::load(&path).expect("loads");
        let mut clock = scripted(at("2026-08-19T03:00:00Z"));
        let bound = at("2026-08-19T03:03:00Z").timestamp();
        let report = serve_loop(
            dir.path(),
            &path,
            &mut served,
            &serve_args(false, false),
            Some(bound),
            &mut clock,
        );
        assert_eq!(report.stop, Stop::Until);
        assert_eq!(report.passes, 4, "03:00 · 03:01 · 03:02 · 03:03 stops");
        assert_eq!(report.decisions, 0);
    }

    /// A moved file is re-read; a broken one poisons the served set
    /// until it reads again — and both are counted.
    #[test]
    fn a_moved_file_is_re_read_and_a_broken_one_poisons_the_served_set() {
        let (dir, path) = cloud_project("reload", CLOUD_DOCTOR);
        let mut served = Served::load(&path).expect("loads");
        // The waits carry the edits: the first rewrites the file (two
        // cloud beats), the 2e breaks it — both mid-loop, both moved.
        let two_beats = format!(
            "nika: v1\narm:\n{CLOUD_DOCTOR}{}",
            concat!(
                "  - workflow: workflows/nightly.nika.yaml\n",
                "    cadence: \"TZ=UTC 0 4 * * *\"\n",
                "    plafond: 0.25\n",
                "    manqué: sauter\n",
                "    où: cloud\n",
            )
        );
        let mut base = scripted(at("2026-08-19T03:00:00Z"));
        let mut inner = base.sleep;
        let edited = path.clone();
        let mut writes = 0u32;
        base.sleep = Box::new(move |span| {
            writes += 1;
            match writes {
                1 => std::fs::write(&edited, &two_beats).expect("the v2 write"),
                2 => std::fs::write(&edited, "nika: v1\narm: [broken\n").expect("the broken write"),
                _ => {}
            }
            inner(span)
        });
        let bound = at("2026-08-19T03:06:00Z").timestamp();
        let report = serve_loop(
            dir.path(),
            &path,
            &mut served,
            &serve_args(false, false),
            Some(bound),
            &mut base,
        );
        assert_eq!(report.stop, Stop::Until);
        assert_eq!(report.reloads, 1, "the v2 read");
        assert_eq!(report.refusals, 1, "the broken read");
        assert_eq!(report.decisions, 0, "cloud beats never fire");
        assert!(served.poisoned, "the broken file poisons what is served");
    }

    /// The rehearsal runs the REAL decision and records NOTHING.
    #[test]
    fn dry_rehearses_the_real_decision_and_records_nothing() {
        let (dir, path) = cloud_project(
            "dry",
            concat!(
                "  - workflow: workflows/doctor.nika.yaml\n",
                "    cadence: \"TZ=UTC 0 3 * * *\"\n",
                "    plafond: 0.25\n",
                "    manqué: sauter\n",
            ),
        );
        let mut served = Served::load(&path).expect("loads");
        let mut clock = scripted(at("2026-08-19T03:02:00Z"));
        let report = serve_loop(
            dir.path(),
            &path,
            &mut served,
            &serve_args(true, true),
            None,
            &mut clock,
        );
        assert_eq!(report.decisions, 1, "the on-time beat is due");
        // … and nothing landed: no sidecar, no lock, no record (N2).
        assert!(!dir.path().join(".nika/arm").exists());
    }

    /// The wait: the gap to the next slot, capped at the minute.
    #[test]
    fn the_wait_is_capped_at_the_minute() {
        let (dir, path) = cloud_project(
            "wait",
            concat!(
                "  - workflow: workflows/doctor.nika.yaml\n",
                "    cadence: \"TZ=UTC 0 3 * * *\"\n",
                "    plafond: 0.25\n",
                "    manqué: sauter\n",
            ),
        );
        let served = Served::load(&path).expect("loads");
        let dir_path = dir.path();
        let _ = dir_path;
        // Two hours out → the cap.
        let span = wait_span(&served, &at("2026-08-19T01:00:00Z"));
        assert_eq!(span.get_seconds(), SLEEP_CAP_S);
        // Thirty ticks out → the gap itself.
        let span = wait_span(&served, &at("2026-08-19T02:59:30Z"));
        assert_eq!(span.get_seconds(), 30);
    }

    /// The hidden hooks: bare RFC 3339 lands on UTC, the zoned form
    /// keeps its zone, garbage teaches — and a scripted clock without a
    /// bound refuses.
    #[test]
    fn the_hooks_parse_rfc3339_and_refuse_garbage() {
        let now = parse_hook("--now", Some("2026-08-19T03:02:00Z"))
            .expect("parses")
            .expect("some");
        assert_eq!(now.timestamp().to_string(), "2026-08-19T03:02:00Z");
        let zoned = parse_hook("--now", Some("2026-08-19T05:02:00+02:00[Europe/Paris]"))
            .expect("parses")
            .expect("some");
        assert_eq!(zoned.timestamp().to_string(), "2026-08-19T03:02:00Z");
        assert!(parse_hook("--now", Some("demain")).is_err());
        assert!(parse_hook("--now", None).expect("parses").is_none());

        let mut args = serve_args(false, false);
        args.now = Some("2026-08-19T03:02:00Z".to_owned());
        assert!(hooks(&args).is_err(), "a boundless replay refuses");
        args.until = Some("2026-08-19T04:00:00Z".to_owned());
        assert!(hooks(&args).is_ok(), "the bound answers it");
    }
}
