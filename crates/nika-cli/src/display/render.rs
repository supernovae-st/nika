// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Frame rendering (spec §3.3) — a pure function `(RunView, Theme, tick) →
//! lines`. No I/O here: the replay loop owns the terminal, this module owns
//! the truth-to-text mapping. Snapshot tests pin BOTH glyph themes.

use crate::display::state::{RunView, TaskState};
use crate::display::theme::{Role, Theme};

/// Render one frame of the run card.
#[must_use]
pub fn frame(view: &RunView, theme: &Theme, tick: usize) -> Vec<String> {
    let mut lines = Vec::with_capacity(view.rows().len() + 6);

    // Header: identity + the statically-proven ceiling.
    let ceiling = view
        .ceiling_usd
        .map(|c| format!(" · ceiling ≤ ${c:.2}"))
        .unwrap_or_default();
    lines.push(format!(
        "  {} nika · {} · {} tasks{ceiling}",
        theme.logo(),
        theme.paint(Role::Strong, &view.workflow),
        view.rows().len(),
    ));

    // The audit-as-greeting line (the trust moment, every run).
    if let Some(permits) = &view.permits {
        let mark = if theme.ascii { "OK" } else { "✓" };
        lines.push(format!(
            "     permits {} {}",
            theme.paint(Role::Good, mark),
            theme.paint(Role::Dim, permits),
        ));
    }
    lines.push(String::new());

    // Task rows — stable order, aligned ids, notes dimmed.
    let width = view.rows().iter().map(|r| r.id.len()).max().unwrap_or(8);
    for row in view.rows() {
        let mut note = row.note.clone();
        if row.state == TaskState::Running && !view.token_samples.is_empty() {
            let spark = theme.sparkline(&view.token_samples);
            if !spark.is_empty() {
                note = format!("{note} {spark}");
            }
        }
        lines.push(format!(
            "  {} {:<width$}  {}",
            theme.glyph(row.state, tick),
            row.id,
            theme.paint(Role::Dim, &note),
        ));
    }

    // Footer meter: progress · live cost vs ceiling · wall clock.
    let cost = match view.ceiling_usd {
        Some(c) => format!("${:.3} of ≤${c:.2}", view.cost_usd),
        None => format!("${:.3}", view.cost_usd),
    };
    #[allow(clippy::cast_precision_loss)] // display-only seconds
    let secs = view.elapsed_ms as f64 / 1000.0;
    let meter = format!(
        "── {}/{} done · {cost} · elapsed {secs:.1}s ",
        view.done_count(),
        view.rows().len(),
    );
    lines.push(format!(
        "  {}",
        theme.paint(Role::Dim, &pad_rule(&meter, 64))
    ));

    // Failure card (only on a failed verdict · derives the explain hint) —
    // the SAME card the compact `--quiet` surface renders (shared helper).
    if view.verdict == Some(false) {
        append_failure_card(&mut lines, view, theme);
    }
    lines
}

/// Render the COMPACT final card (spec §3.5 `--quiet` · "final card only ·
/// errors always") — the one-line verdict + cost, plus the failure card when
/// the run failed. NO per-task storyboard. A run with no verdict yet (called
/// before the terminal frame) renders the header alone.
#[must_use]
pub fn verdict_frame(view: &RunView, theme: &Theme) -> Vec<String> {
    let mut lines = Vec::with_capacity(4);
    let glyph = match view.verdict {
        Some(true) => theme.glyph(TaskState::Ok, 0),
        Some(false) => theme.glyph(TaskState::Failed, 0),
        None => theme.glyph(TaskState::Pending, 0),
    };
    let cost = match view.ceiling_usd {
        Some(c) => format!("${:.3} of ≤${c:.2}", view.cost_usd),
        None => format!("${:.3}", view.cost_usd),
    };
    #[allow(clippy::cast_precision_loss)] // display-only seconds
    let secs = view.elapsed_ms as f64 / 1000.0;
    lines.push(format!(
        "  {} {} · {} tasks · {secs:.1}s · {cost}",
        glyph,
        theme.paint(Role::Strong, &view.workflow),
        view.rows().len(),
    ));

    // Errors always (spec §3.5) — the same failure card the full frame emits,
    // appended so a quiet run still surfaces WHY it failed + the explain hint.
    if view.verdict == Some(false) {
        append_failure_card(&mut lines, view, theme);
    }
    lines
}

