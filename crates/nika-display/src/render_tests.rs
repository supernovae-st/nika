// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The render goldens — split from `render.rs` at the 1500-LOC file
//! wall (the run/mod.rs → tests.rs precedent): same module, own file,
//! `super::*` unchanged.

use super::*;
use crate::demo;

fn fold(events: &[nika_event::Event]) -> RunView {
    let mut view = RunView::new();
    for ev in events {
        view.apply(ev);
    }
    view
}

const UNICODE: Theme = Theme::new(false, false, false);
const ASCII: Theme = Theme::new(false, true, false);

/// Golden frame — the unicode theme, colour off (the exact spec story).
/// Time is a first-class column now: each settled row carries its wall
/// time right-aligned after the note column, per-task spend follows on
/// the rows whose completion reported one; the skipped row stays bare.
#[test]
fn golden_success_frame_unicode() {
    let lines = frame(&fold(&demo::success()), &UNICODE, 0);
    let expected = [
        "  🦋 nika · veille-news · 5 tasks · ceiling ≤ $0.04",
        "     permits ✓ network:read(hn.algolia.com) · fs:write(./out)",
        "",
        "  ✔  fetch_top     http 200 · 1.2s · 34 KB         1.2s",
        "  ✔  extract_ai    jq · 0.1s · 12 items           130ms",
        "  ✔  summarize     claude-sonnet · 3.1s · $0.011   3.0s · $0.01",
        "  ✔  write_md      2.1 KB written                 290ms",
        "  ↷  notify_slack  when: env.CI != 'true'",
    ];
    assert_eq!(&lines[..8], &expected[..]);
    // The meter line: pinned prefix + rule-padded to a stable width.
    assert!(
        lines[8].starts_with("  ── 5/5 done · $0.01 of ≤$0.04 · elapsed 4.7s "),
        "meter: {}",
        lines[8]
    );
    assert_eq!(lines[8].chars().count(), 66, "2-indent + 64-rule");
}

/// Golden frame — the ASCII theme is first-class, not best-effort.
#[test]
fn golden_success_frame_ascii() {
    let lines = frame(&fold(&demo::success()), &ASCII, 0);
    assert_eq!(
        lines[0],
        "  [nika] nika · veille-news · 5 tasks · ceiling ≤ $0.04"
    );
    assert_eq!(
        lines[3],
        "  ok fetch_top     http 200 · 1.2s · 34 KB         1.2s"
    );
}

#[test]
fn the_per_row_failure_detail_reaches_the_operator() {
    // The user-sim finding: `row.detail` was documented « feeds the
    // failure card » but stamp_terminal never copied it — the journal
    // carried « NIKA-VAR-001 · supply it with --var … » while the
    // live render showed a mute ✖. The WHY must be IN the frame.
    let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("provider refused (429) · retried 2×")),
        "the TaskFailed `detail` field must render: {lines:?}"
    );
}

#[test]
fn golden_failure_card_carries_the_explain_hint() {
    let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
    let tail = &lines[lines.len() - 2..];
    assert!(tail[0].contains("NIKA-431"), "headline: {tail:?}");
    assert_eq!(tail[1], "    fix: nika explain NIKA-431");
    // The meter's honesty counter (#393): a failing run's summary line
    // never reads byte-identical to a clean one.
    let meter = lines.iter().find(|l| l.contains("done")).expect("meter");
    assert!(
        meter.contains(" failed · "),
        "meter counts the failure: {meter}"
    );
}

/// Presentation dedup (#393 · the #392 CI lesson kept the DATA intact):
/// a headline carrying the same spec code twice renders it once ·
/// foreign or single codes stay untouched.
#[test]
fn failure_card_dedups_a_double_spec_code() {
    assert_eq!(
        dedup_code_line(
            "NIKA-BUILTIN-WRITE-001 · tool `nika:write` reported an error: NIKA-BUILTIN-WRITE-001 · parent directory missing"
        ),
        "NIKA-BUILTIN-WRITE-001 · tool `nika:write` reported an error: parent directory missing"
    );
    assert_eq!(
        dedup_code_line("NIKA-431 · provider refused (429)"),
        "NIKA-431 · provider refused (429)"
    );
    assert_eq!(dedup_code_line("no code here"), "no code here");
}

/// The cascade rows RENDER as blocked `⊘` (the runtime's
/// upstream-failure cancellation · dim · never red) — and because
/// the demo failure has exactly ONE failed task, the note NAMES it:
/// `blocked · summarize failed` (unambiguous from the stream).
#[test]
fn golden_failure_frame_renders_cancelled_rows() {
    let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("  ⊘  write_md") && l.contains("blocked · summarize failed")),
        "unicode blocked row names the one failed upstream: {lines:?}"
    );
    let ascii = frame(&fold(&demo::failure()), &ASCII, 0);
    assert!(
        ascii
            .iter()
            .any(|l| l.starts_with("  x  write_md") && l.contains("blocked · summarize failed")),
        "ascii blocked row (err X ≠ blocked x): {ascii:?}"
    );
}

