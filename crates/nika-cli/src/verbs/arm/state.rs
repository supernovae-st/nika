// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The firing sidecar (D3) — the state the FILE never carries:
//!
//! ```text
//! .nika/arm/<label>/lock           { "pid": u32, "started_at": RFC3339 }
//! .nika/arm/<label>/last.json      { "slot": RFC3339, "fired_at": RFC3339,
//!                                    "trace": path|null, "exit": u8,
//!                                    "kind": "fired|skipped|paused|failed" }
//! .nika/arm/<label>/history.ndjson one line per decision · append-only
//! ```
//!
//! It lives NEXT TO the traces, at the root of the project that arms
//! the beats (the directory holding `nika.yaml`) — never in the YAML
//! (what changes by itself is never written in what a human re-reads).
//!
//! The lock's owner must be ALIVE to hold: a lock whose pid answers
//! signal 0 is a running tick (law ⑥ governs it); a dead pid's lock is
//! a crash remnant, taken over. The takeover assumes ONE firer per beat
//! (D2 — launchd today, `serve` at ②): two racers re-reading the same
//! stale lock resolve through the atomic `create_new`, the loser
//! re-judges the winner's live pid.
//!
//! N2 rides the read half: a missing OR corrupt `last.json` reads as
//! « never fired » — the planner then owes the on-time window alone and
//! invents no backlog, which is the safe failure direction (at most one
//! on-time re-fire, never a catch-up storm).

use std::io;
use std::path::{Path, PathBuf};

use jiff::{Timestamp, Zoned};

/// The sidecar root below a project directory (D3 — next to the traces).
const ARM_DIR: &str = ".nika/arm";

/// The firing state, rooted at `<project>/.nika/arm`.
pub struct ArmState {
    root: PathBuf,
}

/// What a lock attempt found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockOutcome {
    /// No live owner — the lock is ours.
    Acquired,
    /// A LIVE process holds it (signal 0 answered) — law ⑥ governs.
    HeldAlive {
        /// The holding process.
        pid: u32,
    },
    /// The holder's pid is dead (a crash remnant) — taken over.
    StaleTaken {
        /// The dead pid, when the remnant parsed.
        old_pid: Option<u32>,
    },
}

/// The decision kinds — the `kind:` vocabulary of `last.json` and
/// `history.ndjson`, and the firer's one-line prefixes (D8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireKind {
    /// The run went and exited clean.
    Fired,
    /// A policy said no (missed · overlap · already · inactive · …).
    Skipped,
    /// A human gate paused the run — PARKED with its trace (law N2:
    /// never resumed, never answered by the firer).
    Paused,
    /// The run went and failed (or refused its own file).
    Failed,
}

impl FireKind {
    /// The wire word (JSON + the one-line prefix).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fired => "fired",
            Self::Skipped => "skipped",
            Self::Paused => "paused",
            Self::Failed => "failed",
        }
    }
}

/// One decision, journaled. `slot` is `None` only for the pre-slot
/// skips (inactive · cloud · expired · webhook) — those journal the
/// decision but leave `last.json` untouched (its `slot` is not
/// nullable). `slots` carries the silence's count when
/// `rattraper-une-fois` fires ONE run for n slots.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// The slot decided (the absolute instant — zone-free on the wire).
    pub slot: Option<Timestamp>,
    /// The decision instant (the injected clock — D5).
    pub decided_at: Timestamp,
    /// What was decided.
    pub kind: FireKind,
    /// The skip's reason (`missed:3` · `overlap` · `cloud` · …).
    pub reason: Option<String>,
    /// The run's trace path, repo-relative (`fired` · `paused` · `failed`).
    pub trace: Option<String>,
    /// The run's exit code (`fired` 0 · `failed` 1|2|3 · `paused` 4).
    pub exit: Option<u8>,
    /// `rattraper-une-fois`: how many slots the one fire answers for.
    pub slots: Option<u32>,
}

/// The parsed `last.json` — the report (PROUVE) and the planner read it.
#[derive(Debug, Clone)]
pub struct LastRecord {
    /// The last DECIDED slot (a skip consumes its slot too).
    pub slot: Timestamp,
    /// The decision instant.
    pub fired_at: Timestamp,
    /// The run's trace path when the decision ran something.
    pub trace: Option<String>,
    /// The run's exit code when it ran.
    pub exit: Option<u8>,
    /// The decision kind.
    pub kind: FireKind,
}

impl ArmState {
    /// The sidecar of the project rooted at `project_dir` (D3).
    #[must_use]
    pub fn at_project(project_dir: &Path) -> Self {
        Self {
            root: project_dir.join(ARM_DIR),
        }
    }

