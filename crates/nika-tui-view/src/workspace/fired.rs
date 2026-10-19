// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a declared schedule's trigger did, as the Session's arm-ledger reader says it
//! ([`nika_session::SessionRuntime::schedule_history`], read once per batch): the rail's
//! activation field at ● once a fire is proved, a line in the conversation when the slots it
//! decided change, and the runs it fired under its workflow in the list. Every fact is the
//! Session's: nothing here reads a ledger, probes a firer, arms or starts anything.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nika_display::state::TaskState;
use nika_session::{LastDecision, ScheduleHistory};

use crate::lang::{Lang, Voice, fill};
use crate::visual::state;

/// Whether a fire of `schedule` is proved: its trigger ran it since its last disarm, a firer
/// stands for it (this machine holds its unit, or a resident `nika serve` serves the project),
/// `nika.yaml` keeps it active and its ledger reads. A declaration alone, a removed trigger, a
/// fire from before the last disarm or an unreadable ledger proves nothing.
#[must_use]
pub fn proved(schedule: &ScheduleHistory) -> bool {
    schedule.active
        && schedule.unreadable.is_none()
        && schedule.fired_since_disarm > 0
        && (schedule.unit_installed || schedule.serve_running)
}

/// The word the rail says beside a declared schedule's ◐, in `voice`: « waiting » once a firer
/// stands for it (its unit on this Mac, a resident `nika serve`) and no fire is proved yet,
/// « declared » while none does, so the two read apart (live gauntlet 5, r01 B7, r02 B15, r05,
/// r08 B15: « Actif ◐ » read the same installed or declared only). A proved fire reads ● and
/// needs no word.
#[must_use]
pub fn standing(schedule: Option<&ScheduleHistory>, voice: Voice) -> &'static str {
    let firer = schedule.is_some_and(|s| s.unit_installed || s.serve_running);
    voice.say(if firer { "waiting" } else { "declared" })
}

/// The schedule `nika.yaml` declares for the workflow at `path` (project-relative), if any.
#[must_use]
pub fn of<'a>(schedules: &'a [ScheduleHistory], path: &str) -> Option<&'a ScheduleHistory> {
    fn bare(path: &str) -> &str {
        path.strip_prefix("./").unwrap_or(path)
    }
    schedules.iter().find(|s| bare(&s.workflow) == bare(path))
}

/// The decisions its ledger counts: runs fired, slots skipped, runs parked at a gate, failures.
#[must_use]
pub fn decisions(schedule: &ScheduleHistory) -> [usize; 4] {
    [
        schedule.fired,
        schedule.skipped,
        schedule.paused,
        schedule.failed,
    ]
}

/// What a schedule's line in the conversation says: the decisions its ledger counts, and
/// whether that ledger cannot be read.
pub type Said = ([usize; 4], bool);

/// What `schedule`'s line says now.
#[must_use]
pub fn state(schedule: &ScheduleHistory) -> Said {
    (decisions(schedule), schedule.unreadable.is_some())
}

/// Whether a schedule's line has news since `before`, what it last said (`None` before its
/// first): a ledger that cannot be read, said once until it reads again though it counts
/// nothing, so the person knows its fires cannot be proved; counted decisions that changed.
/// Nothing counted is no news.
#[must_use]
pub fn news(before: Option<Said>, now: Said) -> bool {
    let (decided, unreadable) = now;
    if unreadable {
        return before.is_none_or(|(_, was)| !was);
    }
    decided != [0; 4] && before.is_none_or(|(was, _)| was != decided)
}

/// How an instant reads in this module: the person's language and the machine's clock.
pub type Clocked<'a> = (Voice, &'a TimeZone);

/// `at` on the clock of `zone`, in `lang`: month-day in English (`10-17 08:00`), day/month in
/// French (`17/10 08:00`), as the runs are dated.
fn on(at: Timestamp, zone: &TimeZone, lang: Lang) -> String {
    let local = at.to_zoned(zone.clone());
    let (month, day) = (local.month(), local.day());
    let time = format!("{:02}:{:02}", local.hour(), local.minute());
    match lang {
        Lang::Fr => format!("{day:02}/{month:02} {time}"),
        _ => format!("{month:02}-{day:02} {time}"),
    }
}