/// ADR-099 — a paused run's frame speaks the pause: the gate row
/// turns `◇` amber, the paused card names the awaiting task, and
/// neither a red glyph nor an explain hint appears (a pause is a
/// state, never a defect). The frame used to stay MUTE — a paused
/// run looked merely unfinished (first-run gate · 2026-07-31).
#[test]
fn golden_paused_frame_names_the_awaiting_gate() {
    let lines = frame(&fold(&demo::paused()), &UNICODE, 0);
    let joined = lines.join("\n");
    assert!(
        lines.iter().any(|l| l.starts_with("  ◇  summarize")),
        "the gate row wears the paused mark: {joined}"
    );
    assert!(
        joined.contains("paused · awaiting an answer for `summarize`"),
        "the paused card names the gate: {joined}"
    );
    assert!(
        joined.contains("--answer summarize=true"),
        "the paused card teaches the --answer boolean: {joined}"
    );
    assert!(
        !joined.contains("=yes"),
        "boolean true/false, not yes: {joined}"
    );
    assert!(!joined.contains('✖'), "never red: {joined}");
    assert!(
        !joined.contains("nika explain"),
        "a pause earns no fix hint: {joined}"
    );
    // The streamed plain close speaks the same card (one voice).
    let close = stream_summary(&fold(&demo::paused()), &UNICODE, &[]).join("\n");
    assert!(
        close.contains("paused · awaiting an answer for `summarize`"),
        "{close}"
    );
}

/// #393 sibling — fallout beside the root cause: cancelled rows earn
/// a `N blocked` meter segment. One failed gate cancelling its whole
/// downstream used to read `23/23 done · 1 failed` — the wall of `⊘`
/// had no summary voice (seo-live-review first-run · 2026-07-31).
#[test]
fn meter_counts_the_blocked_fallout() {
    let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
    let meter = lines.iter().find(|l| l.contains("done")).expect("meter");
    assert!(
        meter.contains("1 failed · 2 blocked · "),
        "root cause + fallout, side by side: {meter}"
    );
    // A clean run never grows the segment.
    let clean = frame(&fold(&demo::success()), &UNICODE, 0);
    let meter = clean.iter().find(|l| l.contains("done")).expect("meter");
    assert!(!meter.contains("blocked"), "{meter}");
}

/// #410 · the OBS-E warning reaches the CONSOLE: a green run whose
/// infer spent tokens and answered blank must say so above the meter
/// (the trace alone knowing was the observability gap) — on the
/// final frame AND the streamed plain close, unicode and ASCII.
#[test]
fn obs_e_warning_renders_above_the_meter() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let task = |n: &str| KeyValue::new("task", Value::String(n.to_owned()));

    let mut view = RunView::new();
    view.apply(&demo::bare_event(EventKind::TaskStarted, 0).with_field(task("summary")));
    view.apply(
        &demo::bare_event(EventKind::TaskCompleted, 5)
            .with_field(task("summary"))
            .with_field(KeyValue::new(
                "warning",
                Value::String(
                    "infer consumed 512 tokens yet the visible answer is empty".to_owned(),
                ),
            )),
    );

    let lines = frame(&view, &UNICODE, 0);
    let warn_at = lines
        .iter()
        .position(|l| l.contains("⚠ summary · infer consumed 512 tokens"))
        .expect("the warning line renders");
    let meter_at = lines
        .iter()
        .position(|l| l.contains("done"))
        .expect("meter present");
    assert!(warn_at < meter_at, "warning speaks before the meter");

    // The streamed plain close carries the same block (stream lane).
    let close = stream_summary(&view, &UNICODE, &[]);
    assert!(
        close[0].contains("⚠ summary"),
        "stream close leads with the warning: {close:?}"
    );

    // ASCII register swaps the glyph, keeps the fact.
    let ascii = frame(&view, &ASCII, 0);
    assert!(
        ascii.iter().any(|l| l.contains("! summary · infer")),
        "{ascii:?}"
    );

    // A warning-less run renders no ⚠ block at all.
    let mut clean = RunView::new();
    clean.apply(&demo::bare_event(EventKind::TaskCompleted, 5).with_field(task("ok_task")));
    assert!(
        !frame(&clean, &UNICODE, 0).iter().any(|l| l.contains('⚠')),
        "no false alarm on a clean run"
    );
}

/// The three never-ran classes render DISTINCTLY (the skip-reason
/// law): `↷ cache hit (resume)` · `↷ when: false` · `⊘ blocked ·
/// upstream failed` — the generic blocked form when SEVERAL tasks
/// failed (ancestry is ambiguous from the stream alone).
#[test]
fn skip_reasons_render_their_three_classes() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let task = |n: &str| KeyValue::new("task", Value::String(n.to_owned()));
    let note = |n: &str| KeyValue::new("note", Value::String(n.to_owned()));

    let mut view = RunView::new();
    view.apply(&demo::bare_event(EventKind::TaskCacheHit, 5).with_field(task("read")));
    view.apply(
        &demo::bare_event(EventKind::TaskSkipped, 10)
            .with_field(task("deploy"))
            .with_field(note("when: gate closed")),
    );
    // TWO failures → the blocked row cannot name ONE upstream.
    view.apply(&demo::bare_event(EventKind::TaskFailed, 15).with_field(task("a")));
    view.apply(&demo::bare_event(EventKind::TaskFailed, 16).with_field(task("b")));
    view.apply(
        &demo::bare_event(EventKind::TaskCancelled, 20)
            .with_field(task("notify"))
            .with_field(note("upstream failed")),
    );

    let lines = frame(&view, &UNICODE, 0);
    let find = |id: &str| {
        lines
            .iter()
            .find(|l| l.contains(id))
            .cloned()
            .expect("every staged row renders")
    };
    assert!(
        find("read").starts_with("  ↷ ") && find("read").contains("cache hit (resume)"),
        "cache hit: {}",
        find("read")
    );
    assert!(
        find("deploy").starts_with("  ↷ ") && find("deploy").contains("when: false"),
        "when gate: {}",
        find("deploy")
    );
    assert!(
        find("notify").starts_with("  ⊘ ") && find("notify").contains("blocked · upstream failed"),
        "ambiguous blocked stays generic: {}",
        find("notify")
    );

    // ASCII parity for the whole vocabulary: ~> skip · x blocked.
    let ascii = frame(&view, &ASCII, 0);
    assert!(
        ascii.iter().any(|l| l.starts_with("  ~> read")),
        "ascii skip glyph: {ascii:?}"
    );
    assert!(
        ascii.iter().any(|l| l.starts_with("  x  notify")),
        "ascii blocked glyph: {ascii:?}"
    );
    assert!(
        !ascii.iter().any(|l| l.contains('↷') || l.contains('⊘')),
        "no unicode leaks into --ascii: {ascii:?}"
    );
}