/// The failure card (workflow-level detail + per-failed-row detail + the
/// `nika explain` hint). Shared by the full [`frame`] and the compact
/// [`verdict_frame`] so the two surfaces can never drift on a failure.
// `&Theme` (not by-value) to match the `frame`/`verdict_frame` borrow that
// threads it here — one calling convention across the render surface.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn append_failure_card(lines: &mut Vec<String>, view: &RunView, theme: &Theme) {
    if let Some(detail) = &view.workflow_detail {
        lines.push(String::new());
        lines.push(format!(
            "  {}{}",
            theme.glyph(TaskState::Failed, 0),
            theme.paint(Role::Strong, detail),
        ));
        if let Some(code) = detail.split_whitespace().find(|w| w.starts_with("NIKA-")) {
            lines.push(theme.paint(Role::Dim, &format!("    fix: nika explain {code}")));
        }
    }
    for row in view.rows() {
        if row.state == TaskState::Failed && !row.detail.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "  {}{}",
                theme.glyph(TaskState::Failed, 0),
                theme.paint(Role::Strong, &row.detail),
            ));
            if let Some(code) = row
                .detail
                .split_whitespace()
                .find(|w| w.starts_with("NIKA-"))
            {
                lines.push(theme.paint(Role::Dim, &format!("    fix: nika explain {code}")));
            }
        }
    }
}

/// Extend a meter line with rule dashes to a stable width.
fn pad_rule(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.to_owned();
    }
    let mut out = String::with_capacity(width * 3);
    out.push_str(text);
    out.extend(std::iter::repeat_n('─', width - len));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    fn fold(events: &[nika_event::Event]) -> RunView {
        let mut view = RunView::new();
        for ev in events {
            view.apply(ev);
        }
        view
    }

    const UNICODE: Theme = Theme {
        color: false,
        ascii: false,
        animate: false,
    };
    const ASCII: Theme = Theme {
        color: false,
        ascii: true,
        animate: false,
    };

    /// Golden frame — the unicode theme, colour off (the exact spec story).
    #[test]
    fn golden_success_frame_unicode() {
        let lines = frame(&fold(&demo::success()), &UNICODE, 0);
        let expected = [
            "  🦋 nika · veille-news · 5 tasks · ceiling ≤ $0.04",
            "     permits ✓ network:read(hn.algolia.com) · fs:write(./out)",
            "",
            "  ✔  fetch_top     http 200 · 1.2s · 34 KB",
            "  ✔  extract_ai    jq · 0.1s · 12 items",
            "  ✔  summarize     claude-sonnet · 3.1s · $0.011",
            "  ✔  write_md      2.1 KB written",
            "  ⊘  notify_slack  when: env.CI != 'true'",
        ];
        assert_eq!(&lines[..8], &expected[..]);
        // The meter line: pinned prefix + rule-padded to a stable width.
        assert!(
            lines[8].starts_with("  ── 5/5 done · $0.011 of ≤$0.04 · elapsed 4.7s "),
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
        assert_eq!(lines[3], "  ok fetch_top     http 200 · 1.2s · 34 KB");
    }

    #[test]
    fn golden_failure_card_carries_the_explain_hint() {
        let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
        let tail = &lines[lines.len() - 2..];
        assert!(tail[0].contains("NIKA-431"), "headline: {tail:?}");
        assert_eq!(tail[1], "    fix: nika explain NIKA-431");
    }

    /// The cascade rows RENDER as §3.1 `◼` (the runtime's
    /// upstream-failure cancellation · dim · never red) — the fold and
    /// the glyph were each pinned alone; this pins the assembled line.
    #[test]
    fn golden_failure_frame_renders_cancelled_rows() {
        let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("  ◼  write_md") && l.contains("upstream failed")),
            "unicode cancelled row: {lines:?}"
        );
        let ascii = frame(&fold(&demo::failure()), &ASCII, 0);
        assert!(
            ascii
                .iter()
                .any(|l| l.starts_with("  x  write_md") && l.contains("upstream failed")),
            "ascii cancelled row (err X ≠ cancelled x): {ascii:?}"
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
        assert!(lines[0].contains("$0.011 of ≤$0.04"), "cost: {}", lines[0]);
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

    #[test]
    fn frame_is_stable_under_ticks_when_nothing_runs() {
        let view = fold(&demo::success());
        assert_eq!(frame(&view, &UNICODE, 0), frame(&view, &UNICODE, 9));
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
}