/// A ledger instant (RFC 3339, as the ledger writes it) on the machine's clock, where the
/// person reads it (live gauntlet 5, section F: times read in UTC); anything that does not
/// read as an instant stays as it came.
fn when(rfc: &str, (voice, machine): Clocked<'_>) -> String {
    rfc.parse::<Timestamp>()
        .map_or_else(|_| rfc.to_owned(), |at| on(at, machine, voice.lang))
}

/// The zone `schedule`'s cadence declares (`TZ=Europe/Paris 0 8 * * *`), from the embedded
/// tzdb, never the host's zoneinfo: its name and its clock. `None` without one, or for a name
/// that tzdb does not hold.
fn declared(schedule: &ScheduleHistory) -> Option<(&str, TimeZone)> {
    let name = (schedule.cadence.strip_prefix("TZ="))?
        .split_whitespace()
        .next()?;
    let (canonical, tzif) = jiff_tzdb::get(name)?;
    Some((name, TimeZone::tzif(canonical, tzif).ok()?))
}

/// When `schedule`'s slot `rfc` was due, on the clock of the zone its cadence declares (a slot
/// belongs to its schedule), that zone named where its clock reads otherwise than the
/// machine's at that instant, in the Session's own words for a zone so both name it the same
/// way ([`nika_session::zone_words`]: `10-16 20:00, Mauritius time`, « 16/10 20:00, heure de
/// Maurice »), its place alone in a list (`compact`, [`nika_session::zone_place`]); on the
/// machine's clock without a declared zone.
fn slot(
    schedule: &ScheduleHistory,
    rfc: &str,
    (voice, machine): Clocked<'_>,
    compact: bool,
) -> String {
    let Ok(at) = rfc.parse::<Timestamp>() else {
        return rfc.to_owned();
    };
    match declared(schedule) {
        Some((name, zone)) if zone.to_offset(at) != machine.to_offset(at) => {
            let said = on(at, &zone, voice.lang);
            if compact {
                format!("{said} {}", nika_session::zone_place(name, voice.lang))
            } else {
                format!("{said}, {}", nika_session::zone_words(name, voice.lang))
            }
        }
        _ => on(at, machine, voice.lang),
    }
}

/// The state a decision reads as: a fire by its exit, a skip, a park at a gate, a failure.
fn task_state(last: &LastDecision) -> TaskState {
    match (last.kind, last.exit) {
        ("fired", Some(0) | None) => TaskState::Ok,
        ("skipped", _) => TaskState::Skipped,
        ("paused", _) => TaskState::Paused,
        _ => TaskState::Failed,
    }
}

/// Why a slot was skipped, in `voice`, from the reason the ledger journals: the slots missed
/// (`missed:2`, « 2 créneaux manqués »), a run still going, a suspension, and the firer's other
/// reasons; a reason this TUI does not know stays as the ledger wrote it.
fn skip_reason(reason: &str, voice: Voice) -> String {
    let missed = (reason.strip_prefix("missed:")).and_then(|n| n.parse::<usize>().ok());
    if let Some(missed) = missed {
        return voice.count(missed, "missed slot");
    }
    let words = match reason {
        "overlap" => "a run was still going",
        "overlap-timeout" => "its queue waited too long",
        "inactive" => "suspended in nika.yaml",
        "expired" => "past its end date",
        "cloud" => "run in the cloud",
        "webhook" => "it fires on its event",
        "serve-stop" => "nika serve stopped",
        "already" => "already decided",
        "not-due" => "not due",
        _ => return reason.to_owned(),
    };
    voice.say(words).to_owned()
}