/// A GATE cancellation names the producer and what it settled as
/// (#1198).
///
/// The runtime has always emitted `blocked_by` on `task_cancelled`; the
/// fold dropped it, so the row could only repeat the runtime's own
/// words — `gate: an edge did not admit` — which name no edge, no
/// upstream and no outcome.
///
/// The outcome is what makes the sentence teach. A `tasks.X.error`
/// binding admits `{failure, skipped}`, so a producer that SUCCEEDED
/// closes the gate: the consumer is a dead path and the gate is right.
/// A reader told only « an edge did not admit », looking at a green
/// upstream one row above, has no way to reach that.
#[test]
fn a_gated_row_names_the_producer_and_its_outcome() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let field = |k: &'static str, v: &str| KeyValue::new(k, Value::String(v.to_owned()));

    let mut view = RunView::new();
    // `process` ran and SUCCEEDED (the for_each whose failed item was
    // recovered by `on_error: skip` settles here).
    view.apply(&demo::bare_event(EventKind::TaskCompleted, 5).with_field(field("task", "process")));
    view.apply(
        &demo::bare_event(EventKind::TaskCancelled, 10)
            .with_field(field("task", "reads_error"))
            .with_field(field("note", "gate: an edge did not admit"))
            .with_field(field("blocked_by", "process")),
    );

    let lines = frame(&view, &UNICODE, 0);
    let row = lines
        .iter()
        .find(|l| l.contains("reads_error"))
        .expect("the gated row renders");
    assert!(
        row.contains("process") && row.contains("settled ok"),
        "the gated row names the producer AND its outcome: {row}"
    );
    assert!(
        !row.contains("an edge did not admit"),
        "the un-teaching note is replaced, not appended to: {row}"
    );

    // A producer that FAILED reads its own outcome — so this cannot pass
    // by printing `ok` for everything.
    let mut view = RunView::new();
    view.apply(&demo::bare_event(EventKind::TaskFailed, 5).with_field(field("task", "fetch")));
    view.apply(
        &demo::bare_event(EventKind::TaskCancelled, 10)
            .with_field(field("task", "consume"))
            .with_field(field("note", "gate: an edge did not admit"))
            .with_field(field("blocked_by", "fetch")),
    );
    let lines = frame(&view, &UNICODE, 0);
    let row = lines
        .iter()
        .find(|l| l.contains("consume"))
        .expect("the gated row renders");
    assert!(
        row.contains("fetch") && row.contains("settled failed"),
        "a failed producer reads as failed: {row}"
    );

    // An engine that emitted no `blocked_by` still gets a sentence, never
    // a half-formatted one naming nobody.
    let mut view = RunView::new();
    view.apply(
        &demo::bare_event(EventKind::TaskCancelled, 10)
            .with_field(field("task", "orphan"))
            .with_field(field("note", "gate: an edge did not admit")),
    );
    let lines = frame(&view, &UNICODE, 0);
    let row = lines
        .iter()
        .find(|l| l.contains("orphan"))
        .expect("the gated row renders");
    assert!(
        row.contains("bindings admit"),
        "no producer named, still a sentence: {row}"
    );
}

/// A mid-retry run RENDERS the `↻` row (§3.1 — the attempt failed ·
/// the TASK has not · the row holds until a terminal frame).
#[test]
fn golden_retrying_frame_renders_the_yellow_arrow() {
    let lines = frame(&fold(&demo::retrying()), &UNICODE, 0);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("  ↻  summarize") && l.contains("rate limited")),
        "unicode retrying row: {lines:?}"
    );
    let ascii = frame(&fold(&demo::retrying()), &ASCII, 0);
    assert!(
        ascii
            .iter()
            .any(|l| l.starts_with("  r  summarize") && l.contains("rate limited")),
        "ascii retrying row: {ascii:?}"
    );
    // Still in flight: no terminal frame · no verdict line.
    let view = fold(&demo::retrying());
    assert_eq!(view.verdict, None, "a retrying run has no verdict yet");
}

/// `--quiet` compact card: the verdict line + cost, NO per-task rows.
#[test]
fn verdict_frame_is_compact_success() {
    let lines = verdict_frame(&fold(&demo::success()), &UNICODE);
    assert_eq!(lines.len(), 1, "success = one verdict line: {lines:?}");
    // Glyph carries its own trailing space (§3.1) + the line's space →
    // two, exactly the task-row convention (`✔  fetch_top`).
    assert!(
        lines[0].starts_with("  ✔  veille-news · 5 tasks · "),
        "verdict line: {}",
        lines[0]
    );
    assert!(lines[0].contains("$0.01 of ≤$0.04"), "cost: {}", lines[0]);
    // NOT the storyboard — no per-task row leaks into the quiet card.
    assert!(
        !lines.iter().any(|l| l.contains("fetch_top")),
        "quiet hides the per-task rows: {lines:?}"
    );
}

