// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::expect_used)]

use std::time::{Duration, Instant};

use nika_display::activity::{Activity, CallMark, CallState, Phase};

use super::*;
use crate::lang::Voice;

fn call(ordinal: u32, state: CallState) -> Activity {
    let mut activity = Activity::now(Phase::Authoring, "writing");
    activity.call = Some(CallMark::new(
        ordinal,
        "plan",
        "deepseek/deepseek-flash",
        state,
    ));
    activity
}

const S: fn(u64) -> Duration = Duration::from_secs;

/// The call `ordinal` returned, its cost as the Session stated it (or nothing stated).
fn returned(ordinal: u32, cost: Option<CallCost>) -> Activity {
    let mut activity = call(ordinal, CallState::Finished);
    activity.call = activity.call.map(|mark| match cost {
        Some(cost) => mark.costing(cost),
        None => mark,
    });
    activity
}

/// The live line sums what the finished calls settled, as the Session priced each one: no cost
/// while none is priced (an unpriced call alone says nothing), `~$0.12 + 1 unpriced` once one
/// returned priced beside it, the sum as more return priced; a call the Session states nothing
/// of adds nothing, and the words never claim a figure no call settled.
#[test]
fn the_live_line_sums_what_the_finished_calls_settled() {
    let started = Instant::now();
    let mut pulse = Pulse::new(started);
    let at = |pulse: &mut Pulse, secs: u64| pulse.line(started + S(secs), None, true, "");
    pulse.observe(&call(1, CallState::Started));
    pulse.observe(&returned(1, Some(CallCost::Unpriced)));
    assert_eq!(
        at(&mut pulse, 5),
        "Preparing · 5s · 1 author call · Ctrl+C stop",
        "nothing priced: no cost said"
    );
    pulse.observe(&call(2, CallState::Started));
    pulse.observe(&returned(2, Some(CallCost::Priced(120_000_000))));
    assert_eq!(
        at(&mut pulse, 10),
        "Preparing · 10s · 2 author calls · ~$0.12 + 1 unpriced · Ctrl+C stop"
    );
    pulse.observe(&call(3, CallState::Started));
    pulse.observe(&returned(3, Some(CallCost::Priced(190_000_000))));
    pulse.observe(&call(4, CallState::Started));
    pulse.observe(&returned(4, None));
    assert_eq!(
        at(&mut pulse, 15),
        "Preparing · 15s · 4 author calls · ~$0.31 + 1 unpriced · Ctrl+C stop"
    );
    let mut priced = Pulse::new(started);
    priced.observe(&call(1, CallState::Started));
    priced.observe(&returned(1, Some(CallCost::Priced(120_000_000))));
    priced.observe(&call(2, CallState::Started));
    assert_eq!(
        at(&mut priced, 5),
        "Preparing · 5s · 2 author calls · ~$0.12 · Ctrl+C stop",
        "the call under way adds nothing yet"
    );
}

/// Each started call counts once, its return adds nothing, and a call first
/// seen returned still counts: the activity card's own count.
#[test]
fn author_calls_are_counted_as_the_activity_card_counts_them() {
    let mut pulse = Pulse::new(Instant::now());
    pulse.observe(&Activity::now(Phase::Understanding, "reading"));
    assert_eq!(pulse.calls(), 0, "a phase is no call");
    pulse.observe(&call(1, CallState::Started));
    pulse.observe(&call(1, CallState::Finished));
    assert_eq!(pulse.calls(), 1);
    pulse.observe(&call(1, CallState::Started));
    pulse.observe(&call(1, CallState::Cancelled));
    assert_eq!(pulse.calls(), 2, "ordinals restart in every scope");
    pulse.observe(&call(7, CallState::Finished));
    assert_eq!(pulse.calls(), 3, "first seen returned");
}