    /// The sidecar root (the report's orphan walk reads it).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The last DECIDED slot as the planner's `last_fired` — strictly
    /// after it, a slot is due again. A skip/park consumes its slot:
    /// « fired » here means « decided ». Missing or corrupt state reads
    /// as never-fired (N2 — the on-time window alone then decides).
    #[must_use]
    pub fn last_fired(&self, label: &str) -> Option<Zoned> {
        self.last(label)
            .map(|r| r.slot.to_zoned(jiff::tz::TimeZone::UTC))
    }

    /// The parsed `last.json` of one beat — `None` when absent or
    /// unreadable (N2's safe direction, see the module doc).
    #[must_use]
    pub fn last(&self, label: &str) -> Option<LastRecord> {
        let text = std::fs::read_to_string(self.root.join(label).join("last.json")).ok()?;
        let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
        let slot: Timestamp = doc.get("slot")?.as_str()?.parse().ok()?;
        let fired_at: Timestamp = doc.get("fired_at")?.as_str()?.parse().ok()?;
        let kind = match doc.get("kind")?.as_str()? {
            "fired" => FireKind::Fired,
            "skipped" => FireKind::Skipped,
            "paused" => FireKind::Paused,
            "failed" => FireKind::Failed,
            _ => return None,
        };
        Some(LastRecord {
            slot,
            fired_at,
            trace: doc
                .get("trace")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            exit: doc
                .get("exit")
                .and_then(serde_json::Value::as_u64)
                .and_then(|e| u8::try_from(e).ok()),
            kind,
        })
    }

    /// The history's skip/fire tallies (`x sauts / y tirs`) — `None`
    /// when no history exists at all (the report then says nothing).
    #[must_use]
    pub fn tallies(&self, label: &str) -> Option<(usize, usize)> {
        let text = std::fs::read_to_string(self.root.join(label).join("history.ndjson")).ok()?;
        let mut skips = 0usize;
        let mut fires = 0usize;
        for line in text.lines() {
            let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            match doc.get("kind").and_then(serde_json::Value::as_str) {
                Some("skipped") => skips += 1,
                Some("fired") => fires += 1,
                _ => {}
            }
        }
        Some((skips, fires))
    }