/// `--quiet` still surfaces errors (spec §3.5 "errors always") — the
/// failure card + explain hint, the SAME the full frame renders.
#[test]
fn verdict_frame_keeps_the_failure_card() {
    let lines = verdict_frame(&fold(&demo::failure()), &UNICODE);
    assert!(
        lines[0].starts_with("  ✖ "),
        "failed verdict glyph: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("NIKA-431")),
        "the failure reason surfaces: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == "    fix: nika explain NIKA-431"),
        "explain hint: {lines:?}"
    );
}

/// Called before a terminal frame (no verdict): header line only, no card.
#[test]
fn verdict_frame_no_verdict_is_header_only() {
    let lines = verdict_frame(&fold(&demo::retrying()), &UNICODE);
    assert_eq!(lines.len(), 1, "no verdict → one line: {lines:?}");
    assert!(lines[0].contains('○'), "pending glyph: {}", lines[0]);
}

/// The ASCII theme is first-class for the quiet card too.
#[test]
fn verdict_frame_ascii_theme() {
    let lines = verdict_frame(&fold(&demo::success()), &ASCII);
    assert!(lines[0].starts_with("  ok veille-news · "), "{}", lines[0]);
}

/// Persona 14 · gauntlet g2: `--quiet` on a recovered run printed `✔`
/// (exit 0, which is correct) so the first glance looked like success.
#[test]
fn verdict_frame_recovered_is_not_a_green_tick() {
    let view = fold(&demo::recovered());
    assert_eq!(view.verdict, Some(true));
    assert!(crate::fruit::recovered_ok(&view));
    let uni = verdict_frame(&view, &UNICODE);
    assert!(
        uni[0].starts_with("  ⚠  recovered ·"),
        "quiet headline names the repair: {}",
        uni[0]
    );
    assert!(
        !uni[0].contains('✔'),
        "a recovered success is not a green tick: {}",
        uni[0]
    );
    let ascii = verdict_frame(&view, &ASCII);
    assert!(
        ascii[0].starts_with("  !  recovered ·"),
        "ascii twin: {}",
        ascii[0]
    );
}

/// The verdict line carries elapsed time as SECONDS (`ms / 1000`): a
/// mutated divisor (`* 1000` → `4_700_000s` · `% 1000` → `700s`) renders a
/// wildly wrong duration, so the exact `4.7s` pins the conversion.
#[test]
fn verdict_frame_renders_elapsed_as_seconds() {
    let mut view = fold(&demo::success());
    view.elapsed_ms = 4700;
    let lines = verdict_frame(&view, &ASCII);
    assert!(lines[0].contains("4.7s"), "elapsed → seconds: {}", lines[0]);
}

/// The living map: accents + a plan + ≥2 tasks earn the wave-column
/// line under the header; the running node carries its verb's own
/// motion frame; sober frames never see it.
#[test]
fn living_map_rides_the_accents_frame() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let field = |k: &str, v: &str| KeyValue::new(k, Value::String(v.to_owned()));

    let mut view = RunView::new();
    view.apply(
        &demo::bare_event(EventKind::TaskStarted, 0)
            .with_field(field("task", "a"))
            .with_field(field("note", "infer · mock/echo")),
    );
    view.apply(&demo::bare_event(EventKind::TaskCompleted, 100).with_field(field("task", "a")));
    view.apply(
        &demo::bare_event(EventKind::TaskStarted, 100)
            .with_field(field("task", "b"))
            .with_field(field("note", "exec · sh")),
    );
    view.set_plan(vec![vec!["a".to_owned()], vec!["b".to_owned()]]);

    let mut accents = Theme::new(true, false, false);
    accents.accents = true;
    accents.animate = true;
    let f = frame(&view, &accents, 1);
    let map = f
        .iter()
        .find(|l| l.contains('⇉'))
        .expect("the map line exists");
    assert!(map.contains('a') && map.contains('b'), "{map}");
    assert!(
        map.contains(crate::theme::SCANLINE[1]),
        "the running exec node turns its own motion: {map}"
    );

    let sober = frame(&view, &Theme::new(false, false, false), 1);
    assert!(
        !sober.iter().any(|l| l.contains('⇉') || l.contains("=>")),
        "sober frames never carry the map"
    );
}

#[test]
fn frame_is_stable_under_ticks_when_nothing_runs() {
    let view = fold(&demo::success());
    assert_eq!(frame(&view, &UNICODE, 0), frame(&view, &UNICODE, 9));
}

/// The interactive accents bracket the duration column (nextest
/// school · `[  1.2s]`), right-aligned inside the SAME width the
/// sober form uses — and rows without a duration (the skipped row)
/// grow no brackets. The sober frame stays byte-identical to the
/// golden above by construction (accents default OFF).
#[test]
fn accents_bracket_the_duration_column_tty_only() {
    let mut accented = UNICODE;
    accented.accents = true;
    let lines = frame(&fold(&demo::success()), &accented, 0);
    assert!(
        lines[3].ends_with("[ 1.2s]"),
        "bracketed right-aligned cell: {}",
        lines[3]
    );
    assert!(
        lines[4].ends_with("[130ms]"),
        "the widest cell sets the width: {}",
        lines[4]
    );
    assert!(
        !lines[7].contains('['),
        "the skipped row grows no brackets: {}",
        lines[7]
    );
    // Cost still rides AFTER the bracketed cell on the row that has one.
    assert!(
        lines[5].contains("[ 3.0s] · $0.01"),
        "cost follows the cell: {}",
        lines[5]
    );
    // The demo runs sub-second — under the 30s SLOW floor, so even
    // the accented frame carries no accidental `slow` marker.
    assert!(
        !lines.iter().any(|l| l.contains("slow")),
        "fast runs stay marker-free: {lines:?}"
    );
}