/// Preparing names its time, its calls once there are some, what `Ctrl+C`
/// does, then the Session's phase words; without a stop it only works and
/// two presses leave. Nothing unknown appears.
#[test]
fn the_preparing_line_says_its_time_calls_and_stop() {
    let started = Instant::now();
    let mut pulse = Pulse::new(started);
    assert_eq!(
        pulse.line(
            started + S(3),
            None,
            true,
            "● authoring · deepseek/deepseek-flash"
        ),
        "Preparing · 3s · Ctrl+C stop · ● authoring · deepseek/deepseek-flash"
    );
    for n in 1..=12 {
        pulse.observe(&call(n, CallState::Started));
    }
    assert_eq!(
        pulse.line(started + S(45), None, true, ""),
        "Preparing · 45s · 12 author calls · Ctrl+C stop"
    );
    let mut other = Pulse::new(started);
    assert_eq!(
        other.line(started + S(130), None, false, "the demo holds this turn"),
        "the demo holds this turn · 2m 10s · Ctrl+C twice leaves"
    );
    let mut bare = Pulse::new(started);
    assert_eq!(
        bare.line(started + S(1), None, false, ""),
        "Working",
        "short"
    );
    let mut bare = Pulse::new(started);
    assert_eq!(
        bare.line(started + S(2), None, false, ""),
        "Working · 2s · Ctrl+C twice leaves"
    );
}

/// The facts are composed at most twice a second: within the window the
/// line is the one composed, whatever arrived since.
#[test]
fn the_facts_are_composed_at_most_twice_a_second() {
    let started = Instant::now();
    let mut pulse = Pulse::new(started);
    let first = pulse.line(started + S(1), None, true, "");
    pulse.observe(&call(1, CallState::Started));
    let soon = started + S(1) + Duration::from_millis(300);
    assert_eq!(pulse.line(soon, None, true, ""), first, "too soon");
    let later = started + S(1) + Duration::from_millis(500);
    assert_eq!(
        pulse.line(later, None, true, ""),
        "Preparing · 1s · 1 author call · Ctrl+C stop"
    );
}

/// A preparation that called its author ends in one line; one without a
/// call, or one that ran a workflow, ends in none. An unknown spend is left
/// out, never written as zero.
#[test]
fn a_preparation_ends_in_one_line_of_known_facts() {
    let started = Instant::now();
    let mut pulse = Pulse::new(started);
    assert_eq!(pulse.prepared(S(2), Some(0.5), false), None, "no call");
    for n in 1..=35 {
        pulse.observe(&call(n, CallState::Started));
    }
    assert_eq!(
        pulse.prepared(S(130), Some(0.91), false).as_deref(),
        Some("Prepared in 2m 10s · 35 calls · $0.91")
    );
    assert_eq!(
        pulse
            .prepared(Duration::from_millis(16_540), None, false)
            .as_deref(),
        Some("Prepared in 16.5s · 35 calls")
    );
    assert_eq!(
        pulse
            .prepared(Duration::from_millis(600), None, true)
            .as_deref(),
        Some("Stopped after 0.6s · 35 calls"),
        "a stopped preparation never says it prepared"
    );
    pulse.run_asked(started);
    assert_eq!(
        pulse.prepared(S(9), Some(0.5), false),
        None,
        "the run card says it"
    );
}

#[test]
fn spend_and_time_read_as_a_person_reads_them() {
    assert_eq!(money(0.012), "$0.012");
    assert_eq!(money(0.25), "$0.25");
    assert_eq!(money(0.034), "$0.034");
    assert_eq!(money(1.5), "$1.50");
    assert_eq!(money(0.0001), "<$0.001");
    assert_eq!(clock(S(59)), "59s");
    assert_eq!(clock(S(61)), "1m 01s");
    assert_eq!(took_words(Duration::from_millis(900)), "0.9s");
    assert_eq!(took_words(S(130)), "2m 10s");
}

/// A run's time rounds as the Session words it: the milliseconds over a
/// thousand to one decimal, never cut. 9857 ms is « 9.9 s » in the Session's
/// summary of the run (`Done · … · 9.9 s · 4 tasks ran`), so the report reads
/// 9.9 too, in either language.
#[test]
fn a_runs_time_rounds_as_the_session_words_it() {
    // The Session's own words for a time under a minute: `{:.1} s` of ms / 1000.
    let session = |ms: u32| format!("{:.1}", f64::from(ms) / 1000.0);
    for ms in [9_857_u32, 16_549, 16_551, 1_999, 59_949, 450] {
        let said = took_words(Duration::from_millis(u64::from(ms)));
        assert_eq!(said, format!("{}s", session(ms)), "{ms} ms");
    }
    assert_eq!(took_words(Duration::from_millis(9_857)), "9.9s");
    assert_eq!(took_words(Duration::from_millis(1_999)), "2.0s");
    assert_eq!(took_words(Duration::from_millis(16_549)), "16.5s");
}

const EXEC: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";

