// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a declared schedule's trigger decided, as the rail, the list and the conversation say
//! it: only a fire of an installed, active trigger proves it active, the runs it fired are
//! counted under their workflow, and its line names the last slot, a catch-up, the slots it
//! skipped, the runs a gate parked and the failures, in the person's language.

use super::*;
use crate::lang::Lang;

/// The cadence of the daily schedules below, as `nika.yaml` declares it.
const DAILY: &str = "TZ=UTC 0 6 * * *";

/// A daily schedule of `compiled-workflow.nika`, installed and active, that fired `fired` runs,
/// its last one answering the slot of 16 October at 06:00 UTC at `decided`, built as the
/// Session builds it.
fn schedule(fired: usize, decided: &str) -> ScheduleHistory {
    let mut history = ScheduleHistory::new("compiled-workflow", "compiled-workflow.nika", DAILY);
    (history.unit_installed, history.fired) = (true, fired);
    history.fired_since_disarm = fired;
    let mut last = LastDecision::new("fired", "2026-10-16T06:00:00Z", decided);
    last.exit = Some(0);
    last.trace = Some(".nika/traces/t.ndjson".to_owned());
    history.last = Some(last);
    history
}

const FRENCH: Voice = Voice {
    lang: Lang::Fr,
    ascii: false,
};

/// The machine's clock these tests read: UTC, as the shell's is until it says the machine's.
const UTC: &TimeZone = &TimeZone::UTC;

/// [`said`] in `voice` on a UTC clock.
fn line(schedule: &ScheduleHistory, voice: Voice) -> String {
    said(schedule, (voice, UTC))
}

/// [`listed`] in `voice` on a UTC clock.
fn entry(schedule: &ScheduleHistory, voice: Voice) -> String {
    listed(schedule, (voice, UTC))
}

/// A declaration alone proves nothing, nor a removed or suspended trigger, nor a fire from
/// before the last disarm, nor an unreadable ledger: only a run its active trigger fired since
/// that disarm, with a firer standing, its unit on this machine or a resident `nika serve`.
#[test]
fn a_fire_is_proved_only_by_a_run_of_an_installed_active_trigger() {
    let proved_one = schedule(1, "2026-10-16T06:00:02Z");
    assert!(proved(&proved_one));
    let declared = ScheduleHistory::new("daily", "daily.nika", DAILY);
    assert!(!proved(&declared), "declared, never fired");
    let mut removed = proved_one.clone();
    (removed.unit_installed, removed.disarmed) = (false, 1);
    assert!(!proved(&removed), "its trigger was removed");
    let mut served = removed.clone();
    served.serve_running = true;
    assert!(proved(&served), "a resident nika serve fires it");
    let mut reinstalled = proved_one.clone();
    (reinstalled.disarmed, reinstalled.fired_since_disarm) = (1, 0);
    reinstalled.last_disarmed_at = Some("2026-10-16T09:00:00Z".to_owned());
    assert!(!proved(&reinstalled), "installed again, not fired since");
    reinstalled.fired_since_disarm = 1;
    assert!(proved(&reinstalled), "fired since it was installed again");
    let mut suspended = proved_one.clone();
    suspended.active = false;
    assert!(!proved(&suspended), "suspended in nika.yaml");
    let mut unread = proved_one;
    unread.unreadable = Some("a broken chain".to_owned());
    assert!(!proved(&unread), "an unreadable ledger proves nothing");
}