/// The SLOW accent (design §1.4): a settled task past
/// `max(2 × median, 30s)` renders its duration YELLOW + the `slow`
/// word — interactive accents only, and the threshold floor keeps
/// mid-scale tasks quiet.
#[test]
fn slow_tasks_self_identify_under_accents() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let task = |n: &str| KeyValue::new("task", Value::String(n.to_owned()));
    let dur = |ms: i64| KeyValue::new("duration_ms", Value::Int(ms));

    let mut view = RunView::new();
    for (name, ms, at) in [
        ("a", 1_000, 1_000u64),
        ("b", 1_200, 2_000),
        ("c", 100_000, 5_000),
    ] {
        view.apply(&demo::bare_event(EventKind::TaskStarted, 0).with_field(task(name)));
        view.apply(
            &demo::bare_event(EventKind::TaskCompleted, at)
                .with_field(task(name))
                .with_field(dur(ms)),
        );
    }

    // Sober register: never the marker, whatever the durations.
    let sober = frame(&view, &UNICODE, 0);
    assert!(!sober.iter().any(|l| l.contains("slow")), "{sober:?}");

    // Accents + colour: the 100s task (median 1.2s → floor 30s)
    // carries the yellow cell + the word; its siblings stay dim.
    let mut accented = Theme::new(true, false, false);
    accented.accents = true;
    let lines = frame(&view, &accented, 0);
    // Row lookup by each row's UNIQUE duration cell (the accents
    // chip is painted — raw-line token probes proved brittle).
    let c_row = lines
        .iter()
        .find(|l| l.contains("1m40s"))
        .expect("c row (the 100s task)");
    assert!(
        c_row.contains("\x1b[33m") && c_row.contains("slow"),
        "the slow task self-identifies in yellow: {c_row:?}"
    );
    let a_row = lines
        .iter()
        .find(|l| l.contains("[ 1.0s]"))
        .expect("a row (the 1.0s task)");
    assert!(
        !a_row.contains("slow") && !a_row.contains("\x1b[33m"),
        "median-scale siblings stay quiet: {a_row:?}"
    );

    // The floor: 25s over a 20s median is NOT slow (2×median = 40s
    // wins the max) — no marker.
    let mut mid = RunView::new();
    for (name, ms, at) in [
        ("x", 10_000, 1_000u64),
        ("y", 20_000, 2_000),
        ("z", 25_000, 3_000),
    ] {
        mid.apply(&demo::bare_event(EventKind::TaskStarted, 0).with_field(task(name)));
        mid.apply(
            &demo::bare_event(EventKind::TaskCompleted, at)
                .with_field(task(name))
                .with_field(dur(ms)),
        );
    }
    let quiet = frame(&mid, &accented, 0);
    assert!(
        !quiet.iter().any(|l| l.contains("slow")),
        "2x-median dominates the floor: {quiet:?}"
    );
}

/// A view with output-carrying completions: `frame_with_outputs`
/// appends the bounded shape tail (+ tokens) on those rows while
/// `frame` stays BYTE-IDENTICAL to today — the piped/CI register
/// never grows tails.
#[test]
fn frame_with_outputs_adds_tails_and_frame_stays_bare() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};

    let mut view = RunView::new();
    let task = |n: &str| KeyValue::new("task", Value::String(n.to_owned()));
    view.apply(&demo::bare_event(EventKind::TaskStarted, 0).with_field(task("audit")));
    view.apply(
        &demo::bare_event(EventKind::TaskCompleted, 100)
            .with_field(task("audit"))
            .with_field(KeyValue::new(
                "output",
                Value::String(r#"{"verdict":"P0","fixes":[1,2]}"#.to_owned()),
            ))
            .with_field(KeyValue::new("tokens", Value::Int(90))),
    );
    let with = frame_with_outputs(&view, &UNICODE, 0);
    let audit = with.iter().find(|l| l.contains("audit")).expect("row");
    assert!(
        audit.contains("→ {fixes[2], verdict} · 30B · 90 tok"),
        "tail rides the completed row: {audit}"
    );
    let bare = frame(&view, &UNICODE, 0);
    assert!(
        !bare.iter().any(|l| l.contains('→')),
        "the bare frame never grows tails: {bare:?}"
    );

    // ASCII parity — the arrow degrades, nothing unicode leaks.
    let ascii = frame_with_outputs(&view, &ASCII, 0);
    assert!(
        ascii.iter().any(|l| l.contains("-> {fixes[2], verdict}")),
        "{ascii:?}"
    );

    // The demo storyboard carries no output fields — with-outputs
    // renders byte-identically to the bare frame (no invented data).
    let demo_view = fold(&demo::success());
    assert_eq!(
        frame_with_outputs(&demo_view, &UNICODE, 0),
        frame(&demo_view, &UNICODE, 0),
        "no output fields → no tails, ever"
    );
}