/// The row of `schedule`'s last slot `last`, in `voice`: how it ended (its exit, why it was
/// skipped, a gate's park, a failure) and its trace, then the missed slots one catch-up run
/// answered, the slot on its schedule's clock and the catch-up on the machine's.
fn last_rows(schedule: &ScheduleHistory, last: &LastDecision, said: Clocked<'_>) -> Vec<String> {
    let voice = said.0;
    let slot = slot(schedule, &last.slot, said, false);
    let exit = (last.exit).map_or_else(|| voice.say("unknown").to_owned(), |e| e.to_string());
    let why = (last.reason.as_deref()).map_or_else(String::new, |r| skip_reason(r, voice));
    let ended = match (last.kind, why.is_empty()) {
        ("fired", _) => voice.say("last slot {slot} · exit {exit}"),
        ("skipped", false) => voice.say("last slot {slot} skipped: {why}"),
        ("skipped", true) => voice.say("last slot {slot} skipped"),
        ("paused", _) => voice.say("last slot {slot} paused: a gate asks you"),
        _ => voice.say("last slot {slot} failed · exit {exit}"),
    };
    let values: [(&str, &str); 3] = [("{slot}", &slot), ("{exit}", &exit), ("{why}", &why)];
    let mut row = fill(ended, &values);
    if let Some(trace) = &last.trace {
        row.push_str(&voice.say(" · trace {trace}").replace("{trace}", trace));
    }
    let mut rows = vec![format!("  {row}")];
    if let Some(missed) = (last.slots).filter(|_| last.kind == "fired") {
        let missed = usize::try_from(missed).unwrap_or(usize::MAX);
        let caught = voice.count(missed, "missed slot caught up");
        let decided = when(&last.decided_at, said);
        let late = voice.say("{caught} at {decided}");
        rows.push(format!(
            "  {}",
            fill(late, &[("{caught}", &caught), ("{decided}", &decided)])
        ));
    }
    rows
}

/// The conversation's line of what `schedule`'s trigger decided, in `voice` and on the machine's
/// clock: the runs it fired, its last slot and how it ended (its exit, its trace, why it was
/// skipped), a catch-up, the slots it skipped, the runs a gate parked and the failures, a
/// removed trigger; a ledger that cannot be read on a line of its own, since nothing it counts
/// can be proved.
#[must_use]
pub fn said(schedule: &ScheduleHistory, (voice, machine): Clocked<'_>) -> String {
    if let Some(why) = &schedule.unreadable {
        let said = voice.say(
            "`{workflow}`: its trigger's ledger cannot be read, so its runs cannot be proved: {why}",
        );
        return fill(said, &[("{workflow}", &schedule.workflow), ("{why}", why)]);
    }
    let fired = voice.count(schedule.fired, "run by its trigger");
    let head = voice.say("`{workflow}` ran on its own: {fired}");
    let mut rows = vec![fill(
        head,
        &[("{workflow}", &schedule.workflow), ("{fired}", &fired)],
    )];
    if let Some(last) = &schedule.last {
        rows.extend(last_rows(schedule, last, (voice, machine)));
    }
    let others: Vec<String> = [
        (schedule.skipped, "slot skipped"),
        (schedule.paused, "run paused at a gate"),
        (schedule.failed, "run failed"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, unit)| voice.count(n, unit))
    .collect();
    if !others.is_empty() {
        rows.push(format!("  {}", others.join(" · ")));
    }
    if schedule.disarmed > 0 && !schedule.unit_installed {
        rows.push(format!(
            "  {}",
            voice.say("its trigger was removed from this Mac")
        ));
    }
    rows.join("\n")
}

/// The list's entry under its workflow, in `voice` and on the machine's clock: the runs its
/// trigger fired, then its last slot on its schedule's clock (that zone's city where it reads
/// otherwise) and that slot's state glyph.
#[must_use]
pub fn listed(schedule: &ScheduleHistory, (voice, machine): Clocked<'_>) -> String {
    let fired = voice.count(schedule.fired, "fired run");
    let Some(last) = &schedule.last else {
        return fired;
    };
    let glyph = state::cell(task_state(last), voice.ascii).0;
    let short = slot(schedule, &last.slot, (voice, machine), true);
    format!("{fired} · {short} {glyph}")
}

#[cfg(test)]
mod tests;
