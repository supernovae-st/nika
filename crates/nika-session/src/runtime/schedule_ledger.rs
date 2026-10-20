// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the arm ledger journals of each schedule `nika.yaml` declares, read for a host (a row of
//! the terminal, an SDK view): how many decisions of each kind, the last slot decided
//! (`last.json`'s projection), and whether this Mac holds the schedule's unit. A read only:
//! nothing is created, locked or repaired. The disarm a « désactive-le » journals is written
//! here too, in the same ledger.

use std::path::{Path, PathBuf};

use nika_arm::state::{ArmState, Decision, FireKind};
use nika_cadence::registry::ArmRegistry;

use super::{Lang, SessionRuntime};

/// One schedule `nika.yaml` declares, and what its arm ledger journals.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScheduleHistory {
    /// The label its ledger is kept under (`.nika/arm/<label>/history.ndjson`): the workflow's
    /// file name without `.nika`, `-2` after a name already taken.
    pub label: String,
    /// The workflow it runs, as `nika.yaml` names it.
    pub workflow: String,
    /// Its cadence as declared (`TZ=… m h dom mon dow`).
    pub cadence: String,
    /// Whether `nika.yaml` keeps it active (`actif` unless written false).
    pub active: bool,
    /// Whether this Mac holds its launchd unit, as the Session's trigger host says (by default
    /// its file in `~/Library/LaunchAgents`). A unit file there is not proof that launchd loaded
    /// it.
    pub unit_installed: bool,
    /// Whether a resident `nika serve` holds this project's server lease now: the other firer
    /// of a declared schedule, beside its unit.
    pub serve_running: bool,
    /// Runs that went and exited cleanly.
    pub fired: usize,
    /// Slots consumed without running: a missed-run policy, an overlap, a suspension.
    pub skipped: usize,
    /// Runs a human gate parked.
    pub paused: usize,
    /// Runs, or their files, that failed.
    pub failed: usize,
    /// Times its unit was torn down.
    pub disarmed: usize,
    /// When its unit was last torn down, RFC 3339, when it was.
    pub last_disarmed_at: Option<String>,
    /// Runs that went and exited cleanly since that disarm (every one, without a disarm): what
    /// proves the trigger installed since then has fired.
    pub fired_since_disarm: usize,
    /// The last fired, skipped, paused or failed slot, when one was journaled.
    pub last: Option<LastDecision>,
    /// Why its ledger could not be read, when it could not: the counts are then zero.
    pub unreadable: Option<String>,
}

/// The last slot-bearing decision of a schedule, as `last.json` projects it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LastDecision {
    /// `fired`, `skipped`, `paused` or `failed`.
    pub kind: &'static str,
    /// The slot it decided, RFC 3339.
    pub slot: String,
    /// When it was decided, RFC 3339.
    pub decided_at: String,
    /// The run's exit code, when it ran.
    pub exit: Option<u8>,
    /// The run's trace, when one was kept.
    pub trace: Option<String>,
    /// Why it skipped (`missed:1`, `overlap`, `inactive`), when it did and the ledger says why.
    pub reason: Option<String>,
    /// How many missed slots one catch-up run answered, when it was one.
    pub slots: Option<u32>,
}

impl ScheduleHistory {
    /// The schedule labelled `label` running `workflow` on `cadence`: active, with no unit, no
    /// resident, nothing journaled.
    #[must_use]
    pub fn new(label: &str, workflow: &str, cadence: &str) -> Self {
        Self {
            label: label.to_owned(),
            workflow: workflow.to_owned(),
            cadence: cadence.to_owned(),
            active: true,
            unit_installed: false,
            serve_running: false,
            fired: 0,
            skipped: 0,
            paused: 0,
            failed: 0,
            disarmed: 0,
            last_disarmed_at: None,
            fired_since_disarm: 0,
            last: None,
            unreadable: None,
        }
    }
}

impl LastDecision {
    /// A `kind` decision of `slot`, decided at `decided_at` (both RFC 3339), with nothing else
    /// said.
    #[must_use]
    pub fn new(kind: &'static str, slot: &str, decided_at: &str) -> Self {
        Self {
            kind,
            slot: slot.to_owned(),
            decided_at: decided_at.to_owned(),
            exit: None,
            trace: None,
            reason: None,
            slots: None,
        }
    }
}

impl SessionRuntime {
    /// Every schedule this project's `nika.yaml` declares, in file order, with what its arm
    /// ledger journals: the count of each decision and the last slot decided, beside whether this
    /// Mac holds its unit. Empty without a project file or a schedule. A read only: nothing is
    /// created, locked or repaired.
    #[must_use]
    pub fn schedule_history(&self) -> Vec<ScheduleHistory> {
        let root = &self.snapshot.root;
        histories(root, &|label| self.trigger_host.installed(root, label))
    }
}