/// A runtime event `n` of `kind` in the run, with `fields`.
fn event(n: u32, kind: &str, fields: &str) -> nika_display::run_story::RunFrame {
    let line = format!(
        r#"{{"correlation":null,"execution":{{"uuid":"{EXEC}"}},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
    );
    nika_display::run_story::RunFrame::decode(&line).expect("a runtime event")
}

fn task(id: &str) -> String {
    format!(r#"{{"key":"task","value":"{id}"}}"#)
}

/// A run of three tasks: the first done, the second under way on its model,
/// then, `priced`, done at a metered cost with the third under way.
fn followed(priced: bool) -> LiveRun {
    let mut leg = LiveRun::asked("flows/veille-tech.nika".to_owned(), false, true, None);
    let started = r#"{"key":"workflow","value":"veille-tech"},{"key":"ceiling_usd","value":0.25}"#;
    let pick_model = format!(
        r#"{},{{"key":"model","value":"anthropic/claude-sonnet-5-5"}}"#,
        task("pick")
    );
    let pick_cost = format!(r#"{},{{"key":"cost_usd","value":0.012}}"#, task("pick"));
    let mut frames = vec![
        event(1, "workflow_started", started),
        event(2, "task_scheduled", &task("fetch")),
        event(3, "task_scheduled", &task("pick")),
        event(4, "task_scheduled", &task("write")),
        event(5, "task_started", &task("fetch")),
        event(6, "task_completed", &task("fetch")),
        event(7, "task_started", &pick_model),
    ];
    if priced {
        frames.push(event(8, "task_completed", &pick_cost));
        frames.push(event(9, "task_started", &task("write")));
    }
    for frame in frames {
        leg.apply(frame);
    }
    leg
}

fn settle(leg: &mut LiveRun, status: &str) {
    let line = format!(
        r#"{{"kind":"run_settled","status":"{status}","cause":"normal","elapsed_ms":16540,"execution":{{"uuid":"{EXEC}"}},"evidence":"unsealed","spend":{{"total_cost_usd":0.034,"priced_calls":1,"qualifier":"priced","unpriced_calls":0}},"tasks":{{"cancelled":0,"failed":0,"never_started":0,"ok":3,"recovered":0,"skipped":0,"total":3}},"outputs":{{}}}}"#
    );
    leg.apply(nika_display::run_story::RunFrame::decode(&line).expect("a settlement"));
}

/// A run the turn asked is followed from its own frames: its file, its time
/// since it was asked, tasks done of all, the one under way and its model,
/// and its metered spend against the ceiling once a price is known; a Run is
/// never stopped here, so two presses leave.
#[test]
fn a_followed_run_says_its_tasks_its_task_under_way_and_its_spend() {
    let started = Instant::now();
    let mut pulse = Pulse::new(started);
    pulse.run_asked(started + S(2));
    let leg = followed(false);
    assert_eq!(
        pulse.line(started + S(14), Some(&leg), true, "→ pick · infer"),
        "Running veille-tech.nika · 12s · 1/3 tasks · now: pick (claude-sonnet-5-5) · Ctrl+C twice leaves"
    );
    let leg = followed(true);
    let mut pulse = Pulse::new(started);
    pulse.run_asked(started);
    assert_eq!(
        pulse.line(started + S(20), Some(&leg), true, ""),
        "Running veille-tech.nika · 20s · 2/3 tasks · now: write · $0.012 of $0.25 · Ctrl+C twice leaves"
    );
    // A leg the turn never asked is not followed: the turn still prepares.
    let mut other = Pulse::new(started);
    assert!(
        other
            .line(started + S(3), Some(&leg), true, "")
            .starts_with("Preparing · 3s")
    );
}

/// Once the run settled, while the turn goes on, the live line says how it
/// ended, as its card and the run's header do (never « Running » with its
/// tasks counted done), then what the turn does now; a run paused at its gate
/// says so.
#[test]
fn a_settled_run_reads_as_it_ended_on_the_live_line_never_running() {
    let started = Instant::now();
    let mut pulse = Pulse::new(started);
    pulse.run_asked(started + S(2));
    let mut failed = followed(true);
    settle(&mut failed, "failed");
    let reading = "reading what the run left";
    assert_eq!(
        pulse.line(started + S(20), Some(&failed), true, reading),
        "veille-tech.nika · Failed after 16.5s · 2 tasks ran · $0.0340 · 1 model call · reading what the run left · Ctrl+C twice leaves"
    );
    let mut succeeded = followed(true);
    settle(&mut succeeded, "succeeded");
    let mut pulse = Pulse::new(started);
    pulse.run_asked(started);
    assert_eq!(
        pulse.line(started + S(20), Some(&succeeded), true, ""),
        "veille-tech.nika · Ran in 16.5s · 2 tasks ran · $0.0340 · 1 model call · Ctrl+C twice leaves"
    );
    let mut paused = followed(true);
    settle(&mut paused, "paused");
    let mut pulse = Pulse::new(started);
    pulse.run_asked(started);
    assert_eq!(
        pulse.line(started + S(20), Some(&paused), true, ""),
        "veille-tech.nika · paused · a gate asks you · Ctrl+C twice leaves"
    );
}

/// A settled run's card opens on how it ended: its time, the tasks its frames
/// counted as they ran (a task its frames left under way counts in none), its
/// metered spend and model calls as its settlement declared them; a paused run,
/// its gate still asking, has none yet.
#[test]
fn a_settled_run_card_opens_on_how_it_ended() {
    let mut leg = followed(true);
    assert_eq!(ran(&leg, Voice::default()), None, "no settlement yet");
    settle(&mut leg, "succeeded");
    assert_eq!(
        ran(&leg, Voice::default()).as_deref(),
        Some("Ran in 16.5s · 2 tasks ran · $0.0340 · 1 model call")
    );
    let mut failed = followed(true);
    settle(&mut failed, "failed");
    assert!(
        ran(&failed, Voice::default()).is_some_and(|line| line.starts_with("Failed after 16.5s"))
    );
    let mut paused = followed(true);
    settle(&mut paused, "paused");
    assert_eq!(ran(&paused, Voice::default()), None);
}

/// The live line says the shell's own words in the person's language: the
/// preparation and its author calls, a run under way with its tasks, the task
/// under way and its spend against the ceiling, how a run ended and how a
/// preparation ended. Facts keep their form (`45s`, `16.5s`, `$0.012`, file,
/// task and model names), and the Session's phase words stay as said.
#[test]
fn the_live_line_speaks_the_persons_language() {
    let french = Voice {
        lang: crate::lang::Lang::Fr,
        ascii: false,
    };
    let started = Instant::now();
    let mut pulse = Pulse::new(started).speaking(french);
    pulse.observe(&call(1, CallState::Started));
    pulse.observe(&returned(1, Some(CallCost::Unpriced)));
    pulse.observe(&call(2, CallState::Started));
    pulse.observe(&returned(2, Some(CallCost::Priced(120_000_000))));
    assert_eq!(
        pulse.line(started + S(45), None, true, "● authoring"),
        "Préparation · 45s · 2 appels auteur · ~$0.12 + 1 sans prix · Ctrl+C arrête · ● authoring"
    );
    assert_eq!(
        pulse.prepared(S(130), Some(0.91), false).as_deref(),
        Some("Préparé en 2m 10s · 2 appels · $0.91")
    );
    assert_eq!(
        pulse
            .prepared(Duration::from_millis(600), None, true)
            .as_deref(),
        Some("Arrêté après 0.6s · 2 appels")
    );
    let mut pulse = Pulse::new(started).speaking(french);
    pulse.run_asked(started);
    assert_eq!(
        pulse.line(started + S(20), Some(&followed(true)), true, ""),
        "Exécution de veille-tech.nika · 20s · 2/3 tâches · en cours : write · $0.012 sur $0.25 · Ctrl+C deux fois quitte"
    );
    let mut failed = followed(true);
    settle(&mut failed, "failed");
    let mut pulse = Pulse::new(started).speaking(french);
    pulse.run_asked(started);
    assert_eq!(
        pulse.line(started + S(20), Some(&failed), true, ""),
        "veille-tech.nika · Échoué après 16.5s · 2 tâches exécutées · $0.0340 · 1 appel de modèle · Ctrl+C deux fois quitte"
    );
    let mut succeeded = followed(true);
    settle(&mut succeeded, "succeeded");
    assert!(
        ran(&succeeded, french).is_some_and(|line| line.starts_with("Exécuté en 16.5s · ")),
        "{:?}",
        ran(&succeeded, french)
    );
    let plain = Voice {
        ascii: true,
        ..french
    };
    assert!(ran(&failed, plain).is_some_and(|line| line.starts_with("Echoue apres 16.5s")));
}