/// The line says the runs fired, the last slot with its exit and its trace, the missed slots
/// one catch-up run answered, as the ledger counts them (never a fire it does not mark as one,
/// however late), and the slots skipped, the runs a gate parked and the failures, in French as
/// in English; the workflow, the instants and the trace keep theirs.
#[test]
fn the_conversation_says_what_the_trigger_decided() {
    let mut caught = schedule(3, "2026-10-16T06:00:04Z");
    (caught.skipped, caught.failed) = (1, 1);
    if let Some(last) = caught.last.as_mut() {
        last.slots = Some(2);
    }
    assert_eq!(
        line(&caught, Voice::default()),
        "`compiled-workflow.nika` ran on its own: 3 runs by its trigger\n  last slot 10-16 06:00 · exit 0 · trace .nika/traces/t.ndjson\n  2 missed slots caught up at 10-16 06:00\n  1 slot skipped · 1 run failed"
    );
    assert_eq!(
        line(&caught, FRENCH),
        "`compiled-workflow.nika` a tourné tout seul : 3 runs par son déclencheur\n  dernier créneau 16/10 06:00 · sortie 0 · sa trace .nika/traces/t.ndjson\n  2 créneaux manqués rattrapés à 16/10 06:00\n  1 créneau sauté · 1 run en échec"
    );
    let late = line(&schedule(1, "2026-10-16T07:30:00Z"), Voice::default());
    assert!(
        !late.contains("caught up"),
        "a late fire on its own slot: {late}"
    );
    let mut parked = schedule(1, "2026-10-16T06:00:02Z");
    let mut last = LastDecision::new("paused", "2026-10-17T06:00:00Z", "2026-10-17T06:00:03Z");
    last.trace = Some(".nika/traces/p.ndjson".to_owned());
    parked.last = Some(last);
    parked.paused = 1;
    let parked = line(&parked, FRENCH);
    assert!(
        parked.contains("dernier créneau 17/10 06:00 en pause : une porte vous demande")
            && parked.contains("1 run en pause à une porte"),
        "{parked}"
    );
    let mut removed = schedule(2, "2026-10-16T06:00:02Z");
    (removed.unit_installed, removed.disarmed) = (false, 1);
    let removed = line(&removed, Voice::default());
    assert!(
        removed.ends_with("its trigger was removed from this Mac"),
        "{removed}"
    );
}

/// Under its workflow the list counts the runs its trigger fired and shows its last slot and
/// that slot's state glyph: done, skipped, parked or failed.
#[test]
fn the_list_counts_the_fired_runs_under_their_workflow() {
    let fired = schedule(3, "2026-10-16T06:00:02Z");
    assert_eq!(
        entry(&fired, Voice::default()),
        "3 fired runs · 10-16 06:00 ✔"
    );
    assert_eq!(entry(&fired, FRENCH), "3 runs déclenchés · 16/10 06:00 ✔");
    let mut failed = fired;
    if let Some(last) = failed.last.as_mut() {
        (last.kind, last.exit) = ("failed", Some(1));
    }
    assert!(entry(&failed, Voice::default()).ends_with('✖'));
    let none = ScheduleHistory::new("daily", "daily.nika", DAILY);
    assert_eq!(entry(&none, Voice::default()), "0 fired runs");
}

/// A ledger instant reads to the minute on the machine's clock, never in UTC (live gauntlet 5,
/// section F), dated as the runs are; anything else as it came.
#[test]
fn a_ledger_instant_reads_to_the_minute_on_the_machines_clock() {
    let east = TimeZone::fixed(jiff::tz::offset(4));
    let english = (Voice::default(), &east);
    assert_eq!(when("2026-10-17T06:00:00Z", english), "10-17 10:00");
    assert_eq!(
        when("2026-10-17T08:00:00+02:00", (FRENCH, &east)),
        "17/10 10:00"
    );
    assert_eq!(when("2026-10-16T22:15:00Z", (FRENCH, &east)), "17/10 02:15");
    assert_eq!(when("not an instant", english), "not an instant");
}

/// A slot belongs to its schedule: it reads on the clock of the zone the cadence declares, that
/// zone named where it reads otherwise than the machine's, while a catch-up reads on the
/// machine's clock; on a machine whose clock agrees, no zone is named.
#[test]
fn a_slot_reads_on_its_schedules_clock_its_zone_named_where_the_machines_differs() {
    let mut paris = schedule(2, "2026-10-16T08:00:00Z");
    paris.cadence = "TZ=Europe/Paris 0 8 * * *".to_owned();
    if let Some(last) = paris.last.as_mut() {
        last.slot = "2026-10-16T06:00:00Z".to_owned();
        last.slots = Some(1);
    }
    let east = TimeZone::fixed(jiff::tz::offset(4));
    let french = said(&paris, (FRENCH, &east));
    assert!(
        french.contains("dernier créneau 16/10 08:00, heure de Paris · sortie 0")
            && french.contains("1 créneau manqué rattrapé à 16/10 12:00"),
        "{french}"
    );
    let english = said(&paris, (Voice::default(), &east));
    assert!(
        english.contains("last slot 10-16 08:00, Paris time · exit 0"),
        "{english}"
    );
    assert_eq!(
        listed(&paris, (FRENCH, &east)),
        "2 runs déclenchés · 16/10 08:00 Paris ✔"
    );
    // A machine on the same clock as Paris that October day names no zone.
    let same = TimeZone::fixed(jiff::tz::offset(2));
    assert_eq!(
        listed(&paris, (FRENCH, &same)),
        "2 runs déclenchés · 16/10 08:00 ✔"
    );
    let mut york = paris.clone();
    york.cadence = "TZ=America/New_York 0 2 * * *".to_owned();
    assert!(
        said(&york, (Voice::default(), &east)).contains("last slot 10-16 02:00, New York time"),
        "{}",
        said(&york, (Voice::default(), &east))
    );
    // A zone reads as the Session names it, its place in the person's language: « heure de
    // Maurice », never the database's « Mauritius » in French.
    let mut mauritius = paris;
    mauritius.cadence = "TZ=Indian/Mauritius 0 20 * * *".to_owned();
    if let Some(last) = mauritius.last.as_mut() {
        last.slot = "2026-10-16T16:00:00Z".to_owned();
    }
    let french = said(&mauritius, (FRENCH, UTC));
    assert!(
        french.contains("dernier créneau 16/10 20:00, heure de Maurice · sortie 0"),
        "{french}"
    );
    let english = said(&mauritius, (Voice::default(), UTC));
    assert!(
        english.contains("last slot 10-16 20:00, Mauritius time · exit 0"),
        "{english}"
    );
    assert_eq!(
        listed(&mauritius, (FRENCH, UTC)),
        "2 runs déclenchés · 16/10 20:00 Maurice ✔"
    );
    assert_eq!(
        listed(&mauritius, (Voice::default(), UTC)),
        "2 fired runs · 10-16 20:00 Mauritius ✔"
    );
}