/// The schedules `nika.yaml` declares above `root`, read with the ledger kept beside it, whether
/// a resident serves the project, and whether each unit is installed as `installed` says.
pub(super) fn histories(root: &Path, installed: &dyn Fn(&str) -> bool) -> Vec<ScheduleHistory> {
    let Some((project, registry)) = registry(root) else {
        return Vec::new();
    };
    let state = ArmState::at_project(&project);
    let serve_running = nika_arm::resident::alive(&project);
    let now = jiff::Timestamp::now();
    let labels = nika_cadence::emit::labels(&registry);
    (registry.beats().zip(labels))
        .map(|(beat, label)| {
            let mut history = ScheduleHistory::new(&label, &beat.workflow, &beat.cadence);
            history.active = beat.is_active();
            history.unit_installed = installed(&label);
            history.serve_running = serve_running;
            let seen = match state.inspect(&label, &now) {
                Ok(seen) => seen,
                Err(error) => {
                    history.unreadable = Some(error.to_string());
                    return history;
                }
            };
            let counts = [
                FireKind::Fired,
                FireKind::Skipped,
                FireKind::Paused,
                FireKind::Failed,
                FireKind::Disarmed,
            ]
            .map(|kind| seen.count(kind));
            match counts {
                [
                    Some(fired),
                    Some(skipped),
                    Some(paused),
                    Some(failed),
                    Some(disarmed),
                ] => {
                    (history.fired, history.skipped, history.paused) = (fired, skipped, paused);
                    (history.failed, history.disarmed) = (failed, disarmed);
                }
                _ => history.unreadable = Some("its journal does not count".to_owned()),
            }
            let decisions = seen.decisions().unwrap_or_default();
            let disarm = (decisions.iter()).rposition(|d| d.kind == FireKind::Disarmed);
            history.last_disarmed_at = disarm
                .and_then(|at| decisions.get(at)?.decided_at)
                .map(|at| at.to_string());
            history.fired_since_disarm = (decisions.iter().skip(disarm.map_or(0, |at| at + 1)))
                .filter(|d| d.kind == FireKind::Fired)
                .count();
            // The journal's last slot-bearing decision is the one `last.json` projects.
            let said = (decisions.iter()).rfind(|d| d.kind != FireKind::Disarmed);
            history.last = seen.last().map(|last| {
                let mut decided = LastDecision::new(
                    last.kind.as_str(),
                    &last.slot.to_string(),
                    &last.fired_at.to_string(),
                );
                decided.exit = last.exit;
                decided.trace.clone_from(&last.trace);
                decided.reason = said.and_then(|d: &Decision| d.reason.clone());
                decided.slots = said.and_then(|d| d.slots);
                decided
            });
            history
        })
        .collect()
}

/// The folder of the project file above `root` and the schedules it declares, when it reads.
pub(super) fn registry(root: &Path) -> Option<(PathBuf, ArmRegistry)> {
    let (file, _) = nika_vocab::project::discover(root).ok().flatten()?;
    let text = std::fs::read_to_string(&file).ok()?;
    let registry = nika_cadence::parse::parse_registry(&text).ok()?;
    let project = file
        .parent()
        .map_or_else(|| root.to_path_buf(), Path::to_path_buf);
    Some((project, registry))
}

/// The label the ledger and the unit of the schedule running `workflow` are kept under.
pub(super) fn label_of(root: &Path, workflow: &Path) -> Option<String> {
    let (_, registry) = registry(root)?;
    let wanted = workflow.strip_prefix("./").unwrap_or(workflow);
    let labels = nika_cadence::emit::labels(&registry);
    (registry.beats().zip(labels)).find_map(|(beat, label)| {
        let declared = Path::new(&beat.workflow);
        (declared.strip_prefix("./").unwrap_or(declared) == wanted).then_some(label)
    })
}

/// The key every unit of the project at `project` is named with.
fn project_key(project: &Path) -> Option<String> {
    let fingerprint = nika_runtime::project_root_fingerprint(project)?;
    Some(nika_cadence::emit::project_key(&fingerprint))
}

/// The launchd file name of a beat's unit.
fn unit_file(key: &str, label: &str) -> String {
    format!("{}.plist", nika_cadence::emit::beat_unit(key, label))
}