/// The running row shows a LIVE elapsed (now − started) and `∥` marks
/// the wave-siblings that actually overlapped — with `||` parity under
/// the ASCII theme (never a unicode leak).
#[test]
fn lanes_and_live_elapsed_render_in_both_themes() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};

    let mut view = RunView::new();
    let task = |name: &str| KeyValue::new("task", Value::String(name.to_owned()));
    view.apply(&demo::bare_event(EventKind::TaskStarted, 100).with_field(task("a")));
    view.apply(&demo::bare_event(EventKind::TaskStarted, 150).with_field(task("b")));
    // a settles at 1000 with a REAL measured duration (900ms) → its
    // reconstructed interval [100, 1000] overlaps b's [150, now].
    view.apply(
        &demo::bare_event(EventKind::TaskCompleted, 1000)
            .with_field(task("a"))
            .with_field(KeyValue::new("duration_ms", Value::Int(900))),
    );

    let lines = frame(&view, &UNICODE, 0);
    let a = lines.iter().find(|l| l.contains(" a ")).expect("a row");
    assert!(
        a.contains("900ms") && a.ends_with('∥'),
        "settled sibling: duration + lane marker: {a}"
    );
    let b = lines.iter().find(|l| l.contains(" b ")).expect("b row");
    assert!(
        b.contains("850ms") && b.ends_with('∥'),
        "running sibling: LIVE elapsed (1000−150) + marker: {b}"
    );

    let ascii = frame(&view, &ASCII, 0);
    assert!(
        ascii.iter().any(|l| l.ends_with("900ms ||")),
        "ascii parity ∥→||: {ascii:?}"
    );
    assert!(
        !ascii.iter().any(|l| l.contains('∥')),
        "no unicode leaks into --ascii: {ascii:?}"
    );
}

/// The sparkline rides the RUNNING row exactly when samples exist —
/// both injection guards are semantic, not cosmetic.
#[test]
fn running_row_carries_sparkline_only_with_samples() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};

    // A running task with NO samples: no spark glyph anywhere.
    let mut without = RunView::new();
    without.apply(
        &demo::bare_event(EventKind::TaskStarted, 10)
            .with_field(KeyValue::new("task", Value::String("summarize".into())))
            .with_field(KeyValue::new("note", Value::String("infer".into()))),
    );
    let lines = frame(&without, &UNICODE, 0);
    assert!(
        !lines.iter().any(|l| l.contains('▇')),
        "no samples → no spark: {lines:?}"
    );

    // A completed task reported tokens: the spark appears on the
    // RUNNING line (single sample 710 → top bar).
    let mut with = RunView::new();
    with.apply(
        &demo::bare_event(EventKind::TaskCompleted, 5)
            .with_field(KeyValue::new("task", Value::String("fetch".into())))
            .with_field(KeyValue::new("tokens", Value::Int(710))),
    );
    with.apply(
        &demo::bare_event(EventKind::TaskStarted, 10)
            .with_field(KeyValue::new("task", Value::String("summarize".into())))
            .with_field(KeyValue::new("note", Value::String("infer".into()))),
    );
    let lines = frame(&with, &UNICODE, 0);
    let running = lines
        .iter()
        .find(|l| l.contains("summarize"))
        .expect("running row renders");
    assert!(
        running.contains('▇'),
        "tokens reported → spark on the running row: {running}"
    );
}

/// #319 — a repaired success SAYS so: the settled row gains
/// ` · recovered` and the meter line counts the repairs — while a
/// clean run's frame stays byte-identical (the golden tests above
/// pin that: the demo storyboard carries no `task_recovered`).
#[test]
fn recovered_rows_and_meter_carry_the_repair_fact() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let task = |n: &str| KeyValue::new("task", Value::String(n.to_owned()));

    let mut view = RunView::new();
    view.apply(
        &demo::bare_event(EventKind::TaskStarted, 0)
            .with_field(task("fragile"))
            .with_field(KeyValue::new(
                "note",
                Value::String("invoke · nika:read".to_owned()),
            )),
    );
    view.apply(
        &demo::bare_event(EventKind::TaskRecovered, 1)
            .with_field(task("fragile"))
            .with_field(KeyValue::new(
                "code",
                Value::String("NIKA-BUILTIN-READ-001".to_owned()),
            )),
    );
    view.apply(
        &demo::bare_event(EventKind::TaskCompleted, 2)
            .with_field(task("fragile"))
            .with_field(KeyValue::new("duration_ms", Value::Int(1))),
    );
    view.apply(&demo::bare_event(EventKind::WorkflowCompleted, 3));

    let lines = frame(&view, &UNICODE, 0);
    let row = lines.iter().find(|l| l.contains("fragile")).expect("row");
    assert!(
        row.contains("1ms · recovered"),
        "the settled line says recovered: {row}"
    );
    let meter = lines.iter().find(|l| l.contains("done")).expect("meter");
    assert!(
        meter.contains("1/1 done · 1 recovered · "),
        "the summary line counts the repair: {meter}"
    );

    // The SUCCESS path only — no failure card grew out of the repair.
    assert!(
        !lines.iter().any(|l| l.contains("fix:")),
        "a recovered success is a success: {lines:?}"
    );

    // Colour ON: the fact paints yellow (the retry family).
    let coloured = frame(&view, &Theme::new(true, false, false), 0);
    let painted = coloured
        .iter()
        .find(|l| l.contains("fragile"))
        .expect("row");
    assert!(
        painted.contains("\x1b[33m · recovered\x1b[0m"),
        "recovered paints Warn: {painted:?}"
    );
}