/// A skipped slot says why, as the ledger's reason reads to a person: the slots missed, a run
/// still going, a suspension, in French as in English; a reason this TUI does not know is said
/// as the ledger wrote it, and a skip with no reason says none.
#[test]
fn a_skipped_slot_says_why() {
    let mut skipped = schedule(1, "2026-10-16T06:00:02Z");
    skipped.skipped = 1;
    let mut last = LastDecision::new("skipped", "2026-10-17T06:00:00Z", "2026-10-17T07:10:00Z");
    for (reason, english, french) in [
        ("missed:1", "1 missed slot", "1 créneau manqué"),
        ("missed:3", "3 missed slots", "3 créneaux manqués"),
        ("overlap", "a run was still going", "un run tournait encore"),
        (
            "inactive",
            "suspended in nika.yaml",
            "suspendu dans nika.yaml",
        ),
        ("a-new-reason", "a-new-reason", "a-new-reason"),
    ] {
        last.reason = Some(reason.to_owned());
        skipped.last = Some(last.clone());
        let row = format!("last slot 10-17 06:00 skipped: {english}\n");
        assert!(line(&skipped, Voice::default()).contains(&row), "{reason}");
        let row = format!("dernier créneau 17/10 06:00 sauté : {french}\n");
        assert!(line(&skipped, FRENCH).contains(&row), "{reason}");
    }
    last.reason = None;
    skipped.last = Some(last);
    assert!(
        line(&skipped, Voice::default()).contains("last slot 10-17 06:00 skipped\n"),
        "no reason, none said"
    );
}

/// An unreadable ledger is said once even when nothing is counted, so the person knows the
/// proof cannot be read: its own line, the workflow and the reason kept, in French as in
/// English; counted decisions that change are news, the same counts or none are not.
#[test]
fn an_unreadable_ledger_is_said_once_even_with_nothing_counted() {
    let mut unread = ScheduleHistory::new("compiled-workflow", "compiled-workflow.nika", DAILY);
    unread.unreadable = Some("arm ledger: journal dialect or chain is invalid".to_owned());
    assert_eq!(
        line(&unread, Voice::default()),
        "`compiled-workflow.nika`: its trigger's ledger cannot be read, so its runs cannot be proved: arm ledger: journal dialect or chain is invalid"
    );
    assert_eq!(
        line(&unread, FRENCH),
        "`compiled-workflow.nika` : le journal de son déclencheur est illisible, ses runs ne peuvent pas être prouvés : arm ledger: journal dialect or chain is invalid"
    );
    let nothing = [0; 4];
    assert!(news(None, (nothing, true)), "unreadable at the opening");
    assert!(!news(Some((nothing, true)), (nothing, true)), "said once");
    assert!(
        news(Some((nothing, false)), (nothing, true)),
        "unreadable since"
    );
    assert!(!news(None, (nothing, false)), "nothing counted, readable");
    assert!(
        news(Some((nothing, true)), ([1, 0, 0, 0], false)),
        "read again"
    );
    assert!(!news(Some(([1, 0, 0, 0], false)), ([1, 0, 0, 0], false)));
    assert!(news(Some(([1, 0, 0, 0], false)), ([1, 1, 0, 0], false)));
    assert!(
        !news(Some(([1, 0, 0, 0], false)), (nothing, false)),
        "no count to say"
    );
}