/// Journal a `disarmed` decision in the ledger of each schedule whose unit `removed` holds,
/// then say each journal line in the person's language, a refused one with why.
pub(super) fn journal_disarms(root: &Path, removed: &[PathBuf], lang: Lang) -> String {
    let Some((project, registry)) = registry(root) else {
        return String::new();
    };
    let Some(key) = project_key(&project) else {
        return String::new();
    };
    let state = ArmState::at_project(&project);
    let mut said = String::new();
    for label in nika_cadence::emit::labels(&registry) {
        let unit = unit_file(&key, &label);
        if !(removed.iter()).any(|path| path.file_name().is_some_and(|name| name == unit.as_str()))
        {
            continue;
        }
        let recorded = state.record_disarm(
            &label,
            jiff::Timestamp::now(),
            std::process::id(),
            "trigger removed from this Mac by the Session (« désactive-le »)",
        );
        let line = match (recorded, lang) {
            (Ok(_), Lang::Fr) => {
                format!("\n  journalisé : disarmed dans `.nika/arm/{label}/history.ndjson`")
            }
            (Ok(_), _) => format!("\n  journaled: disarmed in `.nika/arm/{label}/history.ndjson`"),
            (Err(error), Lang::Fr) => format!("\n  historique non journalisé ({error})"),
            (Err(error), _) => format!("\n  history not journaled ({error})"),
        };
        said.push_str(&line);
    }
    said
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::path::Path;

    use nika_arm::state::{ArmState, FireKind, HistoryEntry};

    use super::{LastDecision, histories, journal_disarms, label_of, unit_file};
    use crate::runtime::Lang;

    fn ts(text: &str) -> jiff::Timestamp {
        text.parse().expect("ts")
    }

    /// A project declaring two schedules, the first with a fired and a skipped slot journaled.
    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("nika.yaml"),
            "nika: proj\narm:\n  - workflow: daily.nika\n    cadence: \"TZ=Europe/Paris 0 8 * * *\"\n    plafond: 0.2\n    manqué: sauter\n  - workflow: reports/weekly.nika\n    cadence: \"TZ=Europe/Paris 0 9 * * 1\"\n    plafond: 0.2\n    manqué: sauter\n    actif: false\n    raison: \"en pause\"\n    jusqu_au: 2027-01-01\n",
        )
        .expect("nika.yaml");
        let state = ArmState::at_project(dir.path());
        let mut fired = HistoryEntry::new(
            Some(ts("2026-10-15T06:00:00Z")),
            ts("2026-10-15T06:00:02Z"),
            FireKind::Fired,
        );
        fired.exit = Some(0);
        state.record_fixture("daily", &fired).expect("fired");
        let mut skipped = HistoryEntry::new(
            Some(ts("2026-10-16T06:00:00Z")),
            ts("2026-10-16T07:30:00Z"),
            FireKind::Skipped,
        );
        skipped.reason = Some("missed:1".to_owned());
        state.record_fixture("daily", &skipped).expect("skipped");
        dir
    }

    /// Each declared schedule is read with its counts, its last decision and its unit; an
    /// untouched one reads empty, and nothing is created for it.
    #[test]
    fn the_history_reads_every_schedule_and_creates_nothing() {
        let dir = project();
        let home = tempfile::tempdir().expect("home");
        let label = label_of(dir.path(), Path::new("./daily.nika")).expect("its label");
        assert_eq!(label, "daily");
        let key = super::project_key(dir.path()).expect("key");
        let agents = home.path().join("Library/LaunchAgents");
        std::fs::create_dir_all(&agents).expect("agents");
        std::fs::write(agents.join(unit_file(&key, "daily")), "<plist/>").expect("unit");
        let agents = home.path().join("Library/LaunchAgents");
        let on_mac = |label: &str| agents.join(unit_file(&key, label)).is_file();
        let seen = histories(dir.path(), &on_mac);
        assert_eq!(seen.len(), 2);
        let (daily, weekly) = (&seen[0], &seen[1]);
        assert_eq!(
            (
                daily.label.as_str(),
                daily.workflow.as_str(),
                daily.active,
                daily.unit_installed
            ),
            ("daily", "daily.nika", true, true)
        );
        assert_eq!(
            (
                daily.fired,
                daily.skipped,
                daily.paused,
                daily.failed,
                daily.disarmed
            ),
            (1, 1, 0, 0, 0)
        );
        assert_eq!(
            daily.last,
            Some(LastDecision {
                kind: "skipped",
                slot: "2026-10-16T06:00:00Z".to_owned(),
                decided_at: "2026-10-16T07:30:00Z".to_owned(),
                exit: None,
                trace: None,
                reason: Some("missed:1".to_owned()),
                slots: None,
            })
        );
        assert_eq!(daily.unreadable, None);
        assert_eq!(
            (
                weekly.label.as_str(),
                weekly.active,
                weekly.unit_installed,
                weekly.fired
            ),
            ("weekly", false, false, 0)
        );
        assert_eq!(weekly.last, None);
        assert!(
            !dir.path().join(".nika/arm/weekly").exists(),
            "a read creates no ledger"
        );
    }

    /// A removed unit journals `disarmed` in its schedule's ledger, said in the person's
    /// language; a unit of no declared schedule journals nothing.
    #[test]
    fn a_removed_unit_journals_its_disarm() {
        let dir = project();
        let key = super::project_key(dir.path()).expect("key");
        let removed = [
            Path::new("/units").join(unit_file(&key, "daily")),
            Path::new("/units").join(unit_file(&key, "unknown")),
        ];
        let said = journal_disarms(dir.path(), &removed, Lang::Fr);
        assert_eq!(
            said,
            "\n  journalisé : disarmed dans `.nika/arm/daily/history.ndjson`"
        );
        let seen = histories(dir.path(), &|_| false);
        assert_eq!((seen[0].disarmed, seen[0].fired), (1, 1));
        assert_eq!(seen[1].disarmed, 0);
        // The disarm came after the fired run: none fired since (supernovae-1c, 2026-10-17).
        assert!(seen[0].last_disarmed_at.is_some());
        assert_eq!(seen[0].fired_since_disarm, 0);
        assert!(
            !seen[0].serve_running,
            "no resident holds this project's lease"
        );
    }
}