/// The failure card targets FAILED rows only — an Ok row that happens
/// to carry a `detail` field renders no card (the `&&` is semantic).
#[test]
fn failure_card_ignores_ok_rows_with_detail() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};

    let mut view = RunView::new();
    view.apply(
        &demo::bare_event(EventKind::TaskCompleted, 5)
            .with_field(KeyValue::new("task", Value::String("ok_task".into())))
            .with_field(KeyValue::new(
                "detail",
                Value::String("NIKA-999 retried twice, recovered".into()),
            )),
    );
    view.apply(
        &demo::bare_event(EventKind::TaskFailed, 10)
            .with_field(KeyValue::new("task", Value::String("bad_task".into())))
            .with_field(KeyValue::new(
                "detail",
                Value::String("NIKA-440 · boom".into()),
            )),
    );
    view.apply(&demo::bare_event(EventKind::WorkflowFailed, 20));

    let lines = frame(&view, &UNICODE, 0);
    let card_lines: Vec<&String> = lines.iter().filter(|l| l.contains("NIKA-")).collect();
    assert_eq!(
        card_lines.len(),
        2,
        "headline + explain hint for the ONE failed row only: {lines:?}"
    );
    assert!(card_lines[0].contains("NIKA-440"));
}

/// The interactive extras (verb chip column · HUD bar) exist ONLY
/// on the accents surface — sober frames keep their exact bytes,
/// and the chip speaks the started note's own verb vocabulary.
#[test]
fn accents_surface_gains_verb_chips_and_the_hud_bar() {
    let mut view = RunView::new();
    for ev in demo::success() {
        view.apply(&ev);
    }
    let sober = Theme::new(false, false, false);
    let plain_frame = frame(&view, &sober, 0).join("\n");
    assert!(!plain_frame.contains('\u{25c6}'), "no chips off-accents");
    assert!(!plain_frame.contains('\u{2578}'), "no bar off-accents");

    let mut live = Theme::new(false, false, false);
    live.accents = true;
    let lines = frame(&view, &live, 0);
    let text = lines.join("\n");
    // demo::success starts `fetch` with `invoke · nika:fetch` → ◆.
    assert!(
        text.contains("\u{25c6} fetch"),
        "the invoke chip rides its row: {text}"
    );
    // The HUD bar closes the frame (all settled → full bar, count).
    let total = view.rows().len();
    assert!(
        lines
            .last()
            .is_some_and(|l| l.contains(&format!("{total}/{total}"))),
        "the bar line carries the count: {lines:?}"
    );
}

// ═══ A-2 · the lying-green law (user gauntlet 2026-07-31) ═══════════

/// One lying-green view per class the fruit module derives — a green
/// verdict whose FORM contradicts it (Priya's ask-back · Carmen's
/// all-fallback inputs · the empty answer).
fn lying_green_views() -> Vec<(&'static str, RunView)> {
    use nika_event::EventKind as K;
    use nika_types::resource::{KeyValue, Value};
    let ev = |kind, ms, fields: &[(&str, &str)]| {
        let mut e = demo::bare_event(kind, ms);
        for (k, v) in fields {
            e = e.with_field(KeyValue::new(*k, Value::String((*v).to_owned())));
        }
        e
    };

    let mut ask_back = RunView::new();
    ask_back.apply(&ev(
        K::TaskStarted,
        0,
        &[("task", "summarize"), ("note", "infer · openai/gpt-5.2")],
    ));
    ask_back.apply(&ev(
        K::TaskCompleted,
        1,
        &[
            ("task", "summarize"),
            (
                "output",
                "\"I don't see any transcripts provided. Could you please share them?\"",
            ),
        ],
    ));
    ask_back.apply(&ev(K::WorkflowCompleted, 2, &[]));

    let mut all_recovered = RunView::new();
    for t in ["f1", "f2", "f3"] {
        all_recovered.apply(&ev(
            K::TaskStarted,
            0,
            &[("task", t), ("note", "invoke · nika:fetch")],
        ));
        all_recovered.apply(&ev(K::TaskRecovered, 1, &[("task", t)]));
        all_recovered.apply(&ev(K::TaskCompleted, 2, &[("task", t)]));
    }
    all_recovered.apply(&ev(
        K::TaskStarted,
        3,
        &[("task", "score"), ("note", "infer · openai/gpt-5.2")],
    ));
    all_recovered.apply(&ev(
        K::TaskCompleted,
        4,
        &[("task", "score"), ("output", "\"7/10 seo score\"")],
    ));
    all_recovered.apply(&ev(K::WorkflowCompleted, 5, &[]));

    let mut empty = RunView::new();
    empty.apply(&ev(
        K::TaskStarted,
        0,
        &[("task", "think"), ("note", "infer · ollama/qwen3.5:4b")],
    ));
    empty.apply(&ev(
        K::TaskCompleted,
        1,
        &[("task", "think"), ("output", "\"\"")],
    ));
    empty.apply(&ev(K::WorkflowCompleted, 2, &[]));

    vec![
        ("ask-back", ask_back),
        ("all-recovered", all_recovered),
        ("empty-answer", empty),
    ]
}