    /// The sidecar directories that name NO known label (law N4:
    /// reported, NEVER erased). `known` carries the registry's labels.
    #[must_use]
    pub fn orphans(&self, known: &[String]) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut out: Vec<String> = entries
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| !known.contains(name))
            .collect();
        out.sort_unstable();
        out
    }

    /// Try the beat's lock. A live holder answers signal 0 — `HeldAlive`
    /// (law ⑥ then governs: sauter · file). A dead holder's file is a
    /// crash remnant — taken over (`StaleTaken`). No file: `Acquired`.
    ///
    /// # Errors
    /// I/O on the sidecar directory (unwritable project, …).
    pub fn try_lock(&self, label: &str, pid: u32, now: &Zoned) -> io::Result<LockOutcome> {
        const MAX_PASSES: u32 = 8;
        let dir = self.dir(label)?;
        let lock = dir.join("lock");
        let body = format!("{{\"pid\":{pid},\"started_at\":\"{}\"}}\n", now.timestamp());
        // The remnant WE removed on the way in, if any (Some(None) =
        // the remnant was unparseable) — it rides the StaleTaken
        // verdict once the atomic create lands.
        let mut removed: Option<Option<u32>> = None;
        let mut passes = 0u32;
        loop {
            passes += 1;
            if passes > MAX_PASSES {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "arm lock: contended past the pass bound",
                ));
            }
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock)
            {
                Ok(mut f) => {
                    use std::io::Write as _;
                    f.write_all(body.as_bytes())?;
                    return Ok(match removed {
                        Some(old_pid) => LockOutcome::StaleTaken { old_pid },
                        None => LockOutcome::Acquired,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
            let old_pid = std::fs::read_to_string(&lock)
                .ok()
                .and_then(|text| lock_pid(&text));
            if let Some(old) = old_pid
                && owner_alive(old)
            {
                return Ok(LockOutcome::HeldAlive { pid: old });
            }
            // The holder is dead (or the file unparseable — either way
            // no LIVE owner is proven): remove the remnant, then the
            // atomic create decides between us and a racer — a racer's
            // win shows up as a live pid on the next pass.
            match std::fs::remove_file(&lock) {
                Ok(()) => removed = Some(old_pid),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
    }

    /// Release the beat's lock. Idempotent: an absent lock is released.
    ///
    /// # Errors
    /// I/O other than `NotFound`.
    pub fn release(&self, label: &str) -> io::Result<()> {
        match std::fs::remove_file(self.root.join(label).join("lock")) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Journal one decision: append the history line, and when the
    /// decision bears a slot, refresh `last.json` (both writes atomic —
    /// tmp + rename for the rewrite, `O_APPEND` for the journal).
    ///
    /// # Errors
    /// I/O on the sidecar (the firer fails the decision loudly then —
    /// a fire without its record is a fire that re-fires).
    pub fn record(&self, label: &str, entry: &HistoryEntry) -> io::Result<()> {
        let dir = self.dir(label)?;
        let line = history_line(entry);
        append_line(&dir.join("history.ndjson"), &line)?;
        if entry.slot.is_some() {
            write_atomic(&dir.join("last.json"), &last_json(entry))?;
        }
        Ok(())
    }

    /// The beat's directory, created on demand.
    fn dir(&self, label: &str) -> io::Result<PathBuf> {
        let dir = self.root.join(label);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

/// The `last.json` document — the locked shape (D3).
fn last_json(entry: &HistoryEntry) -> String {
    let slot = entry.slot.map_or(String::new(), |s| s.to_string());
    let trace = entry
        .trace
        .as_deref()
        .map_or("null".to_owned(), |t| format!("\"{t}\""));
    let exit = entry.exit.unwrap_or(0);
    format!(
        "{{\"slot\":\"{slot}\",\"fired_at\":\"{}\",\"trace\":{trace},\"exit\":{exit},\"kind\":\"{}\"}}\n",
        entry.decided_at,
        entry.kind.as_str()
    )
}

/// One history line — the same fields, plus the skip's reason and the
/// catch-up count when they exist.
fn history_line(entry: &HistoryEntry) -> String {
    let slot = entry.slot.map_or("null".to_owned(), |s| format!("\"{s}\""));
    let reason = entry
        .reason
        .as_deref()
        .map_or("null".to_owned(), |r| format!("\"{r}\""));
    let trace = entry
        .trace
        .as_deref()
        .map_or("null".to_owned(), |t| format!("\"{t}\""));
    let exit = entry.exit.map_or("null".to_owned(), |e| e.to_string());
    let slots = entry.slots.map_or("null".to_owned(), |s| s.to_string());
    format!(
        "{{\"slot\":{slot},\"decided_at\":\"{}\",\"kind\":\"{}\",\"reason\":{reason},\"trace\":{trace},\"exit\":{exit},\"slots\":{slots}}}",
        entry.decided_at,
        entry.kind.as_str()
    )
}

/// The pid out of a lock file — `None` when the file does not parse
/// (an unparseable lock proves no live owner: taken over as stale).
fn lock_pid(text: &str) -> Option<u32> {
    let doc: serde_json::Value = serde_json::from_str(text).ok()?;
    doc.get("pid")?.as_u64().and_then(|p| u32::try_from(p).ok())
}

/// Does a process with this pid exist? Signal 0 probes without
/// delivering. `EPERM` is an owner too — a process we may not signal is
/// still a LIVE holder (stealing its lock would double-fire).
#[cfg(unix)]
fn owner_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    let pid = Pid::from_raw(i32::try_from(pid).unwrap_or(-1));
    // Ok = the process answered · EPERM = a process exists we may not
    // signal — both are a LIVE holder; ESRCH (and the rest) = dead.
    matches!(kill(pid, None), Ok(()) | Err(nix::errno::Errno::EPERM))
}

/// The non-unix fallback: no signal surface, so every holder reads
/// alive — the conservative direction (never steal, never double-fire).
#[cfg(not(unix))]
fn owner_alive(_pid: u32) -> bool {
    true
}

/// Rewrite a file atomically: write a sibling tmp, rename it over.
fn write_atomic(path: &Path, body: &str) -> io::Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("state");
    let tmp = path.with_file_name(format!("{name}.tmp-{}", std::process::id()));
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path)
}

/// One line onto the journal (`O_APPEND` — several firers never tear a
/// line, each append landing whole).
fn append_line(path: &Path, line: &str) -> io::Result<()> {
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn state(tag: &str) -> (tempfile::TempDir, ArmState) {
        let dir = tempfile::Builder::new()
            .prefix(&format!("nika-arm-state-{tag}-"))
            .tempdir()
            .expect("tmp dir");
        let state = ArmState::at_project(dir.path());
        (dir, state)
    }

    fn at(text: &str) -> Zoned {
        text.parse::<Timestamp>()
            .expect("ts")
            .to_zoned(jiff::tz::TimeZone::UTC)
    }

    fn entry(kind: FireKind) -> HistoryEntry {
        HistoryEntry {
            slot: Some("2026-08-19T03:00:00Z".parse::<Timestamp>().expect("ts")),
            decided_at: "2026-08-19T03:02:00Z".parse::<Timestamp>().expect("ts"),
            kind,
            reason: None,
            trace: None,
            exit: Some(0),
            slots: None,
        }
    }

    /// (a) No state reads as never-fired (N2); the recorded slot reads
    /// back as the planner's `last_fired`.
    #[test]
    fn last_fired_is_none_then_the_recorded_slot() {
        let (_dir, state) = state("last");
        assert!(state.last_fired("doctor").is_none(), "N2: no state");
        state
            .record("doctor", &entry(FireKind::Fired))
            .expect("record");
        let fired = state.last_fired("doctor").expect("the recorded slot");
        let expected: Timestamp = "2026-08-19T03:00:00Z".parse().expect("ts");
        assert_eq!(fired.timestamp(), expected);
    }

    /// (b) A lock whose pid is THIS process is a LIVE holder.
    #[test]
    fn a_lock_held_by_a_living_owner_refuses_the_takeover() {
        let (_dir, state) = state("live");
        let now = at("2026-08-19T03:02:00Z");
        let me = std::process::id();
        let first = state.try_lock("doctor", me, &now).expect("lock");
        assert_eq!(first, LockOutcome::Acquired);
        let second = state
            .try_lock("doctor", me.wrapping_add(1), &now)
            .expect("re-lock");
        assert_eq!(second, LockOutcome::HeldAlive { pid: me });
        state.release("doctor").expect("release");
    }

    /// (c) A lock whose pid cannot exist (999999 dies past macOS's and
    /// Linux's `pid_max`) is a crash remnant — taken over.
    #[test]
    fn a_dead_owner_s_lock_is_taken_over() {
        let (_dir, state) = state("stale");
        let now = at("2026-08-19T03:02:00Z");
        let dir = state.root().join("doctor");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(
            dir.join("lock"),
            "{\"pid\":999999,\"started_at\":\"2026-08-19T03:00:00Z\"}\n",
        )
        .expect("remnant");
        let outcome = state
            .try_lock("doctor", std::process::id(), &now)
            .expect("lock");
        assert_eq!(
            outcome,
            LockOutcome::StaleTaken {
                old_pid: Some(999_999)
            }
        );
        // The takeover rewrote the file with OUR live pid.
        let body = std::fs::read_to_string(dir.join("lock")).expect("lock body");
        assert!(body.contains(&std::process::id().to_string()), "{body}");
        state.release("doctor").expect("release");
    }

    /// An unparseable lock proves no live owner — stale, taken over.
    #[test]
    fn a_corrupt_lock_is_taken_over() {
        let (_dir, state) = state("corrupt");
        let now = at("2026-08-19T03:02:00Z");
        let dir = state.root().join("doctor");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("lock"), "not json\n").expect("remnant");
        let outcome = state
            .try_lock("doctor", std::process::id(), &now)
            .expect("lock");
        assert!(
            matches!(outcome, LockOutcome::StaleTaken { .. }),
            "{outcome:?}"
        );
        state.release("doctor").expect("release");
    }

    /// (d) Two decisions = two history lines, append-only.
    #[test]
    fn two_records_append_two_history_lines() {
        let (_dir, state) = state("hist");
        let mut skipped = entry(FireKind::Skipped);
        skipped.reason = Some("missed:1".to_owned());
        state
            .record("doctor", &entry(FireKind::Fired))
            .expect("one");
        state.record("doctor", &skipped).expect("two");
        let text =
            std::fs::read_to_string(state.root().join("doctor/history.ndjson")).expect("history");
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text.contains("\"kind\":\"fired\""), "{text}");
        assert!(text.contains("\"reason\":\"missed:1\""), "{text}");
        // The tallies the report prints ride the same journal.
        assert_eq!(state.tallies("doctor"), Some((1, 1)));
        // … and last.json carries the LAST decision.
        let last = state.last("doctor").expect("last.json");
        assert_eq!(last.kind, FireKind::Skipped);
    }

    /// The orphan walk names sidecar dirs no registry knows — N4:
    /// reported, never erased.
    #[test]
    fn a_sidecar_dir_without_a_registry_entry_is_an_orphan() {
        let (_dir, state) = state("orphan");
        state
            .record("doctor", &entry(FireKind::Fired))
            .expect("rec");
        state.record("ghost", &entry(FireKind::Fired)).expect("rec");
        let orphans = state.orphans(&["doctor".to_owned()]);
        assert_eq!(orphans, vec!["ghost".to_owned()]);
        // … and the orphan's record DEMEURE (the walk never writes).
        assert!(state.root().join("ghost/last.json").exists());
    }

    /// Release is idempotent — an absent lock is released.
    #[test]
    fn release_without_a_lock_is_ok() {
        let (_dir, state) = state("rel");
        state.release("doctor").expect("idempotent");
    }
}