/// THE LAW (A-2): a green closing surface never stays silent on a
/// derived caution. EVERY caution `fruit::cautions` derives — including
/// a class added AFTER this test was written — must land on EVERY human
/// closing surface: the full frame · the plain streamed close · the
/// compact `--quiet` card · the shareable verdict card. The assertion
/// iterates the DERIVED set, never a hard-coded one (the paste-safety
/// law's future-mode arm, applied to truth lines).
#[test]
fn every_lying_green_caution_reaches_every_closing_surface() {
    for (class, view) in lying_green_views() {
        let cautions = crate::fruit::cautions(&view, false);
        assert!(!cautions.is_empty(), "{class}: the class must derive");
        let surfaces = [
            ("frame", frame(&view, &UNICODE, 0).join("\n")),
            (
                "stream_summary",
                stream_summary(&view, &UNICODE, &[]).join("\n"),
            ),
            ("verdict_frame", verdict_frame(&view, &UNICODE).join("\n")),
            (
                "verdict_card",
                crate::flow::verdict_card(&view, &UNICODE, &[]).join("\n"),
            ),
        ];
        for caution in &cautions {
            // The card fits rows to its inner width — the law holds on a
            // distinctive head, ellipsis or not.
            let head: String = caution.chars().take(40).collect();
            for (name, text) in &surfaces {
                assert!(
                    text.contains(head.as_str()),
                    "{class}: the {name} surface dropped `{caution}`:\n{text}"
                );
            }
        }
    }
}

/// The FRUIT block pass-through + the rehearsal fact: the caller's
/// composed notes (`wrote …` — sizes are the caller's stat) land on the
/// note-carrying closes, and an all-mock run announces the rehearsal on
/// both — the audit's beat ⑤ miss ("the run wrote ./output.md — and
/// never said so") pinned as law.
#[test]
fn fruit_notes_and_rehearsal_land_on_the_closing_surfaces() {
    use nika_event::EventKind as K;
    use nika_types::resource::{KeyValue, Value};
    let ev = |kind, ms, fields: &[(&str, &str)]| {
        let mut e = demo::bare_event(kind, ms);
        for (k, v) in fields {
            e = e.with_field(KeyValue::new(*k, Value::String((*v).to_owned())));
        }
        e
    };
    let mut view = RunView::new();
    view.apply(&ev(
        K::TaskStarted,
        0,
        &[("task", "think"), ("note", "infer · mock/echo")],
    ));
    view.apply(&ev(
        K::TaskCompleted,
        1,
        &[("task", "think"), ("output", "\"mock(echo) · hi\"")],
    ));
    view.apply(&ev(
        K::TaskStarted,
        2,
        &[("task", "persist"), ("note", "invoke · nika:write")],
    ));
    view.apply(&ev(
        K::TaskCompleted,
        3,
        &[("task", "persist"), ("output", "\"output.md\"")],
    ));
    view.apply(&ev(K::WorkflowCompleted, 4, &[]));

    let notes = vec!["wrote output.md (74B)".to_owned()];
    let close = stream_summary(&view, &UNICODE, &notes).join("\n");
    assert!(close.contains("wrote output.md (74B)"), "{close}");
    assert!(
        close.contains("rehearsal · a mock model echoed the prompt"),
        "{close}"
    );
    let card = crate::flow::verdict_card(&view, &UNICODE, &notes).join("\n");
    assert!(card.contains("wrote output.md (74B)"), "{card}");
    assert!(card.contains("rehearsal · a mock model echoed"), "{card}");

    // A real-model run announces NO rehearsal (the fact never spams).
    let mut real = RunView::new();
    real.apply(&ev(
        K::TaskStarted,
        0,
        &[("task", "think"), ("note", "infer · openai/gpt-5.2")],
    ));
    real.apply(&ev(
        K::TaskCompleted,
        1,
        &[("task", "think"), ("output", "\"a real answer\"")],
    ));
    real.apply(&ev(K::WorkflowCompleted, 2, &[]));
    assert!(
        !stream_summary(&real, &UNICODE, &[])
            .join("\n")
            .contains("rehearsal"),
    );
}

/// C08 · issue 1302: the mock OG storyboard / closing surfaces must
/// name the rehearsal. `nika OK · $0.00` alone is the lying card.
#[test]
fn mock_image_storyboard_names_the_rehearsal() {
    use nika_event::EventKind as K;
    use nika_types::resource::{KeyValue, Value};
    let ev = |kind, ms, fields: &[(&str, &str)]| {
        let mut e = demo::bare_event(kind, ms);
        for (k, v) in fields {
            e = e.with_field(KeyValue::new(*k, Value::String((*v).to_owned())));
        }
        e
    };
    let mock_out = r#"{"provider":"mock","warnings":[]}"#;
    let mut view = RunView::new();
    view.apply(&ev(K::WorkflowStarted, 0, &[("workflow", "og-images")]));
    view.apply(&ev(
        K::TaskStarted,
        1,
        &[("task", "hero"), ("note", "invoke · nika:image_generate")],
    ));
    view.apply(&ev(
        K::TaskCompleted,
        2,
        &[("task", "hero"), ("output", mock_out)],
    ));
    view.apply(&ev(K::WorkflowCompleted, 3, &[]));

    let close = stream_summary(&view, &ASCII, &[]).join("\n");
    assert!(
        close.contains("rehearsal") || close.contains("not a real image"),
        "streamed close hid the mock: {close}"
    );
    let card = crate::flow::verdict_card(&view, &ASCII, &[]).join("\n");
    assert!(
        card.contains("rehearsal") || card.contains("not a real image"),
        "verdict card hid the mock: {card}"
    );
    assert!(
        card.contains("og-images"),
        "the card must still name the workflow: {card}"
    );
}
